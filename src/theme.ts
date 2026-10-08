import type { Theme } from "./api";

// The page is styled by `data-theme` on <html>, always "light" or "dark".
// "system" resolves through the OS setting and follows it when it changes.
// The choice is cached locally so the first paint is right before settings load.

const systemDark = window.matchMedia("(prefers-color-scheme: dark)");

function cached(): Theme {
  try {
    const t = localStorage.getItem("theme");
    if (t === "light" || t === "dark") return t;
  } catch {}
  return "system";
}

let current: Theme = cached();

function paint() {
  const resolved = current === "system" ? (systemDark.matches ? "dark" : "light") : current;
  document.documentElement.dataset.theme = resolved;
}

export function setTheme(theme: Theme) {
  current = theme;
  try {
    localStorage.setItem("theme", theme);
  } catch {}
  paint();
}

systemDark.addEventListener("change", paint);
paint();
