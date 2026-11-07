// Логотип. Свой можно положить в branding/logo.png рядом с .exe (без пересборки).
// Встроенный: в тёмной теме — вариант со светлой обводкой (чёрный силуэт иначе не виден).
import logoLight from "../assets/logo.png";
import logoDark from "../assets/logo-outlined.png";
import { app } from "./store.svelte";

export const logo = () => {
  if (app.branding.logo) return app.branding.logo;
  const t = app.config?.theme;
  const dark = t?.mode === "light" ? false : t?.mode === "dark" ? true : matchMedia("(prefers-color-scheme: dark)").matches;
  return dark ? logoDark : logoLight;
};
