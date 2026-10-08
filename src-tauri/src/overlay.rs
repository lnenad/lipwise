//! The small floating status pill. It's created non-focusable and click-through
//! so showing it never steals focus from the app the user is dictating into.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Manager, PhysicalPosition};

/// Bumped on every show so a delayed hide can't close a newer session's overlay.
static GENERATION: AtomicU64 = AtomicU64::new(0);

const BOTTOM_MARGIN: f64 = 90.0;

pub fn setup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.set_ignore_cursor_events(true);
    }
}

pub fn show(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    let Some(window) = app.get_webview_window("overlay") else {
        return;
    };
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    if let (Some(monitor), Ok(size)) = (monitor, window.outer_size()) {
        let area = monitor.work_area();
        let scale = monitor.scale_factor();
        let x = area.position.x + (area.size.width as i32 - size.width as i32) / 2;
        let y = area.position.y + area.size.height as i32
            - size.height as i32
            - (BOTTOM_MARGIN * scale) as i32;
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    let _ = window.show();
    let _ = window.set_always_on_top(true);
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
    }
}

pub fn hide_after(app: &AppHandle, delay: Duration) {
    let generation = GENERATION.load(Ordering::SeqCst);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if GENERATION.load(Ordering::SeqCst) == generation {
            hide(&app);
        }
    });
}
