//! Lightweight progress reporting used by both the CLI and the TUI.

use std::sync::Arc;

/// A progress update emitted by the engine.
#[derive(Debug, Clone)]
pub enum Progress {
    /// A named stage with an optional overall fraction (0.0 – 1.0).
    Stage { text: String, fraction: Option<f32> },
    /// Page `done` of `total` being processed.
    Page { done: u32, total: u32 },
    /// Informational message (fallbacks, warnings, …).
    Log(String),
}

impl Progress {
    pub fn stage(text: impl Into<String>, fraction: f32) -> Self {
        Progress::Stage {
            text: text.into(),
            fraction: Some(fraction.clamp(0.0, 1.0)),
        }
    }

    pub fn page(done: u32, total: u32) -> Self {
        Progress::Page { done, total }
    }

    pub fn log(text: impl Into<String>) -> Self {
        Progress::Log(text.into())
    }
}

/// Callback type. It is `Send + Sync` so batch jobs can share one sink.
pub type ProgressFn = Arc<dyn Fn(Progress) + Send + Sync>;

/// A sink that ignores everything.
pub fn noop() -> ProgressFn {
    Arc::new(|_| {})
}

/// Build a sink from any compatible closure.
pub fn sink<F>(f: F) -> ProgressFn
where
    F: Fn(Progress) + Send + Sync + 'static,
{
    Arc::new(f)
}
