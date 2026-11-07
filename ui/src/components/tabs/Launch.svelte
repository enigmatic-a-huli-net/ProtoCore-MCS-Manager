<script lang="ts">
  import { open as pick, ask } from "@tauri-apps/plugin-dialog";
  import Icon from "../Icon.svelte";
  import Hint from "../Hint.svelte";
  import { api, CORE_LABEL, DOWNLOADABLE, supportsMods, type CoreKind, type JavaInstall, type ServerEntry } from "../../lib/api";
  import { app, status, t, toast, fail, saveServer, refreshState } from "../../lib/store.svelte";

  let { server }: { server: ServerEntry } = $props();

  // Работаем с копией, сохраняем по кнопке
  let s = $state<ServerEntry>(structuredClone($state.snapshot(server)) as ServerEntry);
  let extraClient = $state(s.mods.extra_client_ids.join(", "));
  let never = $state(s.mods.never_disable.join(", "));
  let preview = $state("");
  let previewErr = $state("");
  let scripts = $state<string[]>([]);

  async function loadPreview() {
    try {
      preview = await api.launchPreview(server.id);
      previewErr = "";
    } catch (e) {
      preview = "";
      previewErr = String(e);
    }
  }
  loadPreview();

  // ---------- Java
  let javas = $state<JavaInstall[]>([]);
  let need = $state<{ required: number | null; mc_version: string } | null>(null);
  let jdl = $state(false);
  async function loadJava() {
    try {
      [javas, need] = await Promise.all([api.javaList(), api.javaRequired(server.id)]);
    } catch (e) {
      fail(e);
    }
  }
  loadJava();
  const req = $derived(need?.required ?? null);
  const autoPick = $derived.by(() => {
    if (!req) return null;
    const exact = javas.filter((j) => j.major === req);
    if (exact.length) return exact[0];
    return req === 8 ? null : (javas.filter((j) => j.major > req).sort((a, b) => a.major - b.major)[0] ?? null);
  });
  const chosen = $derived(s.launch.java_path.trim() ? javas.find((j) => j.path === s.launch.java_path.trim()) : autoPick);
  const javaBad = $derived(!!req && !!chosen && (chosen.major < req || (req === 8 && chosen.major > 8)));
  async function downloadJava() {
    if (!req) return;
    jdl = true;
    app.javaProgress = null;
    try {
      const j = await api.javaDownload(req);
      toast(t("jv.downloaded", { v: j.version }), "ok");
      if (s.launch.java_path.trim()) s.launch.java_path = j.path;
      await loadJava();
      loadPreview();
    } catch (e) {
      fail(e);
    } finally {
      jdl = false;
      app.javaProgress = null;
    }
  }
  const srcLabel = (x: string) => t(`jv.src.${x}` as any) as string;

  api.detect(server.path).then((d) => (scripts = d.scripts)).catch(() => {});

  const ids = (x: string) => x.split(",").map((v) => v.trim()).filter(Boolean);

  async function save() {
    try {
      s.mods.extra_client_ids = ids(extraClient);
      s.mods.never_disable = ids(never);
      s.launch.memory_mb = Math.max(512, Number(s.launch.memory_mb) || 4096);
      s = (await saveServer(s)) as ServerEntry;
      toast(t("ln.saved"), "ok");
      loadPreview();
    } catch (e) {
      fail(e);
    }
  }

  async function redetect() {
    try {
      const [srv] = await api.redetect(server.id);
      s.core = srv.core;
      s.mc_version = srv.mc_version;
      s.loader_version = srv.loader_version;
      await refreshState();
      loadPreview();
    } catch (e) {
      fail(e);
    }
  }

  async function chooseScript() {
    const f = await pick({ defaultPath: server.path, filters: [{ name: "Script", extensions: ["bat", "cmd", "sh"] }] });
    if (typeof f === "string") s.launch.script = f;
  }
  async function chooseJava() {
    const f = await pick({ filters: [{ name: "Java", extensions: ["exe", ""] }] });
    if (typeof f === "string") s.launch.java_path = f;
  }

  async function remove() {
    if (!(await ask(t("confirm.remove", { name: server.name }), { kind: "warning" }))) return;
    try {
      await api.removeServer(server.id);
      await refreshState();
    } catch (e) {
      fail(e);
    }
  }

  // ---------- обновление ядра
  let coreBuilds = $state<{ id: string; label: string; stable: boolean }[] | null>(null);
  let coreBuild = $state("");
  let coreBusy = $state(false);
  const running = $derived(!["stopped", "crashed"].includes(status(server.id).status));
  const canUpdate = $derived(DOWNLOADABLE.includes(s.core) && !!s.mc_version);

  async function loadCoreBuilds() {
    try {
      coreBuilds = await api.coreBuilds(s.core, s.mc_version);
      coreBuild = "";
    } catch (e) {
      fail(e);
    }
  }

  async function updateCore() {
    coreBusy = true;
    app.createLog = [];
    app.createProgress = null;
    try {
      const srv = await api.updateCore(server.id, coreBuild);
      s.loader_version = srv.loader_version;
      s.launch.jar_override = srv.launch.jar_override;
      await refreshState();
      toast(`${t("core.done")}: ${CORE_LABEL[srv.core]} ${srv.loader_version}`, "ok");
      loadPreview();
    } catch (e) {
      fail(e);
    } finally {
      coreBusy = false;
    }
  }

  const CORES: CoreKind[] = ["vanilla", "paper", "purpur", "folia", "spigot", "fabric", "quilt", "forge", "neoforge", "youer", "mohist", "arclight"];
  const memGb = $derived((s.launch.memory_mb / 1024).toFixed(1));
</script>

<form class="wrapx scroll" onsubmit={(e) => { e.preventDefault(); save(); }}>
  <section class="panel">
    <h3>{t("ln.general")}</h3>
    <div class="grid2">
      <label class="field"><span>{t("ln.name")}</span><input bind:value={s.name} /></label>
      <label class="field"><span>{t("ln.path")}</span><input value={s.path} readonly /></label>
      <label class="field">
        <span>{t("ln.core")}</span>
        <select bind:value={s.core} onchange={() => (s.core_override = true)}>
          {#each CORES as c}<option value={c}>{CORE_LABEL[c]}</option>{/each}
          <option value="unknown">?</option>
        </select>
      </label>
      <label class="field"><span>{t("ln.mc")}</span><input bind:value={s.mc_version} /></label>
    </div>
    <div class="row">
      <label class="check"><input type="checkbox" checked={!s.core_override} onchange={(e) => (s.core_override = !e.currentTarget.checked)} />{t("ln.coreAuto")}</label>
      <span class="grow"></span>
      <button type="button" class="btn sm" onclick={redetect}><Icon name="search" size={14} />{t("ln.redetect")}</button>
    </div>
  </section>

  <section class="panel">
    <h3>{t("core.title")}</h3>
    <div class="row wrap">
      <span>{t("core.current", { v: `${CORE_LABEL[s.core]} ${s.loader_version || ""} · Minecraft ${s.mc_version || "?"}` })}</span>
      <span class="grow"></span>
      {#if !canUpdate}
        <span class="muted small">{t("core.unsupported")}</span>
      {:else if coreBuilds === null}
        <button type="button" class="btn sm" onclick={loadCoreBuilds}>{t("core.load")}</button>
      {:else}
        <select bind:value={coreBuild} style="width:auto" disabled={coreBusy}>
          <option value="">{t("new.latest")}</option>
          {#each coreBuilds.slice(0, 50) as b}<option value={b.id}>{b.label}{b.stable ? "" : " ⚠"}</option>{/each}
        </select>
        <button type="button" class="btn sm primary" disabled={coreBusy || running} onclick={updateCore} title={running ? t("core.stopFirst") : ""}>
          <Icon name="download" size={14} />{coreBusy ? t("core.updating") : t("core.update")}
        </button>
      {/if}
    </div>
    {#if canUpdate}<p class="muted small" style="margin:0">{running ? t("core.stopFirst") : t("core.note")}</p>{/if}
    {#if coreBusy}
      <div class="corelog mono">{#each app.createLog.slice(-6) as l}<div>{l}</div>{/each}</div>
    {/if}
  </section>

  <section class="panel">
    <h3>{t("ln.mode")}</h3>
    <div class="modes">
      <label class:on={s.launch.mode === "auto"}><input type="radio" bind:group={s.launch.mode} value="auto" />{t("ln.auto")}</label>
      <label class:on={s.launch.mode === "script"}><input type="radio" bind:group={s.launch.mode} value="script" />{t("ln.script")}</label>
    </div>

    {#if s.launch.mode === "script"}
      <label class="field">
        <span>{t("ln.scriptPath")}</span>
        <div class="row">
          <input bind:value={s.launch.script} list="scripts" />
          <button type="button" class="btn" onclick={chooseScript}>{t("act.browse")}</button>
        </div>
        <datalist id="scripts">{#each scripts as x}<option value={x}></option>{/each}</datalist>
      </label>
    {:else}
      <div class="grid2">
        <label class="field">
          <span>{t("ln.java")}<Hint text={t("jv.hint")} /></span>
          <div class="row">
            <select bind:value={s.launch.java_path} class="grow">
              <option value="">{autoPick ? t("jv.autoIs", { v: autoPick.major }) : t("jv.auto")}</option>
              {#each javas as j (j.path)}
                <option value={j.path}>Java {j.major} · {j.vendor || "?"} · {srcLabel(j.source)}</option>
              {/each}
              {#if s.launch.java_path && !javas.some((j) => j.path === s.launch.java_path)}
                <option value={s.launch.java_path}>{s.launch.java_path}</option>
              {/if}
            </select>
            <button type="button" class="btn" onclick={chooseJava}>{t("act.browse")}</button>
          </div>
          {#if req}
            <span class="jv" class:bad={javaBad || !chosen}>
              {#if javaBad}{t("jv.wrong", { mc: need?.mc_version ?? "", req, have: chosen?.major ?? 0 })}
              {:else if !chosen}{t("jv.missing", { mc: need?.mc_version ?? "", req })}
              {:else}<Icon name="check" size={12} />{t("jv.ok", { mc: need?.mc_version ?? "", req })}{/if}
              {#if javaBad || !chosen}
                <button type="button" class="btn sm" disabled={jdl} onclick={downloadJava}>
                  <Icon name="download" size={13} />{jdl ? (app.javaProgress?.total ? `${Math.round((app.javaProgress.done / app.javaProgress.total) * 100)}%` : "…") : t("jv.download", { v: req })}
                </button>
              {/if}
            </span>
          {/if}
        </label>
        <label class="field">
          <span>{t("ln.memory")} — {memGb} GB<Hint text={t("hn.memory")} /></span>
          <div class="row">
            <input type="range" min="1024" max="32768" step="512" bind:value={s.launch.memory_mb} class="grow" />
            <input type="number" min="512" step="512" bind:value={s.launch.memory_mb} style="width:7em" />
          </div>
        </label>
        <label class="field">
          <span>{t("ln.preset")}<Hint text={t("hn.preset")} /></span>
          <select bind:value={s.launch.jvm_preset}>
            <option value="aikar">{t("ln.aikar")}</option>
            <option value="zgc">{t("ln.zgc")}</option>
            <option value="none">{t("ln.none")}</option>
          </select>
        </label>
        <label class="field"><span>{t("ln.args")}</span><input bind:value={s.launch.server_args} /></label>
        <label class="field"><span>{t("ln.extra")}</span><input bind:value={s.launch.extra_jvm_args} placeholder="-Dlog4j2.formatMsgNoLookups=true" /></label>
        <label class="field"><span>{t("ln.jar")}</span><input bind:value={s.launch.jar_override} /></label>
      </div>
    {/if}

    <div class="grid2">
      <label class="field"><span>{t("ln.stopTimeout")}</span><input type="number" min="10" bind:value={s.launch.stop_timeout_sec} /></label>
    </div>
    <label class="check"><input type="checkbox" bind:checked={s.auto_restart} />{t("ln.autoRestart")}</label>

    <div class="preview">
      <span>{t("ln.preview")}</span>
      {#if preview}<code>{preview}</code>{:else}<code class="bad">{previewErr}</code>{/if}
    </div>
  </section>

  {#if supportsMods(s.core) || s.core === "unknown"}
    <section class="panel">
      <h3>{t("ln.modsBlock")}</h3>
      <label class="check"><input type="checkbox" bind:checked={s.mods.auto_disable_client} />{t("ln.autoClient")}<Hint text={t("hn.client")} /></label>
      <label class="check indent"><input type="checkbox" bind:checked={s.mods.disable_blacklisted} disabled={!s.mods.auto_disable_client} />{t("ln.autoClientLikely")}</label>
      <div class="grid2">
        <label class="field"><span>{t("ln.extraClient")}</span><input bind:value={extraClient} placeholder="mymod, othermod" /></label>
        <label class="field"><span>{t("ln.neverDisable")}</span><input bind:value={never} placeholder="journeymap" /></label>
      </div>
      <h3 class="sub">{t("ln.quarantine")}</h3>
      <label class="check"><input type="checkbox" bind:checked={s.mods.auto_quarantine} />{t("ln.qAuto")}<Hint text={t("hn.quarantine")} /></label>
      {#if s.mods.auto_quarantine}
        <label class="check indent"><input type="checkbox" bind:checked={s.mods.quarantine_restart} />{t("ln.qRestart")}</label>
        <label class="check indent"><input type="checkbox" bind:checked={s.mods.quarantine_content} />{t("ln.qContent")}</label>
        {#if s.mods.quarantine_content}<p class="warnline indent">{t("ln.qContentWarn")}</p>{/if}
        <label class="field indent narrow"><span>{t("ln.qRounds")}</span><input type="number" min="1" max="20" bind:value={s.mods.quarantine_max_rounds} /></label>
      {/if}
    </section>
  {/if}

  <div class="actions">
    <button type="submit" class="btn primary">{t("act.save")}</button>
    <span class="grow"></span>
    <button type="button" class="btn danger" onclick={remove}><Icon name="trash" size={14} />{t("act.remove")}</button>
  </div>
</form>

<style>
  .jv { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; font-size: 0.85em; color: var(--ok); }
  .jv.bad { color: var(--warn); }
  .wrapx { flex: 1; padding: 14px 16px 20px; display: grid; align-content: start; gap: 14px; max-width: 920px; }
  section { padding: 16px 18px; display: grid; gap: 12px; }
  .grid2 { display: grid; grid-template-columns: 1fr 1fr; gap: 12px 16px; }
  .modes { display: flex; gap: 8px; }
  .modes label {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 8px 14px;
    border: 1px solid var(--line);
    border-radius: calc(var(--radius) * 0.8);
    cursor: pointer;
  }
  .modes label.on { border-color: var(--accent); background: var(--accent-soft); }
  .modes input { accent-color: var(--accent); }
  .indent { padding-left: 1.7em; }
  .sub { margin-top: 6px; }
  .narrow { max-width: 14em; }
  .warnline { margin: 0; color: var(--warn); font-size: 0.88em; max-width: 70ch; }
  .preview { display: grid; gap: 4px; }
  .preview span { font-size: 0.85em; color: var(--muted); }
  .preview code {
    display: block;
    padding: 8px 10px;
    background: var(--console);
    border: 1px solid var(--line);
    border-radius: 8px;
    font-family: var(--mono);
    font-size: 0.8em;
    white-space: pre-wrap;
    word-break: break-all;
    user-select: text;
  }
  .preview code.bad { color: var(--err); }
  .actions { display: flex; gap: 10px; }
  .small { font-size: 0.88em; }
  .corelog { padding: 8px 10px; background: var(--console); border: 1px solid var(--line); border-radius: 8px; font-size: 0.78em; }
  .corelog div { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  input[type="range"] { accent-color: var(--accent); height: auto; padding: 0; border: 0; background: none; }
</style>
