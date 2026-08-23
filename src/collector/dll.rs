use std::env;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct LibraryResolver {
    search_paths: Vec<PathBuf>,
}

impl LibraryResolver {
    pub fn new(binary_dir: &Path, user_search_paths: &[PathBuf]) -> Self {
        let mut search_paths = Vec::new();

        search_paths.push(binary_dir.to_path_buf());
        search_paths.extend(user_search_paths.iter().cloned());

        if let Ok(home) = env::var("HOME") {
            let ship_dir = PathBuf::from(home).join(".ship");
            if ship_dir.exists() {
                search_paths.push(ship_dir.clone());
                search_paths.push(ship_dir.join("sfml-win").join("bin"));
            }
        }

        let mingw_roots = [
            "/usr/x86_64-w64-mingw32/bin",
            "/usr/x86_64-w64-mingw32/lib",
            "/usr/lib/gcc/x86_64-w64-mingw32",
            "/usr/i686-w64-mingw32/bin",
            "/usr/i686-w64-mingw32/lib",
            "/usr/lib/gcc/i686-w64-mingw32",
        ];

        for root in mingw_roots {
            let pb = PathBuf::from(root);
            if pb.exists() {
                search_paths.push(pb);
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

            for entry in WalkDir::new(base_path).max_depth(8).into_iter().filter_map(|e| e.ok()) {
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