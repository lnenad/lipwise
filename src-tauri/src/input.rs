//! Getting text in and out of the user's focused app: paste via the clipboard
//! plus a synthetic Ctrl/Cmd+V, and read the selection with a synthetic copy.
//! All functions block briefly; call them off the async runtime.

use std::thread::sleep;
use std::time::Duration;

use anyhow::{anyhow, Result};
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

#[cfg(target_os = "macos")]
const MODIFIER: Key = Key::Meta;
#[cfg(not(target_os = "macos"))]
const MODIFIER: Key = Key::Control;

fn enigo() -> Result<Enigo> {
    Enigo::new(&Settings::default()).map_err(|e| anyhow!("Keyboard control unavailable: {e}"))
}

fn chord(enigo: &mut Enigo, key: char) -> Result<()> {
    let run = |e: &mut Enigo| -> enigo::InputResult<()> {
        e.key(MODIFIER, Direction::Press)?;
        e.key(Key::Unicode(key), Direction::Click)?;
        e.key(MODIFIER, Direction::Release)
    };
    run(enigo).map_err(|e| anyhow!("Couldn't send keystroke: {e}"))
}

/// Release modifiers the user may still be holding from the shortcut, so a
/// synthetic Ctrl+C doesn't become Ctrl+Shift+C (devtools in some apps).
fn release_modifiers(enigo: &mut Enigo) {
    let mut keys = vec![Key::Shift, Key::Control, Key::Meta];
    // A stray Alt key-up opens the menu bar on Windows; only release it on macOS.
    if cfg!(target_os = "macos") {
        keys.push(Key::Alt);
    }
    for key in keys {
        let _ = enigo.key(key, Direction::Release);
    }
}

/// Types `text` into the focused app by pasting it.
pub fn paste(text: &str, restore_clipboard: bool) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let mut clipboard = Clipboard::new()?;
    let previous = if restore_clipboard {
        clipboard.get_text().ok()
    } else {
        None
    };
    clipboard.set_text(text)?;
    sleep(Duration::from_millis(40));

    let mut enigo = enigo()?;
    release_modifiers(&mut enigo);
    chord(&mut enigo, 'v')?;

    if let Some(previous) = previous {
        // Give the target app time to read the clipboard before swapping it back.
        sleep(Duration::from_millis(350));
        let _ = clipboard.set_text(previous);
    }
    Ok(())
}

/// Returns the text currently selected in the focused app ("" if none).
pub fn copy_selection() -> Result<String> {
    let mut clipboard = Clipboard::new()?;
    let previous = clipboard.get_text().ok();
    // A sentinel tells "nothing selected" apart from "selection equals old clipboard".
    let sentinel = format!("\u{2063}lipwise-{}", std::process::id());
    clipboard.set_text(sentinel.clone())?;

    let mut enigo = enigo()?;
    release_modifiers(&mut enigo);
    sleep(Duration::from_millis(30));
    chord(&mut enigo, 'c')?;

    let mut selection = String::new();
    for _ in 0..10 {
        sleep(Duration::from_millis(30));
        match clipboard.get_text() {
            Ok(t) if t != sentinel => {
                selection = t;
                break;
            }
            _ => {}
        }
    }
    match previous {
        Some(p) => clipboard.set_text(p)?,
        None => clipboard.clear()?,
    }
    Ok(selection)
}
