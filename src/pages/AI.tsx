import { useState, type ComponentType } from "react";
import { useApp } from "../App";
import { api, isMac, isWindows, type Provider, type ProviderConfig, type RemoteProvider } from "../api";
import LocalSetup from "./LocalSetup";
import { Group, Notice, PaneHeader, Row, Spinner, Tag, Toggle, type Tint } from "../components";
import { Asterisk, Check, Cloud, Cube, Desktop, Server, Sparkles } from "../icons";

const keyStore = isMac
  ? "Kept in your Keychain."
  : isWindows
    ? "Kept in Windows Credential Manager."
    : "Kept in your system keyring.";

const PROVIDERS: {
  id: Provider;
  label: string;
  hint: string;
  blurb: string;
  icon: ComponentType<{ size?: number | string }>;
  tint: Tint;
  needsKey: boolean;
  showUrl: boolean;
  keyUrl?: string;
}[] = [
  {
    id: "local",
    label: "On This Computer",
    hint: "Private and free. No account needed.",
    blurb:
      "Lipwise picks a model that suits your hardware, installs llama.cpp, and runs it for you. Nothing leaves your computer, and no internet is needed after setup.",
    icon: Desktop,
    tint: "green",
    needsKey: false,
    showUrl: false,
  },
  {
    id: "anthropic",
    label: "Claude",
    hint: "Best at following spoken edits precisely",
    blurb: "Anthropic's Claude. Only the transcript is sent, never your audio.",
    icon: Asterisk,
    tint: "orange",
    needsKey: true,
    showUrl: false,
    keyUrl: "https://console.anthropic.com/settings/keys",
  },
  {
    id: "openai",
    label: "OpenAI",
    hint: "Or any OpenAI-compatible hosted API",
    blurb: "OpenAI's API, or any hosted OpenAI-compatible service (Groq, OpenRouter, …) by changing the URL.",
    icon: Cloud,
    tint: "teal",
    needsKey: true,
    showUrl: true,
  },
  {
    id: "ollama",
    label: "Ollama",
    hint: "Run open models offline",
    blurb: "Runs a model on your machine through Ollama. Install Ollama, then run “ollama pull llama3.2”.",
    icon: Cube,
    tint: "gray",
    needsKey: false,
    showUrl: true,
  },
  {
    id: "custom",
    label: "Other Local Server",
    hint: "LM Studio, llama-server, vLLM…",
    blurb: "Any OpenAI-compatible server running on your machine or network.",
    icon: Server,
    tint: "indigo",
    needsKey: false,
    showUrl: true,
  },
];

const CLAUDE_MODELS = ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-4-5"];

const SAMPLE =
  "um so I wanted to let you know the meeting is on Tuesday no wait Thursday at 10 new paragraph can you bring the slides question mark make it sound professional";

export default function AI() {
  const { settings, update, status, toast } = useApp();
  const [models, setModels] = useState<string[]>([]);
  const [fetching, setFetching] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; text: string } | null>(null);

  const provider = PROVIDERS.find((p) => p.id === settings.provider)!;
  const isLocal = settings.provider === "local";
  // The local provider has no editable endpoint; fall back to a blank config for the form helpers.
  const cfg: ProviderConfig = isLocal
    ? { base_url: "", model: "", api_key: "" }
    : settings.providers[settings.provider as RemoteProvider];
  const envKey =
    (settings.provider === "anthropic" && status?.env_keys.anthropic) ||
    (settings.provider === "openai" && status?.env_keys.openai);

  const setCfg = (patch: Partial<ProviderConfig>) =>
    update((s) => ({
      ...s,
      providers: { ...s.providers, [s.provider]: { ...s.providers[s.provider as RemoteProvider], ...patch } },
    }));

  const fetchModels = async () => {
    setFetching(true);
    try {
      const list = await api.listAiModels(settings.provider as RemoteProvider, cfg);
      setModels(list);
      if (list.length === 0) toast("The provider returned no models");
      else toast(`Found ${list.length} models`);
    } catch (e) {
      toast(String(e), true);
    } finally {
      setFetching(false);
    }
  };

  const test = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const out = await api.previewAi({ ...settings, ai_enabled: true }, "dictate", SAMPLE);
      setTestResult({ ok: true, text: out });
    } catch (e) {
      setTestResult({ ok: false, text: String(e) });
    } finally {
      setTesting(false);
    }
  };

  const suggestions = settings.provider === "anthropic" && models.length === 0 ? CLAUDE_MODELS : models;

  return (
    <>
      <PaneHeader icon={Sparkles} tint="ai" title="AI Editor">
        After transcription, the AI applies the commands you spoke, like “scratch that” or “make it formal”, and cleans up
        the text. It also powers command mode.
      </PaneHeader>

      <Group footer="If the AI can't be reached, Lipwise types the raw transcript, so you never lose words.">
        <Row title="AI Editing" hint={settings.ai_enabled ? undefined : "Off. Lipwise types exactly what it hears."}>
          <Toggle label="AI editing" checked={settings.ai_enabled} onChange={(v) => update({ ai_enabled: v })} />
        </Row>
      </Group>

      <div className={`stack-section ${settings.ai_enabled ? "" : "dimmed"}`}>
        <Group title="Provider">
          {PROVIDERS.map((p) => (
            <Row
              key={p.id}
              icon={p.icon}
              tint={p.tint}
              title={p.label}
              hint={p.hint}
              className={settings.provider === p.id ? "row-selected" : ""}
              onClick={() => {
                if (settings.provider === p.id) return;
                setModels([]);
                setTestResult(null);
                update({ provider: p.id });
              }}
            >
              {settings.provider === p.id ? <Check className="row-check" /> : null}
            </Row>
          ))}
        </Group>

        {isLocal ? (
          <LocalSetup />
        ) : (
          <Group
            title={provider.label}
            footer={
              <>
                {provider.blurb}
                {provider.id === "anthropic" && " You can also set ANTHROPIC_API_KEY instead of pasting a key."}
                {provider.id === "openai" && " You can also set OPENAI_API_KEY instead of pasting a key."}
              </>
            }
          >
            {provider.showUrl && (
              <Row title="Server URL">
                <input
                  type="text"
                  className="field field-wide"
                  value={cfg.base_url}
                  spellCheck={false}
                  onChange={(e) => setCfg({ base_url: e.target.value })}
                />
              </Row>
            )}
            <Row
              title={provider.needsKey ? "API Key" : "API Key (optional)"}
              hint={
                <>
                  {keyStore}
                  {provider.keyUrl && (
                    <>
                      {" "}
                      <a href={provider.keyUrl} target="_blank" rel="noreferrer">
                        Get a key
                      </a>
                    </>
                  )}
                </>
              }
            >
              <input
                type="password"
                className="field field-wide"
                value={cfg.api_key}
                placeholder={envKey ? "Using environment key" : provider.needsKey ? "Required" : "None"}
                onChange={(e) => setCfg({ api_key: e.target.value })}
              />
            </Row>
            <Row
              title="Model"
              hint={settings.provider === "anthropic" ? "Opus is the most capable. Sonnet and Haiku are faster." : undefined}
            >
              <div className="field-combo">
                <input
                  type="text"
                  className="field field-wide"
                  list="ai-models"
                  value={cfg.model}
                  placeholder="Model name"
                  spellCheck={false}
                  onChange={(e) => setCfg({ model: e.target.value })}
                />
                <datalist id="ai-models">
                  {suggestions.map((m) => (
                    <option key={m} value={m} />
                  ))}
                </datalist>
                <button className="btn btn-sm" onClick={fetchModels} disabled={fetching}>
                  {fetching ? <Spinner size={13} /> : "List"}
                </button>
              </div>
            </Row>
          </Group>
        )}

        <Group title="Try It">
          <div className="cell try">
            <div className="try-head">
              <div className="try-status">
                {!status?.ai_ready ? (
                  <Tag tone="orange">Not configured</Tag>
                ) : settings.provider === "local" && status.local_ai.state !== "running" ? (
                  <Tag tone="orange">{status.local_ai.state === "starting" ? "Loading model" : "Model stopped"}</Tag>
                ) : (
                  <Tag tone="green">Connected</Tag>
                )}
              </div>
              <button className="btn btn-primary" onClick={test} disabled={testing}>
                {testing && <Spinner size={13} />}
                {testing ? "Testing…" : "Test with a Sample"}
              </button>
            </div>
            {testResult && (
              <div className="exchange">
                <div className="bubble bubble-said">{SAMPLE}</div>
                {testResult.ok ? (
                  <div className="bubble bubble-typed">{testResult.text}</div>
                ) : (
                  <Notice tone="error">{testResult.text}</Notice>
                )}
              </div>
            )}
          </div>
        </Group>
      </div>
    </>
  );
}
