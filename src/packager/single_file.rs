use anyhow::{anyhow, Context, Result};
use colored::*;
use std::env;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

use super::archive::ArchiveBuilder;

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

const DECOMPRESSOR_C_SOURCE: &str = r#"
#include <stddef.h>
#include <string.h>

int ship_lz4_decompress(const unsigned char *src, size_t src_len, unsigned char *dest, size_t dest_len) {
    size_t src_pos = 0;
    size_t dest_pos = 0;

    while (src_pos < src_len && dest_pos < dest_len) {
        unsigned char token = src[src_pos++];
        size_t lit_len = token >> 4;

        if (lit_len == 15) {
            unsigned char s = 0;
            do {
                if (src_pos >= src_len) return -1;
                s = src[src_pos++];
                lit_len += s;
            } while (s == 255);
        }

        if (src_pos + lit_len > src_len || dest_pos + lit_len > dest_len) return -2;
        memcpy(dest + dest_pos, src + src_pos, lit_len);
        src_pos += lit_len;
        dest_pos += lit_len;

        if (dest_pos >= dest_len) break;
        if (src_pos + 2 > src_len) return -3;

        unsigned short offset = (unsigned short)(src[src_pos] | (src[src_pos + 1] << 8));
        src_pos += 2;
        if (offset == 0 || (size_t)offset > dest_pos) return -4;

        size_t match_len = (token & 0x0F) + 4;
        if (match_len == 19) {
            unsigned char s = 0;
            do {
                if (src_pos >= src_len) return -5;
                s = src[src_pos++];
                match_len += s;
            } while (s == 255);
        }

        if (dest_pos + match_len > dest_len) return -6;
        for (size_t i = 0; i < match_len; i++) {
            dest[dest_pos] = dest[dest_pos - offset];
            dest_pos++;
        }
    }

    return (dest_pos == dest_len) ? 0 : -7;
}
"#;

pub fn build_linux_standalone_exe(
    staging_dir: &Path,
    output_exe: &Path,
    main_exe_name: &str,
) -> Result<()> {
    let mut archive_builder = ArchiveBuilder::new();

    for entry in WalkDir::new(staging_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let path = entry.path();
            let rel = path.strip_prefix(staging_dir)?.to_string_lossy().replace('\\', "/");
            archive_builder.add_file(path, &rel)?;
        }
    }

    let archive_bytes = archive_builder.build()?;

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    archive_bytes.hash(&mut hasher);
    let build_hash = format!("{:016x}", hasher.finish());

    let temp_build = staging_dir.parent().unwrap_or_else(|| Path::new(".")).join("ship_linux_build");
    fs::create_dir_all(&temp_build)?;

    let archive_bin_path = temp_build.join("payload.bin");
    fs::write(&archive_bin_path, &archive_bytes)?;

    let asm_file = temp_build.join("payload.s");
    let asm_content = format!(
        ".section .rodata\n\
         .global ship_payload_start\n\
         .global ship_payload_end\n\
         ship_payload_start:\n\
             .incbin \"{}\"\n\
         ship_payload_end:\n",
        archive_bin_path.display()
    );
    fs::write(&asm_file, asm_content)?;

    let lz4_file = temp_build.join("lz4_stub.c");
    fs::write(&lz4_file, DECOMPRESSOR_C_SOURCE)?;

    let c_file = temp_build.join("stub.c");
    let c_content = format!(
        r#"#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

#define SHIP_BUILD_HASH "{build_hash}"

extern const unsigned char ship_payload_start[];
extern const unsigned char ship_payload_end[];
extern int ship_lz4_decompress(const unsigned char *src, size_t src_len, unsigned char *dest, size_t dest_len);

static unsigned int calc_crc32(const unsigned char *data, size_t len) {{
    unsigned int crc = 0xFFFFFFFF;
    for (size_t i = 0; i < len; i++) {{
        crc ^= (unsigned int)data[i];
        for (int j = 0; j < 8; j++) {{
            unsigned int mask = -(crc & 1);
            crc = (crc >> 1) ^ (0xEDB88320U & mask);
        }}
    }}
    return ~crc;
}}

static void create_parent_dirs(char* path) {{
    for (char* p = path + 1; *p; p++) {{
        if (*p == '/') {{
            *p = '\0';
            mkdir(path, 0755);
            *p = '/';
        }}
    }}
}}

int main(int argc, char **argv) {{
    const char *tmp_root = getenv("TMPDIR");
    if (!tmp_root || !*tmp_root) {{
        tmp_root = "/tmp";
    }}

    char app_dir[1024];
    snprintf(app_dir, sizeof(app_dir), "%s/ShipApp_%s", tmp_root, "{main_exe_name}");
    mkdir(app_dir, 0755);

    char hash_path[1024];
    snprintf(hash_path, sizeof(hash_path), "%s/.ship_hash", app_dir);

    char exe_path[1024];
    snprintf(exe_path, sizeof(exe_path), "%s/%s", app_dir, "{main_exe_name}");

    int need_extract = 1;
    FILE* hf = fopen(hash_path, "r");
    if (hf) {{
        char stored_hash[64] = {{0}};
        if (fgets(stored_hash, sizeof(stored_hash), hf)) {{
            char* nl = strchr(stored_hash, '\n');
            if (nl) *nl = '\0';
            char* cr = strchr(stored_hash, '\r');
            if (cr) *cr = '\0';
            if (strcmp(stored_hash, SHIP_BUILD_HASH) == 0 && access(exe_path, X_OK) == 0) {{
                need_extract = 0;
            }}
        }}
        fclose(hf);
    }}

    if (need_extract) {{
        const unsigned char* p = ship_payload_start;
        const unsigned char* end_ptr = ship_payload_end;

        if ((size_t)(end_ptr - p) < 12 || memcmp(p, "SHIP", 4) != 0) {{
            fprintf(stderr, "Error: Corrupted archive header\n");
            return 1;
        }}

        p += 8;
        unsigned int file_count = 0;
        memcpy(&file_count, p, 4);
        p += 4;

        const unsigned char* cur = p;
        for (unsigned int i = 0; i < file_count; i++) {{
            unsigned short plen = 0;
            memcpy(&plen, cur, 2);
            cur += 2 + plen + 8 + 8 + 8 + 4;
        }}
        const unsigned char* data_start = cur;

        cur = p;
        for (unsigned int i = 0; i < file_count; i++) {{
            unsigned short path_len = 0;
            memcpy(&path_len, cur, 2);
            cur += 2;

            char rel_path[1024] = {{0}};
            if (path_len < sizeof(rel_path)) {{
                memcpy(rel_path, cur, path_len);
                rel_path[path_len] = '\0';
            }}
            cur += path_len;

            unsigned long long uncomp_sz = 0;
            memcpy(&uncomp_sz, cur, 8);
            cur += 8;

            unsigned long long comp_sz = 0;
            memcpy(&comp_sz, cur, 8);
            cur += 8;

            unsigned long long offset = 0;
            memcpy(&offset, cur, 8);
            cur += 8;

            unsigned int expected_crc = 0;
            memcpy(&expected_crc, cur, 4);
            cur += 4;

            const unsigned char* comp_bytes = data_start + offset;
            unsigned char* uncomp_bytes = (unsigned char*)malloc((size_t)uncomp_sz);
            if (!uncomp_bytes && uncomp_sz > 0) {{
                fprintf(stderr, "Error: Memory allocation failed\n");
                return 2;
            }}

            if (uncomp_sz > 0) {{
                int err = ship_lz4_decompress(comp_bytes, (size_t)comp_sz, uncomp_bytes, (size_t)uncomp_sz);
                if (err != 0) {{
                    fprintf(stderr, "Error: Failed decompressing %s\n", rel_path);
                    free(uncomp_bytes);
                    return 3;
                }}
                if (calc_crc32(uncomp_bytes, (size_t)uncomp_sz) != expected_crc) {{
                    fprintf(stderr, "Error: Checksum mismatch for %s\n", rel_path);
                    free(uncomp_bytes);
                    return 4;
                }}
            }}

            char out_path[1024];
            snprintf(out_path, sizeof(out_path), "%s/%s", app_dir, rel_path);
            create_parent_dirs(out_path);

            FILE* out_f = fopen(out_path, "wb");
            if (out_f) {{
                if (uncomp_sz > 0) {{
                    fwrite(uncomp_bytes, 1, (size_t)uncomp_sz, out_f);
                }}
                fclose(out_f);
                chmod(out_path, 0755);
            }}

            if (uncomp_bytes) free(uncomp_bytes);
        }}

        FILE* out_hf = fopen(hash_path, "w");
        if (out_hf) {{
            fputs(SHIP_BUILD_HASH, out_hf);
            fclose(out_hf);
        }}
    }}

    char ld_env[2048];
    const char *orig_ld = getenv("LD_LIBRARY_PATH");
    if (orig_ld && *orig_ld) {{
        snprintf(ld_env, sizeof(ld_env), "%s:%s", app_dir, orig_ld);
    }} else {{
        snprintf(ld_env, sizeof(ld_env), "%s", app_dir);
    }}
    setenv("LD_LIBRARY_PATH", ld_env, 1);

    argv[0] = exe_path;
    execv(exe_path, argv);

    perror("execv failed");
    return 1;
}}
"#
    );
    fs::write(&c_file, c_content)?;

    if let Some(parent) = output_exe.parent() {
        fs::create_dir_all(parent)?;
    }

    let status = Command::new("gcc")
        .arg("-O2")
        .arg(&asm_file)
        .arg(&c_file)
        .arg(&lz4_file)
        .arg("-o")
        .arg(output_exe)
        .status()
        .context("Failed to invoke host gcc compiler for Linux standalone stub")?;

    let _ = fs::remove_dir_all(&temp_build);

    if !status.success() {
        return Err(anyhow!("Failed to compile Linux standalone executable"));
    }

    Ok(())
}

pub fn build_standalone_exe(staging_dir: &Path, output_exe: &Path, main_exe_name: &str) -> Result<()> {
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

    let mut archive_builder = ArchiveBuilder::new();

    for entry in WalkDir::new(staging_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let path = entry.path();
            let rel = path.strip_prefix(staging_dir)?.to_string_lossy().replace('\\', "/");
            archive_builder.add_file(path, &rel)?;
        }
    }

    let archive_bytes = archive_builder.build()?;

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    archive_bytes.hash(&mut hasher);
    let build_hash = format!("{:016x}", hasher.finish());

    let temp_build = staging_dir.parent().unwrap_or_else(|| Path::new(".")).join("ship_stub_build");
    fs::create_dir_all(&temp_build)?;

    let archive_bin_path = temp_build.join("payload.bin");
    fs::write(&archive_bin_path, &archive_bytes)?;

    let asm_file = temp_build.join("payload.s");
    let asm_content = format!(
        ".section .rdata,\"dr\"\n\
         .global ship_payload_start\n\
         .global ship_payload_end\n\
         ship_payload_start:\n\
             .incbin \"{}\"\n\
         ship_payload_end:\n",
        archive_bin_path.display()
    );
    fs::write(&asm_file, asm_content)?;

    let lz4_file = temp_build.join("lz4_stub.c");
    fs::write(&lz4_file, DECOMPRESSOR_C_SOURCE)?;

    let c_file = temp_build.join("stub.c");
    let c_content = format!(
        r#"#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define SHIP_BUILD_HASH "{build_hash}"

extern const unsigned char ship_payload_start[];
extern const unsigned char ship_payload_end[];
extern int ship_lz4_decompress(const unsigned char *src, size_t src_len, unsigned char *dest, size_t dest_len);

static void log_debug(const char* msg) {{
    char temp[MAX_PATH];
    GetTempPathA(MAX_PATH, temp);
    char log_path[MAX_PATH];
    snprintf(log_path, MAX_PATH, "%sship_debug.log", temp);
    FILE* f = fopen(log_path, "a");
    if (f) {{
        fputs(msg, f);
        fputc('\n', f);
        fclose(f);
    }}
}}

static unsigned int calc_crc32(const unsigned char *data, size_t len) {{
    unsigned int crc = 0xFFFFFFFF;
    for (size_t i = 0; i < len; i++) {{
        crc ^= (unsigned int)data[i];
        for (int j = 0; j < 8; j++) {{
            unsigned int mask = -(crc & 1);
            crc = (crc >> 1) ^ (0xEDB88320U & mask);
        }}
    }}
    return ~crc;
}}

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
    log_debug("=== Ship Stub Launch ===");

    char temp[MAX_PATH];
    GetTempPathA(MAX_PATH, temp);

    char app_dir[MAX_PATH];
    snprintf(app_dir, MAX_PATH, "%sShipApp_%s", temp, "{main_exe_name}");
    CreateDirectoryA(app_dir, NULL);

    char hash_path[MAX_PATH];
    snprintf(hash_path, MAX_PATH, "%s/.ship_hash", app_dir);

    char exe_path[MAX_PATH];
    snprintf(exe_path, MAX_PATH, "%s/%s", app_dir, "{target_bin_name}");

    int need_extract = 1;
    FILE* hf = fopen(hash_path, "r");
    if (hf) {{
        char stored_hash[64] = {{0}};
        if (fgets(stored_hash, sizeof(stored_hash), hf)) {{
            char* nl = strchr(stored_hash, '\n');
            if (nl) *nl = '\0';
            char* cr = strchr(stored_hash, '\r');
            if (cr) *cr = '\0';
            if (strcmp(stored_hash, SHIP_BUILD_HASH) == 0) {{
                DWORD attr = GetFileAttributesA(exe_path);
                if (attr != INVALID_FILE_ATTRIBUTES && !(attr & FILE_ATTRIBUTE_DIRECTORY)) {{
                    need_extract = 0;
                    log_debug("Payload hash match: skipping extraction");
                }}
            }}
        }}
        fclose(hf);
    }}

    if (need_extract) {{
        log_debug("Extracting payload...");
        const unsigned char* p = ship_payload_start;
        const unsigned char* end_ptr = ship_payload_end;

        if ((size_t)(end_ptr - p) < 12 || memcmp(p, "SHIP", 4) != 0) {{
            log_debug("ERROR: Corrupted header");
            MessageBoxA(NULL, "Corrupted binary package header.", "Ship Launch Error", MB_OK | MB_ICONERROR);
            return 1;
        }}

        p += 4;
        p += 2;
        p += 2;

        unsigned int file_count = 0;
        memcpy(&file_count, p, 4);
        p += 4;

        const unsigned char* cur = p;
        for (unsigned int i = 0; i < file_count; i++) {{
            unsigned short plen = 0;
            memcpy(&plen, cur, 2);
            cur += 2 + plen + 8 + 8 + 8 + 4;
        }}
        const unsigned char* data_start = cur;

        cur = p;
        for (unsigned int i = 0; i < file_count; i++) {{
            unsigned short path_len = 0;
            memcpy(&path_len, cur, 2);
            cur += 2;

            char rel_path[MAX_PATH] = {{0}};
            if (path_len < MAX_PATH) {{
                memcpy(rel_path, cur, path_len);
                rel_path[path_len] = '\0';
            }}
            cur += path_len;

            unsigned long long uncomp_sz = 0;
            memcpy(&uncomp_sz, cur, 8);
            cur += 8;

            unsigned long long comp_sz = 0;
            memcpy(&comp_sz, cur, 8);
            cur += 8;

            unsigned long long offset = 0;
            memcpy(&offset, cur, 8);
            cur += 8;

            unsigned int expected_crc = 0;
            memcpy(&expected_crc, cur, 4);
            cur += 4;

            const unsigned char* comp_bytes = data_start + offset;
            unsigned char* uncomp_bytes = (unsigned char*)malloc((size_t)uncomp_sz);
            if (!uncomp_bytes && uncomp_sz > 0) {{
                log_debug("ERROR: Memory allocation failed");
                MessageBoxA(NULL, "Memory allocation failed during decompression.", "Ship Error", MB_OK | MB_ICONERROR);
                return 2;
            }}

            if (uncomp_sz > 0) {{
                int err = ship_lz4_decompress(comp_bytes, (size_t)comp_sz, uncomp_bytes, (size_t)uncomp_sz);
                if (err != 0) {{
                    char msg[256];
                    snprintf(msg, sizeof(msg), "Failed decompressing '%s' (Error %d)", rel_path, err);
                    log_debug(msg);
                    MessageBoxA(NULL, msg, "Ship Decompression Error", MB_OK | MB_ICONERROR);
                    free(uncomp_bytes);
                    return 3;
                }}

                if (calc_crc32(uncomp_bytes, (size_t)uncomp_sz) != expected_crc) {{
                    char msg[256];
                    snprintf(msg, sizeof(msg), "CRC32 mismatch for '%s'", rel_path);
                    log_debug(msg);
                    MessageBoxA(NULL, msg, "Ship Integrity Error", MB_OK | MB_ICONERROR);
                    free(uncomp_bytes);
                    return 4;
                }}
            }}

            char out_path[MAX_PATH];
            snprintf(out_path, MAX_PATH, "%s/%s", app_dir, rel_path);
            create_parent_dirs(out_path);

            HANDLE hFile = CreateFileA(out_path, GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
            if (hFile != INVALID_HANDLE_VALUE) {{
                DWORD written = 0;
                if (uncomp_sz > 0) {{
                    WriteFile(hFile, uncomp_bytes, (DWORD)uncomp_sz, &written, NULL);
                }}
                CloseHandle(hFile);
            }}

            if (uncomp_bytes) {{
                free(uncomp_bytes);
            }}
        }}

        FILE* out_hf = fopen(hash_path, "w");
        if (out_hf) {{
            fputs(SHIP_BUILD_HASH, out_hf);
            fclose(out_hf);
        }}
        log_debug("Extraction complete.");
    }}

    char cmd_line[MAX_PATH * 2];
    snprintf(cmd_line, sizeof(cmd_line), "\"%s\"", exe_path);

    STARTUPINFOA si;
    PROCESS_INFORMATION pi;
    ZeroMemory(&si, sizeof(si));
    si.cb = sizeof(si);
    ZeroMemory(&pi, sizeof(pi));

    log_debug("Spawning child process...");
    if (CreateProcessA(exe_path, cmd_line, NULL, NULL, FALSE, 0, NULL, app_dir, &si, &pi)) {{
        WaitForSingleObject(pi.hProcess, INFINITE);
        DWORD code = 0;
        GetExitCodeProcess(pi.hProcess, &code);
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);

        char exit_msg[128];
        snprintf(exit_msg, sizeof(exit_msg), "Process finished with exit code: %lu", code);
        log_debug(exit_msg);

        if (code != 0) {{
            char msg[512];
            snprintf(msg, sizeof(msg),
                "Application exited with code: 0x%lX (%lu)\n\n"
                "If this is 0xC0000135, a required DLL is missing.\n"
                "Path: %s",
                code, code, exe_path);
            MessageBoxA(NULL, msg, "Ship Runtime Alert", MB_OK | MB_ICONERROR);
        }}
        return (int)code;
    }} else {{
        char msg[256];
        snprintf(msg, sizeof(msg), "Failed to launch process '%s' (Error %lu)", exe_path, GetLastError());
        log_debug(msg);
        MessageBoxA(NULL, msg, "Ship Execution Error", MB_OK | MB_ICONERROR);
    }}

    return 1;
}}
"#
    );
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

    let rc_file = temp_build.join("resource.rc");
    let rc_content = format!(
        r#"1 ICON "icon.ico"

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
        .context("Failed to invoke windres for icon and version resources")?;

    if !windres_status.success() {
        return Err(anyhow!("windres failed to compile resource file"));
    }

    if let Some(parent) = output_exe.parent() {
        fs::create_dir_all(parent)?;
    }

    let status = Command::new("x86_64-w64-mingw32-gcc")
        .arg("-O2")
        .arg("-static")
        .arg("-static-libgcc")
        .arg("-mwindows")
        .arg(&asm_file)
        .arg(&c_file)
        .arg(&lz4_file)
        .arg(&res_file)
        .arg("-luser32")
        .arg("-lkernel32")
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