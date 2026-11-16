//! Audio / video conversion and frame extraction through ffmpeg.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::backends;
use super::error::{Error, Result};
use super::format::Format;
use super::image_ops;
use super::options::ConvertOptions;
use super::progress::{Progress, ProgressFn};

/// Metadata extracted with ffprobe.
#[derive(Debug, Clone, Default)]
pub struct MediaInfo {
    pub duration_secs: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub fps: Option<f64>,
}

/// Probe a media file. Returns `None` when ffprobe is unavailable or fails.
pub fn info(path: &Path) -> Option<MediaInfo> {
    let ffprobe = backends::which("ffprobe")?;
    let output = backends::run(
        ffprobe,
        [
            OsString::from("-v"),
            OsString::from("error"),
            OsString::from("-show_entries"),
            OsString::from(
                "format=duration:stream=width,height,codec_name,codec_type,r_frame_rate",
            ),
            OsString::from("-of"),
            OsString::from("json"),
            path.as_os_str().to_os_string(),
        ],
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let json: Value = serde_json::from_slice(&output.stdout).ok()?;
    let mut info = MediaInfo::default();
    if let Some(duration) = json
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|d| d.parse::<f64>().ok())
    {
        info.duration_secs = Some(duration);
    }
    if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
        for stream in streams {
            let kind = stream.get("codec_type").and_then(|v| v.as_str());
            let codec = stream
                .get("codec_name")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            match kind {
                Some("video") => {
                    if info.video_codec.is_none() {
                        info.video_codec = codec;
                        info.width = stream
                            .get("width")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u32);
                        info.height = stream
                            .get("height")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u32);
                        info.fps = stream
                            .get("r_frame_rate")
                            .and_then(|v| v.as_str())
                            .and_then(parse_rate);
                    }
                }
                Some("audio") if info.audio_codec.is_none() => {
                    info.audio_codec = codec;
                }
                _ => {}
            }
        }
    }
    Some(info)
}

fn parse_rate(rate: &str) -> Option<f64> {
    let (num, den) = rate.split_once('/')?;
    let num: f64 = num.parse().ok()?;
    let den: f64 = den.parse().ok()?;
    (den != 0.0).then_some(num / den)
}

/// Convert video/audio to another media format, or extract a frame.
pub fn convert_media(
    input: &Path,
    output: &Path,
    source: Format,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    if opts.target.is_raster() {
        return extract_frame(input, output, opts, progress);
    }
    if !(opts.target.is_media() || opts.target == Format::Gif) {
        return Err(Error::unsupported(source.label(), opts.target.label()));
    }

    let (_, ffmpeg) = backends::require(&["ffmpeg"], "install ffmpeg: sudo apt install ffmpeg")?;
    if output.exists() && !opts.overwrite {
        return Err(Error::Exists(output.to_path_buf()));
    }
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    progress(Progress::stage("converting media", 0.2));
    let mut args: Vec<OsString> = vec![
        "-y".into(),
        "-nostdin".into(),
        "-loglevel".into(),
        "error".into(),
        "-i".into(),
        input.as_os_str().to_os_string(),
    ];
    args.extend(media_arguments(opts.target, opts));
    args.push(output.as_os_str().to_os_string());

    let result = backends::run_checked(&ffmpeg, args, "ffmpeg");
    if let Err(err) = result {
        let _ = std::fs::remove_file(output);
        return Err(err);
    }
    progress(Progress::stage("done", 1.0));
    Ok(vec![output.to_path_buf()])
}

fn media_arguments(target: Format, opts: &ConvertOptions) -> Vec<OsString> {
    let crf = (51.0 - opts.quality_fraction() * 45.0)
        .round()
        .clamp(0.0, 51.0);
    let vp9_crf = (63.0 - opts.quality_fraction() * 50.0)
        .round()
        .clamp(0.0, 63.0);
    let mut args: Vec<OsString> = Vec::new();
    match target {
        Format::Mp4 | Format::Mkv | Format::Mov => {
            args.extend(strings([
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                &crf.to_string(),
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
            ]));
            if target == Format::Mp4 {
                args.extend(strings(["-movflags", "+faststart"]));
            }
        }
        Format::Webm => {
            args.extend(strings([
                "-c:v",
                "libvpx-vp9",
                "-crf",
                &vp9_crf.to_string(),
                "-b:v",
                "0",
                "-c:a",
                "libopus",
                "-b:a",
                "128k",
            ]));
        }
        Format::Avi => {
            args.extend(strings([
                "-c:v",
                "mpeg4",
                "-q:v",
                &(31.0 - opts.quality_fraction() * 27.0).round().to_string(),
                "-c:a",
                "libmp3lame",
                "-q:a",
                "3",
            ]));
        }
        Format::Gif => {
            let fps = opts.fps.clamp(1.0, 60.0);
            let width = opts
                .resize
                .and_then(|r| r.width)
                .unwrap_or(960)
                .clamp(16, 1920);
            let filter = format!(
                "fps={fps},scale='min({width},iw)':-1:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5"
            );
            args.extend(strings(["-vf", &filter, "-loop", "0"]));
        }
        Format::Mp3 => {
            let q = (9.0 - opts.quality_fraction() * 8.0)
                .round()
                .clamp(0.0, 9.0);
            args.extend(strings([
                "-vn",
                "-c:a",
                "libmp3lame",
                "-q:a",
                &q.to_string(),
            ]));
        }
        Format::Wav => args.extend(strings(["-vn", "-c:a", "pcm_s16le"])),
        Format::Ogg => {
            let q = (opts.quality_fraction() * 10.0).round().clamp(0.0, 10.0);
            args.extend(strings([
                "-vn",
                "-c:a",
                "libvorbis",
                "-q:a",
                &q.to_string(),
            ]));
        }
        Format::Flac => args.extend(strings(["-vn", "-c:a", "flac", "-compression_level", "8"])),
        Format::Aac | Format::M4a => {
            args.extend(strings([
                "-vn",
                "-c:a",
                "aac",
                "-b:a",
                &aac_bitrate(opts.quality),
            ]));
        }
        Format::Opus => {
            args.extend(strings([
                "-vn",
                "-c:a",
                "libopus",
                "-b:a",
                &opus_bitrate(opts.quality),
            ]));
        }
        _ => {}
    }
    args
}

fn aac_bitrate(quality: u8) -> String {
    let kbps = 64 + (quality as u32).min(100) * 2;
    format!("{kbps}k")
}

fn opus_bitrate(quality: u8) -> String {
    let kbps = 48 + (quality as u32).min(100);
    format!("{kbps}k")
}

fn strings<const N: usize>(items: [&str; N]) -> Vec<OsString> {
    items.into_iter().map(OsString::from).collect()
}

/// Extract one frame from a video and encode it as an image.
pub fn extract_frame(
    input: &Path,
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    let (_, ffmpeg) = backends::require(&["ffmpeg"], "install ffmpeg: sudo apt install ffmpeg")?;
    if output.exists() && !opts.overwrite {
        return Err(Error::Exists(output.to_path_buf()));
    }
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    progress(Progress::stage("extracting frame", 0.25));
    let temp = tempfile::Builder::new()
        .prefix("sorcerer-frame-")
        .suffix(".png")
        .tempfile()
        .map_err(|e| Error::io(std::env::temp_dir(), e))?;
    let temp_path = temp.path().to_path_buf();

    let seek = format!("{:.3}", opts.seek.max(0.0));
    let args: Vec<OsString> = vec![
        "-y".into(),
        "-nostdin".into(),
        "-loglevel".into(),
        "error".into(),
        "-ss".into(),
        seek.into(),
        "-i".into(),
        input.as_os_str().to_os_string(),
        "-frames:v".into(),
        "1".into(),
        "-update".into(),
        "1".into(),
        temp_path.as_os_str().to_os_string(),
    ];
    backends::run_checked(&ffmpeg, args, "ffmpeg")?;

    if !temp_path.exists() {
        return Err(Error::message(
            "ffmpeg could not extract a frame (is the seek position valid?)",
        ));
    }
    let mut img = image_ops::load_image(&temp_path)?;
    img = image_ops::transform(img, opts);
    image_ops::encode(&img, opts.target, output, opts, progress)?;
    progress(Progress::stage("done", 1.0));
    Ok(vec![output.to_path_buf()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frame_rate() {
        assert_eq!(parse_rate("30000/1001"), Some(29.97002997002997));
        assert_eq!(parse_rate("25/1"), Some(25.0));
        assert_eq!(parse_rate("bad"), None);
    }
}
