//! The ratatui interface (implemented in `app` / `ui` / `preview`).

mod app;
mod preview;
mod theme;
mod ui;

pub use app::{run, run_with};
