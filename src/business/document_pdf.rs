//! Optional fixed local Chromium worker. OS policy supplies memory/network isolation.
use crate::{
    App, backup,
    error::{Error, Result},
};
use std::{path::PathBuf, process::Stdio, time::Duration};
struct Workspace(PathBuf);
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Worker(tokio::process::Child);
impl Drop for Worker {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(id) = self
            .0
            .id()
            .and_then(|id| rustix::process::Pid::from_raw(id as i32))
        {
            let _ = rustix::process::kill_process_group(id, rustix::process::Signal::KILL);
        }
    }
}
pub async fn render(app: &App, html: &[u8]) -> Result<Vec<u8>> {
    if !app.config.documents.pdf_enabled {
        return Err(Error::invalid("Local PDF document rendering is disabled."));
    }
    if html.is_empty() || html.len() > super::documents::MAX_ARTIFACT {
        return Err(Error::invalid("PDF source exceeds its byte limit."));
    }
    let path = app
        .config
        .data_dir
        .join(format!(".document-{}", uuid::Uuid::new_v4()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&path)?;
    let workspace = Workspace(std::fs::canonicalize(&path)?);
    let input = workspace.0.join("source.html");
    let output = workspace.0.join("document.pdf");
    backup::write_private(&input, html)
        .map_err(|_| Error::invalid("Cannot create the private PDF source."))?;
    let mut command = tokio::process::Command::new(&app.config.documents.chromium);
    command
        .env_clear()
        .env("HOME", &workspace.0)
        .args([
            "--headless",
            "--disable-gpu",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-sync",
            "--no-pdf-header-footer",
        ])
        .arg(format!(
            "--user-data-dir={}",
            workspace.0.join("profile").display()
        ))
        .arg(format!("--print-to-pdf={}", output.display()))
        .arg(
            url::Url::from_file_path(std::fs::canonicalize(&input)?)
                .map_err(|_| Error::invalid("Invalid local PDF source path."))?
                .as_str(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    let mut worker = Worker(
        command
            .spawn()
            .map_err(|_| Error::invalid("Cannot start the configured local PDF renderer."))?,
    );
    let status = match tokio::time::timeout(Duration::from_secs(20), worker.0.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(_)) => return Err(Error::invalid("Local PDF renderer failed.")),
        Err(_) => {
            return Err(Error::invalid(
                "Local PDF rendering exceeded its twenty-second deadline.",
            ));
        }
    };
    if !status.success() {
        return Err(Error::invalid(
            "Local PDF renderer failed. Check its installation and host isolation policy.",
        ));
    }
    let bytes = backup::read_bounded(&output, super::documents::MAX_ARTIFACT).await?;
    if !bytes.starts_with(b"%PDF-") {
        return Err(Error::invalid(
            "The local renderer did not produce a PDF document.",
        ));
    }
    Ok(bytes)
}
