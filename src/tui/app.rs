//! Application state and the event loop for the Sorcerer TUI.

use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::core::convert_file;
use crate::core::format::Format;
use crate::core::options::ConvertOptions;
use crate::core::progress::{Progress, ProgressFn};
use crate::util::paths;

use super::preview::Preview;
use super::ui;

/// Which panel receives navigation keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Browser,
    Queue,
}

/// State of one job in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Pending,
    Running,
    Done,
    Failed,
}

/// A file waiting to be converted.
pub struct QueueItem {
    pub input: PathBuf,
    pub target: Format,
    pub state: JobState,
    pub progress: f32,
    pub stage: String,
    pub outputs: Vec<PathBuf>,
    pub error: Option<String>,
}

impl QueueItem {
    fn new(input: PathBuf, target: Format) -> Self {
        Self {
            input,
            target,
            state: JobState::Pending,
            progress: 0.0,
            stage: String::new(),
            outputs: Vec::new(),
            error: None,
        }
    }
}

/// One entry in the file browser.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// The synthetic `..` entry.
    pub parent: bool,
    pub format: Option<Format>,
    pub size: u64,
}

/// A modal overlay.
pub enum Overlay {
    None,
    Help,
    Formats { formats: Vec<Format>, index: usize },
    Input(InputPrompt),
}

/// What an input prompt will do when the user presses Enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Output,
    Filter,
    Goto,
    Quality,
    Dpi,
}

/// A single-line text prompt.
pub struct InputPrompt {
    pub kind: InputKind,
    pub title: String,
    pub hint: String,
    pub value: String,
    pub cursor: usize,
}

impl InputPrompt {
    fn new(kind: InputKind, title: &str, hint: &str, value: String) -> Self {
        let cursor = value.chars().count();
        Self {
            kind,
            title: title.to_string(),
            hint: hint.to_string(),
            value,
            cursor,
        }
    }

    pub fn len(&self) -> usize {
        self.value.chars().count()
    }

    fn byte_at(&self, index: usize) -> usize {
        self.value
            .char_indices()
            .nth(index)
            .map(|(byte, _)| byte)
            .unwrap_or(self.value.len())
    }

    fn insert(&mut self, ch: char) {
        let at = self.byte_at(self.cursor);
        self.value.insert(at, ch);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            let at = self.byte_at(self.cursor - 1);
            self.value.remove(at);
            self.cursor -= 1;
        }
    }

    fn delete(&mut self) {
        if self.cursor < self.len() {
            let at = self.byte_at(self.cursor);
            self.value.remove(at);
        }
    }
}

struct WorkerJob {
    index: usize,
    input: PathBuf,
    output: PathBuf,
    options: ConvertOptions,
}

enum WorkerEvent {
    Started(usize),
    Progress(usize, Option<f32>, String),
    Finished {
        index: usize,
        outputs: Vec<PathBuf>,
        error: Option<String>,
    },
    AllDone {
        ok: usize,
        failed: usize,
    },
}

struct Worker {
    events: Receiver<WorkerEvent>,
}

/// The whole interface state.
pub struct App {
    pub cwd: PathBuf,
    all_entries: Vec<Entry>,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub marked: BTreeSet<PathBuf>,

    pub focus: Focus,
    pub target: Format,
    pub output_dir: Option<PathBuf>,
    pub quality: u8,
    pub dpi: u32,
    pub grayscale: bool,
    pub overwrite: bool,
    pub show_hidden: bool,
    pub jobs: usize,
    pub filter: String,

    pub queue: Vec<QueueItem>,
    pub queue_selected: usize,

    pub preview: Preview,
    pending_preview: Option<(PathBuf, Instant)>,

    pub overlay: Overlay,
    pub status: String,
    pub status_error: bool,

    worker: Option<Worker>,
    pub should_quit: bool,
}

impl App {
    pub fn with_options(start: Option<PathBuf>, show_hidden: bool, jobs: usize) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut app = Self {
            cwd,
            all_entries: Vec::new(),
            entries: Vec::new(),
            selected: 0,
            marked: BTreeSet::new(),
            focus: Focus::Browser,
            target: Format::Png,
            output_dir: None,
            quality: Format::Png.default_quality(),
            dpi: 150,
            grayscale: false,
            overwrite: false,
            show_hidden,
            jobs,
            filter: String::new(),
            queue: Vec::new(),
            queue_selected: 0,
            preview: Preview::empty(),
            pending_preview: None,
            overlay: Overlay::None,
            status: "Ready — mark files with Space, choose a target with t, then press c."
                .to_string(),
            status_error: false,
            worker: None,
            should_quit: false,
        };

        let mut initial_file = None;
        if let Some(start) = start {
            let start = if start.is_absolute() {
                start
            } else {
                app.cwd.join(start)
            };
            if start.is_dir() {
                app.cwd = start;
            } else if start.is_file() {
                initial_file = Some(start.clone());
                if let Some(parent) = start.parent() {
                    app.cwd = parent.to_path_buf();
                }
            }
        }

        app.reload();
        if let Some(file) = initial_file {
            app.select_path(&file);
        }
        app.queue_preview();
        app
    }

    // -----------------------------------------------------------------
    // Browser
    // -----------------------------------------------------------------

    fn reload(&mut self) {
        let mut entries = Vec::new();
        if let Some(parent) = self.cwd.parent() {
            entries.push(Entry {
                name: "..".to_string(),
                path: parent.to_path_buf(),
                is_dir: true,
                parent: true,
                format: None,
                size: 0,
            });
        }

        match std::fs::read_dir(&self.cwd) {
            Ok(read_dir) => {
                for item in read_dir.flatten() {
                    let name = item.file_name().to_string_lossy().to_string();
                    if !self.show_hidden && name.starts_with('.') {
                        continue;
                    }
                    let path = item.path();
                    let metadata = item.metadata().ok();
                    let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                    let format = if is_dir {
                        None
                    } else {
                        path.extension()
                            .and_then(|ext| ext.to_str())
                            .and_then(Format::from_extension)
                    };
                    let size = metadata.map(|m| m.len()).unwrap_or(0);
                    entries.push(Entry {
                        name,
                        path,
                        is_dir,
                        parent: false,
                        format,
                        size,
                    });
                }
            }
            Err(err) => {
                self.status = format!("Cannot read {}: {err}", paths::pretty(&self.cwd));
                self.status_error = true;
            }
        }

        let start = usize::from(entries.first().map(|e| e.parent).unwrap_or(false));
        entries[start..].sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        self.all_entries = entries;
        self.apply_filter();
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len().saturating_sub(1);
        }
    }

    fn apply_filter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.entries = self
            .all_entries
            .iter()
            .filter(|entry| {
                entry.parent || needle.is_empty() || entry.name.to_lowercase().contains(&needle)
            })
            .cloned()
            .collect();
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len().saturating_sub(1);
        }
    }

    fn select_path(&mut self, path: &Path) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| paths::same_path(&entry.path, path))
        {
            self.selected = index;
        }
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.entries
            .get(self.selected)
            .map(|entry| entry.path.clone())
    }

    fn current_entry(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    fn move_selection(&mut self, delta: isize) {
        if self.focus == Focus::Queue {
            if self.queue.is_empty() {
                return;
            }
            self.queue_selected = clamp_index(self.queue_selected, delta, self.queue.len());
            return;
        }
        if self.entries.is_empty() {
            return;
        }
        let before = self.selected;
        self.selected = clamp_index(self.selected, delta, self.entries.len());
        if self.selected != before {
            self.queue_preview();
        }
    }

    fn jump(&mut self, last: bool) {
        if self.focus == Focus::Queue {
            if !self.queue.is_empty() {
                self.queue_selected = if last { self.queue.len() - 1 } else { 0 };
            }
            return;
        }
        if !self.entries.is_empty() {
            self.selected = if last { self.entries.len() - 1 } else { 0 };
            self.queue_preview();
        }
    }

    fn enter_selected(&mut self) {
        let Some(entry) = self.current_entry().cloned() else {
            return;
        };
        if entry.is_dir {
            self.cwd = entry.path;
            self.filter.clear();
            self.reload();
            self.selected = self
                .entries
                .iter()
                .position(|entry| !entry.parent)
                .unwrap_or(0);
            self.queue_preview();
        } else {
            self.pending_preview = None;
            self.preview = Preview::load(&entry.path);
        }
    }

    fn go_parent(&mut self) {
        let Some(parent) = self.cwd.parent().map(Path::to_path_buf) else {
            self.set_status("Already at the root directory.", true);
            return;
        };
        let previous_name = self
            .cwd
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        self.cwd = parent;
        self.filter.clear();
        self.reload();
        if let Some(name) = previous_name
            && let Some(index) = self
                .entries
                .iter()
                .position(|entry| !entry.parent && entry.is_dir && entry.name == name)
        {
            self.selected = index;
        }
        self.queue_preview();
    }

    fn toggle_mark(&mut self) {
        let Some(entry) = self.current_entry().cloned() else {
            return;
        };
        if entry.is_dir {
            self.set_status(
                "Directories cannot be marked — open one or use `a` on it.",
                true,
            );
            return;
        }
        if self.marked.remove(&entry.path) {
            self.set_status(format!("Unmarked {}", entry.name), false);
        } else {
            self.marked.insert(entry.path);
            self.set_status(
                format!(
                    "Marked {} · {} file(s) marked",
                    entry.name,
                    self.marked.len()
                ),
                false,
            );
        }
        if self.focus == Focus::Browser {
            self.move_selection(1);
        }
    }

    fn refresh(&mut self) {
        let selected = self.selected_path();
        self.reload();
        if let Some(path) = selected {
            self.select_path(&path);
        }
        self.queue_preview();
    }

    // -----------------------------------------------------------------
    // Preview
    // -----------------------------------------------------------------

    fn queue_preview(&mut self) {
        let Some(entry) = self.current_entry().cloned() else {
            self.preview = Preview::empty();
            self.pending_preview = None;
            return;
        };
        if entry.is_dir {
            let count = std::fs::read_dir(&entry.path)
                .map(|read_dir| read_dir.flatten().count())
                .unwrap_or(0);
            self.preview = Preview::directory(&entry.path, count);
            self.pending_preview = None;
        } else {
            self.pending_preview = Some((entry.path, Instant::now()));
        }
    }

    fn tick(&mut self) {
        let Some((path, since)) = &self.pending_preview else {
            return;
        };
        if since.elapsed() < Duration::from_millis(110) {
            return;
        }
        let path = path.clone();
        self.pending_preview = None;
        self.preview = Preview::load(&path);
    }

    // -----------------------------------------------------------------
    // Queue
    // -----------------------------------------------------------------

    fn supported_by_extension(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(Format::from_extension)
            .is_some()
    }

    fn addable_paths(&self) -> Vec<PathBuf> {
        if !self.marked.is_empty() {
            return self.marked.iter().cloned().collect();
        }
        let Some(entry) = self.current_entry() else {
            return Vec::new();
        };
        if entry.is_dir {
            if entry.parent {
                return Vec::new();
            }
            let mut files: Vec<PathBuf> = std::fs::read_dir(&entry.path)
                .map(|read_dir| {
                    read_dir
                        .flatten()
                        .map(|item| item.path())
                        .filter(|path| path.is_file() && Self::supported_by_extension(path))
                        .collect()
                })
                .unwrap_or_default();
            files.sort();
            files
        } else {
            vec![entry.path.clone()]
        }
    }

    fn add_selection_to_queue(&mut self) {
        let candidates = self.addable_paths();
        if candidates.is_empty() {
            self.set_status(
                "Nothing to add — select a file or mark some with Space.",
                true,
            );
            return;
        }
        let before = self.queue.len();
        for path in candidates {
            if self
                .queue
                .iter()
                .any(|item| paths::same_path(&item.input, &path))
            {
                continue;
            }
            self.queue.push(QueueItem::new(path, self.target));
        }
        let added = self.queue.len() - before;
        if added == 0 {
            self.set_status("Those files are already in the queue.", false);
        } else {
            self.set_status(
                format!("Queued {added} file(s) · {} total", self.queue.len()),
                false,
            );
        }
    }

    fn output_for(&self, input: &Path) -> PathBuf {
        let mut output = match &self.output_dir {
            Some(dir) => paths::output_in_dir(input, dir, self.target),
            None => paths::with_extension(input, self.target),
        };
        if paths::same_path(&output, input) {
            let stem = paths::file_stem(input);
            output = input.with_file_name(format!("{stem}-converted.{}", self.target.extension()));
        }
        output
    }

    fn options(&self) -> ConvertOptions {
        ConvertOptions {
            quality: self.quality,
            dpi: self.dpi,
            grayscale: self.grayscale,
            overwrite: self.overwrite,
            ..ConvertOptions::for_target(self.target)
        }
    }

    fn start_conversion(&mut self) {
        if self.worker.is_some() {
            self.set_status(
                "A conversion is already running — wait for it to finish.",
                true,
            );
            return;
        }
        if self.queue.is_empty() {
            self.add_selection_to_queue();
        }
        if self.queue.is_empty() {
            return;
        }

        let has_pending = self
            .queue
            .iter()
            .any(|item| matches!(item.state, JobState::Pending | JobState::Failed));
        if !has_pending {
            let outputs_exist =
                self.queue
                    .iter()
                    .flat_map(|item| item.outputs.iter())
                    .any(|path| {
                        path.exists()
                            && path
                                .extension()
                                .and_then(|extension| extension.to_str())
                                .map(|extension| {
                                    extension.eq_ignore_ascii_case(self.target.extension())
                                })
                                .unwrap_or(false)
                    });
            if outputs_exist && !self.overwrite {
                self.set_status(
                    "The queue already finished — press w to allow overwrite, or x to clear it.",
                    true,
                );
                return;
            }
        }
        for item in &mut self.queue {
            if !has_pending || item.state == JobState::Failed {
                item.state = JobState::Pending;
                item.target = self.target;
                item.progress = 0.0;
                item.stage.clear();
                item.outputs.clear();
                item.error = None;
            }
        }

        let target = self.target;
        let jobs: Vec<WorkerJob> = self
            .queue
            .iter()
            .enumerate()
            .filter(|(_, item)| item.state == JobState::Pending)
            .map(|(index, item)| WorkerJob {
                index,
                input: item.input.clone(),
                output: self.output_for(&item.input),
                options: self.options(),
            })
            .collect();

        if jobs.is_empty() {
            self.set_status("Nothing to convert.", false);
            return;
        }

        let count = jobs.len();
        let parallelism = if self.jobs == 0 {
            num_cpus::get()
        } else {
            self.jobs
        };
        let (sender, events) = mpsc::channel();
        thread::spawn(move || run_worker(jobs, sender, parallelism));
        self.worker = Some(Worker { events });
        self.focus = Focus::Queue;
        self.queue_selected = self
            .queue
            .iter()
            .position(|item| item.state == JobState::Pending)
            .unwrap_or(0);
        self.set_status(
            format!(
                "Converting {count} file(s) to {} — output: {}",
                target.label(),
                super::preview::display_path(self.output_dir.as_deref())
            ),
            false,
        );
    }

    fn remove_queue_item(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        if self
            .queue
            .get(self.queue_selected)
            .map(|item| item.state == JobState::Running)
            .unwrap_or(false)
        {
            self.set_status("That job is running and cannot be removed.", true);
            return;
        }
        let removed = self.queue.remove(self.queue_selected);
        self.queue_selected = self.queue_selected.min(self.queue.len().saturating_sub(1));
        self.set_status(
            format!(
                "Removed {} from the queue.",
                paths::file_stem(&removed.input)
            ),
            false,
        );
    }

    fn clear_queue(&mut self) {
        if self
            .queue
            .iter()
            .any(|item| item.state == JobState::Running)
        {
            self.set_status("Wait for the running conversion to finish.", true);
            return;
        }
        self.queue.clear();
        self.queue_selected = 0;
        self.set_status("Queue cleared.", false);
    }

    fn poll_worker(&mut self) {
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(worker) = self.worker.as_ref() {
            loop {
                match worker.events.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        for event in events {
            self.apply_worker_event(event);
        }
        if disconnected {
            self.worker = None;
            let mut interrupted = false;
            for item in &mut self.queue {
                if item.state == JobState::Running {
                    interrupted = true;
                    item.state = JobState::Failed;
                    item.stage = "stopped".to_string();
                    item.error = Some("conversion worker stopped unexpectedly".to_string());
                }
            }
            if interrupted {
                self.set_status("A conversion stopped unexpectedly.", true);
            }
        }
    }

    fn apply_worker_event(&mut self, event: WorkerEvent) {
        match event {
            WorkerEvent::Started(index) => {
                if let Some(item) = self.queue.get_mut(index) {
                    item.state = JobState::Running;
                    item.stage = "starting…".to_string();
                    item.progress = 0.0;
                }
            }
            WorkerEvent::Progress(index, fraction, stage) => {
                if let Some(item) = self.queue.get_mut(index) {
                    if let Some(fraction) = fraction {
                        item.progress = fraction.clamp(0.0, 1.0);
                    }
                    if !stage.is_empty() {
                        item.stage = stage;
                    }
                }
            }
            WorkerEvent::Finished {
                index,
                outputs,
                error,
            } => {
                if let Some(item) = self.queue.get_mut(index) {
                    item.progress = 1.0;
                    match error {
                        Some(error) => {
                            item.state = JobState::Failed;
                            item.stage = "failed".to_string();
                            item.error = Some(error);
                        }
                        None => {
                            item.state = JobState::Done;
                            item.stage = "done".to_string();
                            item.outputs = outputs;
                        }
                    }
                }
            }
            WorkerEvent::AllDone { ok, failed } => {
                self.worker = None;
                let mut message = format!("Finished — {ok} converted");
                if failed > 0 {
                    message.push_str(&format!(", {failed} failed"));
                }
                self.set_status(message, failed > 0);

                if ok == 1
                    && failed == 0
                    && let Some(output) = self
                        .queue
                        .iter()
                        .rev()
                        .find_map(|item| item.outputs.first().cloned())
                {
                    self.preview = Preview::load(&output);
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // Settings
    // -----------------------------------------------------------------

    fn set_target(&mut self, target: Format) {
        self.target = target;
        self.quality = target.default_quality();
        for item in &mut self.queue {
            if matches!(item.state, JobState::Pending | JobState::Failed) {
                item.target = target;
            }
        }
        self.set_status(
            format!(
                "Target: {} ({}) · quality {}",
                target.label(),
                target.extension(),
                self.quality
            ),
            false,
        );
    }

    fn current_format(&self) -> Option<Format> {
        if let Some(entry) = self.current_entry()
            && !entry.is_dir
            && let Some(format) = entry.format
        {
            return Some(format);
        }
        self.marked
            .iter()
            .find_map(|path| Format::from_path(path).ok())
    }

    fn open_formats(&mut self) {
        let formats = self
            .current_format()
            .map(|format| format.targets_from())
            .unwrap_or_else(Format::all);
        let index = formats
            .iter()
            .position(|format| *format == self.target)
            .unwrap_or(0);
        self.overlay = Overlay::Formats { formats, index };
    }

    fn open_output_prompt(&mut self) {
        let value = self
            .output_dir
            .as_ref()
            .map(|path| paths::pretty(path))
            .unwrap_or_default();
        self.overlay = Overlay::Input(InputPrompt::new(
            InputKind::Output,
            "Output folder",
            "Leave empty to write next to each source file. Relative paths start in the browser folder.",
            value,
        ));
    }

    fn open_filter_prompt(&mut self) {
        self.overlay = Overlay::Input(InputPrompt::new(
            InputKind::Filter,
            "Filter files",
            "Only names containing this text are shown. Leave empty to show everything.",
            self.filter.clone(),
        ));
    }

    fn open_goto_prompt(&mut self) {
        self.overlay = Overlay::Input(InputPrompt::new(
            InputKind::Goto,
            "Go to folder",
            "Enter a directory path (~ is expanded).",
            paths::pretty(&self.cwd),
        ));
    }

    fn open_quality_prompt(&mut self) {
        self.overlay = Overlay::Input(InputPrompt::new(
            InputKind::Quality,
            "Quality",
            "1 – 100. Used by JPEG, WebP, AVIF, JXL and media targets.",
            self.quality.to_string(),
        ));
    }

    fn open_dpi_prompt(&mut self) {
        self.overlay = Overlay::Input(InputPrompt::new(
            InputKind::Dpi,
            "DPI",
            "Resolution for PDF/SVG rendering and raster → PDF (36 – 1200).",
            self.dpi.to_string(),
        ));
    }

    fn resolve_path(&self, raw: &str) -> PathBuf {
        let raw = raw.trim();
        let expanded = if raw == "~" {
            dirs::home_dir().unwrap_or_else(|| self.cwd.clone())
        } else if let Some(rest) = raw.strip_prefix("~/") {
            dirs::home_dir()
                .unwrap_or_else(|| self.cwd.clone())
                .join(rest)
        } else {
            PathBuf::from(raw)
        };
        if expanded.is_absolute() {
            expanded
        } else {
            self.cwd.join(expanded)
        }
    }

    fn apply_input(&mut self, kind: InputKind, value: &str) {
        match kind {
            InputKind::Output => {
                let value = value.trim();
                if value.is_empty() {
                    self.output_dir = None;
                    self.set_status("Output: same folder as each source file.", false);
                } else {
                    let path = self.resolve_path(value);
                    self.output_dir = Some(path.clone());
                    self.set_status(format!("Output: {}", paths::pretty(&path)), false);
                }
            }
            InputKind::Filter => {
                self.filter = value.trim().to_string();
                self.selected = 0;
                self.apply_filter();
                self.queue_preview();
                if self.filter.is_empty() {
                    self.set_status("Filter cleared.", false);
                } else {
                    self.set_status(
                        format!(
                            "Filter: `{}` · {} match(es)",
                            self.filter,
                            self.entries.len()
                        ),
                        false,
                    );
                }
            }
            InputKind::Goto => {
                let value = value.trim();
                if value.is_empty() {
                    return;
                }
                let path = self.resolve_path(value);
                if path.is_dir() {
                    self.cwd = path;
                    self.filter.clear();
                    self.reload();
                    self.selected = self
                        .entries
                        .iter()
                        .position(|entry| !entry.parent)
                        .unwrap_or(0);
                    self.queue_preview();
                } else {
                    self.set_status(format!("Not a directory: {}", path.display()), true);
                }
            }
            InputKind::Quality => match value.trim().parse::<u8>() {
                Ok(quality) if (1..=100).contains(&quality) => {
                    self.quality = quality;
                    self.set_status(format!("Quality: {quality}"), false);
                }
                _ => self.set_status("Quality must be a number between 1 and 100.", true),
            },
            InputKind::Dpi => match value.trim().parse::<u32>() {
                Ok(dpi) if (36..=1200).contains(&dpi) => {
                    self.dpi = dpi;
                    self.set_status(format!("DPI: {dpi}"), false);
                }
                _ => self.set_status("DPI must be a number between 36 and 1200.", true),
            },
        }
    }

    fn adjust_quality(&mut self, delta: i16) {
        self.quality = (self.quality as i16 + delta).clamp(1, 100) as u8;
        self.set_status(format!("Quality: {}", self.quality), false);
    }

    fn adjust_dpi(&mut self, delta: i32) {
        self.dpi = (self.dpi as i32 + delta).clamp(36, 1200) as u32;
        self.set_status(format!("DPI: {}", self.dpi), false);
    }

    fn toggle_overwrite(&mut self) {
        self.overwrite = !self.overwrite;
        self.set_status(
            format!(
                "Overwrite existing files: {}",
                if self.overwrite { "on" } else { "off" }
            ),
            false,
        );
    }

    fn toggle_grayscale(&mut self) {
        self.grayscale = !self.grayscale;
        self.set_status(
            format!("Grayscale: {}", if self.grayscale { "on" } else { "off" }),
            false,
        );
    }

    fn set_status(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.status_error = error;
    }

    // -----------------------------------------------------------------
    // Keys
    // -----------------------------------------------------------------

    fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.should_quit = true;
            return;
        }

        if !matches!(self.overlay, Overlay::None) {
            let overlay = std::mem::replace(&mut self.overlay, Overlay::None);
            match overlay {
                Overlay::None => {}
                Overlay::Help => {}
                Overlay::Formats { formats, mut index } => {
                    let mut keep = true;
                    let mut chosen = None;
                    match key.code {
                        KeyCode::Esc => keep = false,
                        KeyCode::Enter => {
                            chosen = formats.get(index).copied();
                            keep = false;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            index = index.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if index + 1 < formats.len() {
                                index += 1;
                            }
                        }
                        KeyCode::PageUp => index = index.saturating_sub(5),
                        KeyCode::PageDown => {
                            if !formats.is_empty() {
                                index = (index + 5).min(formats.len() - 1);
                            }
                        }
                        KeyCode::Home => index = 0,
                        KeyCode::End if !formats.is_empty() => {
                            index = formats.len() - 1;
                        }
                        _ => {}
                    }
                    if let Some(format) = chosen {
                        self.set_target(format);
                    } else if keep && !formats.is_empty() {
                        self.overlay = Overlay::Formats { formats, index };
                    }
                }
                Overlay::Input(mut prompt) => {
                    let mut keep = true;
                    let mut apply = false;
                    match key.code {
                        KeyCode::Esc => keep = false,
                        KeyCode::Enter => {
                            keep = false;
                            apply = true;
                        }
                        KeyCode::Backspace => prompt.backspace(),
                        KeyCode::Delete => prompt.delete(),
                        KeyCode::Left => prompt.cursor = prompt.cursor.saturating_sub(1),
                        KeyCode::Right => prompt.cursor = (prompt.cursor + 1).min(prompt.len()),
                        KeyCode::Home => prompt.cursor = 0,
                        KeyCode::End => prompt.cursor = prompt.len(),
                        KeyCode::Char(ch)
                            if !key.modifiers.contains(KeyModifiers::CONTROL)
                                && !key.modifiers.contains(KeyModifiers::ALT) =>
                        {
                            prompt.insert(ch);
                        }
                        _ => {}
                    }
                    if apply {
                        self.apply_input(prompt.kind, &prompt.value);
                    }
                    if keep {
                        self.overlay = Overlay::Input(prompt);
                    }
                }
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => {
                if self.filter.is_empty() {
                    self.set_status("Press q to quit · ? for help.", false);
                } else {
                    self.filter.clear();
                    self.selected = 0;
                    self.apply_filter();
                    self.queue_preview();
                    self.set_status("Filter cleared.", false);
                }
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Browser => Focus::Queue,
                    Focus::Queue => Focus::Browser,
                };
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::Home => self.jump(false),
            KeyCode::End => self.jump(true),
            KeyCode::Left | KeyCode::Backspace => {
                if self.focus == Focus::Browser {
                    self.go_parent();
                }
            }
            KeyCode::Enter => {
                if self.focus == Focus::Browser {
                    self.enter_selected();
                } else if let Some(item) = self.queue.get(self.queue_selected) {
                    if let Some(output) = item.outputs.first() {
                        self.preview = Preview::load(output);
                        self.set_status(format!("Previewing {}", paths::pretty(output)), false);
                    } else {
                        self.set_status("This job has no output yet.", true);
                    }
                }
            }
            KeyCode::Char(' ') => {
                if self.focus == Focus::Browser {
                    self.toggle_mark();
                }
            }
            KeyCode::Char('a') => self.add_selection_to_queue(),
            KeyCode::Char('c') => self.start_conversion(),
            KeyCode::Char('t') => self.open_formats(),
            KeyCode::Char('o') => self.open_output_prompt(),
            KeyCode::Char('f') => self.open_filter_prompt(),
            KeyCode::Char('g') => self.open_goto_prompt(),
            KeyCode::Char('p') => self.open_quality_prompt(),
            KeyCode::Char('i') => self.open_dpi_prompt(),
            KeyCode::Char('w') => self.toggle_overwrite(),
            KeyCode::Char('n') => self.toggle_grayscale(),
            KeyCode::Char('[') => self.adjust_quality(-5),
            KeyCode::Char(']') => self.adjust_quality(5),
            KeyCode::Char(',') => self.adjust_dpi(-25),
            KeyCode::Char('.') => self.adjust_dpi(25),
            KeyCode::Char('r') => {
                self.refresh();
                self.set_status("Browser refreshed.", false);
            }
            KeyCode::Char('h') => {
                self.show_hidden = !self.show_hidden;
                self.refresh();
                self.set_status(
                    if self.show_hidden {
                        "Showing hidden files."
                    } else {
                        "Hiding dotfiles."
                    },
                    false,
                );
            }
            KeyCode::Char('x') => {
                if self.focus == Focus::Queue {
                    self.clear_queue();
                } else if self.marked.is_empty() {
                    self.set_status("No marks to clear.", false);
                } else {
                    self.marked.clear();
                    self.set_status("Marks cleared.", false);
                }
            }
            KeyCode::Char('d') => {
                if self.focus == Focus::Queue {
                    self.remove_queue_item();
                }
            }
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            KeyCode::Char('1') => self.set_target(Format::Png),
            KeyCode::Char('2') => self.set_target(Format::Jpeg),
            KeyCode::Char('3') => self.set_target(Format::Webp),
            KeyCode::Char('4') => self.set_target(Format::Pdf),
            _ => {}
        }
    }

    fn event_loop(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            self.poll_worker();
            self.tick();
            terminal.draw(|frame| ui::draw(frame, self))?;

            if event::poll(Duration::from_millis(70))?
                && let Event::Key(key) = event::read()?
                && key.kind != KeyEventKind::Release
            {
                self.on_key(key);
            }

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }
}

fn clamp_index(index: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let value = index as isize + delta;
    value.clamp(0, len as isize - 1) as usize
}

fn run_worker(jobs: Vec<WorkerJob>, sender: mpsc::Sender<WorkerEvent>, parallelism: usize) {
    let total = jobs.len();
    let threads = parallelism.max(1).min(total.max(1));
    let queue = Arc::new(Mutex::new(jobs));
    let ok = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..threads {
        let queue = Arc::clone(&queue);
        let sender = sender.clone();
        let ok = Arc::clone(&ok);
        let failed = Arc::clone(&failed);

        handles.push(thread::spawn(move || {
            loop {
                let job = match queue.lock() {
                    Ok(mut queue) => queue.pop(),
                    Err(_) => break,
                };
                let Some(job) = job else {
                    break;
                };

                let _ = sender.send(WorkerEvent::Started(job.index));
                let index = job.index;
                let progress_sender = sender.clone();
                let progress: ProgressFn = Arc::new(move |event: Progress| {
                    let (fraction, stage) = match event {
                        Progress::Stage { text, fraction } => (fraction, text),
                        Progress::Page { done, total } => (
                            Some(done as f32 / total.max(1) as f32),
                            format!("page {done}/{total}"),
                        ),
                        Progress::Log(text) => (None, text),
                    };
                    let _ = progress_sender.send(WorkerEvent::Progress(index, fraction, stage));
                });

                match convert_file(&job.input, &job.output, &job.options, &progress) {
                    Ok(outputs) => {
                        ok.fetch_add(1, Ordering::Relaxed);
                        let _ = sender.send(WorkerEvent::Finished {
                            index: job.index,
                            outputs,
                            error: None,
                        });
                    }
                    Err(err) => {
                        failed.fetch_add(1, Ordering::Relaxed);
                        let _ = sender.send(WorkerEvent::Finished {
                            index: job.index,
                            outputs: Vec::new(),
                            error: Some(err.to_string()),
                        });
                    }
                }
            }
        }));
    }

    for handle in handles {
        let _ = handle.join();
    }

    let _ = sender.send(WorkerEvent::AllDone {
        ok: ok.load(Ordering::Relaxed),
        failed: failed.load(Ordering::Relaxed),
    });
}

/// Start the full-screen interface.
pub fn run(start: Option<PathBuf>) -> anyhow::Result<()> {
    run_with(start, false, 0)
}

/// Start the interface with the options accepted by `sorce tui`.
pub fn run_with(start: Option<PathBuf>, show_hidden: bool, jobs: usize) -> anyhow::Result<()> {
    if !std::io::stdout().is_terminal() {
        anyhow::bail!(
            "the interactive interface needs a terminal; use `sorce convert --help` for the CLI"
        );
    }

    let mut terminal = ratatui::init();
    let mut app = App::with_options(start, show_hidden, jobs);
    let result = app.event_loop(&mut terminal);
    ratatui::restore();
    result
}

/// Shared status helper used by the UI.
pub fn queue_counts(app: &App) -> (usize, usize, usize, usize) {
    let mut pending = 0;
    let mut running = 0;
    let mut done = 0;
    let mut failed = 0;
    for item in &app.queue {
        match item.state {
            JobState::Pending => pending += 1,
            JobState::Running => running += 1,
            JobState::Done => done += 1,
            JobState::Failed => failed += 1,
        }
    }
    (pending, running, done, failed)
}

/// Human readable queue summary for the block title.
pub fn queue_summary(app: &App) -> String {
    let (pending, running, done, failed) = queue_counts(app);
    let mut parts = Vec::new();
    if pending > 0 {
        parts.push(format!("{pending} pending"));
    }
    if running > 0 {
        parts.push(format!("{running} running"));
    }
    if done > 0 {
        parts.push(format!("{done} done"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    if parts.is_empty() {
        "empty".to_string()
    } else {
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_selection() {
        assert_eq!(clamp_index(0, -1, 5), 0);
        assert_eq!(clamp_index(4, 1, 5), 4);
        assert_eq!(clamp_index(2, 1, 5), 3);
        assert_eq!(clamp_index(0, 0, 0), 0);
    }

    #[test]
    fn edits_prompt_with_unicode() {
        let mut prompt = InputPrompt::new(InputKind::Filter, "Filter", "", "aé".to_string());
        assert_eq!(prompt.cursor, 2);
        prompt.backspace();
        assert_eq!(prompt.value, "a");
        prompt.insert('ß');
        assert_eq!(prompt.value, "aß");
        prompt.cursor = 0;
        prompt.delete();
        assert_eq!(prompt.value, "ß");
        prompt.cursor = 1;
        prompt.insert('!');
        assert_eq!(prompt.value, "ß!");
    }

    #[test]
    fn extension_detection_is_cheap() {
        assert!(App::supported_by_extension(Path::new("photo.jpg")));
        assert!(App::supported_by_extension(Path::new("doc.pdf")));
        assert!(!App::supported_by_extension(Path::new("notes.xyz")));
    }
}
