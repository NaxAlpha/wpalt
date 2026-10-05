//! Optional local FFmpeg worker. Fixed command arguments, single-file input,
//! protocol restrictions and bounded output/time. Host OS process isolation is
//! still required when the operator accepts untrusted codecs.
use crate::{
    App, backup,
    error::{Error, Result},
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::process::Command;
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.enabled {
            anyhow::ensure!(
                self.ffmpeg.is_absolute() && self.ffprobe.is_absolute(),
                "video requires absolute owner-installed ffmpeg and ffprobe paths"
            );
        }
        Ok(())
    }
}
struct Workspace(PathBuf);
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn process(command: &mut Command) -> Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|_| Error::invalid("Cannot start the configured video worker."))?;
    match tokio::time::timeout(Duration::from_secs(20), child.wait()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(_) => Err(Error::invalid(
            "Video processing failed; unsupported or damaged source.",
        )),
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(Error::invalid(
                "Video worker exceeded its twenty-second deadline.",
            ))
        }
    }
}
pub async fn transcode(app: &App, source: &[u8]) -> Result<Vec<u8>> {
    let started = Instant::now();
    let config = &app.config.video;
    if !config.enabled {
        return Err(Error::invalid("Local video processing is disabled."));
    }
    if source.is_empty() || source.len() > app.config.max_upload_bytes.min(32 * 1024 * 1024) {
        return Err(Error::invalid(
            "Video source exceeds its configured upload limit.",
        ));
    }
    let _permit = app
        .media_work
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::invalid("Media workers are busy; try again shortly."))?;
    tracing::debug!(event = "video_worker_admitted", source_bytes = source.len());
    let dir = app
        .config
        .data_dir
        .join(format!(".video-{}", uuid::Uuid::new_v4()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700).create(&dir)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir(&dir)?;
    }
    let workspace = Workspace(dir);
    let input = workspace.0.join("source");
    let output = workspace.0.join("output.mp4");
    let report = workspace.0.join("probe.json");
    backup::write_private(&input, source)
        .map_err(|_| Error::invalid("Cannot create private video source."))?;
    let report_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&report)?;
    let mut probe = Command::new(&config.ffprobe);
    probe
        .args([
            "-v",
            "error",
            "-max_alloc",
            "67108864",
            "-protocol_whitelist",
            "file",
            "-format_whitelist",
            "mov,matroska,webm",
            "-show_entries",
            "stream=codec_type,width,height:format=duration",
            "-of",
            "json",
        ])
        .arg(&input)
        .stdin(Stdio::null())
        .stdout(Stdio::from(report_file))
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = probe
        .spawn()
        .map_err(|_| Error::invalid("Cannot start configured video inspection."))?;
    match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
        Ok(Ok(status)) if status.success() => (),
        _ => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(Error::invalid("Video inspection failed or timed out."));
        }
    }
    tracing::debug!(
        event = "video_inspection_completed",
        elapsed_us = started.elapsed().as_micros() as u64
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&backup::read_bounded(&report, 16 * 1024).await?)
            .map_err(|_| Error::invalid("Invalid video inspection."))?;
    let duration = metadata["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|n| n.is_finite() && *n > 0. && *n <= 120.)
        .ok_or_else(|| Error::invalid("Video duration must be known and at most two minutes."))?;
    let streams = metadata["streams"]
        .as_array()
        .ok_or_else(|| Error::invalid("Missing video streams."))?;
    if streams.len() > 8
        || !streams.iter().any(|s| s["codec_type"] == "video")
        || streams
            .iter()
            .filter(|s| s["codec_type"] == "video")
            .any(|s| {
                !matches!(s["width"].as_u64(), Some(1..=1920))
                    || !matches!(s["height"].as_u64(), Some(1..=1080))
            })
    {
        return Err(Error::invalid(
            "Video sources support up to eight streams and 1920×1080 frames.",
        ));
    }
    let mut encoder = Command::new(&config.ffmpeg);
    encoder.args(["-v","error","-nostdin","-n","-max_alloc","67108864","-threads","1","-filter_threads","1","-protocol_whitelist","file","-format_whitelist","mov,matroska,webm","-i"])
        .arg(&input).args(["-map","0:v:0","-map","0:a:0?","-map_metadata","-1","-map_chapters","-1","-vf","scale=w='min(1280,iw)':h='min(720,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2,setsar=1","-r","30","-c:v","libx264","-preset","veryfast","-crf","28","-pix_fmt","yuv420p","-threads","1","-c:a","aac","-b:a","96k","-t"])
        .arg(duration.to_string()).args(["-fs","33554432","-movflags","+faststart","-f","mp4"]).arg(&output);
    process(&mut encoder).await?;
    let bytes = backup::read_bounded(&output, 32 * 1024 * 1024).await?;
    if bytes.len() < 16 || bytes.len() >= 32 * 1024 * 1024 || &bytes[4..8] != b"ftyp" {
        return Err(Error::invalid("Video output failed its bounded MP4 check."));
    }
    tracing::debug!(
        event = "video_worker_completed",
        output_bytes = bytes.len(),
        elapsed_us = started.elapsed().as_micros() as u64
    );
    Ok(bytes)
}
