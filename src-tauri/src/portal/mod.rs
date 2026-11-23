//! XDG desktop portals, for what a Wayland session doesn't let an app do by itself:
//! global shortcuts and typing into other apps. Nothing here depends on Tauri.

pub mod keyboard;
pub mod shortcuts;

use tokio::sync::{Mutex, OnceCell};

/// Whether this is a Wayland session, where X11 key grabs and synthetic X11 keystrokes
/// only reach apps running under XWayland.
pub fn wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t.eq_ignore_ascii_case("wayland"))
}

/// Held while a portal request may show a dialog, so the desktop shows one at a time.
pub static DIALOGS: Mutex<()> = Mutex::const_new(());

/// Tells the portal which app this is, so it can keep the user's choices. A Flatpak
/// has an app id the portal can see; a host app has to register one before any other
/// portal call. Registering fails without an installed .desktop file under that id,
/// which an AppImage often lacks; the portals still work, the desktop just may not
/// remember choices as well. Only the first call does anything.
pub async fn register(app_id: &str) {
    static REGISTERED: OnceCell<()> = OnceCell::const_new();
    REGISTERED
        .get_or_init(|| async {
            match ashpd::AppID::try_from(app_id) {
                Ok(id) => {
                    if let Err(e) = ashpd::register_host_app(id).await {
                        log::info!("couldn't register with the portal as {app_id}: {e}");
                    }
                }
                Err(e) => log::warn!("invalid app id {app_id}: {e}"),
            }
        })
        .await;
}
