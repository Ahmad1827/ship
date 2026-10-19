use colored::*;
use goblin::elf::Elf;
use std::cell::OnceCell;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

use crate::sdk;

/// Directories inside a project tree that never hold runtime libraries worth walking.
const PRUNED_DIRS: [&str; 3] = [".git", "node_modules", ".cache"];

struct SearchRoot {
    path: PathBuf,
    max_depth: usize,
    /// Lowercase file name -> first matching path, built on the first lookup that reaches this root.
    files: OnceCell<HashMap<String, PathBuf>>,
}

impl SearchRoot {
    fn new(path: PathBuf, max_depth: usize) -> Self {
        Self { path, max_depth, files: OnceCell::new() }
    }

    fn lookup(&self, lower_name: &str) -> Option<&PathBuf> {
        self.files
            .get_or_init(|| {
                let mut files = HashMap::new();
                let walker = WalkDir::new(&self.path)
                    .max_depth(self.max_depth)
                    .into_iter()
                    .filter_entry(|e| {
                        e.depth() == 0
                            || !e.file_name().to_str().map_or(false, |n| PRUNED_DIRS.contains(&n))
                    });
                for entry in walker.filter_map(|e| e.ok()) {
                    if !entry.file_type().is_file() && !entry.path_is_symlink() {
                        continue;
                    }
                    if let Some(name) = entry.file_name().to_str() {
                        files
                            .entry(name.to_ascii_lowercase())
                            .or_insert_with(|| entry.path().to_path_buf());
                    }
                }
                files
            })
            .get(lower_name)
    }
}

pub struct LibraryResolver {
    roots: Vec<SearchRoot>,
    is_elf: bool,
    ldconfig: OnceCell<HashMap<String, PathBuf>>,
}

impl LibraryResolver {
    pub fn new(binary_path: &Path, user_search_paths: &[PathBuf], is_msvc: bool) -> Self {
        let mut roots = Vec::new();
        let elf_paths = elf_runtime_paths(binary_path);
        let is_elf = elf_paths.is_some();

        // The binary's own RPATH/RUNPATH is what the loader would honour first.
        for p in elf_paths.unwrap_or_default() {
            roots.push(SearchRoot::new(p, 1));
        }

        if let Some(parent) = binary_path.parent() {
            roots.push(SearchRoot::new(parent.to_path_buf(), 5));
            if let Some(grandparent) = parent.parent() {
                roots.push(SearchRoot::new(grandparent.to_path_buf(), 5));
            }
        }

        if let Ok(current) = env::current_dir() {
            roots.push(SearchRoot::new(current, 5));
        }

        for p in user_search_paths {
            roots.push(SearchRoot::new(p.clone(), 5));
        }

        if !is_elf {
            if is_msvc {
                match sdk::ensure(&sdk::SFML_MSVC) {
                    Ok(dir) => roots.push(SearchRoot::new(dir.join("bin"), 5)),
                    Err(e) => println!("  {} Could not fetch SFML MSVC SDK: {:#}", "✖".yellow(), e),
                }
            } else {
                if let Ok(ship_dir) = sdk::ship_dir() {
                    roots.push(SearchRoot::new(ship_dir.join(sdk::SFML_MINGW.cache_name).join("bin"), 5));
                }
                for root in [
                    "/usr/x86_64-w64-mingw32/bin",
                    "/usr/x86_64-w64-mingw32/lib",
                    "/usr/lib/gcc/x86_64-w64-mingw32",
                ] {
                    roots.push(SearchRoot::new(PathBuf::from(root), 5));
                }
            }
        }

        // PATH entries are flat directories; walking them recursively is what made lookups
        // crawl on WSL, where PATH includes huge /mnt/c trees.
        if let Some(path_var) = env::var_os("PATH") {
            for p in env::split_paths(&path_var) {
                roots.push(SearchRoot::new(p, 1));
            }
        }

        roots.retain(|r| r.path.is_dir());
        Self { roots, is_elf, ldconfig: OnceCell::new() }
    }

    pub fn find_library(&self, name: &str) -> Option<PathBuf> {
        let lower = name.to_ascii_lowercase();

        if let Some(found) = self.roots.iter().find_map(|r| r.lookup(&lower)) {
            return Some(found.clone());
        }

        if self.is_elf {
            return self.ldconfig.get_or_init(load_ldconfig_cache).get(name).cloned();
        }
        None
    }
}

/// `Some(rpath dirs)` for ELF binaries, `None` for anything else.
fn elf_runtime_paths(binary_path: &Path) -> Option<Vec<PathBuf>> {
    let buffer = fs::read(binary_path).ok()?;
    let elf = Elf::parse(&buffer).ok()?;
    let origin = binary_path.parent().unwrap_or(Path::new("."));

    let paths = elf
        .runpaths
        .iter()
        .chain(elf.rpaths.iter())
        .flat_map(|entry| entry.split(':'))
        .filter(|p| !p.is_empty())
        .map(|p| {
            let expanded = p.replace("${ORIGIN}", "$ORIGIN");
            match expanded.strip_prefix("$ORIGIN") {
                Some(rest) => origin.join(rest.trim_start_matches('/')),
                None => PathBuf::from(expanded),
            }
        })
        .collect();
    Some(paths)
}

/// Parses `ldconfig -p` into soname -> path, keeping only 64-bit entries.
fn load_ldconfig_cache() -> HashMap<String, PathBuf> {
    let mut map = HashMap::new();
    let output = Command::new("ldconfig")
        .arg("-p")
        .output()
        .or_else(|_| Command::new("/sbin/ldconfig").arg("-p").output());
    let Ok(output) = output else { return map };

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        // "\tlibSDL2-2.0.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libSDL2-2.0.so.0"
        let Some((lhs, path)) = line.trim().split_once(" => ") else { continue };
        if !lhs.contains("x86-64") {
            continue;
        }
        if let Some(soname) = lhs.split_whitespace().next() {
            map.entry(soname.to_string()).or_insert_with(|| PathBuf::from(path));
        }
    }
    map
}
