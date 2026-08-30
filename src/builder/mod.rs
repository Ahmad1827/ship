use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetPlatform {
    Windows,
    Linux,
}

pub struct BuildResult {
    pub binary_path: PathBuf,
    pub project_name: String,
}

pub fn auto_build(project_dir: &Path, target: TargetPlatform) -> Result<BuildResult> {
    let name = project_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("app")
        .to_string();

    let build_dir = project_dir.join("build");
    std::fs::create_dir_all(&build_dir)?;

    let binary_path = match target {
        TargetPlatform::Windows => {
            let out = build_dir.join(format!("{}.exe", name));
            let mut cmd = Command::new("x86_64-w64-mingw32-g++");
            cmd.arg("-O3")
                .arg("-mwindows")
                .arg("-o")
                .arg(&out);

            for entry in walkdir::WalkDir::new(project_dir)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let p = entry.path();
                if p.is_file() && p.extension().map_or(false, |ext| ext == "cpp") {
                    if !p.starts_with(&build_dir) && !p.starts_with(project_dir.join("dist")) {
                        cmd.arg(p);
                    }
                }
            }

            cmd.arg("-lsfml-graphics")
                .arg("-lsfml-window")
                .arg("-lsfml-system")
                .arg("-lsfml-audio")
                .arg("-lsfml-network");

            let status = cmd.status()?;
            if !status.success() {
                anyhow::bail!("Compilation failed");
            }
            out
        }
        TargetPlatform::Linux => {
            let out = build_dir.join(&name);
            let mut cmd = Command::new("g++");
            cmd.arg("-O3").arg("-o").arg(&out);

            for entry in walkdir::WalkDir::new(project_dir)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let p = entry.path();
                if p.is_file() && p.extension().map_or(false, |ext| ext == "cpp") {
                    if !p.starts_with(&build_dir) && !p.starts_with(project_dir.join("dist")) {
                        cmd.arg(p);
                    }
                }
            }

            cmd.arg("-lsfml-graphics")
                .arg("-lsfml-window")
                .arg("-lsfml-system")
                .arg("-lsfml-audio")
                .arg("-lsfml-network");

            let status = cmd.status()?;
            if !status.success() {
                anyhow::bail!("Compilation failed");
            }
            out
        }
    };

    Ok(BuildResult {
        binary_path,
        project_name: name,
    })
}