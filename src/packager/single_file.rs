use anyhow::{anyhow, Context, Result};
use colored::*;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

use super::archive::{stable_hash, ArchiveBuilder};
use crate::project::ProjectConfig;

/// Windows-only knobs for the standalone launcher.
pub struct StubOptions<'a> {
    pub console: bool,
    pub project: &'a ProjectConfig,
}

/// Escapes a value for a double-quoted string in a .rc file.
fn rc_escape(value: &str) -> String {
    value.replace('"', "\"\"")
}

pub(super) enum IconCandidate {
    Ico(PathBuf),
    Png(PathBuf),
}

fn convert_png_to_ico(png_path: &Path, ico_path: &Path) -> Result<()> {
    let png_bytes = fs::read(png_path)?;
    if png_bytes.len() < 24 || &png_bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(anyhow!("Invalid PNG file: {:?}", png_path));
    }

    let width = u32::from_be_bytes([png_bytes[16], png_bytes[17], png_bytes[18], png_bytes[19]]);
    let height = u32::from_be_bytes([png_bytes[20], png_bytes[21], png_bytes[22], png_bytes[23]]);

    let b_width = if width >= 256 { 0u8 } else { width as u8 };
    let b_height = if height >= 256 { 0u8 } else { height as u8 };

    let mut ico = File::create(ico_path)?;

    ico.write_all(&[0x00, 0x00])?;
    ico.write_all(&[0x01, 0x00])?;
    ico.write_all(&[0x01, 0x00])?;

    ico.write_all(&[b_width])?;
    ico.write_all(&[b_height])?;
    ico.write_all(&[0x00])?;
    ico.write_all(&[0x00])?;
    ico.write_all(&[0x01, 0x00])?;
    ico.write_all(&[0x20, 0x00])?;

    let size = png_bytes.len() as u32;
    ico.write_all(&size.to_le_bytes())?;
    ico.write_all(&(22u32).to_le_bytes())?;

    ico.write_all(&png_bytes)?;
    Ok(())
}

fn generate_default_ico(ico_path: &Path) -> Result<()> {
    let mut buf = Vec::with_capacity(1150);

    buf.extend_from_slice(&[0x00, 0x00]);
    buf.extend_from_slice(&[0x01, 0x00]);
    buf.extend_from_slice(&[0x01, 0x00]);

    buf.extend_from_slice(&[16, 16, 0, 0]);
    buf.extend_from_slice(&[0x01, 0x00]);
    buf.extend_from_slice(&[0x20, 0x00]);
    buf.extend_from_slice(&(1128u32).to_le_bytes());
    buf.extend_from_slice(&(22u32).to_le_bytes());

    buf.extend_from_slice(&(40u32).to_le_bytes());
    buf.extend_from_slice(&(16i32).to_le_bytes());
    buf.extend_from_slice(&(32i32).to_le_bytes());
    buf.extend_from_slice(&(1u16).to_le_bytes());
    buf.extend_from_slice(&(32u16).to_le_bytes());
    buf.extend_from_slice(&(0u32).to_le_bytes());
    buf.extend_from_slice(&(1024u32).to_le_bytes());
    buf.extend_from_slice(&[0u8; 16]);

    for y in 0..16 {
        for x in 0..16 {
            let is_border = x == 0 || x == 15 || y == 0 || y == 15;
            let dx = (x as i32 - 7).abs();
            let dy = (y as i32 - 8).abs();
            let is_center = (dx + dy) <= 4;

            if is_center {
                buf.extend_from_slice(&[0x40, 0xd0, 0xff, 0xff]);
            } else if is_border {
                buf.extend_from_slice(&[0x60, 0x30, 0x20, 0xff]);
            } else {
                buf.extend_from_slice(&[0x22, 0x14, 0x10, 0xff]);
            }
        }
    }

    buf.resize(buf.len() + 64, 0);

    fs::write(ico_path, buf)?;
    Ok(())
}

pub(super) fn find_icon_candidate(staging_dir: &Path) -> Option<IconCandidate> {
    for entry in WalkDir::new(staging_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                if ext.eq_ignore_ascii_case("ico") {
                    return Some(IconCandidate::Ico(entry.path().to_path_buf()));
                }
            }
        }
    }

    if let Ok(cur) = env::current_dir() {
        for name in &["app.ico", "icon.ico"] {
            let p = cur.join(name);
            if p.is_file() {
                return Some(IconCandidate::Ico(p));
            }
        }
        for entry in WalkDir::new(&cur).max_depth(3).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                    if ext.eq_ignore_ascii_case("ico") {
                        return Some(IconCandidate::Ico(entry.path().to_path_buf()));
                    }
                }
            }
        }
    }

    for entry in WalkDir::new(staging_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
                if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                    if ext.eq_ignore_ascii_case("png")
                        && (stem.eq_ignore_ascii_case("icon")
                            || stem.eq_ignore_ascii_case("app")
                            || stem.to_ascii_lowercase().contains("icon"))
                    {
                        return Some(IconCandidate::Png(entry.path().to_path_buf()));
                    }
                }
            }
        }
    }

    if let Ok(cur) = env::current_dir() {
        for name in &[
            "icon.png",
            "app.png",
            "logo.png",
            "Resources/icon.png",
            "resources/icon.png",
            "assets/icon.png",
        ] {
            let p = cur.join(name);
            if p.is_file() {
                return Some(IconCandidate::Png(p));
            }
        }
        for entry in WalkDir::new(&cur).max_depth(3).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
                    if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                        if ext.eq_ignore_ascii_case("png")
                            && (stem.eq_ignore_ascii_case("icon")
                                || stem.eq_ignore_ascii_case("app")
                                || stem.to_ascii_lowercase().contains("icon"))
                        {
                            return Some(IconCandidate::Png(entry.path().to_path_buf()));
                        }
                    }
                }
            }
        }
    }

    None
}

// Launcher sources, compiled per package with a generated ship_config.h.
const STUB_LZ4_C: &str = include_str!("stub/lz4.c");
const STUB_LINUX_C: &str = include_str!("stub/linux.c");
const STUB_WINDOWS_C: &str = include_str!("stub/windows.c");

/// Scratch dir for one stub build; removed on drop so failed builds don't leave it behind.
struct StubBuildDir(PathBuf);

impl Drop for StubBuildDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Escapes a value for a C string literal.
fn c_string(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii_graphic() || c == ' ' => out.push(c),
            c => {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    // Octal escapes always stop after 3 digits, unlike \x.
                    out.push_str(&format!("\\{:03o}", b));
                }
            }
        }
    }
    out.push('"');
    out
}

/// Packs the staging dir and writes everything the stub compiler needs into `build_dir`:
/// the payload, an assembly file that embeds it, the C sources and ship_config.h.
/// Returns the source files to compile.
fn prepare_stub_sources(
    staging_dir: &Path,
    build_dir: &Path,
    exe_name: &str,
    console: bool,
    stub_source: &str,
    rodata_section: &str,
) -> Result<Vec<PathBuf>> {
    let mut archive_builder = ArchiveBuilder::new();
    for entry in WalkDir::new(staging_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let path = entry.path();
            let rel = path.strip_prefix(staging_dir)?.to_string_lossy().replace('\\', "/");
            archive_builder.add_file(path, &rel)?;
        }
    }
    let archive_bytes = archive_builder.build()?;
    let build_hash = stable_hash(&archive_bytes);

    fs::create_dir_all(build_dir)?;
    let payload = build_dir.join("payload.bin");
    fs::write(&payload, &archive_bytes)?;

    let mut asm = format!(
        "{}\n.global ship_payload_start\n.global ship_payload_end\nship_payload_start:\n    .incbin {}\nship_payload_end:\n",
        rodata_section,
        c_string(&payload.to_string_lossy())
    );
    if rodata_section.contains("rodata") {
        asm.push_str(".section .note.GNU-stack,\"\",@progbits\n");
    }
    let asm_file = build_dir.join("payload.s");
    fs::write(&asm_file, asm)?;

    fs::write(
        build_dir.join("ship_config.h"),
        format!(
            "#define SHIP_BUILD_HASH {}\n#define SHIP_EXE_NAME {}\n#define SHIP_CONSOLE {}\n",
            c_string(&build_hash),
            c_string(exe_name),
            if console { 1 } else { 0 }
        ),
    )?;

    let lz4_file = build_dir.join("lz4.c");
    fs::write(&lz4_file, STUB_LZ4_C)?;
    let stub_file = build_dir.join("stub.c");
    fs::write(&stub_file, stub_source)?;

    Ok(vec![asm_file, stub_file, lz4_file])
}

fn run_compiler(mut cmd: Command, what: &str) -> Result<()> {
    let program = cmd.get_program().to_string_lossy().to_string();
    let status = cmd
        .status()
        .with_context(|| format!("Failed to invoke {} for {}", program, what))?;
    if !status.success() {
        return Err(anyhow!("{} failed to build {}", program, what));
    }
    Ok(())
}

pub fn build_linux_standalone_exe(
    staging_dir: &Path,
    output_exe: &Path,
    main_exe_name: &str,
) -> Result<()> {
    let build_dir = StubBuildDir(staging_dir.parent().unwrap_or_else(|| Path::new(".")).join("ship_linux_build"));
    let sources = prepare_stub_sources(staging_dir, &build_dir.0, main_exe_name, false, STUB_LINUX_C, ".section .rodata")?;

    if let Some(parent) = output_exe.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut cmd = Command::new("gcc");
    cmd.arg("-O2")
        // Paths are bounded at 1 KB; gcc's "may be truncated" notes are just noise for users.
        .arg("-Wno-format-truncation")
        .arg("-I")
        .arg(&build_dir.0)
        .args(&sources)
        .arg("-o")
        .arg(output_exe);
    run_compiler(cmd, "the Linux standalone launcher")
}

pub fn build_standalone_exe(staging_dir: &Path, output_exe: &Path, main_exe_name: &str, opts: &StubOptions) -> Result<()> {
    let target_bin_name = if main_exe_name.ends_with(".exe") {
        main_exe_name.to_string()
    } else {
        format!("{}.exe", main_exe_name)
    };

    let pthread_candidates = [
        "/usr/x86_64-w64-mingw32/lib/libwinpthread-1.dll",
        "/usr/lib/gcc/x86_64-w64-mingw32/13-win32/libwinpthread-1.dll",
        "/usr/lib/gcc/x86_64-w64-mingw32/12-win32/libwinpthread-1.dll",
    ];
    for p in &pthread_candidates {
        let pth = Path::new(p);
        if pth.is_file() {
            let dest = staging_dir.join("libwinpthread-1.dll");
            if !dest.exists() {
                let _ = fs::copy(pth, dest);
            }
            break;
        }
    }

    let build_dir = StubBuildDir(staging_dir.parent().unwrap_or_else(|| Path::new(".")).join("ship_stub_build"));
    let temp_build = &build_dir.0;
    let sources = prepare_stub_sources(
        staging_dir,
        temp_build,
        &target_bin_name,
        opts.console,
        STUB_WINDOWS_C,
        ".section .rdata,\"dr\"",
    )?;

    let icon_dest = temp_build.join("icon.ico");
    let icon = match &opts.project.icon {
        Some(path) if path.extension().map_or(false, |e| e.eq_ignore_ascii_case("png")) => Some(IconCandidate::Png(path.clone())),
        Some(path) => Some(IconCandidate::Ico(path.clone())),
        None => find_icon_candidate(staging_dir),
    };
    match icon {
        Some(IconCandidate::Ico(ico_path)) => {
            println!("{} Found icon file: {}", "🎨".bright_yellow(), ico_path.display().to_string().bright_white());
            fs::copy(&ico_path, &icon_dest).with_context(|| format!("Failed to read icon {:?}", ico_path))?;
        }
        Some(IconCandidate::Png(png_path)) => {
            println!("{} Converting PNG icon: {}", "🎨".bright_yellow(), png_path.display().to_string().bright_white());
            convert_png_to_ico(&png_path, &icon_dest)?;
        }
        None => {
            println!("{} No icon found, embedding default icon", "🎨".bright_cyan());
            let _ = generate_default_ico(&icon_dest);
        }
    }

    let rc_file = temp_build.join("resource.rc");
    let project = opts.project;
    let app_name = rc_escape(main_exe_name.trim_end_matches(".exe"));
    let [v1, v2, v3, v4] = project.version_numbers();
    let version = format!("{}.{}.{}.{}", v1, v2, v3, v4);
    let product = project.name.as_deref().map(rc_escape).unwrap_or_else(|| app_name.clone());
    let description = project.description.as_deref().map(rc_escape).unwrap_or_else(|| product.clone());
    let company = project.company.as_deref().map(rc_escape).unwrap_or_default();
    let copyright = project.copyright.as_deref().map(rc_escape).unwrap_or_default();
    let rc_content = format!(
        r#"1 ICON "icon.ico"

1 VERSIONINFO
FILEVERSION {v1},{v2},{v3},{v4}
PRODUCTVERSION {v1},{v2},{v3},{v4}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "{company}"
            VALUE "FileDescription", "{description}"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "{app_name}"
            VALUE "LegalCopyright", "{copyright}"
            VALUE "OriginalFilename", "{app_name}.exe"
            VALUE "ProductName", "{product}"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#
    );
    fs::write(&rc_file, rc_content)?;

    let res_file = temp_build.join("resource.o");
    let mut windres = Command::new("x86_64-w64-mingw32-windres");
    windres.current_dir(temp_build).args(["resource.rc", "-O", "coff", "-o", "resource.o"]);
    run_compiler(windres, "the icon and version resources")?;

    if let Some(parent) = output_exe.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut cmd = Command::new("x86_64-w64-mingw32-gcc");
    cmd.arg("-O2")
        .arg("-static")
        .arg("-static-libgcc")
        .arg(if opts.console { "-mconsole" } else { "-mwindows" })
        .arg("-I")
        .arg(temp_build)
        .args(&sources)
        .arg(&res_file)
        .arg("-luser32")
        .arg("-lkernel32")
        .arg("-o")
        .arg(output_exe);
    run_compiler(cmd, "the Windows standalone launcher")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_string_escapes() {
        assert_eq!(c_string("game.exe"), "\"game.exe\"");
        assert_eq!(c_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(c_string("é"), "\"\\303\\251\"");
    }
}
