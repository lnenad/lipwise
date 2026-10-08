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

pub fn load(path: &PathBuf) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
            log::warn!("settings file unreadable ({e}); using defaults");
            Settings::default()
        }),
        Err(_) => Settings::default(),
    }
}

pub fn save(path: &PathBuf, settings: &Settings) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
