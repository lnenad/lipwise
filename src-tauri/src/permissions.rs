//! What the OS lets Lipwise do: record from the microphone, and type into other apps
//! with synthetic keystrokes. Each platform gates these differently:
//! - macOS asks once for the microphone (TCC) and needs Accessibility for keystrokes.
//! - Windows has a per-user "let desktop apps use the microphone" switch and no
//!   keystroke permission.
//! - Linux has neither, but synthetic keystrokes only reach X11 apps, so on Wayland
//!   typing works only in apps running under XWayland.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // each platform uses some of these
pub enum Access {
    Granted,
    /// Not decided yet; asking shows the system prompt.
    Ask,
    /// Turned off; only the user can turn it on, in system settings.
    Denied,
    /// Works in some apps but not all (typing on Wayland).
    Limited,
    /// No microphone connected.
    Missing,
}

#[derive(Serialize, Clone, Copy, Debug)]
pub struct Permissions {
    pub microphone: Access,
    pub typing: Access,
}

pub fn check() -> Permissions {
    let microphone = match microphone() {
        Access::Granted if !crate::audio::has_microphone() => Access::Missing,
        access => access,
    };
    Permissions {
        microphone,
        typing: typing(),
    }
}

/// Shows the system prompt if the user hasn't been asked yet, otherwise opens the
/// system settings page where microphone access is turned on. Emits
/// "permissions-changed" if the prompt is answered.
pub fn request_microphone(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if microphone() == Access::Ask {
        let app = app.clone();
        mac::request_microphone(move |granted| {
            log::info!("microphone access {}", if granted { "granted" } else { "denied" });
            let _ = tauri::Emitter::emit(&app, "permissions-changed", ());
        });
        return;
    }
    open_settings(app, MICROPHONE_SETTINGS);
}

/// Asks for the Accessibility access synthetic keystrokes need (macOS only). The first
/// time this shows the system prompt, which also adds Lipwise to the list in System
/// Settings; after that it opens that list directly.
pub fn request_typing(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static PROMPTED: AtomicBool = AtomicBool::new(false);
        if !PROMPTED.swap(true, Ordering::Relaxed) {
            mac::prompt_accessibility();
        } else {
            open_settings(app, ACCESSIBILITY_SETTINGS);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

fn open_settings(app: &AppHandle, url: &str) {
    if url.is_empty() {
        return;
    }
    if let Err(e) = app.opener().open_url(url, None::<&str>) {
        log::warn!("couldn't open {url}: {e}");
    }
}

// ---------------------------------------------------------------- macOS

#[cfg(target_os = "macos")]
const MICROPHONE_SETTINGS: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";
#[cfg(target_os = "macos")]
const ACCESSIBILITY_SETTINGS: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

#[cfg(target_os = "macos")]
pub fn microphone() -> Access {
    mac::microphone()
}

#[cfg(target_os = "macos")]
pub fn typing() -> Access {
    if mac::accessibility_trusted() {
        Access::Granted
    } else {
        Access::Denied
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;

    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, Bool};
    use objc2_foundation::{NSDictionary, NSNumber, NSString};

    use super::Access;

    // Loads AVCaptureDevice, which is looked up by name below.
    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {}

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> u8;
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
        static kAXTrustedCheckOptionPrompt: *const c_void;
    }

    fn capture_device() -> Option<&'static AnyClass> {
        AnyClass::get(c"AVCaptureDevice")
    }

    /// AVMediaTypeAudio.
    fn audio() -> Retained<NSString> {
        NSString::from_str("soun")
    }

    pub fn microphone() -> Access {
        let Some(cls) = capture_device() else {
            return Access::Granted;
        };
        // AVAuthorizationStatus: notDetermined, restricted, denied, authorized.
        let status: isize = unsafe { msg_send![cls, authorizationStatusForMediaType: &*audio()] };
        match status {
            0 => Access::Ask,
            3 => Access::Granted,
            _ => Access::Denied,
        }
    }

    /// Shows the system microphone prompt. `done` runs on an arbitrary thread.
    pub fn request_microphone(done: impl Fn(bool) + 'static) {
        let Some(cls) = capture_device() else { return };
        let block = RcBlock::new(move |granted: Bool| done(granted.as_bool()));
        unsafe {
            let _: () = msg_send![cls, requestAccessForMediaType: &*audio(), completionHandler: &*block];
        }
    }

    pub fn accessibility_trusted() -> bool {
        unsafe { AXIsProcessTrusted() != 0 }
    }

    pub fn prompt_accessibility() {
        // SAFETY: kAXTrustedCheckOptionPrompt is a CFString, toll-free bridged to NSString.
        let key: &NSString = unsafe { &*(kAXTrustedCheckOptionPrompt as *const NSString) };
        let yes = NSNumber::new_bool(true);
        let options = NSDictionary::from_slices(&[key], &[&*yes]);
        unsafe { AXIsProcessTrustedWithOptions(Retained::as_ptr(&options) as *const c_void) };
    }
}

// ---------------------------------------------------------------- Windows

#[cfg(windows)]
const MICROPHONE_SETTINGS: &str = "ms-settings:privacy-microphone";

/// Windows has three switches, any of which blocks desktop apps: microphone access for
/// the device, for this user's apps, and for desktop (non-Store) apps. Unset means on.
#[cfg(windows)]
pub fn microphone() -> Access {
    const KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
    let denied = [
        win::read_string(win::HKEY_LOCAL_MACHINE, KEY, "Value"),
        win::read_string(win::HKEY_CURRENT_USER, KEY, "Value"),
        win::read_string(win::HKEY_CURRENT_USER, &format!(r"{KEY}\NonPackaged"), "Value"),
    ]
    .iter()
    .any(|v| v.as_deref() == Some("Deny"));
    if denied {
        Access::Denied
    } else {
        Access::Granted
    }
}

#[cfg(windows)]
pub fn typing() -> Access {
    Access::Granted
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    pub type Hkey = isize;
    // Predefined handles are sign-extended 32-bit values.
    pub const HKEY_CURRENT_USER: Hkey = 0x8000_0001u32 as i32 as isize;
    pub const HKEY_LOCAL_MACHINE: Hkey = 0x8000_0002u32 as i32 as isize;
    const RRF_RT_REG_SZ: u32 = 0x2;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(
            key: Hkey,
            subkey: *const u16,
            value: *const u16,
            flags: u32,
            kind: *mut u32,
            data: *mut c_void,
            len: *mut u32,
        ) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub fn read_string(root: Hkey, subkey: &str, value: &str) -> Option<String> {
        let mut buf = [0u16; 64];
        let mut len = std::mem::size_of_val(&buf) as u32;
        // SAFETY: NUL-terminated names and a buffer whose size in bytes is passed in `len`.
        let status = unsafe {
            RegGetValueW(
                root,
                wide(subkey).as_ptr(),
                wide(value).as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut c_void,
                &mut len,
            )
        };
        if status != 0 {
            return None;
        }
        let chars = (len as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buf[..chars]))
    }
}

// ---------------------------------------------------------------- Linux

#[cfg(not(any(target_os = "macos", windows)))]
const MICROPHONE_SETTINGS: &str = "";

#[cfg(not(any(target_os = "macos", windows)))]
pub fn microphone() -> Access {
    Access::Granted
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn typing() -> Access {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t.eq_ignore_ascii_case("wayland"));
    if wayland {
        Access::Limited
    } else {
        Access::Granted
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn reads_registry_strings() {
        let name = win::read_string(win::HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "ProductName");
        assert!(name.as_deref().is_some_and(|n| n.starts_with("Windows")), "{name:?}");
        assert_eq!(win::read_string(win::HKEY_CURRENT_USER, r"Software\Lipwise\NoSuchKey", "Value"), None);
        // Whatever this machine's settings are, the check returns a decision.
        assert!(matches!(microphone(), Access::Granted | Access::Denied));
    }
}
