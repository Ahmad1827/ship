pub mod cargo;
pub mod simple_cpp;

use anyhow::{anyhow, Result};
use colored::*;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TargetPlatform {
    Windows,
    Linux,
}

pub struct BuildResult {
    pub binary_path: PathBuf,
    pub project_name: String,
}

pub fn auto_build(project_dir: &Path, target: TargetPlatform) -> Result<BuildResult> {
    if project_dir.join("Cargo.toml").exists() {
        println!("{} Detected Rust/Cargo project", "➜".cyan());
        return cargo::build(project_dir, target);
    }

    if project_dir.join("CMakeLists.txt").exists() {
        println!("{} Detected CMake project", "➜".cyan());
        return simple_cpp::build_cmake(project_dir, target);
    }

    if project_dir.join("Makefile").exists() {
        println!("{} Detected Makefile project", "➜".cyan());
        return simple_cpp::build_make(project_dir, target);
    }

    let cpp_files = simple_cpp::find_source_files(project_dir);
    if !cpp_files.is_empty() {
        println!("{} Detected raw C/C++ files", "➜".cyan());
        return simple_cpp::build_direct(project_dir, &cpp_files, target);
    }

    Err(anyhow!("No recognizable project structure found in {:?}", project_dir))
}