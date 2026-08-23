use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "ship")]
#[command(about = "One-command auto-build, DLL resolver and packager for itch.io / GitHub")]
pub struct Cli {
    #[arg(help = "Optional explicit binary path (if omitted, auto-builds current project)")]
    pub binary: Option<PathBuf>,

    #[arg(short, long, default_value = "windows", help = "Target platform: windows or linux")]
    pub target: String,

    #[arg(short, long, default_value = "dist", help = "Output directory")]
    pub output: PathBuf,

    #[arg(short, long, help = "Custom name for output file")]
    pub name: Option<String>,

    #[arg(short = 's', long, help = "Bundle everything into a single standalone .exe")]
    pub single: bool,

    #[arg(long, help = "Generate both .zip and single standalone .exe")]
    pub all: bool,

    #[arg(long, help = "Additional library search paths")]
    pub search_paths: Vec<PathBuf>,

    #[arg(short = 'a', long, help = "Additional asset folders to include")]
    pub extra_assets: Vec<PathBuf>,
}