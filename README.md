# ship

> Zero-config cross-platform build pipeline, dynamic dependency harvester, and standalone single-file packager for native tools and indie games.

[![Rust](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20WSL%20%7C%20Windows-brightgreen.svg)]()

---

## Overview

When developing games or native applications in C++, Rust, SFML, SDL2, or Raylib, running a project inside an IDE works seamlessly. Distributing the final binary, however, typically leads to runtime failures:
* Missing runtime DLLs (`libgcc_s_seh-1.dll`, `libstdc++-6.dll`, `sfml-graphics-2.dll`).
* Missing or relative asset path breakage (`Resources/`, `assets/`, font fallbacks).
* Complex manual cross-compilation setups between Linux/WSL and Windows.

`ship` automates the entire process into a single command. It detects the build system, compiles the project for the target OS, inspects the binary's import tables, pulls every non-system dynamic library and asset directory, and packages everything into a distribution-ready archive or a single standalone executable.

---

## Features

* **Zero-Config Auto-Build**: Auto-detects and builds Cargo, CMake, Make, and single/multi-file C++ projects without modifying source trees.
* **Autonomous Windows Cross-Compilation**: Cross-compiles for Windows directly from Linux/WSL using MinGW toolchains.
* **Recursive Dependency Harvester**: Parses PE (`.exe`) and ELF import tables using `goblin` to detect and collect all third-party and toolchain DLLs.
* **Smart Asset & Font Sniffer**: Automatically discovers resource folders (`assets/`, `Resources/`, `sprites/`, `data/`) and validates static TrueType font dependencies.
* **Automated SDK Fetcher**: Downloads and caches required external Windows SDKs (such as SFML MinGW) directly in `~/.ship/`, verified against pinned SHA-256 checksums.
* **AppImage Output (`--appimage`)**: Packages Linux builds as a single `.AppImage` with desktop entry and icon.
* **Single-File Standalone Mode (`-s`)**: Embeds the app, its libraries and assets into one executable (Windows `.exe` or Linux binary). On first launch it extracts to a per-user cache and reuses it on later launches until the app changes.
* **Project Config (`ship.toml`)**: Optional per-project settings for name, version info, icon, extra assets, excludes, link libraries and console mode.
* **Dry Run & Strict Mode**: Preview exactly what would be packaged, or fail the build when a library is missing.
* **Clean Staging Isolation**: Builds in dedicated temporary directories (`build-ship-*`), leaving local build environments untouched.

---

## Installation

### Prerequisites (Linux / WSL)

```bash
sudo apt update
sudo apt install -y mingw-w64 curl build-essential fonts-dejavu-core squashfs-tools
rustup target add x86_64-pc-windows-gnu
```

### Building and Installing `ship`

```bash
git clone [https://github.com/Ahmad1827/ship.git](https://github.com/Ahmad1827/ship.git)
cd ship
cargo install --path .
```

---

## Usage

Navigate to any project root and run:

### 1. Default Portable ZIP Package
```bash
ship
```
Generates a portable `.zip` in `dist/` containing the compiled binary, all resolved DLLs, and associated assets.

### 2. Standalone Single-File Executable
```bash
ship -s
```
Generates a single self-contained `.exe` in `dist/` that runs directly without requiring archive extraction.

### 3. Generate Both ZIP and Standalone Executable
```bash
ship --all
```

### 4. Target Linux Natively
```bash
ship -t linux
```

### 5. Package a Pre-Compiled Binary Directly
```bash
ship ./path/to/game.exe
```

### 6. Linux AppImage
```bash
ship -t linux --appimage          # zip + .AppImage
ship -t linux --appimage -s       # standalone + .AppImage
```
Builds a `.AppImage` from the pinned official AppImage runtime (downloaded once, SHA-256 verified) and `mksquashfs`. The app runs with its bundled libraries and its own folder as working directory, like the standalone launcher.

### 7. Preview Without Packaging
```bash
ship --dry-run            # lists libraries (found / missing / OS-provided), assets and fonts
ship --dry-run --strict   # same, but exits with an error if a library is missing (handy in CI)
```

---

## Project Config (`ship.toml`)

Run `ship --init` to write a commented template. Every setting is optional, and command-line flags always override the file. Unknown keys are rejected, so typos don't get silently ignored.

```toml
name = "MyGame"
target = "windows"            # or "linux"
version = "1.2.0"             # Windows file/product version
description = "My awesome game"
company = "Me"
icon = "assets/icon.png"      # .ico or .png for the standalone .exe

assets = ["levels", "config.json"]    # extra folders/files beyond auto-detection
exclude = ["assets/raw", "notes.txt"] # package-relative paths to leave out
search_paths = ["third_party/bin"]    # extra DLL / .so search paths

console = false   # true for terminal apps: keeps stdout/stdin attached on Windows
fonts = true      # force (true) or disable (false) fallback fonts; default is automatic

# Raw C/C++ projects only
link = ["z", "sfml-graphics", "sfml-window", "sfml-system"]
cflags = ["-std=c++20", "-Iinclude"]
```

---

## CLI Reference

| Flag | Short | Description | Default |
| :--- | :--- | :--- | :--- |
| `[binary]` | | Optional path to an existing binary (skips build step) | Auto-detect |
| `--target` | `-t` | Target platform (`windows` or `linux`) | `windows` |
| `--single` | `-s` | Bundle into a single standalone executable | `false` |
| `--all` | | Generate both `.zip` and standalone executable | `false` |
| `--appimage` | | Also build a Linux `.AppImage` (needs `mksquashfs`) | `false` |
| `--output` | `-o` | Output directory | `dist` |
| `--name` | `-n` | Custom output base name | Project Name |
| `--extra-assets` | `-a` | Explicit additional asset folders or files | None |
| `--search-paths` | | Additional custom DLL/SO library search paths | None |
| `--console` | | Build a console (terminal) app on Windows instead of a GUI app | `false` |
| `--dry-run` | | Show what would be packaged without creating a package | `false` |
| `--strict` | | Fail if any required library cannot be found | `false` |
| `--init` | | Write a commented `ship.toml` template | |

Set `SHIP_DEBUG=1` when running a Windows standalone executable to log launcher activity to `%TEMP%\ship_debug.log`.

---

## Supported Build Systems

`ship` automatically discovers and orchestrates the following project structures:

* **Rust / Cargo**: `Cargo.toml`
* **CMake**: `CMakeLists.txt`
* **Makefiles**: `Makefile`
* **Raw C/C++**: Direct compilation of `.cpp`, `.c`, `.cc` source trees

---

## Architecture

```
Project Directory
       │
       ▼
 1. Builder Engine (Detects Cargo / CMake / Make -> Builds Target Binary)
       │
       ▼
 2. Binary Inspector (Parses PE/ELF Headers via Goblin)
       │
       ▼
 3. Dependency Resolver (Scans Toolchains, MinGW & ~/.ship/ SDKs)
       │
       ▼
 4. Asset Collector (Collects Assets, Resources, Fonts & Configs)
       │
       ▼
 5. Packaging Engine ──► Staging Directory
                              │
                              ├──► Portable .zip Archive
                              └──► Standalone executable (Embedded Stub Loader)
```

---

## Development

```bash
cargo test                 # unit tests
tests/integration.sh       # end-to-end: packages fixture projects and checks the output
```

CI runs both on Linux, then runs the cross-built Windows standalone on a real Windows runner.

---

## License

Distributed under the MIT License.