//! Speech-model catalog and downloads.
//!
//! `catalog.json` is Handy's model catalog (github.com/cjpais/Handy, MIT): every
//! entry is a GGUF file in the `handy-computer` Hugging Face org that
//! transcribe.cpp can run. Files download from Hugging Face at the pinned
//! revision, falling back to Handy's static mirror, and are verified against the
//! catalog's sha256 before use.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::download;

#[derive(Deserialize)]
struct CatalogRoot {
    #[serde(default)]
    mirrors: Vec<String>,
    models: Vec<CatalogModel>,
}

#[derive(Deserialize)]
struct CatalogModel {
    id: String,
    revision: String,
    name: String,
    description: String,
    architecture: String,
    languages: Vec<String>,
    capabilities: Capabilities,
    speed_score: Option<f32>,
    accuracy_score: Option<f32>,
    files: Vec<QuantFile>,
    default_quant: String,
    #[serde(default)]
    recommended: bool,
    recommended_rank: Option<u32>,
}

#[derive(Deserialize)]
struct Capabilities {
    streaming: bool,
    translate: bool,
}

#[derive(Deserialize)]
struct QuantFile {
    filename: String,
    quant: String,
    size_bytes: u64,
    sha256: Option<String>,
}

static CATALOG: LazyLock<CatalogRoot> = LazyLock::new(|| {
    serde_json::from_str(include_str!("catalog.json")).expect("bundled catalog.json is valid")
});

impl CatalogModel {
    fn file(&self) -> &QuantFile {
        self.files
            .iter()
            .find(|f| f.quant == self.default_quant)
            .unwrap_or(&self.files[0])
    }
}

fn find(id: &str) -> Result<&'static CatalogModel> {
    CATALOG
        .models
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| anyhow!("Unknown model: {id}"))
}

#[derive(Serialize, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub architecture: String,
    pub languages: Vec<String>,
    pub size_bytes: u64,
    pub speed: f32,
    pub accuracy: f32,
    pub recommended: bool,
    pub rank: u32,
    pub streaming: bool,
    pub translate: bool,
    pub downloaded: bool,
}

pub fn models_dir(app_data: &Path) -> PathBuf {
    app_data.join("models")
}

/// Where a catalog model's file lives once downloaded.
pub fn model_path(app_data: &Path, id: &str) -> Result<PathBuf> {
    let m = find(id)?;
    Ok(models_dir(app_data)
        .join(id.replace('/', "__"))
        .join(&m.file().filename))
}

pub fn is_downloaded(app_data: &Path, id: &str) -> bool {
    model_path(app_data, id).map(|p| p.is_file()).unwrap_or(false)
}

pub fn list(app_data: &Path) -> Vec<ModelInfo> {
    let mut models: Vec<ModelInfo> = CATALOG
        .models
        .iter()
        .map(|m| ModelInfo {
            id: m.id.clone(),
            name: m.name.clone(),
            description: m.description.clone(),
            architecture: m.architecture.clone(),
            languages: m.languages.clone(),
            size_bytes: m.file().size_bytes,
            speed: m.speed_score.unwrap_or(0.0),
            accuracy: m.accuracy_score.unwrap_or(0.0),
            recommended: m.recommended,
            rank: m.recommended_rank.unwrap_or(u32::MAX),
            streaming: m.capabilities.streaming,
            translate: m.capabilities.translate,
            downloaded: is_downloaded(app_data, &m.id),
        })
        .collect();
    models.sort_by_key(|m| m.rank);
    models
}

pub fn delete(app_data: &Path, id: &str) -> Result<()> {
    let path = model_path(app_data, id)?;
    if let Some(dir) = path.parent() {
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
    }
    Ok(())
}

#[derive(Serialize, Clone)]
struct Progress<'a> {
    id: &'a str,
    stage: &'a str,
    downloaded: u64,
    total: u64,
    error: Option<String>,
}

/// In-flight downloads, keyed by model id, holding their cancel flags.
#[derive(Default)]
pub struct Downloads(Mutex<HashMap<String, Arc<AtomicBool>>>);

impl Downloads {
    pub fn cancel(&self, id: &str) {
        if let Some(flag) = self.0.lock().unwrap().get(id) {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

pub async fn download(app: AppHandle, downloads: &Downloads, app_data: PathBuf, id: String) -> Result<()> {
    let cancel = {
        let mut map = downloads.0.lock().unwrap();
        if map.contains_key(&id) {
            bail!("Already downloading");
        }
        let flag = Arc::new(AtomicBool::new(false));
        map.insert(id.clone(), flag.clone());
        flag
    };
    let result = download_inner(&app, &app_data, &id, &cancel).await;
    downloads.0.lock().unwrap().remove(&id);

    let (stage, error) = match &result {
        Ok(()) => ("done", None),
        Err(_) if cancel.load(Ordering::Relaxed) => ("cancelled", None),
        Err(e) => ("error", Some(e.to_string())),
    };
    emit(&app, &id, stage, 0, 0, error);
    result
}

async fn download_inner(app: &AppHandle, app_data: &Path, id: &str, cancel: &AtomicBool) -> Result<()> {
    let model = find(id)?;
    let file = model.file();
    let mut urls = vec![format!(
        "https://huggingface.co/{}/resolve/{}/{}",
        model.id, model.revision, file.filename
    )];
    // Mirrors are untrusted bit pipes: only use them when we can verify the hash.
    if file.sha256.is_some() {
        urls.extend(CATALOG.mirrors.iter().map(|base| {
            format!("{}/{}/{}/{}", base.trim_end_matches('/'), model.id, model.revision, file.filename)
        }));
    }
    download::fetch_verified(
        &urls,
        &model_path(app_data, id)?,
        file.size_bytes,
        file.sha256.as_deref(),
        cancel,
        |stage, done, total| {
            let stage = match stage {
                download::Stage::Downloading => "downloading",
                download::Stage::Verifying => "verifying",
            };
            emit(app, id, stage, done, total, None);
        },
    )
    .await
}

fn emit(app: &AppHandle, id: &str, stage: &str, downloaded: u64, total: u64, error: Option<String>) {
    let _ = app.emit(
        "model-download",
        Progress {
            id,
            stage,
            downloaded,
            total,
            error,
        },
    );
}
