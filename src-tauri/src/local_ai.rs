//! "On this computer" AI: Lipwise detects the hardware, installs llama.cpp and
//! a small instruct model, and runs `llama-server` as a hidden child process on
//! localhost. Nothing leaves the machine.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::download;
use crate::settings::LocalAi;

// ---------------------------------------------------------------- models

pub struct LocalModel {
    pub id: &'static str,
    pub name: &'static str,
    pub blurb: &'static str,
    repo: &'static str,
    revision: &'static str,
    file: &'static str,
    pub size: u64,
    sha256: &'static str,
}

/// Qwen3.5 instruct models (Apache-2.0), Q4_K_M quantization, pinned revisions,
/// smallest first. The 2B was dropped: it ignored whole-text commands ("make it
/// sound professional") under the dictation prompt.
pub const MODELS: [LocalModel; 2] = [
    LocalModel {
        id: "qwen3.5-4b",
        name: "Standard · Qwen3.5 4B",
        blurb: "Follows every kind of spoken edit, including style rewrites like \"make it formal\". Quick with a graphics card or Apple Silicon, slower on CPU-only computers.",
        repo: "unsloth/Qwen3.5-4B-GGUF",
        revision: "e87f176479d0855a907a41277aca2f8ee7a09523",
        file: "Qwen3.5-4B-Q4_K_M.gguf",
        size: 2_740_937_888,
        sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
    },
    LocalModel {
        id: "qwen3.5-9b",
        name: "Best · Qwen3.5 9B",
        blurb: "Most accurate rewrites and translations. Needs a graphics card with 10 GB+ or a 24 GB+ Mac.",
        repo: "unsloth/Qwen3.5-9B-GGUF",
        revision: "3885219b6810b007914f3a7950a8d1b469d598a5",
        file: "Qwen3.5-9B-Q4_K_M.gguf",
        size: 5_680_522_464,
        sha256: "03b74727a860a56338e042c4420bb3f04b2fec5734175f4cb9fa853daf52b7e8",
    },
];

fn find_model(id: &str) -> Result<&'static LocalModel> {
    MODELS
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| anyhow!("Unknown local model: {id}"))
}

fn model_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("local-ai")
}

fn llama_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("llama.cpp")
}

/// Oldest llama.cpp build we trust to have the flags we pass (automatic GPU
/// layer fitting, `--reasoning-budget`, `--chat-template-kwargs`).
const MIN_LLAMA_BUILD: u32 = 8000;

/// Memory sizes are shown the way vendors label them (GiB, "24 GB" for 24 GiB).
const GB: f32 = 1_073_741_824.0;

// ---------------------------------------------------------------- hardware

#[derive(Serialize, Clone)]
pub struct Gpu {
    pub name: String,
    pub vram_gb: Option<f32>,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Metal,
    Vulkan,
    Cpu,
}

#[derive(Serialize)]
pub struct ModelOption {
    pub id: &'static str,
    pub name: &'static str,
    pub blurb: &'static str,
    pub size_bytes: u64,
    pub downloaded: bool,
    /// Fits comfortably in the memory this machine would run it from.
    pub fits: bool,
    /// Enough free disk space to download it (always true once downloaded).
    pub fits_disk: bool,
}

#[derive(Serialize)]
pub struct Hardware {
    pub ram_gb: f32,
    pub cpu_cores: usize,
    pub gpus: Vec<Gpu>,
    pub backend: Backend,
    /// One line for the UI, e.g. "AMD Radeon RX 7900 XTX (24 GB) via Vulkan".
    pub summary: String,
    pub recommended: &'static str,
    /// A recent llama-server already installed on the system, which we'll reuse.
    pub existing_server: Option<String>,
    /// Free space on the drive Lipwise stores its data on.
    pub free_disk_gb: Option<f32>,
    pub models: Vec<ModelOption>,
}

/// Room for llama.cpp's unpacked build plus some headroom.
const DISK_MARGIN: u64 = 600_000_000;

/// Free bytes on the volume holding `path` (the deepest matching mount point).
fn free_space(path: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

/// Inspects RAM, GPUs and installed llama.cpp. Blocking (runs a few short commands).
pub fn detect(data_dir: &Path) -> Hardware {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let ram_gb = sys.total_memory() as f32 / GB;
    let cpu_cores = std::thread::available_parallelism().map_or(4, |n| n.get());

    let gpus = detect_gpus(ram_gb);
    // A dedicated GPU (2 GB+ of its own memory) is worth offloading to; small
    // integrated chips are usually no faster than the CPU.
    let best_gpu = gpus
        .iter()
        .filter_map(|g| g.vram_gb.map(|v| (g, v)))
        .max_by(|a, b| a.1.total_cmp(&b.1));
    let backend = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Backend::Metal
    } else if best_gpu.is_some_and(|(_, v)| v >= 2.0) {
        Backend::Vulkan
    } else {
        Backend::Cpu
    };

    // Memory the model would live in.
    let budget = match backend {
        Backend::Metal => ram_gb * 0.6, // unified memory, shared with the OS and apps
        Backend::Vulkan => best_gpu.map_or(0.0, |(_, v)| v),
        Backend::Cpu => ram_gb * 0.5,
    };
    let fits = |m: &LocalModel| m.size as f32 / GB + 1.5 <= budget;
    // The biggest model that fits comfortably on a GPU; on CPU, latency matters
    // more than the last bit of quality, so stay with the smallest.
    let recommended = match backend {
        Backend::Cpu => MODELS[0].id,
        _ => MODELS.iter().rev().find(|m| fits(m)).unwrap_or(&MODELS[0]).id,
    };

    let summary = match (backend, best_gpu) {
        (Backend::Metal, _) => format!("{} with {:.0} GB unified memory via Metal", apple_chip(), ram_gb),
        (Backend::Vulkan, Some((g, v))) => format!("{} ({v:.0} GB) via Vulkan", g.name),
        _ => format!("CPU only, {cpu_cores} threads, {ram_gb:.0} GB RAM"),
    };

    let free = free_space(data_dir);
    let models: Vec<ModelOption> = MODELS
        .iter()
        .map(|m| {
            let downloaded = model_dir(data_dir).join(m.file).is_file();
            ModelOption {
                id: m.id,
                name: m.name,
                blurb: m.blurb,
                size_bytes: m.size,
                downloaded,
                fits: fits(m),
                fits_disk: downloaded || free.is_none_or(|f| m.size + DISK_MARGIN <= f),
            }
        })
        .collect();
    // If the ideal model won't fit on disk, recommend the largest smaller one that does.
    // MODELS is ordered smallest to largest.
    let ideal = models.iter().position(|m| m.id == recommended).unwrap_or(0);
    let recommended = models[..=ideal].iter().rev().find(|m| m.fits_disk).map_or(recommended, |m| m.id);

    Hardware {
        ram_gb,
        cpu_cores,
        backend,
        summary,
        recommended,
        existing_server: find_existing_server(data_dir).map(|p| p.display().to_string()),
        free_disk_gb: free.map(|b| b as f32 / GB),
        models,
        gpus,
    }
}

fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::null())
}

fn run(program: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Option<String> {
    let out = quiet(&mut Command::new(program)).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr))
}

#[cfg(target_os = "windows")]
fn detect_gpus(_ram_gb: f32) -> Vec<Gpu> {
    // The display-adapter class key has each GPU's real dedicated memory as a
    // 64-bit value (WMI's AdapterRAM is capped at 4 GB).
    let script = r#"Get-ItemProperty 'HKLM:\SYSTEM\ControlSet001\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0*' -ErrorAction SilentlyContinue | ForEach-Object { "$($_.DriverDesc)|$($_.'HardwareInformation.qwMemorySize')" }"#;
    run("powershell", &["-NoProfile", "-NonInteractive", "-Command", script])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (name, bytes) = line.trim().split_once('|')?;
            if name.is_empty() || name.contains("Basic Display") || name.contains("Remote") {
                return None;
            }
            Some(Gpu {
                name: name.to_string(),
                vram_gb: bytes.parse::<u64>().ok().map(|b| b as f32 / GB),
            })
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn detect_gpus(_ram_gb: f32) -> Vec<Gpu> {
    let mut gpus: Vec<Gpu> = run("nvidia-smi", &["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (name, mib) = line.split_once(',')?;
            Some(Gpu {
                name: name.trim().to_string(),
                vram_gb: mib.trim().parse::<f32>().ok().map(|m| m * 1_048_576.0 / GB),
            })
        })
        .collect();
    // AMD (amdgpu) exposes VRAM size in sysfs.
    if let Ok(cards) = std::fs::read_dir("/sys/class/drm") {
        for card in cards.flatten() {
            let vram = card.path().join("device/mem_info_vram_total");
            if let Ok(bytes) = std::fs::read_to_string(&vram) {
                if let Ok(bytes) = bytes.trim().parse::<u64>() {
                    gpus.push(Gpu {
                        name: "AMD Radeon GPU".into(),
                        vram_gb: Some(bytes as f32 / GB),
                    });
                }
            }
        }
    }
    gpus
}

#[cfg(target_os = "macos")]
fn detect_gpus(ram_gb: f32) -> Vec<Gpu> {
    if cfg!(target_arch = "aarch64") {
        vec![Gpu {
            name: apple_chip(),
            vram_gb: Some(ram_gb),
        }]
    } else {
        Vec::new()
    }
}

fn apple_chip() -> String {
    run("sysctl", &["-n", "machdep.cpu.brand_string"])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Apple Silicon".into())
}

// ---------------------------------------------------------------- llama.cpp

fn server_name() -> &'static str {
    if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

/// Parses the build number from `llama-server --version`.
fn llama_build(server: &Path) -> Option<u32> {
    let out = run(server, &["--version"])?;
    let digits = |s: &str| s.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok();
    out.split("build ")
        .nth(1)
        .and_then(digits)
        .or_else(|| out.split("version: ").nth(1).and_then(digits))
}

/// A usable llama-server: one Lipwise installed, or a recent one on the system.
fn find_existing_server(data_dir: &Path) -> Option<PathBuf> {
    if let Some(ours) = find_file(&llama_dir(data_dir), server_name(), 3) {
        return Some(ours);
    }
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).map(|d| d.join(server_name())).collect())
        .unwrap_or_default();
    candidates.extend(["/opt/homebrew/bin/llama-server", "/usr/local/bin/llama-server"].map(PathBuf::from));
    candidates
        .into_iter()
        .filter(|p| p.is_file())
        .find(|p| llama_build(p).is_some_and(|b| b >= MIN_LLAMA_BUILD))
}

fn find_file(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    if depth == 0 {
        return None;
    }
    subdirs.into_iter().find_map(|d| find_file(&d, name, depth - 1))
}

/// The llama.cpp build Lipwise installs. It's pinned like the models, so a new
/// llama.cpp release is a code change here rather than something every install
/// picks up unreviewed. To move to a newer one, pick a `bNNNN` release on
/// github.com/ggml-org/llama.cpp/releases and copy each asset's size and SHA-256.
const LLAMA_TAG: &str = "b11446";

/// Each build we use from that release: asset suffix, size in bytes, SHA-256.
const LLAMA_ASSETS: [(&str, u64, &str); 10] = [
    ("-bin-win-vulkan-x64.zip", 33_337_862, "a27c90495e4816380a1882eb850080d03806ceaf9e979460743411822672ff76"),
    ("-bin-win-cpu-x64.zip", 19_398_456, "03c4fb7fe3c7612979e8dcc3953a4e58a281159b4fa7481e7f6eb2ebdc1075ce"),
    ("-bin-win-vulkan-arm64.zip", 25_882_001, "8962e4867c434c200c01ac63d98e5ed539bc842531198984fa070d869336022e"),
    ("-bin-win-cpu-arm64.zip", 12_227_954, "9a0643da7f15e42b76640da080287c83f60ad1e24bb6f3523c2ad13824382288"),
    ("-bin-macos-arm64.tar.gz", 11_970_641, "9f2a616ffd11a7f9a2682d34d1bbb44f3c24634420b5da3d3a8b591fa2488088"),
    ("-bin-macos-x64.tar.gz", 11_487_036, "a8a459ec8abf768ef130fe72926d69c2937e8a6222f32813fb640bb3bf86e6ef"),
    ("-bin-ubuntu-vulkan-x64.tar.gz", 31_638_289, "f8ce39bee242a478dcd4d48aa6e45fbec2be5bfd8aa09a56c7514d58dc0ca719"),
    ("-bin-ubuntu-x64.tar.gz", 17_692_543, "c5755973e290ca0ebac2ca2bd1ea70bf2858f38dade027acecc4237bc6cfc7da"),
    ("-bin-ubuntu-vulkan-arm64.tar.gz", 24_846_311, "4698389847c6818c7a17b009e22c1dbc621e54498f4554ab25e1be1bca6210c8"),
    ("-bin-ubuntu-arm64.tar.gz", 13_682_913, "d07bc13f447b1c74c75818b1a48344151ec84fadd6acacd523e3496254c18af7"),
];

/// The pinned build for this OS/arch/backend: file name, size and SHA-256.
fn llama_asset(backend: Backend) -> Result<(String, u64, &'static str)> {
    let gpu = backend == Backend::Vulkan;
    let suffix = match (std::env::consts::OS, std::env::consts::ARCH, gpu) {
        ("windows", "x86_64", true) => "-bin-win-vulkan-x64.zip",
        ("windows", "x86_64", false) => "-bin-win-cpu-x64.zip",
        ("windows", "aarch64", true) => "-bin-win-vulkan-arm64.zip",
        ("windows", "aarch64", false) => "-bin-win-cpu-arm64.zip",
        ("macos", "aarch64", _) => "-bin-macos-arm64.tar.gz",
        ("macos", "x86_64", _) => "-bin-macos-x64.tar.gz",
        ("linux", "x86_64", true) => "-bin-ubuntu-vulkan-x64.tar.gz",
        ("linux", "x86_64", false) => "-bin-ubuntu-x64.tar.gz",
        ("linux", "aarch64", true) => "-bin-ubuntu-vulkan-arm64.tar.gz",
        ("linux", "aarch64", false) => "-bin-ubuntu-arm64.tar.gz",
        (os, arch, _) => bail!("No llama.cpp build available for {os}/{arch}"),
    };
    let (_, size, sha256) = LLAMA_ASSETS
        .iter()
        .find(|(s, ..)| *s == suffix)
        .ok_or_else(|| anyhow!("No pinned llama.cpp build for {suffix}"))?;
    Ok((format!("llama-{LLAMA_TAG}{suffix}"), *size, sha256))
}

async fn install_llama(
    data_dir: &Path,
    backend: Backend,
    cancel: &AtomicBool,
    progress: &impl Fn(&str, &str, u64, u64),
) -> Result<PathBuf> {
    let (name, size, sha) = llama_asset(backend)?;
    let tag = LLAMA_TAG;
    let url = format!("https://github.com/ggml-org/llama.cpp/releases/download/{tag}/{name}");

    let root = llama_dir(data_dir);
    let archive = root.join(&name);
    download::fetch_verified(&[url], &archive, size, Some(sha), cancel, |stage, done, total| match stage {
        download::Stage::Downloading => progress("llama", &format!("Downloading llama.cpp {tag}…"), done, total),
        download::Stage::Verifying => progress("llama", "Verifying llama.cpp…", done, total),
    })
    .await?;

    progress("llama", "Unpacking llama.cpp…", 0, 0);
    let dest = root.join(tag);
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest)?;
    let (archive_c, dest_c) = (archive.clone(), dest.clone());
    tauri::async_runtime::spawn_blocking(move || extract(&archive_c, &dest_c)).await??;
    let _ = std::fs::remove_file(&archive);

    // Drop older builds we installed before.
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            if entry.path().is_dir() && entry.file_name() != tag {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    find_file(&dest, server_name(), 3).ok_or_else(|| anyhow!("llama-server missing from the downloaded build"))
}

/// Unpacks .zip and .tar.gz with the OS's own `tar` (bsdtar on Windows 10+ reads zip too).
fn extract(archive: &Path, dest: &Path) -> Result<()> {
    let tar = if cfg!(windows) {
        PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join(r"System32\tar.exe")
    } else {
        PathBuf::from("tar")
    };
    let out = quiet(&mut Command::new(tar))
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(dest)
        .output()
        .context("Couldn't run tar to unpack llama.cpp")?;
    if !out.status.success() {
        bail!("Unpacking failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

/// GPUs llama.cpp itself can use, from `llama-server --list-devices`.
fn usable_devices(server: &Path) -> Vec<String> {
    run(server, &["--list-devices"])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| l.contains(": ") && (l.starts_with("Vulkan") || l.starts_with("Metal") || l.starts_with("CUDA") || l.starts_with("ROCm")))
        .map(str::to_string)
        .collect()
}

// ---------------------------------------------------------------- setup

#[derive(Serialize, Clone)]
struct SetupProgress<'a> {
    step: &'a str,
    message: &'a str,
    downloaded: u64,
    total: u64,
}

static CANCEL: AtomicBool = AtomicBool::new(false);
static SETUP_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn cancel_setup() {
    CANCEL.store(true, Ordering::Relaxed);
}

/// Wizard step 2: puts llama.cpp and the chosen model on disk. Emits
/// `local-ai-setup` progress events and returns the configuration that steps 3
/// (run) and 4 (apply) use.
pub async fn download(app: &AppHandle, data_dir: &Path, model_id: &str) -> Result<LocalAi> {
    if SETUP_RUNNING.swap(true, Ordering::SeqCst) {
        bail!("A download is already running");
    }
    CANCEL.store(false, Ordering::Relaxed);
    let result = download_inner(app, data_dir, model_id).await;
    SETUP_RUNNING.store(false, Ordering::SeqCst);

    let (step, message) = match &result {
        Ok(_) => ("done", "Everything is downloaded".to_string()),
        Err(_) if CANCEL.load(Ordering::Relaxed) => ("cancelled", "Download cancelled".to_string()),
        Err(e) => ("error", e.to_string()),
    };
    let _ = app.emit("local-ai-setup", SetupProgress { step, message: &message, downloaded: 0, total: 0 });
    result
}

async fn download_inner(app: &AppHandle, data_dir: &Path, model_id: &str) -> Result<LocalAi> {
    let progress = |step: &str, message: &str, downloaded: u64, total: u64| {
        let _ = app.emit("local-ai-setup", SetupProgress { step, message, downloaded, total });
    };
    let model = find_model(model_id)?;
    let dir = data_dir.to_path_buf();
    let hw = tauri::async_runtime::spawn_blocking(move || detect(&dir)).await?;

    let model_path = model_dir(data_dir).join(model.file);
    if !model_path.is_file() {
        if let Some(free) = free_space(data_dir) {
            let needed = model.size + DISK_MARGIN;
            if free < needed {
                bail!(
                    "Not enough disk space: {} needs about {:.1} GB free and this drive has {:.1} GB. Free up some space or pick a smaller model.",
                    model.name,
                    needed as f32 / GB,
                    free as f32 / GB
                );
            }
        }
    }

    let server = match hw.existing_server {
        Some(path) => {
            log::info!("using existing llama-server at {path}");
            PathBuf::from(path)
        }
        None => install_llama(data_dir, hw.backend, &CANCEL, &progress).await?,
    };
    if hw.backend != Backend::Cpu {
        let s = server.clone();
        let devices = tauri::async_runtime::spawn_blocking(move || usable_devices(&s)).await?;
        if devices.is_empty() {
            log::warn!("llama.cpp found no usable GPU; it will run on the CPU");
        } else {
            log::info!("llama.cpp devices: {}", devices.join("; "));
        }
    }

    let url = format!("https://huggingface.co/{}/resolve/{}/{}", model.repo, model.revision, model.file);
    download::fetch_verified(&[url], &model_path, model.size, Some(model.sha256), &CANCEL, |stage, done, total| {
        match stage {
            download::Stage::Downloading => progress("model", &format!("Downloading {}…", model.name), done, total),
            download::Stage::Verifying => progress("model", "Verifying the model…", done, total),
        }
    })
    .await?;

    Ok(LocalAi {
        model_id: model.id.to_string(),
        model_path: model_path.display().to_string(),
        server_path: server.display().to_string(),
    })
}

/// Handles a saved configuration whose model is no longer offered (the 2B):
/// switches to the largest offered model that's already downloaded, or clears
/// the configuration so the wizard runs again. Old model files are removed.
/// Returns the replacement, or `None` when nothing needs to change.
pub fn migrate(data_dir: &Path, config: &LocalAi) -> Option<LocalAi> {
    if !config.configured() || find_model(&config.model_id).is_ok() {
        return None;
    }
    let replacement = MODELS
        .iter()
        .rev()
        .find_map(|m| installed_config(data_dir, m.id).ok())
        .unwrap_or_default();
    log::info!(
        "local model '{}' is no longer offered; switching to '{}'",
        config.model_id,
        if replacement.configured() { replacement.model_id.as_str() } else { "(none, setup needed)" }
    );
    // Remove only files of models we no longer offer; keep any other downloads.
    if let Ok(entries) = std::fs::read_dir(model_dir(data_dir)) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let offered = MODELS.iter().any(|m| name == m.file);
            if !offered && entry.path().extension().is_some_and(|e| e == "gguf") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    Some(replacement)
}

/// The configuration for a model whose files are already downloaded (steps 3 and 4).
pub fn installed_config(data_dir: &Path, model_id: &str) -> Result<LocalAi> {
    let model = find_model(model_id)?;
    let model_path = model_dir(data_dir).join(model.file);
    if !model_path.is_file() {
        bail!("Download {} first", model.name);
    }
    let server = find_existing_server(data_dir).ok_or_else(|| anyhow!("Download llama.cpp first"))?;
    Ok(LocalAi {
        model_id: model.id.to_string(),
        model_path: model_path.display().to_string(),
        server_path: server.display().to_string(),
    })
}

/// Deletes downloaded models other than the one in use, to give the disk space back.
pub fn prune_models(data_dir: &Path, keep: &LocalAi) {
    let keep = PathBuf::from(&keep.model_path);
    if let Ok(entries) = std::fs::read_dir(model_dir(data_dir)) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path != keep && path.extension().is_some_and(|e| e == "gguf") {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// Deletes everything Lipwise installed for local AI. A system llama.cpp is left alone.
pub fn remove(data_dir: &Path) -> Result<()> {
    stop();
    for dir in [model_dir(data_dir), llama_dir(data_dir)] {
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- server

struct Running {
    child: Child,
    port: u16,
    config_key: String,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);
/// Serializes starts so a warm-up and a first dictation don't launch two servers.
static STARTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static APP: OnceLock<(AppHandle, PathBuf)> = OnceLock::new();

#[derive(Serialize, Clone)]
pub struct ServerState {
    /// stopped | starting | running | error
    pub state: &'static str,
    /// The model the server is (or was being) started with.
    pub model_id: String,
    pub message: String,
}

static STATE: Mutex<ServerState> = Mutex::new(ServerState {
    state: "stopped",
    model_id: String::new(),
    message: String::new(),
});

pub fn init(app: AppHandle, data_dir: PathBuf) {
    let _ = APP.set((app, data_dir));
    if let Ok(path) = pid_file() {
        kill_stale_at(&path);
    }
}

pub fn state() -> ServerState {
    STATE.lock().unwrap().clone()
}

fn set_state(state: &'static str, model_id: &str, message: impl Into<String>) {
    *STATE.lock().unwrap() = ServerState {
        state,
        model_id: model_id.to_string(),
        message: message.into(),
    };
    if let Some((app, _)) = APP.get() {
        let _ = app.emit("local-ai-state", ());
    }
}

fn data_dir() -> Result<&'static Path> {
    APP.get().map(|(_, d)| d.as_path()).ok_or_else(|| anyhow!("local AI not initialised"))
}

fn pid_file() -> Result<PathBuf> {
    Ok(data_dir()?.join("llama-server.pid"))
}

/// Kills a llama-server we started in a previous run that didn't shut down cleanly,
/// as recorded in the pid file at `path`.
pub fn kill_stale_at(path: &Path) {
    let Some(pid) = std::fs::read_to_string(path).ok().and_then(|s| s.trim().parse::<u32>().ok()) else {
        return;
    };
    let pid = sysinfo::Pid::from_u32(pid);
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    if let Some(process) = sys.process(pid) {
        if process.name().to_string_lossy().contains("llama-server") {
            log::info!("stopping leftover llama-server (pid {pid})");
            process.kill();
        }
    }
    let _ = std::fs::remove_file(path);
}

/// Starts llama-server for `config` unless it's already running. Returns the API base URL.
pub async fn ensure_running(config: &LocalAi) -> Result<String> {
    if !config.configured() {
        bail!("Set up the local AI first");
    }
    let _starting = STARTING.lock().await;
    let key = format!("{}|{}", config.server_path, config.model_path);
    {
        let mut running = RUNNING.lock().unwrap();
        if let Some(r) = running.as_mut() {
            if r.config_key == key && matches!(r.child.try_wait(), Ok(None)) {
                return Ok(base_url(r.port));
            }
        }
    }
    stop();

    let server = PathBuf::from(&config.server_path);
    let model = PathBuf::from(&config.model_path);
    if !server.is_file() || !model.is_file() {
        set_state("error", &config.model_id, "Local AI files are missing. Run setup again.");
        bail!("Local AI files are missing. Run setup again.");
    }

    set_state("starting", &config.model_id, "Loading the local model…");
    let port = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
    let log_path = data_dir()?.join("local-ai.log");
    let log = std::fs::File::create(&log_path)?;
    let mut cmd = Command::new(&server);
    quiet(&mut cmd)
        .current_dir(server.parent().unwrap_or(Path::new(".")))
        .arg("--model")
        .arg(&model)
        .args(["--host", "127.0.0.1", "--port", &port.to_string()])
        .args(["--alias", "lipwise-local", "--ctx-size", "8192", "--parallel", "1", "--no-webui"])
        // Dictation edits are short and latency-bound: no thinking, ever.
        .args(thinking_off_args(&server))
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    let child = cmd.spawn().with_context(|| format!("Couldn't start {}", server.display()))?;
    let _ = std::fs::write(pid_file()?, child.id().to_string());
    log::info!("started llama-server (pid {}) on port {port}", child.id());
    *RUNNING.lock().unwrap() = Some(Running {
        child,
        port,
        config_key: key,
    });

    match wait_healthy(port).await {
        Ok(()) => {
            let name = find_model(&config.model_id).map(|m| m.name).unwrap_or("local model");
            set_state("running", &config.model_id, format!("{name} is running"));
            Ok(base_url(port))
        }
        Err(e) => {
            stop();
            let tail = std::fs::read_to_string(&log_path)
                .map(|log| log.lines().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | "))
                .unwrap_or_default();
            let message = format!("Local AI failed to start: {e}. {tail}");
            set_state("error", &config.model_id, message.clone());
            bail!(message)
        }
    }
}

/// The API base URL of the server already running `config`. Never starts one: a model
/// the user stopped (or didn't autostart) stays stopped, and the caller falls back to
/// the raw transcript. A model that is still loading is waited for.
pub async fn running_url(config: &LocalAi) -> Result<String> {
    if !config.configured() {
        bail!("Set up the local AI first");
    }
    // A start in progress holds this lock until the server is healthy (or has failed).
    let _starting = STARTING.lock().await;
    let key = format!("{}|{}", config.server_path, config.model_path);
    let mut running = RUNNING.lock().unwrap();
    if let Some(r) = running.as_mut() {
        if r.config_key == key && matches!(r.child.try_wait(), Ok(None)) {
            return Ok(base_url(r.port));
        }
    }
    bail!("the local model isn't running")
}

/// Newer llama.cpp has `--reasoning off`; older builds (e.g. a system install) only
/// understand the budget + chat-template switch, which newer ones deprecate.
fn thinking_off_args(server: &Path) -> &'static [&'static str] {
    let help = run(server, &["--help"]).unwrap_or_default();
    if help.contains("--reasoning [on|off") {
        &["--reasoning", "off"]
    } else {
        &["--reasoning-budget", "0", "--chat-template-kwargs", r#"{"enable_thinking":false}"#]
    }
}

async fn wait_healthy(port: u16) -> Result<()> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(2)).build()?;
    let url = format!("http://127.0.0.1:{port}/health");
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        {
            let mut running = RUNNING.lock().unwrap();
            match running.as_mut().map(|r| r.child.try_wait()) {
                Some(Ok(None)) => {}
                Some(Ok(Some(status))) => bail!("it exited ({status})"),
                _ => bail!("it was stopped"),
            }
        }
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    bail!("timed out loading the model")
}

pub fn stop() {
    if let Some(mut r) = RUNNING.lock().unwrap().take() {
        let _ = r.child.kill();
        let _ = r.child.wait();
        log::info!("stopped llama-server");
    }
    if let Ok(path) = pid_file() {
        let _ = std::fs::remove_file(path);
    }
    // Keep a start error visible until the next attempt replaces it.
    let errored = STATE.lock().unwrap().state == "error";
    if !errored {
        set_state("stopped", "", "");
    }
}

fn base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/v1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_platform_we_ship_has_a_build() {
        for backend in [Backend::Cpu, Backend::Vulkan, Backend::Metal] {
            assert!(llama_asset(backend).is_ok(), "{backend:?}");
        }
    }

    #[test]
    fn migrates_off_retired_models() {
        let dir = std::env::temp_dir().join(format!("lipwise-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(model_dir(&dir)).unwrap();
        std::fs::create_dir_all(llama_dir(&dir).join("b1")).unwrap();
        std::fs::write(llama_dir(&dir).join("b1").join(server_name()), b"").unwrap();
        let old = model_dir(&dir).join("Qwen3.5-2B-Q4_K_M.gguf");
        let nine = model_dir(&dir).join(MODELS[1].file);
        std::fs::write(&old, b"").unwrap();
        std::fs::write(&nine, b"").unwrap();
        let retired = LocalAi {
            model_id: "qwen3.5-2b".into(),
            model_path: old.display().to_string(),
            server_path: "x".into(),
        };

        // A downloaded offered model takes over; only the retired file is removed.
        let replacement = migrate(&dir, &retired).expect("should migrate");
        assert_eq!(replacement.model_id, MODELS[1].id);
        assert!(!old.exists() && nine.exists());
        // An offered model needs no migration.
        assert!(migrate(&dir, &replacement).is_none());
        // Nothing offered on disk: configuration is cleared so setup runs again.
        std::fs::remove_file(&nine).unwrap();
        assert!(!migrate(&dir, &retired).unwrap().configured());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_hardware() {
        let hw = detect(&std::env::temp_dir());
        println!("{} | recommended {} | free disk {:?} GB | gpus {:?}", hw.summary, hw.recommended, hw.free_disk_gb, hw.gpus.iter().map(|g| (&g.name, g.vram_gb)).collect::<Vec<_>>());
        assert!(hw.ram_gb > 0.5);
    }
}
