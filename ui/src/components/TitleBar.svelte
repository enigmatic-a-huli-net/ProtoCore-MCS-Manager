<script lang="ts">
  // Своя полоса заголовка (окно без системной рамки). Тянуть окно можно за пустое место.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Icon from "./Icon.svelte";
  import { app, t } from "../lib/store.svelte";
  import type { Key } from "../lib/i18n";
  import { logo } from "../lib/brand";

  const win = getCurrentWindow();
</script>

<header class="tb" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <img src={logo()} alt="" width="18" height="18" />
    <span data-tauri-drag-region>ProtoCore <b>MCS</b> Manager</span>
  </div>
  {#if app.net?.enabled && app.net.last_min.grade}
    {@const g = app.net.last_10min.grade || app.net.last_min.grade}
    <span class="net {g}" title={`${t("net.quality")}: ${t(`net.${g}` as Key)} · ${t("net.loss")} ${app.net.last_10min.loss.toFixed(1)}% · ${t("net.ping")} ${Math.round(app.net.last_10min.avg_rtt)} мс`}>
      <span class="d"></span>{Math.round(app.net.last_min.avg_rtt)} мс{#if app.net.last_10min.loss >= 0.5} · {app.net.last_10min.loss.toFixed(1)}%{/if}
    </span>
  {/if}
  <button
    class="upd"
    class:has={app.update?.available}
    title={app.update?.available ? t("up.available", { v: app.update.latest }) : t("up.check")}
    onclick={() => (app.updateOpen = true)}
  >
    <Icon name="download" size={14} />
    {#if app.update?.available}<span>v{app.update.latest}</span>{/if}
  </button>
  <div class="ctl">
    <button title={t("tb.minimize")} onclick={() => win.minimize()}><Icon name="min" size={14} /></button>
    <button title={t("tb.maximize")} onclick={() => win.toggleMaximize()}><Icon name="max" size={12} /></button>
    <button class="close" title={t("tb.close")} onclick={() => win.close()}><Icon name="x" size={14} /></button>
  </div>
</header>

<style>
  .tb {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 36px;
    padding-left: 14px;
    background: var(--surface);
    border-bottom: 1px solid var(--line);
    user-select: none;
  }
  .brand { display: flex; align-items: center; gap: 9px; font-size: 0.88em; color: var(--muted); }
  .brand b { color: var(--text); font-weight: 600; }
  .net { margin-left: auto; display: inline-flex; align-items: center; gap: 6px; font-size: 0.8em; color: var(--muted); font-variant-numeric: tabular-nums; }
  .net .d { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); }
  .net.fair .d { background: var(--warn); }
  .net.bad .d { background: var(--err); }
  .net.bad { color: var(--err); }
  .net + .upd { margin-left: 8px; }
  .upd {
    margin-left: auto;
    margin-right: 6px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 9px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-size: 0.82em;
    cursor: pointer;
  }
  .upd:hover { background: var(--hover); color: var(--text); }
  .upd.has { background: var(--accent-soft); color: var(--accent); font-weight: 600; }
  .ctl { display: flex; height: 100%; }
  .ctl button {
    width: 46px;
    height: 100%;
    display: grid;
    place-items: center;
    border: 0;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .ctl button:hover { background: var(--hover); color: var(--text); }
  .ctl .close:hover { background: #e81123; color: #fff; }
</style>
