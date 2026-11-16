//! Command line definitions for `sorce`.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

use sorcerer::core::format::Format;

/// Parse a format name (`png`, `jpeg`, `pdf`, `docx`, …).
pub fn parse_format(s: &str) -> Result<Format, String> {
    s.parse::<Format>().map_err(|e| e.to_string())
}

#[derive(Debug, Parser)]
#[command(
    name = "sorce",
    bin_name = "sorce",
    version = sorcerer::VERSION,
    author = "Panz-08",
    about = "Sorcerer — an all-in-one file & document converter for the terminal",
    long_about = "Sorcerer converts images, PDFs, office documents, SVGs, text and media \
                  without leaving the terminal. Run without a command to open the ratatui interface.",
    after_help = "Examples:\n  sorce photo.jpg -o photo.png\n  sorce scan.pdf -o pages --to png --dpi 200\n  sorce report.docx -o report.pdf\n  sorce batch ~/Pictures --to webp -o converted -j 8\n  sorce merge a.pdf b.pdf -o combined.pdf\n  sorce doctor\n\nCrafted by Panz-08 · https://github.com/Panz-08/sorcerer"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Convert one or more files (also accepts directories and globs).
    #[command(alias = "c", alias = "conv")]
    Convert(ConvertArgs),

    /// Convert a whole folder or glob into an output directory.
    #[command(alias = "b")]
    Batch(BatchArgs),

    /// Merge PDFs, images, documents or text into one PDF.
    #[command(alias = "m")]
    Merge(MergeArgs),

    /// Show format, dimensions, pages, duration and other metadata.
    #[command(alias = "i", alias = "info")]
    Inspect(InspectArgs),

    /// List every supported format and what it can become.
    #[command(alias = "f")]
    Formats(FormatsArgs),

    /// Check which optional backends are installed.
    #[command(alias = "doc")]
    Doctor,

    /// Open the full-screen ratatui file converter.
    #[command(alias = "ui")]
    Tui(TuiArgs),

    /// Print shell completion scripts.
    #[command(alias = "complete")]
    Completions {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: Shell,
    },
}

/// Conversion knobs shared by `convert` and `batch`.
#[derive(Debug, Clone, Args)]
pub struct ConvertFlags {
    /// Target format (png, jpg, webp, pdf, docx, mp4, …).
    #[arg(short = 't', long = "to", alias = "format", value_name = "FORMAT", value_parser = parse_format)]
    pub to: Option<Format>,

    /// Output file or directory.
    #[arg(short = 'o', long = "output", value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Quality for lossy formats (1–100).
    #[arg(short = 'q', long, value_name = "1-100")]
    pub quality: Option<u8>,

    /// DPI used for PDF/SVG rendering and PDF page density.
    #[arg(long, value_name = "DPI", default_value_t = 150)]
    pub dpi: u32,

    /// PDF pages: all, 1, 2-5, 1,3,7-9, 10-.
    #[arg(short = 'p', long, value_name = "SPEC", default_value = "all")]
    pub pages: String,

    /// Resize images, e.g. 1920x1080, 1920x or x1080.
    #[arg(long, value_name = "WxH")]
    pub resize: Option<String>,

    /// Resize mode: contain, cover or fill.
    #[arg(long, value_name = "MODE", default_value = "contain")]
    pub fit: String,

    /// Rotate clockwise by 0, 90, 180 or 270 degrees.
    #[arg(long, value_name = "DEG", default_value_t = 0)]
    pub rotate: u16,

    /// Flip: none, h, v or both.
    #[arg(long, value_name = "MODE", default_value = "none")]
    pub flip: String,

    /// Convert to grayscale.
    #[arg(short = 'g', long)]
    pub grayscale: bool,

    /// Background for transparent images (#rrggbb, white, black, …).
    #[arg(long, value_name = "COLOR", default_value = "white")]
    pub background: String,

    /// Replace existing output files.
    #[arg(short = 'y', long)]
    pub overwrite: bool,

    /// Paper size when writing PDFs: fit, a4, a4-auto or letter.
    #[arg(long, value_name = "SIZE", default_value = "fit")]
    pub page_size: String,

    /// Margin in millimetres for fixed PDF paper sizes.
    #[arg(long, value_name = "MM", default_value_t = 12.0)]
    pub margin: f32,

    /// Frame rate for video → GIF.
    #[arg(long, value_name = "FPS", default_value_t = 12.0)]
    pub fps: f32,

    /// Seek position (seconds) for video → image.
    #[arg(long, value_name = "SECONDS", default_value_t = 0.0)]
    pub seek: f64,

    /// Parallel jobs (0 = one per CPU).
    #[arg(short = 'j', long, value_name = "N", default_value_t = 0)]
    pub jobs: usize,

    /// Show what would happen without writing anything.
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Only print errors and the final summary.
    #[arg(long)]
    pub quiet: bool,
}

/// `sorce convert`
#[derive(Debug, Args)]
pub struct ConvertArgs {
    /// Input files, directories or glob patterns.
    #[arg(required = true, value_name = "INPUT")]
    pub inputs: Vec<String>,

    #[command(flatten)]
    pub flags: ConvertFlags,

    /// Recurse into input directories.
    #[arg(short = 'r', long)]
    pub recursive: bool,

    /// Only accept these source formats (comma separated).
    #[arg(long, value_name = "FORMATS", value_delimiter = ',', value_parser = parse_format)]
    pub from: Vec<Format>,

    /// Keep the relative folder structure inside the output directory.
    #[arg(long)]
    pub preserve_tree: bool,
}

/// `sorce batch`
#[derive(Debug, Args)]
pub struct BatchArgs {
    /// Directories, files or glob patterns (default: current directory).
    #[arg(value_name = "PATH", default_value = ".")]
    pub paths: Vec<String>,

    #[command(flatten)]
    pub flags: ConvertFlags,

    /// Only accept these source formats (comma separated).
    #[arg(long, value_name = "FORMATS", value_delimiter = ',', value_parser = parse_format)]
    pub from: Vec<Format>,

    /// Flatten the output instead of keeping the relative folder structure.
    #[arg(long)]
    pub flat: bool,

    /// Do not recurse into subdirectories.
    #[arg(long)]
    pub no_recursive: bool,
}

/// `sorce merge`
#[derive(Debug, Args)]
pub struct MergeArgs {
    /// Input PDFs, images, documents or text files.
    #[arg(required = true, value_name = "INPUT")]
    pub inputs: Vec<String>,

    /// Destination PDF.
    #[arg(short = 'o', long, value_name = "FILE")]
    pub output: PathBuf,

    /// Quality for images embedded in the PDF.
    #[arg(short = 'q', long, value_name = "1-100", default_value_t = 92)]
    pub quality: u8,

    /// DPI for SVG rasterisation and image page density.
    #[arg(long, default_value_t = 150)]
    pub dpi: u32,

    /// Paper size: fit, a4, a4-auto or letter.
    #[arg(long, default_value = "fit")]
    pub page_size: String,

    /// Margin in millimetres.
    #[arg(long, default_value_t = 12.0)]
    pub margin: f32,

    /// Replace an existing PDF.
    #[arg(short = 'y', long)]
    pub overwrite: bool,

    /// Recurse into directories.
    #[arg(short = 'r', long)]
    pub recursive: bool,

    /// Show the merge plan only.
    #[arg(short = 'n', long)]
    pub dry_run: bool,
}

/// `sorce inspect`
#[derive(Debug, Args)]
pub struct InspectArgs {
    /// Files or directories to inspect.
    #[arg(value_name = "FILE", default_value = ".")]
    pub files: Vec<String>,

    /// Recurse into directories.
    #[arg(short = 'r', long)]
    pub recursive: bool,

    /// Machine readable JSON output.
    #[arg(long)]
    pub json: bool,
}

/// `sorce formats`
#[derive(Debug, Args)]
pub struct FormatsArgs {
    /// Machine readable JSON output.
    #[arg(long)]
    pub json: bool,
}

/// `sorce tui`
#[derive(Debug, Args)]
pub struct TuiArgs {
    /// Start in this directory (or select this file).
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// Show hidden files in the browser.
    #[arg(long)]
    pub hidden: bool,

    /// Preferred number of parallel conversion workers.
    #[arg(short = 'j', long, default_value_t = 0)]
    pub jobs: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_quality_and_format() {
        let cli = Cli::try_parse_from([
            "sorce", "convert", "a.jpg", "-o", "a.png", "--to", "png", "-q", "80",
        ])
        .unwrap();
        match cli.command {
            Some(Command::Convert(args)) => {
                assert_eq!(args.flags.to, Some(Format::Png));
                assert_eq!(args.flags.quality, Some(80));
            }
            _ => panic!("expected convert"),
        }
    }
}
