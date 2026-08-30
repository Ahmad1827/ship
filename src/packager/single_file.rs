use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

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

    if let Some(parent) = output_exe.parent() {
        fs::create_dir_all(parent)?;
    }

    let status = Command::new("x86_64-w64-mingw32-gcc")
        .arg("-O2")
        .arg("-mwindows")
        .arg(&asm_file)
        .arg(&c_file)
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