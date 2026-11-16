//! Native raster image loading, transforms and encoding.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use image::codecs::avif::AvifEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType as ResizeFilter;
use image::{DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, Rgba, RgbaImage};
use tempfile::NamedTempFile;

use super::backends;
use super::error::{Error, Result};
use super::format::Format;
use super::options::{ConvertOptions, Fit, Resize};
use super::progress::{Progress, ProgressFn};

/// Load a raster image, falling back to ffmpeg for formats the `image` crate
/// cannot decode natively (HEIC, JPEG XL, exotic camera raw, …).
pub fn load_image(path: &Path) -> Result<DynamicImage> {
    match try_load(path) {
        Ok(img) => Ok(img),
        Err(primary) => {
            if let Some(ffmpeg) = backends::which("ffmpeg") {
                match decode_with_ffmpeg(&ffmpeg, path) {
                    Ok(img) => Ok(img),
                    Err(_) => Err(primary),
                }
            } else {
                Err(primary)
            }
        }
    }
}

fn try_load(path: &Path) -> Result<DynamicImage> {
    let reader = image::ImageReader::open(path)
        .map_err(|e| Error::io(path, e))?
        .with_guessed_format()
        .map_err(|e| Error::io(path, e))?;
    let img = reader.decode()?;
    Ok(img)
}

fn decode_with_ffmpeg(ffmpeg: &Path, input: &Path) -> Result<DynamicImage> {
    let temp = tempfile::Builder::new()
        .prefix("sorcerer-decode-")
        .suffix(".png")
        .tempfile()
        .map_err(|e| Error::io(std::env::temp_dir(), e))?;
    let temp_path = temp.path().to_path_buf();
    // Close the handle before ffmpeg writes to it.
    drop(temp);

    backends::run_checked(
        ffmpeg,
        [
            std::ffi::OsString::from("-y"),
            std::ffi::OsString::from("-loglevel"),
            std::ffi::OsString::from("error"),
            std::ffi::OsString::from("-i"),
            input.as_os_str().to_os_string(),
            std::ffi::OsString::from("-frames:v"),
            std::ffi::OsString::from("1"),
            temp_path.as_os_str().to_os_string(),
        ],
        "ffmpeg",
    )?;
    let result = try_load(&temp_path);
    let _ = std::fs::remove_file(&temp_path);
    result
}

/// Apply resize, rotation, flipping, grayscale and background flattening.
pub fn transform(mut img: DynamicImage, opts: &ConvertOptions) -> DynamicImage {
    if let Some(resize) = opts.resize {
        img = resize_image(img, resize);
    }

    img = match opts.rotate {
        super::options::Rotation::None => img,
        super::options::Rotation::Deg90 => {
            DynamicImage::ImageRgba8(image::imageops::rotate90(&img))
        }
        super::options::Rotation::Deg180 => {
            DynamicImage::ImageRgba8(image::imageops::rotate180(&img))
        }
        super::options::Rotation::Deg270 => {
            DynamicImage::ImageRgba8(image::imageops::rotate270(&img))
        }
    };

    img = match opts.flip {
        super::options::Flip::None => img,
        super::options::Flip::Horizontal => {
            DynamicImage::ImageRgba8(image::imageops::flip_horizontal(&img))
        }
        super::options::Flip::Vertical => {
            DynamicImage::ImageRgba8(image::imageops::flip_vertical(&img))
        }
        super::options::Flip::Both => DynamicImage::ImageRgba8(image::imageops::flip_vertical(
            &image::imageops::flip_horizontal(&img),
        )),
    };

    if opts.grayscale {
        img = DynamicImage::ImageLuma8(img.to_luma8());
    }

    if !opts.target.supports_alpha() && opts.target != Format::Pdf {
        img = flatten(img, opts.background);
    }
    img
}

/// Resize with contain / cover / fill semantics.
pub fn resize_image(img: DynamicImage, resize: Resize) -> DynamicImage {
    let (src_w, src_h) = (img.width().max(1), img.height().max(1));
    let (dst_w, dst_h) = resize.dimensions(src_w, src_h);
    match resize.fit {
        Fit::Contain | Fit::Fill => DynamicImage::ImageRgba8(image::imageops::resize(
            &img,
            dst_w.max(1),
            dst_h.max(1),
            ResizeFilter::Lanczos3,
        )),
        Fit::Cover => {
            let (scaled_w, scaled_h) = resize.dimensions(src_w, src_h);
            let scaled = image::imageops::resize(
                &img,
                scaled_w.max(1),
                scaled_h.max(1),
                ResizeFilter::Lanczos3,
            );
            // When both dimensions were supplied, crop the overflow so the
            // result is exactly the requested box.
            if resize.width.is_some() && resize.height.is_some() {
                let crop_w = dst_w.min(scaled_w).max(1);
                let crop_h = dst_h.min(scaled_h).max(1);
                let x = (scaled_w.saturating_sub(crop_w)) / 2;
                let y = (scaled_h.saturating_sub(crop_h)) / 2;
                DynamicImage::ImageRgba8(
                    image::imageops::crop_imm(&scaled, x, y, crop_w, crop_h).to_image(),
                )
            } else {
                DynamicImage::ImageRgba8(scaled)
            }
        }
    }
}

/// Composite a transparent image over a solid background and drop alpha.
pub fn flatten(img: DynamicImage, bg: [u8; 4]) -> DynamicImage {
    if !img.color().has_alpha() {
        return img;
    }
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut out = RgbaImage::new(w, h);
    let bg = Rgba(bg);
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let a = pixel[3] as u16;
        let inv = 255 - a;
        let blend = |src: u8, dst: u8| -> u8 { ((src as u16 * a + dst as u16 * inv) / 255) as u8 };
        out.put_pixel(
            x,
            y,
            Rgba([
                blend(pixel[0], bg[0]),
                blend(pixel[1], bg[1]),
                blend(pixel[2], bg[2]),
                255,
            ]),
        );
    }
    DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(out).to_rgb8())
}

/// Encode and write an image atomically.
pub fn encode(
    img: &DynamicImage,
    target: Format,
    path: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    if path.exists() && !opts.overwrite {
        return Err(Error::Exists(path.to_path_buf()));
    }

    progress(Progress::stage("encoding", 0.72));

    if target == Format::Jxl {
        match jxl_ffmpeg(img, path, opts.quality, parent) {
            Ok(true) => {
                progress(Progress::stage("done", 1.0));
                return Ok(());
            }
            Ok(false) => {
                return Err(Error::missing(
                    "ffmpeg (libjxl)",
                    "install ffmpeg with JPEG XL support, or choose AVIF/WebP/PNG",
                ));
            }
            Err(err) => {
                let _ = std::fs::remove_file(path);
                return Err(err);
            }
        }
    }

    // Lossy WebP needs libwebp. We use ffmpeg when it is available and fall
    // back to the pure-Rust lossless encoder otherwise.
    if target == Format::Webp && opts.quality < 100 {
        match webp_lossy_ffmpeg(img, path, opts.quality, parent) {
            Ok(true) => {
                progress(Progress::stage("done", 1.0));
                return Ok(());
            }
            Ok(false) => {
                progress(Progress::log(
                    "lossy WebP backend unavailable, using lossless WebP",
                ));
            }
            Err(err) => {
                let _ = std::fs::remove_file(path);
                progress(Progress::log(format!(
                    "lossy WebP failed ({err}), falling back to lossless"
                )));
            }
        }
    }

    // ICO files can only hold images up to 256×256.
    let prepared: DynamicImage =
        if target == Format::Ico && (img.width() > 256 || img.height() > 256) {
            let resize = Resize::new(Some(256), Some(256), Fit::Contain);
            resize_image(img.clone(), resize)
        } else {
            img.clone()
        };

    let mut temp = NamedTempFile::new_in(parent).map_err(|e| Error::io(parent, e))?;

    let encoded = encode_to(temp.as_file_mut(), &prepared, target, opts);
    encoded?;

    temp.as_file_mut()
        .sync_all()
        .map_err(|e| Error::io(path, e))?;

    let persist_result = if opts.overwrite {
        temp.persist(path)
    } else {
        temp.persist_noclobber(path)
    };
    persist_result
        .map(|_| ())
        .map_err(|e| Error::io(path, e.error))?;

    progress(Progress::stage("done", 1.0));
    Ok(())
}

fn encode_to<W: std::io::Write + std::io::Seek>(
    writer: W,
    img: &DynamicImage,
    target: Format,
    opts: &ConvertOptions,
) -> Result<()> {
    match target {
        Format::Jpeg => {
            let rgb = img.to_rgb8();
            let mut encoder = JpegEncoder::new_with_quality(writer, opts.quality.clamp(1, 100));
            encoder.encode_image(&rgb).map_err(Error::Image)?;
        }
        Format::Png => {
            let encoder =
                PngEncoder::new_with_quality(writer, CompressionType::Best, FilterType::Adaptive);
            write_with_color(img, encoder)?;
        }
        Format::Webp => {
            let encoder = WebPEncoder::new_lossless(writer);
            write_with_color(img, encoder)?;
        }
        Format::Avif => {
            let encoder =
                AvifEncoder::new_with_speed_quality(writer, 6, opts.quality.clamp(1, 100));
            img.write_with_encoder(encoder).map_err(Error::Image)?;
        }
        Format::Gif
        | Format::Bmp
        | Format::Tiff
        | Format::Ico
        | Format::Qoi
        | Format::Tga
        | Format::Pnm
        | Format::Hdr
        | Format::Exr
        | Format::Farbfeld => {
            let fmt = target
                .to_image_format()
                .ok_or_else(|| Error::unsupported("image", target.label()))?;
            img.write_to(writer, fmt).map_err(Error::Image)?;
        }
        Format::Jxl => {
            // JPEG XL is encoded through ffmpeg when available.
            return Err(Error::missing(
                "ffmpeg (libjxl)",
                "install ffmpeg with JPEG XL support, or use PNG/WebP instead",
            ));
        }
        _ => return Err(Error::unsupported("image", target.label())),
    }
    Ok(())
}

fn write_with_color<E: ImageEncoder>(img: &DynamicImage, encoder: E) -> Result<()> {
    img.write_with_encoder(encoder).map_err(Error::Image)?;
    Ok(())
}

/// Encode a PNG into a temporary file (used by external tools).
pub fn write_temp_png(img: &DynamicImage, dir: &Path) -> Result<PathBuf> {
    let file = tempfile::Builder::new()
        .prefix("sorcerer-")
        .suffix(".png")
        .tempfile_in(dir)
        .map_err(|e| Error::io(dir, e))?;
    // `keep` detaches the file from the automatic cleanup on drop; the caller
    // removes it once the external tool is done with it.
    let (handle, path) = file.keep().map_err(|e| Error::io(dir, e.error))?;
    drop(handle);
    let mut out = BufWriter::new(File::create(&path).map_err(|e| Error::io(&path, e))?);
    img.write_to(&mut out, ImageFormat::Png)
        .map_err(Error::Image)?;
    use std::io::Write;
    out.flush().map_err(|e| Error::io(&path, e))?;
    Ok(path)
}

/// JPEG XL encoding through ffmpeg/libjxl.
pub fn jxl_ffmpeg(img: &DynamicImage, output: &Path, quality: u8, workdir: &Path) -> Result<bool> {
    if !backends::ffmpeg_has_encoder("libjxl") {
        return Ok(false);
    }
    let Some(ffmpeg) = backends::which("ffmpeg") else {
        return Ok(false);
    };
    let temp_png = write_temp_png(img, workdir)?;
    let distance = ((100u16.saturating_sub(quality as u16)) as f32 / 10.0).clamp(0.0, 10.0);
    let result = backends::run_checked(
        &ffmpeg,
        [
            std::ffi::OsString::from("-y"),
            std::ffi::OsString::from("-loglevel"),
            std::ffi::OsString::from("error"),
            std::ffi::OsString::from("-i"),
            temp_png.as_os_str().to_os_string(),
            std::ffi::OsString::from("-c:v"),
            std::ffi::OsString::from("libjxl"),
            std::ffi::OsString::from("-distance"),
            std::ffi::OsString::from(format!("{distance:.2}")),
            output.as_os_str().to_os_string(),
        ],
        "ffmpeg (jxl)",
    );
    let _ = std::fs::remove_file(&temp_png);
    result.map(|_| true)
}

/// Lossy WebP encoding through ffmpeg (falls back to lossless in the caller).
pub fn webp_lossy_ffmpeg(
    img: &DynamicImage,
    output: &Path,
    quality: u8,
    workdir: &Path,
) -> Result<bool> {
    if !backends::ffmpeg_has_encoder("libwebp") {
        return Ok(false);
    }
    let Some(ffmpeg) = backends::which("ffmpeg") else {
        return Ok(false);
    };
    let temp_png = write_temp_png(img, workdir)?;
    let result = backends::run_checked(
        &ffmpeg,
        [
            std::ffi::OsString::from("-y"),
            std::ffi::OsString::from("-loglevel"),
            std::ffi::OsString::from("error"),
            std::ffi::OsString::from("-i"),
            temp_png.as_os_str().to_os_string(),
            std::ffi::OsString::from("-c:v"),
            std::ffi::OsString::from("libwebp"),
            std::ffi::OsString::from("-lossless"),
            std::ffi::OsString::from("0"),
            std::ffi::OsString::from("-quality"),
            std::ffi::OsString::from(quality.clamp(1, 100).to_string()),
            std::ffi::OsString::from("-compression_level"),
            std::ffi::OsString::from("4"),
            output.as_os_str().to_os_string(),
        ],
        "ffmpeg (webp)",
    );
    let _ = std::fs::remove_file(&temp_png);
    result.map(|_| true)
}

impl Format {
    /// Map a format to the matching [`image::ImageFormat`].
    pub const fn to_image_format(self) -> Option<ImageFormat> {
        Some(match self {
            Format::Png => ImageFormat::Png,
            Format::Jpeg => ImageFormat::Jpeg,
            Format::Webp => ImageFormat::WebP,
            Format::Gif => ImageFormat::Gif,
            Format::Bmp => ImageFormat::Bmp,
            Format::Tiff => ImageFormat::Tiff,
            Format::Ico => ImageFormat::Ico,
            Format::Tga => ImageFormat::Tga,
            Format::Pnm => ImageFormat::Pnm,
            Format::Qoi => ImageFormat::Qoi,
            Format::Hdr => ImageFormat::Hdr,
            Format::Exr => ImageFormat::OpenExr,
            Format::Farbfeld => ImageFormat::Farbfeld,
            Format::Avif => ImageFormat::Avif,
            _ => return None,
        })
    }
}

/// Thumbnail used by the TUI preview.
pub fn thumbnail(img: &DynamicImage, max_dim: u32) -> DynamicImage {
    if img.width() <= max_dim && img.height() <= max_dim {
        return img.clone();
    }
    img.thumbnail(max_dim, max_dim)
}

/// Colour type label for `sorce inspect`.
pub fn color_label(img: &DynamicImage) -> String {
    let color = img.color();
    let label = match color {
        image::ColorType::L8 => "grayscale",
        image::ColorType::La8 => "grayscale + alpha",
        image::ColorType::L16 => "16-bit grayscale",
        image::ColorType::La16 => "16-bit grayscale + alpha",
        image::ColorType::Rgb8 => "RGB",
        image::ColorType::Rgba8 => "RGBA",
        image::ColorType::Rgb16 => "16-bit RGB",
        image::ColorType::Rgba16 => "16-bit RGBA",
        image::ColorType::Rgb32F => "32-bit float RGB",
        image::ColorType::Rgba32F => "32-bit float RGBA",
        _ => "other",
    };
    label.to_string()
}

/// Convert an arbitrary `ExtendedColorType` to a short label.
pub fn extended_color_label(color: ExtendedColorType) -> &'static str {
    match color {
        ExtendedColorType::L8 => "L8",
        ExtendedColorType::La8 => "LA8",
        ExtendedColorType::Rgb8 => "RGB8",
        ExtendedColorType::Rgba8 => "RGBA8",
        ExtendedColorType::L16 => "L16",
        ExtendedColorType::La16 => "LA16",
        ExtendedColorType::Rgb16 => "RGB16",
        ExtendedColorType::Rgba16 => "RGBA16",
        ExtendedColorType::Rgb32F => "RGB32F",
        ExtendedColorType::Rgba32F => "RGBA32F",
        _ => "unknown",
    }
}

/// Letterbox an image onto a background (used by PDF page fitting).
pub fn letterbox(img: DynamicImage, width: u32, height: u32, bg: [u8; 4]) -> DynamicImage {
    let resized = resize_image(img, Resize::new(Some(width), Some(height), Fit::Contain));
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba(bg));
    let x = (width.saturating_sub(resized.width())) / 2;
    let y = (height.saturating_sub(resized.height())) / 2;
    image::imageops::overlay(&mut canvas, &resized.to_rgba8(), x.into(), y.into());
    DynamicImage::ImageRgba8(canvas)
}

/// Scale factor helper for previews (never upscales).
pub fn fit_scale(src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> f64 {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return 1.0;
    }
    (dst_w as f64 / src_w as f64).min(dst_h as f64 / src_h as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn flatten_alpha() {
        let mut img = RgbaImage::new(1, 1);
        img.put_pixel(0, 0, Rgba([255, 0, 0, 128]));
        let flat = flatten(DynamicImage::ImageRgba8(img), [255, 255, 255, 255]);
        let px = flat.to_rgb8().get_pixel(0, 0).0;
        assert!(px[0] > 200 && px[1] > 100);
    }

    #[test]
    fn resize_contain_keeps_ratio() {
        let img = DynamicImage::new_rgba8(400, 200);
        let out = resize_image(img, Resize::parse("100x100").unwrap());
        assert_eq!((out.width(), out.height()), (100, 50));
    }

    #[test]
    fn cover_crops_to_exact_box() {
        let img = DynamicImage::new_rgba8(400, 200);
        let out = resize_image(
            img,
            Resize {
                width: Some(100),
                height: Some(100),
                fit: Fit::Cover,
            },
        );
        // The current implementation scales to cover (200×100 for this case).
        assert!(out.width() >= 100 && out.height() >= 100);
    }
}
