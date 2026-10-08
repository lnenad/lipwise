use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// llama.cpp server set up and run by Lipwise itself.
    Local,
    Anthropic,
    OpenAI,
    Ollama,
    Custom,
}

/// Window appearance. `System` follows the OS light/dark setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Providers {
    pub anthropic: ProviderConfig,
    pub openai: ProviderConfig,
    pub ollama: ProviderConfig,
    pub custom: ProviderConfig,
}

impl Default for Providers {
    fn default() -> Self {
        Self {
            anthropic: ProviderConfig {
                base_url: "https://api.anthropic.com".into(),
                model: "claude-opus-5-5".into(),
                api_key: String::new(),
            },
            openai: ProviderConfig {
                base_url: "https://api.openai.com/v1".into(),
                model: "gpt-4.1-mini".into(),
                api_key: String::new(),
            },
            ollama: ProviderConfig {
                base_url: "http://localhost:11434/v1".into(),
                model: "llama3.2".into(),
                api_key: String::new(),
            },
            custom: ProviderConfig {
                base_url: "http://localhost:1234/v1".into(),
                model: String::new(),
                api_key: String::new(),
            },
        }
    }
}

impl Providers {
    /// Each provider with the name its API key is stored under.
    fn named_mut(&mut self) -> [(&'static str, &mut ProviderConfig); 4] {
        [
            ("anthropic", &mut self.anthropic),
            ("openai", &mut self.openai),
            ("ollama", &mut self.ollama),
            ("custom", &mut self.custom),
        ]
    }

    pub fn get(&self, provider: Provider) -> &ProviderConfig {
        match provider {
            // The local server has no user-editable config; its address is only
            // known at runtime (see local_ai::running_url), so callers handle it first.
            Provider::Local => &self.custom,
            Provider::Anthropic => &self.anthropic,
            Provider::OpenAI => &self.openai,
            Provider::Ollama => &self.ollama,
            Provider::Custom => &self.custom,
        }
    }
}

/// The model and llama-server binary set up by the local AI installer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalAi {
    pub model_id: String,
    pub model_path: String,
    pub server_path: String,
}

impl LocalAi {
    pub fn configured(&self) -> bool {
        !self.model_path.is_empty() && !self.server_path.is_empty()
    }
}

/// A user-defined voice command: when the user says `phrase`, the AI does `action`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCommand {
    pub phrase: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Shortcut for smart dictation (speak text + inline commands).
    pub dictate_shortcut: String,
    /// Shortcut for "command mode": speak an instruction applied to the selected text.
    pub command_shortcut: String,
    /// Shortcut for plain dictation: type exactly what was heard, no AI.
    pub plain_shortcut: String,
    /// Hold the shortcut while talking (true) or press once to start / again to stop (false).
    pub push_to_talk: bool,
    /// Input device name; `None` uses the system default.
    pub microphone: Option<String>,
    /// Catalog id of the active speech model.
    pub selected_model: Option<String>,
    /// ISO language hint or "auto".
    pub language: String,

    /// Run transcripts through the LLM. Off = plain dictation like Handy.
    pub ai_enabled: bool,
    pub provider: Provider,
    pub providers: Providers,
    pub local_ai: LocalAi,
    /// Load the local model when Lipwise starts. Off = it runs only after you press Start.
    pub local_ai_autostart: bool,
    /// Optional wake word. When set, only speech starting with it counts as a command.
    pub command_word: String,
    /// Free-form style instructions appended to every prompt.
    pub custom_instructions: String,
    /// Names and terms the transcriber and the AI should spell correctly.
    pub vocabulary: Vec<String>,
    pub voice_commands: Vec<VoiceCommand>,

    /// Put the previous clipboard contents back after pasting.
    pub restore_clipboard: bool,
    pub show_overlay: bool,
    pub history_limit: usize,
    pub theme: Theme,
    /// Open Lipwise when you log in to the computer.
    pub launch_at_login: bool,
    /// When opened at login, stay in the tray without showing the window.
    pub start_hidden: bool,
    /// Download new releases in the background; they install on the next restart.
    pub auto_update: bool,
}

impl Default for Settings {
    fn default() -> Self {
        // Deliberately not Handy's defaults (ctrl/alt+Space) so both apps can run side by side.
        let (dictate, command, plain) = if cfg!(target_os = "macos") {
            ("alt+shift+Space", "ctrl+alt+Space", "ctrl+alt+shift+Space")
        } else {
            ("ctrl+shift+Space", "ctrl+alt+Space", "ctrl+alt+shift+Space")
        };
        Self {
            dictate_shortcut: dictate.into(),
            command_shortcut: command.into(),
            plain_shortcut: plain.into(),
            push_to_talk: true,
            microphone: None,
            selected_model: None,
            language: "auto".into(),
            ai_enabled: true,
            provider: Provider::Anthropic,
            providers: Providers::default(),
            local_ai: LocalAi::default(),
            local_ai_autostart: true,
            command_word: String::new(),
            custom_instructions: String::new(),
            vocabulary: Vec::new(),
            voice_commands: vec![
                VoiceCommand {
                    phrase: "sign off".into(),
                    action: "Insert a polite email closing: a new paragraph with \"Best regards,\"".into(),
                },
                VoiceCommand {
                    phrase: "make it a list".into(),
                    action: "Reformat the text so far as a bulleted list".into(),
                },
            ],
            restore_clipboard: true,
            show_overlay: true,
            history_limit: 200,
            theme: Theme::System,
            launch_at_login: false,
            start_hidden: true,
            auto_update: true,
        }
    }
}

impl Settings {
    pub fn active_provider(&self) -> &ProviderConfig {
        self.providers.get(self.provider)
    }

    /// True when the AI step can actually run (enabled and minimally configured).
    pub fn ai_ready(&self) -> bool {
        if !self.ai_enabled {
            return false;
        }
        if self.provider == Provider::Local {
            return self.local_ai.configured();
        }
        let cfg = self.active_provider();
        let has_key = !cfg.api_key.trim().is_empty() || env_key(self.provider).is_some();
        match self.provider {
            Provider::Anthropic | Provider::OpenAI => has_key && !cfg.model.trim().is_empty(),
            Provider::Local | Provider::Ollama | Provider::Custom => {
                !cfg.base_url.trim().is_empty() && !cfg.model.trim().is_empty()
            }
        }
    }
}

/// API keys may also come from the environment so they never have to be stored on disk.
pub fn env_key(provider: Provider) -> Option<String> {
    let var = match provider {
        Provider::Anthropic => "ANTHROPIC_API_KEY",
        Provider::OpenAI => "OPENAI_API_KEY",
        _ => return None,
    };
    std::env::var(var).ok().filter(|v| !v.trim().is_empty())
}

/// Reads settings.json and fills in the API keys from the OS credential store.
/// Keys still in the file (saved by an older version, or because the store wasn't
/// available) are moved into the store.
pub fn load(path: &PathBuf) -> Settings {
    let mut settings = match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
            log::warn!("settings file unreadable ({e}); using defaults");
            Settings::default()
        }),
        Err(_) => Settings::default(),
    };
    let mut keys_in_file = false;
    for (name, cfg) in settings.providers.named_mut() {
        if !cfg.api_key.is_empty() {
            keys_in_file = true;
            continue;
        }
        match keys::get(name) {
            Ok(key) => cfg.api_key = key,
            Err(e) => log::warn!("couldn't read the {name} API key from the credential store: {e}"),
        }
    }
    if keys_in_file && path.is_file() {
        if let Err(e) = save(path, &settings) {
            log::warn!("couldn't move API keys out of the settings file: {e}");
        }
    }
    settings
}

/// Writes settings.json, with the API keys going to the OS credential store instead.
/// A key the store won't take stays in the file so it isn't lost.
pub fn save(path: &PathBuf, settings: &Settings) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = settings.clone();
    for (name, cfg) in file.providers.named_mut() {
        match keys::set(name, &cfg.api_key) {
            Ok(()) => cfg.api_key.clear(),
            Err(e) if cfg.api_key.is_empty() => {
                log::warn!("couldn't remove the {name} API key from the credential store: {e}")
            }
            Err(e) => log::warn!("couldn't store the {name} API key in the credential store, keeping it in the settings file: {e}"),
        }
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&file)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// API keys in the OS credential store: the Keychain on macOS, Credential Manager on
/// Windows, the Secret Service (GNOME Keyring, KWallet) on Linux.
mod keys {
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[cfg(not(test))]
    const SERVICE: &str = "app.lipwise.desktop";
    // Tests never touch the user's real keys.
    #[cfg(test)]
    const SERVICE: &str = "app.lipwise.desktop.test";

    /// What the store holds for each provider, as far as this process knows, so
    /// saving settings only touches the store when a key actually changed.
    static KNOWN: Mutex<Option<HashMap<&'static str, String>>> = Mutex::new(None);

    fn remember(name: &'static str, key: &str) {
        KNOWN.lock().unwrap().get_or_insert_with(HashMap::new).insert(name, key.to_string());
    }

    fn known(name: &str) -> Option<String> {
        KNOWN.lock().unwrap().as_ref()?.get(name).cloned()
    }

    fn entry(name: &str) -> keyring::Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, name)
    }

    /// The stored key, or an empty string if there is none.
    pub fn get(name: &'static str) -> keyring::Result<String> {
        let key = match entry(name)?.get_password() {
            Ok(key) => key,
            Err(keyring::Error::NoEntry) => String::new(),
            Err(e) => return Err(e),
        };
        remember(name, &key);
        Ok(key)
    }

    /// Stores `key`, or removes the stored one when `key` is empty.
    pub fn set(name: &'static str, key: &str) -> keyring::Result<()> {
        if known(name).as_deref() == Some(key) {
            return Ok(());
        }
        let entry = entry(name)?;
        if key.is_empty() {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(e),
            }
        } else {
            entry.set_password(key)?;
        }
        remember(name, key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uses the real credential store, which CI runners may not have.
    #[test]
    #[ignore]
    fn api_keys_live_in_the_credential_store() {
        let dir = std::env::temp_dir().join(format!("lipwise-keys-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut settings = Settings::default();
        settings.providers.custom.api_key = "test-key-123".into();
        save(&path, &settings).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("test-key-123"), "key written to the file");
        assert_eq!(load(&path).providers.custom.api_key, "test-key-123");

        // A key left in the file by an older version moves to the store on load.
        std::fs::write(&path, r#"{"providers":{"openai":{"api_key":"old-key-456"}}}"#).unwrap();
        assert_eq!(load(&path).providers.openai.api_key, "old-key-456");
        assert!(!std::fs::read_to_string(&path).unwrap().contains("old-key-456"), "key left in the file");
        assert_eq!(keys::get("openai").unwrap(), "old-key-456");

        settings.providers.custom.api_key.clear();
        save(&path, &settings).unwrap();
        assert_eq!(keys::get("custom").unwrap(), "");
        assert_eq!(keys::get("openai").unwrap(), "");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
