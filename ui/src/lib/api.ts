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
  /** 0.5 — медленно, 1 — обычно, 2 — быстро */
  anim_speed: number;
  /** показывать анимации, даже если в Windows включено «уменьшить движение» */
  anim_force: boolean;
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
    auto_quarantine: boolean;
    quarantine_content: boolean;
    quarantine_restart: boolean;
    quarantine_max_rounds: number;
  };
  auto_restart: boolean;
}

export interface Config {
  language: "ru" | "en";
  theme: Theme;
  close_to_tray: boolean;
  console_lines: number;
  check_updates: boolean;
  update_repo: string;
  curseforge_key: string;
  servers_root: string;
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
  wrong_deps: string[];
  problems: string[];
}

export interface WrongVersion { id: string; file: string; have: string; need: string; need_human: string; needed_by: string; needed_by_file: string }

export interface MissingDep { id: string; needed_by: string[]; disabled_file: string | null }
export interface Conflict { kind: "definite" | "possible"; a: string; b: string; reason: string }

export interface ModReport {
  mods: ModInfo[];
  missing: MissingDep[];
  wrong_versions: WrongVersion[];
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
  saveSettings: (settings: Pick<Config, "language" | "theme" | "close_to_tray" | "console_lines" | "check_updates" | "curseforge_key" | "servers_root">) =>
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

  analyze: (id: string, source: "auto" | "crash" | "latest" | "console") => invoke<Analysis>("analyze_logs", { id, source }),
  saveCompressed: (id: string, text: string) => invoke<string>("save_compressed", { id, text }),
  quarantine: (id: string, file: string, reason: string) => invoke<void>("quarantine_mod", { id, file, reason }),

  coreVersions: (core: CoreKind) => invoke<{ id: string; stable: boolean }[]>("core_versions", { core }),
  coreBuilds: (core: CoreKind, mc: string) => invoke<{ id: string; label: string; stable: boolean }[]>("core_builds", { core, mc }),
  createServer: (req: NewServer) => invoke<ServerEntry>("create_server", { req }),
  updateCore: (id: string, build: string) => invoke<ServerEntry>("update_core", { id, build }),
  javaCheck: (java: string) => invoke<string>("java_check", { java }),

  catSearch: (id: string, kind: Kind, source: Source, query: string, sort: string, offset: number) =>
    invoke<{ items: Project[]; total: number }>("catalog_search", { id, kind, source, query, sort, offset }),
  catVersions: (id: string, kind: Kind, source: Source, project: string) =>
    invoke<FileRef[]>("catalog_versions", { id, kind, source, project }),
  catPlan: (id: string, kind: Kind, source: Source, project: string, version?: string) =>
    invoke<Plan>("catalog_plan", { id, kind, source, project, version: version ?? null }),
  catApply: (id: string, kind: Kind, plan: Plan) => invoke<string[]>("catalog_apply", { id, kind, plan }),
  catUpdates: (id: string, kind: Kind) => invoke<UpdatesReport>("catalog_updates", { id, kind }),
  openPath: (path: string) => invoke<void>("open_path", { path, reveal: false }),
  revealPath: (path: string) => invoke<void>("open_path", { path, reveal: true }),
  checkCfKey: (key: string) => invoke<string>("check_cf_key", { key }),
  catFix: (id: string, modId: string, range: string) => invoke<Plan>("catalog_fix", { id, modId, range }),

  checkUpdate: () => invoke<UpdateInfo>("check_update"),
  installUpdate: () => invoke<void>("install_update"),
};

export interface UpdateInfo {
  available: boolean;
  current: string;
  latest: string;
  notes: string;
  published: string;
  html_url: string;
  asset_url: string;
  asset_size: number;
}

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

export interface Culprit { file: string; name: string; id: string; confidence: "low" | "medium" | "high"; client_only: boolean; enabled: boolean }
export interface Finding {
  fix: { mod_id: string; range: string } | null;
  code: string;
  severity: "info" | "warn" | "error";
  title: string;
  detail: string;
  hint: string;
  line: string;
  culprits: Culprit[];
  count: number;
}
export interface Analysis {
  source: string;
  source_path: string;
  findings: Finding[];
  compressed: string;
  total_lines: number;
  errors: number;
  warnings: number;
  lag_warnings: number;
}

export type Kind = "mod" | "plugin";
export type Source = "modrinth" | "hangar" | "curseforge";

export interface NewServer {
  name: string;
  dir: string;
  core: CoreKind;
  mc: string;
  build: string;
  memory_mb: number;
  java_path: string;
  accept_eula: boolean;
}

export interface Project {
  source: Source;
  id: string;
  slug: string;
  title: string;
  author: string;
  description: string;
  icon_url: string;
  downloads: number;
  url: string;
  server_side: string;
  installed: boolean;
}

export interface FileRef {
  source: Source;
  project_id: string;
  version_id: string;
  version_name: string;
  version_number: string;
  file_name: string;
  url: string;
  sha1: string | null;
  size: number;
  date: string;
  channel: string;
  deps: { project_id: string; required: boolean; name: string }[];
  incompatible: string[];
  page_url: string;
}

export interface PlanItem { title: string; icon_url: string; file: FileRef; reason: "selected" | "dependency" | "update" | "fix"; replaces: string | null }
export interface Plan { items: PlanItem[]; already: string[]; warnings: string[] }
export interface UpdateItem { file: string; title: string; icon_url: string; current: string; latest: FileRef; prerelease: boolean }
export interface UpdatesReport { updates: UpdateItem[]; unknown: string[]; checked: number }

export const DOWNLOADABLE: CoreKind[] = ["paper", "purpur", "folia", "fabric", "neoforge", "forge", "youer", "mohist", "arclight", "vanilla"];

export function compact(n: number): string {
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}K`;
  return String(n);
}
