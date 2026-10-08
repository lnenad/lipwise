/** SF Symbols–style line icons. Sized by font-size (1em) and colored by currentColor. */
import type { SVGProps } from "react";

type IconProps = SVGProps<SVGSVGElement> & { size?: number | string };

function base({ size = "1em", ...rest }: IconProps) {
  return {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
    ...rest,
  };
}

export const Waveform = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M4 10v4M8 6v12M12 3v18M16 7v10M20 10v4" />
  </svg>
);

export const Mic = (p: IconProps) => (
  <svg {...base(p)}>
    <rect x="9" y="2.5" width="6" height="12" rx="3" />
    <path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21" />
  </svg>
);

export const Sparkles = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M10 3.5 11.6 8a3 3 0 0 0 1.9 1.9L18 11.5l-4.5 1.6a3 3 0 0 0-1.9 1.9L10 19.5 8.4 15a3 3 0 0 0-1.9-1.9L2 11.5l4.5-1.6A3 3 0 0 0 8.4 8L10 3.5Z" />
    <path d="M18.5 2.5v4M16.5 4.5h4M19 16.5v3M17.5 18h3" />
  </svg>
);

export const Bubble = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M12 3.5c5 0 9 3.3 9 7.5s-4 7.5-9 7.5c-1 0-2-.1-2.9-.4L4.5 20l1.2-3.7C4 15 3 13.1 3 11c0-4.2 4-7.5 9-7.5Z" />
    <path d="M8 9.5h8M8 12.5h5" />
  </svg>
);

export const Gear = (p: IconProps) => (
  <svg {...base(p)}>
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z" />
  </svg>
);

export const Clock = (p: IconProps) => (
  <svg {...base(p)}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5l3 2" />
  </svg>
);

export const House = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M3.5 10.5 12 3.5l8.5 7" />
    <path d="M5.5 9v10.5h13V9" />
    <path d="M10 19.5v-5h4v5" />
  </svg>
);

export const Keyboard = (p: IconProps) => (
  <svg {...base(p)}>
    <rect x="2.5" y="5.5" width="19" height="13" rx="2.5" />
    <path d="M6.5 9.5h.01M10 9.5h.01M13.5 9.5h.01M17 9.5h.01M6.5 12.5h.01M10 12.5h.01M13.5 12.5h.01M17 12.5h.01M8 15.5h8" />
  </svg>
);

export const Check = (p: IconProps) => (
  <svg {...base({ strokeWidth: 2.4, ...p })}>
    <path d="m5 12.5 4.5 4.5L19 7.5" />
  </svg>
);

export const ChevronRight = (p: IconProps) => (
  <svg {...base({ strokeWidth: 2.2, ...p })}>
    <path d="m9.5 6 6 6-6 6" />
  </svg>
);

export const ChevronUpDown = (p: IconProps) => (
  <svg {...base({ strokeWidth: 2.2, ...p })}>
    <path d="m8 9.5 4-4 4 4M8 14.5l4 4 4-4" />
  </svg>
);

export const Search = (p: IconProps) => (
  <svg {...base(p)}>
    <circle cx="10.5" cy="10.5" r="6.5" />
    <path d="m15.5 15.5 5 5" />
  </svg>
);

export const Plus = (p: IconProps) => (
  <svg {...base({ strokeWidth: 2.2, ...p })}>
    <path d="M12 5v14M5 12h14" />
  </svg>
);

export const Minus = (p: IconProps) => (
  <svg {...base({ strokeWidth: 2.4, ...p })}>
    <path d="M6 12h12" />
  </svg>
);

export const XMark = (p: IconProps) => (
  <svg {...base({ strokeWidth: 2.2, ...p })}>
    <path d="m7 7 10 10M17 7 7 17" />
  </svg>
);

export const Copy = (p: IconProps) => (
  <svg {...base(p)}>
    <rect x="8.5" y="8.5" width="12" height="12" rx="2.5" />
    <path d="M15.5 8.5v-2a3 3 0 0 0-3-3h-6a3 3 0 0 0-3 3v6a3 3 0 0 0 3 3h2" />
  </svg>
);

export const Trash = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M4 6.5h16M9.5 6.5V4.5h5v2M6 6.5l1 13.5h10l1-13.5M10 10.5v6M14 10.5v6" />
  </svg>
);

export const ArrowDown = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M12 4v14M6 12.5l6 6 6-6" />
  </svg>
);

export const Lock = (p: IconProps) => (
  <svg {...base(p)}>
    <rect x="4.5" y="10.5" width="15" height="10.5" rx="2.5" />
    <path d="M8 10.5V7.5a4 4 0 0 1 8 0v3" />
  </svg>
);

export const Desktop = (p: IconProps) => (
  <svg {...base(p)}>
    <rect x="2.5" y="4" width="19" height="13" rx="2" />
    <path d="M8.5 21h7M12 17v4" />
  </svg>
);

export const Cloud = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M7 19a4.5 4.5 0 0 1-.6-9A6 6 0 0 1 18 9.5a4.75 4.75 0 0 1-.5 9.5H7Z" />
  </svg>
);

export const Server = (p: IconProps) => (
  <svg {...base(p)}>
    <rect x="3.5" y="4" width="17" height="7" rx="2" />
    <rect x="3.5" y="13" width="17" height="7" rx="2" />
    <path d="M7 7.5h.01M7 16.5h.01" />
  </svg>
);

export const Cube = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="m12 3 8 4.5v9L12 21l-8-4.5v-9L12 3Z" />
    <path d="m4 7.5 8 4.5 8-4.5M12 12v9" />
  </svg>
);

export const Asterisk = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M12 3.5v17M4.6 7.75l14.8 8.5M4.6 16.25l14.8-8.5" />
  </svg>
);

export const Warning = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M10.3 4.2 2.6 17.5A2 2 0 0 0 4.3 20.5h15.4a2 2 0 0 0 1.7-3L13.7 4.2a2 2 0 0 0-3.4 0Z" />
    <path d="M12 9.5v4M12 17h.01" />
  </svg>
);

export const Info = (p: IconProps) => (
  <svg {...base(p)}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 11v5.5M12 7.75h.01" />
  </svg>
);

export const Text = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M4 6h16M4 10.5h16M4 15h10M4 19.5h7" />
  </svg>
);

export const Cursor = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M9 4h2.5M12.5 4H15M12 4v16M9 20h2.5M12.5 20H15" />
  </svg>
);

export const Wand = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="m4 20 11-11M13 7l4 4" />
    <path d="M18 3v3M16.5 4.5h3M20 9v2M19 10h2M10 3v2M9 4h2" />
  </svg>
);

/* ---------------------------------------------------------------- filled glyphs
   SF Symbols ".fill" style for the sidebar and app-icon tiles: solid shapes with
   cut-outs (even-odd fills), so they read as solid marks at small sizes. */

function solid({ size = "1em", ...rest }: IconProps) {
  return { width: size, height: size, viewBox: "0 0 24 24", fill: "currentColor", fillRule: "evenodd" as const, "aria-hidden": true, ...rest };
}

/** A gear outline with `teeth` trapezoid teeth and a round hole, as one even-odd path. */
function gearPath(teeth: number, outer: number, inner: number, hole: number) {
  const pts: string[] = [];
  const step = (Math.PI * 2) / teeth;
  const at = (r: number, a: number) => `${(12 + r * Math.cos(a)).toFixed(2)} ${(12 + r * Math.sin(a)).toFixed(2)}`;
  for (let i = 0; i < teeth; i++) {
    const a = i * step - Math.PI / 2;
    pts.push(at(inner, a - step * 0.36), at(outer, a - step * 0.2), at(outer, a + step * 0.2), at(inner, a + step * 0.36));
  }
  return `M${pts.join("L")}Z M${12 + hole} 12a${hole} ${hole} 0 1 0 ${-hole * 2} 0a${hole} ${hole} 0 1 0 ${hole * 2} 0Z`;
}
const GEAR = gearPath(8, 10.2, 7.6, 3.1);

export const HouseFill = (p: IconProps) => (
  <svg {...solid(p)}>
    <path d="M12.7 2.9a1.1 1.1 0 0 0-1.4 0L2.6 10.2a1 1 0 0 0 1.3 1.5l1.1-.9v8.7A1.5 1.5 0 0 0 6.5 21H10v-5a.6.6 0 0 1 .6-.6h2.8a.6.6 0 0 1 .6.6v5h3.5a1.5 1.5 0 0 0 1.5-1.5v-8.7l1.1.9a1 1 0 0 0 1.3-1.5l-8.7-7.3Z" />
  </svg>
);

export const MicFill = (p: IconProps) => (
  <svg {...solid(p)}>
    <rect x="8.25" y="1.75" width="7.5" height="13" rx="3.75" />
    <path d="M5.5 10.2a.9.9 0 0 1 .9.9 5.6 5.6 0 0 0 11.2 0 .9.9 0 1 1 1.8 0 7.4 7.4 0 0 1-6.5 7.35V20.5h2.6a.9.9 0 1 1 0 1.8H8.5a.9.9 0 1 1 0-1.8h2.6v-2.05a7.4 7.4 0 0 1-6.5-7.35.9.9 0 0 1 .9-.9Z" />
  </svg>
);

export const SparklesFill = (p: IconProps) => (
  <svg {...solid(p)}>
    <path d="M9.5 4.5c.75 4.1 2.9 6.25 7 7-4.1.75-6.25 2.9-7 7-.75-4.1-2.9-6.25-7-7 4.1-.75 6.25-2.9 7-7Z" />
    <path d="M18 1.5c.38 2.05 1.45 3.12 3.5 3.5-2.05.38-3.12 1.45-3.5 3.5-.38-2.05-1.45-3.12-3.5-3.5 2.05-.38 3.12-1.45 3.5-3.5Z" />
    <path d="M18.5 15.5c.3 1.6 1.1 2.4 2.7 2.7-1.6.3-2.4 1.1-2.7 2.7-.3-1.6-1.1-2.4-2.7-2.7 1.6-.3 2.4-1.1 2.7-2.7Z" />
  </svg>
);

export const BubbleFill = (p: IconProps) => (
  <svg {...solid(p)}>
    <path d="M12 3c5.25 0 9.5 3.5 9.5 7.85s-4.25 7.85-9.5 7.85c-.85 0-1.67-.09-2.45-.26-1.25 1.15-3 1.95-5.05 2.06a.4.4 0 0 1-.3-.68c.86-.9 1.42-2 1.6-3.2C3.84 15.2 2.5 13.15 2.5 10.85 2.5 6.5 6.75 3 12 3Z" />
  </svg>
);

export const ClockFill = (p: IconProps) => (
  <svg {...solid(p)}>
    <path d="M12 2.25a9.75 9.75 0 1 1 0 19.5 9.75 9.75 0 0 1 0-19.5Zm0 3.6a.95.95 0 0 0-.95.95V12c0 .32.16.62.43.8l3.4 2.25a.95.95 0 1 0 1.05-1.58l-2.98-1.98V6.8a.95.95 0 0 0-.95-.95Z" />
  </svg>
);

export const GearFill = (p: IconProps) => (
  <svg {...solid(p)} strokeLinejoin="round" stroke="currentColor" strokeWidth={1.2}>
    <path d={GEAR} />
  </svg>
);

/** Bars of a sound wave, drawn as solid rounded pills. */
export const WaveformFill = (p: IconProps) => (
  <svg {...solid(p)}>
    <rect x="2.6" y="9.5" width="2.4" height="5" rx="1.2" />
    <rect x="6.6" y="6" width="2.4" height="12" rx="1.2" />
    <rect x="10.8" y="2.75" width="2.4" height="18.5" rx="1.2" />
    <rect x="15" y="7" width="2.4" height="10" rx="1.2" />
    <rect x="19" y="9.75" width="2.4" height="4.5" rx="1.2" />
  </svg>
);

export const WandFill = (p: IconProps) => (
  <svg {...solid(p)}>
    <path d="M14.3 7.6a1.3 1.3 0 0 1 1.84 0l.26.26a1.3 1.3 0 0 1 0 1.84L5.6 20.5a1.3 1.3 0 0 1-1.84 0l-.26-.26a1.3 1.3 0 0 1 0-1.84L14.3 7.6Z" />
    <path d="M18 1.8c.3 1.65 1.15 2.5 2.8 2.8-1.65.3-2.5 1.15-2.8 2.8-.3-1.65-1.15-2.5-2.8-2.8 1.65-.3 2.5-1.15 2.8-2.8ZM10 1.5c.2 1.1.8 1.7 1.9 1.9-1.1.2-1.7.8-1.9 1.9-.2-1.1-.8-1.7-1.9-1.9 1.1-.2 1.7-.8 1.9-1.9ZM20 10.5c.2 1.1.8 1.7 1.9 1.9-1.1.2-1.7.8-1.9 1.9-.2-1.1-.8-1.7-1.9-1.9 1.1-.2 1.7-.8 1.9-1.9Z" />
  </svg>
);

export const TextFill = (p: IconProps) => (
  <svg {...solid(p)}>
    <rect x="3" y="4.5" width="18" height="2.6" rx="1.3" />
    <rect x="3" y="10.7" width="13" height="2.6" rx="1.3" />
    <rect x="3" y="16.9" width="16" height="2.6" rx="1.3" />
  </svg>
);
