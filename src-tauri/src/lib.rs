mod audio;
mod download;
mod history;
mod input;
mod llm;
mod local_ai;
mod models;
mod overlay;
mod pipeline;
mod settings;
mod transcribe;
mod updater;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent, Wry};

use pipeline::{Controller, Mode, Status};
use settings::{Provider, ProviderConfig, Settings, Theme};

pub struct AppState {
    pub data_dir: PathBuf,
    settings_path: PathBuf,
    pub settings: RwLock<Settings>,
    pub recorder: audio::Recorder,
    pub transcriber: transcribe::Transcriber,
    pub history: history::History,
    pub status: Mutex<Status>,
    /// Why the shortcuts couldn't be registered, if they couldn't.
    shortcut_error: Mutex<Option<String>>,
    downloads: models::Downloads,
    tray_ai_toggle: Mutex<Option<CheckMenuItem<Wry>>>,
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Brings the settings window forward, optionally on a given page.
pub fn show_main(app: &AppHandle, page: Option<&str>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    if let Some(page) = page {
        let _ = app.emit_to("main", "navigate", page);
    }
}

/// Loads the selected model in the background so the first dictation is fast.
fn preload_selected(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let Some(id) = state.settings.read().unwrap().selected_model.clone() else {
            return;
        };
        if !models::is_downloaded(&state.data_dir, &id) {
            return;
        }
        let _ = app.emit("model-loading", true);
        if let Ok(path) = models::model_path(&state.data_dir, &id) {
            if let Err(e) = state.transcriber.ensure_loaded(&id, &path) {
                log::error!("{e}");
            }
        }
        let _ = app.emit("model-loading", false);
    });
}

fn shortcut_bindings(s: &Settings) -> [(Mode, &str); 3] {
    [
        (Mode::Dictate, s.dictate_shortcut.as_str()),
        (Mode::Command, s.command_shortcut.as_str()),
        (Mode::Plain, s.plain_shortcut.as_str()),
    ]
}

fn store_settings(app: &AppHandle, new: Settings) -> anyhow::Result<()> {
    let state = app.state::<AppState>();
    settings::save(&state.settings_path, &new)?;
    if let Some(item) = state.tray_ai_toggle.lock().unwrap().as_ref() {
        let _ = item.set_checked(new.ai_enabled);
    }
    *state.settings.write().unwrap() = new;
    let _ = app.emit("settings-changed", ());
    Ok(())
}

// ---------------------------------------------------------------- commands

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.read().unwrap().clone()
}

#[tauri::command]
fn save_settings(app: AppHandle, state: State<AppState>, mut settings: Settings) -> CmdResult<()> {
    let old = state.settings.read().unwrap().clone();
    // Only the local AI installer writes these paths; don't let a stale UI copy clobber them.
    settings.local_ai = old.local_ai.clone();
    if shortcut_bindings(&old) != shortcut_bindings(&settings) {
        pipeline::register_shortcuts(&app, &shortcut_bindings(&settings)).map_err(err)?;
        *state.shortcut_error.lock().unwrap() = None;
    }
    if old.theme != settings.theme {
        apply_theme(&app, settings.theme);
    }
    if old.launch_at_login != settings.launch_at_login {
        set_launch_at_login(&app, settings.launch_at_login).map_err(err)?;
    }
    let auto_update_turned_on = settings.auto_update && !old.auto_update;
    let model_changed = old.selected_model != settings.selected_model;
    let local_on = settings.provider == Provider::Local && settings.ai_enabled;
    let local_was_on = old.provider == Provider::Local && old.ai_enabled;
    store_settings(&app, settings).map_err(err)?;
    if model_changed {
        preload_selected(&app);
    }
    // Free the local model's memory when it's not in use; warm it when it is.
    if local_was_on && !local_on {
        local_ai::stop();
    } else if local_on && !local_was_on {
        warm_local_ai(&app);
    }
    if auto_update_turned_on && !cfg!(debug_assertions) {
        tauri::async_runtime::spawn(async move {
            if let Err(e) = updater::check(&app).await {
                log::warn!("update check failed: {e}");
            }
        });
    }
    Ok(())
}

/// Lipwise used to be SmartEar, and kept its data under this folder name.
const OLD_DATA_DIR: &str = "app.smartear.desktop";

/// Moves the old data folder over once so settings, history and downloaded models
/// carry across. If it can't be moved, it's used where it is and the move is tried
/// again next start.
fn migrate_data_dir(data_dir: &Path) -> PathBuf {
    let Some(old) = data_dir.parent().map(|p| p.join(OLD_DATA_DIR)) else {
        return data_dir.into();
    };
    if data_dir.exists() || !old.exists() {
        return data_dir.into();
    }
    // A llama-server left behind by the old build holds the model open and blocks the move.
    local_ai::kill_stale_at(&old.join("llama-server.pid"));
    let mut result = std::fs::rename(&old, data_dir);
    for _ in 0..10 {
        if result.is_ok() {
            break;
        }
        // Windows releases a killed process's file handles shortly after it exits.
        std::thread::sleep(std::time::Duration::from_millis(200));
        result = std::fs::rename(&old, data_dir);
    }
    match result {
        Ok(()) => {
            log::info!("moved {} to {}", old.display(), data_dir.display());
            data_dir.into()
        }
        Err(e) => {
            log::warn!("couldn't move {} to {}: {e}; using it in place", old.display(), data_dir.display());
            old
        }
    }
}

/// Settings saved before the move hold absolute paths into the old data folder.
/// Returns whether any were rebased.
fn rebase_local_ai_paths(local_ai: &mut settings::LocalAi, data_dir: &Path) -> bool {
    let Some(old) = data_dir.parent().map(|p| p.join(OLD_DATA_DIR)) else {
        return false;
    };
    if old == data_dir {
        return false;
    }
    let mut changed = false;
    for path in [&mut local_ai.model_path, &mut local_ai.server_path] {
        if let Ok(rest) = Path::new(path.as_str()).strip_prefix(&old) {
            *path = data_dir.join(rest).to_string_lossy().into_owned();
            changed = true;
        }
    }
    changed
}

/// Passed by the OS login item, so a login launch can be told apart from a manual one.
const AUTOSTART_ARG: &str = "--autostart";

/// Adds Lipwise to, or removes it from, the apps the OS opens at login.
/// Dev builds leave the login item alone: they load the UI from the Vite dev server,
/// which isn't running at login, and would replace the installed app's entry.
fn set_launch_at_login(app: &AppHandle, enabled: bool) -> Result<(), tauri_plugin_autostart::Error> {
    use tauri_plugin_autostart::ManagerExt;
    if cfg!(debug_assertions) {
        log::info!("dev build: not changing the login item (launch at login = {enabled})");
        return Ok(());
    }
    if enabled {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    }
}

/// Tints the native title bar (and, where the platform ties them together, the
/// webview's color scheme). The page itself switches on its `data-theme` attribute.
fn apply_theme(app: &AppHandle, theme: Theme) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_theme(match theme {
            Theme::System => None,
            Theme::Light => Some(tauri::Theme::Light),
            Theme::Dark => Some(tauri::Theme::Dark),
        });
        #[cfg(windows)]
        if let Ok(resolved) = window.theme() {
            tint_title_bar(&window, resolved);
        }
    }
}

/// With "Show accent color on title bars" on, Windows paints the title bar and border
/// in the accent color. Pin them to the app's own colors instead, keeping the native
/// title bar. Windows 11 only; older versions ignore these attributes.
#[cfg(windows)]
fn tint_title_bar(window: &tauri::WebviewWindow, theme: tauri::Theme) {
    use std::ffi::c_void;
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(hwnd: *mut c_void, attribute: u32, value: *const c_void, size: u32) -> i32;
    }
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_CAPTION_COLOR: u32 = 35;
    const DWMWA_TEXT_COLOR: u32 = 36;
    // COLORREFs (0x00BBGGRR): the caption matches --frame in styles.css, so the title
    // bar and sidebar read as one surface; the text matches --sidebar-label.
    let (caption, text, border): (u32, u32, u32) = match theme {
        tauri::Theme::Dark => (0x0020_2020, 0x00E7_E7E7, 0x0038_3838),
        _ => (0x00F0_F0F0, 0x0028_2626, 0x00D6_D6D6),
    };
    let Ok(hwnd) = window.hwnd() else { return };
    for (attribute, color) in [(DWMWA_CAPTION_COLOR, caption), (DWMWA_TEXT_COLOR, text), (DWMWA_BORDER_COLOR, border)] {
        // SAFETY: a valid window handle and a 4-byte COLORREF, as the attribute expects.
        unsafe { DwmSetWindowAttribute(hwnd.0 as _, attribute, &color as *const u32 as *const c_void, 4) };
    }
}

fn warm_local_ai(app: &AppHandle) {
    let settings = app.state::<AppState>().settings.read().unwrap().clone();
    if settings.provider != Provider::Local || !settings.ai_enabled || !settings.local_ai.configured() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if let Err(e) = local_ai::ensure_running(&settings.local_ai).await {
            log::error!("{e}");
        }
    });
}

#[tauri::command]
async fn local_ai_detect(state: State<'_, AppState>) -> CmdResult<local_ai::Hardware> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || local_ai::detect(&data_dir))
        .await
        .map_err(err)
}

/// Local AI wizard, step 2: download llama.cpp (if needed) and the model.
#[tauri::command]
async fn local_ai_download(app: AppHandle, model_id: String) -> CmdResult<()> {
    let data_dir = app.state::<AppState>().data_dir.clone();
    local_ai::download(&app, &data_dir, &model_id).await.map_err(err)?;
    Ok(())
}

#[derive(serde::Serialize)]
struct TrialRun {
    said: &'static str,
    typed: String,
    millis: u128,
}

/// Step 3: start the model and run one sample dictation through it.
#[tauri::command]
async fn local_ai_start(app: AppHandle, model_id: String) -> CmdResult<TrialRun> {
    let state = app.state::<AppState>();
    let config = local_ai::installed_config(&state.data_dir, &model_id).map_err(err)?;
    local_ai::ensure_running(&config).await.map_err(err)?;

    const SAID: &str = "um so the meeting is on tuesday no wait thursday at ten new paragraph can you bring the slides question mark";
    let mut trial = state.settings.read().unwrap().clone();
    trial.provider = Provider::Local;
    trial.local_ai = config;
    let started = std::time::Instant::now();
    let typed = llm::polish_dictation(&trial, SAID).await.map_err(err)?;
    Ok(TrialRun {
        said: SAID,
        typed,
        millis: started.elapsed().as_millis(),
    })
}

/// Step 4: point Lipwise at the local model.
#[tauri::command]
fn local_ai_apply(app: AppHandle, state: State<AppState>, model_id: String) -> CmdResult<()> {
    let config = local_ai::installed_config(&state.data_dir, &model_id).map_err(err)?;
    local_ai::prune_models(&state.data_dir, &config);
    let mut settings = state.settings.read().unwrap().clone();
    settings.local_ai = config;
    settings.provider = Provider::Local;
    settings.ai_enabled = true;
    store_settings(&app, settings).map_err(err)?;
    warm_local_ai(&app);
    Ok(())
}

/// Starts the configured local model (the Start button).
#[tauri::command]
async fn local_ai_run(app: AppHandle) -> CmdResult<()> {
    let config = app.state::<AppState>().settings.read().unwrap().local_ai.clone();
    local_ai::ensure_running(&config).await.map(|_| ()).map_err(err)
}

/// Stops the local model: the Stop button, or a model started in the wizard but never applied.
#[tauri::command]
fn local_ai_stop() {
    local_ai::stop();
}

#[tauri::command]
fn local_ai_cancel() {
    local_ai::cancel_setup();
}

#[tauri::command]
fn local_ai_remove(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    local_ai::remove(&state.data_dir).map_err(err)?;
    let mut settings = state.settings.read().unwrap().clone();
    settings.local_ai = Default::default();
    store_settings(&app, settings).map_err(err)
}

#[tauri::command]
async fn check_for_update(app: AppHandle) -> CmdResult<Option<updater::Available>> {
    updater::check(&app).await.map_err(err)
}

#[tauri::command]
fn install_update(app: AppHandle) -> CmdResult<()> {
    updater::install(&app).map_err(err)
}

#[tauri::command]
fn get_status(app: AppHandle, state: State<AppState>) -> serde_json::Value {
    serde_json::json!({
        "status": *state.status.lock().unwrap(),
        "loaded_model": state.transcriber.loaded_id(),
        "ai_ready": state.settings.read().unwrap().ai_ready(),
        "shortcut_error": *state.shortcut_error.lock().unwrap(),
        "local_ai": local_ai::state(),
        "update": app.state::<updater::Updates>().ready(),
        "env_keys": {
            "anthropic": settings::env_key(Provider::Anthropic).is_some(),
            "openai": settings::env_key(Provider::OpenAI).is_some(),
        },
    })
}

#[tauri::command]
fn list_models(state: State<AppState>) -> Vec<models::ModelInfo> {
    models::list(&state.data_dir)
}

#[tauri::command]
async fn download_model(app: AppHandle, id: String) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let data_dir = state.data_dir.clone();
    models::download(app.clone(), &state.downloads, data_dir, id.clone())
        .await
        .map_err(err)?;
    // First model downloaded? Make it the active one.
    let mut settings = state.settings.read().unwrap().clone();
    let current_ok = settings
        .selected_model
        .as_ref()
        .is_some_and(|m| models::is_downloaded(&state.data_dir, m));
    if !current_ok {
        settings.selected_model = Some(id);
        store_settings(&app, settings).map_err(err)?;
        preload_selected(&app);
    }
    Ok(())
}

#[tauri::command]
fn cancel_download(state: State<AppState>, id: String) {
    state.downloads.cancel(&id);
}

#[tauri::command]
fn delete_model(app: AppHandle, state: State<AppState>, id: String) -> CmdResult<()> {
    if state.transcriber.loaded_id().as_deref() == Some(id.as_str()) {
        state.transcriber.unload();
    }
    models::delete(&state.data_dir, &id).map_err(err)?;
    let mut settings = state.settings.read().unwrap().clone();
    if settings.selected_model.as_deref() == Some(id.as_str()) {
        settings.selected_model = models::list(&state.data_dir)
            .into_iter()
            .find(|m| m.downloaded)
            .map(|m| m.id);
        store_settings(&app, settings).map_err(err)?;
        preload_selected(&app);
    }
    Ok(())
}

#[tauri::command]
fn list_microphones() -> Vec<String> {
    audio::list_microphones()
}

#[tauri::command]
async fn list_ai_models(provider: Provider, config: ProviderConfig) -> CmdResult<Vec<String>> {
    llm::list_models(provider, &config).await.map_err(err)
}

/// Runs text through the AI step without speaking — powers the Playground.
#[tauri::command]
async fn preview_ai(settings: Settings, mode: Mode, text: String, selection: String) -> CmdResult<String> {
    match mode {
        Mode::Dictate => llm::polish_dictation(&settings, &text).await,
        Mode::Command => llm::run_command(&settings, &text, &selection).await,
        Mode::Plain => Ok(text),
    }
    .map_err(err)
}

#[tauri::command]
fn get_history(state: State<AppState>) -> Vec<history::Entry> {
    state.history.all()
}

#[tauri::command]
fn delete_history(state: State<AppState>, ids: Vec<u64>) {
    state.history.remove(&ids);
}

#[tauri::command]
fn clear_history(state: State<AppState>) {
    state.history.clear();
}

#[tauri::command]
fn cancel_recording(controller: State<Controller>) {
    controller.cancel();
}

// ---------------------------------------------------------------- setup

fn build_tray(app: &AppHandle, ai_enabled: bool) -> tauri::Result<CheckMenuItem<Wry>> {
    let open = MenuItem::with_id(app, "open", "Open Lipwise", true, None::<&str>)?;
    let ai = CheckMenuItem::with_id(app, "ai", "AI editing", true, ai_enabled, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Lipwise", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &ai, &PredefinedMenuItem::separator(app)?, &quit])?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("Lipwise")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app, None),
            "ai" => {
                let mut settings = app.state::<AppState>().settings.read().unwrap().clone();
                settings.ai_enabled = !settings.ai_enabled;
                if let Err(e) = store_settings(app, settings) {
                    log::error!("{e}");
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle(), None);
            }
        })
        .build(app)?;
    Ok(ai)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_main(app, None)))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_ARG]),
        ))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(pipeline::on_shortcut)
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = migrate_data_dir(&app.path().app_data_dir()?);
            std::fs::create_dir_all(&data_dir)?;
            let settings_path = data_dir.join("settings.json");
            let mut settings = settings::load(&settings_path);
            if rebase_local_ai_paths(&mut settings.local_ai, &data_dir) {
                if let Err(e) = settings::save(&settings_path, &settings) {
                    log::warn!("couldn't save rebased local AI paths: {e}");
                }
            }
            log::info!("data directory: {}", data_dir.display());

            transcribe::init_backends();

            app.manage(AppState {
                history: history::History::load(data_dir.join("history.json")),
                data_dir,
                settings_path,
                settings: RwLock::new(settings.clone()),
                recorder: audio::Recorder::new(),
                transcriber: transcribe::Transcriber::default(),
                status: Mutex::new(Status {
                    phase: "idle",
                    mode: Mode::Dictate,
                    message: String::new(),
                }),
                downloads: models::Downloads::default(),
                tray_ai_toggle: Mutex::new(None),
                shortcut_error: Mutex::new(None),
            });
            app.manage(Controller::new(handle.clone()));
            app.manage(updater::Updates::default());

            if let Err(e) = pipeline::register_shortcuts(&handle, &shortcut_bindings(&settings)) {
                log::error!("{e}");
                *app.state::<AppState>().shortcut_error.lock().unwrap() = Some(e.to_string());
            }
            let ai_toggle = build_tray(&handle, settings.ai_enabled)?;
            *app.state::<AppState>().tray_ai_toggle.lock().unwrap() = Some(ai_toggle);

            overlay::setup(&handle);
            apply_theme(&handle, settings.theme);
            if let Some(main) = app.get_webview_window("main") {
                // Closing the window keeps Lipwise running in the tray.
                let window = main.clone();
                main.on_window_event(move |event| match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    // The OS switched light/dark while the theme follows the system.
                    #[cfg(windows)]
                    WindowEvent::ThemeChanged(theme) => tint_title_bar(&window, *theme),
                    _ => {}
                });
                let at_login = std::env::args().any(|a| a == AUTOSTART_ARG);
                if !(at_login && settings.start_hidden) {
                    let _ = main.show();
                }
            }
            // Re-register so the login item follows the app if it was moved or updated.
            if settings.launch_at_login {
                if let Err(e) = set_launch_at_login(&handle, true) {
                    log::warn!("couldn't refresh the login item: {e}");
                }
            }
            preload_selected(&handle);
            local_ai::init(handle.clone(), app.state::<AppState>().data_dir.clone());
            let state = app.state::<AppState>();
            if let Some(replacement) = local_ai::migrate(&state.data_dir, &settings.local_ai) {
                let mut migrated = settings.clone();
                migrated.local_ai = replacement;
                if let Err(e) = store_settings(&handle, migrated) {
                    log::error!("couldn't save migrated local AI settings: {e}");
                }
            }
            if settings.local_ai_autostart {
                warm_local_ai(&handle);
            }
            updater::spawn_checker(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            get_status,
            list_models,
            download_model,
            cancel_download,
            delete_model,
            list_microphones,
            list_ai_models,
            preview_ai,
            get_history,
            delete_history,
            clear_history,
            cancel_recording,
            local_ai_detect,
            local_ai_download,
            local_ai_start,
            local_ai_apply,
            local_ai_run,
            local_ai_stop,
            local_ai_cancel,
            local_ai_remove,
            check_for_update,
            install_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Lipwise")
        .run(|_, event| {
            if let tauri::RunEvent::Exit = event {
                local_ai::stop();
            }
        });
}
