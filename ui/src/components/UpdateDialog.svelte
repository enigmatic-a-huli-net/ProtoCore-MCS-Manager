<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Icon from "./Icon.svelte";
  import { api, bytes } from "../lib/api";
  import { app, t, fail } from "../lib/store.svelte";

  let checking = $state(true);
  let installing = $state(false);
  let error = $state("");

  // всегда проверяем заново при открытии
  api
    .checkUpdate()
    .then((u) => (app.update = u))
    .catch((e) => (error = String(e)))
    .finally(() => (checking = false));

  async function install() {
    installing = true;
    app.updateProgress = null;
    try {
      await api.installUpdate(); // после этого менеджер закроется сам
    } catch (e) {
      installing = false;
      fail(e);
    }
  }

  const pct = $derived(
    app.updateProgress && app.updateProgress.total ? Math.round((app.updateProgress.done / app.updateProgress.total) * 100) : 0,
  );
  const close = () => !installing && (app.updateOpen = false);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="modal" role="dialog" aria-modal="true" aria-label={t("up.title")}>
    <h2>{t("up.title")}</h2>

    {#if checking}
      <p class="muted">{t("up.checking")}</p>
    {:else if error}
      <p class="err">{error}</p>
    {:else if app.update}
      <div class="row wrap">
        <span class="chip">{t("up.current", { v: app.update.current })}</span>
        <span class="chip {app.update.available ? 'accent' : 'ok'}">{t("up.latest", { v: app.update.latest || "—" })}</span>
        {#if app.update.asset_size}<span class="muted small">{bytes(app.update.asset_size)}</span>{/if}
      </div>
      {#if !app.update.available}
        <p class="muted">{t("up.none")}</p>
      {:else}
        {#if app.update.notes}
          <div class="notes">
            <h3>{t("up.notes")}</h3>
            <pre>{app.update.notes}</pre>
          </div>
        {/if}
        <p class="muted small">{t("up.note")}</p>
        {#if installing}
          <div class="bar"><div style:width={`${pct}%`}></div></div>
          <span class="muted small">{t("up.downloading", { p: pct })}</span>
        {/if}
      {/if}
    {/if}

    <div class="actions">
      {#if app.update?.html_url}
        <button class="btn ghost" onclick={() => openUrl(app.update!.html_url).catch(fail)}>{t("up.page")}</button>
      {/if}
      <span class="grow"></span>
      <button class="btn" disabled={installing} onclick={close}>{t("act.cancel")}</button>
      {#if app.update?.available}
        <button class="btn primary" disabled={installing} onclick={install}>
          <Icon name="download" size={14} />{t("up.install")}
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  p { margin: 0; }
  .err { color: var(--err); }
  .small { font-size: 0.88em; }
  .notes h3 { font-size: 0.9em; color: var(--muted); margin-bottom: 6px; }
  .notes pre {
    margin: 0;
    max-height: 220px;
    overflow: auto;
    padding: 10px 12px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 8px;
    font-family: var(--font);
    font-size: 0.9em;
    white-space: pre-wrap;
  }
  .bar { height: 6px; border-radius: 6px; background: var(--surface-2); overflow: hidden; }
  .bar div { height: 100%; background: var(--accent); transition: width 0.2s; }
</style>
