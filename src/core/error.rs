//! Errors used by the conversion engine.

use std::path::PathBuf;

use thiserror::Error;

/// Everything that can go wrong during a conversion.
#[derive(Debug, Error)]
pub enum Error {
    #[error("unsupported conversion: {from} → {to}{hint}")]
    Unsupported {
        from: String,
        to: String,
        hint: String,
    },

    #[error(
        "backend `{backend}` is required for this conversion but was not found on PATH.\n  hint: {hint}"
    )]
    MissingBackend { backend: String, hint: String },

    #[error("backend `{backend}` failed: {message}")]
    BackendFailed { backend: String, message: String },

    #[error("cannot read or write {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("unknown or unsupported file format: {0}")]
    UnknownFormat(String),

    #[error("output already exists: {} (pass --overwrite to replace it)", .0.display())]
    Exists(PathBuf),

    #[error("invalid option: {0}")]
    InvalidOption(String),

    #[error("invalid page selection `{0}` (try `all`, `1`, `2-5`, `1,3,7-9`)")]
    InvalidPages(String),

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("PDF error: {0}")]
    Pdf(String),

    #[error("SVG error: {0}")]
    Svg(String),

    #[error("{0}")]
    Message(String),
}

/// Convenience result type for engine operations.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }

    pub fn unsupported(from: impl Into<String>, to: impl Into<String>) -> Self {
        Error::Unsupported {
            from: from.into(),
            to: to.into(),
            hint: String::new(),
        }
    }

    pub fn unsupported_hint(
        from: impl Into<String>,
        to: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Error::Unsupported {
            from: from.into(),
            to: to.into(),
            hint: format!("\n  hint: {}", hint.into()),
        }
    }

    pub fn missing(backend: impl Into<String>, hint: impl Into<String>) -> Self {
        Error::MissingBackend {
            backend: backend.into(),
            hint: hint.into(),
        }
    }

    pub fn backend(backend: impl Into<String>, message: impl Into<String>) -> Self {
        Error::BackendFailed {
            backend: backend.into(),
            message: message.into(),
        }
    }

    pub fn message(message: impl Into<String>) -> Self {
        Error::Message(message.into())
    }
}
