//! Updates from GitHub Releases. With `auto_update` on, release builds check in the
//! background and download quietly; the new version installs when the user restarts.

use std::sync::Mutex;
use std::time::Duration;

use anyhow::{bail, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::{local_ai, AppState};

/// Leaves startup (model loading, the local AI warming up) to finish first.
const FIRST_CHECK: Duration = Duration::from_secs(60);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

struct Downloaded {
    update: Update,
    bytes: Vec<u8>,
}

#[derive(Default)]
pub struct Updates {
    ready: Mutex<Option<Downloaded>>,
    /// Held while checking or downloading, so the timer and the Check button don't overlap.
    busy: tokio::sync::Mutex<()>,
}

/// A downloaded update, waiting for a restart.
#[derive(Serialize, Clone)]
pub struct Available {
    pub version: String,
    pub notes: String,
}

impl Updates {
    pub fn ready(&self) -> Option<Available> {
        self.ready.lock().unwrap().as_ref().map(|d| Available {
            version: d.update.version.clone(),
            notes: d.update.body.clone().unwrap_or_default(),
        })
    }
}

/// Checks soon after startup and then every few hours, while `auto_update` is on.
/// Dev builds don't check: they'd offer to replace themselves with the release.
pub fn spawn_checker(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            if auto_update(&app) {
                if let Err(e) = check(&app).await {
                    log::warn!("update check failed: {e}");
                }
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

fn auto_update(app: &AppHandle) -> bool {
    app.state::<AppState>().settings.read().unwrap().auto_update
}

/// Looks for a newer release and downloads it. Returns it once it's ready to install.
pub async fn check(app: &AppHandle) -> Result<Option<Available>> {
    let updates = app.state::<Updates>();
    let _busy = updates.busy.lock().await;
    if let Some(ready) = updates.ready() {
        return Ok(Some(ready));
    }
    let Some(update) = app.updater()?.check().await? else {
        return Ok(None);
    };
    log::info!("downloading Lipwise {}", update.version);
    let bytes = update.download(|_, _| {}, || {}).await?;
    *updates.ready.lock().unwrap() = Some(Downloaded { update, bytes });
    let ready = updates.ready();
    let _ = app.emit("update-ready", &ready);
    Ok(ready)
}

/// Installs the downloaded update and restarts into it.
pub fn install(app: &AppHandle) -> Result<()> {
    let updates = app.state::<Updates>();
    let Some(downloaded) = updates.ready.lock().unwrap().take() else {
        bail!("No update has been downloaded yet.");
    };
    // On Windows the installer ends this process without running the exit handlers.
    local_ai::stop();
    if let Err(e) = downloaded.update.install(&downloaded.bytes) {
        *updates.ready.lock().unwrap() = Some(downloaded);
        return Err(e.into());
    }
    app.restart()
}
