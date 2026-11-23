//! Keystrokes into other apps through the RemoteDesktop portal, for Wayland sessions.
//!
//! Synthetic X11 keystrokes only reach apps running under XWayland, and a Wayland
//! compositor gives an ordinary app no way to type into another. The RemoteDesktop
//! portal does, with the user's permission: asking for the keyboard alone, the desktop
//! shows a dialog once, and the restore token it hands back skips the dialog next time.
//! While the session is open, GNOME shows a sharing indicator in the top bar.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use ashpd::desktop::remote_desktop::{DeviceType, KeyState, RemoteDesktop, SelectDevicesOptions, StartOptions};
use ashpd::desktop::{CreateSessionOptions, PersistMode, Session};
use futures_util::StreamExt;

/// XK_Control_L.
pub const CONTROL: i32 = 0xffe3;

/// The keysym of a lowercase ASCII letter, which is its code point. Sent as a keysym,
/// the compositor finds the key for it in whatever layout is active.
pub fn letter(c: char) -> i32 {
    debug_assert!(c.is_ascii_lowercase());
    c as i32
}

/// Whether the desktop offers keyboard control through the portal at all.
pub async fn available() -> bool {
    match RemoteDesktop::new().await {
        Ok(proxy) => proxy
            .available_device_types()
            .await
            .is_ok_and(|types| types.contains(DeviceType::KEYBOARD)),
        Err(_) => false,
    }
}

pub struct Keyboard {
    proxy: RemoteDesktop,
    session: Arc<Session<RemoteDesktop>>,
    open: Arc<AtomicBool>,
}

impl Keyboard {
    /// Opens a keyboard-only session. Without a valid `restore_token` the desktop asks
    /// the user, so this can wait on them; it fails if they decline. Returns the token
    /// to pass next time (each token works once). `on_closed` runs if the desktop
    /// ends the session, e.g. the user stops it from the top bar. Must run on Tokio.
    pub async fn start(
        restore_token: Option<&str>,
        on_closed: impl FnOnce() + Send + 'static,
    ) -> Result<(Self, Option<String>)> {
        let proxy = RemoteDesktop::new().await?;
        let session = proxy.create_session(CreateSessionOptions::default()).await?;
        let options = SelectDevicesOptions::default()
            .set_devices(DeviceType::KEYBOARD)
            .set_persist_mode(PersistMode::ExplicitlyRevoked)
            .set_restore_token(restore_token.filter(|t| !t.is_empty()));
        proxy.select_devices(&session, options).await?.response()?;
        let selected = proxy.start(&session, None, StartOptions::default()).await?.response()?;
        if !selected.devices().contains(DeviceType::KEYBOARD) {
            let _ = session.close().await;
            return Err(anyhow!("the keyboard wasn't shared"));
        }

        // Shared with the watcher, whose stream borrows the session.
        let session = Arc::new(session);
        let watched = session.clone();
        let open = Arc::new(AtomicBool::new(true));
        let flag = open.clone();
        tokio::spawn(async move {
            match watched.receive_closed().await {
                Ok(mut closed) => {
                    closed.next().await;
                }
                Err(e) => {
                    log::warn!("can't watch the keyboard session: {e}");
                    return;
                }
            }
            flag.store(false, Ordering::Relaxed);
            on_closed();
        });
        let token = selected.restore_token().map(str::to_owned);
        Ok((Self { proxy, session, open }, token))
    }

    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Relaxed)
    }

    /// Presses `key` while holding `modifier` (both keysyms).
    pub async fn chord(&self, modifier: i32, key: i32) -> Result<()> {
        let result = async {
            self.send(modifier, KeyState::Pressed).await?;
            self.send(key, KeyState::Pressed).await?;
            self.send(key, KeyState::Released).await
        }
        .await;
        // Never leave the modifier held down, even if the key failed.
        let released = self.send(modifier, KeyState::Released).await;
        result.and(released)
    }

    async fn send(&self, keysym: i32, state: KeyState) -> Result<()> {
        self.proxy
            .notify_keyboard_keysym(&self.session, keysym, state, Default::default())
            .await
            .map_err(|e| anyhow!("Couldn't send keystroke: {e}"))
    }
}
