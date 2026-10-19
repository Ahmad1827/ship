use std::fs;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

/// True for build outputs and VCS dirs that must never be scanned for assets or sources.
/// `target`/`dist` are common words (a "target" sprite folder), so they only count at the project root.
pub fn is_ignored_dir(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() || entry.depth() == 0 {
        return false;
    }
    let Some(name) = entry.file_name().to_str() else { return false };
    matches!(name, ".git" | "node_modules" | "CMakeFiles" | "build")
        || name.starts_with("build-")
        || name.starts_with("build_")
        || name.starts_with("cmake-build-")
        || (entry.depth() == 1 && matches!(name, "target" | "dist"))
}

pub struct AssetCollector {
    found_dirs: Vec<PathBuf>,
    found_files: Vec<PathBuf>,
}

impl AssetCollector {
    pub fn discover(project_dir: &Path, candidate_dir_names: &[&str], extra_assets: &[PathBuf]) -> Self {
        let mut found_dirs = Vec::new();
        let mut found_files = Vec::new();

        let walker = WalkDir::new(project_dir).max_depth(4).into_iter().filter_entry(|e| !is_ignored_dir(e));
        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();

            if entry.file_type().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    let name_lower = name.to_lowercase();
                    if candidate_dir_names.contains(&name_lower.as_str()) {
                        if !found_dirs.contains(&path.to_path_buf()) {
                            found_dirs.push(path.to_path_buf());
                        }
                    }
                }
            } else if entry.file_type().is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    let asset_extensions = [
                        "ttf", "otf", "fon", "png", "jpg", "jpeg", "bmp",
                        "wav", "ogg", "mp3", "json", "xml", "ini", "txt", "csv", "glsl", "vert", "frag",
                    ];
                    if asset_extensions.contains(&ext_lower.as_str()) {
                        if path.parent() == Some(project_dir) {
                            found_files.push(path.to_path_buf());
                        }
                    }
                }
            }
        }

        for extra in extra_assets {
            if extra.exists() {
                if extra.is_dir() && !found_dirs.contains(extra) {
                    found_dirs.push(extra.clone());
                } else if extra.is_file() && !found_files.contains(extra) {
                    found_files.push(extra.clone());
                }
            }
        }

        Self { found_dirs, found_files }
    }

    pub fn directories(&self) -> &[PathBuf] {
        &self.found_dirs
    }

    pub fn files(&self) -> &[PathBuf] {
        &self.found_files
    }

    /// Every "something.ttf" / "something.otf" string literal in the C/C++ sources.
    pub fn detect_referenced_fonts(&self, project_dir: &Path) -> Vec<String> {
        let mut referenced = Vec::new();

        let walker = WalkDir::new(project_dir).into_iter().filter_entry(|e| !is_ignored_dir(e));
        for entry in walker.filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();

            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if !matches!(ext, "cpp" | "hpp" | "h" | "cc" | "cxx") {
                continue;
            }
            let Ok(content) = fs::read_to_string(path) else { continue };

            for line in content.lines() {
                let lower = line.to_lowercase();
                if !(lower.contains(".ttf") || lower.contains(".otf")) {
                    continue;
                }
                for quote in ['"', '\''] {
                    let mut start_idx = None;
                    for (i, c) in line.char_indices() {
                        if c != quote {
                            continue;
                        }
                        if let Some(s) = start_idx {
                            let token = &line[s + 1..i];
                            let token_lower = token.to_lowercase();
                            if (token_lower.ends_with(".ttf") || token_lower.ends_with(".otf"))
                                && token.len() < 256
                                && !referenced.contains(&token.to_string())
                            {
                                referenced.push(token.to_string());
                            }
                            start_idx = None;
                        } else {
                            start_idx = Some(i);
                        }
                    }
                }
            }
        }

        referenced
    }
}