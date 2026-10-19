mod builder;
mod cli;
mod collector;
mod config;
mod packager;
mod parser;
mod project;
mod sdk;

use anyhow::{Context, Result};
use clap::Parser;
use colored::*;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::PathBuf;

use builder::{auto_build, BuildOptions, TargetPlatform};
use cli::Cli;
use collector::assets::AssetCollector;
use collector::dll::LibraryResolver;
use config::Config;
use packager::single_file::StubOptions;
use packager::StagingBuilder;
use parser::{BinaryFormat, parse_dependencies};
use project::ProjectConfig;

fn main() -> Result<()> {
    let args = Cli::parse();
    let config = Config::new();
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    if args.init {
        let path = current_dir.join(project::CONFIG_FILE);
        if path.exists() {
            anyhow::bail!("{} already exists", project::CONFIG_FILE);
        }
        fs::write(&path, project::TEMPLATE)?;
        println!("{} Wrote {}", "✔".green(), path.display().to_string().bright_white());
        return Ok(());
    }

    // Command-line flags win over ship.toml, which wins over the defaults.
    let project = ProjectConfig::load(&current_dir)?;
    let target_name = args.target.clone().or_else(|| project.target.clone()).unwrap_or_else(|| "windows".into());
    let output_dir = args.output.clone().or_else(|| project.output.clone()).unwrap_or_else(|| PathBuf::from("dist"));
    let custom_name = args.name.clone().or_else(|| project.name.clone());
    let console = args.console || project.console;
    let search_paths: Vec<PathBuf> = args.search_paths.iter().chain(&project.search_paths).cloned().collect();
    let extra_assets: Vec<PathBuf> = args.extra_assets.iter().chain(&project.assets).cloned().collect();
    let build_options = BuildOptions {
        console,
        link: project.link.clone(),
        cflags: project.cflags.clone(),
    };

    let target_platform = match target_name.to_lowercase().as_str() {
        "windows" | "win" => TargetPlatform::Windows,
        "linux" | "lin" => TargetPlatform::Linux,
        other => anyhow::bail!("Unsupported target: {}. Use 'windows' or 'linux'", other),
    };
    // Check signing and upload prerequisites now rather than after a full build.
    let sign_config = match (&project.sign, target_platform) {
        (Some(sign), TargetPlatform::Windows) if !args.dry_run => {
            packager::sign::preflight(sign)?;
            Some(sign)
        }
        _ => None,
    };
    let push_target = match &args.push {
        Some(cli_target) => {
            let target = packager::itch::resolve_target(cli_target.as_deref(), project.itch.as_ref(), target_platform)?;
            if !args.dry_run {
                packager::itch::preflight()?;
            }
            Some(target)
        }
        None => None,
    };

    if args.appimage && target_platform != TargetPlatform::Linux {
        anyhow::bail!("--appimage needs --target linux (or target = \"linux\" in {})", project::CONFIG_FILE);
    }

    let (binary_path, app_name) = match args.binary {
        Some(path) => {
            let canonical = path.canonicalize()
                .with_context(|| format!("Binary not found: {:?}", path))?;
            let stem = canonical.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("app")
                .to_string();
            (canonical, stem)
        }
        None => {
            println!("{} Starting auto-build for {:?}", "📦".bright_yellow(), target_platform);
            let result = auto_build(&current_dir, target_platform, &build_options)?;
            println!("{} Build succeeded: {}", "✔".green(), result.binary_path.display().to_string().bright_white());
            (result.binary_path, result.project_name)
        }
    };

    println!("{}", format!("Packaging {}...", app_name).bold().bright_magenta());

    let is_msvc = if let Ok((_, deps)) = parse_dependencies(&binary_path) {
        deps.iter().any(|d| {
            let dl = d.to_ascii_lowercase();
            dl.starts_with("msvcp") || dl.starts_with("vcruntime")
        })
    } else {
        false
    };

    if is_msvc {
        println!("  {} Detected MSVC toolchain binary", "➜".cyan());
    }

    let resolver = LibraryResolver::new(&binary_path, &search_paths, is_msvc);

    let mut resolved_libraries: HashMap<String, Option<PathBuf>> = HashMap::new();
    let mut to_scan: Vec<PathBuf> = vec![binary_path.clone()];
    let mut scanned: HashSet<PathBuf> = HashSet::new();
    let mut os_libraries: HashSet<String> = HashSet::new();

    while let Some(current_file) = to_scan.pop() {
        if scanned.contains(&current_file) {
            continue;
        }
        scanned.insert(current_file.clone());

        if let Ok((format, deps)) = parse_dependencies(&current_file) {
            for dep in deps {
                let is_system = match format {
                    BinaryFormat::Pe => config.is_system_dll(&dep),
                    BinaryFormat::Elf => config.is_system_so(&dep),
                };

                if !is_system && !resolved_libraries.contains_key(&dep) && !os_libraries.contains(&dep) {
                    let location = resolver.find_library(&dep);
                    if matches!(format, BinaryFormat::Pe)
                        && location.as_deref().map_or(false, |l| config.is_windows_system_location(l))
                    {
                        os_libraries.insert(dep);
                        continue;
                    }
                    if let Some(ref loc) = location {
                        if loc.is_file() {
                            to_scan.push(loc.clone());
                        }
                    }
                    resolved_libraries.insert(dep, location);
                }
            }
        }
    }

    let mut libraries_list: Vec<(String, Option<PathBuf>)> = resolved_libraries.into_iter().collect();
    libraries_list.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));
    let missing: Vec<&str> = libraries_list.iter().filter(|(_, p)| p.is_none()).map(|(n, _)| n.as_str()).collect();
    println!("  Total libraries resolved: {}", libraries_list.len() - missing.len());

    let project_root = if let Some(p) = binary_path.parent() {
        if p.ends_with("Release") || p.ends_with("Debug") || p.ends_with("x64") {
            p.parent().and_then(|pp| pp.parent()).unwrap_or(&current_dir)
        } else {
            &current_dir
        }
    } else {
        &current_dir
    };

    let asset_collector = AssetCollector::discover(
        project_root,
        &config.asset_dirs,
        &extra_assets,
    );
    let referenced_fonts = asset_collector.detect_referenced_fonts(project_root);

    // Fallback fonts only where they can matter: font paths seen in the source, plus the
    // usual guesses for apps linking a text library. `fonts = false/true` in ship.toml forces it.
    let library_names: Vec<&str> = libraries_list.iter().map(|(n, _)| n.as_str()).collect();
    let font_mode = project.fonts;
    let common_font_locations = match font_mode {
        Some(force) => force,
        None => referenced_fonts.is_empty() && collector::font::needs_font_file(&library_names),
    };

    if args.dry_run {
        print_dry_run(&binary_path, &libraries_list, &os_libraries, &asset_collector, &referenced_fonts, common_font_locations, &project);
        if let Some(target) = &push_target {
            println!("{} {}", "Would push to:".bold(), target);
        }
        if project.sign.is_some() && target_platform == TargetPlatform::Windows {
            println!("{} yes ({})", "Sign:".bold(), project.sign.as_ref().unwrap().pkcs12.display());
        }
        if args.strict && !missing.is_empty() {
            anyhow::bail!("Missing libraries: {}", missing.join(", "));
        }
        return Ok(());
    }

    if args.strict && !missing.is_empty() {
        anyhow::bail!(
            "Missing libraries: {} (use --search-paths or search_paths in {} to point ship at them)",
            missing.join(", "),
            project::CONFIG_FILE
        );
    }

    let fallback_label = format!("{}-{:?}", app_name, target_platform).to_lowercase();
    let zip_label = custom_name.as_deref().unwrap_or(&fallback_label);
    let stub_options = StubOptions { console, project: &project };

    let mut staging = StagingBuilder::new(&output_dir, &app_name, target_platform)?;
    staging.copy_binary(&binary_path)?;
    staging.copy_libraries(&libraries_list)?;
    staging.copy_assets(asset_collector.directories())?;
    staging.copy_loose_files(asset_collector.files())?;
    staging.apply_excludes(&project)?;
    if font_mode != Some(false) {
        staging.enforce_fonts(&referenced_fonts, common_font_locations)?;
    }
    if let (Some(sign), Some(main_binary)) = (sign_config, staging.main_binary()) {
        // Before packaging, so the zip and the standalone payload carry the signed binary.
        packager::sign::sign_file(sign, &main_binary, &app_name)?;
    }

    // --appimage adds to the default zip unless -s/--all picked other formats.
    let want_zip = args.all || (!args.single && !args.appimage);
    let want_standalone = args.single || args.all;
    let mut outputs: Vec<(&str, PathBuf)> = Vec::new();

    if want_zip {
        println!("{} Creating .zip archive...", "⚡".bright_yellow());
        outputs.push(("ZIP package", staging.bundle_zip(Some(zip_label))?));
    }
    if want_standalone {
        println!("{} Creating standalone single executable...", "⚡".bright_yellow());
        let exe = staging.bundle_single_file(Some(zip_label), &stub_options)?;
        if let Some(sign) = sign_config {
            packager::sign::sign_file(sign, &exe, &app_name)?;
        }
        outputs.push(("Standalone executable", exe));
    }
    if args.appimage {
        println!("{} Creating AppImage...", "⚡".bright_yellow());
        outputs.push(("AppImage", staging.bundle_appimage(Some(zip_label), &stub_options)?));
    }
    if let Some(target) = &push_target {
        // The package folder when there is a zip (butler diffs folders efficiently),
        // otherwise the single file that was built.
        let push_path = if want_zip {
            staging.staging_dir().to_path_buf()
        } else {
            outputs[0].1.clone()
        };
        packager::itch::push(&push_path, target, project.version.as_deref())?;
        outputs.push(("Pushed to itch.io", PathBuf::from(target)));
    }
    staging.cleanup()?;

    println!();
    for (label, path) in outputs {
        println!("{} {}: {}", "SUCCESS!".bold().green(), label, path.display().to_string().bright_green());
    }

    Ok(())
}

fn print_dry_run(
    binary_path: &PathBuf,
    libraries: &[(String, Option<PathBuf>)],
    os_libraries: &HashSet<String>,
    assets: &AssetCollector,
    referenced_fonts: &[String],
    common_font_locations: bool,
    project: &ProjectConfig,
) {
    println!("\n{}", "Dry run - nothing was packaged".bold().bright_cyan());
    println!("{} {}", "Binary:".bold(), binary_path.display());

    println!("{}", "Libraries to bundle:".bold());
    if libraries.is_empty() {
        println!("  (none)");
    }
    for (name, path) in libraries {
        match path {
            Some(p) => println!("  {} {} {}", "✔".green(), name.cyan(), p.display().to_string().bright_black()),
            None => println!("  {} {} {}", "✖".red(), name.bright_red(), "NOT FOUND".red()),
        }
    }
    if !os_libraries.is_empty() {
        let mut names: Vec<&String> = os_libraries.iter().collect();
        names.sort();
        let names: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        println!("{} {}", "Provided by the OS (skipped):".bold(), names.join(", ").bright_black());
    }

    println!("{}", "Assets:".bold());
    if assets.directories().is_empty() && assets.files().is_empty() {
        println!("  (none)");
    }
    for dir in assets.directories() {
        println!("  {} {}/", "▸".bright_blue(), dir.display());
    }
    for file in assets.files() {
        println!("  {} {}", "▸".bright_blue(), file.display());
    }
    if !project.exclude.is_empty() {
        println!("{} {}", "Excluded:".bold(), project.exclude.join(", "));
    }

    let fonts = match (project.fonts, referenced_fonts.is_empty(), common_font_locations) {
        (Some(false), _, _) => "disabled in ship.toml".to_string(),
        (_, false, _) => format!("fallback for missing: {}", referenced_fonts.join(", ")),
        (_, true, true) => "fallback in common locations (app links a text library)".to_string(),
        (_, true, false) => "none needed".to_string(),
    };
    println!("{} {}", "Fonts:".bold(), fonts);
}
