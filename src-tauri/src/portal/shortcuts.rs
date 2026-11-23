//! Global shortcuts through the GlobalShortcuts portal, for Wayland sessions.
//!
//! The global-shortcut plugin grabs keys through X11. In a Wayland session those grabs
//! only fire while an XWayland window has focus, so the shortcuts seem dead in most
//! apps. Wayland desktops offer this portal instead (GNOME 48+, KDE Plasma, Hyprland):
//! the app proposes triggers, the desktop asks the user to confirm them once, then
//! reports each press and release.

use std::sync::Arc;

use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
use ashpd::desktop::{CreateSessionOptions, Session};
use futures_util::StreamExt;
use tokio::sync::Mutex;

/// A shortcut to ask the desktop for.
pub struct Wanted {
    /// Stable id the desktop stores the user's choice under.
    pub id: &'static str,
    /// Shown to the user in the desktop's shortcut settings.
    pub description: &'static str,
    /// In Lipwise's own format, e.g. "ctrl+shift+Space".
    pub shortcut: String,
}

/// A shortcut the desktop bound, and its trigger as the desktop describes it.
#[derive(Debug, Clone)]
pub struct Bound {
    pub id: String,
    pub trigger: String,
}

pub struct Portal {
    proxy: GlobalShortcuts,
    session: Mutex<Option<Session<GlobalShortcuts>>>,
}

impl Portal {
    /// Connects to the portal and starts reporting shortcut presses (`true`) and
    /// releases (`false`) to `on_event`, by shortcut id. Must run on a Tokio runtime.
    pub async fn connect(
        app_id: &str,
        on_event: impl Fn(&str, bool) + Send + Sync + 'static,
    ) -> ashpd::Result<Self> {
        super::register(app_id).await;
        let proxy = GlobalShortcuts::new().await?;
        let on_event = Arc::new(on_event);
        let mut activated = proxy.receive_activated().await?;
        let mut deactivated = proxy.receive_deactivated().await?;
        let on_press = on_event.clone();
        tokio::spawn(async move {
            while let Some(a) = activated.next().await {
                on_press(a.shortcut_id(), true);
            }
        });
        tokio::spawn(async move {
            while let Some(d) = deactivated.next().await {
                on_event(d.shortcut_id(), false);
            }
        });
        Ok(Self {
            proxy,
            session: Mutex::new(None),
        })
    }

    /// Replaces the bound shortcuts. The first time, the desktop shows a dialog to
    /// confirm or change the triggers, so this can wait on the user. Afterwards it
    /// keeps the user's choice and may ignore a new preferred trigger.
    pub async fn bind(&self, wanted: &[Wanted]) -> ashpd::Result<Vec<Bound>> {
        let mut current = self.session.lock().await;
        if let Some(old) = current.take() {
            let _ = old.close().await;
        }
        let session = self.proxy.create_session(CreateSessionOptions::default()).await?;
        let shortcuts: Vec<NewShortcut> = wanted
            .iter()
            .map(|w| {
                NewShortcut::new(w.id, w.description).preferred_trigger(to_trigger(&w.shortcut).as_deref())
            })
            .collect();
        let response = self
            .proxy
            .bind_shortcuts(&session, &shortcuts, None, BindShortcutsOptions::default())
            .await?
            .response()?;
        *current = Some(session);
        Ok(response
            .shortcuts()
            .iter()
            .map(|s| Bound {
                id: s.id().to_owned(),
                trigger: s.trigger_description().to_owned(),
            })
            .collect())
    }
}

/// Converts Lipwise's shortcut format ("ctrl+shift+Space", keys as KeyboardEvent.code)
/// to the XDG shortcuts format ("CTRL+SHIFT+space", keys as xkb keysym names).
/// None when a key has no known keysym; the desktop then asks the user for one.
pub fn to_trigger(shortcut: &str) -> Option<String> {
    let mut parts = Vec::new();
    for token in shortcut.split('+').map(str::trim) {
        let part = match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => "CTRL".to_owned(),
            "alt" | "option" => "ALT".to_owned(),
            "shift" => "SHIFT".to_owned(),
            "super" | "meta" | "cmd" | "command" | "logo" => "LOGO".to_owned(),
            _ => keysym(token)?,
        };
        parts.push(part);
    }
    (!parts.is_empty()).then(|| parts.join("+"))
}

fn keysym(code: &str) -> Option<String> {
    let code = code.strip_prefix("Key").filter(|c| c.len() == 1).unwrap_or(code);
    let code = code.strip_prefix("Digit").filter(|c| c.len() == 1).unwrap_or(code);
    if code.len() == 1 && code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Some(code.to_ascii_lowercase());
    }
    if let Some(n) = code.strip_prefix(['F', 'f']).and_then(|n| n.parse::<u8>().ok()) {
        return (1..=24).contains(&n).then(|| format!("F{n}"));
    }
    if let Some(rest) = code.strip_prefix("Numpad") {
        return match rest {
            d if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) => Some(format!("KP_{d}")),
            "Add" => Some("KP_Add".into()),
            "Subtract" => Some("KP_Subtract".into()),
            "Multiply" => Some("KP_Multiply".into()),
            "Divide" => Some("KP_Divide".into()),
            "Decimal" => Some("KP_Decimal".into()),
            "Enter" => Some("KP_Enter".into()),
            _ => None,
        };
    }
    let name = match code {
        "Space" | "space" => "space",
        "Enter" | "Return" => "Return",
        "Tab" => "Tab",
        "Backspace" => "BackSpace",
        "Delete" => "Delete",
        "Insert" => "Insert",
        "Home" => "Home",
        "End" => "End",
        "PageUp" => "Page_Up",
        "PageDown" => "Page_Down",
        "ArrowUp" | "Up" => "Up",
        "ArrowDown" | "Down" => "Down",
        "ArrowLeft" | "Left" => "Left",
        "ArrowRight" | "Right" => "Right",
        "Escape" => "Escape",
        "Minus" => "minus",
        "Equal" => "equal",
        "BracketLeft" => "bracketleft",
        "BracketRight" => "bracketright",
        "Backslash" => "backslash",
        "Semicolon" => "semicolon",
        "Quote" => "apostrophe",
        "Backquote" => "grave",
        "Comma" => "comma",
        "Period" => "period",
        "Slash" => "slash",
        "Pause" => "Pause",
        "ScrollLock" => "Scroll_Lock",
        "PrintScreen" => "Print",
        _ => return None,
    };
    Some(name.to_owned())
}

/// Whether the desktop's description of a trigger names the same keys as a shortcut in
/// Lipwise's format. Desktops word triggers their own way ("Shift+Ctrl+Space",
/// "<Control><Shift>space"), so this compares the sets of key names.
pub fn same_keys(shortcut: &str, trigger_description: &str) -> bool {
    fn keys(text: &str) -> Vec<String> {
        let mut keys: Vec<String> = text
            .split(['+', '<', '>', ' '])
            .filter(|t| !t.is_empty())
            .map(|t| match t.to_ascii_lowercase().as_str() {
                "control" | "ctl" | "primary" => "ctrl".to_owned(),
                "super" | "meta" | "logo" | "win" | "windows" => "logo".to_owned(),
                "option" => "alt".to_owned(),
                other => other.to_owned(),
            })
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }
    match to_trigger(shortcut) {
        Some(trigger) => keys(&trigger) == keys(trigger_description),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_shortcuts_to_xdg_triggers() {
        assert_eq!(to_trigger("ctrl+shift+Space").as_deref(), Some("CTRL+SHIFT+space"));
        assert_eq!(to_trigger("ctrl+alt+shift+Space").as_deref(), Some("CTRL+ALT+SHIFT+space"));
        assert_eq!(to_trigger("super+KeyD").as_deref(), Some("LOGO+d"));
        assert_eq!(to_trigger("ctrl+Digit1").as_deref(), Some("CTRL+1"));
        assert_eq!(to_trigger("ctrl+shift+D").as_deref(), Some("CTRL+SHIFT+d"));
        assert_eq!(to_trigger("F9").as_deref(), Some("F9"));
        assert_eq!(to_trigger("alt+BracketLeft").as_deref(), Some("ALT+bracketleft"));
        assert_eq!(to_trigger("ctrl+Numpad5").as_deref(), Some("CTRL+KP_5"));
        assert_eq!(to_trigger("ctrl+IntlRo"), None);
    }

    #[test]
    fn compares_desktop_trigger_descriptions() {
        assert!(same_keys("ctrl+shift+Space", "Shift+Ctrl+Space"));
        assert!(same_keys("ctrl+shift+Space", "<Control><Shift>space"));
        assert!(same_keys("super+KeyD", "Super+D"));
        assert!(!same_keys("ctrl+shift+Space", "Ctrl+Space"));
    }
}
