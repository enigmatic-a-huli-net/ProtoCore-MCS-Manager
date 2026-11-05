<script lang="ts">
  import { open as pick } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import { api, CORE_LABEL, type Detection } from "../lib/api";
  import { app, t, fail, refreshState, toast } from "../lib/store.svelte";

  let path = $state("");
  let name = $state("");
  let det = $state<Detection | null>(null);
  let busy = $state(false);

  async function choose() {
    const f = await pick({ directory: true, title: t("add.pick") });
    if (typeof f !== "string") return;
    path = f;
    name = f.split(/[\\/]/).filter(Boolean).pop() ?? "";
    try {
      det = await api.detect(f);
    } catch (e) {
      fail(e);
    }
  }

  async function add() {
    busy = true;
    try {
      const s = await api.addServer(path, name);
      await refreshState();
      app.view = "server";
      app.tab = "console";
      app.addOpen = false;
      toast(`${s.name}: ${CORE_LABEL[s.core]} ${s.mc_version}`, "ok");
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }

  function close() {
    app.addOpen = false;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="modal" role="dialog" aria-modal="true" aria-label={t("add.title")}>
    <h2>{t("add.title")}</h2>

    <button class="drop" onclick={choose}>
      <Icon name="folder" size={22} />
      {#if path}<span class="mono">{path}</span>{:else}<span>{t("add.pick")}</span>{/if}
    </button>

    {#if det}
      <div class="det">
        {#if det.core !== "unknown"}
          <div class="row wrap">
            <span class="chip ok"><Icon name="check" size={12} />{t("add.detected")}</span>
            <b>{CORE_LABEL[det.core]} {det.loader_version}</b>
            <span>Minecraft {det.mc_version || "?"}</span>
            <span class="muted small">({det.source})</span>
          </div>
        {:else}
          <p class="muted">{t("add.notDetected")}</p>
        {/if}
        <div class="row wrap small">
          {#if det.has_mods_dir}<span class="chip">mods/</span>{/if}
          {#if det.has_plugins_dir}<span class="chip">plugins/</span>{/if}
          {#if det.scripts.length}<span class="muted">{t("add.scripts")}: {det.scripts.join(", ")}</span>{/if}
          {#if !det.eula_accepted}<span class="chip warn">{t("add.eulaNo")}</span>{/if}
        </div>
      </div>
      <label class="field"><span>{t("add.name")}</span><input bind:value={name} /></label>
    {/if}

    <div class="actions">
      <button class="btn ghost" onclick={close}>{t("act.cancel")}</button>
      <button class="btn primary" disabled={!path || busy} onclick={add}>{t("add.add")}</button>
    </div>
  </div>
</div>

<style>
  .drop {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 18px;
    border: 1.5px dashed var(--line);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--muted);
    font: inherit;
    text-align: left;
    cursor: pointer;
    word-break: break-all;
  }
  .drop:hover { border-color: var(--accent); color: var(--text); }
  .det { display: grid; gap: 8px; }
  .det p { margin: 0; }
  .small { font-size: 0.88em; }
</style>
