use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;
use super::{BuildOptions, BuildResult, TargetPlatform};
use crate::collector::assets::is_ignored_dir;

pub fn find_source_files(dir: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    let walker = WalkDir::new(dir).max_depth(3).into_iter().filter_entry(|e| !is_ignored_dir(e));
    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if matches!(ext, "cpp" | "c" | "cc" | "cxx") {
                    sources.push(path.to_path_buf());
                }
            }
        }
    }
    sources
}

pub fn build_direct(project_dir: &Path, sources: &[PathBuf], target: TargetPlatform, opts: &BuildOptions) -> Result<BuildResult> {
    let project_name = project_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("app")
        .to_string();

    let (compiler, binary_name, subfolder) = match target {
        TargetPlatform::Windows => ("x86_64-w64-mingw32-g++", format!("{}.exe", project_name), "build-ship-windows"),
        TargetPlatform::Linux => ("g++", project_name.clone(), "build-ship-linux"),
    };

    let build_dir = project_dir.join(subfolder);
    fs::create_dir_all(&build_dir)?;
    let output_bin = build_dir.join(&binary_name);

    let mut cmd = Command::new(compiler);
    cmd.current_dir(project_dir);
    cmd.arg("-O3");
    cmd.args(&opts.cflags);

    if target == TargetPlatform::Windows {
        if !opts.console {
            cmd.arg("-mwindows");
        }

        for entry in WalkDir::new(project_dir).max_depth(2).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                    if ext.eq_ignore_ascii_case("rc") {
                        let res_o = build_dir.join("app_res.o");
                        let st = Command::new("x86_64-w64-mingw32-windres")
                            .current_dir(project_dir)
                            .arg(entry.path())
                            .arg("-O")
                            .arg("coff")
                            .arg("-o")
                            .arg(&res_o)
                            .status();
                        if let Ok(s) = st {
                            if s.success() && res_o.exists() {
                                cmd.arg(&res_o);
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    for src in sources {
        cmd.arg(src);
    }
    // Libraries go after the sources so the linker can resolve their symbols.
    for lib in &opts.link {
        cmd.arg(format!("-l{}", lib.trim_start_matches("-l")));
    }
    cmd.arg("-o").arg(&output_bin);

    let status = cmd.status().with_context(|| format!("Failed to invoke compiler: {}", compiler))?;
    if !status.success() {
        return Err(anyhow!("C++ compilation failed"));
    }

    Ok(BuildResult {
        binary_path: output_bin,
        project_name,
    })
}

fn extract_cmake_target_name(project_dir: &Path) -> Option<String> {
    let cmakelists = fs::read_to_string(project_dir.join("CMakeLists.txt")).ok()?;
    for line in cmakelists.lines() {
        let trimmed = line.trim();
        if trimmed.to_ascii_lowercase().starts_with("add_executable(") {
            let inner = trimmed
                .trim_start_matches(|c: char| c != '(')
                .trim_start_matches('(')
                .trim_end_matches(')');
            if let Some(target) = inner.split_whitespace().next() {
                return Some(target.to_string());
            }
        }
    }
    None
}

pub fn build_cmake(project_dir: &Path, target: TargetPlatform, opts: &BuildOptions) -> Result<BuildResult> {
    let subfolder = match target {
        TargetPlatform::Windows => "build-ship-windows",
        TargetPlatform::Linux => "build-ship-linux",
    };
    let build_dir = project_dir.join(subfolder);
    fs::create_dir_all(&build_dir)?;

    let mut config_cmd = Command::new("cmake");
    config_cmd.current_dir(&build_dir);
    config_cmd.arg("-DCMAKE_BUILD_TYPE=Release");

    if target == TargetPlatform::Windows {
        config_cmd.arg("-DCMAKE_SYSTEM_NAME=Windows");
        config_cmd.arg("-DCMAKE_C_COMPILER=x86_64-w64-mingw32-gcc");
        config_cmd.arg("-DCMAKE_CXX_COMPILER=x86_64-w64-mingw32-g++");
        if !opts.console {
            config_cmd.arg("-DCMAKE_EXE_LINKER_FLAGS=-mwindows");
        }

        let cmakelists = fs::read_to_string(project_dir.join("CMakeLists.txt")).unwrap_or_default();
        if cmakelists.contains("SFML") {
            let sfml_path = crate::sdk::ensure(&crate::sdk::SFML_MINGW)?;
            config_cmd.arg(format!("-DCMAKE_PREFIX_PATH={}", sfml_path.display()));
            config_cmd.arg(format!("-DSFML_DIR={}/lib/cmake/SFML", sfml_path.display()));
        }
    }
    config_cmd.arg("..");

    let status = config_cmd.status().context("Failed to configure CMake")?;
    if !status.success() {
        return Err(anyhow!("CMake configuration failed"));
    }

    let mut build_cmd = Command::new("cmake");
    build_cmd.current_dir(&build_dir);
    build_cmd.arg("--build").arg(".").arg("--config").arg("Release");

    let status = build_cmd.status().context("Failed to build CMake project")?;
    if !status.success() {
        return Err(anyhow!("CMake build failed"));
    }

    let target_hint = extract_cmake_target_name(project_dir);
    let fallback_name = project_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("app")
        .to_string();

    let ext = if target == TargetPlatform::Windows { ".exe" } else { "" };

    for entry in WalkDir::new(&build_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let path_str = path.to_string_lossy();

        if path_str.contains("CMakeFiles") || path_str.contains("CMakeTmp") {
            continue;
        }

        if path.is_file() {
            if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                if file_name.ends_with(ext) && !file_name.contains("CMake") {
                    if let Some(ref hint) = target_hint {
                        let expected = format!("{}{}", hint, ext);
                        if file_name.eq_ignore_ascii_case(&expected) {
                            return Ok(BuildResult {
                                binary_path: path.to_path_buf(),
                                project_name: hint.clone(),
                            });
                        }
                    } else if file_name != "a.exe" {
                        return Ok(BuildResult {
                            binary_path: path.to_path_buf(),
                            project_name: file_name.trim_end_matches(".exe").to_string(),
                        });
                    }
                }
            }
        }
    }

    for entry in WalkDir::new(&build_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let path_str = path.to_string_lossy();
        if !path_str.contains("CMakeFiles") && path.is_file() {
            if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                if file_name.ends_with(ext) {
                    return Ok(BuildResult {
                        binary_path: path.to_path_buf(),
                        project_name: fallback_name,
                    });
                }
            }
        }
    }

    Err(anyhow!("Could not locate built binary in {:?}", build_dir))
}

pub fn build_make(project_dir: &Path, target: TargetPlatform, opts: &BuildOptions) -> Result<BuildResult> {
    let mut cmd = Command::new("make");
    cmd.current_dir(project_dir);

    if target == TargetPlatform::Windows {
        cmd.arg("CC=x86_64-w64-mingw32-gcc");
        cmd.arg("CXX=x86_64-w64-mingw32-g++");
        if !opts.console {
            cmd.arg("LDFLAGS=-mwindows");
        }
    }

    let status = cmd.status().context("Failed to run make")?;
    if !status.success() {
        return Err(anyhow!("Make failed"));
    }

    let project_name = project_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("app")
        .to_string();

    let ext = if target == TargetPlatform::Windows { ".exe" } else { "" };
    for entry in WalkDir::new(project_dir).max_depth(3).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let path_str = path.to_string_lossy();
        if path_str.contains("CMakeFiles") {
            continue;
        }

        if path.is_file() {
            if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                if file_name.ends_with(ext) && !file_name.contains('.') && target == TargetPlatform::Linux {
                    return Ok(BuildResult {
                        binary_path: path.to_path_buf(),
                        project_name,
                    });
                } else if target == TargetPlatform::Windows && file_name.ends_with(".exe") {
                    return Ok(BuildResult {
                        binary_path: path.to_path_buf(),
                        project_name,
                    });
                }
            }
        }
    }

    Err(anyhow!("Could not locate built binary after make"))
}