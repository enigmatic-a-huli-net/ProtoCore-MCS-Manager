<script lang="ts">
  // Набор иконок в одном месте — без внешних библиотек, чтобы не раздувать сборку.
  let { name, size = 16 }: { name: string; size?: number } = $props();

  const P: Record<string, string> = {
    play: "M7 4.5v15l12-7.5z",
    stop: "M6.5 6.5h11v11h-11z",
    restart: "M20 12a8 8 0 1 1-2.34-5.66M20 4v4.5h-4.5",
    plus: "M12 5v14M5 12h14",
    folder: "M3.5 7a2 2 0 0 1 2-2h4l2 2.5h7a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z",
    settings: "M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4zM19.4 13.5l1.6 1.2-2 3.4-1.9-.7a7.5 7.5 0 0 1-1.9 1.1l-.3 2h-3.8l-.3-2a7.5 7.5 0 0 1-1.9-1.1l-1.9.7-2-3.4 1.6-1.2a7 7 0 0 1 0-2.9L3 9.3l2-3.4 1.9.7a7.5 7.5 0 0 1 1.9-1.1l.3-2h3.8l.3 2a7.5 7.5 0 0 1 1.9 1.1l1.9-.7 2 3.4-1.6 1.2a7 7 0 0 1 0 2.9z",
    terminal: "M4 5.5h16v13H4zM7.5 10l2.5 2-2.5 2M12 14.5h4.5",
    puzzle: "M10 4.5a2 2 0 1 1 4 0V6h4v4h-1.5a2 2 0 1 0 0 4H18v4h-4v-1.5a2 2 0 1 0-4 0V18H6v-4h1.5a2 2 0 1 0 0-4H6V6h4z",
    plug: "M9 3.5v4M15 3.5v4M6.5 7.5h11V11a5.5 5.5 0 0 1-11 0zM12 16.5v4",
    archive: "M4 5h16v4H4zM5.5 9v10h13V9M10 13h4",
    rocket: "M14 4.5c3 0 5.5 2.5 5.5 5.5L13 16.5 7.5 11zM7.5 11 5 11.5 3.5 15l3.5-.5M13 16.5l-.5 2.5-3.5 1.5.5-3.5M15 9.5h.01",
    sliders: "M4 7h9M17 7h3M4 17h3M11 17h9M15 5v4M9 15v4",
    file: "M6.5 3.5h7l4 4v13h-11zM13.5 3.5v4h4M9 12.5h6M9 16h6",
    x: "M6 6l12 12M18 6 6 18",
    min: "M5 12h14",
    max: "M5.5 5.5h13v13h-13z",
    search: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM15.5 15.5 20 20",
    alert: "M12 4 21 19.5H3zM12 10v4M12 16.8v.01",
    check: "M5 12.5l4.5 4.5L19 7.5",
    trash: "M4.5 7h15M9.5 7V4.5h5V7M6.5 7l1 13h9l1-13",
    back: "M9 7 4.5 11.5 9 16M5 11.5h9a5.5 5.5 0 0 1 0 11h-2",
    download: "M12 4v11M7.5 10.5 12 15l4.5-4.5M5 19.5h14",
    link: "M10 14a4 4 0 0 0 5.66 0l3-3a4 4 0 0 0-5.66-5.66l-1 1M14 10a4 4 0 0 0-5.66 0l-3 3a4 4 0 0 0 5.66 5.66l1-1",
    skull: "M12 3.5a7.5 7.5 0 0 0-4.5 13.5v3h9v-3A7.5 7.5 0 0 0 12 3.5zM9 11h.01M15 11h.01M10.5 20.5v-2M13.5 20.5v-2",
    user: "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4.5 20.5a7.5 7.5 0 0 1 15 0",
    shield: "M12 3.5 19 6v6c0 4.5-3 7.5-7 8.5-4-1-7-4-7-8.5V6z",
    chevron: "M9 6l6 6-6 6",
    copy: "M8.5 8.5h11v11h-11zM5 15.5h-.5V4.5h11V5",
  };
  const filled = new Set(["play", "stop"]);
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill={filled.has(name) ? "currentColor" : "none"}
  stroke="currentColor"
  stroke-width={filled.has(name) ? 0 : 1.8}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  style="flex:none"
>
  <path d={P[name] ?? P.file} />
</svg>
