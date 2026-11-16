//! SVG rendering powered by resvg / usvg / tiny-skia.

use std::path::Path;

use image::{DynamicImage, RgbaImage};

use super::error::{Error, Result};

/// Render an SVG file to a raster image at the requested DPI.
///
/// 96 DPI is the SVG "native" resolution; `dpi` scales the output accordingly.
pub fn render_svg(input: &Path, dpi: u32) -> Result<DynamicImage> {
    let data = std::fs::read(input).map_err(|e| Error::io(input, e))?;
    let opt = usvg::Options {
        resources_dir: input.parent().map(|p| p.to_path_buf()),
        dpi: 96.0,
        ..usvg::Options::default()
    };

    let tree = usvg::Tree::from_data(&data, &opt).map_err(|e| Error::Svg(e.to_string()))?;
    let size = tree.size();
    let scale = dpi.max(1) as f32 / 96.0;
    let width = (size.width() * scale).ceil().max(1.0) as u32;
    let height = (size.height() * scale).ceil().max(1.0) as u32;

    if width > 32_768 || height > 32_768 {
        return Err(Error::message(format!(
            "refusing to render an SVG larger than 32768 px (requested {width}×{height}); lower --dpi"
        )));
    }
    if width.saturating_mul(height) > 180_000_000 {
        return Err(Error::message(format!(
            "refusing to render an SVG with {width}×{height} pixels; lower --dpi or resize first"
        )));
    }

    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or_else(|| {
        Error::message(format!("could not allocate a {width}×{height} SVG canvas"))
    })?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    let bytes = pixmap.data();
    let buffer = RgbaImage::from_raw(width, height, bytes.to_vec())
        .ok_or_else(|| Error::message("resvg returned an unexpected pixel buffer size"))?;
    Ok(DynamicImage::ImageRgba8(buffer))
}

/// Basic SVG dimensions (in CSS pixels at 96 DPI).
pub fn svg_size(input: &Path) -> Result<(u32, u32)> {
    let data = std::fs::read(input).map_err(|e| Error::io(input, e))?;
    let opt = usvg::Options {
        resources_dir: input.parent().map(|p| p.to_path_buf()),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(&data, &opt).map_err(|e| Error::Svg(e.to_string()))?;
    let size = tree.size();
    Ok((
        size.width().round().max(1.0) as u32,
        size.height().round().max(1.0) as u32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn renders_simple_svg() {
        let mut file = tempfile::Builder::new().suffix(".svg").tempfile().unwrap();
        write!(
            file,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#
        )
        .unwrap();
        let img = render_svg(file.path(), 96).unwrap();
        assert_eq!((img.width(), img.height()), (20, 10));
    }
}
