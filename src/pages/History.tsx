import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useApp } from "../App";
import { api, type HistoryEntry, type Mode } from "../api";
import { ActionBar, AppIcon, Checkbox, DetailHeader, Disclosure, ListItem, Notice, Tag, useConfirm, type Tint } from "../components";
import { Clock, Copy, Search, TextFill, Trash, WandFill, WaveformFill, XMark } from "../icons";

const MODES: Record<Mode, { label: string; icon: typeof WaveformFill; tint: Tint }> = {
  dictate: { label: "Dictation", icon: WaveformFill, tint: "blue" },
  command: { label: "Command", icon: WandFill, tint: "purple" },
  plain: { label: "Plain", icon: TextFill, tint: "gray" },
};
const modeOf = (e: HistoryEntry) => MODES[e.mode] ?? MODES.dictate;

/** "Today", "Yesterday", or a date like "Monday, October 3". */
function dayLabel(date: Date): string {
  const today = new Date();
  const start = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((start(today) - start(date)) / 86_400_000);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  return date.toLocaleDateString([], {
    weekday: "long",
    month: "long",
    day: "numeric",
    year: date.getFullYear() === today.getFullYear() ? undefined : "numeric",
  });
}

const time = (e: HistoryEntry) => new Date(e.timestamp).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
const words = (text: string) => text.split(/\s+/).filter(Boolean).length;
const wordCount = (text: string) => {
  const n = words(text);
  return `${n} ${n === 1 ? "word" : "words"}`;
};

export default function History() {
  const { toast } = useApp();
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [query, setQuery] = useState("");
  const [viewing, setViewing] = useState<number | null>(null);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const [confirmDelete, armDelete, disarmDelete] = useConfirm();

  useEffect(() => {
    const load = () => api.getHistory().then(setEntries);
    load();
    const un = listen("history-changed", load);
    return () => {
      un.then((f) => f());
    };
  }, []);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return entries;
    return entries.filter((e) => e.output.toLowerCase().includes(q) || e.transcript.toLowerCase().includes(q));
  }, [entries, query]);

  const days = useMemo(() => {
    const groups: { label: string; items: HistoryEntry[] }[] = [];
    for (const e of visible) {
      const label = dayLabel(new Date(e.timestamp));
      const last = groups[groups.length - 1];
      if (last?.label === label) last.items.push(e);
      else groups.push({ label, items: [e] });
    }
    return groups;
  }, [visible]);

  const copy = (text: string) => navigator.clipboard.writeText(text).then(() => toast("Copied"));

  const toggle = (id: number, on: boolean) =>
    setChecked((all) => {
      const next = new Set(all);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });

  const selected = entries.filter((e) => checked.has(e.id));
  const allChecked = visible.length > 0 && visible.every((e) => checked.has(e.id));
  const someChecked = !allChecked && visible.some((e) => checked.has(e.id));

  const remove = async (ids: number[]) => {
    try {
      if (ids.length === entries.length) await api.clearHistory();
      else await api.deleteHistory(ids);
    } catch (e) {
      toast(String(e), true);
    }
    setChecked(new Set());
    setEntries(await api.getHistory());
  };

  if (entries.length === 0) {
    return (
      <div className="split-empty" data-tauri-drag-region>
        <div className="empty">
          <Clock size={36} />
          <b>No dictations yet</b>
          <span>What you dictate will show up here, with what Lipwise heard. It stays on this computer.</span>
        </div>
      </div>
    );
  }

  const shown = entries.find((e) => e.id === viewing) ?? visible[0];

  return (
    <div className="split">
      <section className="split-list">
        <div className="list-toolbar" data-tauri-drag-region>
          <label className="search">
            <Search />
            <input type="text" placeholder="Search history" value={query} onChange={(e) => setQuery(e.target.value)} />
            {query && (
              <button className="search-clear" onClick={() => setQuery("")} aria-label="Clear search">
                <XMark />
              </button>
            )}
          </label>
          <div className="list-select-all">
            <Checkbox
              label="Select all"
              checked={allChecked}
              mixed={someChecked}
              onChange={() => setChecked(allChecked || someChecked ? new Set() : new Set(visible.map((e) => e.id)))}
            />
            <span>
              {visible.length} {visible.length === 1 ? "item" : "items"}
            </span>
          </div>
        </div>

        <div className="list-scroll" role="listbox" aria-label="Dictations">
          {days.map((day) => (
            <div key={day.label}>
              <h2 className="list-caption">{day.label}</h2>
              {day.items.map((e) => {
                const m = modeOf(e);
                return (
                  <ListItem
                    key={e.id}
                    selected={shown?.id === e.id}
                    onSelect={() => setViewing(e.id)}
                    check={<Checkbox label="Select" checked={checked.has(e.id)} onChange={(on) => toggle(e.id, on)} />}
                    leading={<AppIcon icon={m.icon} tint={m.tint} />}
                    title={e.output || <span className="list-muted">Nothing typed</span>}
                    subtitle={`${time(e)} · ${m.label}${e.ai_used ? " · AI" : ""}`}
                    trailing={e.output ? words(e.output) : undefined}
                  />
                );
              })}
            </div>
          ))}
          {visible.length === 0 && <div className="list-empty">Nothing matches “{query}”.</div>}
        </div>
      </section>

      <section className="split-detail">
        <div className="detail-scroll">
          <div className="detail-drag" data-tauri-drag-region />
          {shown && (
            <EntryDetail
              e={shown}
              onCopy={() => copy(shown.output)}
              onDelete={() => remove([shown.id])}
            />
          )}
        </div>
      </section>

      {selected.length > 0 && (
        <ActionBar
          icons={selected.map((e) => <AppIcon key={e.id} icon={modeOf(e).icon} tint={modeOf(e).tint} size={16} />)}
          summary={`${selected.length} selected · ${selected.reduce((n, e) => n + words(e.output), 0)} words`}
        >
          <button
            className="btn btn-sm btn-plain"
            onClick={() => {
              disarmDelete();
              setChecked(new Set());
            }}
          >
            Cancel
          </button>
          <button
            className="btn btn-sm btn-bar"
            onClick={() =>
              copy(
                selected
                  .map((e) => e.output)
                  .filter(Boolean)
                  .join("\n\n"),
              )
            }
          >
            Copy
          </button>
          <button
            className={`btn btn-sm btn-bar ${confirmDelete ? "btn-danger" : ""}`}
            onClick={() => armDelete(() => remove(selected.map((e) => e.id)))}
          >
            {confirmDelete ? "Confirm Delete" : "Delete"}
          </button>
        </ActionBar>
      )}
    </div>
  );
}

function EntryDetail({ e, onCopy, onDelete }: { e: HistoryEntry; onCopy: () => void; onDelete: () => void }) {
  const m = modeOf(e);
  return (
    <>
      <DetailHeader
        title={`${dayLabel(new Date(e.timestamp))}, ${time(e)}`}
        subtitle={
          <>
            {m.label}
            {e.ai_used && <Tag tone="purple">AI</Tag>}
          </>
        }
        actions={
          <>
            {e.output && (
              <button className="icon-btn" onClick={onCopy} title="Copy" aria-label="Copy">
                <Copy />
              </button>
            )}
            <button className="icon-btn" onClick={onDelete} title="Delete" aria-label="Delete">
              <Trash />
            </button>
          </>
        }
      />

      {e.note && <Notice tone="warn">{e.note}</Notice>}

      <Disclosure title="Typed" value={e.output ? wordCount(e.output) : undefined}>
        {e.output ? <div className="entry-text">{e.output}</div> : <span className="secondary">Nothing was typed.</span>}
      </Disclosure>

      {e.mode === "command" && e.selection && (
        <Disclosure title="Selected Text" value={wordCount(e.selection)}>
          <div className="entry-heard">{e.selection}</div>
        </Disclosure>
      )}

      <Disclosure title={e.mode === "command" ? "Instruction" : "Heard"} value={wordCount(e.transcript)}>
        <div className="entry-heard">“{e.transcript}”</div>
      </Disclosure>
    </>
  );
}
