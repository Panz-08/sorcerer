//! `sorce` — Sorcerer's command line entry point.

mod cli;

use std::collections::{BTreeSet, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use clap::Parser;
use clap_complete::generate;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use cli::{
    BatchArgs, Cli, Command, ConvertArgs, ConvertFlags, FormatsArgs, InspectArgs, MergeArgs,
    TuiArgs,
};
use sorcerer::core::batch::{BatchSpec, JobProgressFn, Plan, Summary, build_plan, execute_plan};
use sorcerer::core::format::Format;
use sorcerer::core::inspect_file;
use sorcerer::core::options::{
    ConvertOptions, Fit, Flip, PageSelection, PageSize, Resize, Rotation, parse_color,
};
use sorcerer::core::progress::Progress;
use sorcerer::util::{human, paths, term};

fn main() {
    let mut args: Vec<OsString> = std::env::args_os().collect();
    if needs_convert(&args) {
        args.insert(1, OsString::from("convert"));
    }
    let cli = Cli::parse_from(args);
    if let Err(err) = dispatch(cli) {
        eprintln!("{} {err}", term::red("error:"));
        let mut source = err.source();
        while let Some(cause) = source {
            eprintln!("  {} {cause}", term::dim("cause:"));
            source = cause.source();
        }
        std::process::exit(1);
    }
}

/// Allow `sorce photo.jpg -o photo.png` as a shorthand for `sorce convert …`.
fn needs_convert(args: &[OsString]) -> bool {
    let Some(first) = args.get(1).and_then(|arg| arg.to_str()) else {
        return false;
    };
    if first.starts_with('-') {
        return false;
    }
    const SUBCOMMANDS: &[&str] = &[
        "convert",
        "c",
        "conv",
        "batch",
        "b",
        "merge",
        "inspect",
        "info",
        "formats",
        "list",
        "doctor",
        "doc",
        "tui",
        "ui",
        "completions",
        "complete",
        "help",
    ];
    !SUBCOMMANDS.contains(&first)
}

fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        None => sorcerer::tui::run(None),
        Some(Command::Convert(args)) => cmd_convert(args),
        Some(Command::Batch(args)) => cmd_batch(args),
        Some(Command::Merge(args)) => cmd_merge(args),
        Some(Command::Inspect(args)) => cmd_inspect(args),
        Some(Command::Formats(args)) => cmd_formats(args),
        Some(Command::Doctor) => cmd_doctor(),
        Some(Command::Tui(args)) => cmd_tui(args),
        Some(Command::Completions { shell }) => {
            let mut command = <Cli as clap::CommandFactory>::command();
            generate(shell, &mut command, "sorce", &mut std::io::stdout());
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Target / options
// ---------------------------------------------------------------------------

fn resolve_target(to: Option<Format>, output: Option<&Path>, hint: &str) -> Result<Format> {
    if let Some(target) = to {
        return Ok(target);
    }
    if let Some(output) = output
        && let Some(ext) = output.extension().and_then(|e| e.to_str())
        && let Some(format) = Format::from_extension(ext)
    {
        return Ok(format);
    }
    anyhow::bail!("{hint}")
}

fn build_options(flags: &ConvertFlags, target: Format) -> Result<ConvertOptions> {
    let mut resize = match flags.resize.as_deref() {
        Some(spec) => Some(Resize::parse(spec)?),
        None => None,
    };
    if let Some(resize) = resize.as_mut() {
        resize.fit = flags.fit.parse::<Fit>()?;
    }
    let opts = ConvertOptions {
        target,
        quality: flags.quality.unwrap_or_else(|| target.default_quality()),
        dpi: flags.dpi,
        pages: PageSelection::parse(&flags.pages)?,
        resize,
        rotate: Rotation::from_degrees(flags.rotate)?,
        flip: flags.flip.parse::<Flip>()?,
        grayscale: flags.grayscale,
        background: parse_color(&flags.background)?,
        overwrite: flags.overwrite,
        page_size: flags.page_size.parse::<PageSize>()?,
        margin_mm: flags.margin,
        fps: flags.fps,
        seek: flags.seek,
    };
    opts.validate()?;
    Ok(opts)
}

fn threads(requested: usize) -> usize {
    if requested == 0 {
        num_cpus::get().max(1)
    } else {
        requested
    }
}

// ---------------------------------------------------------------------------
// convert / batch
// ---------------------------------------------------------------------------

fn cmd_convert(args: ConvertArgs) -> Result<()> {
    let target = resolve_target(
        args.flags.to,
        args.flags.output.as_deref(),
        "specify a target with --to <FORMAT> or an output name with a known extension",
    )?;
    let opts = build_options(&args.flags, target)?;

    let inputs = paths::expand_inputs(&args.inputs, args.recursive, &args.from)
        .context("could not collect input files")?;
    let spec = BatchSpec {
        target,
        output: args.flags.output.clone(),
        merge_pdf: false,
        overwrite: opts.overwrite,
        preserve_tree: args.preserve_tree,
    };
    let plan = build_plan(&inputs, &spec).context("could not plan the conversion")?;
    run_plan(plan, opts, &args.flags)
}

fn cmd_batch(args: BatchArgs) -> Result<()> {
    let output = args.flags.output.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("converted")
    });
    let target = resolve_target(
        args.flags.to,
        Some(&output),
        "batch mode needs a target: pass --to <FORMAT> (the default output folder has no extension)",
    )?;
    let opts = build_options(&args.flags, target)?;

    let mut inputs = paths::expand_inputs(&args.paths, !args.no_recursive, &args.from)
        .context("could not collect input files")?;
    // Never scan our own output directory.
    let output_abs = absolute(&output);
    inputs.retain(|input| !absolute(input).starts_with(&output_abs));

    if inputs.is_empty() {
        anyhow::bail!("no matching files found in {}", args.paths.join(", "));
    }

    let spec = BatchSpec {
        target,
        output: Some(output.clone()),
        merge_pdf: false,
        overwrite: opts.overwrite,
        preserve_tree: !args.flat,
    };
    let plan = build_plan(&inputs, &spec).context("could not plan the batch")?;
    run_plan(plan, opts, &args.flags)
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn cmd_merge(args: MergeArgs) -> Result<()> {
    let inputs = paths::expand_inputs(&args.inputs, args.recursive, &[])
        .context("could not collect input files")?;
    if inputs.is_empty() {
        anyhow::bail!("nothing to merge");
    }
    let output = paths::with_extension(&args.output, Format::Pdf);
    let opts = ConvertOptions {
        target: Format::Pdf,
        quality: args.quality,
        dpi: args.dpi,
        page_size: args.page_size.parse::<PageSize>()?,
        margin_mm: args.margin,
        overwrite: args.overwrite,
        ..ConvertOptions::default()
    };
    opts.validate()?;

    if args.dry_run {
        println!(
            "{} {}",
            term::bold("merge plan"),
            term::dim(&format!("({})", sorcerer::version_line()))
        );
        for input in &inputs {
            println!("  {} {}", term::dim("+"), paths::pretty(input));
        }
        println!("  {} {}", term::green("→"), paths::pretty(&output));
        return Ok(());
    }

    let plan = Plan::Merge { inputs, output };
    let flags = ConvertFlags {
        quality: Some(opts.quality),
        dpi: opts.dpi,
        overwrite: opts.overwrite,
        ..default_flags()
    };
    run_plan(plan, opts, &flags)
}

fn default_flags() -> ConvertFlags {
    ConvertFlags {
        to: None,
        output: None,
        quality: None,
        dpi: 150,
        pages: "all".to_string(),
        resize: None,
        fit: "contain".to_string(),
        rotate: 0,
        flip: "none".to_string(),
        grayscale: false,
        background: "white".to_string(),
        overwrite: false,
        page_size: "fit".to_string(),
        margin: 12.0,
        fps: 12.0,
        seek: 0.0,
        jobs: 0,
        dry_run: false,
        quiet: false,
    }
}

fn run_plan(plan: Plan, opts: ConvertOptions, flags: &ConvertFlags) -> Result<()> {
    if flags.dry_run {
        print_plan(&plan, opts.target);
        return Ok(());
    }

    let labels: Vec<String> = match &plan {
        Plan::Individual(jobs) => jobs
            .iter()
            .map(|job| {
                job.input
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| job.input.display().to_string())
            })
            .collect(),
        Plan::Merge { inputs, .. } => vec![format!("merge {} files", inputs.len())],
    };

    let progress = Arc::new(CliProgress::new(&labels, flags.quiet));
    let callback_progress = progress.clone();
    let on_progress: JobProgressFn = Arc::new(move |index, event| {
        callback_progress.handle(index, event);
    });

    let summary = execute_plan(&plan, &opts, threads(flags.jobs), &on_progress);
    progress.finish();
    print_summary(&summary, flags.quiet);
    if summary.failed() > 0 {
        anyhow::bail!("{} conversion(s) failed", summary.failed());
    }
    Ok(())
}

fn print_plan(plan: &Plan, target: Format) {
    println!(
        "{} {}",
        term::bold("sorcerer · dry run"),
        term::dim(&format!("({})", sorcerer::version_line()))
    );
    match plan {
        Plan::Individual(jobs) => {
            for job in jobs {
                let source = Format::from_path(&job.input)
                    .map(|f| sorcerer::core::describe(f, target))
                    .unwrap_or_else(|_| "unknown input".to_string());
                println!(
                    "  {} {}  {}  {}",
                    term::cyan("→"),
                    paths::pretty(&job.input),
                    term::dim("=>"),
                    paths::pretty(&job.output)
                );
                println!("      {}", term::dim(&source));
            }
            println!(
                "\n  {} · target {}",
                human::files(jobs.len()),
                term::bold(target.label())
            );
        }
        Plan::Merge { inputs, output } => {
            for input in inputs {
                println!("  {} {}", term::dim("+"), paths::pretty(input));
            }
            println!(
                "  {} {}  {}",
                term::cyan("="),
                paths::pretty(output),
                term::dim("(PDF merge)")
            );
            println!("\n  {} → one PDF", human::files(inputs.len()));
        }
    }
}

struct CliProgress {
    mp: MultiProgress,
    overall: ProgressBar,
    bars: Vec<ProgressBar>,
    completed: Mutex<HashSet<usize>>,
    total: usize,
    quiet: bool,
    per_job: bool,
}

impl CliProgress {
    fn new(labels: &[String], quiet: bool) -> Self {
        let visible = !quiet && term::stderr_colors();
        let mp = MultiProgress::with_draw_target(if visible {
            ProgressDrawTarget::stderr()
        } else {
            ProgressDrawTarget::hidden()
        });
        let total = labels.len();
        let per_job = total <= 12;
        let overall = mp.add(ProgressBar::new(total as u64));
        overall.set_style(
            ProgressStyle::with_template(
                "{spinner:.magenta} {msg} [{bar:32.magenta/blue}] {pos}/{len}",
            )
            .unwrap()
            .progress_chars("█▓░"),
        );
        overall.set_message("sorcerer");
        let mut bars = Vec::new();
        if per_job {
            for label in labels {
                let bar = mp.add(ProgressBar::new(100));
                bar.set_style(
                    ProgressStyle::with_template(
                        "{spinner:.cyan} {msg:.52} {bar:20.cyan/blue} {pos:>3}%",
                    )
                    .unwrap()
                    .progress_chars("█▓░"),
                );
                bar.set_message(label.clone());
                bars.push(bar);
            }
        }
        Self {
            mp,
            overall,
            bars,
            completed: Mutex::new(HashSet::new()),
            total,
            quiet,
            per_job,
        }
    }

    fn handle(&self, index: usize, event: Progress) {
        match event {
            Progress::Stage { text, fraction } => {
                if let Some(fraction) = fraction {
                    if let Some(bar) = self.bars.get(index) {
                        bar.set_position((fraction.clamp(0.0, 1.0) * 100.0) as u64);
                    }
                    if fraction >= 0.999 {
                        self.mark_done(index);
                    }
                }
                if let Some(bar) = self.bars.get(index) {
                    let name = bar.message().split(" · ").next().unwrap_or("").to_string();
                    bar.set_message(format!("{name} · {text}"));
                } else if !self.quiet {
                    self.overall.set_message(format!(
                        "{} · {}",
                        index + 1,
                        human::truncate(&text, 40)
                    ));
                }
            }
            Progress::Page { done, total } => {
                if let Some(bar) = self.bars.get(index) {
                    let name = bar.message().split(" · ").next().unwrap_or("").to_string();
                    bar.set_message(format!("{name} · page {done}/{total}"));
                }
            }
            Progress::Log(message) => {
                if !self.quiet && self.per_job {
                    let _ = self.mp.println(format!("  {} {message}", term::dim("·")));
                }
            }
        }
    }

    fn mark_done(&self, index: usize) {
        if let Ok(mut completed) = self.completed.lock()
            && completed.insert(index)
        {
            self.overall.inc(1);
        }
    }

    fn finish(&self) {
        for bar in &self.bars {
            bar.finish_and_clear();
        }
        self.overall.finish_and_clear();
        let _ = &self.total;
    }
}

fn print_summary(summary: &Summary, quiet: bool) {
    let succeeded = summary.succeeded();
    let failed = summary.failed();
    let elapsed = human::duration(summary.duration);

    if succeeded == 1 && summary.results.len() == 1 && !quiet {
        let result = &summary.results[0];
        for output in &result.outputs {
            println!("{} {}", term::green("✓"), paths::pretty(output));
        }
        if let Some(first) = result.outputs.first() {
            let size = std::fs::metadata(first).map(|m| m.len()).unwrap_or(0);
            println!(
                "  {} → {} in {}",
                human::bytes(result.input_bytes),
                human::bytes(size),
                elapsed
            );
        }
    } else if !quiet {
        println!(
            "{} {} converted · {} → {} · {}",
            term::green("✓"),
            succeeded,
            human::bytes(summary.input_bytes()),
            human::bytes(summary.output_bytes()),
            elapsed
        );
    }

    if failed > 0 {
        println!("{} {} failed", term::red("✗"), failed);
        for result in summary.results.iter().filter(|r| !r.ok()) {
            println!("  {} {}", term::red("×"), paths::pretty(&result.input));
            if let Some(error) = &result.error {
                for line in error.lines() {
                    println!("      {}", term::dim(line));
                }
            }
        }
    } else if quiet {
        println!(
            "{} {} converted in {}",
            term::green("✓"),
            succeeded,
            elapsed
        );
    }
}

// ---------------------------------------------------------------------------
// inspect
// ---------------------------------------------------------------------------

fn cmd_inspect(args: InspectArgs) -> Result<()> {
    let files = expand_any(&args.files, args.recursive)?;
    let mut infos = Vec::new();
    for file in &files {
        match inspect_file(file) {
            Ok(info) => infos.push(info),
            Err(err) => eprintln!("{} {}: {err}", term::yellow("!"), file.display()),
        }
    }
    if args.json {
        let json = serde_json::to_string_pretty(&infos)?;
        println!("{json}");
        return Ok(());
    }
    if infos.is_empty() {
        anyhow::bail!("nothing to inspect");
    }

    for (index, info) in infos.iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("{}", term::bold(&info.name));
        println!(
            "  {:<11} {} ({})",
            term::dim("format"),
            info.format,
            info.mime
        );
        println!("  {:<11} {}", term::dim("category"), info.category);
        println!("  {:<11} {}", term::dim("size"), human::bytes(info.size));
        if let (Some(w), Some(h)) = (info.width, info.height) {
            println!("  {:<11} {w}×{h}", term::dim("dimensions"));
        }
        if let Some(pages) = info.pages {
            println!("  {:<11} {pages}", term::dim("pages"));
        }
        if let Some(duration) = info.duration_secs {
            println!("  {:<11} {:.2} s", term::dim("duration"), duration);
        }
        if let Some(codec) = &info.video_codec {
            println!("  {:<11} {codec}", term::dim("video"));
        }
        if let Some(codec) = &info.audio_codec {
            println!("  {:<11} {codec}", term::dim("audio"));
        }
        if let Some(color) = &info.color {
            println!("  {:<11} {color}", term::dim("colour"));
        }
        if let (Some(lines), Some(words)) = (info.lines, info.words) {
            println!("  {:<11} {lines} lines · {words} words", term::dim("text"));
        }
        if let Some(modified) = info.modified {
            println!(
                "  {:<11} {}",
                term::dim("modified"),
                human::unix_datetime(modified)
            );
        }
        println!("  {:<11} {}", term::dim("path"), paths::pretty(&info.path));
    }
    Ok(())
}

fn expand_any(raw: &[String], recursive: bool) -> Result<Vec<PathBuf>> {
    let mut out = BTreeSet::new();
    for item in raw {
        if item.contains('*') || item.contains('?') || item.contains('[') {
            for entry in glob::glob(item).with_context(|| format!("invalid glob `{item}`"))? {
                let path = entry?;
                if path.is_dir() {
                    collect_any_dir(&path, recursive, &mut out);
                } else if path.is_file() {
                    out.insert(path);
                }
            }
        } else {
            let path = PathBuf::from(item);
            if path.is_dir() {
                collect_any_dir(&path, recursive, &mut out);
            } else if path.is_file() {
                out.insert(path);
            }
        }
    }
    if out.is_empty() {
        anyhow::bail!("no files found");
    }
    Ok(out.into_iter().collect())
}

fn collect_any_dir(dir: &Path, recursive: bool, out: &mut BTreeSet<PathBuf>) {
    if recursive {
        for entry in walkdir::WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                out.insert(entry.into_path());
            }
        }
    } else if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                out.insert(entry.path());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// formats / doctor / tui
// ---------------------------------------------------------------------------

fn cmd_formats(args: FormatsArgs) -> Result<()> {
    let all = Format::all();
    if args.json {
        let rows: Vec<serde_json::Value> = all
            .iter()
            .map(|f| {
                serde_json::json!({
                    "format": f.label(),
                    "extension": f.extension(),
                    "category": f.category().label(),
                    "mime": f.mime(),
                    "description": f.description(),
                    "targets": f.targets_from().iter().map(|t| t.label()).collect::<Vec<_>>(),
                    "external": matches!(f, Format::Heic | Format::Jxl),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    println!(
        "{} {}",
        term::bold("sorcerer formats"),
        term::dim(&format!("· {} supported", all.len()))
    );
    for category in ["Image", "Vector", "Document", "Text", "Media"] {
        let formats: Vec<Format> = all
            .iter()
            .copied()
            .filter(|f| f.category().label().eq_ignore_ascii_case(category))
            .collect();
        if formats.is_empty() {
            continue;
        }
        println!("\n{}", term::cyan(&category.to_uppercase()));
        for format in formats {
            let external = match format {
                Format::Heic | Format::Jxl => term::yellow(" (needs ffmpeg)"),
                f if f.is_office() || f == Format::Csv => term::yellow(" (needs LibreOffice)"),
                _ => String::new(),
            };
            println!(
                "  {:<9} {:<6} {}{}",
                term::bold(format.label()),
                ".".to_string() + format.extension(),
                format.description(),
                external
            );
        }
    }
    println!(
        "\n{} use {} to see what each format can become.",
        term::dim("tip:"),
        term::bold("sorce formats --json")
    );
    Ok(())
}

fn cmd_doctor() -> Result<()> {
    println!("{}", term::banner());
    println!("{}", term::bold("built-in engine"));
    for (name, purpose) in [
        (
            "image codecs",
            "JPEG, PNG, WebP, GIF, TIFF, BMP, ICO, QOI, TGA, HDR, EXR, AVIF",
        ),
        ("SVG renderer", "resvg + tiny-skia"),
        ("PDF writer", "images, text and multi-page documents"),
        ("ratatui UI", "interactive converter"),
    ] {
        println!("  {} {:<15} {}", term::green("✓"), name, term::dim(purpose));
    }

    println!("\n{}", term::bold("external backends"));
    let statuses = sorcerer::core::backends::doctor();
    let mut missing = 0usize;
    for status in &statuses {
        match &status.path {
            Some(path) => println!(
                "  {} {:<15} {}",
                term::green("✓"),
                status.name,
                term::dim(&paths::pretty(path))
            ),
            None => {
                missing += 1;
                println!(
                    "  {} {:<15} {}",
                    term::yellow("✗"),
                    status.name,
                    term::dim(status.purpose)
                );
            }
        }
    }
    println!("\n{}", term::bold("ffmpeg codecs"));
    for encoder in [
        "libwebp",
        "libjxl",
        "libx264",
        "libvpx-vp9",
        "libmp3lame",
        "libopus",
    ] {
        let found = sorcerer::core::backends::ffmpeg_has_encoder(encoder);
        println!(
            "  {} {:<12} {}",
            if found {
                term::green("✓")
            } else {
                term::yellow("✗")
            },
            encoder,
            if found {
                term::dim("available")
            } else {
                term::dim("missing")
            }
        );
    }

    println!(
        "\n{} {} backend(s) missing. Images, SVG, PDF and text work without any of them.",
        term::dim("note:"),
        missing
    );
    Ok(())
}

fn cmd_tui(args: TuiArgs) -> Result<()> {
    sorcerer::tui::run_with(args.path, args.hidden, args.jobs)
}

#[cfg(test)]
mod shim_tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    #[test]
    fn bare_files_become_convert() {
        assert!(needs_convert(&args(&["sorce", "photo.jpg"])));
        assert!(needs_convert(&args(&["sorce", "*.png", "-o", "out"])));
    }

    #[test]
    fn commands_and_flags_are_left_alone() {
        assert!(!needs_convert(&args(&["sorce"])));
        assert!(!needs_convert(&args(&["sorce", "convert", "a.jpg"])));
        assert!(!needs_convert(&args(&["sorce", "tui"])));
        assert!(!needs_convert(&args(&["sorce", "--version"])));
    }
}
