//! Bounded native image processing. Originals remain authoritative; transformed
//! output is delivered only after the original media authorization boundary.
use crate::error::{Error, Result};
use std::io::Cursor;

pub fn derivative(bytes: &[u8], width: u32) -> Result<Vec<u8>> {
    if ![320, 640, 1280, 1920].contains(&width) || bytes.len() > 32 * 1024 * 1024 {
        return Err(Error::invalid(
            "Choose a supported image width: 320, 640, 1280 or 1920.",
        ));
    }
    let format = image::guess_format(bytes).map_err(|_| Error::invalid("Unsupported image."))?;
    // Animated sources keep their original animation; no silent first-frame conversion.
    if format == image::ImageFormat::Gif {
        return Err(Error::invalid(
            "Animated GIF derivatives are unsupported; use the original.",
        ));
    }
    if ![
        image::ImageFormat::Png,
        image::ImageFormat::Jpeg,
        image::ImageFormat::WebP,
    ]
    .contains(&format)
    {
        return Err(Error::invalid("Unsupported image."));
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| Error::invalid("Image exceeds processing limits or is damaged."))?;
    let target = width.min(image.width());
    let height = ((image.height() as u64 * target as u64) / image.width() as u64).max(1) as u32;
    let resized = if target == image.width() {
        image
    } else {
        image.resize_exact(target, height, image::imageops::FilterType::Triangle)
    };
    let mut output = Cursor::new(Vec::new());
    resized
        .write_to(&mut output, image::ImageFormat::WebP)
        .map_err(|_| Error::invalid("Image encoding failed."))?;
    Ok(output.into_inner())
}
