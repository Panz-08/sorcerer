//! External backend discovery and process helpers.
//!
//! Sorcerer implements raster image processing, SVG rendering and PDF writing
//! entirely in Rust.  A few conversions need a helping hand from mature system
//! tools (Poppler, Ghostscript, LibreOffice, ffmpeg).  This module finds them
//! and runs them safely (never through a shell).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::error::{Error, Result};

/// Find an executable on `PATH`.
pub fn which(command: &str) -> Option<PathBuf> {
    if command.contains(std::path::MAIN_SEPARATOR) {
        let path = PathBuf::from(command);
        if is_executable(&path) {
            return Some(path);
        }
        return None;
    }

    let path_var = std::env::var_os("PATH")?;
    let exts: Vec<OsString> = if cfg!(windows) {
        std::env::var_os("PATHEXT")
            .map(|v| {
                v.to_string_lossy()
                    .split(';')
                    .map(|s| s.trim_start_matches('.').to_string())
                    .filter(|s| !s.is_empty())
                    .map(OsString::from)
                    .collect()
            })
            .unwrap_or_else(|| vec![OsString::from("exe")])
    } else {
        Vec::new()
    };

    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(command);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        if cfg!(windows) {
            for ext in &exts {
                let mut name = OsString::from(command);
                name.push(".");
                name.push(ext);
                let candidate = dir.join(name);
                if is_executable(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// First available command from a preference list.
pub fn first_available(commands: &[&str]) -> Option<(String, PathBuf)> {
    for command in commands {
        if let Some(path) = which(command) {
            return Some(((*command).to_string(), path));
        }
    }
    None
}

/// Require a backend or fail with an actionable hint.
pub fn require(commands: &[&str], hint: &str) -> Result<(String, PathBuf)> {
    first_available(commands).ok_or_else(|| Error::missing(commands.join(" / "), hint))
}

/// Run a command, returning its output even when the exit status is non-zero.
pub fn run<I, S>(program: impl AsRef<OsStr>, args: I) -> std::io::Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(program).args(args).output()
}

/// Run a command and turn a non-zero exit status into a readable error.
pub fn run_checked<I, S>(program: impl AsRef<OsStr>, args: I, label: &str) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let program = program.as_ref().to_os_string();
    let output = run(&program, args).map_err(|e| {
        Error::io(
            PathBuf::from(&program),
            std::io::Error::new(e.kind(), format!("{label}: {e}")),
        )
    })?;
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(Error::backend(label, tail(&stderr, 1800)))
    }
}

fn tail(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= max {
        return trimmed.to_string();
    }
    let start = trimmed.len() - max;
    format!("…{}", &trimmed[start..])
}

/// Description of an external tool, used by `sorce doctor`.
#[derive(Debug, Clone)]
pub struct BackendStatus {
    pub name: &'static str,
    pub command: &'static str,
    pub purpose: &'static str,
    pub required: bool,
    pub path: Option<PathBuf>,
}

impl BackendStatus {
    pub fn found(&self) -> bool {
        self.path.is_some()
    }
}

fn status(
    name: &'static str,
    command: &'static str,
    purpose: &'static str,
    required: bool,
) -> BackendStatus {
    BackendStatus {
        name,
        command,
        purpose,
        required,
        path: which(command),
    }
}

/// Every external backend Sorcerer may use.
pub fn doctor() -> Vec<BackendStatus> {
    vec![
        status("Poppler render", "pdftoppm", "PDF → PNG/JPEG/…", false),
        status("Poppler SVG", "pdftocairo", "PDF → SVG", false),
        status("Poppler text", "pdftotext", "PDF → text", false),
        status(
            "Poppler info",
            "pdfinfo",
            "PDF page count & metadata",
            false,
        ),
        status("Poppler merge", "pdfunite", "merge PDF files", false),
        status("Poppler split", "pdfseparate", "split PDF pages", false),
        status("Ghostscript", "gs", "fallback PDF renderer", false),
        status("MuPDF", "mutool", "fallback PDF renderer", false),
        status(
            "LibreOffice",
            "soffice",
            "office documents → PDF/images",
            false,
        ),
        status("ffmpeg", "ffmpeg", "video & audio conversion", false),
        status("ffprobe", "ffprobe", "media metadata", false),
        status("cwebp", "cwebp", "lossy WebP encoding", false),
        status("pandoc", "pandoc", "Markdown/HTML conversion", false),
        status("wkhtmltopdf", "wkhtmltopdf", "HTML → PDF", false),
        status("Chromium", "chromium", "HTML → PDF", false),
        status("Google Chrome", "google-chrome", "HTML → PDF", false),
    ]
}

/// True when ffmpeg understands the given encoder.
pub fn ffmpeg_has_encoder(name: &str) -> bool {
    static CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let encoders = CACHE.get_or_init(|| {
        which("ffmpeg")
            .and_then(|ffmpeg| {
                let out = run(ffmpeg, ["-hide_banner", "-encoders"]).ok()?;
                if !out.status.success() {
                    return None;
                }
                Some(
                    String::from_utf8_lossy(&out.stdout)
                        .lines()
                        .filter_map(|line| line.split_whitespace().nth(1).map(str::to_string))
                        .collect(),
                )
            })
            .unwrap_or_default()
    });
    encoders.iter().any(|e| e == name)
}
