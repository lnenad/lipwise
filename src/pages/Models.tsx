import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useApp } from "../App";
import { api, formatBytes, type DownloadProgress, type ModelInfo } from "../api";
import { Group, Notice, PaneHeader, Progress, Row, Select, Tag } from "../components";
import { ArrowDown, Check, Mic, Search, XMark } from "../icons";

const LANGUAGES: [string, string][] = [
  ["auto", "Auto-detect"], ["en", "English"], ["es", "Spanish"], ["fr", "French"], ["de", "German"],
  ["it", "Italian"], ["pt", "Portuguese"], ["nl", "Dutch"], ["pl", "Polish"], ["ru", "Russian"],
  ["uk", "Ukrainian"], ["sr", "Serbian"], ["hr", "Croatian"], ["cs", "Czech"], ["sv", "Swedish"],
  ["da", "Danish"], ["fi", "Finnish"], ["nb", "Norwegian"], ["tr", "Turkish"], ["ar", "Arabic"],
  ["hi", "Hindi"], ["ja", "Japanese"], ["ko", "Korean"], ["zh", "Chinese"], ["vi", "Vietnamese"],
];

/** Five-segment rating, from a 0–100 score. */
function Rating({ label, value }: { label: string; value: number }) {
  const filled = Math.max(1, Math.round(value / 20));
  return (
    <span className="rating" title={`${label}: ${value}/100`}>
      {label}
      <span className="rating-bar">
        {Array.from({ length: 5 }, (_, i) => (
          <i key={i} className={i < filled ? "on" : ""} />
        ))}
      </span>
    </span>
  );
}

export default function Models() {
  const { settings, update, status, toast } = useApp();
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [showAll, setShowAll] = useState(false);
  const [query, setQuery] = useState("");

  const refresh = useCallback(() => api.listModels().then(setModels), []);

  useEffect(() => {
    refresh();
    const un = listen<DownloadProgress>("model-download", (e) => {
      const p = e.payload;
      if (p.stage === "downloading" || p.stage === "verifying") {
        setProgress((all) => ({ ...all, [p.id]: p }));
        return;
      }
      setProgress(({ [p.id]: _, ...rest }) => rest);
      if (p.stage === "error") toast(`Download failed: ${p.error}`, true);
      refresh();
    });
    return () => {
      un.then((f) => f());
    };
  }, [refresh, toast]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return models.filter((m) => {
      if (q) {
        return (
          m.name.toLowerCase().includes(q) ||
          m.description.toLowerCase().includes(q) ||
          m.languages.some((l) => l === q)
        );
      }
      return showAll || m.recommended || m.downloaded;
    });
  }, [models, showAll, query]);

  const download = (id: string) => {
    setProgress((all) => ({ ...all, [id]: { id, stage: "downloading", downloaded: 0, total: 0, error: null } }));
    api.downloadModel(id).catch(() => {});
  };

  const remove = async (m: ModelInfo) => {
    try {
      await api.deleteModel(m.id);
      refresh();
    } catch (e) {
      toast(String(e), true);
    }
  };

  const selected = models.find((m) => m.id === settings.selected_model);
  const languageUnsupported =
    selected && settings.language !== "auto" && selected.languages.length > 0 && !selected.languages.includes(settings.language);

  return (
    <>
      <PaneHeader icon={Mic} tint="red" title="Speech Models">
        Transcription runs entirely on this computer, so your voice never leaves it. Start with a recommended model. You can
        switch at any time.
      </PaneHeader>

      <Group
        footer={
          languageUnsupported ? undefined : "Choosing your language improves accuracy. Auto-detect works for most models."
        }
      >
        <Row title="Spoken Language">
          <Select value={settings.language} onChange={(e) => update({ language: e.target.value })}>
            {LANGUAGES.map(([code, name]) => (
              <option key={code} value={code}>
                {name}
              </option>
            ))}
          </Select>
        </Row>
      </Group>
      {languageUnsupported && (
        <Notice tone="warn">
          {selected.name} doesn't list this language, so it will auto-detect instead. Search the models below by language
          code (e.g. “{settings.language}”) to find one that does.
        </Notice>
      )}

      <div className="filter-bar">
        <div className="segmented" role="tablist">
          <button role="tab" aria-selected={!showAll} className={!showAll ? "active" : ""} onClick={() => setShowAll(false)}>
            Recommended
          </button>
          <button role="tab" aria-selected={showAll} className={showAll ? "active" : ""} onClick={() => setShowAll(true)}>
            All Models
          </button>
        </div>
        <label className="search">
          <Search />
          <input
            type="text"
            placeholder="Search by name or language (en, de, ja)"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {query && (
            <button className="search-clear" onClick={() => setQuery("")} aria-label="Clear search">
              <XMark />
            </button>
          )}
        </label>
      </div>

      <Group>
        {visible.map((m) => {
          const active = m.id === settings.selected_model;
          const p = progress[m.id];
          const pct = p && p.total > 0 ? (p.downloaded / p.total) * 100 : null;
          return (
            <div className={`cell model ${active ? "model-active" : ""}`} key={m.id}>
              <span className="model-check">{active && <Check />}</span>
              <div className="model-info">
                <div className="model-title">
                  <h3>{m.name}</h3>
                  {m.recommended && <Tag tone="blue">Recommended</Tag>}
                  {active && status?.loaded_model !== m.id && <Tag>Loading</Tag>}
                </div>
                <p className="model-desc">{m.description}</p>
                <div className="model-meta">
                  <span>{formatBytes(m.size_bytes)}</span>
                  <span>{m.languages.length === 1 ? m.languages[0].toUpperCase() : `${m.languages.length} languages`}</span>
                  <Rating label="Speed" value={m.speed} />
                  <Rating label="Accuracy" value={m.accuracy} />
                </div>
                {p && (
                  <div className="model-progress">
                    <Progress value={p.stage === "verifying" ? null : pct} />
                    <span>
                      {p.stage === "verifying"
                        ? "Verifying…"
                        : p.total > 0
                          ? `${formatBytes(p.downloaded)} of ${formatBytes(p.total)}`
                          : "Starting…"}
                    </span>
                  </div>
                )}
              </div>
              <div className="model-actions">
                {p ? (
                  <button className="btn btn-sm" onClick={() => api.cancelDownload(m.id)} disabled={p.stage === "verifying"}>
                    Cancel
                  </button>
                ) : m.downloaded ? (
                  <>
                    {active ? (
                      <span className="in-use">In Use</span>
                    ) : (
                      <button className="btn btn-sm btn-primary" onClick={() => update({ selected_model: m.id })}>
                        Use
                      </button>
                    )}
                    <button className="btn btn-sm btn-plain btn-destructive" onClick={() => remove(m)}>
                      Delete
                    </button>
                  </>
                ) : (
                  <button className="btn btn-sm get" onClick={() => download(m.id)}>
                    <ArrowDown />
                    Get
                  </button>
                )}
              </div>
            </div>
          );
        })}
        {visible.length === 0 && <div className="cell empty-cell">No models match “{query}”.</div>}
      </Group>
      {!query && !showAll && (
        <p className="after-note">
          Showing recommended and downloaded models.{" "}
          <button className="link" onClick={() => setShowAll(true)}>
            Browse all {models.length}
          </button>
        </p>
      )}
    </>
  );
}
