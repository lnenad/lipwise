//! The dictation pipeline: shortcut → record → transcribe → AI → paste.
//!
//! Shortcut events are funnelled through one worker thread so press/release
//! pairs are handled strictly in order; the slow part (transcription and the AI
//! call) runs on the async runtime while the worker stays responsive.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::{audio, history, input, llm, models, overlay, AppState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Transcribe, then let the AI apply spoken commands and clean up.
    Dictate,
    /// Speak an instruction for the selected text.
    Command,
    /// Transcribe and type exactly what was heard; no AI.
    Plain,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Dictate => "Dictation",
            Mode::Command => "Command mode",
            Mode::Plain => "Plain dictation",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Recording {
        mode: Mode,
        started: Instant,
        /// Push-to-talk tapped briefly: keep recording until the next press.
        latched: bool,
    },
    Busy,
}

enum Event {
    Pressed(Mode),
    Released(Mode),
    Cancel,
}

#[derive(Serialize, Clone)]
pub struct Status {
    /// idle | recording | transcribing | thinking | done | error
    pub phase: &'static str,
    pub mode: Mode,
    pub message: String,
}

/// Ids of the currently registered shortcuts, so the handler can tell them apart.
#[derive(Default)]
pub struct Shortcuts {
    bindings: Vec<(Mode, Shortcut)>,
    escape: Option<Shortcut>,
}

pub struct Controller {
    tx: Mutex<mpsc::Sender<Event>>,
    pub shortcuts: Mutex<Shortcuts>,
}

/// A tap shorter than this on a push-to-talk shortcut switches to hands-free mode.
const TAP_THRESHOLD: Duration = Duration::from_millis(300);

impl Controller {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("pipeline".into())
            .spawn(move || worker(app, rx))
            .expect("failed to spawn pipeline thread");
        Self {
            tx: Mutex::new(tx),
            shortcuts: Mutex::new(Shortcuts::default()),
        }
    }

    fn send(&self, event: Event) {
        let _ = self.tx.lock().unwrap().send(event);
    }

    pub fn cancel(&self) {
        self.send(Event::Cancel);
    }
}

/// Global-shortcut plugin callback.
pub fn on_shortcut(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    let controller = app.state::<Controller>();
    let which = {
        let s = controller.shortcuts.lock().unwrap();
        let id = shortcut.id();
        if s.escape.as_ref().is_some_and(|e| e.id() == id) {
            if event.state() == ShortcutState::Pressed {
                controller.send(Event::Cancel);
            }
            return;
        }
        match s.bindings.iter().find(|(_, b)| b.id() == id) {
            Some((mode, _)) => *mode,
            None => return,
        }
    };
    controller.send(match event.state() {
        ShortcutState::Pressed => Event::Pressed(which),
        ShortcutState::Released => Event::Released(which),
    });
}

/// Registers one shortcut per mode, restoring the previous ones on failure.
pub fn register_shortcuts(app: &AppHandle, wanted: &[(Mode, &str)]) -> anyhow::Result<()> {
    let mut parsed: Vec<(Mode, Shortcut)> = Vec::new();
    for (mode, text) in wanted {
        let shortcut = text
            .parse::<Shortcut>()
            .map_err(|e| anyhow::anyhow!("Invalid shortcut \"{text}\": {e}"))?;
        if let Some((other, _)) = parsed.iter().find(|(_, s)| s.id() == shortcut.id()) {
            anyhow::bail!("{} and {} need different shortcuts", other.label(), mode.label());
        }
        parsed.push((*mode, shortcut));
    }

    let controller = app.state::<Controller>();
    let mut current = controller.shortcuts.lock().unwrap();
    let gs = app.global_shortcut();
    for (_, s) in &current.bindings {
        let _ = gs.unregister(*s);
    }
    let result = parsed.iter().try_for_each(|(_, s)| gs.register(*s));
    match result {
        Ok(()) => {
            current.bindings = parsed;
            Ok(())
        }
        Err(e) => {
            for (_, s) in &parsed {
                let _ = gs.unregister(*s);
            }
            for (_, s) in &current.bindings {
                let _ = gs.register(*s);
            }
            anyhow::bail!("Couldn't register shortcut (another app may be using it): {e}")
        }
    }
}

pub fn emit_status(app: &AppHandle, phase: &'static str, mode: Mode, message: impl Into<String>) {
    let status = Status {
        phase,
        mode,
        message: message.into(),
    };
    *app.state::<AppState>().status.lock().unwrap() = status.clone();
    let _ = app.emit("status", status);
}

fn worker(app: AppHandle, rx: mpsc::Receiver<Event>) {
    let phase = Arc::new(Mutex::new(Phase::Idle));
    let metering = Arc::new(AtomicBool::new(false));
    while let Ok(event) = rx.recv() {
        let current = *phase.lock().unwrap();
        let push_to_talk = app.state::<AppState>().settings.read().unwrap().push_to_talk;
        match (current, event) {
            (Phase::Idle, Event::Pressed(mode)) => {
                if start(&app, mode, &metering) {
                    *phase.lock().unwrap() = Phase::Recording {
                        mode,
                        started: Instant::now(),
                        latched: !push_to_talk,
                    };
                }
            }
            (Phase::Recording { mode, started, latched: false }, Event::Released(m)) if m == mode => {
                if started.elapsed() < TAP_THRESHOLD {
                    *phase.lock().unwrap() = Phase::Recording { mode, started, latched: true };
                    emit_status(&app, "recording", mode, "Hands-free · press to stop");
                } else {
                    finish(&app, mode, &phase, &metering);
                }
            }
            (Phase::Recording { mode, latched: true, .. }, Event::Pressed(_)) => {
                finish(&app, mode, &phase, &metering);
            }
            (Phase::Recording { mode, .. }, Event::Cancel) => {
                stop_recording(&app, &metering);
                *phase.lock().unwrap() = Phase::Idle;
                emit_status(&app, "idle", mode, "Cancelled");
                overlay::hide(&app);
            }
            _ => {}
        }
    }
}

/// Starts recording. Returns false (after telling the user why) if it can't.
fn start(app: &AppHandle, mode: Mode, metering: &Arc<AtomicBool>) -> bool {
    let state = app.state::<AppState>();
    let settings = state.settings.read().unwrap().clone();

    let Some(model_id) = settings
        .selected_model
        .clone()
        .filter(|id| models::is_downloaded(&state.data_dir, id))
    else {
        fail(app, mode, "Download a speech model first");
        crate::show_main(app, Some("models"));
        return false;
    };
    if mode == Mode::Command && !settings.ai_ready() {
        fail(app, mode, "Command mode needs an AI provider. Set one up in AI settings.");
        crate::show_main(app, Some("ai"));
        return false;
    }
    if let Some(problem) = missing_access(app) {
        fail(app, mode, problem);
        return false;
    }
    if let Err(e) = state.recorder.start(settings.microphone.clone()) {
        fail(app, mode, format!("Microphone error: {e}"));
        return false;
    }

    // Warm the model while the user talks so transcription starts immediately.
    let warm = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = warm.state::<AppState>();
        if let Ok(path) = models::model_path(&state.data_dir, &model_id) {
            if let Err(e) = state.transcriber.ensure_loaded(&model_id, &path) {
                log::error!("preload failed: {e}");
            }
        }
    });

    // Escape cancels, but only while recording so we don't swallow it otherwise.
    if let Ok(esc) = "Escape".parse::<Shortcut>() {
        if app.global_shortcut().register(esc).is_ok() {
            app.state::<Controller>().shortcuts.lock().unwrap().escape = Some(esc);
        }
    }

    metering.store(true, Ordering::Relaxed);
    let meter_app = app.clone();
    let meter_flag = metering.clone();
    std::thread::spawn(move || {
        while meter_flag.load(Ordering::Relaxed) {
            let level = meter_app.state::<AppState>().recorder.level();
            let _ = meter_app.emit_to("overlay", "mic-level", level);
            std::thread::sleep(Duration::from_millis(50));
        }
    });

    let hint = match mode {
        Mode::Dictate => "Listening…",
        Mode::Command => "Say what to do with the selection…",
        Mode::Plain => "Listening (no AI)…",
    };
    emit_status(app, "recording", mode, hint);
    if settings.show_overlay {
        overlay::show(app);
    }
    true
}

/// Checked before recording: without microphone access the recording is silent (or,
/// on macOS, interrupted by the system prompt), and without typing access the result
/// can't be pasted. Asks for whatever's missing and says what to do.
fn missing_access(app: &AppHandle) -> Option<&'static str> {
    use crate::permissions::{self, Access};
    match permissions::microphone() {
        Access::Ask => {
            permissions::request_microphone(app);
            return Some("Allow microphone access, then try again");
        }
        Access::Denied => {
            crate::show_main(app, Some("home"));
            return Some("Microphone access is off for Lipwise");
        }
        _ => {}
    }
    if permissions::typing() == Access::Denied {
        permissions::request_typing(app);
        crate::show_main(app, Some("home"));
        return Some("Allow Lipwise to type into other apps");
    }
    None
}

fn stop_recording(app: &AppHandle, metering: &AtomicBool) -> Vec<f32> {
    metering.store(false, Ordering::Relaxed);
    let controller = app.state::<Controller>();
    if let Some(esc) = controller.shortcuts.lock().unwrap().escape.take() {
        let _ = app.global_shortcut().unregister(esc);
    }
    app.state::<AppState>().recorder.stop()
}

fn finish(app: &AppHandle, mode: Mode, phase: &Arc<Mutex<Phase>>, metering: &AtomicBool) {
    let samples = stop_recording(app, metering);
    *phase.lock().unwrap() = Phase::Busy;

    // Grab the selection now, while the user's app still has focus.
    let selection = if mode == Mode::Command {
        input::copy_selection(app).unwrap_or_else(|e| {
            log::warn!("couldn't read selection: {e}");
            String::new()
        })
    } else {
        String::new()
    };

    let app = app.clone();
    let phase = phase.clone();
    tauri::async_runtime::spawn(async move {
        process(&app, mode, samples, selection).await;
        *phase.lock().unwrap() = Phase::Idle;
    });
}

async fn process(app: &AppHandle, mode: Mode, samples: Vec<f32>, selection: String) {
    let state = app.state::<AppState>();
    let settings = state.settings.read().unwrap().clone();

    if samples.len() < (audio::TARGET_RATE as usize) / 4 || audio::is_silent(&samples) {
        done(app, mode, "No speech detected");
        return;
    }

    emit_status(app, "transcribing", mode, "Transcribing…");
    let model_id = settings.selected_model.clone().unwrap_or_default();
    let language = settings.language.clone();
    let vocabulary = settings.vocabulary.clone();
    let task_app = app.clone();
    let transcript = tauri::async_runtime::spawn_blocking(move || {
        let state = task_app.state::<AppState>();
        let path = models::model_path(&state.data_dir, &model_id)?;
        state.transcriber.ensure_loaded(&model_id, &path)?;
        state.transcriber.transcribe(&samples, &language, &vocabulary)
    })
    .await
    .map_err(anyhow::Error::from)
    .and_then(|r| r);

    let transcript = match transcript {
        Ok(t) if !t.is_empty() => t,
        Ok(_) => return done(app, mode, "No speech detected"),
        Err(e) => return fail(app, mode, e.to_string()),
    };

    let mut note = None;
    let (output, ai_used) = match mode {
        Mode::Dictate if settings.ai_ready() => {
            emit_status(app, "thinking", mode, "Applying your edits…");
            match llm::polish_dictation(&settings, &transcript).await {
                Ok(text) => (text, true),
                Err(e) => {
                    log::warn!("AI step failed: {e}");
                    note = Some(format!("AI unavailable, typed the raw transcript ({e})"));
                    (transcript.clone(), false)
                }
            }
        }
        Mode::Dictate | Mode::Plain => (transcript.clone(), false),
        Mode::Command => {
            emit_status(app, "thinking", mode, "Working on it…");
            match llm::run_command(&settings, &transcript, &selection).await {
                Ok(text) => (text, true),
                Err(e) => {
                    record(app, &settings, mode, &transcript, "", &selection, false, Some(e.to_string()));
                    return fail(app, mode, e.to_string());
                }
            }
        }
    };

    let text = output.clone();
    let restore = settings.restore_clipboard;
    let paste_app = app.clone();
    let pasted = tauri::async_runtime::spawn_blocking(move || input::paste(&paste_app, &text, restore))
        .await
        .map_err(anyhow::Error::from)
        .and_then(|r| r);
    if let Err(e) = pasted {
        note = Some(format!("Couldn't paste ({e}). The text is in your history."));
    }

    record(app, &settings, mode, &transcript, &output, &selection, ai_used, note.clone());
    match note {
        Some(n) => fail(app, mode, n),
        None => done(app, mode, if output.is_empty() { "Nothing to type" } else { "Done" }),
    }
}

#[allow(clippy::too_many_arguments)]
fn record(
    app: &AppHandle,
    settings: &crate::settings::Settings,
    mode: Mode,
    transcript: &str,
    output: &str,
    selection: &str,
    ai_used: bool,
    note: Option<String>,
) {
    let entry = history::Entry {
        id: 0,
        timestamp: chrono::Local::now(),
        mode: format!("{mode:?}").to_lowercase(),
        transcript: transcript.to_string(),
        output: output.to_string(),
        selection: selection.to_string(),
        ai_used,
        note,
    };
    app.state::<AppState>().history.add(entry, settings.history_limit);
    let _ = app.emit("history-changed", ());
}

fn done(app: &AppHandle, mode: Mode, message: &str) {
    emit_status(app, "done", mode, message);
    overlay::hide_after(app, Duration::from_millis(900));
}

fn fail(app: &AppHandle, mode: Mode, message: impl Into<String>) {
    let message = message.into();
    log::warn!("{message}");
    emit_status(app, "error", mode, message);
    if app.state::<AppState>().settings.read().unwrap().show_overlay {
        overlay::show(app);
    }
    overlay::hide_after(app, Duration::from_millis(3500));
}
