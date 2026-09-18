use anyhow::{anyhow, Context, Result};
use colored::*;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

fn find_icon_file(staging_dir: &Path) -> Option<PathBuf> {
    for entry in WalkDir::new(staging_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                if ext.eq_ignore_ascii_case("ico") {
                    return Some(entry.path().to_path_buf());
                }
            }
        }
    }

    if let Ok(cur) = env::current_dir() {
        for name in &["wisdomParkicon.ico", "app.ico", "icon.ico"] {
            let p = cur.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
        for entry in WalkDir::new(&cur).max_depth(3).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                    if ext.eq_ignore_ascii_case("ico") {
                        return Some(entry.path().to_path_buf());
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
    if let Some(icon_path) = find_icon_file(staging_dir) {
        println!("{} Found icon file: {}", "🎨".bright_yellow(), icon_path.display().to_string().bright_white());
        let icon_dest = temp_build.join("icon.ico");
        if fs::copy(&icon_path, &icon_dest).is_ok() {
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
                        println!("{} Embedded crown icon into standalone executable", "✔".green());
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
    gcc_cmd.arg("-O2")
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