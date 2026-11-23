//! Getting text in and out of the user's focused app: paste via the clipboard
//! plus a synthetic Ctrl/Cmd+V, and read the selection with a synthetic copy.
//! All functions block briefly; call them off the async runtime.
//!
//! On macOS the keystrokes are sent from the main thread: the keyboard layout is read
//! through Text Input Source APIs (here and in enigo), which abort the process when
//! called from any other thread.

use std::thread::sleep;
use std::time::Duration;

use anyhow::{anyhow, Result};
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use tauri::AppHandle;

#[cfg(target_os = "macos")]
const MODIFIER: Key = Key::Meta;
#[cfg(not(target_os = "macos"))]
const MODIFIER: Key = Key::Control;

/// A Ctrl/Cmd shortcut sent to the focused app.
#[derive(Clone, Copy)]
enum Shortcut {
    Copy,
    Paste,
}

impl Shortcut {
    /// The key to press with Ctrl/Cmd so the app reads this shortcut whatever the
    /// keyboard layout. `Key::Unicode('v')` looks the letter up in the active layout,
    /// and with one that has no "v" (Russian, Greek, ...) enigo types a "v" on Windows
    /// and presses keycode 0, the A key, on macOS.
    #[cfg(windows)]
    fn key(self) -> Key {
        // Apps match the virtual-key code, which every layout gives the V key.
        match self {
            Shortcut::Copy => Key::C,
            Shortcut::Paste => Key::V,
        }
    }

    /// Call on the main thread.
    #[cfg(target_os = "macos")]
    fn key(self) -> Key {
        // kVK_ANSI_C and kVK_ANSI_V, the keys on a US keyboard, if the layout can't be read.
        let (letter, ansi) = match self {
            Shortcut::Copy => ('c', 8),
            Shortcut::Paste => ('v', 9),
        };
        let keycode = layout::command_keycode(letter).unwrap_or_else(|| {
            log::warn!("couldn't find Cmd+{letter} in the keyboard layout; using the US key");
            ansi
        });
        Key::Other(keycode.into())
    }

    #[cfg(target_os = "linux")]
    fn letter(self) -> char {
        match self {
            Shortcut::Copy => 'c',
            Shortcut::Paste => 'v',
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    fn key(self) -> Key {
        match self {
            Shortcut::Copy => Key::Unicode('c'),
            Shortcut::Paste => Key::Unicode('v'),
        }
    }
}

/// Where the active keyboard layout puts a letter for Command shortcuts. Adapted from
/// Handy (MIT, github.com/cjpais/Handy).
#[cfg(target_os = "macos")]
mod layout {
    use std::ffi::c_void;

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        static kTISPropertyUnicodeKeyLayoutData: *const c_void;
        fn TISCopyCurrentKeyboardLayoutInputSource() -> *const c_void;
        fn TISGetInputSourceProperty(source: *const c_void, key: *const c_void) -> *const c_void;
        fn LMGetKbdType() -> u8;
        fn UCKeyTranslate(
            layout: *const u8,
            keycode: u16,
            action: u16,
            modifiers: u32,
            keyboard_type: u32,
            options: u32,
            dead_key_state: *mut u32,
            max_len: usize,
            len: *mut usize,
            chars: *mut u16,
        ) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
        fn CFRelease(value: *const c_void);
    }

    const KEY_ACTION_DISPLAY: u16 = 3; // kUCKeyActionDisplay
    const NO_DEAD_KEYS: u32 = 1; // kUCKeyTranslateNoDeadKeysMask
    const COMMAND: u32 = 1; // cmdKey (bit 8), shifted right 8 as UCKeyTranslate expects

    /// Releases a Text Input Source reference.
    struct Source(*const c_void);

    impl Drop for Source {
        fn drop(&mut self) {
            // SAFETY: a Create Rule reference this struct owns.
            unsafe { CFRelease(self.0) };
        }
    }

    /// The keycode the active layout reads as `letter` while Command is held, which is
    /// what apps match shortcuts on. Non-Latin layouts (Russian, Greek, ...) switch to
    /// QWERTY under Command, so there it's the US key, while Dvorak keeps its own
    /// letters. None if the layout can't be read or has no such key. Main thread only.
    pub fn command_keycode(letter: char) -> Option<u16> {
        let want = u16::try_from(u32::from(letter)).ok()?;
        // SAFETY: called on the main thread. The source follows the Create Rule and is
        // released when `source` drops; the layout bytes it owns stay valid until then.
        // Every out-pointer is to a local of the declared size.
        unsafe {
            let source = TISCopyCurrentKeyboardLayoutInputSource();
            if source.is_null() {
                return None;
            }
            let source = Source(source);
            let data = TISGetInputSourceProperty(source.0, kTISPropertyUnicodeKeyLayoutData);
            if data.is_null() {
                return None;
            }
            let layout = CFDataGetBytePtr(data);
            if layout.is_null() {
                return None;
            }
            let keyboard_type = u32::from(LMGetKbdType());
            for keycode in 0..128 {
                let (mut dead_keys, mut len, mut chars) = (0, 0, [0u16; 4]);
                let status = UCKeyTranslate(
                    layout,
                    keycode,
                    KEY_ACTION_DISPLAY,
                    COMMAND,
                    keyboard_type,
                    NO_DEAD_KEYS,
                    &mut dead_keys,
                    chars.len(),
                    &mut len,
                    chars.as_mut_ptr(),
                );
                if status == 0 && len == 1 && chars[0] == want {
                    return Some(keycode);
                }
            }
            None
        }
    }
}

fn enigo() -> Result<Enigo> {
    Enigo::new(&Settings::default()).map_err(|e| anyhow!("Keyboard control unavailable: {e}"))
}

fn chord(enigo: &mut Enigo, key: Key) -> Result<()> {
    let run = |e: &mut Enigo| -> enigo::InputResult<()> {
        e.key(MODIFIER, Direction::Press)?;
        e.key(key, Direction::Click)?;
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

/// Releases any held modifiers, waits `settle`, then sends the shortcut.
fn shortcut(app: &AppHandle, shortcut: Shortcut, settle: Duration) -> Result<()> {
    #[cfg(target_os = "linux")]
    if let Some(result) = wayland::ctrl(shortcut.letter(), settle) {
        return result;
    }
    let press = move || -> Result<()> {
        let mut enigo = enigo()?;
        release_modifiers(&mut enigo);
        sleep(settle);
        chord(&mut enigo, shortcut.key())
    };
    if cfg!(target_os = "macos") {
        // Runs inline if this already is the main thread, so waiting can't deadlock.
        let (tx, rx) = std::sync::mpsc::channel();
        app.run_on_main_thread(move || {
            let _ = tx.send(press());
        })?;
        rx.recv().map_err(|_| anyhow!("Couldn't send keystroke: the app is shutting down"))?
    } else {
        press()
    }
}

/// Types `text` into the focused app by pasting it.
pub fn paste(app: &AppHandle, text: &str, restore_clipboard: bool) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let mut clipboard = Clipboard::new()?;
    let previous = restore_clipboard.then(|| saved::save(&mut clipboard));
    clipboard.set_text(text)?;
    sleep(Duration::from_millis(40));

    shortcut(app, Shortcut::Paste, Duration::ZERO)?;

    if let Some(previous) = previous {
        // Give the target app time to read the clipboard before swapping it back.
        sleep(Duration::from_millis(350));
        saved::restore(&mut clipboard, previous);
    }
    Ok(())
}

/// What was on the clipboard before a paste, so it can be put back. Every format is
/// kept, so images, files and rich text come back too, and an empty clipboard is left
/// empty. Formats held as GDI handles rather than memory are skipped; Windows
/// synthesizes the bitmap ones from the DIB that is kept.
#[cfg(windows)]
mod saved {
    use arboard::Clipboard;
    use clipboard_win::raw;

    pub struct Saved(Vec<(u32, Vec<u8>)>);

    /// CF_BITMAP, CF_METAFILEPICT, CF_PALETTE, CF_ENHMETAFILE, CF_OWNERDISPLAY,
    /// CF_DSPBITMAP, CF_DSPMETAFILEPICT, CF_DSPENHMETAFILE, and the private and GDI
    /// object ranges.
    fn is_handle(format: u32) -> bool {
        matches!(format, 2 | 3 | 9 | 14 | 0x80 | 0x82 | 0x83 | 0x8E | 0x200..=0x3FF)
    }

    pub fn save(_clipboard: &mut Clipboard) -> Saved {
        let Ok(_open) = clipboard_win::Clipboard::new_attempts(10) else {
            return Saved(Vec::new());
        };
        let formats = raw::EnumFormats::new().filter(|&f| !is_handle(f)).collect::<Vec<_>>();
        let read = |format| {
            let mut data = Vec::new();
            raw::get_vec(format, &mut data).ok().map(|_| (format, data))
        };
        Saved(formats.into_iter().filter_map(read).collect())
    }

    pub fn restore(_clipboard: &mut Clipboard, saved: Saved) {
        let Ok(_open) = clipboard_win::Clipboard::new_attempts(10) else { return };
        let _ = raw::empty();
        for (format, data) in &saved.0 {
            let _ = raw::set_without_clear(*format, data);
        }
    }
}

/// Keystrokes through the RemoteDesktop portal in Wayland sessions, where the X11 ones
/// enigo sends only reach apps running under XWayland. The clipboard needs nothing
/// special: the desktop bridges the X11 clipboard arboard uses to Wayland apps.
#[cfg(target_os = "linux")]
pub mod wayland {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::Duration;

    use anyhow::Result;
    use tauri::{AppHandle, Emitter, Manager};
    use tokio::sync::Mutex;

    use crate::permissions::Access;
    use crate::portal::{self, keyboard::{self, Keyboard}};
    use crate::AppState;

    const STARTING: u8 = 0;
    const READY: u8 = 1;
    const NOT_ALLOWED: u8 = 2;
    const UNAVAILABLE: u8 = 3;
    static STATE: AtomicU8 = AtomicU8::new(STARTING);
    static KEYBOARD: Mutex<Option<Keyboard>> = Mutex::const_new(None);

    /// The restore token that skips the desktop's dialog. Empty once the user has
    /// declined, so Lipwise doesn't ask again at every start.
    fn token_path(app: &AppHandle) -> PathBuf {
        app.state::<AppState>().data_dir.join("remote-desktop-token")
    }

    pub fn access() -> Access {
        match STATE.load(Ordering::Relaxed) {
            READY => Access::Granted,
            NOT_ALLOWED => Access::Ask,
            _ => Access::Limited,
        }
    }

    fn set(app: &AppHandle, state: u8) {
        STATE.store(state, Ordering::Relaxed);
        let _ = app.emit("permissions-changed", ());
    }

    /// Opens the keyboard session in the background, asking the user if there's no
    /// saved permission. At launch (`ask` false), not if they declined before.
    pub fn start(app: &AppHandle, ask: bool) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _dialog = portal::DIALOGS.lock().await;
            let mut current = KEYBOARD.lock().await;
            if current.as_ref().is_some_and(Keyboard::is_open) {
                return;
            }
            portal::register(&app.config().identifier).await;
            if !keyboard::available().await {
                log::info!("no RemoteDesktop portal with a keyboard; typing reaches XWayland apps only");
                return set(&app, UNAVAILABLE);
            }
            let path = token_path(&app);
            let saved = std::fs::read_to_string(&path).ok();
            if !ask && saved.as_deref() == Some("") {
                return set(&app, NOT_ALLOWED);
            }
            let closed = app.clone();
            let on_closed = move || {
                log::info!("the desktop ended keyboard control");
                set(&closed, NOT_ALLOWED);
            };
            match Keyboard::start(saved.as_deref(), on_closed).await {
                Ok((keyboard, token)) => {
                    log::info!("typing through the RemoteDesktop portal");
                    *current = Some(keyboard);
                    let _ = match token {
                        Some(token) => std::fs::write(&path, token),
                        // Nothing to restore from: ask again next launch.
                        None => std::fs::remove_file(&path),
                    };
                    set(&app, READY);
                }
                Err(e) => {
                    log::warn!("keyboard control through the portal not allowed: {e}");
                    let _ = std::fs::write(&path, "");
                    set(&app, NOT_ALLOWED);
                }
            }
        });
    }

    /// Waits `settle`, then sends Ctrl+`letter` through the portal. None if its session
    /// isn't open, for the caller to fall back to X11 keystrokes. The portal's keyboard
    /// is a device of its own, so keys the user still holds can't be released for
    /// them; `settle` gives them time to let go. Call off the async runtime.
    pub fn ctrl(letter: char, settle: Duration) -> Option<Result<()>> {
        if STATE.load(Ordering::Relaxed) != READY {
            return None;
        }
        tauri::async_runtime::block_on(async {
            // Only busy while starting, when it isn't READY anyway.
            let current = KEYBOARD.try_lock().ok()?;
            let session = current.as_ref().filter(|k| k.is_open())?;
            tokio::time::sleep(settle).await;
            Some(session.chord(keyboard::CONTROL, keyboard::letter(letter)).await)
        })
    }
}

/// What was on the clipboard before a paste, so it can be put back: files, an image,
/// rich text (HTML) or plain text, whichever was there, or nothing. These are the
/// types arboard can write; other formats an app offers (RTF, its own) are lost.
#[cfg(not(any(target_os = "macos", windows)))]
mod saved {
    use std::path::PathBuf;

    use arboard::{Clipboard, ImageData};

    pub enum Saved {
        Empty,
        Files(Vec<PathBuf>),
        Image(ImageData<'static>),
        Html(String, Option<String>),
        Text(String),
    }

    pub fn save(clipboard: &mut Clipboard) -> Saved {
        match clipboard.get().file_list() {
            Ok(files) if !files.is_empty() => return Saved::Files(files),
            _ => {}
        }
        if let Ok(image) = clipboard.get_image() {
            return Saved::Image(image);
        }
        let text = clipboard.get_text().ok();
        if let Ok(html) = clipboard.get().html() {
            return Saved::Html(html, text);
        }
        text.map_or(Saved::Empty, Saved::Text)
    }

    pub fn restore(clipboard: &mut Clipboard, saved: Saved) {
        let _ = match saved {
            Saved::Empty => clipboard.clear(),
            Saved::Files(files) => clipboard.set().file_list(&files),
            Saved::Image(image) => clipboard.set_image(image),
            Saved::Html(html, text) => clipboard.set_html(html, text),
            Saved::Text(text) => clipboard.set_text(text),
        };
    }
}

/// What was on the clipboard before a paste, so it can be put back. Every item is kept
/// in every type it offers, so images, files and rich text come back too, and an
/// empty clipboard is left empty.
#[cfg(target_os = "macos")]
mod saved {
    use arboard::Clipboard;
    use objc2::rc::{autoreleasepool, Retained};
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardType, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSData};

    pub struct Saved(Vec<Vec<(Retained<NSPasteboardType>, Retained<NSData>)>>);

    pub fn save(_clipboard: &mut Clipboard) -> Saved {
        autoreleasepool(|_| {
            let Some(items) = NSPasteboard::generalPasteboard().pasteboardItems() else {
                return Saved(Vec::new());
            };
            let read = |item: Retained<NSPasteboardItem>| {
                let types = item.types();
                types.iter().filter_map(|ty| item.dataForType(&ty).map(|data| (ty, data))).collect()
            };
            Saved(items.iter().map(read).collect())
        })
    }

    pub fn restore(_clipboard: &mut Clipboard, saved: Saved) {
        autoreleasepool(|_| {
            let pasteboard = NSPasteboard::generalPasteboard();
            pasteboard.clearContents();
            let items: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = saved
                .0
                .iter()
                .filter(|types| !types.is_empty())
                .map(|types| {
                    let item = NSPasteboardItem::new();
                    for (ty, data) in types {
                        item.setData_forType(data, ty);
                    }
                    ProtocolObject::from_retained(item)
                })
                .collect();
            if !items.is_empty() {
                pasteboard.writeObjects(&NSArray::from_retained_slice(&items));
            }
        })
    }
}

/// Returns the text currently selected in the focused app ("" if none).
pub fn copy_selection(app: &AppHandle) -> Result<String> {
    let mut clipboard = Clipboard::new()?;
    let previous = clipboard.get_text().ok();
    // A sentinel tells "nothing selected" apart from "selection equals old clipboard".
    let sentinel = format!("\u{2063}lipwise-{}", std::process::id());
    clipboard.set_text(sentinel.clone())?;

    shortcut(app, Shortcut::Copy, Duration::from_millis(30))?;

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
