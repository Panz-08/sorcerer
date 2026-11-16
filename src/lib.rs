//! Sorcerer — an all-in-one file & document converter for the terminal.
//!
//! Crafted with love by Panz-08.
//!
//! The crate is split into a reusable engine ([`core`]), a terminal user
//! interface built with [ratatui](https://ratatui.rs) ([`tui`]) and a few
//! small helpers ([`util`]).  The binary is called `sorce`.

pub mod core;
pub mod tui;
pub mod util;

/// Long application name.
pub const APP_NAME: &str = "sorcerer";
/// Name of the command line binary.
pub const BIN_NAME: &str = "sorce";
/// Version taken from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The human behind the magic.
pub const AUTHOR: &str = "Panz-08";
/// One line description used in banners and `--version`.
pub const TAGLINE: &str = "an all-in-one file & document converter for the terminal";

/// `Sorcerer 0.1.0 · by Panz-08`
pub fn version_line() -> String {
    format!("{APP_NAME} {VERSION} · by {AUTHOR}")
}
