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

    let mut res_obj = None;
    if let Some(candidate) = find_icon_candidate(staging_dir) {
        let icon_dest = temp_build.join("icon.ico");
        let prepared = match candidate {
            IconCandidate::Ico(ico_path) => {
                println!("{} Found icon file: {}", "🎨".bright_yellow(), ico_path.display().to_string().bright_white());
                fs::copy(&ico_path, &icon_dest).is_ok()
            }
            IconCandidate::Png(png_path) => {
                println!("{} Converting PNG icon: {}", "🎨".bright_yellow(), png_path.display().to_string().bright_white());
                convert_png_to_ico(&png_path, &icon_dest).is_ok()
            }
        };

        if prepared {
            let rc_file = temp_build.join("icon.rc");
            let rc_content = "1 ICON \"icon.ico\"\n";
            if fs::write(&rc_file, rc_content).is_ok() {
                let res_file = temp_build.join("icon.o");
                let windres_status = Command::new("x86_64-w64-mingw32-windres")
                    .current_dir(&temp_build)
                    .arg("icon.rc")
                    .arg("-O")
                    .arg("coff")
                    .arg("-o")
                    .arg("icon.o")
                    .status();

                if let Ok(st) = windres_status {
                    if st.success() && res_file.exists() {
                        println!("{} Embedded icon into standalone executable", "✔".green());
                        res_obj = Some(res_file);
                    }
                }
            }
        }
    }

    if let Some(parent) = output_exe.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut gcc_cmd = Command::new("x86_64-w64-mingw32-gcc");
    gcc_cmd
        .arg("-O2")
        .arg("-mwindows")
        .arg(&asm_file)
        .arg(&c_file);

    if let Some(ref res) = res_obj {
        gcc_cmd.arg(res);
    }

    let status = gcc_cmd
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