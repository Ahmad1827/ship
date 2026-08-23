mod builder;
mod cli;
mod collector;
mod config;
mod packager;
mod parser;

use anyhow::{Context, Result};
use clap::Parser;
use colored::*;
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};

use builder::{auto_build, TargetPlatform};
use cli::Cli;
use collector::assets::{AssetCollector, get_guaranteed_font};
use collector::dll::LibraryResolver;
use config::Config;
use packager::StagingBuilder;
use parser::{BinaryFormat, parse_dependencies};

fn main() -> Result<()> {
    let args = Cli::parse();
    let config = Config::new();
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    let target_platform = match args.target.to_lowercase().as_str() {
        "windows" | "win" => TargetPlatform::Windows,
        "linux" | "lin" => TargetPlatform::Linux,
        other => anyhow::bail!("Unsupported target: {}. Use 'windows' or 'linux'", other),
    };

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
            let result = auto_build(&current_dir, target_platform)?;
            println!("{} Build succeeded: {}", "✔".green(), result.binary_path.display().to_string().bright_white());
            (result.binary_path, result.project_name)
        }
    };

    println!("{}", format!("Packaging {}...", app_name).bold().bright_magenta());

    let binary_dir = binary_path.parent().unwrap_or_else(|| Path::new("."));
    let resolver = LibraryResolver::new(binary_dir, &args.search_paths);

    let mut resolved_libraries: HashMap<String, Option<PathBuf>> = HashMap::new();
    let mut to_scan: Vec<PathBuf> = vec![binary_path.clone()];
    let mut scanned: HashSet<PathBuf> = HashSet::new();

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

                if !is_system && !resolved_libraries.contains_key(&dep) {
                    let location = resolver.find_library(&dep);
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

    let libraries_list: Vec<(String, Option<PathBuf>)> = resolved_libraries.into_iter().collect();
    println!("  Total libraries resolved: {}", libraries_list.len());

    let asset_collector = AssetCollector::discover(
        &current_dir,
        &config.asset_dirs,
        &args.extra_assets,
    );

    let valid_font = get_guaranteed_font();

    let fallback_label = format!("{}-{:?}", app_name, target_platform).to_lowercase();
    let zip_label = args.name.as_deref().unwrap_or(&fallback_label);

    let mut staging = StagingBuilder::new(&args.output, &app_name)?;
    staging.copy_binary(&binary_path)?;
    staging.copy_libraries(&libraries_list)?;
    staging.copy_assets(asset_collector.directories(), &valid_font)?;
    staging.copy_loose_files(asset_collector.files())?;
    staging.enforce_resources(&valid_font)?;

    if args.single {
        println!("{} Creating standalone single executable...", "⚡".bright_yellow());
        let exe_path = staging.bundle_single_file(Some(zip_label))?;
        staging.cleanup()?;
        println!("\n{} Standalone executable ready: {}", "SUCCESS!".bold().green(), exe_path.display().to_string().bright_green());
    } else if args.all {
        println!("{} Creating .zip archive...", "⚡".bright_yellow());
        let zip_path = staging.bundle_zip(Some(zip_label))?;
        println!("{} Creating standalone single executable...", "⚡".bright_yellow());
        let exe_path = staging.bundle_single_file(Some(zip_label))?;
        staging.cleanup()?;
        println!("\n{} ZIP package: {}", "SUCCESS!".bold().green(), zip_path.display().to_string().bright_green());
        println!("{} Standalone EXE: {}", "SUCCESS!".bold().green(), exe_path.display().to_string().bright_green());
    } else {
        let zip_path = staging.bundle_zip(Some(zip_label))?;
        staging.cleanup()?;
        println!("\n{} ZIP package ready: {}", "SUCCESS!".bold().green(), zip_path.display().to_string().bright_green());
    }

    Ok(())
}