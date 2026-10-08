//! Resumable, integrity-checked file downloads shared by speech models and the local AI setup.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Copy)]
pub enum Stage {
    Downloading,
    Verifying,
}

pub fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .user_agent(concat!("Lipwise/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

/// Downloads `total` bytes into `dest`, trying each URL in turn (later ones are
/// fallbacks). A partial file is resumed. When `sha256` is given the result is
/// verified before being moved into place.
pub async fn fetch_verified(
    urls: &[String],
    dest: &Path,
    total: u64,
    sha256: Option<&str>,
    cancel: &AtomicBool,
    progress: impl Fn(Stage, u64, u64),
) -> Result<()> {
    if dest.is_file() {
        return Ok(());
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let name = dest.file_name().and_then(|n| n.to_str()).unwrap_or("download");
    let part = dest.with_file_name(format!("{name}.part"));
    let client = client()?;

    let mut errors = Vec::new();
    for url in urls {
        log::info!("downloading {url}");
        match fetch(&client, url, &part, total, cancel, &progress).await {
            Ok(()) => break,
            Err(e) if cancel.load(Ordering::Relaxed) => return Err(e),
            Err(e) => {
                log::warn!("download from {url} failed: {e}");
                errors.push(e.to_string());
            }
        }
    }
    if errors.len() == urls.len() {
        bail!("Download failed: {}", errors.join("; "));
    }

    if let Some(expected) = sha256 {
        progress(Stage::Verifying, total, total);
        let path = part.clone();
        let actual = tauri::async_runtime::spawn_blocking(move || sha256_file(&path)).await??;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&part);
            bail!("Downloaded file failed its integrity check; please retry");
        }
    }
    std::fs::rename(&part, dest)?;
    Ok(())
}

/// Downloads `url` into `part`, resuming from whatever is already there.
async fn fetch(
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    total: u64,
    cancel: &AtomicBool,
    progress: &impl Fn(Stage, u64, u64),
) -> Result<()> {
    let mut have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    if have > total {
        std::fs::remove_file(part)?;
        have = 0;
    }
    if have == total {
        return Ok(());
    }

    let mut req = client.get(url);
    if have > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let resp = req.send().await?.error_for_status()?;
    let resumed = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let mut out = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(part)
        .await?;
    if !resumed {
        have = 0;
    }

    let mut stream = resp.bytes_stream();
    let mut last_emit = Instant::now();
    loop {
        let next = tokio::time::timeout(Duration::from_secs(60), stream.next())
            .await
            .map_err(|_| anyhow!("Download stalled"))?;
        let Some(chunk) = next else { break };
        if cancel.load(Ordering::Relaxed) {
            out.flush().await?;
            bail!("Cancelled");
        }
        let chunk = chunk?;
        out.write_all(&chunk).await?;
        have += chunk.len() as u64;
        if last_emit.elapsed() > Duration::from_millis(250) {
            progress(Stage::Downloading, have, total);
            last_emit = Instant::now();
        }
    }
    out.flush().await?;
    if have != total {
        bail!("Download ended early ({have} of {total} bytes)");
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}
