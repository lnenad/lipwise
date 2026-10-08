import { useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { useApp } from "../App";
import { api, formatBytes, type Hardware, type SetupProgress, type TrialRun } from "../api";
import { Group, Notice, Progress, Row, Spinner, Tag, Toggle } from "../components";
import { Check } from "../icons";

type StepNo = 1 | 2 | 3 | 4;

function Step({
  n,
  current,
  title,
  summary,
  onEdit,
  children,
}: {
  n: StepNo;
  current: StepNo | 5;
  title: string;
  /** One line shown once the step is complete. */
  summary?: ReactNode;
  onEdit?: () => void;
  children: ReactNode;
}) {
  const state = n < current ? "done" : n === current ? "active" : "pending";
  return (
    <div className={`step step-${state}`}>
      <div className="step-marker">{state === "done" ? <Check /> : n}</div>
      <div className="step-body">
        <div className="step-title">
          <h3>{title}</h3>
          {state === "done" && onEdit && (
            <button className="link" onClick={onEdit}>
              Change
            </button>
          )}
        </div>
        {state === "done" && summary && <div className="step-summary">{summary}</div>}
        {state === "active" && <div className="step-content">{children}</div>}
      </div>
    </div>
  );
}

/** "On this computer": choose → download → run → use, each as its own step. */
export default function LocalSetup() {
  const { settings, update, status, toast } = useApp();
  const [hw, setHw] = useState<Hardware | null>(null);
  const [wizardOpen, setWizardOpen] = useState(false);
  const [step, setStep] = useState<StepNo | 5>(1);
  const [choice, setChoice] = useState<string | null>(null);
  const [progress, setProgress] = useState<SetupProgress | null>(null);
  const [starting, setStarting] = useState(false);
  const [trial, setTrial] = useState<TrialRun | null>(null);
  const [startError, setStartError] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);

  const active = settings.provider === "local" && settings.local_ai.model_path !== "";
  const showWizard = wizardOpen || !active;

  // If the wizard is left with a model started but not adopted, free its memory.
  const startedUnapplied = useRef(false);
  const providerRef = useRef(settings.provider);
  providerRef.current = settings.provider;
  useEffect(
    () => () => {
      if (startedUnapplied.current && providerRef.current !== "local") api.localAiStop();
    },
    [],
  );

  const refreshHw = () =>
    api.localAiDetect().then((h) => {
      setHw(h);
      setChoice((c) => c ?? (settings.local_ai.model_id || h.recommended));
      return h;
    });

  useEffect(() => {
    refreshHw();
    const un = listen<SetupProgress>("local-ai-setup", (e) => {
      const p = e.payload;
      if (p.step === "done") {
        setProgress(null);
        refreshHw();
        setStep(3);
      } else if (p.step === "cancelled") {
        setProgress(null);
      } else if (p.step === "error") {
        setProgress(null);
        toast(p.message, true);
      } else {
        setProgress(p);
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  if (!hw)
    return (
      <Group title="On This Computer">
        <div className="cell loading-cell">
          <Spinner />
          Checking your computer…
        </div>
      </Group>
    );

  const server = status?.local_ai;
  const model = hw.models.find((m) => m.id === choice) ?? null;
  const llamaReady = hw.existing_server !== null;
  const needsDownload = !model?.downloaded || !llamaReady;
  const runningThis = server?.state === "running" && server.model_id === choice;

  const goTo = (n: StepNo) => {
    setStep(n);
    if (n <= 2) {
      setTrial(null);
      setStartError(null);
    }
  };

  const download = () => {
    if (!model) return;
    setProgress({ step: llamaReady ? "model" : "llama", message: "Starting download…", downloaded: 0, total: 0 });
    api.localAiDownload(model.id).catch(() => {});
  };

  const start = async () => {
    if (!model) return;
    setStarting(true);
    setStartError(null);
    try {
      const run = await api.localAiStart(model.id);
      setTrial(run);
      startedUnapplied.current = true;
    } catch (e) {
      setStartError(String(e));
    } finally {
      setStarting(false);
    }
  };

  const apply = async () => {
    if (!model) return;
    setApplying(true);
    try {
      await api.localAiApply(model.id);
      startedUnapplied.current = false;
      setStep(5);
      setWizardOpen(false);
      refreshHw();
      toast(`Lipwise now uses ${model.name}`);
    } catch (e) {
      toast(String(e), true);
    } finally {
      setApplying(false);
    }
  };

  const remove = async () => {
    try {
      await api.localAiRemove();
      setWizardOpen(false);
      setStep(1);
      setTrial(null);
      await refreshHw();
      toast("Local AI removed");
    } catch (e) {
      toast(String(e), true);
    }
  };

  // ---- in use: compact status
  if (!showWizard) {
    const current = hw.models.find((m) => m.id === settings.local_ai.model_id);
    const tone = server?.state === "running" ? "ok" : server?.state === "starting" ? "busy" : server?.state === "error" ? "error" : "idle";
    const run = () => api.localAiRun().catch((e) => toast(String(e), true));
    return (
      <Group title="On This Computer" footer={`${hw.summary}. The model stops when you quit Lipwise.`}>
        <Row
          title={current?.name ?? "Local model"}
          hint={
            server?.state === "running"
              ? "Running"
              : server?.state === "starting"
                ? "Loading the model…"
                : server?.state === "error"
                  ? "Couldn't start"
                  : "Stopped. Dictation types without AI edits until you start it."
          }
        >
          <span className={`status-light status-${tone}`} />
          {server?.state === "running" ? (
            <button className="btn btn-sm" onClick={() => api.localAiStop()}>
              Stop
            </button>
          ) : server?.state === "starting" ? (
            <button className="btn btn-sm" disabled>
              <Spinner size={12} />
              Starting
            </button>
          ) : (
            <button className="btn btn-sm btn-primary" onClick={run}>
              Start
            </button>
          )}
        </Row>
        <Row title="Start with Lipwise" hint="Load the model in the background when Lipwise opens">
          <Toggle
            label="Start the local model with Lipwise"
            checked={settings.local_ai_autostart}
            onChange={(v) => update({ local_ai_autostart: v })}
          />
        </Row>
        {server?.state === "error" && (
          <div className="cell">
            <Notice tone="error">{server.message}</Notice>
          </div>
        )}
        <div className="cell button-row">
          <button
            className="btn"
            onClick={() => {
              setChoice(settings.local_ai.model_id);
              setStep(1);
              setWizardOpen(true);
            }}
          >
            Change Model…
          </button>
          <button className="btn btn-plain btn-destructive" onClick={remove}>
            Remove Local AI
          </button>
        </div>
      </Group>
    );
  }

  const pct = progress && progress.total > 0 ? (progress.downloaded / progress.total) * 100 : null;

  return (
    <Group title="Set Up Local AI">
      <div className="cell steps">
        <Step
          n={1}
          current={step}
          title="Choose a model"
          summary={model && `${model.name} · ${formatBytes(model.size_bytes)}`}
          onEdit={progress || starting ? undefined : () => goTo(1)}
        >
          <p className="secondary">
            Your computer: {hw.summary}
            {hw.free_disk_gb !== null && ` · ${hw.free_disk_gb.toFixed(1)} GB free`}
          </p>
          <div className="choices" role="radiogroup">
            {hw.models.map((m) => (
              <label key={m.id} className={`choice ${choice === m.id ? "selected" : ""} ${m.fits_disk ? "" : "disabled"}`}>
                <input
                  type="radio"
                  name="local-model"
                  checked={choice === m.id}
                  disabled={!m.fits_disk}
                  onChange={() => setChoice(m.id)}
                />
                <span className="radio" />
                <span className="choice-body">
                  <span className="choice-title">
                    <b>{m.name}</b>
                    {m.id === hw.recommended && <Tag tone="blue">Best for this computer</Tag>}
                    {m.downloaded && <Tag tone="green">Downloaded</Tag>}
                    {!m.fits_disk ? <Tag tone="orange">Not enough space</Tag> : !m.fits && <Tag tone="orange">May be slow</Tag>}
                  </span>
                  <span className="choice-hint">{m.blurb}</span>
                </span>
                <span className="choice-size">{formatBytes(m.size_bytes)}</span>
              </label>
            ))}
          </div>
          <div className="button-row">
            <button className="btn btn-primary" disabled={!model?.fits_disk} onClick={() => goTo(2)}>
              Continue
            </button>
            {active && (
              <button className="btn btn-plain" onClick={() => setWizardOpen(false)}>
                Keep Current Model
              </button>
            )}
          </div>
        </Step>

        <Step
          n={2}
          current={step}
          title="Download"
          summary="llama.cpp and the model are on this computer"
          onEdit={starting ? undefined : () => goTo(2)}
        >
          <div className="checklist">
            <div className={`check-item ${llamaReady ? "done" : ""}`}>
              <span className="check-mark">{llamaReady && <Check />}</span>
              <span>
                <b>llama.cpp</b>
                <span className="secondary">
                  {llamaReady ? "Installed" : "The engine that runs the model. A small download picked for your hardware."}
                </span>
              </span>
            </div>
            <div className={`check-item ${model?.downloaded ? "done" : ""}`}>
              <span className="check-mark">{model?.downloaded && <Check />}</span>
              <span>
                <b>{model?.name}</b>
                <span className="secondary">
                  {model?.downloaded ? "Downloaded" : `${formatBytes(model?.size_bytes ?? 0)}, verified after download`}
                </span>
              </span>
            </div>
          </div>
          {progress ? (
            <div className="download">
              <Progress value={pct} />
              <div className="download-info">
                <span>
                  {progress.message}
                  {progress.total > 0 && ` · ${formatBytes(progress.downloaded)} of ${formatBytes(progress.total)}`}
                </span>
                <button className="btn btn-sm" onClick={() => api.localAiCancel()}>
                  Cancel
                </button>
              </div>
            </div>
          ) : needsDownload ? (
            <div className="button-row">
              <button className="btn btn-primary" onClick={download}>
                Download
              </button>
            </div>
          ) : (
            <div className="button-row">
              <button className="btn btn-primary" onClick={() => goTo(3)}>
                Continue
              </button>
              <span className="secondary">Everything is already on this computer.</span>
            </div>
          )}
        </Step>

        <Step
          n={3}
          current={step}
          title="Run the model"
          summary={trial && `Answered a test dictation in ${(trial.millis / 1000).toFixed(1)} s`}
          onEdit={applying ? undefined : () => goTo(3)}
        >
          <p className="secondary">
            Lipwise starts the model in the background on {hw.summary.replace(/ via .*/, "")} and sends it a test dictation.
          </p>
          {trial ? (
            <>
              <div className="exchange">
                <div className="bubble bubble-said">{trial.said}</div>
                <div className="bubble bubble-typed">{trial.typed || <span className="secondary">Nothing</span>}</div>
              </div>
              <div className="button-row">
                <button className="btn btn-primary" onClick={() => setStep(4)}>
                  Continue
                </button>
                <button className="btn" onClick={start} disabled={starting}>
                  Test Again
                </button>
                <span className="secondary">Answered in {(trial.millis / 1000).toFixed(1)} s</span>
              </div>
            </>
          ) : (
            <>
              {startError && <Notice tone="error">{startError}</Notice>}
              <div className="button-row">
                <button className="btn btn-primary" onClick={start} disabled={starting}>
                  {starting && <Spinner size={13} />}
                  {starting ? "Loading the Model…" : startError ? "Try Again" : runningThis ? "Test It" : "Start the Model"}
                </button>
                {starting && <span className="secondary">This takes a few seconds the first time.</span>}
              </div>
            </>
          )}
        </Step>

        <Step n={4} current={step} title="Use it in Lipwise">
          <p>
            Lipwise will use <b>{model?.name}</b> for dictation and voice commands. It starts with Lipwise and stops when
            you quit.
            {settings.provider !== "local" && " Your other AI settings are kept, so you can switch back anytime."}
          </p>
          <div className="button-row">
            <button className="btn btn-primary" onClick={apply} disabled={applying}>
              {applying && <Spinner size={13} />}
              {applying ? "Applying…" : "Use This Model"}
            </button>
          </div>
        </Step>
      </div>
      {settings.local_ai.model_path !== "" && (
        <button className="row row-button row-destructive" onClick={remove}>
          Remove Local AI
        </button>
      )}
    </Group>
  );
}
