// Заготовка логотипа. Свой логотип: файл branding/logo.png рядом с .exe (без пересборки)
// или замени ui/src/assets/logo.svg и пересобери.
import defaultLogo from "../assets/logo.svg";
import { app } from "./store.svelte";

export const logo = () => app.branding.logo ?? defaultLogo;
