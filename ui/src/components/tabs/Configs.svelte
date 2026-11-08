<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import Icon from "../Icon.svelte";
  import { api, bytes, type CfgEntry, type CfgFile, type CfgLoaded, type ServerEntry } from "../../lib/api";
  import { status, t, toast, fail } from "../../lib/store.svelte";
  import type { Key } from "../../lib/i18n";

  let { server }: { server: ServerEntry } = $props();

  let files = $state<CfgFile[]>([]);
  let fq = $state("");
  let cur = $state<CfgLoaded | null>(null);
  let edits = $state<Record<string, string>>({});
  let mode = $state<"form" | "text">("form");
  let text = $state("");
  let q = $state("");
  let onlyHints = $state(false);
  let saving = $state(false);
  let loading = $state(false);

  async function loadList() {
    try {
      files = await api.cfgList(server.id);
      if (!cur && files.length) open(files[0].rel);
    } catch (e) {
      fail(e);
    }
  }
  loadList();

  const dirty = $derived(mode === "form" ? Object.keys(edits).length > 0 : !!cur && text !== cur.text);

  async function open(rel: string) {
    if (dirty && !(await ask(t("cf.discard"), { kind: "warning" }))) return;
    loading = true;
    try {
      cur = await api.cfgLoad(server.id, rel);
      text = cur.text;
      edits = {};
      q = "";
    } catch (e) {
      fail(e);
    } finally {
      loading = false;
    }
  }

  function setVal(e: CfgEntry, v: string) {
    if (v === e.value) delete edits[e.path];
    else edits[e.path] = v;
  }
  const val = (e: CfgEntry) => edits[e.path] ?? e.value;

  async function save() {
    if (!cur) return;
    saving = true;
    try {
      if (mode === "form") {
        cur = await api.cfgSave(server.id, cur.rel, Object.entries(edits).map(([path, value]) => ({ path, value })));
      } else {
        cur = await api.cfgSaveText(server.id, cur.rel, text);
      }
      text = cur.text;
      edits = {};
      toast(status(server.id).status === "stopped" ? t("cf.saved") : t("cf.savedRestart"), "ok");
    } catch (e) {
      fail(e);
    } finally {
      saving = false;
    }
  }

  async function switchMode(m: "form" | "text") {
    if (m === mode || !cur) return;
    if (dirty && !(await ask(t("cf.discard"), { kind: "warning" }))) return;
    edits = {};
    text = cur.text;
    mode = m;
  }

  // ---------- server.properties: рекомендации для нестабильного канала
  const RECOMMENDED: Record<string, string> = { "view-distance": "10", "simulation-distance": "6", "network-compression-threshold": "256" };
  function recommend() {
    for (const e of cur?.entries ?? []) if (RECOMMENDED[e.key] !== undefined) setVal(e, RECOMMENDED[e.key]);
  }

  // ---------- список файлов
  const groups = $derived.by(() => {
    const f = files.filter((x) => !fq || x.rel.toLowerCase().includes(fq.toLowerCase()) || x.title.toLowerCase().includes(fq.toLowerCase()));
    return (["core", "mods", "plugins"] as const).map((g) => ({ g, items: f.filter((x) => x.group === g) })).filter((x) => x.items.length);
  });
  const short = (f: CfgFile) => (f.group === "core" ? f.title : f.rel.replace(/^(config|plugins|defaultconfigs)\//, ""));

  // ---------- записи: фильтр и группировка по разделам
  const shown = $derived.by(() => {
    const ql = q.toLowerCase();
    return (cur?.entries ?? []).filter(
      (e) => (!onlyHints || e.hint) && (!ql || e.path.toLowerCase().includes(ql) || e.hint.toLowerCase().includes(ql) || e.comment.toLowerCase().includes(ql)),
    );
  });
  const sections = $derived.by(() => {
    const out: { name: string; items: CfgEntry[] }[] = [];
    for (const e of shown) {
      const name = e.path.slice(0, Math.max(0, e.path.length - e.key.length - 1));
      const last = out[out.length - 1];
      if (last && last.name === name) last.items.push(e);
      else out.push({ name, items: [e] });
    }
    return out;
  });
  const hintCount = $derived(cur?.entries.filter((e) => e.hint).length ?? 0);

  // варианты из комментария модов: «Allowed Values: A, B, C»
  const PROP_OPTIONS: Record<string, string[]> = {
    difficulty: ["peaceful", "easy", "normal", "hard"],
    gamemode: ["survival", "creative", "adventure", "spectator"],
    "level-type": ["minecraft:normal", "minecraft:flat", "minecraft:large_biomes", "minecraft:amplified", "minecraft:single_biome_surface"],
    "op-permission-level": ["1", "2", "3", "4"],
    "function-permission-level": ["1", "2", "3", "4"],
    "redstone-implementation": ["VANILLA", "EIGENCRAFT", "ALTERNATE_CURRENT"],
  };
  function options(e: CfgEntry): string[] | null {
    if (cur?.rel === "server.properties" || cur?.rel.endsWith("paper-world-defaults.yml")) {
      const o = PROP_OPTIONS[e.key];
      if (o) return o;
    }
    const m = e.comment.match(/Allowed Values:\s*(.+)/i);
    if (!m) return null;
    const o = m[1].split(",").map((x) => x.trim()).filter(Boolean);
    return o.length > 1 && o.length < 30 ? o : null;
  }
  function range(e: CfgEntry): string {
    return e.comment.match(/Range:\s*([^\n]+)/i)?.[1] ?? "";
  }
  // комментарий без служебных строк
  const cleanComment = (c: string) =>
    c
      .split("\n")
      .map((l) => l.replace(/^[\s.]+/, "").trim())
      .filter((l) => l && !/^(Range|Allowed Values|Default):/i.test(l))
      .join(" ");
</script>

<div class="layout">
  <aside class="files scroll">
    <div class="search">
      <Icon name="search" size={13} />
      <input placeholder={t("cf.findFile")} bind:value={fq} />
    </div>
    {#each groups as gr (gr.g)}
      <div class="ghead">{t(`cf.group.${gr.g}` as Key)}</div>
      {#each gr.items as f (f.rel)}
        <button class="file" class:active={cur?.rel === f.rel} onclick={() => open(f.rel)} title={f.rel}>
          <span class="fname">{short(f)}</span>
          {#if f.group === "core" && f.title !== f.rel}<span class="frel mono">{f.rel}</span>{/if}
        </button>
      {/each}
    {:else}
      <p class="muted small pad">{t("cf.none")}</p>
    {/each}
  </aside>

  <section class="main">
    {#if cur}
      <div class="head">
        <div class="grow minw">
          <h3 class="mono">{cur.rel}</h3>
          <p class="muted small">{t("cf.note")}</p>
        </div>
        <div class="seg">
          <button class:on={mode === "form"} onclick={() => switchMode("form")}>{t("cf.form")}</button>
          <button class:on={mode === "text"} onclick={() => switchMode("text")}>{t("cf.text")}</button>
        </div>
        <button class="btn primary sm" disabled={!dirty || saving} onclick={save}>
          {saving ? "…" : mode === "form" && Object.keys(edits).length ? t("cf.saveN", { n: Object.keys(edits).length }) : t("act.save")}
        </button>
      </div>

      {#if mode === "form"}
        <div class="bar">
          <div class="search wide">
            <Icon name="search" size={13} />
            <input placeholder={t("cf.find")} bind:value={q} />
          </div>
          {#if hintCount}
            <label class="check small"><input type="checkbox" bind:checked={onlyHints} />{t("cf.onlyHints", { n: hintCount })}</label>
          {/if}
          {#if cur.rel === "server.properties"}
            <span class="grow"></span>
            <button class="btn ghost sm" onclick={recommend} title={t("pr.recommendText")}>{t("pr.recommend")}</button>
          {/if}
        </div>
        <div class="entries scroll" class:dim={loading}>
          {#each sections as sec, si (si + sec.name)}
            {#if sec.name}<div class="sec mono">{sec.name}</div>{/if}
            {#each sec.items as e (e.path)}
              {@const opts = options(e)}
              {@const changed = edits[e.path] !== undefined}
              <div class="row1" class:changed class:ro={!e.editable}>
                <div class="k">
                  <span class="key mono">{e.key}</span>
                  {#if e.hint}
                    <span class="hint">{e.hint}</span>
                  {:else if cleanComment(e.comment)}
                    <span class="hint muted" title={e.comment}>{cleanComment(e.comment)}</span>
                  {/if}
                </div>
                <div class="v">
                  {#if !e.editable}
                    <span class="raw mono" title={t("cf.rawOnly")}>{e.value}</span>
                  {:else if e.kind === "bool"}
                    <label class="switch"><input type="checkbox" checked={val(e) === "true"} onchange={(ev) => setVal(e, (ev.currentTarget as HTMLInputElement).checked ? "true" : "false")} /><span></span></label>
                  {:else if opts}
                    <select value={val(e)} onchange={(ev) => setVal(e, (ev.currentTarget as HTMLSelectElement).value)}>
                      {#each opts.includes(val(e)) ? opts : [val(e), ...opts] as o}<option value={o}>{o}</option>{/each}
                    </select>
                  {:else if e.kind === "int" || e.kind === "float"}
                    <input class="num" type="text" inputmode="decimal" value={val(e)} oninput={(ev) => setVal(e, (ev.currentTarget as HTMLInputElement).value)} />
                    {#if range(e)}<span class="muted small">{range(e)}</span>{/if}
                  {:else}
                    <input class="txt" type={/secret|password|token/i.test(e.key) ? "password" : "text"} value={val(e)} oninput={(ev) => setVal(e, (ev.currentTarget as HTMLInputElement).value)} />
                  {/if}
                  {#if changed}
                    <button class="btn ghost sm icon" title={t("cf.reset", { v: e.value })} onclick={() => setVal(e, e.value)}><Icon name="back" size={12} /></button>
                  {/if}
                </div>
              </div>
            {/each}
          {:else}
            <p class="muted small pad">{q ? t("cf.noMatch") : t("cf.emptyFile")}</p>
          {/each}
        </div>
      {:else}
        <p class="muted small" style="margin:0">{t("cf.textNote")}</p>
        <textarea class="code mono" bind:value={text} spellcheck="false"></textarea>
      {/if}
    {:else if !files.length}
      <div class="empty">{t("cf.none")}</div>
    {/if}
  </section>
</div>

<style>
  .layout { flex: 1; min-height: 0; display: grid; grid-template-columns: 260px 1fr; }
  .files { border-right: 1px solid var(--line); padding: 10px 8px; display: flex; flex-direction: column; gap: 2px; }
  .ghead { margin: 10px 8px 4px; font-size: 0.75em; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .file { display: grid; gap: 1px; width: 100%; padding: 6px 10px; border: 0; border-radius: 8px; background: none; color: var(--text); font: inherit; font-size: 0.9em; text-align: left; cursor: pointer; }
  .file:hover { background: var(--surface); }
  .file.active { background: color-mix(in srgb, var(--accent) 18%, transparent); }
  .fname { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .frel { font-size: 0.78em; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .search { position: relative; color: var(--muted); }
  .search :global(svg) { position: absolute; left: 9px; top: 50%; transform: translateY(-50%); }
  .search input { padding-left: 28px; height: 2em; width: 100%; }
  .search.wide { width: 320px; }
  .main { min-width: 0; min-height: 0; display: flex; flex-direction: column; gap: 8px; padding: 12px 16px 14px; }
  .head { display: flex; align-items: center; gap: 12px; }
  .head h3 { font-size: 1em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .head p { margin: 2px 0 0; }
  .minw { min-width: 0; }
  .seg { display: flex; padding: 3px; background: var(--surface-2); border-radius: calc(var(--radius) * 0.8); }
  .seg button { padding: 4px 12px; border: 0; border-radius: calc(var(--radius) * 0.6); background: none; color: var(--muted); font: inherit; font-size: 0.88em; cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.15); }
  .bar { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .small { font-size: 0.85em; }
  .pad { padding: 8px; }
  .entries { flex: 1; min-height: 0; display: grid; align-content: start; gap: 1px; padding-right: 4px; }
  .entries.dim { opacity: 0.5; }
  .sec { margin: 12px 0 4px; padding: 0 10px; font-size: 0.8em; color: var(--accent); }
  .row1 { display: grid; grid-template-columns: minmax(0, 1fr) minmax(200px, 340px); gap: 14px; align-items: center; padding: 6px 10px; border-radius: 8px; border-left: 2px solid transparent; }
  .row1:hover { background: var(--surface); }
  .row1.changed { border-left-color: var(--accent); background: color-mix(in srgb, var(--accent) 7%, transparent); }
  .row1.ro { opacity: 0.7; }
  .k { display: grid; gap: 2px; min-width: 0; }
  .key { font-size: 0.9em; overflow-wrap: anywhere; }
  .hint { font-size: 0.82em; line-height: 1.4; color: var(--text); opacity: 0.8; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .hint.muted { color: var(--muted); opacity: 1; -webkit-line-clamp: 2; line-clamp: 2; }
  .v { display: flex; align-items: center; gap: 8px; justify-content: flex-end; min-width: 0; }
  .v .txt { width: 100%; height: 2em; }
  .v .num { width: 9em; height: 2em; text-align: right; font-variant-numeric: tabular-nums; }
  .v select { max-width: 100%; height: 2em; }
  .raw { font-size: 0.82em; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  .switch { position: relative; width: 36px; height: 20px; flex: none; }
  .switch input { opacity: 0; width: 0; height: 0; position: absolute; }
  .switch span { position: absolute; inset: 0; border-radius: 999px; background: var(--surface-2); border: 1px solid var(--line); cursor: pointer; transition: background calc(0.15s * var(--anim-k)); }
  .switch span::after { content: ""; position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%; background: var(--muted); transition: transform calc(0.15s * var(--anim-k)); }
  .switch input:checked + span { background: var(--accent); border-color: var(--accent); }
  .switch input:checked + span::after { transform: translateX(16px); background: #fff; }
  .switch input:focus-visible + span { outline: 2px solid var(--accent); outline-offset: 2px; }
  .code { flex: 1; min-height: 0; resize: none; font-size: 0.85em; line-height: 1.5; background: var(--console); tab-size: 4; white-space: pre; }
  @media (max-width: 1000px) { .layout { grid-template-columns: 200px 1fr; } .row1 { grid-template-columns: 1fr; } .v { justify-content: flex-start; } }
</style>
