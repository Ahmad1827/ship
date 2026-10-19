//! Linux AppImage output: the official type-2 runtime with a squashfs image of an AppDir
//! appended to it. That is all an AppImage is, so no appimagetool is needed.

use anyhow::{anyhow, Context, Result};
use colored::*;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;

use super::archive::{calculate_crc32, StableHasher};
use super::single_file::{find_icon_candidate, IconCandidate};
use crate::project::ProjectConfig;
use crate::sdk;

/// Removes the scratch AppDir and squashfs image even when a step fails.
struct Scratch<'a>(&'a Path, &'a Path);

impl Drop for Scratch<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0);
        let _ = fs::remove_file(self.1);
    }
}

pub fn build_appimage(
    staging_dir: &Path,
    output: &Path,
    main_exe_name: &str,
    project: &ProjectConfig,
    console: bool,
) -> Result<()> {
    let runtime = sdk::ensure_tool(&sdk::APPIMAGE_RUNTIME)?;

    let work_root = output.parent().unwrap_or_else(|| Path::new("."));
    let app_dir = work_root.join("ship_appimage.AppDir");
    let squashfs = work_root.join("ship_appimage.squashfs");
    let _scratch = Scratch(&app_dir, &squashfs);
    if app_dir.exists() {
        fs::remove_dir_all(&app_dir)?;
    }

    // The packaged app keeps the same layout as the zip, under AppDir/app/.
    let payload_dir = app_dir.join("app");
    for entry in WalkDir::new(staging_dir).into_iter().filter_map(|e| e.ok()) {
        let rel = entry.path().strip_prefix(staging_dir)?;
        let dest = payload_dir.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest)?;
        } else if entry.file_type().is_file() {
            fs::copy(entry.path(), &dest)?;
        }
    }

    let display_name = project.name.clone().unwrap_or_else(|| main_exe_name.to_string());
    let id = desktop_id(&display_name);

    let app_run = app_dir.join("AppRun");
    let script = if project.appimage.writable {
        writable_app_run_script(main_exe_name, &id, &hash_dir(&payload_dir)?)
    } else {
        app_run_script(main_exe_name)
    };
    fs::write(&app_run, script)?;
    set_executable(&app_run)?;

    fs::write(app_dir.join(format!("{}.desktop", id)), desktop_entry(&display_name, &id, main_exe_name, project, console))?;

    let icon_png = load_png_icon(staging_dir, project)?;
    fs::write(app_dir.join(format!("{}.png", id)), &icon_png)?;
    fs::write(app_dir.join(".DirIcon"), &icon_png)?;

    let status = Command::new("mksquashfs")
        .arg(&app_dir)
        .arg(&squashfs)
        .args(["-root-owned", "-noappend", "-comp", "zstd", "-quiet"])
        .status()
        .context("Failed to run mksquashfs (install squashfs-tools)")?;
    if !status.success() {
        return Err(anyhow!("mksquashfs failed to build the AppImage filesystem"));
    }

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut image = fs::read(&runtime)?;
    image.extend_from_slice(&fs::read(&squashfs)?);
    fs::write(output, image)?;
    set_executable(output)?;
    Ok(())
}

/// Same behaviour as the standalone launcher: libraries from the package, cwd = package dir.
fn app_run_script(exe_name: &str) -> String {
    format!(
        r#"#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export LD_LIBRARY_PATH="$HERE/app${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}"
cd "$HERE/app" || exit 1
exec ./{} "$@"
"#,
        shell_quote(exe_name)
    )
}

/// Copies the app into a per-user data dir the first time and after every update, then runs
/// it from there. Files the app creates itself survive updates; shipped files are refreshed.
fn writable_app_run_script(exe_name: &str, id: &str, build_hash: &str) -> String {
    format!(
        r#"#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
DATA="${{XDG_DATA_HOME:-$HOME/.local/share}}/ship/{id}"
if [ "$(cat "$DATA/.ship_build" 2>/dev/null)" != "{hash}" ]; then
    mkdir -p "$DATA" || exit 1
    cp -rf "$HERE/app/." "$DATA/" || exit 1
    chmod -R u+w "$DATA"
    echo "{hash}" > "$DATA/.ship_build"
fi
export LD_LIBRARY_PATH="$DATA${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}"
cd "$DATA" || exit 1
exec ./{exe} "$@"
"#,
        id = id,
        hash = build_hash,
        exe = shell_quote(exe_name)
    )
}

/// Content hash of a directory tree (paths and bytes), used to detect app updates.
fn hash_dir(dir: &Path) -> Result<String> {
    let mut files: Vec<_> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();
    files.sort();
    let mut hasher = StableHasher::new();
    for file in files {
        hasher.update(file.strip_prefix(dir)?.to_string_lossy().as_bytes());
        hasher.update(&[0]);
        hasher.update(&fs::read(&file)?);
    }
    Ok(hasher.finish())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn desktop_entry(name: &str, id: &str, exe_name: &str, project: &ProjectConfig, console: bool) -> String {
    let categories = project.categories.as_deref().unwrap_or("Game;");
    let mut entry = format!(
        "[Desktop Entry]\nType=Application\nName={}\nExec={}\nIcon={}\nTerminal={}\nCategories={}\n",
        name.replace('\n', " "),
        exe_name,
        id,
        console,
        if categories.ends_with(';') { categories.to_string() } else { format!("{};", categories) }
    );
    if let Some(description) = &project.description {
        entry.push_str(&format!("Comment={}\n", description.replace('\n', " ")));
    }
    entry
}

/// A file-name-safe id for the .desktop and icon files.
fn desktop_id(name: &str) -> String {
    let id: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if id.is_empty() { "app".into() } else { id }
}

/// The project's PNG icon, or a generated default (AppImages need a PNG, not an .ico).
fn load_png_icon(staging_dir: &Path, project: &ProjectConfig) -> Result<Vec<u8>> {
    let candidate = match &project.icon {
        Some(path) if path.extension().map_or(false, |e| e.eq_ignore_ascii_case("png")) => {
            Some(IconCandidate::Png(path.clone()))
        }
        Some(_) => None, // .ico configured: fine for Windows, but useless here
        None => find_icon_candidate(staging_dir),
    };
    if let Some(IconCandidate::Png(path)) = candidate {
        let data = fs::read(&path).with_context(|| format!("Failed to read icon {:?}", path))?;
        if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            println!("{} AppImage icon: {}", "🎨".bright_yellow(), path.display().to_string().bright_white());
            return Ok(data);
        }
    }
    println!("{} No PNG icon found, using default AppImage icon", "🎨".bright_cyan());
    Ok(default_icon_png())
}

/// 64x64 version of the default Windows icon: dark tile, border, gold diamond.
fn default_icon_png() -> Vec<u8> {
    const SIZE: u32 = 64;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let border = x < 4 || y < 4 || x >= 60 || y >= 60;
            let diamond = (x - 31).abs() + (y - 32).abs() <= 16;
            let rgb = if diamond {
                [0xff, 0xd0, 0x40]
            } else if border {
                [0x20, 0x30, 0x60]
            } else {
                [0x10, 0x14, 0x22]
            };
            rgba.extend_from_slice(&rgb);
            rgba.push(0xff);
        }
    }
    encode_png(SIZE, SIZE, &rgba)
}

/// Minimal RGBA8 PNG encoder (no filtering).
fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut crc_input = kind.to_vec();
        crc_input.extend_from_slice(data);
        out.extend_from_slice(&crc_input);
        out.extend_from_slice(&calculate_crc32(&crc_input).to_be_bytes());
    }

    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, RGBA, deflate, no filter, no interlace

    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for row in rgba.chunks(width as usize * 4) {
        raw.push(0); // filter type: none
        raw.extend_from_slice(row);
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&raw).expect("writing to a Vec cannot fail");
    let idat = encoder.finish().expect("writing to a Vec cannot fail");

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &idat);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_icon_is_png() {
        let png = default_icon_png();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(&png[12..16], b"IHDR");
        assert!(png.ends_with(&[0xae, 0x42, 0x60, 0x82])); // IEND CRC
    }

    #[test]
    fn quoting() {
        assert_eq!(shell_quote("my game"), "'my game'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(desktop_id("My Game!"), "My_Game_");
    }
}
