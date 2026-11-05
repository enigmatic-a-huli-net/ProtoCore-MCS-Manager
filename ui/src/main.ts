import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

// Отключаем стандартное меню браузера по правому клику — это приложение, а не сайт
document.addEventListener("contextmenu", (e) => {
  const el = e.target as HTMLElement;
  if (!el.closest("input, textarea, .console-body")) e.preventDefault();
});

export default mount(App, { target: document.getElementById("app")! });
