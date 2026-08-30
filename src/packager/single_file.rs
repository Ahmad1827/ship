use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn create_payload_zip(staging_dir: &Path, zip_path: &Path) -> Result<()> {
    let file = File::create(zip_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for entry in WalkDir::new(staging_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path == zip_path {
            continue;
        }

        let name = path.strip_prefix(staging_dir)?;
        let name_str = name.to_str().context("Invalid unicode in path")?;

        if path.is_file() {
            zip.start_file(name_str, options)?;
            let mut f = File::open(path)?;
            io::copy(&mut f, &mut zip)?;
        } else if !name_str.is_empty() {
            zip.add_directory(name_str, options)?;
        }
    }
    zip.finish()?;
    Ok(())
}

pub fn build_standalone_exe<P: AsRef<Path>>(
    staging_dir: &Path,
    output_exe: &Path,
    _main_bin: P,
) -> Result<PathBuf> {
    let zip_payload_path = staging_dir.join("payload.zip");
    create_payload_zip(staging_dir, &zip_payload_path)?;

    let mut zip_bytes = Vec::new();
    File::open(&zip_payload_path)?.read_to_end(&mut zip_bytes)?;
    let _ = fs::remove_file(&zip_payload_path);

    let temp_stub_c = staging_dir.join("stub.c");
    let temp_res_rc = staging_dir.join("resource.rc");
    let temp_res_o = staging_dir.join("resource.o");
    let temp_bin_dat = staging_dir.join("payload.dat");

    File::create(&temp_bin_dat)?.write_all(&zip_bytes)?;

    let rc_content = "101 RCDATA \"payload.dat\"\n";
    fs::write(&temp_res_rc, rc_content)?;

    let stub_c_code = r#"
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <shellapi.h>
#include <stdio.h>
#include <stdlib.h>

int WINAPI WinMain(HINSTANCE hInstance, HINSTANCE hPrevInstance, LPSTR lpCmdLine, int nCmdShow) {
    char tempPath[MAX_PATH];
    char extractDir[MAX_PATH];
    char targetExe[MAX_PATH];
    
    GetTempPathA(MAX_PATH, tempPath);
    
    char exePath[MAX_PATH];
    GetModuleFileNameA(NULL, exePath, MAX_PATH);
    char* baseName = strrchr(exePath, '\\');
    baseName = baseName ? baseName + 1 : exePath;

    snprintf(extractDir, MAX_PATH, "%sShipApp_%s", tempPath, baseName);
    CreateDirectoryA(extractDir, NULL);

    HRSRC hRes = FindResourceA(NULL, MAKEINTRESOURCE(101), RT_RCDATA);
    if (hRes) {
        HGLOBAL hData = LoadResource(NULL, hRes);
        DWORD size = SizeofResource(NULL, hRes);
        void* pData = LockResource(hData);
        
        char zipPath[MAX_PATH];
        snprintf(zipPath, MAX_PATH, "%s\\payload.zip", extractDir);
        FILE* f = fopen(zipPath, "wb");
        if (f) {
            fwrite(pData, 1, size, f);
            fclose(f);
            
            char cmd[MAX_PATH * 3];
            snprintf(cmd, sizeof(cmd), "powershell -WindowStyle Hidden -NoProfile -Command \"Expand-Archive -Path '%s' -DestinationPath '%s' -Force\"", zipPath, extractDir);
            
            STARTUPINFOA si;
            PROCESS_INFORMATION pi;
            ZeroMemory(&si, sizeof(si));
            si.cb = sizeof(si);
            si.dwFlags = STARTF_USESHOWWINDOW;
            si.wShowWindow = SW_HIDE;
            ZeroMemory(&pi, sizeof(pi));
            
            if (CreateProcessA(NULL, cmd, NULL, NULL, FALSE, CREATE_NO_WINDOW, NULL, NULL, &si, &pi)) {
                WaitForSingleObject(pi.hProcess, INFINITE);
                CloseHandle(pi.hProcess);
                CloseHandle(pi.hThread);
            }
            DeleteFileA(zipPath);
        }
    }

    WIN32_FIND_DATAA fd;
    char searchMask[MAX_PATH];
    snprintf(searchMask, MAX_PATH, "%s\\*.exe", extractDir);
    HANDLE hFind = FindFirstFileA(searchMask, &fd);
    
    if (hFind != INVALID_HANDLE_VALUE) {
        do {
            if (_stricmp(fd.cFileName, baseName) != 0) {
                snprintf(targetExe, MAX_PATH, "%s\\%s", extractDir, fd.cFileName);
                break;
            }
        } while (FindNextFileA(hFind, &fd));
        FindClose(hFind);

        STARTUPINFOA si;
        PROCESS_INFORMATION pi;
        ZeroMemory(&si, sizeof(si));
        si.cb = sizeof(si);
        ZeroMemory(&pi, sizeof(pi));

        if (CreateProcessA(NULL, targetExe, NULL, NULL, FALSE, 0, NULL, extractDir, &si, &pi)) {
            WaitForSingleObject(pi.hProcess, INFINITE);
            CloseHandle(pi.hProcess);
            CloseHandle(pi.hThread);
        }
    }

    return 0;
}
"#;

    fs::write(&temp_stub_c, stub_c_code)?;

    let windres_status = Command::new("x86_64-w64-mingw32-windres")
        .current_dir(staging_dir)
        .arg("resource.rc")
        .arg("-O")
        .arg("coff")
        .arg("-o")
        .arg("resource.o")
        .status()
        .context("Failed to run windres")?;

    if !windres_status.success() {
        anyhow::bail!("windres failed to compile payload resource");
    }

    if let Some(parent) = output_exe.parent() {
        fs::create_dir_all(parent)?;
    }

    let gcc_status = Command::new("x86_64-w64-mingw32-gcc")
        .current_dir(staging_dir)
        .arg("-O2")
        .arg("-mwindows")
        .arg("-o")
        .arg(output_exe)
        .arg("stub.c")
        .arg("resource.o")
        .status()
        .context("Failed to run MinGW GCC for standalone stub")?;

    let _ = fs::remove_file(temp_stub_c);
    let _ = fs::remove_file(temp_res_rc);
    let _ = fs::remove_file(temp_res_o);
    let _ = fs::remove_file(temp_bin_dat);

    if !gcc_status.success() {
        anyhow::bail!("Failed to link standalone executable");
    }

    Ok(output_exe.to_path_buf())
}