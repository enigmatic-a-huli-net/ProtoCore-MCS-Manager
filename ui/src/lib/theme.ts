// Применение темы: базовые цвета из CSS (app.css), пользовательские поверх.
import type { Theme } from "./api";

export const DEFAULT_THEME: Theme = {
  mode: "dark", accent: "#b85ad8", background: "", surface: "", text: "",
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
  root.style.setProperty("--radius", `${t.radius}px`);
  root.style.setProperty("--fs", `${t.font_size}px`);
}
