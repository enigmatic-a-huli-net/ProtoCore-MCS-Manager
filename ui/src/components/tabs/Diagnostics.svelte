<script lang="ts">
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import Icon from "../Icon.svelte";
  import { api, type Analysis, type Culprit, type ServerEntry } from "../../lib/api";
  import { t, toast, fail } from "../../lib/store.svelte";
  import type { Key } from "../../lib/i18n";

  let { server }: { server: ServerEntry } = $props();

  type Source = "auto" | "crash" | "latest" | "console";
  let source = $state<Source>("auto");
  let result = $state<Analysis | null>(null);
  let error = $state("");
  let loading = $state(false);
  let openLine = $state<number | null>(null);
  let done = $state<Record<string, boolean>>({});

  async function run() {
    loading = true;
    error = "";
    try {
      result = await api.analyze(server.id, source);
    } catch (e) {
      result = null;
      error = String(e);
    } finally {
      loading = false;
    }
  }
  run();

  async function quarantine(c: Culprit, reason: string) {
    try {
      await api.quarantine(server.id, c.file, reason);
      done[c.file] = true;
      toast(t("dg.quarantined", { name: c.name }), "ok");
    } catch (e) {
      fail(e);
    }
  }

  async function copy() {
    if (!result) return;
    try {
      await navigator.clipboard.writeText(result.compressed);
      toast(t("dg.copied"), "ok");
    } catch (e) {
      fail(e);
    }
  }

  async function save() {
    if (!result) return;
    try {
      const p = await api.saveCompressed(server.id, result.compressed);
      toast(t("dg.savedTo", { p }), "ok");
      revealItemInDir(p).catch(() => {});
    } catch (e) {
      fail(e);
    }
  }

  const sources: Source[] = ["auto", "crash", "latest", "console"];
</script>

<div class="wrapx scroll">
  <div class="bar">
    <div class="seg">
      {#each sources as s}
        <button class:on={source === s} onclick={() => { source = s; run(); }}>{t(`dg.${s}` as Key)}</button>
      {/each}
    </div>
    <button class="btn sm" disabled={loading} onclick={run}><Icon name="restart" size={14} />{t("dg.run")}</button>
    {#if result}
      <span class="grow"></span>
      <div class="facts">
        <span>{t("dg.lines")} <b>{result.total_lines}</b></span>
        <span>{t("dg.errors")} <b class:bad={result.errors > 0}>{result.errors}</b></span>
        <span>{t("dg.warnings")} <b>{result.warnings}</b></span>
        <span>{t("dg.lag")} <b>{result.lag_warnings}</b></span>
      </div>
    {/if}
  </div>

  {#if error}
    <div class="empty">{error}</div>
  {:else if result}
    {#if result.source_path}<p class="muted mono small">{result.source_path}</p>{/if}

    {#if result.findings.length}
      <div class="findings">
        {#each result.findings as f, i (f.code + f.title)}
          <article class="f {f.severity}">
            <header>
              <Icon name="alert" size={15} />
              <h3>{f.title}</h3>
              {#if f.count > 1}<span class="chip">{t("dg.times", { n: f.count })}</span>{/if}
            </header>
            {#if f.detail}<p>{f.detail}</p>{/if}
            {#if f.hint}<p class="hint">{f.hint}</p>{/if}

            {#if f.culprits.length}
              <div class="culprits">
                <span class="muted small">{t("dg.culprits")}</span>
                {#each f.culprits as c (c.file)}
                  <div class="c">
                    <b>{c.name}</b>
                    <span class="chip {c.confidence === 'high' ? 'err' : c.confidence === 'medium' ? 'warn' : ''}">{t(`dg.conf.${c.confidence}` as Key)}</span>
                    {#if c.client_only}<span class="chip accent">{t("dg.client")}</span>{/if}
                    <span class="mono muted small">{c.file}</span>
                    <span class="grow"></span>
                    {#if f.code === "wrong_version" || f.code === "missing_dep"}
                      <!-- карантин тут не поможет: нужна другая версия / недостающий мод -->
                    {:else if c.enabled && !done[c.file]}
                      <button class="btn sm" onclick={() => quarantine(c, f.title)}>
                        <Icon name="shield" size={13} />{t("dg.quarantine")}
                      </button>
                    {:else}
                      <span class="chip ok"><Icon name="check" size={12} />{t("dg.inQuarantine")}</span>
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}

            {#if f.line}
              <button class="line mono" class:open={openLine === i} onclick={() => (openLine = openLine === i ? null : i)}>{f.line}</button>
            {/if}
          </article>
        {/each}
      </div>
    {:else}
      <div class="ok"><Icon name="check" size={16} />{t("dg.nothing")}</div>
    {/if}

    <section class="panel comp">
      <div class="row">
        <div class="grow">
          <h3>{t("dg.compressed")}</h3>
          <p class="muted small">{t("dg.compressedText")}</p>
        </div>
        <button class="btn sm" onclick={copy}><Icon name="copy" size={14} />{t("dg.copy")}</button>
        <button class="btn sm" onclick={save}><Icon name="download" size={14} />{t("dg.saveFile")}</button>
      </div>
      <pre>{result.compressed}</pre>
    </section>
  {:else if loading}
    <div class="empty">…</div>
  {/if}
</div>

<style>
  .wrapx { flex: 1; padding: 12px 16px 18px; display: grid; align-content: start; gap: 12px; }
  .bar { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .seg { display: flex; padding: 3px; background: var(--surface-2); border-radius: calc(var(--radius) * 0.8); }
  .seg button { padding: 4px 12px; border: 0; border-radius: calc(var(--radius) * 0.6); background: none; color: var(--muted); font: inherit; font-size: 0.9em; cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.15); }
  .facts { display: flex; gap: 16px; font-size: 0.88em; color: var(--muted); }
  .facts b { color: var(--text); font-variant-numeric: tabular-nums; }
  .facts b.bad { color: var(--err); }
  .small { font-size: 0.85em; }
  p { margin: 0; }

  .findings { display: grid; gap: 10px; }
  .f {
    padding: 12px 16px;
    display: grid;
    gap: 6px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-left: 3px solid var(--info);
    border-radius: var(--radius);
  }
  .f.error { border-left-color: var(--err); }
  .f.warn { border-left-color: var(--warn); }
  .f header { display: flex; align-items: center; gap: 8px; }
  .f.error header :global(svg) { color: var(--err); }
  .f.warn header :global(svg) { color: var(--warn); }
  .f h3 { font-size: 1em; }
  .f p { font-size: 0.92em; max-width: 90ch; }
  .hint { color: var(--muted); }
  .culprits { display: grid; gap: 4px; margin-top: 4px; }
  .c { display: flex; align-items: center; gap: 8px; padding: 5px 8px; border-radius: 8px; background: var(--bg); }
  .line {
    padding: 6px 8px;
    border: 0;
    border-radius: 6px;
    background: var(--console);
    color: var(--muted);
    font-size: 0.8em;
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .line.open { white-space: pre-wrap; word-break: break-all; color: var(--text); }
  .ok { display: flex; gap: 10px; align-items: center; padding: 12px 16px; border-radius: var(--radius); background: color-mix(in srgb, var(--ok) 10%, transparent); color: var(--ok); }

  .comp { padding: 14px 16px; display: grid; gap: 10px; }
  .comp pre {
    margin: 0;
    max-height: 420px;
    overflow: auto;
    padding: 10px 12px;
    background: var(--console);
    border: 1px solid var(--line);
    border-radius: 8px;
    font-family: var(--mono);
    font-size: 0.8em;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
</style>
