<script lang="ts">
  import Icon from "../Icon.svelte";
  import { api, type BisectSettings, type ServerEntry } from "../../lib/api";
  import { app, getLog, logVersion, status, t, toast, fail } from "../../lib/store.svelte";
  import type { Key } from "../../lib/i18n";

  let { server }: { server: ServerEntry } = $props();

  let set = $state<BisectSettings>({
    mode: "batch",
    batch: 5,
    timeout_sec: 300,
    memory_mb: Math.min(server.launch.memory_mb, 8192),
    find_all: false,
    keep_copy: false,
  });
  let testId = $state(`${server.id}~test`);
  let showConsole = $state(true);
  let done = $state<Record<string, boolean>>({});

  api.bisectInfo(server.id).then((i) => {
    testId = i.test_id;
    if (!app.bisect[server.id]) app.bisect[server.id] = { running: i.running, status: "", steps: i.last?.steps ?? [], report: i.last };
  });

  const b = $derived(app.bisect[server.id]);
  const mainRunning = $derived(!["stopped", "crashed"].includes(status(server.id).status));
  const lines = $derived.by(() => {
    logVersion();
    return getLog(testId).slice(-14);
  });

  async function start() {
    app.bisect[server.id] = { running: true, status: "…", steps: [], report: null };
    try {
      await api.bisectStart(server.id, $state.snapshot(set));
    } catch (e) {
      app.bisect[server.id] = { running: false, status: "", steps: [], report: null };
      fail(e);
    }
  }

  async function quarantine(file: string, reason: string) {
    try {
      await api.quarantine(server.id, file, reason);
      done[file] = true;
      toast(t("dg.quarantined", { name: file }), "ok");
    } catch (e) {
      fail(e);
    }
  }

  const outKey = (k: string) => `bs.${k}` as Key;
</script>

<section class="panel bis">
  <div class="row">
    <Icon name="search" size={18} />
    <h3 class="grow">{t("bs.title")}</h3>
    {#if b?.running}
      <button class="btn sm danger" onclick={() => api.bisectCancel(server.id)}><Icon name="stop" size={12} />{t("bs.stop")}</button>
    {:else}
      <button class="btn sm primary" onclick={start}><Icon name="play" size={12} />{t("bs.start")}</button>
    {/if}
  </div>
  <p class="muted small">{t("bs.text")}</p>
  {#if mainRunning && !b?.running}<p class="small warnline">{t("bs.runningWarn")}</p>{/if}

  {#if !b?.running}
    <div class="grid">
      <label class="field">
        <span>{t("bs.mode")}</span>
        <select bind:value={set.mode}>
          <option value="batch">{t("bs.batch")}</option>
          <option value="binary">{t("bs.binary")}</option>
        </select>
      </label>
      {#if set.mode === "batch"}
        <label class="field"><span>{t("bs.size")}</span><input type="number" min="1" max="50" bind:value={set.batch} /></label>
      {/if}
      <label class="field"><span>{t("bs.timeout")}</span><input type="number" min="60" bind:value={set.timeout_sec} /></label>
      <label class="field"><span>{t("bs.memory")} — {(set.memory_mb / 1024).toFixed(0)}</span><input type="range" min="2048" max="16384" step="1024" bind:value={set.memory_mb} /></label>
    </div>
    <div class="row wrap">
      <label class="check"><input type="checkbox" bind:checked={set.find_all} />{t("bs.all")}</label>
      <label class="check"><input type="checkbox" bind:checked={set.keep_copy} />{t("bs.keep")}</label>
    </div>
  {/if}

  {#if b}
    {#if b.status}<p class="status"><span class="spin"></span>{b.status}</p>{/if}

    {#if b.report}
      <div class="result {b.report.culprits.length ? 'found' : b.report.baseline_failed || b.report.group.length ? 'bad' : ''}">
        <b>{b.report.culprits.length ? t("bs.found") : b.report.group.length ? t("bs.group") : ""}</b>
        <span>{b.report.message}</span>
        {#each b.report.culprits as c}
          <div class="c">
            <b>{c.name}</b>
            <span class="mono muted small">{c.file}</span>
            <span class="muted small grow">{c.reason}</span>
            {#if done[c.file]}
              <span class="chip ok"><Icon name="check" size={11} />{t("dg.inQuarantine")}</span>
            {:else}
              <button class="btn sm" onclick={() => quarantine(c.file, c.reason)}><Icon name="shield" size={13} />{t("dg.quarantine")}</button>
            {/if}
          </div>
        {/each}
        {#if b.report.group.length}<div class="small">{b.report.group.join(", ")}</div>{/if}
      </div>
    {/if}

    {#if b.steps.length}
      <div class="steps">
        <span class="muted small">{t("bs.steps")}: {b.steps.length}</span>
        {#each b.steps.slice().reverse().slice(0, 30) as s (s.n)}
          <div class="st">
            <span class="n muted">{s.n}</span>
            <span class="chip {s.outcome.kind === 'ok' ? 'ok' : 'err'}">{t(outKey(s.outcome.kind))}</span>
            <span class="grow">{s.label}</span>
            <span class="muted small">{s.mods} · {s.seconds} c</span>
          </div>
        {/each}
      </div>
    {/if}

    {#if b.running || lines.length}
      <button class="link small" onclick={() => (showConsole = !showConsole)}><Icon name="terminal" size={13} />{t("bs.console")}</button>
      {#if showConsole}
        <div class="con mono">{#each lines as l (l.seq)}<div class={l.level}>{l.text}</div>{/each}</div>
      {/if}
    {/if}
  {/if}
</section>

<style>
  .bis { padding: 14px 16px; display: grid; gap: 10px; }
  .small { font-size: 0.86em; margin: 0; }
  .warnline { color: var(--warn); }
  .grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; }
  input[type="range"] { accent-color: var(--accent); height: auto; padding: 0; border: 0; background: none; }
  .status { display: flex; align-items: center; gap: 8px; margin: 0; color: var(--accent); font-size: 0.92em; }
  .spin { width: 12px; height: 12px; border-radius: 50%; border: 2px solid var(--accent-line); border-top-color: var(--accent); animation: spin calc(1.4s * var(--anim-k)) linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .result { display: grid; gap: 6px; padding: 10px 12px; border-radius: 8px; background: var(--surface-2); }
  .result.found { background: color-mix(in srgb, var(--err) 10%, transparent); }
  .result.bad { background: color-mix(in srgb, var(--warn) 12%, transparent); }
  .c { display: flex; align-items: center; gap: 10px; }
  .steps { display: grid; gap: 3px; max-height: 220px; overflow: auto; }
  .st { display: flex; align-items: center; gap: 10px; font-size: 0.9em; }
  .n { width: 2em; text-align: right; font-variant-numeric: tabular-nums; }
  .link { display: inline-flex; align-items: center; gap: 6px; padding: 0; border: 0; background: none; color: var(--muted); font: inherit; cursor: pointer; width: max-content; }
  .con { padding: 8px 10px; background: var(--console); border: 1px solid var(--line); border-radius: 8px; font-size: 0.76em; max-height: 220px; overflow: auto; }
  .con div { white-space: pre-wrap; word-break: break-all; }
  .con .warn { color: var(--warn); }
  .con .error { color: var(--err); }
  .con .system { color: var(--accent); }
</style>
