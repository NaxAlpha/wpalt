//! Bounded native image processing. Originals remain authoritative; transformed
//! output is delivered only after the original media authorization boundary.
use crate::error::{Error, Result};
use std::io::Cursor;

pub fn derivative(bytes: &[u8], width: u32) -> Result<Vec<u8>> {
    derivative_format(bytes, width, "webp")
}
pub fn derivative_format(bytes: &[u8], width: u32, format: &str) -> Result<Vec<u8>> {
    if !["webp", "avif"].contains(&format) {
        return Err(Error::invalid("Unsupported derivative format."));
    }
    if ![320, 640, 1280, 1920].contains(&width) || bytes.len() > 32 * 1024 * 1024 {
        return Err(Error::invalid(
            "Choose a supported image width: 320, 640, 1280 or 1920.",
        ));
    }
    if format == "avif" && width > 1280 {
        return Err(Error::invalid("AVIF supports widths 320, 640 and 1280."));
    }
    let source_format =
        image::guess_format(bytes).map_err(|_| Error::invalid("Unsupported image."))?;
    // Animated sources keep their original animation; no silent first-frame conversion.
    if source_format == image::ImageFormat::Gif {
        return Err(Error::invalid(
            "Animated GIF derivatives are unsupported; use the original.",
        ));
    }
    if ![
        image::ImageFormat::Png,
        image::ImageFormat::Jpeg,
        image::ImageFormat::WebP,
    ]
    .contains(&source_format)
    {
        return Err(Error::invalid("Unsupported image."));
    }
    let mut inspection_limits = image::Limits::default();
    inspection_limits.max_image_width = Some(4096);
    inspection_limits.max_image_height = Some(4096);
    inspection_limits.max_alloc = Some(64 * 1024 * 1024);
    let animated = match source_format {
        image::ImageFormat::Png => {
            image::codecs::png::PngDecoder::with_limits(Cursor::new(bytes), inspection_limits)
                .and_then(|decoder| decoder.is_apng())
        }
        image::ImageFormat::WebP => image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))
            .map(|decoder| decoder.has_animation()),
        _ => Ok(false),
    }
    .map_err(|_| Error::invalid("Image header is damaged or exceeds limits."))?;
    if animated {
        return Err(Error::invalid(
            "Animated derivatives are unsupported; use the original.",
        ));
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), source_format);
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
    if format == "avif" {
        if target > 1280 || u64::from(target) * u64::from(height) > 2_000_000 {
            return Err(Error::invalid(
                "AVIF derivatives are limited to width 1280 and two million pixels.",
            ));
        }
        resized
            .write_with_encoder(
                image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut output, 10, 60)
                    .with_num_threads(Some(1)),
            )
            .map_err(|_| Error::invalid("AVIF encoding failed."))?;
    } else {
        resized
            .write_to(&mut output, image::ImageFormat::WebP)
            .map_err(|_| Error::invalid("Image encoding failed."))?;
    }
    Ok(output.into_inner())
}

/// Separate bounded derivative storage; never stores authentication decisions.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub cache_bytes: usize,
    pub cache_entries: usize,
    pub cache_seconds: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            cache_bytes: 8 * 1024 * 1024,
            cache_entries: 128,
            cache_seconds: 3600,
        }
    }
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.cache_bytes <= 64 * 1024 * 1024,
            "media cache_bytes must be 0..64 MiB"
        );
        anyhow::ensure!(
            (1..=1024).contains(&self.cache_entries),
            "media cache_entries must be 1..1024"
        );
        anyhow::ensure!(
            (1..=3600).contains(&self.cache_seconds),
            "media cache_seconds must be 1..3600"
        );
        Ok(())
    }
    pub fn storage(&self) -> super::cache::Config {
        super::cache::Config {
            enabled: self.cache_bytes > 0,
            max_bytes: self.cache_bytes,
            max_entries: self.cache_entries,
            ttl_seconds: self.cache_seconds,
            ..Default::default()
        }
    }
}

/// Header-only local layout inspection: at most 128 × 64 KiB, no remote fetch or
/// pixel decode. Cache contains metadata, never an authorization decision.
pub async fn dimensions(
    app: &crate::App,
    sources: Vec<(String, String, String)>,
) -> std::collections::BTreeMap<String, (u32, u32)> {
    use axum::{
        body::{Bytes, to_bytes},
        http::HeaderMap,
    };
    use std::collections::BTreeMap;
    let start = std::time::Instant::now();
    let storage = app.config.media.storage();
    let mut resolved = BTreeMap::new();
    let mut pending = Vec::new();
    for (id, filename, sha) in sources.into_iter().take(128) {
        if !crate::backup::safe_filename(&filename) || sha.len() != 64 {
            continue;
        }
        let key = format!("dimensions:{id}:{sha}");
        let hit = if storage.enabled {
            app.media_cache.lock().await.get(&key, 0, &storage)
        } else {
            None
        };
        if let Some(response) = hit
            && let Ok(bytes) = to_bytes(response.into_body(), 8).await
            && let Ok(bytes) = <[u8; 8]>::try_from(bytes.as_ref())
        {
            let width = u32::from_le_bytes(bytes[..4].try_into().unwrap());
            let height = u32::from_le_bytes(bytes[4..].try_into().unwrap());
            resolved.insert(id, (width, height));
        } else {
            pending.push((id, filename, key));
        }
    }
    if pending.is_empty() {
        tracing::debug!(
            event = "media_layout_cache",
            images = resolved.len(),
            elapsed_us = start.elapsed().as_micros() as u64
        );
        return resolved;
    }
    let Ok(permit) = app.media_work.clone().try_acquire_owned() else {
        tracing::debug!(
            event = "media_layout_deferred",
            reason = "worker_busy",
            images = resolved.len()
        );
        return resolved;
    };
    let directory = app.config.data_dir.join("media");
    let inspected = tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let _permit = permit;
        let mut inspected = Vec::new();
        for (id, filename, key) in pending {
            let path = directory.join(filename);
            if !std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
                continue;
            }
            let Ok(file) = std::fs::File::open(&path) else {
                continue;
            };
            let mut bytes = Vec::new();
            if file.take(64 * 1024).read_to_end(&mut bytes).is_err() {
                continue;
            }
            let Ok(mut reader) = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()
            else {
                continue;
            };
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(4096);
            limits.max_image_height = Some(4096);
            limits.max_alloc = Some(2 * 1024 * 1024);
            reader.limits(limits);
            if let Ok((width, height)) = reader.into_dimensions()
                && (1..=4096).contains(&width)
                && (1..=4096).contains(&height)
            {
                inspected.push((id, key, width, height));
            }
        }
        inspected
    })
    .await
    .unwrap_or_default();
    for (id, key, width, height) in inspected {
        resolved.insert(id, (width, height));
        if storage.enabled {
            let mut bytes = Vec::with_capacity(8);
            bytes.extend(width.to_le_bytes());
            bytes.extend(height.to_le_bytes());
            app.media_cache.lock().await.insert(
                key,
                0,
                Bytes::from(bytes),
                HeaderMap::new(),
                &storage,
            );
        }
    }
    tracing::debug!(
        event = "media_layout_inspected",
        images = resolved.len(),
        elapsed_us = start.elapsed().as_micros() as u64
    );
    resolved
}
