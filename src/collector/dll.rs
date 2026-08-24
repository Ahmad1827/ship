use anyhow::Result;
use colored::*;
use std::env;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;
use zip::ZipArchive;

fn ensure_sfml_msvc_sdk() -> Result<PathBuf> {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let ship_dir = PathBuf::from(home).join(".ship");
    let sfml_msvc_dir = ship_dir.join("sfml-msvc");

    if sfml_msvc_dir.exists() {
        return Ok(sfml_msvc_dir);
    }

    println!("{} Auto-fetching SFML MSVC (Visual Studio) SDK...", "⚡".bright_yellow());
    let zip_path = ship_dir.join("sfml-msvc.zip");
    let url = "https://github.com/SFML/SFML/releases/download/2.6.1/SFML-2.6.1-windows-vc17-64-bit.zip";

    let _ = Command::new("curl").arg("-L").arg("-o").arg(&zip_path).arg(url).status();

    if let Ok(file) = File::open(&zip_path) {
        if let Ok(mut archive) = ZipArchive::new(file) {
            for i in 0..archive.len() {
                if let Ok(mut file) = archive.by_index(i) {
                    if let Some(outpath) = file.enclosed_name().map(|p| ship_dir.join(p)) {
                        if file.is_dir() {
                            let _ = fs::create_dir_all(&outpath);
                        } else {
                            if let Some(p) = outpath.parent() {
                                let _ = fs::create_dir_all(p);
                            }
                            if let Ok(mut outfile) = File::create(&outpath) {
                                let _ = io::copy(&mut file, &mut outfile);
                            }
                        }
                    }
                }
            }
        }
    }

    let _ = fs::remove_file(zip_path);
    let extracted = ship_dir.join("SFML-2.6.1");
    if extracted.exists() {
        let _ = fs::rename(extracted, &sfml_msvc_dir);
    }

    println!("{} SFML MSVC SDK ready", "✔".green());
    Ok(sfml_msvc_dir)
}

pub struct LibraryResolver {
    search_paths: Vec<PathBuf>,
}

impl LibraryResolver {
    pub fn new(binary_path: &Path, user_search_paths: &[PathBuf], is_msvc: bool) -> Self {
        let mut search_paths = Vec::new();

        if let Some(parent) = binary_path.parent() {
            search_paths.push(parent.to_path_buf());
            if let Some(grandparent) = parent.parent() {
                search_paths.push(grandparent.to_path_buf());
            }
        }

        if let Ok(current) = env::current_dir() {
            search_paths.push(current);
        }

        search_paths.extend(user_search_paths.iter().cloned());

        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let ship_dir = PathBuf::from(home).join(".ship");

        if is_msvc {
            if let Ok(msvc_sdk) = ensure_sfml_msvc_sdk() {
                search_paths.push(msvc_sdk.join("bin"));
            }
        } else {
            search_paths.push(ship_dir.join("sfml-win").join("bin"));
            let mingw_roots = [
                "/usr/x86_64-w64-mingw32/bin",
                "/usr/x86_64-w64-mingw32/lib",
                "/usr/lib/gcc/x86_64-w64-mingw32",
            ];
            for root in mingw_roots {
                let pb = PathBuf::from(root);
                if pb.exists() {
                    search_paths.push(pb);
                }
            }
        }

        if let Some(path_var) = env::var_os("PATH") {
            for p in env::split_paths(&path_var) {
                search_paths.push(p);
            }
        }

        Self { search_paths }
    }

    pub fn find_library(&self, name: &str) -> Option<PathBuf> {
        let target_name = name.to_ascii_lowercase();

        for base_path in &self.search_paths {
            if !base_path.exists() {
                continue;
            }

            for entry in WalkDir::new(base_path).max_depth(5).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                        if file_name.to_ascii_lowercase() == target_name {
                            return Some(path.to_path_buf());
                        }
                    }
                }
            }
        }
        None
    }
}