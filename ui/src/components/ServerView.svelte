<script lang="ts">
  import { onDestroy } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import Console from "./tabs/Console.svelte";
  import Diagnostics from "./tabs/Diagnostics.svelte";
  import Mods from "./tabs/Mods.svelte";
  import Plugins from "./tabs/Plugins.svelte";
  import Backups from "./tabs/Backups.svelte";
  import Launch from "./tabs/Launch.svelte";
  import Properties from "./tabs/Properties.svelte";
  import Schedule from "./tabs/Schedule.svelte";
  import Optimize from "./tabs/Optimize.svelte";
  import { api, bytes, CORE_LABEL, supportsMods, supportsPlugins, type Metrics, type ServerEntry } from "../lib/api";
  import { app, status, t, fail, startServer, type Tab } from "../lib/store.svelte";
  import type { Key } from "../lib/i18n";

  let { server }: { server: ServerEntry } = $props();

  const st = $derived(status(server.id));
  const live = $derived(st.status === "running" || st.status === "starting" || st.status === "stopping");
  const busy = $derived(app.backup[server.id]);
  const tps = $derived(st.status === "running" ? app.tps[server.id] : undefined);

  let metrics = $state<Metrics | null>(null);
  let uptime = $state(0);

  // Метрики опрашиваются только пока сервер работает и окно видно
  const timer = setInterval(async () => {
    if (!live || document.hidden) return;
    try {
      metrics = await api.metrics(server.id);
    } catch {
      /* сервер мог как раз завершиться */
    }
  }, 2000);
  let tick = setInterval(() => live && uptime++, 1000);
  $effect(() => {
    uptime = st.uptime;
    if (!live) metrics = null;
  });
  onDestroy(() => {
    clearInterval(timer);
    clearInterval(tick);
  });

  function fmtUptime(s: number) {
    const [H, M, S] = app.config?.language === "en" ? ["h", "min", "s"] : ["ч", "мин", "с"];
    const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60);
    return h ? `${h} ${H} ${m} ${M}` : `${m} ${M} ${s % 60} ${S}`;
  }

  async function kill() {
    if (await ask(t("confirm.kill"), { kind: "warning" })) api.kill(server.id).catch(fail);
  }

  const tabs = $derived(
    [
      { id: "console", icon: "terminal", show: true },
      { id: "diag", icon: "alert", show: true },
      { id: "mods", icon: "puzzle", show: supportsMods(server.core) || server.core === "unknown" },
      { id: "plugins", icon: "plug", show: supportsPlugins(server.core) },
      { id: "backups", icon: "archive", show: true },
      { id: "schedule", icon: "restart", show: true },
      { id: "optimize", icon: "rocket", show: true },
      { id: "launch", icon: "sliders", show: true },
      { id: "properties", icon: "file", show: true },
    ].filter((x) => x.show) as { id: Tab; icon: string }[],
  );

  $effect(() => {
    if (!tabs.some((x) => x.id === app.tab)) app.tab = "console";
  });
</script>

<section class="view">
  <header class="hdr">
    <div class="core-badge {st.status}" title={t(`status.${st.status}` as Key)}>
      <span class="ring"></span>
      <span class="glyph">{CORE_LABEL[server.core].slice(0, 2)}</span>
    </div>

    <div class="ident">
      <div class="row">
        <h1>{server.name}</h1>
        <span class="chip {st.status === 'running' ? 'ok' : st.status === 'crashed' ? 'err' : live ? 'warn' : ''}">
          {t(`status.${st.status}` as Key)}
        </span>
      </div>
      <div class="row muted sub">
        <span>{CORE_LABEL[server.core]} {server.loader_version}</span>
        <span>Minecraft {server.mc_version || "?"}</span>
        <button class="path" onclick={() => api.openPath(server.path).catch(fail)} title={t("act.openFolder")}>
          <Icon name="folder" size={13} />{server.path}
        </button>
      </div>
    </div>

    <div class="stats">
      <div><span>{t("hdr.players")}</span><b>{live ? st.players.length : "—"}</b></div>
      <div title={tps?.mspt != null ? t("hdr.mspt", { v: tps.mspt.toFixed(1) }) : ""}>
        <span>{t("hdr.tps")}</span>
        <b class:tps-ok={tps && tps.tps >= 19.5} class:tps-mid={tps && tps.tps >= 15 && tps.tps < 19.5} class:tps-bad={tps && tps.tps < 15}>{tps ? tps.tps.toFixed(1) : "—"}</b>
      </div>
      <div><span>{t("hdr.ram")}</span><b>{metrics ? bytes(metrics.ram_bytes) : "—"}</b></div>
      <div><span>{t("hdr.cpu")}</span><b>{metrics ? `${Math.round(metrics.cpu / Math.max(1, metrics.cores))}%` : "—"}</b></div>
      <div><span>{t("hdr.uptime")}</span><b>{live ? fmtUptime(uptime) : "—"}</b></div>
    </div>

    <div class="actions">
      {#if !live}
        <button class="btn primary" disabled={!!busy} onclick={() => startServer(server.id)}>
          <Icon name="play" size={14} />{t("act.start")}
        </button>
      {:else}
        <button class="btn" disabled={st.status === "stopping"} onclick={() => api.restart(server.id).catch(fail)}>
          <Icon name="restart" size={15} />{t("act.restart")}
        </button>
        {#if st.status === "stopping"}
          <button class="btn danger solid" onclick={kill}><Icon name="skull" size={15} />{t("act.kill")}</button>
        {:else}
          <button class="btn danger" onclick={() => api.stop(server.id).catch(fail)}>
            <Icon name="stop" size={13} />{t("act.stop")}
          </button>
        {/if}
      {/if}
    </div>
  </header>

  {#if busy}
    <div class="progress">
      <div class="bar" style:width={`${Math.round((busy.progress?.percent ?? 0) * 100)}%`}></div>
      <span>
        {busy.phase === "restore" ? t("bk.restoring") : t("bk.progress", { p: Math.round((busy.progress?.percent ?? 0) * 100) })}
      </span>
    </div>
  {/if}

  <nav class="tabs">
    {#each tabs as tb (tb.id)}
      <button class:active={app.tab === tb.id} onclick={() => (app.tab = tb.id)}>
        <Icon name={tb.icon} size={15} />{t(`tab.${tb.id}` as Key)}
      </button>
    {/each}
  </nav>

  <div class="content">
    {#if app.tab === "console"}<Console {server} />
    {:else if app.tab === "diag"}<Diagnostics {server} />
    {:else if app.tab === "mods"}<Mods {server} />
    {:else if app.tab === "plugins"}<Plugins {server} />
    {:else if app.tab === "backups"}<Backups {server} />
    {:else if app.tab === "schedule"}<Schedule {server} />
    {:else if app.tab === "optimize"}<Optimize {server} />
    {:else if app.tab === "launch"}<Launch {server} />
    {:else if app.tab === "properties"}<Properties {server} />
    {/if}
  </div>
</section>

<style>
  .view { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .hdr {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: 18px;
    padding: 18px 22px 14px;
  }
  .core-badge { position: relative; width: 54px; height: 54px; display: grid; place-items: center; }
  .glyph {
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    border-radius: 12px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    font-weight: 700;
    font-size: 1.05em;
    color: var(--muted);
  }
  .ring { position: absolute; inset: 0; border-radius: 16px; border: 2px solid transparent; }
  .running .ring { border-color: var(--ok); animation: pulse calc(3.2s * var(--anim-k)) ease-out infinite; }
  .running .glyph { color: var(--ok); }
  .starting .ring, .stopping .ring { border-color: var(--warn); border-style: dashed; animation: spin calc(7s * var(--anim-k)) linear infinite; }
  .crashed .ring { border-color: var(--err); }
  .crashed .glyph { color: var(--err); }
  @keyframes pulse { 0% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--ok) 45%, transparent); } 70%, 100% { box-shadow: 0 0 0 9px transparent; } }
  @keyframes spin { to { transform: rotate(360deg); } }

  .ident { min-width: 0; display: grid; gap: 4px; }
  .ident h1 { font-size: 1.35em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub { font-size: 0.88em; gap: 14px; min-width: 0; white-space: nowrap; }
  .sub > span { flex: none; }
  .path {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .path:hover { color: var(--accent); }

  .stats { display: flex; gap: 18px; }
  .stats div { min-width: 3.2em; }
  .stats div { display: grid; text-align: right; }
  .stats span { font-size: 0.78em; color: var(--muted); }
  .stats b { font-weight: 600; font-variant-numeric: tabular-nums; }
  .tps-ok { color: var(--ok); }
  .tps-mid { color: var(--warn); }
  .tps-bad { color: var(--err); }
  .actions { display: flex; gap: 8px; }

  .progress { position: relative; margin: 0 22px 8px; height: 24px; border-radius: 6px; background: var(--surface-2); overflow: hidden; }
  .progress .bar { position: absolute; inset: 0 auto 0 0; background: var(--accent-soft); transition: width 0.25s; }
  .progress span { position: relative; display: block; padding: 3px 10px; font-size: 0.82em; color: var(--muted); }

  .tabs { display: flex; gap: 2px; padding: 0 16px; border-bottom: 1px solid var(--line); overflow-x: auto; scrollbar-width: none; }
  .tabs::-webkit-scrollbar { display: none; }
  .tabs button { flex: none; }
  .tabs button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 10px 12px;
    border: 0;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }
  .tabs button:hover { color: var(--text); }
  .tabs button.active { color: var(--text); border-bottom-color: var(--accent); }
  .content { flex: 1; min-height: 0; display: flex; flex-direction: column; }

  @media (max-width: 1180px) {
    .hdr { grid-template-columns: auto 1fr auto; }
    .stats { grid-column: 1 / -1; order: 4; justify-content: flex-start; }
    .stats div { text-align: left; }
  }
</style>
