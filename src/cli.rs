use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "ship")]
#[command(about = "One-command auto-build, DLL resolver and packager for itch.io / GitHub")]
pub struct Cli {
    #[arg(help = "Optional explicit binary path (if omitted, auto-builds current project)")]
    pub binary: Option<PathBuf>,

    #[arg(short, long, help = "Target platform: windows or linux [default: windows]")]
    pub target: Option<String>,

    #[arg(short, long, help = "Output directory [default: dist]")]
    pub output: Option<PathBuf>,

    #[arg(short, long, help = "Custom name for output file")]
    pub name: Option<String>,

    #[arg(short = 's', long, help = "Bundle everything into a single standalone .exe")]
    pub single: bool,

    #[arg(long, help = "Generate both .zip and single standalone .exe")]
    pub all: bool,

    #[arg(long, help = "Also build a Linux .AppImage (needs mksquashfs)")]
    pub appimage: bool,

    #[arg(long, help = "Additional library search paths")]
    pub search_paths: Vec<PathBuf>,

    #[arg(short = 'a', long, help = "Additional asset folders to include")]
    pub extra_assets: Vec<PathBuf>,

    #[arg(long, help = "Build a console app on Windows (keeps the terminal attached)")]
    pub console: bool,

    #[arg(long, help = "Show what would be packaged (still builds) without creating a package")]
    pub dry_run: bool,

    #[arg(long, help = "Fail if any required library cannot be found")]
    pub strict: bool,

    #[arg(long, help = "Write a commented ship.toml template in the current directory")]
    pub init: bool,
}