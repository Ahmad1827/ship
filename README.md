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
* **Automated SDK Fetcher**: Downloads and caches required external Windows SDKs (such as SFML MinGW) directly in `~/.ship/`.
* **Single-File Standalone Mode (`-s`)**: Embeds all DLLs and assets into a single `.exe` executable via an in-memory stub loader for zero-install distribution.
* **Clean Staging Isolation**: Builds in dedicated temporary directories (`build-ship-*`), leaving local build environments untouched.

---

## Installation

### Prerequisites (Linux / WSL)

```bash
sudo apt update
sudo apt install -y mingw-w64 curl build-essential fonts-dejavu-core
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

---

## CLI Reference

| Flag | Short | Description | Default |
| :--- | :--- | :--- | :--- |
| `[binary]` | | Optional path to an existing binary (skips build step) | Auto-detect |
| `--target` | `-t` | Target platform (`windows` or `linux`) | `windows` |
| `--single` | `-s` | Bundle into a single standalone `.exe` | `false` |
| `--all` | | Generate both `.zip` and standalone `.exe` | `false` |
| `--output` | `-o` | Output directory | `dist` |
| `--name` | `-n` | Custom output base name | Project Name |
| `--extra-assets` | `-a` | Explicit additional asset folders or files | None |
| `--search-paths` | | Additional custom DLL/SO library search paths | None |

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
                              └──► Standalone .exe (Embedded Stub Loader)
```

---

## License

Distributed under the MIT License.