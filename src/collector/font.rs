//! Font safety net.
//!
//! If a packaged SFML/raylib app can't load its font, every piece of text is
//! drawn as a single dot. This module only ADDS a fallback font where the app
//! has none. Fonts that belong to the app are never touched or overwritten.

use anyhow::Result;
use colored::*;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// A known-good font compiled into the `ship` binary itself, so packaging
/// never depends on the host machine or on the network.
/// Put any TTF there before building ship, e.g.:
///   cp /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf resources/fallback.ttf
pub static EMBEDDED_FONT: &[u8] = include_bytes!("../../resources/fallback.ttf");

/// Paths (relative to the package root) where apps usually look for a font.
pub const COMMON_FONT_TARGETS: &[&str] = &[
    "font.ttf",
    "arial.ttf",
    "Resources/font.ttf",
    "fonts/font.ttf",
    "assets/font.ttf",
    "assets/fonts/font.ttf",
];

/// True if `data` is a font FreeType can actually use for normal text:
/// it parses, and it has glyphs for letters and digits.
pub fn is_usable_font(data: &[u8]) -> bool {
    if data.len() < 1024 {
        return false; // empty file, HTML "404: Not Found", etc.
    }
    let count = ttf_parser::fonts_in_collection(data).unwrap_or(1).max(1);
    (0..count).any(|i| match ttf_parser::Face::parse(data, i) {
        Ok(face) => ['A', 'z', '0'].iter().all(|c| face.glyph_index(*c).is_some()),
        Err(_) => false,
    })
}

/// True if the file opens as a font at all (icon / symbol fonts count too).
pub fn parses_as_font(data: &[u8]) -> bool {
    let count = ttf_parser::fonts_in_collection(data).unwrap_or(1).max(1);
    (0..count).any(|i| ttf_parser::Face::parse(data, i).is_ok())
}

pub fn is_font_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc"))
        .unwrap_or(false)
}

/// The font we inject: a working system font if there is one, else the embedded one.
pub fn fallback_font_bytes() -> Vec<u8> {
    let candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/mnt/c/Windows/Fonts/arial.ttf",
    ];
    for c in candidates {
        if let Ok(data) = fs::read(c) {
            if is_usable_font(&data) {
                return data;
            }
        }
    }
    EMBEDDED_FONT.to_vec()
}

/// Turns a path found in the source code ("/fonts/font.ttf", "C:\\x\\a.ttf",
/// "../res/a.ttf") into a safe path *inside* the package. Never escapes it.
pub fn sanitize_relative(raw: &str) -> Option<PathBuf> {
    let unified = raw.replace('\\', "/");
    let without_drive = match unified.as_bytes() {
        [d, b':', ..] if d.is_ascii_alphabetic() => &unified[2..],
        _ => &unified[..],
    };
    let mut out = PathBuf::new();
    for comp in Path::new(without_drive).components() {
        if let Component::Normal(c) = comp {
            out.push(c);
        }
    }
    if out.as_os_str().is_empty() { None } else { Some(out) }
}

/// Adds the fallback font to every expected location that is still empty.
/// The app's own fonts always win: an existing file is never overwritten.
pub fn enforce_fonts(staging_dir: &Path, referenced: &[String]) -> Result<()> {
    let fallback = fallback_font_bytes();
    if !is_usable_font(&fallback) {
        anyhow::bail!("The fallback font is not a valid font. Replace resources/fallback.ttf and rebuild ship.");
    }

    // 1. Only WARN about app fonts that look broken - never replace them.
    for entry in walkdir::WalkDir::new(staging_dir).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if entry.file_type().is_file() && is_font_path(p) {
            let ok = fs::read(p).map(|d| parses_as_font(&d)).unwrap_or(false);
            if !ok {
                println!("  {} Font looks broken (kept as is): {}", "⚠".yellow(),
                         p.strip_prefix(staging_dir).unwrap_or(p).display().to_string().bright_yellow());
            }
        }
    }

    // 2. Common locations + every font path the source code mentions.
    let mut targets: Vec<PathBuf> = COMMON_FONT_TARGETS.iter().map(PathBuf::from).collect();
    for r in referenced {
        if let Some(rel) = sanitize_relative(r) {
            if let Some(name) = rel.file_name() {
                targets.push(PathBuf::from(name));   // also next to the exe
            }
            targets.push(rel);
        }
    }
    targets.sort();
    targets.dedup();

    let mut added = 0;
    for rel in targets {
        let dest = staging_dir.join(&rel);
        if dest.exists() {
            continue;   // the app already has a file here: keep it
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &fallback)?;
        added += 1;
    }
    println!("  {} Fonts verified ({} fallback copies added)", "✔".green(), added);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_font_is_valid() {
        assert!(is_usable_font(EMBEDDED_FONT));
    }

    #[test]
    fn rejects_garbage() {
        assert!(!is_usable_font(b"404: Not Found"));
        assert!(!is_usable_font(&vec![0u8; 5000]));
    }

    #[test]
    fn sanitizes_paths() {
        assert_eq!(sanitize_relative("/fonts/font.ttf"), Some(PathBuf::from("fonts/font.ttf")));
        assert_eq!(sanitize_relative("C:\\Windows\\Fonts\\arial.ttf"), Some(PathBuf::from("Windows/Fonts/arial.ttf")));
        assert_eq!(sanitize_relative("../res/a.ttf"), Some(PathBuf::from("res/a.ttf")));
        assert_eq!(sanitize_relative("./font.ttf"), Some(PathBuf::from("font.ttf")));
        assert_eq!(sanitize_relative("/"), None);
    }
}