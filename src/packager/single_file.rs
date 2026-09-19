use anyhow::{anyhow, Context, Result};
use colored::*;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

enum IconCandidate {
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

fn find_icon_candidate(staging_dir: &Path) -> Option<IconCandidate> {
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
        for name in &["wisdomParkicon.ico", "app.ico", "icon.ico"] {
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

pub fn build_standalone_exe(staging_dir: &Path, output_exe: &Path, main_exe_name: &str) -> Result<()> {
    let mut files = Vec::new();

    for entry in WalkDir::new(staging_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let path = entry.path().to_path_buf();
            let rel = path.strip_prefix(staging_dir)?.to_string_lossy().replace('\\', "/");
            files.push((path, rel));
        }
    }

    let temp_build = staging_dir.parent().unwrap_or_else(|| Path::new(".")).join("ship_stub_build");
    fs::create_dir_all(&temp_build)?;

    let asm_file = temp_build.join("payload.s");
    let c_file = temp_build.join("stub.c");

    let mut asm_content = String::from(".section .rdata,\"dr\"\n");
    let mut c_content = String::new();

    c_content.push_str("#include <windows.h>\n#include <stdio.h>\n#include <stdlib.h>\n\n");
    c_content.push_str("typedef struct { const char* rel_path; const unsigned char* start; const unsigned char* end; } EmbeddedFile;\n\n");

    for (i, (abs_path, _)) in files.iter().enumerate() {
        asm_content.push_str(&format!(
            ".global payload_file_{i}\n.global payload_file_{i}_end\npayload_file_{i}:\n    .incbin \"{}\"\npayload_file_{i}_end:\n\n",
            abs_path.display()
        ));

        c_content.push_str(&format!(
            "extern const unsigned char payload_file_{i}[];\nextern const unsigned char payload_file_{i}_end[];\n"
        ));
    }

    c_content.push_str("\nstatic const EmbeddedFile g_files[] = {\n");
    for (i, (_, rel_path)) in files.iter().enumerate() {
        c_content.push_str(&format!(
            "    {{ \"{rel_path}\", payload_file_{i}, payload_file_{i}_end }},\n"
        ));
    }
    c_content.push_str("};\n\n");

    c_content.push_str(&format!(
        r#"
static void create_parent_dirs(char* path) {{
    for (char* p = path; *p; p++) {{
        if (*p == '/' || *p == '\\') {{
            char old = *p;
            *p = '\0';
            CreateDirectoryA(path, NULL);
            *p = old;
        }}
    }}
}}

int WINAPI WinMain(HINSTANCE hInst, HINSTANCE hPrev, LPSTR lpCmdLine, int nCmdShow) {{
    char temp[MAX_PATH];
    GetTempPathA(MAX_PATH, temp);

    char app_dir[MAX_PATH];
    snprintf(app_dir, MAX_PATH, "%sShipApp_%s", temp, "{main_exe_name}");
    CreateDirectoryA(app_dir, NULL);

    size_t count = sizeof(g_files) / sizeof(g_files[0]);
    for (size_t i = 0; i < count; i++) {{
        char out_path[MAX_PATH];
        snprintf(out_path, MAX_PATH, "%s/%s", app_dir, g_files[i].rel_path);
        create_parent_dirs(out_path);

        HANDLE hFile = CreateFileA(out_path, GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
        if (hFile != INVALID_HANDLE_VALUE) {{
            DWORD written = 0;
            size_t size = (size_t)(g_files[i].end - g_files[i].start);
            WriteFile(hFile, g_files[i].start, (DWORD)size, &written, NULL);
            CloseHandle(hFile);
        }}
    }}

    char exe_path[MAX_PATH];
    snprintf(exe_path, MAX_PATH, "%s/{main_exe_name}", app_dir);

    STARTUPINFOA si;
    PROCESS_INFORMATION pi;
    ZeroMemory(&si, sizeof(si));
    si.cb = sizeof(si);
    ZeroMemory(&pi, sizeof(pi));

    if (CreateProcessA(exe_path, GetCommandLineA(), NULL, NULL, FALSE, 0x08000000, NULL, app_dir, &si, &pi)) {{
        WaitForSingleObject(pi.hProcess, INFINITE);
        DWORD code = 0;
        GetExitCodeProcess(pi.hProcess, &code);
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
        return (int)code;
    }}
    return 1;
}}
"#
    ));

    fs::write(&asm_file, asm_content)?;
    fs::write(&c_file, c_content)?;

    let icon_dest = temp_build.join("icon.ico");
    match find_icon_candidate(staging_dir) {
        Some(IconCandidate::Ico(ico_path)) => {
            println!("{} Found icon file: {}", "🎨".bright_yellow(), ico_path.display().to_string().bright_white());
            let _ = fs::copy(&ico_path, &icon_dest);
        }
        Some(IconCandidate::Png(png_path)) => {
            println!("{} Converting PNG icon: {}", "🎨".bright_yellow(), png_path.display().to_string().bright_white());
            let _ = convert_png_to_ico(&png_path, &icon_dest);
        }
        None => {
            println!("{} No icon found, embedding default icon", "🎨".bright_cyan());
            let _ = generate_default_ico(&icon_dest);
        }
    }

    let manifest_file = temp_build.join("app.manifest");
    let manifest_content = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity version="1.0.0.0" processorArchitecture="*" name="Ship.App" type="win32"/>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2, PerMonitor</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#;
    fs::write(&manifest_file, manifest_content)?;

    let rc_file = temp_build.join("resource.rc");
    let rc_content = format!(
        r#"1 ICON "icon.ico"
1 24 "app.manifest"

1 VERSIONINFO
FILEVERSION 1,0,0,0
PRODUCTVERSION 1,0,0,0
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
            VALUE "CompanyName", "Ship"
            VALUE "FileDescription", "Glad tidings to the strangers"
            VALUE "FileVersion", "1.0.0.0"
            VALUE "InternalName", "{main_exe_name}"
            VALUE "LegalCopyright", "Copyright (c) 2026"
            VALUE "OriginalFilename", "{main_exe_name}.exe"
            VALUE "ProductName", "{main_exe_name}"
            VALUE "ProductVersion", "1.0.0.0"
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
    let windres_status = Command::new("x86_64-w64-mingw32-windres")
        .current_dir(&temp_build)
        .arg("resource.rc")
        .arg("-O")
        .arg("coff")
        .arg("-o")
        .arg("resource.o")
        .status()
        .context("Failed to invoke windres for icon, manifest, and version resources")?;

    if !windres_status.success() {
        return Err(anyhow!("windres failed to compile resource file"));
    }

    if let Some(parent) = output_exe.parent() {
        fs::create_dir_all(parent)?;
    }

    let status = Command::new("x86_64-w64-mingw32-gcc")
        .arg("-O2")
        .arg("-mwindows")
        .arg(&asm_file)
        .arg(&c_file)
        .arg(&res_file)
        .arg("-o")
        .arg(output_exe)
        .status()
        .context("Failed to invoke MinGW compiler for standalone stub")?;

    let _ = fs::remove_dir_all(&temp_build);

    if !status.success() {
        return Err(anyhow!("Failed to build standalone executable"));
    }

    Ok(())
}