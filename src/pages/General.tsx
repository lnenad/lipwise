import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useApp } from "../App";
import { api, type Theme } from "../api";
import { Group, Notice, PaneHeader, Row, Select, ShortcutInput, Spinner, Toggle } from "../components";
import { Gear, Text, Wand, Waveform } from "../icons";

const THEMES: { id: Theme; label: string }[] = [
  { id: "system", label: "System" },
  { id: "light", label: "Light" },
  { id: "dark", label: "Dark" },
];

export default function General() {
  const { settings, update, status, refreshStatus, toast } = useApp();
  const [mics, setMics] = useState<string[]>([]);
  const [version, setVersion] = useState("");
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    api.listMicrophones().then(setMics);
    getVersion().then(setVersion);
  }, []);

  const checkNow = () => {
    setChecking(true);
    api
      .checkForUpdate()
      .then((found) => {
        if (!found) toast("Lipwise is up to date");
        refreshStatus();
      })
      .catch((e) => toast(String(e), true))
      .finally(() => setChecking(false));
  };

  const restart = () => {
    setInstalling(true);
    api.installUpdate().catch((e) => {
      toast(String(e), true);
      setInstalling(false);
    });
  };

  return (
    <>
      <PaneHeader icon={Gear} tint="gray" title="Settings">
        Shortcuts, microphone, and how Lipwise behaves while you dictate.
      </PaneHeader>

      {status?.shortcut_error && <Notice tone="error">{status.shortcut_error}</Notice>}

      <Group title="Keyboard Shortcuts" footer="Click a shortcut, then press the new key combination. Press Esc while recording to cancel.">
        <Row icon={Waveform} tint="blue" title="Dictate" hint="Speak text, with commands along the way">
          <ShortcutInput value={settings.dictate_shortcut} onChange={(v) => update({ dictate_shortcut: v })} />
        </Row>
        <Row icon={Wand} tint="purple" title="Command" hint="Speak an instruction for the selected text">
          <ShortcutInput value={settings.command_shortcut} onChange={(v) => update({ command_shortcut: v })} />
        </Row>
        <Row icon={Text} tint="gray" title="Plain Dictation" hint="Types exactly what you said, with no AI">
          <ShortcutInput value={settings.plain_shortcut} onChange={(v) => update({ plain_shortcut: v })} />
        </Row>
      </Group>

      <Group
        footer={
          settings.push_to_talk
            ? "Hold the shortcut while talking. A quick tap switches to hands-free until you press again."
            : "Press once to start, and again to finish."
        }
      >
        <Row title="Push to Talk">
          <Toggle label="Push to talk" checked={settings.push_to_talk} onChange={(v) => update({ push_to_talk: v })} />
        </Row>
      </Group>

      <Group title="Audio">
        <Row title="Microphone">
          <Select
            value={settings.microphone ?? ""}
            onChange={(e) => update({ microphone: e.target.value || null })}
            className="popup-wide"
          >
            <option value="">System Default</option>
            {mics.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </Select>
        </Row>
      </Group>

      <Group title="Appearance">
        <Row title="Theme" hint="System follows your computer's light or dark setting">
          <div className="segmented" role="radiogroup" aria-label="Theme">
            {THEMES.map((t) => (
              <button
                key={t.id}
                role="radio"
                aria-checked={settings.theme === t.id}
                className={settings.theme === t.id ? "active" : ""}
                onClick={() => update({ theme: t.id })}
              >
                {t.label}
              </button>
            ))}
          </div>
        </Row>
      </Group>

      <Group title="Startup">
        <Row title="Open at Login" hint="Start Lipwise when you log in to this computer">
          <Toggle label="Open at login" checked={settings.launch_at_login} onChange={(v) => update({ launch_at_login: v })} />
        </Row>
        {settings.launch_at_login && (
          <Row title="Start in Tray" hint="At login, wait in the tray instead of opening this window">
            <Toggle label="Start in tray" checked={settings.start_hidden} onChange={(v) => update({ start_hidden: v })} />
          </Row>
        )}
      </Group>

      <Group title="Updates">
        <Row title="Update Automatically" hint="Download new versions in the background and install them when Lipwise restarts">
          <Toggle label="Update automatically" checked={settings.auto_update} onChange={(v) => update({ auto_update: v })} />
        </Row>
        {status?.update ? (
          <Row title="Update Ready" hint={`Lipwise ${status.update.version} is downloaded (you have ${version})`}>
            <button className="btn btn-sm btn-primary" onClick={restart} disabled={installing}>
              {installing ? <Spinner size={13} /> : "Restart to Update"}
            </button>
          </Row>
        ) : (
          <Row title="Version" hint={version}>
            <button className="btn btn-sm" onClick={checkNow} disabled={checking}>
              {checking ? <Spinner size={13} /> : "Check Now"}
            </button>
          </Row>
        )}
      </Group>

      <Group title="Behavior" footer="Lipwise keeps running in the background when you close this window.">
        <Row title="Show Status Overlay" hint="A small pill at the bottom of the screen while you dictate">
          <Toggle label="Show status overlay" checked={settings.show_overlay} onChange={(v) => update({ show_overlay: v })} />
        </Row>
        <Row title="Restore Clipboard" hint="Lipwise pastes through the clipboard, then puts back what was there">
          <Toggle label="Restore clipboard" checked={settings.restore_clipboard} onChange={(v) => update({ restore_clipboard: v })} />
        </Row>
        <Row title="Keep History" hint="How many past dictations to remember">
          <Select value={settings.history_limit} onChange={(e) => update({ history_limit: Number(e.target.value) })}>
            {[50, 200, 1000].map((n) => (
              <option key={n} value={n}>
                {n} items
              </option>
            ))}
          </Select>
        </Row>
      </Group>
    </>
  );
}
