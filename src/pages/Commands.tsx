import { useState } from "react";
import { useApp } from "../App";
import { api, type Mode } from "../api";
import { Group, Notice, PaneHeader, Row, Spinner } from "../components";
import { Bubble, ChevronRight, Minus, Plus, XMark } from "../icons";

const BUILTIN: [string, string][] = [
  ["“scratch that”, “delete that”", "Removes what you just said"],
  ["“no wait, …”, “I mean …”", "Keeps only your correction"],
  ["“new line”, “new paragraph”", "Line and paragraph breaks"],
  ["“bullet point”, “numbered list”", "Lists"],
  ["“comma”, “period”, “question mark”", "Spoken punctuation"],
  ["“replace X with Y”", "Fixes a word or phrase"],
  ["“make it formal / shorter / friendlier”", "Rewrites the text so far"],
  ["“translate this to Spanish”", "Translates the text"],
];

export default function Commands() {
  const { settings, update, status, go } = useApp();
  const [newTerm, setNewTerm] = useState("");
  const [mode, setMode] = useState<Mode>("dictate");
  const [said, setSaid] = useState("");
  const [selection, setSelection] = useState("");
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const [running, setRunning] = useState(false);

  const setCommand = (i: number, patch: Partial<{ phrase: string; action: string }>) =>
    update((s) => ({
      ...s,
      voice_commands: s.voice_commands.map((c, j) => (j === i ? { ...c, ...patch } : c)),
    }));

  const addTerm = () => {
    const terms = newTerm
      .split(",")
      .map((t) => t.trim())
      .filter((t) => t && !settings.vocabulary.includes(t));
    if (terms.length) update({ vocabulary: [...settings.vocabulary, ...terms] });
    setNewTerm("");
  };

  const tryIt = async () => {
    setRunning(true);
    setResult(null);
    try {
      setResult({ ok: true, text: await api.previewAi({ ...settings, ai_enabled: true }, mode, said, selection) });
    } catch (e) {
      setResult({ ok: false, text: String(e) });
    } finally {
      setRunning(false);
    }
  };

  return (
    <>
      <PaneHeader icon={Bubble} tint="orange" title="Voice Commands">
        Speak commands naturally while you dictate. The AI editor tells your commands apart from your words.
      </PaneHeader>

      {!status?.ai_ready && (
        <Notice tone="warn">
          <span>
            Voice commands need the AI editor.{" "}
            <button className="link" onClick={() => go("ai")}>
              Set it up
            </button>
          </span>
        </Notice>
      )}

      <Group title="Built In">
        {BUILTIN.map(([say, does]) => (
          <Row key={say} title={say}>
            <span className="detail">{does}</span>
          </Row>
        ))}
      </Group>

      <Group title="Your Commands" footer="Teach Lipwise your own phrases, and describe in plain words what should happen.">
        {settings.voice_commands.map((c, i) => (
          <div className="cell command" key={i}>
            <button
              className="delete-btn"
              aria-label="Remove command"
              onClick={() => update((s) => ({ ...s, voice_commands: s.voice_commands.filter((_, j) => j !== i) }))}
            >
              <Minus />
            </button>
            <input
              type="text"
              className="bare"
              placeholder="When I say…"
              value={c.phrase}
              onChange={(e) => setCommand(i, { phrase: e.target.value })}
            />
            <ChevronRight className="command-arrow" />
            <input
              type="text"
              className="bare"
              placeholder="…do this"
              value={c.action}
              onChange={(e) => setCommand(i, { action: e.target.value })}
            />
          </div>
        ))}
        <button
          className="row row-button row-add"
          onClick={() => update((s) => ({ ...s, voice_commands: [...s.voice_commands, { phrase: "", action: "" }] }))}
        >
          <span className="add-icon">
            <Plus />
          </span>
          Add Command
        </button>
      </Group>

      <Group title="Vocabulary" footer="Names, jargon, and product names to spell correctly. Used by both the speech model and the AI.">
        <div className="cell vocab">
          {settings.vocabulary.map((t) => (
            <span className="token" key={t}>
              {t}
              <button aria-label={`Remove ${t}`} onClick={() => update({ vocabulary: settings.vocabulary.filter((v) => v !== t) })}>
                <XMark />
              </button>
            </span>
          ))}
          <input
            type="text"
            className="bare vocab-input"
            placeholder={settings.vocabulary.length ? "Add more…" : "Add words, separated by commas"}
            value={newTerm}
            onChange={(e) => setNewTerm(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") addTerm();
              if (e.key === "Backspace" && !newTerm && settings.vocabulary.length)
                update({ vocabulary: settings.vocabulary.slice(0, -1) });
            }}
            onBlur={addTerm}
          />
        </div>
      </Group>

      <Group
        footer={
          <>
            If you dictate a lot of text that sounds like commands, set a wake word. Then only “
            {settings.command_word || "computer"}, new paragraph” counts as a command.
          </>
        }
      >
        <Row title="Wake Word">
          <input
            type="text"
            className="field"
            placeholder="None"
            value={settings.command_word}
            onChange={(e) => update({ command_word: e.target.value })}
          />
        </Row>
      </Group>

      <Group title="Writing Style" footer="Extra instructions the AI follows every time.">
        <textarea
          className="bare cell-textarea"
          placeholder="e.g. Use British spelling. Keep my casual tone. Never use em dashes."
          value={settings.custom_instructions}
          onChange={(e) => update({ custom_instructions: e.target.value })}
        />
      </Group>

      <Group
        title="Playground"
        accessory={
          <div className="segmented segmented-sm" role="tablist">
            <button role="tab" aria-selected={mode === "dictate"} className={mode === "dictate" ? "active" : ""} onClick={() => setMode("dictate")}>
              Dictation
            </button>
            <button role="tab" aria-selected={mode === "command"} className={mode === "command" ? "active" : ""} onClick={() => setMode("command")}>
              Command
            </button>
          </div>
        }
        footer="Type what you'd say to see what Lipwise would type, without using the microphone."
      >
        {mode === "command" && (
          <textarea
            className="bare cell-textarea"
            placeholder="Selected text (optional)"
            value={selection}
            onChange={(e) => setSelection(e.target.value)}
          />
        )}
        <textarea
          className="bare cell-textarea"
          placeholder={
            mode === "dictate"
              ? "remind me to call Anna on Monday scratch that on Tuesday new paragraph and buy milk"
              : "make this shorter and friendlier"
          }
          value={said}
          onChange={(e) => setSaid(e.target.value)}
        />
        <div className="cell try">
          <div className="try-head">
            <span className="secondary">{mode === "dictate" ? "What you'd say while dictating" : "What you'd ask for"}</span>
            <button className="btn btn-primary" onClick={tryIt} disabled={running || !said.trim()}>
              {running && <Spinner size={13} />}
              {running ? "Thinking…" : "Try It"}
            </button>
          </div>
          {result &&
            (result.ok ? (
              <div className="bubble bubble-typed">{result.text || <span className="secondary">Nothing would be typed</span>}</div>
            ) : (
              <Notice tone="error">{result.text}</Notice>
            ))}
        </div>
      </Group>
    </>
  );
}
