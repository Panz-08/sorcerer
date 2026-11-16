//! Sorcerer's conversion engine.
//!
//! The engine is usable as a library:
//!
//! ```no_run
//! use sorcerer::core::{convert_file, ConvertOptions, Format};
//! use std::path::Path;
//!
//! let opts = ConvertOptions::for_target(Format::Png);
//! # let progress = sorcerer::core::progress::noop();
//! let outputs = convert_file(Path::new("photo.jpg"), Path::new("photo.png"), &opts, &progress)?;
//! # Ok::<(), sorcerer::core::Error>(())
//! ```

pub mod backends;
pub mod batch;
pub mod convert;
pub mod docs;
pub mod error;
pub mod format;
pub mod image_ops;
pub mod inspect;
pub mod media;
pub mod merge;
pub mod options;
pub mod pdf_render;
pub mod pdf_write;
pub mod progress;
pub mod svg;

pub use batch::{BatchSpec, Job, JobProgressFn, JobResult, Plan, Summary};
pub use convert::{Route, can_convert, convert_file, describe, route};
pub use error::{Error, Result};
pub use format::{Category, Format};
pub use inspect::{FileInfo, inspect as inspect_file};
pub use merge::merge_to_pdf;
pub use options::{
    ConvertOptions, Fit, Flip, PageSelection, PageSize, Resize, Rotation, parse_color,
};
pub use progress::{Progress, ProgressFn, noop as noop_progress};
