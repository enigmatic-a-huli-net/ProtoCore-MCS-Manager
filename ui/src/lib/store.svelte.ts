// Общее состояние интерфейса (Svelte 5 runes).
import { api, on, type AppSnapshot, type BackupEvent, type Config, type LogLine, type ServerEntry, type StatusInfo } from "./api";
import { translate, type Key } from "./i18n";
import { applyTheme } from "./theme";

export type Tab = "console" | "mods" | "plugins" | "backups" | "launch" | "properties";

interface Toast { id: number; kind: "info" | "ok" | "error"; text: string }

export const app = $state({
  ready: false,
  config: null as Config | null,
  paths: null as AppSnapshot["paths"] | null,
  restic: { found: false, path: "", version: "" },
  version: "",
  statuses: {} as Record<string, StatusInfo>,
  tab: "console" as Tab,
  view: "server" as "server" | "settings",
  branding: { logo: null as string | null, splash: null as string | null },
  toasts: [] as Toast[],
  backup: {} as Record<string, BackupEvent | null>,
  addOpen: false,
  eulaFor: "" as string,
});

// Логи держим «сырыми» (без глубокой реактивности) — их тысячи строк.
const logStore: Record<string, LogLine[]> = {};
let logTick = $state(0);
export const logVersion = () => logTick;
export const getLog = (id: string) => logStore[id] ?? [];

export function t(key: Key, vars?: Record<string, string | number>): string {
  return translate(app.config?.language ?? "ru", key, vars);
}

export function selected(): ServerEntry | null {
  const c = app.config;
  if (!c) return null;
  return c.servers.find((s) => s.id === c.selected) ?? c.servers[0] ?? null;
}

export function status(id: string): StatusInfo {
  return app.statuses[id] ?? { status: "stopped", pid: null, uptime: 0, players: [], exit_code: null };
}

let toastSeq = 0;
export function toast(text: string, kind: Toast["kind"] = "info") {
  const id = ++toastSeq;
  app.toasts.push({ id, kind, text });
  setTimeout(() => (app.toasts = app.toasts.filter((x) => x.id !== id)), kind === "error" ? 7000 : 3500);
}

export function fail(e: unknown) {
  toast(t("err.generic", { e: String(e) }), "error");
}

function appendLines(id: string, lines: LogLine[]) {
  const cap = app.config?.console_lines ?? 5000;
  const cur = logStore[id] ?? [];
  const last = cur.length ? cur[cur.length - 1].seq : 0;
  const fresh = lines.filter((l) => l.seq > last);
  if (!fresh.length) return;
  let next = cur.concat(fresh);
  if (next.length > cap) next = next.slice(next.length - cap);
  logStore[id] = next;
  logTick++;
}

export async function refreshState() {
  const s = await api.state();
  app.config = s.config;
  app.paths = s.paths;
  app.restic = s.restic;
  app.version = s.version;
  app.statuses = s.statuses;
  applyTheme(s.config.theme);
}

export async function init() {
  await refreshState();
  app.branding = await api.branding().catch(() => ({ logo: null, splash: null }));

  // Консоль восстанавливается из буфера ядра (окно могло быть выгружено в трей)
  for (const s of app.config!.servers) {
    appendLines(s.id, await api.log(s.id, 0));
  }

  await on<{ id: string; lines: LogLine[] }>("pcm://log", ({ id, lines }) => appendLines(id, lines));
  await on<{ id: string; info: StatusInfo }>("pcm://status", ({ id, info }) => {
    app.statuses[id] = info;
  });
  await on<BackupEvent>("pcm://backup", (ev) => {
    app.backup[ev.id] = ev.phase === "done" || ev.phase === "restored" || ev.phase === "error" ? null : ev;
    if (ev.phase === "error") toast(ev.message, "error");
  });

  // Системная тема могла смениться — переприменяем
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => app.config && applyTheme(app.config.theme));
  app.ready = true;
}

export function clearLocalLog(id: string) {
  logStore[id] = [];
  logTick++;
}

export async function saveServer(s: ServerEntry) {
  const saved = await api.updateServer($state.snapshot(s) as ServerEntry);
  const c = app.config!;
  const i = c.servers.findIndex((x) => x.id === saved.id);
  if (i >= 0) c.servers[i] = saved;
  return saved;
}

export async function startServer(id: string) {
  try {
    await api.start(id);
  } catch (e) {
    if (String(e).includes("EULA_REQUIRED")) app.eulaFor = id;
    else fail(e);
  }
}
