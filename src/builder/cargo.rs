use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use super::{BuildResult, TargetPlatform};

pub fn build(project_dir: &Path, target: TargetPlatform) -> Result<BuildResult> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(project_dir);
    cmd.arg("build").arg("--release");

    let target_triple = match target {
        TargetPlatform::Windows => {
            cmd.arg("--target").arg("x86_64-pc-windows-gnu");
            Some("x86_64-pc-windows-gnu")
        }
        TargetPlatform::Linux => None,
    };

    let status = cmd.status().context("Failed to execute cargo")?;
    if !status.success() {
        return Err(anyhow!("Cargo build failed"));
    }

    let cargo_toml = fs::read_to_string(project_dir.join("Cargo.toml"))?;
    let project_name = cargo_toml
        .lines()
        .find(|l| l.trim().starts_with("name ="))
        .and_then(|l| l.split('=').nth(1))
        .map(|s| s.trim().trim_matches('"'))
        .unwrap_or("app")
        .to_string();

    let binary_name = match target {
        TargetPlatform::Windows => format!("{}.exe", project_name),
        TargetPlatform::Linux => project_name.clone(),
    };

    let binary_path = match target_triple {
        Some(triple) => project_dir.join("target").join(triple).join("release").join(&binary_name),
        None => project_dir.join("target").join("release").join(&binary_name),
    };

    if !binary_path.exists() {
        return Err(anyhow!("Built binary not found at {:?}", binary_path));
    }

    Ok(BuildResult {
        binary_path,
        project_name,
    })
}