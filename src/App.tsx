import { createContext, useCallback, useContext, useEffect, useRef, useState, type ComponentType } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type AppStatus, type Settings } from "./api";
import { setTheme } from "./theme";
import { BubbleFill, ClockFill, GearFill, HouseFill, MicFill, SparklesFill } from "./icons";
import Home from "./pages/Home";
import Models from "./pages/Models";
import AI from "./pages/AI";
import Commands from "./pages/Commands";
import General from "./pages/General";
import History from "./pages/History";

export type Page = "home" | "models" | "ai" | "commands" | "general" | "history";

type Patch = Partial<Settings> | ((s: Settings) => Settings);

interface Ctx {
  settings: Settings;
  update: (patch: Patch) => void;
  status: AppStatus | null;
  refreshStatus: () => void;
  toast: (message: string, error?: boolean) => void;
  go: (page: Page) => void;
}

const AppContext = createContext<Ctx>(null!);
export const useApp = () => useContext(AppContext);

interface PageDef {
  id: Page;
  label: string;
  icon: ComponentType<{ size?: number | string }>;
}

/** Sidebar sections with small captions; settings sit at the bottom. */
const SECTIONS: { title?: string; pages: PageDef[] }[] = [
  { pages: [{ id: "home", label: "Overview", icon: HouseFill }] },
  {
    title: "Dictation",
    pages: [
      { id: "models", label: "Speech Models", icon: MicFill },
      { id: "ai", label: "AI Editor", icon: SparklesFill },
      { id: "commands", label: "Voice Commands", icon: BubbleFill },
    ],
  },
  { title: "Activity", pages: [{ id: "history", label: "History", icon: ClockFill }] },
];
const FOOTER: PageDef[] = [{ id: "general", label: "Settings", icon: GearFill }];
const PAGES = [...SECTIONS.flatMap((s) => s.pages), ...FOOTER];

/** Pages laid out as a list with a detail pane, rather than one scrolling column. */
const SPLIT_PAGES: Page[] = ["history"];

export type Readiness = { tone: "ok" | "busy" | "rec" | "warn" | "error"; text: string };

/** One-line summary of whether dictation works right now. Shared by the sidebar and Overview. */
export function readiness(settings: Settings, status: AppStatus | null): Readiness {
  const phase = status?.status.phase ?? "idle";
  if (!settings.selected_model) return { tone: "warn", text: "Needs a speech model" };
  if (status?.shortcut_error) return { tone: "error", text: "Shortcut unavailable" };
  const access = status?.permissions;
  if (access?.microphone === "missing") return { tone: "warn", text: "No microphone found" };
  if (access && access.microphone !== "granted") return { tone: "warn", text: "Allow microphone" };
  if (access?.typing === "denied") return { tone: "warn", text: "Allow typing" };
  if (phase === "recording") return { tone: "rec", text: "Listening…" };
  if (phase === "transcribing") return { tone: "busy", text: "Transcribing…" };
  if (phase === "thinking") return { tone: "busy", text: "Editing…" };
  if (phase === "error") return { tone: "error", text: "Something went wrong" };
  if (status?.loaded_model !== settings.selected_model) return { tone: "busy", text: "Loading model…" };
  return { tone: "ok", text: "Ready" };
}

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [page, setPage] = useState<Page>("home");
  const [toastMsg, setToastMsg] = useState<{ text: string; error: boolean; key: number } | null>(null);
  const [scrolled, setScrolled] = useState(false);
  const pendingSave = useRef<number | null>(null);
  const latest = useRef<Settings | null>(null);
  const mainRef = useRef<HTMLElement>(null);

  const refreshStatus = useCallback(() => {
    api.getStatus().then(setStatus).catch(() => {});
  }, []);

  const toast = useCallback((text: string, error = false) => {
    const key = Date.now();
    setToastMsg({ text, error, key });
    window.setTimeout(() => setToastMsg((t) => (t?.key === key ? null : t)), error ? 5000 : 2200);
  }, []);

  const reload = useCallback(() => {
    api.getSettings().then((s) => {
      latest.current = s;
      setSettings(s);
    });
    refreshStatus();
  }, [refreshStatus]);

  useEffect(() => {
    reload();
    const unlisten = [
      listen("status", refreshStatus),
      listen("model-loading", refreshStatus),
      listen("local-ai-state", refreshStatus),
      listen("update-ready", refreshStatus),
      listen("permissions-changed", refreshStatus),
      // Changes made elsewhere (tray menu, first download) — don't clobber unsaved edits.
      listen("settings-changed", () => {
        if (pendingSave.current === null) reload();
        else refreshStatus();
      }),
      listen<string>("navigate", (e) => setPage(e.payload as Page)),
    ];
    return () => unlisten.forEach((p) => p.then((f) => f()));
  }, [reload, refreshStatus]);

  // Access is granted in system settings, which doesn't tell the app, so look again
  // while something's missing: every couple of seconds, and on coming back to the window.
  const needsAccess =
    !!status && (status.permissions.microphone !== "granted" || status.permissions.typing === "denied");
  useEffect(() => {
    if (!needsAccess) return;
    const timer = window.setInterval(refreshStatus, 2000);
    window.addEventListener("focus", refreshStatus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", refreshStatus);
    };
  }, [needsAccess, refreshStatus]);

  const theme = settings?.theme;
  useEffect(() => {
    if (theme) setTheme(theme);
  }, [theme]);

  // Each pane opens at the top.
  useEffect(() => {
    mainRef.current?.scrollTo({ top: 0 });
    setScrolled(false);
  }, [page]);

  const update = useCallback(
    (patch: Patch) => {
      const base = latest.current;
      if (!base) return;
      const next = typeof patch === "function" ? patch(base) : { ...base, ...patch };
      latest.current = next;
      setSettings(next);
      if (pendingSave.current !== null) window.clearTimeout(pendingSave.current);
      pendingSave.current = window.setTimeout(() => {
        pendingSave.current = null;
        api
          .saveSettings(latest.current!)
          .then(refreshStatus)
          .catch((e) => {
            toast(String(e), true);
            reload();
          });
      }, 400);
    },
    [refreshStatus, reload, toast],
  );

  if (!settings) return null;

  const ctx: Ctx = { settings, update, status, refreshStatus, toast, go: setPage };
  const ready = readiness(settings, status);
  const current = PAGES.find((p) => p.id === page)!;
  const split = SPLIT_PAGES.includes(page);

  const navItem = (p: PageDef) => (
    <button
      key={p.id}
      className={`nav-item ${page === p.id ? "selected" : ""}`}
      onClick={() => setPage(p.id)}
      aria-current={page === p.id ? "page" : undefined}
    >
      <p.icon size={17} />
      <span>{p.label}</span>
      {p.id === "models" && !settings.selected_model && <span className="nav-badge" title="Action needed" />}
      {p.id === "ai" && settings.ai_enabled && status && !status.ai_ready && <span className="nav-badge" title="Not set up" />}
      {p.id === "general" && status?.update && <span className="nav-badge" title="Update ready" />}
    </button>
  );

  return (
    <AppContext.Provider value={ctx}>
      <div className="window">
        <aside className="sidebar">
          <div className="titlebar-space" data-tauri-drag-region />
          <button className="identity" onClick={() => setPage("home")}>
            <img src="/app-icon.svg" alt="" />
            <span className="identity-text">
              <b>Lipwise</b>
              <span className={`status status-${ready.tone}`}>
                <i />
                {ready.text}
              </span>
            </span>
          </button>
          <nav>
            {SECTIONS.map((section, i) => (
              <div className="nav-section" key={i}>
                {section.title && <h2 className="nav-caption">{section.title}</h2>}
                {section.pages.map(navItem)}
              </div>
            ))}
          </nav>
          <nav className="nav-footer">{FOOTER.map(navItem)}</nav>
        </aside>

        {split ? (
          <main className="main main-split" key={page}>
            {page === "history" && <History />}
          </main>
        ) : (
          <main className="main" ref={mainRef} onScroll={(e) => setScrolled(e.currentTarget.scrollTop > 72)}>
            <div className={`toolbar ${scrolled ? "scrolled" : ""}`} data-tauri-drag-region>
              <span className="toolbar-title" data-tauri-drag-region>
                {current.label}
              </span>
            </div>
            <div className="pane" key={page}>
              {page === "home" && <Home />}
              {page === "models" && <Models />}
              {page === "ai" && <AI />}
              {page === "commands" && <Commands />}
              {page === "general" && <General />}
            </div>
          </main>
        )}
      </div>
      {toastMsg && (
        <div className={`hud ${toastMsg.error ? "hud-error" : ""}`} key={toastMsg.key} role="status">
          {toastMsg.text}
        </div>
      )}
    </AppContext.Provider>
  );
}
