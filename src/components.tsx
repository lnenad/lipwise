import { useEffect, useState, type ComponentType, type ReactNode, type SelectHTMLAttributes } from "react";
import { shortcutKeys } from "./api";
import { Check, ChevronRight, ChevronUpDown, Minus } from "./icons";

type Icon = ComponentType<{ size?: number | string }>;
export type Tint = "blue" | "indigo" | "purple" | "pink" | "red" | "orange" | "green" | "teal" | "gray" | "brand" | "ai";

/** A rounded, colored square holding a glyph, as in System Settings. */
export function IconTile({ icon: I, tint, size = "md" }: { icon: Icon; tint: Tint; size?: "sm" | "md" | "lg" }) {
  return (
    <span className={`tile tile-${size} tint-${tint}`}>
      <I />
    </span>
  );
}

/** A macOS-style app icon: a squircle with a lit gradient, holding a solid glyph or a letter. */
export function AppIcon({ tint, icon: I, letter, size = 32 }: { tint: Tint; icon?: Icon; letter?: string; size?: number }) {
  return (
    <span className={`app-icon tint-${tint}`} style={{ width: size, height: size, fontSize: size * 0.56 }}>
      {I ? <I /> : <b>{letter}</b>}
    </span>
  );
}

/** The header at the top of every pane. */
export function PaneHeader({ icon, tint, title, children }: { icon: Icon; tint: Tint; title: string; children?: ReactNode }) {
  return (
    <header className="pane-header">
      <IconTile icon={icon} tint={tint} size="lg" />
      <h1>{title}</h1>
      {children && <p>{children}</p>}
    </header>
  );
}

/** An inset grouped list: optional header, the rounded container, optional footer. */
export function Group({
  title,
  accessory,
  footer,
  className = "",
  children,
}: {
  title?: ReactNode;
  accessory?: ReactNode;
  footer?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section className="group">
      {(title || accessory) && (
        <div className="group-header">
          <h2>{title}</h2>
          {accessory}
        </div>
      )}
      <div className={`group-body ${className}`}>{children}</div>
      {footer && <div className="group-footer">{footer}</div>}
    </section>
  );
}

/** One cell in a group: label and detail text on the left, a control on the right. */
export function Row({
  title,
  hint,
  icon,
  tint,
  children,
  onClick,
  className: extra = "",
}: {
  title: ReactNode;
  hint?: ReactNode;
  icon?: Icon;
  tint?: Tint;
  children?: ReactNode;
  onClick?: () => void;
  className?: string;
}) {
  const className = `${icon ? "has-icon" : ""} ${extra}`;
  const content = (
    <>
      {icon && <IconTile icon={icon} tint={tint ?? "gray"} size="sm" />}
      <div className="row-label">
        <span className="row-title">{title}</span>
        {hint && <span className="row-hint">{hint}</span>}
      </div>
      {children !== undefined && <div className="row-accessory">{children}</div>}
    </>
  );
  return onClick ? (
    <button className={`row row-button ${className}`} onClick={onClick}>
      {content}
    </button>
  ) : (
    <div className={`row ${className}`}>{content}</div>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label?: string;
}) {
  return (
    <label className="switch">
      <input type="checkbox" role="switch" aria-label={label} checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span />
    </label>
  );
}

/** A native select drawn as a macOS pop-up button. */
export function Select({ className = "", children, ...rest }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <span className={`popup ${className}`}>
      <select {...rest}>{children}</select>
      <ChevronUpDown />
    </span>
  );
}

export function Keys({ shortcut, size }: { shortcut: string; size?: "lg" }) {
  return (
    <span className={`keys ${size === "lg" ? "keys-lg" : ""}`}>
      {shortcutKeys(shortcut).map((k, i) => (
        <kbd key={i}>{k}</kbd>
      ))}
    </span>
  );
}

export function Tag({ tone = "gray", children }: { tone?: "gray" | "blue" | "green" | "orange" | "red" | "purple"; children: ReactNode }) {
  return <span className={`tag tag-${tone}`}>{children}</span>;
}

/** Apple-style activity indicator. */
export function Spinner({ size = 16 }: { size?: number }) {
  return (
    <span className="spinner" style={{ width: size, height: size }} role="progressbar" aria-label="Loading">
      {Array.from({ length: 8 }, (_, i) => (
        <i key={i} style={{ transform: `rotate(${i * 45}deg)`, animationDelay: `${(i - 8) * 0.1}s` }} />
      ))}
    </span>
  );
}

/** Thin capsule progress bar. `value` is 0–100, or null for indeterminate. */
export function Progress({ value }: { value: number | null }) {
  return (
    <div className={`progress ${value === null ? "indeterminate" : ""}`}>
      <i style={value === null ? undefined : { width: `${Math.max(2, value)}%` }} />
    </div>
  );
}

export function Notice({ tone, children }: { tone: "warn" | "error" | "info" | "ok"; children: ReactNode }) {
  return <div className={`notice notice-${tone}`}>{children}</div>;
}

const MODIFIER_CODES = new Set([
  "ControlLeft", "ControlRight", "ShiftLeft", "ShiftRight",
  "AltLeft", "AltRight", "MetaLeft", "MetaRight",
]);

/** Click, then press a key combination. Produces strings like "ctrl+shift+Space". */
export function ShortcutInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [listening, setListening] = useState(false);

  useEffect(() => {
    if (!listening) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.code === "Escape") {
        setListening(false);
        return;
      }
      if (MODIFIER_CODES.has(e.code)) return;
      const mods = [
        e.ctrlKey && "ctrl",
        e.altKey && "alt",
        e.shiftKey && "shift",
        e.metaKey && "super",
      ].filter(Boolean);
      // A bare letter would fire constantly while typing; require a modifier
      // unless it's a function key.
      if (mods.length === 0 && !/^F\d+$/.test(e.code)) return;
      setListening(false);
      onChange([...mods, e.code].join("+"));
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [listening, onChange]);

  return (
    <button
      className={`shortcut-field ${listening ? "listening" : ""}`}
      onClick={() => setListening(true)}
      onBlur={() => setListening(false)}
      title="Click, then press a new key combination"
    >
      {listening ? <span className="shortcut-prompt">Type shortcut…</span> : <Keys shortcut={value} />}
    </button>
  );
}

/* ---------------------------------------------------------------- list + detail panes */

/** A small macOS-style checkbox. `mixed` draws the dash used by "select all". */
export function Checkbox({
  checked,
  mixed = false,
  onChange,
  label,
}: {
  checked: boolean;
  mixed?: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <label className={`checkbox ${checked || mixed ? "on" : ""}`} onClick={(e) => e.stopPropagation()}>
      <input type="checkbox" aria-label={label} checked={checked} aria-checked={mixed ? "mixed" : checked} onChange={(e) => onChange(e.target.checked)} />
      {mixed ? <Minus /> : checked && <Check />}
    </label>
  );
}

/** A selectable row in a list pane: optional checkbox, leading art, two lines of text, a trailing value. */
export function ListItem({
  selected,
  onSelect,
  check,
  leading,
  title,
  subtitle,
  trailing,
}: {
  selected: boolean;
  onSelect: () => void;
  check?: ReactNode;
  leading?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  trailing?: ReactNode;
}) {
  return (
    <div
      className={`list-item ${selected ? "selected" : ""}`}
      role="option"
      aria-selected={selected}
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect();
        }
      }}
    >
      {check !== undefined && <span className="list-check">{check}</span>}
      {leading}
      <span className="list-text">
        <span className="list-title">{title}</span>
        {subtitle && <span className="list-subtitle">{subtitle}</span>}
      </span>
      {trailing !== undefined && <span className="list-trailing">{trailing}</span>}
    </div>
  );
}

/** Title block at the top of a detail pane, with a hairline under it. */
export function DetailHeader({ title, subtitle, actions }: { title: ReactNode; subtitle?: ReactNode; actions?: ReactNode }) {
  return (
    <header className="detail-header">
      <div className="detail-heading">
        <h1>{title}</h1>
        {subtitle && <p>{subtitle}</p>}
      </div>
      {actions && <div className="detail-actions">{actions}</div>}
    </header>
  );
}

/** A collapsible section in a detail pane, with an optional value on the right of its header. */
export function Disclosure({
  title,
  value,
  defaultOpen = true,
  children,
}: {
  title: ReactNode;
  value?: ReactNode;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <section className={`disclosure ${open ? "open" : ""}`}>
      <button className="disclosure-header" onClick={() => setOpen(!open)} aria-expanded={open}>
        <span className="disclosure-title">{title}</span>
        <ChevronRight className="disclosure-chevron" />
        <span className="disclosure-value">{value}</span>
      </button>
      {open && <div className="disclosure-body">{children}</div>}
    </section>
  );
}

/** Floating bar for acting on checked items: a strip of their icons, a summary, and the actions. */
export function ActionBar({ icons, summary, children }: { icons?: ReactNode[]; summary: ReactNode; children: ReactNode }) {
  const max = 9;
  return (
    <div className="action-bar" role="toolbar">
      <span className="action-info">
        {icons && icons.length > 0 && (
          <span className="action-icons">
            {icons.slice(0, max)}
            {icons.length > max && <span className="action-more">+{icons.length - max}</span>}
          </span>
        )}
        <span className="action-summary">{summary}</span>
      </span>
      <span className="action-buttons">{children}</span>
    </div>
  );
}

/** Two-step confirmation for destructive buttons: the first click arms it for a few seconds. */
export function useConfirm(ms = 3000): [boolean, (run: () => void) => void, () => void] {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const t = window.setTimeout(() => setArmed(false), ms);
    return () => window.clearTimeout(t);
  }, [armed, ms]);
  const trigger = (run: () => void) => {
    if (!armed) return setArmed(true);
    setArmed(false);
    run();
  };
  return [armed, trigger, () => setArmed(false)];
}
