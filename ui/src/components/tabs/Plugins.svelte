<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import Icon from "../Icon.svelte";
  import Catalog from "./Catalog.svelte";
  import Updates from "./Updates.svelte";
  import { api, bytes, type PluginInfo, type PluginReport, type ServerEntry } from "../../lib/api";
  import { status, t, toast, fail } from "../../lib/store.svelte";
  import type { Key } from "../../lib/i18n";

  let { server }: { server: ServerEntry } = $props();
  let view = $state<"installed" | "find" | "updates">("installed");

  let report = $state<PluginReport | null>(null);
  let q = $state("");
  let open = $state("");
  const live = $derived(!["stopped", "crashed"].includes(status(server.id).status));

  async function load() {
    try {
      report = await api.plugins(server.id);
    } catch (e) {
      fail(e);
    }
  }
  load();

  const shown = $derived(
    (report?.plugins ?? []).filter((p) => {
      const f = q.trim().toLowerCase();
      return !f || p.name.toLowerCase().includes(f) || p.file.toLowerCase().includes(f);
    }),
  );

  async function toggle(p: PluginInfo) {
    if (p.enabled && p.required_by.length) {
      if (!(await ask(t("mods.warnDeps", { list: p.required_by.join(", ") }), { kind: "warning" }))) return;
    }
    try {
      await api.setPlugin(server.id, p.file, !p.enabled);
      if (live) toast(t("mods.runningNote"));
      await load();
    } catch (e) {
      fail(e);
    }
  }
</script>

<div class="wrapx">
  <div class="views">
    {#each ["installed", "find", "updates"] as const as v}
      <button class:on={view === v} onclick={() => (view = v)}>
        <Icon name={v === "installed" ? "plug" : v === "find" ? "search" : "download"} size={14} />{t(`cat.${v}` as Key)}
      </button>
    {/each}
  </div>
  {#if view === "find"}
    <Catalog {server} kind="plugin" onInstalled={load} />
  {:else if view === "updates"}
    <Updates {server} kind="plugin" onUpdated={load} />
  {:else}
  <div class="bar">
    <div class="search">
      <Icon name="search" size={14} />
      <input placeholder={t("mods.search")} bind:value={q} />
    </div>
    {#if report}<span class="muted">{t("mods.count", { on: report.enabled_count, off: report.disabled_count })}</span>{/if}
    <span class="grow"></span>
    <button class="btn ghost sm icon" title={t("act.openFolder")} onclick={() => api.openPath(`${server.path}${server.path.includes("\\") ? "\\" : "/"}plugins`).catch(fail)}>
      <Icon name="folder" size={15} />
    </button>
    <button class="btn ghost sm icon" title={t("act.refresh")} onclick={load}><Icon name="restart" size={15} /></button>
  </div>

  {#if report?.missing.length}
    <div class="alert">
      <h3><Icon name="alert" size={15} />{t("mods.missing")}</h3>
      {#each report.missing as d}
        <div><b>{d.id}</b> <span class="muted">{t("mods.missingFor", { list: d.needed_by.join(", ") })}</span></div>
      {/each}
    </div>
  {/if}

  <div class="list scroll">
    {#if report && !report.plugins.length}<div class="empty">{t("pl.empty")}</div>{/if}
    {#each shown as p (p.file + p.enabled)}
      <div class="item" class:off={!p.enabled}>
        <div class="main">
          <button class="info" onclick={() => (open = open === p.file ? "" : p.file)}>
            {#if p.icon}<img src={p.icon} alt="" width="36" height="36" />
            {:else}<span class="avatar">{p.name.slice(0, 2).toUpperCase()}</span>{/if}
            <span class="txt">
              <span class="name">{p.name}<span class="ver">{p.version}</span></span>
              <span class="meta">
                {#if p.paper_plugin}<span class="chip accent">Paper</span>{/if}
                {#if p.enabled && p.missing_deps.length}<span class="chip err">−{p.missing_deps.join(", ")}</span>{/if}
                {#if p.problems.length}<span class="chip warn">{p.problems[0]}</span>{/if}
                <span class="file">{p.file}</span>
              </span>
            </span>
          </button>
          <input type="checkbox" class="toggle" checked={p.enabled} onchange={() => toggle(p)} aria-label="on/off" />
        </div>
        {#if open === p.file}
          <div class="details">
            {#if p.description}<p>{p.description}</p>{/if}
            <dl>
              <dt>main</dt><dd class="mono">{p.main || "—"}</dd>
              <dt>api</dt><dd>{p.api_version || "—"}</dd>
              <dt>{t("mods.file")}</dt><dd class="mono">{p.file} · {bytes(p.size)}</dd>
              {#if p.authors.length}<dt>{t("mods.by", { a: "" }).trim()}</dt><dd>{p.authors.join(", ")}</dd>{/if}
              {#if p.depend.length}<dt>{t("pl.depend")}</dt><dd>{p.depend.join(", ")}</dd>{/if}
              {#if p.softdepend.length}<dt>{t("pl.softdepend")}</dt><dd>{p.softdepend.join(", ")}</dd>{/if}
              {#if p.required_by.length}<dt>{t("mods.requiredBy")}</dt><dd>{p.required_by.join(", ")}</dd>{/if}
              {#if p.website}<dt>URL</dt><dd><a href={p.website} target="_blank" rel="noreferrer">{p.website}</a></dd>{/if}
            </dl>
          </div>
        {/if}
      </div>
    {/each}
  </div>
  {/if}
</div>

<style>
  .views { display: flex; gap: 2px; padding: 3px; background: var(--surface-2); border-radius: calc(var(--radius) * 0.8); width: max-content; }
  .views button { display: inline-flex; align-items: center; gap: 6px; padding: 5px 12px; border: 0; border-radius: calc(var(--radius) * 0.6); background: none; color: var(--muted); font: inherit; font-size: 0.9em; cursor: pointer; }
  .views button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.15); }
  .wrapx { display: flex; flex-direction: column; flex: 1; min-height: 0; padding: 12px 16px 0; gap: 10px; }
  .bar { display: flex; align-items: center; gap: 12px; font-size: 0.92em; }
  .search { position: relative; width: 250px; color: var(--muted); }
  .search :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); }
  .search input { padding-left: 30px; height: 2.1em; }
  .alert { padding: 10px 14px; border-radius: var(--radius); border: 1px solid color-mix(in srgb, var(--err) 40%, transparent); background: color-mix(in srgb, var(--err) 7%, transparent); font-size: 0.9em; }
  .alert h3 { display: flex; gap: 7px; align-items: center; color: var(--err); font-size: 0.95em; margin-bottom: 4px; }
  .list { flex: 1; display: grid; align-content: start; gap: 4px; padding-bottom: 16px; }
  .item { border-radius: var(--radius); border: 1px solid transparent; }
  .item:hover { background: var(--surface); border-color: var(--line); }
  .item.off .name, .item.off img, .item.off .avatar { opacity: 0.5; }
  .main { display: flex; align-items: center; gap: 10px; padding: 6px 12px; }
  .info { flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; padding: 0; border: 0; background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .info img { border-radius: 8px; }
  .avatar { width: 36px; height: 36px; flex: none; display: grid; place-items: center; border-radius: 8px; background: var(--surface-2); color: var(--muted); font-weight: 700; font-size: 0.85em; }
  .txt { display: grid; min-width: 0; }
  .name { font-weight: 550; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ver { margin-left: 8px; color: var(--muted); font-weight: 400; font-size: 0.88em; }
  .meta { display: flex; gap: 6px; align-items: center; min-width: 0; }
  .file { font-size: 0.8em; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .details { padding: 4px 16px 14px 70px; font-size: 0.9em; }
  .details p { margin: 0 0 10px; color: var(--muted); max-width: 75ch; }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 5px 16px; margin: 0; }
  dt { color: var(--muted); }
  dd { margin: 0; overflow-wrap: anywhere; }
</style>
