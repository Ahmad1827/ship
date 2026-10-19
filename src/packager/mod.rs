pub mod single_file;
pub mod zip;
pub mod archive;
pub mod appimage;

use anyhow::{Context, Result};
use colored::*;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::builder::TargetPlatform;
use crate::collector::font;
use crate::project::ProjectConfig;
use single_file::StubOptions;

pub struct StagingBuilder<'a> {
    staging_dir: PathBuf,
    output_dir: &'a Path,
    app_name: String,
    main_exe_name: Option<String>,
    target: TargetPlatform,
}

impl<'a> StagingBuilder<'a> {
    pub fn new(output_dir: &'a Path, app_name: &str, target: TargetPlatform) -> Result<Self> {
        fs::create_dir_all(output_dir)?;
        let staging_dir = output_dir.join(format!("{}_staging", app_name));
        if staging_dir.exists() {
            fs::remove_dir_all(&staging_dir)?;
        }
        fs::create_dir_all(&staging_dir)?;

        Ok(Self {
            staging_dir,
            output_dir,
            app_name: app_name.to_string(),
            main_exe_name: None,
            target,
        })
    }

    pub fn copy_binary<P: AsRef<Path>>(&mut self, binary_path: P) -> Result<()> {
        let path = binary_path.as_ref();
        let file_name = path.file_name().context("Binary has no filename")?;
        let file_name_str = file_name.to_string_lossy().to_string();
        let dest = self.staging_dir.join(&file_name);
        fs::copy(path, &dest)?;
        self.main_exe_name = Some(file_name_str.clone());
        println!("  {} Copied binary: {}", "✔".green(), file_name_str.bright_white());
        Ok(())
    }

    pub fn copy_libraries(&self, libraries: &[(String, Option<PathBuf>)]) -> Result<()> {
        for (name, resolved_path) in libraries {
            match resolved_path {
                Some(src) => {
                    let dest = self.staging_dir.join(name);
                    fs::copy(src, &dest)?;
                    println!("  {} Copied lib: {} (from {:?})", "✔".green(), name.cyan(), src);
                }
                None => {
                    println!("  {} Missing lib: {}", "✖".yellow(), name.bright_yellow());
                }
            }
        }
        Ok(())
    }

    pub fn copy_assets(&self, asset_dirs: &[PathBuf]) -> Result<()> {
        for dir in asset_dirs {
            let dir_name = dir.file_name().context("Asset dir has no name")?;
            let target_dir = self.staging_dir.join(dir_name);

            for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                let relative = path.strip_prefix(dir)?;
                let dest_path = target_dir.join(relative);

                if path.is_dir() {
                    fs::create_dir_all(&dest_path)?;
                } else if path.is_file() {
                    if let Some(parent) = dest_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::copy(path, &dest_path)?;
                }
            }
            println!("  {} Bundled asset folder: {}", "✔".green(), dir_name.to_string_lossy().bright_blue());
        }
        Ok(())
    }

    pub fn copy_loose_files(&self, files: &[PathBuf]) -> Result<()> {
        for file in files {
            let file_name = file.file_name().context("File has no name")?;
            let dest = self.staging_dir.join(file_name);
            fs::copy(file, &dest)?;
            println!("  {} Bundled file: {}", "✔".green(), file_name.to_string_lossy().bright_blue());
        }
        Ok(())
    }

    /// Removes everything under the `exclude` entries of ship.toml.
    pub fn apply_excludes(&self, project: &ProjectConfig) -> Result<()> {
        if project.exclude.is_empty() {
            return Ok(());
        }
        let mut walker = WalkDir::new(&self.staging_dir).min_depth(1).into_iter();
        while let Some(entry) = walker.next() {
            let entry = entry?;
            let rel = entry.path().strip_prefix(&self.staging_dir)?.to_string_lossy().replace('\\', "/");
            if !project.is_excluded(&rel) {
                continue;
            }
            if entry.file_type().is_dir() {
                fs::remove_dir_all(entry.path())?;
                walker.skip_current_dir();
            } else {
                fs::remove_file(entry.path())?;
            }
            println!("  {} Excluded: {}", "✂".bright_black(), rel.bright_black());
        }
        Ok(())
    }

    /// Adds a fallback font where the app expects one but none was packaged.
    pub fn enforce_fonts(&self, referenced_fonts: &[String], common_locations: bool) -> Result<()> {
        font::enforce_fonts(&self.staging_dir, referenced_fonts, common_locations)
    }

    pub fn bundle_zip(&self, custom_name: Option<&str>) -> Result<PathBuf> {
        let zip_name = match custom_name {
            Some(name) => format!("{}.zip", name),
            None => format!("{}-portable.zip", self.app_name),
        };
        let zip_path = self.output_dir.join(&zip_name);
        zip::compress_directory(&self.staging_dir, &zip_path)?;
        Ok(zip_path)
    }

    pub fn bundle_single_file(&self, custom_name: Option<&str>, opts: &StubOptions) -> Result<PathBuf> {
        let base = custom_name.unwrap_or(&self.app_name);
        let exe_name = match self.target {
            TargetPlatform::Windows => format!("{}-standalone.exe", base),
            TargetPlatform::Linux => format!("{}-standalone", base),
        };
        let output_exe = self.output_dir.join(&exe_name);

        match self.target {
            TargetPlatform::Windows => {
                let main_bin = self.main_exe_name.as_deref().unwrap_or("app.exe");
                single_file::build_standalone_exe(&self.staging_dir, &output_exe, main_bin, opts)?;
            }
            TargetPlatform::Linux => {
                let main_bin = self.main_exe_name.as_deref().unwrap_or("app");
                single_file::build_linux_standalone_exe(&self.staging_dir, &output_exe, main_bin)?;
            }
        }
        Ok(output_exe)
    }

    pub fn bundle_appimage(&self, custom_name: Option<&str>, opts: &StubOptions) -> Result<PathBuf> {
        if self.target != TargetPlatform::Linux {
            anyhow::bail!("AppImage is a Linux format; use --target linux");
        }
        let base = custom_name.unwrap_or(&self.app_name);
        let output = self.output_dir.join(format!("{}-x86_64.AppImage", base));
        let main_bin = self.main_exe_name.as_deref().unwrap_or("app");
        appimage::build_appimage(&self.staging_dir, &output, main_bin, opts.project, opts.console)?;
        Ok(output)
    }

    pub fn cleanup(&self) -> Result<()> {
        if self.staging_dir.exists() {
            fs::remove_dir_all(&self.staging_dir)?;
        }
        Ok(())
    }
}