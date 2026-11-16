# Changelog

All notable changes to Sorcerer are documented here.

## [0.1.0] — 2026-11-16

### Added

- Initial release of **Sorcerer** (`sorce`) by **Panz-08**.
- Full-screen ratatui interface with file browser, image/PDF/SVG/text/video
  previews, settings panel, conversion queue, and progress reporting.
- Native conversion engine:
  - raster images via the `image` crate (decode, resize, rotate, flip,
    grayscale, background flattening, encode);
  - SVG rendering through resvg / usvg / tiny-skia;
  - dependency-free PDF writer for images and text;
  - Markdown, HTML, and plain-text conversion.
- External backend integration:
  - Poppler for PDF rendering, text extraction, merging, and splitting;
  - Ghostscript and MuPDF as PDF render fallbacks;
  - LibreOffice headless for office document conversions;
  - ffmpeg / ffprobe for video, audio, GIF, and frame extraction.
- CLI commands: `convert`, `batch`, `merge`, `inspect`, `formats`, `doctor`,
  `tui`, and `completions`.
- Bare invocation (`sorce photo.jpg -o photo.png`) is treated as `convert`.
- Parallel batch conversion with configurable worker count.
- PDF page selection (`all`, `1`, `2-5`, `1,3,7-9`, `10-`).
- Output collision protection, dry-run planning, JSON inspection output, and
  shell completion generation.
- MIT license.

[0.1.0]: https://github.com/Panz-08/sorcerer/releases/tag/v0.1.0
