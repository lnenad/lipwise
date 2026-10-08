import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { readiness, useApp } from "../App";
import { api, type HistoryEntry, type ModelInfo } from "../api";
import { Group, Keys, Notice, Row } from "../components";
import { Mic, Sparkles, Text, Wand, Waveform } from "../icons";

const EXAMPLES: { said: string; typed: string; context?: string }[] = [
  {
    said: "Let's meet on Tuesday, no wait, Wednesday at three",
    typed: "Let's meet on Wednesday at three.",
  },
  {
    said: "Shopping list new line milk new line eggs make it a list",
    typed: "Shopping list:\n- Milk\n- Eggs",
  },
  {
    said: "hey can you send me the report scratch that please send the report by Friday make it formal",
    typed: "Could you please send the report by Friday?",
  },
  {
    context: "With text selected",
    said: "translate this to German",
    typed: "The selection, replaced with its German translation.",
  },
];

const PROVIDER_NAMES: Record<string, string> = {
  local: "On this computer",
  anthropic: "Claude",
  openai: "OpenAI",
  ollama: "Ollama",
  custom: "Local server",
};

export default function Home() {
  const { settings, status, go } = useApp();
  const [recent, setRecent] = useState<HistoryEntry[]>([]);
  const [models, setModels] = useState<ModelInfo[]>([]);

  useEffect(() => {
    const load = () => api.getHistory().then((h) => setRecent(h.slice(0, 3)));
    load();
    api.listModels().then(setModels);
    const un = listen("history-changed", load);
    return () => {
      un.then((f) => f());
    };
  }, []);

  const ready = readiness(settings, status);
  const hasModel = !!settings.selected_model;
  const aiReady = !!status?.ai_ready;
  const modelName = models.find((m) => m.id === settings.selected_model)?.name;

  const headline = !hasModel ? "Let's get you set up" : ready.tone === "ok" ? "Ready when you are" : ready.text;

  return (
    <>
      <section className={`hero hero-${ready.tone}`}>
        <div className="hero-icon">
          <span className="hero-ring" />
          <img src="/app-icon.svg" alt="" />
        </div>
        <h1>{headline}</h1>
        {hasModel ? (
          <p>
            {settings.push_to_talk ? "Hold" : "Press"} <Keys shortcut={settings.dictate_shortcut} /> in any app and start
            talking.
          </p>
        ) : (
          <>
            <p>Lipwise types what you say, wherever your cursor is. First, download a speech model. It runs privately on this computer.</p>
            <button className="btn btn-primary btn-lg" onClick={() => go("models")}>
              Choose a Speech Model
            </button>
          </>
        )}
      </section>

      {status?.shortcut_error && (
        <Notice tone="error">
          <span>
            {status.shortcut_error}{" "}
            <button className="link" onClick={() => go("general")}>
              Choose another shortcut
            </button>
          </span>
        </Notice>
      )}

      <Group
        title="Shortcuts"
        accessory={
          <button className="link" onClick={() => go("general")}>
            Edit
          </button>
        }
        footer={
          settings.push_to_talk
            ? "Hold a shortcut while you talk, or tap it once to go hands-free. Press Esc to cancel."
            : "Press a shortcut to start, and again to finish. Press Esc to cancel."
        }
      >
        <Row icon={Waveform} tint="blue" title="Dictate" hint="Speak text, with commands like “new paragraph” along the way">
          <Keys shortcut={settings.dictate_shortcut} />
        </Row>
        <Row icon={Wand} tint="purple" title="Command" hint="Select text, then say what to do: “shorten this”, “reply saying yes”">
          <Keys shortcut={settings.command_shortcut} />
        </Row>
        <Row icon={Text} tint="gray" title="Plain Dictation" hint="Exactly what you said, with no AI. Good for code and names.">
          <Keys shortcut={settings.plain_shortcut} />
        </Row>
      </Group>

      <Group title="Setup">
        <Row icon={Mic} tint="red" title="Speech Model" onClick={() => go("models")}>
          <span className={`detail ${hasModel ? "" : "detail-warn"}`}>{hasModel ? (modelName ?? "Installed") : "Not installed"}</span>
          <span className="chevron" />
        </Row>
        <Row icon={Sparkles} tint="ai" title="AI Editor" onClick={() => go("ai")}>
          <span className={`detail ${settings.ai_enabled && !aiReady ? "detail-warn" : ""}`}>
            {!settings.ai_enabled ? "Off" : aiReady ? PROVIDER_NAMES[settings.provider] : "Not set up"}
          </span>
          <span className="chevron" />
        </Row>
      </Group>

      <Group title="What the AI Editor Does">
        <div className="examples">
          {EXAMPLES.map((ex) => (
            <figure className="example" key={ex.said}>
              {ex.context && <span className="example-context">{ex.context}</span>}
              <blockquote>{ex.said}</blockquote>
              <figcaption>{ex.typed}</figcaption>
            </figure>
          ))}
        </div>
      </Group>

      {recent.length > 0 && (
        <Group
          title="Recent"
          accessory={
            <button className="link" onClick={() => go("history")}>
              Show All
            </button>
          }
        >
          {recent.map((e) => (
            <div className="cell recent" key={e.id}>
              <span className="recent-text">{e.output || <span className="secondary">Nothing typed</span>}</span>
              <time className="secondary">{new Date(e.timestamp).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}</time>
            </div>
          ))}
        </Group>
      )}
    </>
  );
}
