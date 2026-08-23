use std::collections::HashSet;

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

        let so_list = [
            "ld-linux-x86-64.so.2", "libc.so.6", "libm.so.6", "libdl.so.2",
            "libpthread.so.0", "librt.so.1", "libstdc++.so.6", "libgcc_s.so.1",
            "libGL.so.1", "libX11.so.6", "libXext.so.6", "libXcursor.so.1",
            "libXinerama.so.1", "libXi.so.6", "libXrandr.so.2", "libXrender.so.1",
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

    pub fn is_system_so(&self, so: &str) -> bool {
        self.system_sos.contains(so)
    }
}