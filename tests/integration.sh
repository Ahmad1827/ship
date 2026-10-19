#!/usr/bin/env bash
# End-to-end checks: package small fixture projects with the real `ship` binary and
# verify the outputs. Needs gcc, g++, zlib headers and mingw-w64 (for the Windows cases).
#
#   tests/integration.sh [path/to/ship]     (default: builds target/release/ship)
#
# Set SHIP_OUT_DIR to keep the Windows test executables (CI runs them on a Windows runner).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SHIP="${1:-}"
if [[ -z "$SHIP" ]]; then
    cargo build --release --manifest-path "$ROOT/Cargo.toml"
    SHIP="$ROOT/target/release/ship"
fi
SHIP="$(realpath "$SHIP")"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
export XDG_CACHE_HOME="$WORK/cache"

pass=0
check() {   # check <description> <command...>
    local desc="$1"; shift
    if "$@"; then
        echo "  ok   $desc"; pass=$((pass + 1))
    else
        echo "  FAIL $desc" >&2; exit 1
    fi
}
contains() { grep -qF -- "$2" <<<"$1"; }

# --- fixture: C++ app that links zlib, reads an asset and echoes its args -------------
APP="$WORK/zapp"
mkdir -p "$APP/assets/levels" "$APP/assets/raw" "$APP/build/stale" "$APP/sprites/target_reticle"
echo "level one" > "$APP/assets/levels/1.txt"
echo "huge" > "$APP/assets/raw/big.psd"
echo "x" > "$APP/sprites/target_reticle/r.txt"
echo 'int main(){return 1;}' > "$APP/build/stale/old.cpp"
cat > "$APP/main.cpp" <<'EOF'
#include <zlib.h>
#include <cstdio>
#include <fstream>
#include <string>
int main(int argc, char** argv) {
    std::ifstream f("assets/levels/1.txt");
    std::string line;
    std::getline(f, line);
    std::printf("zlib=%s level=[%s] args=%d", zlibVersion(), line.c_str(), argc - 1);
    for (int i = 1; i < argc; i++) std::printf(" [%s]", argv[i]);
    std::printf("\n");
    return 0;
}
EOF
cat > "$APP/ship.toml" <<'EOF'
name = "ZApp"
target = "linux"
link = ["z"]
exclude = ["assets/raw"]
EOF

echo "linux zip"
(cd "$APP" && "$SHIP" >/dev/null)
listing="$(unzip -Z1 "$APP/dist/ZApp.zip")"
check "binary packaged"                   contains "$listing" "zapp"
check "libz resolved via ldconfig"        contains "$listing" "libz.so.1"
check "nested asset kept"                 contains "$listing" "assets/levels/1.txt"
check "'target_' folder not pruned"       contains "$listing" "sprites/target_reticle/r.txt"
check "excluded path removed"             bash -c "! grep -q 'assets/raw' <<<\"\$0\"" "$listing"
check "nested asset dir not duplicated"   bash -c "! grep -q '^levels/' <<<\"\$0\"" "$listing"
check "no fallback fonts for non-text app" bash -c "! grep -q '\.ttf' <<<\"\$0\"" "$listing"

echo "linux standalone"
(cd "$APP" && "$SHIP" -s >/dev/null)
out="$(cd / && "$APP/dist/ZApp-standalone" "two words" x)"
check "runs from another cwd and finds assets" contains "$out" "level=[level one]"
check "forwards arguments"                     contains "$out" "args=2 [two words] [x]"
check "extracts into private cache dir"        test "$(stat -c %a "$XDG_CACHE_HOME/ship")" = 700
echo "level two" > "$APP/assets/levels/1.txt"
(cd "$APP" && "$SHIP" -s >/dev/null && ./dist/ZApp-standalone >/dev/null)
check "old extraction cleaned up"              test "$(ls "$XDG_CACHE_HOME/ship" | wc -l)" = 1

if command -v mksquashfs >/dev/null; then
    echo "appimage"
    (cd "$APP" && "$SHIP" --appimage >/dev/null)
    image="$APP/dist/ZApp-x86_64.AppImage"
    check "AppImage is executable"           test -x "$image"
    # CI runners may lack FUSE; the runtime can always run by extracting itself.
    out="$(cd / && "$image" --appimage-extract-and-run "two words")"
    check "AppImage runs and finds assets"   contains "$out" "level=[level two]"
    check "AppImage forwards arguments"      contains "$out" "args=1 [two words]"
    listing="$(cd "$WORK" && "$image" --appimage-extract >/dev/null && find squashfs-root -printf '%P\n')"
    check "AppImage has desktop entry"       contains "$listing" "ZApp.desktop"
    check "AppImage has icon"                contains "$listing" ".DirIcon"
    check "AppImage bundles libz"            contains "$listing" "app/libz.so.1"
    if command -v desktop-file-validate >/dev/null; then
        check "desktop entry validates"      desktop-file-validate "$WORK/squashfs-root/ZApp.desktop"
    fi
    rm -rf "$WORK/squashfs-root"
else
    echo "appimage: skipped (squashfs-tools not installed)"
fi

echo "dry run / strict"
mkdir -p "$WORK/gone/lib"
echo 'int f(void){return 0;}' > "$WORK/gone/f.c"
echo 'int f(void); int main(void){return f();}' > "$WORK/gone/m.c"
gcc -shared -fPIC -o "$WORK/gone/lib/libgone.so" "$WORK/gone/f.c"
gcc "$WORK/gone/m.c" -L"$WORK/gone/lib" -lgone -o "$WORK/gone/app"
rm -rf "$WORK/gone/lib"
dry="$(cd "$WORK/gone" && "$SHIP" -t linux --dry-run ./app)"
check "dry run reports missing lib"     contains "$dry" "libgone.so"
check "dry run creates no package"      test ! -e "$WORK/gone/dist/app-linux.zip"
check "--strict fails on missing lib"   bash -c "! (cd '$WORK/gone' && '$SHIP' -t linux --strict ./app >/dev/null 2>&1)"

if command -v x86_64-w64-mingw32-gcc >/dev/null; then
    echo "windows"
    WIN="$WORK/winapp"
    mkdir -p "$WIN/assets"
    echo "hi from asset" > "$WIN/assets/msg.txt"
    cat > "$WIN/main.cpp" <<'EOF'
#include <cstdio>
#include <fstream>
#include <string>
int main(int argc, char** argv) {
    std::ifstream f("assets/msg.txt");
    std::string line;
    std::getline(f, line);
    std::printf("asset=[%s] args=%d", line.c_str(), argc - 1);
    for (int i = 1; i < argc; i++) std::printf(" [%s]", argv[i]);
    std::printf("\n");
    return 0;
}
EOF
    cat > "$WIN/ship.toml" <<'EOF'
name = "WinApp"
version = "2.3.1"
company = "Ship CI"
console = true
EOF
    (cd "$WIN" && "$SHIP" -t windows --all >/dev/null)
    listing="$(unzip -Z1 "$WIN/dist/WinApp.zip")"
    check "mingw runtime DLLs bundled"  contains "$listing" "libstdc++-6.dll"
    pe="$(x86_64-w64-mingw32-objdump -p "$WIN/dist/WinApp-standalone.exe")"
    check "console subsystem"           contains "$pe" "(Windows CUI)"
    strs="$(strings -el "$WIN/dist/WinApp-standalone.exe")"
    check "version info embedded"       contains "$strs" "2.3.1.0"
    check "product name from ship.toml" contains "$strs" "WinApp"
    if [[ -n "${SHIP_OUT_DIR:-}" ]]; then
        mkdir -p "$SHIP_OUT_DIR"
        cp "$WIN/dist/WinApp-standalone.exe" "$SHIP_OUT_DIR/"
    fi
else
    echo "windows: skipped (mingw-w64 not installed)"
fi

echo "all $pass checks passed"
