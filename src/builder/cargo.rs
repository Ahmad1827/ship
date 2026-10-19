use anyhow::{anyhow, Context, Result};
use colored::*;
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use super::{BuildResult, TargetPlatform};

struct BinArtifact {
    name: String,
    executable: PathBuf,
    from_root_manifest: bool,
}

/// Builds with `--message-format=json` and takes the executable paths cargo reports, so
/// workspaces, `[[bin]]` targets, renamed binaries and `CARGO_TARGET_DIR` all just work.
pub fn build(project_dir: &Path, target: TargetPlatform) -> Result<BuildResult> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(project_dir);
    cmd.arg("build").arg("--release");
    // Diagnostics still render to stderr as usual; only artifact messages come back as JSON.
    cmd.arg("--message-format=json-render-diagnostics");

    if target == TargetPlatform::Windows {
        cmd.arg("--target").arg("x86_64-pc-windows-gnu");
    }

    let mut child = cmd
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to execute cargo")?;

    let root_manifest = project_dir.join("Cargo.toml").canonicalize().ok();
    let mut bins = Vec::new();

    let stdout = child.stdout.take().context("cargo stdout unavailable")?;
    for line in BufReader::new(stdout).lines() {
        let Ok(msg) = serde_json::from_str::<Value>(&line?) else { continue };
        if msg["reason"] != "compiler-artifact" {
            continue;
        }
        let Some(exe) = msg["executable"].as_str() else { continue };
        let is_bin = msg["target"]["kind"]
            .as_array()
            .map_or(false, |kinds| kinds.iter().any(|k| k == "bin"));
        if !is_bin {
            continue;
        }

        let manifest = msg["manifest_path"].as_str().map(PathBuf::from);
        bins.push(BinArtifact {
            name: msg["target"]["name"].as_str().unwrap_or("app").to_string(),
            executable: PathBuf::from(exe),
            from_root_manifest: manifest.is_some() && manifest == root_manifest,
        });
    }

    let status = child.wait().context("Failed to wait for cargo")?;
    if !status.success() {
        return Err(anyhow!("Cargo build failed"));
    }

    pick_binary(project_dir, bins)
}

fn pick_binary(project_dir: &Path, mut bins: Vec<BinArtifact>) -> Result<BuildResult> {
    if bins.is_empty() {
        return Err(anyhow!("Cargo build produced no binary targets"));
    }

    // In a workspace, prefer binaries of the package in the directory ship was run from.
    if bins.iter().any(|b| b.from_root_manifest) {
        bins.retain(|b| b.from_root_manifest);
    }

    let dir_name = project_dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let index = bins.iter().position(|b| b.name == dir_name).unwrap_or(0);
    let chosen = bins.swap_remove(index);

    if !bins.is_empty() {
        let others: Vec<&str> = bins.iter().map(|b| b.name.as_str()).collect();
        println!(
            "  {} Multiple binaries built, packaging '{}' (others: {}). Pass a binary path to choose.",
            "➜".cyan(),
            chosen.name,
            others.join(", ")
        );
    }

    if !chosen.executable.exists() {
        return Err(anyhow!("Built binary not found at {:?}", chosen.executable));
    }

    Ok(BuildResult {
        binary_path: chosen.executable,
        project_name: chosen.name,
    })
}
