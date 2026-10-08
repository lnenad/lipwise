import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import type { Status } from "./api";
import { Check, Sparkles } from "./icons";
import "./overlay.css";

const BARS = 11;

function Overlay() {
  const [status, setStatus] = useState<Status>({ phase: "idle", mode: "dictate", message: "" });
  const [levels, setLevels] = useState<number[]>(Array(BARS).fill(0));
  const history = useRef<number[]>(Array(BARS).fill(0));
  // Bumped when a new recording starts, so the capsule replays its entrance.
  const [session, setSession] = useState(0);
  const lastPhase = useRef(status.phase);

  useEffect(() => {
    const unlisten = [
      listen<Status>("status", (e) => {
        if (e.payload.phase === "recording" && lastPhase.current !== "recording") setSession((n) => n + 1);
        lastPhase.current = e.payload.phase;
        setStatus(e.payload);
      }),
      listen<number>("mic-level", (e) => {
        // Speech RMS is small; scale and compress it so the bars move visibly.
        const v = Math.min(1, Math.sqrt(e.payload * 12));
        history.current = [...history.current.slice(1), v];
        setLevels(history.current);
      }),
    ];
    return () => unlisten.forEach((p) => p.then((f) => f()));
  }, []);

  const { phase, mode, message } = status;
  const label = mode === "command" ? "Command" : mode === "plain" ? "Plain Dictation" : "Dictation";

  return (
    <div className={`island ${phase} mode-${mode}`} key={session}>
      <div className="lead" key={`lead-${phase}`}>
        {phase === "recording" && <span className="rec" />}
        {phase === "transcribing" && <Spinner />}
        {phase === "thinking" && <Sparkles className="sparkle" />}
        {phase === "done" && (
          <span className="glyph ok">
            <Check />
          </span>
        )}
        {phase === "error" && <span className="glyph err">!</span>}
      </div>
      <div className="text" key={`text-${phase}`}>
        <div className="label">{label}</div>
        <div className="message">{message}</div>
      </div>
      {phase === "recording" && (
        <div className="wave" aria-hidden>
          {levels.map((l, i) => {
            // Taper the edges so the waveform reads as a single shape.
            const taper = 1 - Math.abs(i - (BARS - 1) / 2) / BARS;
            return <i key={i} style={{ height: `${Math.max(14, l * 100 * (0.55 + taper * 0.6))}%` }} />;
          })}
        </div>
      )}
    </div>
  );
}

function Spinner() {
  return (
    <span className="spinner">
      {Array.from({ length: 8 }, (_, i) => (
        <i key={i} style={{ transform: `rotate(${i * 45}deg)`, animationDelay: `${(i - 8) * 0.1}s` }} />
      ))}
    </span>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
