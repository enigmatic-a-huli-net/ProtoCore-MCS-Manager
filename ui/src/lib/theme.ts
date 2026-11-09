// Применение темы: базовые цвета из CSS (app.css), пользовательские поверх.
import type { Theme } from "./api";

export const DEFAULT_THEME: Theme = {
  mode: "dark", accent: "#b85ad8", background: "", surface: "", text: "",
  surface2: "", line: "", muted: "", console: "", ok: "", warn: "", err: "", info: "",
  radius: 10, font_size: 14, animations: true, anim_speed: 1, anim_force: false,
};

export const ACCENTS = ["#b85ad8", "#7c5cff", "#3fa7ff", "#2fbf8f", "#e8a33d", "#ef5b6b", "#d36bd6", "#8a9a5b"];

export function applyTheme(t: Theme) {
  const root = document.documentElement;
  const dark = t.mode === "system" ? matchMedia("(prefers-color-scheme: dark)").matches : t.mode === "dark";
  root.dataset.theme = dark ? "dark" : "light";
  root.dataset.anim = !t.animations ? "off" : t.anim_force ? "force" : "on";
  root.style.setProperty("--anim-k", String(1 / Math.min(2, Math.max(0.5, t.anim_speed || 1))));
  const set = (k: string, v: string) => (v ? root.style.setProperty(k, v) : root.style.removeProperty(k));
  set("--accent", t.accent);
  set("--bg", t.background);
  set("--surface", t.surface);
  set("--text", t.text);
  set("--surface-2", t.surface2);
  set("--line", t.line);
  set("--muted", t.muted);
  set("--console", t.console);
  set("--ok", t.ok);
  set("--warn", t.warn);
  set("--err", t.err);
  set("--info", t.info);
  root.style.setProperty("--radius", `${t.radius}px`);
  root.style.setProperty("--fs", `${t.font_size}px`);
}

/** #rgb, #rrggbb или #rrggbbaa → нормализованный #rrggbb(aa); иначе null */
export function normHex(v: string): string | null {
  let s = v.trim().toLowerCase();
  if (!s.startsWith("#")) s = "#" + s;
  if (/^#[0-9a-f]{3}$/.test(s)) s = "#" + [...s.slice(1)].map((c) => c + c).join("");
  return /^#([0-9a-f]{6}|[0-9a-f]{8})$/.test(s) ? s : null;
}

type P = Partial<Theme> & { mode: Theme["mode"]; accent: string };
const preset = (p: P): Theme => ({ ...DEFAULT_THEME, ...p });

/** Встроенные пресеты */
export const BUILTIN_PRESETS: { name: string; theme: Theme }[] = [
  { name: "ProtoCore", theme: preset({ mode: "dark", accent: "#b85ad8" }) },
  { name: "ProtoCore Light", theme: preset({ mode: "light", accent: "#9b3fc0" }) },
  {
    name: "Midnight",
    theme: preset({ mode: "dark", accent: "#5aa9ff", background: "#0e1220", surface: "#151b2e", surface2: "#1d2540", line: "#2a3456", text: "#e6ebf7", muted: "#8b96b5", console: "#0a0e19" }),
  },
  {
    name: "Forest",
    theme: preset({ mode: "dark", accent: "#3ecf8e", background: "#101714", surface: "#16201b", surface2: "#1e2b24", line: "#2a3a31", text: "#e4efe8", muted: "#8ea596", console: "#0b110e" }),
  },
  {
    name: "Ember",
    theme: preset({ mode: "dark", accent: "#f08a4b", background: "#1a1412", surface: "#231b18", surface2: "#2e2420", line: "#3d302a", text: "#f3e9e4", muted: "#a8958c", console: "#130e0c" }),
  },
  {
    name: "Sakura",
    theme: preset({ mode: "light", accent: "#e0568a", background: "#fbf3f6", surface: "#ffffff", surface2: "#f6e7ee", line: "#ecd5df", text: "#2b1a22", muted: "#86697a", console: "#fffafc" }),
  },
  {
    name: "Mono",
    theme: preset({ mode: "dark", accent: "#d0d0d0", background: "#121212", surface: "#1a1a1a", surface2: "#232323", line: "#303030", text: "#ededed", muted: "#8f8f8f", console: "#0c0c0c", radius: 4 }),
  },
];
