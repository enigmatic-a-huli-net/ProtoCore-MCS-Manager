// Мост к ядру на Rust. Каждая функция = одна команда из src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type CoreKind =
  | "vanilla" | "paper" | "purpur" | "folia" | "spigot" | "fabric" | "quilt"
  | "forge" | "neoforge" | "youer" | "mohist" | "arclight" | "unknown";

export type Status = "stopped" | "starting" | "running" | "stopping" | "crashed";
export type Level = "info" | "warn" | "error" | "system";

export interface Theme {
  mode: "dark" | "light" | "system";
  accent: string;
  background: string;
  surface: string;
  text: string;
  radius: number;
  font_size: number;
  animations: boolean;
}

export interface LaunchSettings {
  mode: "auto" | "script";
  script: string;
  java_path: string;
  memory_mb: number;
  jvm_preset: "aikar" | "zgc" | "none";
  extra_jvm_args: string;
  server_args: string;
  jar_override: string;
  stop_timeout_sec: number;
}

export interface ServerEntry {
  id: string;
  name: string;
  path: string;
  core: CoreKind;
  core_override: boolean;
  mc_version: string;
  loader_version: string;
  launch: LaunchSettings;
  backup: {
    snapshot_on_start: boolean;
    repo_path: string;
    password: string;
    keep_last: number;
    excludes: string[];
  };
  mods: {
    auto_disable_client: boolean;
    disable_blacklisted: boolean;
    extra_client_ids: string[];
    never_disable: string[];
  };
  auto_restart: boolean;
}

export interface Config {
  language: "ru" | "en";
  theme: Theme;
  close_to_tray: boolean;
  console_lines: number;
  selected: string;
  servers: ServerEntry[];
}

export interface StatusInfo {
  status: Status;
  pid: number | null;
  uptime: number;
  players: string[];
  exit_code: number | null;
}

export interface LogLine { seq: number; ts: number; level: Level; text: string }

export interface Metrics { cpu: number; ram_bytes: number; system_ram_total: number; cores: number }

export interface ResticStatus { found: boolean; path: string; version: string }

export interface AppSnapshot {
  config: Config;
  paths: { base: string; data: string; restic_root: string; tools: string; branding: string };
  statuses: Record<string, StatusInfo>;
  restic: ResticStatus;
  version: string;
  os: string;
}

export interface Detection {
  core: CoreKind;
  mc_version: string;
  loader_version: string;
  target: { kind: "jar" | "args_file" | "none"; path?: string };
  scripts: string[];
  has_mods_dir: boolean;
  has_plugins_dir: boolean;
  eula_accepted: boolean;
  source: string;
}

export interface Dependency { id: string; required: boolean; version: string; side: string }

export interface ModInfo {
  file: string;
  enabled: boolean;
  size: number;
  id: string;
  loader: string | null;
  ids: string[];
  name: string;
  version: string;
  description: string;
  authors: string[];
  icon: string | null;
  side: "both" | "client" | "server" | "unknown";
  client: "no" | "likely" | "definite";
  client_reason: string;
  deps: Dependency[];
  provides: string[];
  breaks: string[];
  conflicts: string[];
  homepage: string;
  disabled_reason: string | null;
  disabled_detail: string | null;
  required_by: string[];
  missing_deps: string[];
  problems: string[];
}

export interface MissingDep { id: string; needed_by: string[]; disabled_file: string | null }
export interface Conflict { kind: "definite" | "possible"; a: string; b: string; reason: string }

export interface ModReport {
  mods: ModInfo[];
  missing: MissingDep[];
  conflicts: Conflict[];
  enabled_count: number;
  disabled_count: number;
  client_count: number;
}

export interface PluginInfo {
  file: string;
  enabled: boolean;
  size: number;
  name: string;
  version: string;
  description: string;
  authors: string[];
  main: string;
  api_version: string;
  website: string;
  depend: string[];
  softdepend: string[];
  paper_plugin: boolean;
  icon: string | null;
  required_by: string[];
  missing_deps: string[];
  problems: string[];
  disabled_reason: string | null;
}

export interface PluginReport { plugins: PluginInfo[]; missing: MissingDep[]; enabled_count: number; disabled_count: number }

export interface Snapshot {
  id: string;
  short_id: string;
  time: string;
  paths: string[];
  tags: string[];
  summary?: { data_added?: number; total_bytes_processed?: number; files_new?: number; files_changed?: number } | null;
}

export interface BackupResult { snapshot_id: string; files_new: number; files_changed: number; data_added: number; total_bytes: number; seconds: number }

export interface BackupEvent {
  id: string;
  phase: "start" | "progress" | "done" | "error" | "restore" | "restored";
  progress: { percent: number; files_done: number; bytes_done: number } | null;
  message: string;
}

export const api = {
  state: () => invoke<AppSnapshot>("get_state"),
  saveSettings: (settings: Pick<Config, "language" | "theme" | "close_to_tray" | "console_lines">) =>
    invoke<void>("save_settings", { settings }),
  branding: () => invoke<{ logo: string | null; splash: string | null }>("get_branding"),

  detect: (path: string) => invoke<Detection>("detect_folder", { path }),
  addServer: (path: string, name?: string) => invoke<ServerEntry>("add_server", { path, name }),
  removeServer: (id: string) => invoke<void>("remove_server", { id }),
  selectServer: (id: string) => invoke<void>("select_server", { id }),
  reorder: (ids: string[]) => invoke<void>("reorder_servers", { ids }),
  updateServer: (server: ServerEntry) => invoke<ServerEntry>("update_server", { server }),
  redetect: (id: string) => invoke<[ServerEntry, Detection]>("redetect_server", { id }),
  launchPreview: (id: string) => invoke<string>("launch_preview", { id }),
  acceptEula: (id: string) => invoke<void>("accept_eula", { id }),
  readProps: (id: string) => invoke<[string, string][]>("read_properties", { id }),
  writeProps: (id: string, changes: [string, string][]) => invoke<void>("write_properties", { id, changes }),

  start: (id: string) => invoke<void>("start_server", { id }),
  stop: (id: string) => invoke<void>("stop_server", { id }),
  kill: (id: string) => invoke<void>("kill_server", { id }),
  restart: (id: string) => invoke<void>("restart_server", { id }),
  send: (id: string, command: string) => invoke<void>("send_command", { id, command }),
  log: (id: string, after = 0) => invoke<LogLine[]>("get_log", { id, after }),
  clearLog: (id: string) => invoke<void>("clear_log", { id }),
  metrics: (id: string) => invoke<Metrics>("get_metrics", { id }),

  mods: (id: string) => invoke<ModReport>("list_mods", { id }),
  setMod: (id: string, file: string, enabled: boolean) => invoke<void>("set_mod_enabled", { id, file, enabled }),
  setMods: (id: string, files: string[], enabled: boolean) => invoke<void>("set_mods_enabled", { id, files, enabled }),
  disableClient: (id: string, includeLikely: boolean) =>
    invoke<{ file: string; name: string; reason: string }[]>("disable_client_mods", { id, includeLikely }),
  plugins: (id: string) => invoke<PluginReport>("list_plugins", { id }),
  setPlugin: (id: string, file: string, enabled: boolean) => invoke<void>("set_plugin_enabled", { id, file, enabled }),

  downloadRestic: () => invoke<ResticStatus>("download_restic"),
  resticStatus: () => invoke<ResticStatus>("restic_status"),
  snapshots: (id: string) => invoke<{ snapshots: Snapshot[]; repo_path: string; repo_size: number }>("list_snapshots", { id }),
  backupNow: (id: string) => invoke<BackupResult>("backup_now", { id }),
  deleteSnapshot: (id: string, snapshot: string) => invoke<void>("delete_snapshot", { id, snapshot }),
  rollback: (id: string, snapshot: string, startAfter: boolean) => invoke<void>("rollback", { id, snapshot, startAfter }),
  checkRepo: (id: string) => invoke<string>("check_repo", { id }),
};

export function on<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => cb(e.payload));
}

export function bytes(b: number, lang: "ru" | "en" = "ru"): string {
  const u = lang === "ru" ? ["Б", "КБ", "МБ", "ГБ", "ТБ"] : ["B", "KB", "MB", "GB", "TB"];
  let v = b, i = 0;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return i === 0 ? `${b} ${u[0]}` : `${v.toFixed(1)} ${u[i]}`;
}

export const CORE_LABEL: Record<CoreKind, string> = {
  vanilla: "Vanilla", paper: "Paper", purpur: "Purpur", folia: "Folia", spigot: "Spigot",
  fabric: "Fabric", quilt: "Quilt", forge: "Forge", neoforge: "NeoForge",
  youer: "Youer", mohist: "Mohist", arclight: "Arclight", unknown: "?",
};

export const supportsMods = (c: CoreKind) =>
  ["fabric", "quilt", "forge", "neoforge", "youer", "mohist", "arclight"].includes(c);
export const supportsPlugins = (c: CoreKind) =>
  ["paper", "purpur", "folia", "spigot", "youer", "mohist", "arclight"].includes(c);
