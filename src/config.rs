use std::collections::HashSet;
use std::path::Path;

pub struct Config {
    system_dlls: HashSet<String>,
    system_sos: HashSet<String>,
    pub asset_dirs: Vec<&'static str>,
}

impl Config {
    pub fn new() -> Self {
        let dll_list = [
            "advapi32.dll", "cfgmgr32.dll", "comctl32.dll", "comdlg32.dll",
            "crypt32.dll", "d3d11.dll", "d3d9.dll", "dwmapi.dll", "dxgi.dll",
            "gdi32.dll", "gdiplus.dll", "imm32.dll", "kernel32.dll",
            "kernelbase.dll", "msvcrt.dll", "ntdll.dll", "ole32.dll",
            "oleaut32.dll", "opengl32.dll", "setupapi.dll", "shell32.dll",
            "shlwapi.dll", "ucrtbase.dll", "user32.dll", "uxtheme.dll",
            "version.dll", "win32u.dll", "winmm.dll", "ws2_32.dll", "wsock32.dll",
        ];

        // Matched on the name stem before ".so", so every SONAME version is covered.
        // These must come from the user's system: glibc, GPU drivers, display server and audio.
        let so_list = [
            "ld-linux-x86-64", "libc", "libm", "libdl", "libpthread", "librt", "libresolv",
            "libutil", "libnsl", "libanl", "libmvec", "libstdc++", "libgcc_s",
            "libGL", "libGLX", "libEGL", "libGLdispatch", "libOpenGL", "libGLESv2",
            "libvulkan", "libdrm", "libgbm",
            "libX11", "libX11-xcb", "libXext", "libXcursor", "libXinerama", "libXi",
            "libXrandr", "libXrender", "libXfixes", "libXss", "libXxf86vm", "libXau",
            "libXdmcp", "libXcomposite", "libXdamage", "libxkbcommon", "libxkbcommon-x11",
            "libasound", "libjack", "libdbus-1", "libudev", "libsystemd", "libfontconfig",
        ];

        Self {
            system_dlls: dll_list.iter().map(|s| s.to_ascii_lowercase()).collect(),
            system_sos: so_list.iter().map(|s| s.to_string()).collect(),
            asset_dirs: vec![
                "assets", "sprites", "textures", "images", "img",
                "res", "resources", "data", "audio", "music",
                "sounds", "shaders", "levels", "maps", "fonts",
            ],
        }
    }

    pub fn is_system_dll(&self, dll: &str) -> bool {
        let lower = dll.to_ascii_lowercase();
        self.system_dlls.contains(&lower) || lower.starts_with("api-ms-win-") || lower.starts_with("ext-ms-")
    }

    /// A DLL that resolved into a Windows system dir (e.g. `/mnt/c/Windows/System32` via a WSL
    /// PATH) is part of the OS even if it is missing from the name list above.
    pub fn is_windows_system_location(&self, path: &Path) -> bool {
        let lower = path.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
        ["/windows/system32/", "/windows/syswow64/", "/windows/winsxs/"]
            .iter()
            .any(|dir| lower.contains(dir))
    }

    pub fn is_system_so(&self, so: &str) -> bool {
        let stem = so.split(".so").next().unwrap_or(so);
        self.system_sos.contains(stem)
            || ["libxcb", "libwayland-", "libpulse", "libpipewire-"]
                .iter()
                .any(|prefix| stem.starts_with(prefix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_so_matching() {
        let config = Config::new();
        assert!(config.is_system_so("libc.so.6"));
        assert!(config.is_system_so("libX11.so.6"));
        assert!(config.is_system_so("libxcb-randr.so.0"));
        assert!(config.is_system_so("libpulse-simple.so.0"));
        assert!(!config.is_system_so("libSDL2-2.0.so.0"));
        assert!(!config.is_system_so("libGLEW.so.2.2"));
        assert!(!config.is_system_so("libsfml-graphics.so.2.6"));
    }

    #[test]
    fn windows_system_location() {
        let config = Config::new();
        assert!(config.is_windows_system_location(Path::new("/mnt/c/WINDOWS/system32/userenv.dll")));
        assert!(config.is_windows_system_location(Path::new("C:\\Windows\\SysWOW64\\rpcrt4.dll")));
        assert!(!config.is_windows_system_location(Path::new("/usr/x86_64-w64-mingw32/bin/libstdc++-6.dll")));
    }
}
