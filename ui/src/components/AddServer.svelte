<script lang="ts">
  import { open as pick } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Icon from "./Icon.svelte";
  import { api, CORE_LABEL, DOWNLOADABLE, type CoreKind, type Detection, type PackInfo } from "../lib/api";
  import { app, t, fail, refreshState, toast } from "../lib/store.svelte";

  let mode = $state<"existing" | "new" | "pack">("existing");

  // ---------- существующая папка
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

  // ---------- из сборки
  let packPath = $state("");
  let pack = $state<PackInfo | null>(null);
  let manual = $state<[string, string][]>([]);

  // ---------- новый сервер
  let core = $state<CoreKind>("neoforge");
  let mcList = $state<{ id: string; stable: boolean }[]>([]);
  let mc = $state("");
  let buildList = $state<{ id: string; label: string; stable: boolean }[]>([]);
  let build = $state("");
  let snapshots = $state(false);
  let newName = $state("");
  let parent = $state(app.config?.servers_root ?? "");
  let memGb = $state(8);
  let java = $state("");
  let javaInfo = $state("");
  let eula = $state(false);
  let loading = $state("");
  let creating = $state(false);

  async function loadVersions() {
    loading = "mc";
    mcList = [];
    buildList = [];
    mc = "";
    build = "";
    try {
      mcList = await api.coreVersions(core);
      mc = (mcList.find((v) => v.stable) ?? mcList[0])?.id ?? "";
      await loadBuilds();
    } catch (e) {
      fail(e);
    } finally {
      loading = "";
    }
  }

  async function loadBuilds() {
    if (!mc) return;
    loading = "build";
    buildList = [];
    build = "";
    try {
      buildList = await api.coreBuilds(core, mc);
    } catch (e) {
      fail(e);
    } finally {
      loading = "";
    }
  }

  async function checkJava() {
    try {
      javaInfo = await api.javaCheck(java);
    } catch (e) {
      javaInfo = String(e);
    }
  }

  async function pickParent() {
    const f = await pick({ directory: true });
    if (typeof f === "string") parent = f;
  }

  function sep() {
    return parent.includes("\\") || !parent.includes("/") ? "\\" : "/";
  }
  const folderName = $derived((newName.trim() || (mode === "pack" && pack ? pack.name : `${CORE_LABEL[core]}-${mc}`)).replace(/[\\/:*?"<>|]/g, "_"));
  const fullPath = $derived(parent ? `${parent.replace(/[\\/]+$/, "")}${sep()}${folderName}` : "");
  const shownVersions = $derived(snapshots ? mcList : mcList.filter((v) => v.stable));
  const needsInstaller = $derived(core === "forge" || core === "neoforge");

  async function create() {
    creating = true;
    app.createLog = [];
    app.createProgress = null;
    try {
      const s = await api.createServer({
        name: newName.trim() || folderName,
        dir: fullPath,
        core,
        mc,
        build,
        memory_mb: Math.round(memGb * 1024),
        java_path: java,
        accept_eula: eula,
      });
      await refreshState();
      app.view = "server";
      app.tab = "console";
      app.addOpen = false;
      toast(`${t("new.done")}: ${s.name}`, "ok");
    } catch (e) {
      fail(e);
    } finally {
      creating = false;
    }
  }

  async function choosePack() {
    const f = await pick({ filters: [{ name: "Modpack", extensions: ["mrpack", "zip"] }] });
    if (typeof f !== "string") return;
    packPath = f;
    try {
      pack = await api.packInspect(f);
      newName = pack.name;
      core = pack.core;
    } catch (e) {
      pack = null;
      fail(e);
    }
  }

  async function importPack() {
    if (!pack) return;
    creating = true;
    app.createLog = [];
    app.createProgress = null;
    manual = [];
    try {
      const r = await api.packImport({
        pack: packPath,
        name: newName.trim() || pack.name,
        dir: fullPath,
        memory_mb: Math.round(memGb * 1024),
        java_path: java,
        accept_eula: eula,
      });
      await refreshState();
      toast(t("pack.done", { d: r.result.downloaded }), "ok");
      if (r.result.failed.length) toast(t("pack.failed", { list: r.result.failed.slice(0, 8).join(", ") }), "error");
      if (r.result.manual.length) {
        manual = r.result.manual;
      } else {
        app.view = "server";
        app.tab = "console";
        app.addOpen = false;
      }
    } catch (e) {
      fail(e);
    } finally {
      creating = false;
    }
  }

  $effect(() => {
    if (mode === "new" && !mcList.length && !loading) loadVersions();
  });
  $effect(() => {
    if ((mode === "new" || mode === "pack") && needsInstaller && !javaInfo) checkJava();
  });

  const pct = $derived(app.createProgress?.total ? Math.round((app.createProgress.done / app.createProgress.total) * 100) : 0);
  const close = () => !creating && (app.addOpen = false);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="modal" class:wide={mode !== "existing"} role="dialog" aria-modal="true" aria-label={t("add.title")}>
    <div class="row">
      <h2 class="grow">{t("add.title")}</h2>
      <div class="seg">
        <button class:on={mode === "existing"} disabled={creating} onclick={() => (mode = "existing")}>{t("add.existing")}</button>
        <button class:on={mode === "new"} disabled={creating} onclick={() => (mode = "new")}>{t("add.new")}</button>
        <button class:on={mode === "pack"} disabled={creating} onclick={() => (mode = "pack")}>{t("add.pack")}</button>
      </div>
    </div>

    {#if mode === "existing"}
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
    {:else if mode === "pack"}
      <button class="drop" onclick={choosePack} disabled={creating}>
        <Icon name="archive" size={22} />
        {#if packPath}<span class="mono">{packPath}</span>{:else}<span>{t("pack.pick")}</span>{/if}
      </button>
      {#if pack}
        <div class="det">
          <div class="row wrap">
            <span class="chip ok"><Icon name="check" size={12} />{pack.format === "modrinth" ? "Modrinth" : "CurseForge"}</span>
            <b>{pack.name} {pack.version}</b>
            <span>{t("pack.info", { core: CORE_LABEL[pack.core], loader: pack.loader_version, mc: pack.mc })}</span>
          </div>
          <span class="muted small">{t("pack.files", { n: pack.files, s: pack.server_files, c: pack.client_only })}</span>
          {#if pack.needs_cf_key && !app.config?.curseforge_key}<span class="chip warn">{t("pack.cfKey")}</span>{/if}
        </div>
        <div class="grid">
          <label class="field"><span>{t("ln.name")}</span><input bind:value={newName} disabled={creating} /></label>
          <label class="field"><span>{t("new.memory")} — {memGb}</span><input type="range" min="2" max="32" step="1" bind:value={memGb} disabled={creating} /></label>
        </div>
        <label class="field">
          <span>{t("new.folder")}</span>
          <div class="row">
            <input bind:value={parent} placeholder="D:\Minecraft" disabled={creating} />
            <button class="btn" onclick={pickParent} disabled={creating}>{t("act.browse")}</button>
          </div>
          {#if fullPath}<span class="muted small mono">{fullPath}</span>{/if}
        </label>
        {#if needsInstaller}
          <label class="field">
            <span>{t("new.java")}</span>
            <div class="row">
              <input bind:value={java} placeholder="java" disabled={creating} onchange={checkJava} />
              <button class="btn" onclick={checkJava} disabled={creating}><Icon name="search" size={14} /></button>
            </div>
            <span class="muted small">{javaInfo ? t("new.javaOk", { v: javaInfo }) : t("new.installerNote")}</span>
          </label>
        {/if}
        <label class="check"><input type="checkbox" bind:checked={eula} disabled={creating} />{t("new.eula")}</label>
      {/if}

      {#if creating}
        <div class="log mono">{#each app.createLog.slice(-8) as l}<div>{l}</div>{/each}</div>
        {#if pct}<div class="bar"><div style:width={`${pct}%`}></div></div>{/if}
      {/if}
      {#if manual.length}
        <div class="manual">
          <span class="small">{t("pack.manual", { n: manual.length })}</span>
          {#each manual as [name, url]}
            <button class="link small" onclick={() => openUrl(url).catch(fail)}>{name}</button>
          {/each}
        </div>
      {/if}

      <div class="actions">
        <button class="btn ghost" disabled={creating} onclick={close}>{manual.length ? t("tb.close") : t("act.cancel")}</button>
        {#if !manual.length}
          <button class="btn primary" disabled={creating || !pack || !fullPath} onclick={importPack}>
            <Icon name="download" size={14} />{creating ? t("pack.importing") : t("pack.import")}
          </button>
        {/if}
      </div>
    {:else}
      <div class="cores">
        {#each DOWNLOADABLE as c}
          <button class:on={core === c} disabled={creating} onclick={() => { core = c; loadVersions(); }}>{CORE_LABEL[c]}</button>
        {/each}
      </div>

      <div class="grid">
        <label class="field">
          <span>{t("new.mc")}</span>
          <select bind:value={mc} onchange={loadBuilds} disabled={creating || loading === "mc"}>
            {#if loading === "mc"}<option>{t("new.loading")}</option>{/if}
            {#each shownVersions as v}<option value={v.id}>{v.id}</option>{/each}
          </select>
        </label>
        <label class="field">
          <span>{t("new.build")}</span>
          <select bind:value={build} disabled={creating || !!loading}>
            <option value="">{t("new.latest")}</option>
            {#each buildList.slice(0, 60) as b}<option value={b.id}>{b.label}{b.stable ? "" : " ⚠"}</option>{/each}
          </select>
        </label>
      </div>
      <label class="check small"><input type="checkbox" bind:checked={snapshots} />{t("new.snapshots")}</label>

      <div class="grid">
        <label class="field"><span>{t("ln.name")}</span><input bind:value={newName} placeholder={folderName} disabled={creating} /></label>
        <label class="field"><span>{t("new.memory")} — {memGb}</span><input type="range" min="2" max="32" step="1" bind:value={memGb} disabled={creating} /></label>
      </div>

      <label class="field">
        <span>{t("new.folder")}</span>
        <div class="row">
          <input bind:value={parent} placeholder="D:\Minecraft" disabled={creating} />
          <button class="btn" onclick={pickParent} disabled={creating}>{t("act.browse")}</button>
        </div>
        {#if fullPath}<span class="muted small mono">{fullPath}</span>{:else}<span class="muted small">{t("new.folderHint")}</span>{/if}
      </label>

      {#if needsInstaller}
        <label class="field">
          <span>{t("new.java")}</span>
          <div class="row">
            <input bind:value={java} placeholder="java" disabled={creating} onchange={checkJava} />
            <button class="btn" onclick={checkJava} disabled={creating}><Icon name="search" size={14} /></button>
          </div>
          <span class="muted small">{javaInfo ? t("new.javaOk", { v: javaInfo }) : t("new.installerNote")}</span>
        </label>
      {/if}

      <label class="check"><input type="checkbox" bind:checked={eula} disabled={creating} />{t("new.eula")}</label>

      {#if creating}
        <div class="log mono">
          {#each app.createLog.slice(-8) as l}<div>{l}</div>{/each}
        </div>
        {#if pct}<div class="bar"><div style:width={`${pct}%`}></div></div>{/if}
      {/if}

      <div class="actions">
        <button class="btn ghost" disabled={creating} onclick={close}>{t("act.cancel")}</button>
        <button class="btn primary" disabled={creating || !mc || !fullPath || !!loading} onclick={create}>
          <Icon name="plus" size={14} />{creating ? t("new.creating") : t("new.create")}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .wide { width: min(680px, calc(100vw - 40px)); }
  .seg { display: flex; padding: 3px; background: var(--surface-2); border-radius: calc(var(--radius) * 0.8); }
  .seg button { padding: 4px 12px; border: 0; border-radius: calc(var(--radius) * 0.6); background: none; color: var(--muted); font: inherit; font-size: 0.9em; cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.15); }
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
  .small { font-size: 0.88em; overflow-wrap: anywhere; }
  .cores { display: flex; flex-wrap: wrap; gap: 6px; }
  .cores button {
    padding: 6px 12px;
    border: 1px solid var(--line);
    border-radius: calc(var(--radius) * 0.8);
    background: var(--bg);
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }
  .cores button.on { border-color: var(--accent); background: var(--accent-soft); color: var(--accent); font-weight: 600; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  input[type="range"] { accent-color: var(--accent); height: auto; padding: 0; border: 0; background: none; }
  .log { padding: 8px 10px; background: var(--console); border: 1px solid var(--line); border-radius: 8px; font-size: 0.78em; max-height: 150px; overflow: hidden; }
  .log div { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .manual { display: grid; gap: 4px; padding: 10px 12px; border-radius: 8px; background: color-mix(in srgb, var(--warn) 12%, transparent); max-height: 160px; overflow: auto; }
  .link { padding: 0; border: 0; background: none; color: var(--accent); font: inherit; cursor: pointer; text-align: left; }
  .bar { height: 6px; border-radius: 6px; background: var(--surface-2); overflow: hidden; }
  .bar div { height: 100%; background: var(--accent); transition: width 0.2s; }
</style>
