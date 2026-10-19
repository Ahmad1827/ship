use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = "ship.toml";

/// Optional per-project settings from `ship.toml`. Every field can be omitted, and
/// command-line flags always win over values set here.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectConfig {
    /// Output base name (same as `--name`).
    pub name: Option<String>,
    /// `windows` or `linux` (same as `--target`).
    pub target: Option<String>,
    /// Output directory (same as `--output`).
    pub output: Option<PathBuf>,

    /// Embedded in the Windows executable's version info, e.g. "1.2.0".
    pub version: Option<String>,
    pub description: Option<String>,
    pub company: Option<String>,
    pub copyright: Option<String>,
    /// `.ico` or `.png` used for the standalone executable.
    pub icon: Option<PathBuf>,

    /// Extra asset folders or files (added to `--extra-assets`).
    pub assets: Vec<PathBuf>,
    /// Package-relative paths to leave out, e.g. "assets/raw" or "notes.txt".
    pub exclude: Vec<String>,
    /// Extra library search paths (added to `--search-paths`).
    pub search_paths: Vec<PathBuf>,

    /// Console app instead of a GUI app on Windows: keeps the terminal attached.
    pub console: bool,
    /// Add fallback fonts where the app is missing one.
    pub fonts: Option<bool>,
    /// AppImage menu categories (freedesktop list), e.g. "Game;ActionGame;". Default "Game;".
    pub categories: Option<String>,

    /// Raw C/C++ builds only: libraries to link, e.g. ["z", "sfml-graphics"].
    pub link: Vec<String>,
    /// Raw C/C++ builds only: extra compiler flags, e.g. ["-std=c++20", "-Iinclude"].
    pub cflags: Vec<String>,
}

impl ProjectConfig {
    /// Loads `ship.toml` from `dir`, or the defaults when there is none.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(CONFIG_FILE);
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path).with_context(|| format!("Failed to read {:?}", path))?;
        let mut config: Self = toml::from_str(&text)
            .map_err(|e| anyhow!("Invalid {}: {}", CONFIG_FILE, e))?;

        // Relative paths in the file are relative to the file, not to where ship was run.
        let resolve = |p: &mut PathBuf| {
            if p.is_relative() {
                *p = dir.join(&*p);
            }
        };
        config.assets.iter_mut().for_each(resolve);
        config.search_paths.iter_mut().for_each(resolve);
        config.icon.as_mut().map(resolve);
        config.output.as_mut().map(resolve);
        Ok(config)
    }

    /// True if a package-relative path (forward slashes) falls under an `exclude` entry.
    pub fn is_excluded(&self, rel_path: &str) -> bool {
        self.exclude.iter().any(|ex| {
            let ex = ex.trim_matches('/');
            rel_path == ex || rel_path.starts_with(&format!("{}/", ex))
        })
    }

    /// The parts of `version` as the four numbers Windows version info expects.
    pub fn version_numbers(&self) -> [u16; 4] {
        let mut out = [1, 0, 0, 0];
        if let Some(v) = &self.version {
            let parts = v.split(['.', '-', '+']).map(|p| p.parse::<u16>().ok());
            for (slot, part) in out.iter_mut().zip(parts) {
                match part {
                    Some(n) => *slot = n,
                    None => break,
                }
            }
            // "2.0" means 2.0.0.0, not 2.0 with the leftover default build numbers.
            let given = v.split('.').count().min(4);
            out.iter_mut().skip(given).for_each(|n| *n = 0);
        }
        out
    }
}

pub const TEMPLATE: &str = r#"# ship.toml - every setting is optional; command-line flags override these.

# name = "MyGame"            # output base name
# target = "windows"         # "windows" or "linux"
# output = "dist"

# Windows version info and icon for the standalone .exe
# version = "1.0.0"
# description = "My awesome game"
# company = "Me"
# copyright = "Copyright (c) 2026 Me"
# icon = "assets/icon.png"   # .ico or .png

# assets = ["levels", "config.json"]   # extra folders/files beyond the auto-detected ones
# exclude = ["assets/raw", "notes.txt"] # package-relative paths to leave out
# search_paths = ["third_party/bin"]    # extra places to look for DLLs / .so files

# console = false    # true for terminal apps (keeps stdout/stdin on Windows)
# fonts = true       # add a fallback font where the app is missing one
# categories = "Game;"   # AppImage menu categories

# Raw C/C++ projects only (ignored for Cargo/CMake/Make):
# link = ["z", "sfml-graphics", "sfml-window", "sfml-system"]
# cflags = ["-std=c++20", "-Iinclude"]
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses() {
        let config: ProjectConfig = toml::from_str(TEMPLATE).expect("template must be valid TOML");
        assert!(config.name.is_none());
    }

    #[test]
    fn rejects_typos() {
        assert!(toml::from_str::<ProjectConfig>("nmae = \"x\"").is_err());
    }

    #[test]
    fn excludes_by_path_prefix() {
        let config = ProjectConfig { exclude: vec!["assets/raw".into(), "notes.txt".into()], ..Default::default() };
        assert!(config.is_excluded("assets/raw"));
        assert!(config.is_excluded("assets/raw/big.psd"));
        assert!(config.is_excluded("notes.txt"));
        assert!(!config.is_excluded("assets/rawhide.png"));
        assert!(!config.is_excluded("assets/level1.txt"));
    }

    #[test]
    fn version_numbers() {
        let v = |s: &str| ProjectConfig { version: Some(s.into()), ..Default::default() }.version_numbers();
        assert_eq!(v("1.2.3"), [1, 2, 3, 0]);
        assert_eq!(v("2.0"), [2, 0, 0, 0]);
        assert_eq!(v("0.4.1-beta"), [0, 4, 1, 0]);
        assert_eq!(ProjectConfig::default().version_numbers(), [1, 0, 0, 0]);
    }
}
