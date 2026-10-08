import { invoke } from "@tauri-apps/api/core";

export type Provider = "local" | "anthropic" | "openai" | "ollama" | "custom";
export type RemoteProvider = Exclude<Provider, "local">;
export type Mode = "dictate" | "command" | "plain";
export type Phase = "idle" | "recording" | "transcribing" | "thinking" | "done" | "error";

export interface ProviderConfig {
  base_url: string;
  model: string;
  api_key: string;
}

export interface VoiceCommand {
  phrase: string;
  action: string;
}

export type Theme = "system" | "light" | "dark";

export interface Settings {
  dictate_shortcut: string;
  command_shortcut: string;
  plain_shortcut: string;
  push_to_talk: boolean;
  microphone: string | null;
  selected_model: string | null;
  language: string;
  ai_enabled: boolean;
  provider: Provider;
  providers: Record<RemoteProvider, ProviderConfig>;
  local_ai: { model_id: string; model_path: string; server_path: string };
  local_ai_autostart: boolean;
  command_word: string;
  custom_instructions: string;
  vocabulary: string[];
  voice_commands: VoiceCommand[];
  restore_clipboard: boolean;
  show_overlay: boolean;
  history_limit: number;
  theme: Theme;
  launch_at_login: boolean;
  start_hidden: boolean;
  auto_update: boolean;
}

export interface ModelInfo {
  id: string;
  name: string;
  description: string;
  architecture: string;
  languages: string[];
  size_bytes: number;
  speed: number;
  accuracy: number;
  recommended: boolean;
  rank: number;
  streaming: boolean;
  translate: boolean;
  downloaded: boolean;
}

export interface Status {
  phase: Phase;
  mode: Mode;
  message: string;
}

export interface HistoryEntry {
  id: number;
  timestamp: string;
  mode: Mode;
  transcript: string;
  output: string;
  selection: string;
  ai_used: boolean;
  note: string | null;
}

export interface AppStatus {
  status: Status;
  loaded_model: string | null;
  /** Mirrors `Settings::ai_ready` in Rust (which also sees API keys from env vars). */
  ai_ready: boolean;
  shortcut_error: string | null;
  local_ai: { state: "stopped" | "starting" | "running" | "error"; model_id: string; message: string };
  env_keys: { anthropic: boolean; openai: boolean };
  /** A downloaded update waiting for a restart. */
  update: UpdateInfo | null;
}

export interface UpdateInfo {
  version: string;
  notes: string;
}

export interface LocalModelOption {
  id: string;
  name: string;
  blurb: string;
  size_bytes: number;
  downloaded: boolean;
  fits: boolean;
  fits_disk: boolean;
}

export interface Hardware {
  ram_gb: number;
  cpu_cores: number;
  gpus: { name: string; vram_gb: number | null }[];
  backend: "metal" | "vulkan" | "cpu";
  summary: string;
  recommended: string;
  existing_server: string | null;
  free_disk_gb: number | null;
  models: LocalModelOption[];
}

export interface SetupProgress {
  step: "llama" | "model" | "done" | "error" | "cancelled";
  message: string;
  downloaded: number;
  total: number;
}

export interface TrialRun {
  said: string;
  typed: string;
  millis: number;
}

export interface DownloadProgress {
  id: string;
  stage: "downloading" | "verifying" | "done" | "error" | "cancelled";
  downloaded: number;
  total: number;
  error: string | null;
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  getStatus: () => invoke<AppStatus>("get_status"),
  checkForUpdate: () => invoke<UpdateInfo | null>("check_for_update"),
  installUpdate: () => invoke<void>("install_update"),
  listModels: () => invoke<ModelInfo[]>("list_models"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),
  cancelDownload: (id: string) => invoke<void>("cancel_download", { id }),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),
  listMicrophones: () => invoke<string[]>("list_microphones"),
  listAiModels: (provider: RemoteProvider, config: ProviderConfig) =>
    invoke<string[]>("list_ai_models", { provider, config }),
  previewAi: (settings: Settings, mode: Mode, text: string, selection = "") =>
    invoke<string>("preview_ai", { settings, mode, text, selection }),
  localAiDetect: () => invoke<Hardware>("local_ai_detect"),
  localAiDownload: (modelId: string) => invoke<void>("local_ai_download", { modelId }),
  localAiStart: (modelId: string) => invoke<TrialRun>("local_ai_start", { modelId }),
  localAiApply: (modelId: string) => invoke<void>("local_ai_apply", { modelId }),
  localAiRun: () => invoke<void>("local_ai_run"),
  localAiStop: () => invoke<void>("local_ai_stop"),
  localAiCancel: () => invoke<void>("local_ai_cancel"),
  localAiRemove: () => invoke<void>("local_ai_remove"),
  getHistory: () => invoke<HistoryEntry[]>("get_history"),
  deleteHistory: (ids: number[]) => invoke<void>("delete_history", { ids }),
  clearHistory: () => invoke<void>("clear_history"),
};

const isMac = navigator.userAgent.includes("Mac");

/** "ctrl+shift+Space" → ["Ctrl", "Shift", "Space"] */
export function shortcutKeys(shortcut: string): string[] {
  return shortcut.split("+").map((part) => {
    const p = part.trim();
    switch (p.toLowerCase()) {
      case "ctrl":
      case "control":
        return isMac ? "⌃" : "Ctrl";
      case "shift":
        return isMac ? "⇧" : "Shift";
      case "alt":
      case "option":
        return isMac ? "⌥" : "Alt";
      case "super":
      case "cmd":
      case "command":
      case "meta":
        return isMac ? "⌘" : "Win";
    }
    if (p.startsWith("Key")) return p.slice(3);
    if (p.startsWith("Digit")) return p.slice(5);
    return p;
  });
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  return `${Math.round(bytes / 1e6)} MB`;
}
