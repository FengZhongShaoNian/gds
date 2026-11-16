//! Command-line argument definition for `gds-images`.

use std::path::PathBuf;

use clap::Parser;

#[derive(Clone, Debug, Parser, PartialEq, Eq)]
#[command(
    name = "gds-images",
    about = "Wayland-native image viewer, annotator, and metadata editor"
)]
pub struct Arguments {
    /// Image file path to open, or `-` to read image bytes from stdin.
    pub file: Option<PathBuf>,

    /// Read image bytes directly from standard input (ideal for grim / slurp pipelines).
    #[arg(long)]
    pub stdin: bool,

    /// Immediately load an image or copied image file from the Wayland clipboard.
    #[arg(long)]
    pub clipboard: bool,

    /// Target output path for immediate or capture-mode saving.
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// Also copy the rendered PNG image to the clipboard whenever saving to disk.
    #[arg(long)]
    pub copy_on_save: bool,

    /// Exit automatically once the image is copied or saved in capture workflows.
    #[arg(long)]
    pub early_exit: bool,

    /// Enable verbose application logging.
    #[arg(long)]
    pub enable_logs: bool,
}
