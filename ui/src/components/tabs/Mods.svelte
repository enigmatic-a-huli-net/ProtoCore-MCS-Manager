<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import Icon from "../Icon.svelte";
  import Catalog from "./Catalog.svelte";
  import Updates from "./Updates.svelte";
  import { api, bytes, supportsMods, type ModInfo, type ModReport, type ServerEntry } from "../../lib/api";
  import { app, status, t, toast, fail } from "../../lib/store.svelte";
  import type { Key } from "../../lib/i18n";

  let { server }: { server: ServerEntry } = $props();
  let view = $state<"installed" | "find" | "updates">("installed");

  type Filter = "all" | "enabled" | "disabled" | "problems" | "client";
  let report = $state<ModReport | null>(null);
  let loading = $state(false);
  let q = $state("");
  let filter = $state<Filter>("all");
  let open = $state("");
  let sel = $state<Record<string, boolean>>({});

  const live = $derived(status(server.id).status !== "stopped" && status(server.id).status !== "crashed");

  async function load() {
    loading = true;
    try {
      report = await api.mods(server.id);
    } catch (e) {
      fail(e);
    } finally {
      loading = false;
    }
  }
  load();

  const shown = $derived.by(() => {
    if (!report) return [];
    const f = q.trim().toLowerCase();
    return report.mods.filter((m) => {
      if (filter === "enabled" && !m.enabled) return false;
      if (filter === "disabled" && m.enabled) return false;
      if (filter === "problems" && !m.problems.length && !m.missing_deps.length && !m.wrong_deps?.length) return false;
      if (filter === "client" && m.client === "no") return false;
      if (!f) return true;
      return m.name.toLowerCase().includes(f) || m.file.toLowerCase().includes(f) || m.ids.some((i) => i.includes(f));
    });
  });

  const selected = $derived(Object.entries(sel).filter(([, v]) => v).map(([k]) => k));

  async function toggle(m: ModInfo) {
    if (m.enabled && m.required_by.length) {
      const ok = await ask(t("mods.warnDeps", { list: m.required_by.join(", ") }), { kind: "warning" });
      if (!ok) return;
    }
    try {
      await api.setMod(server.id, m.file, !m.enabled);
      if (live) toast(t("mods.runningNote"));
      await load();
    } catch (e) {
      fail(e);
    }
  }

  async function bulk(enable: boolean) {
    try {
      await api.setMods(server.id, selected, enable);
      sel = {};
      await load();
    } catch (e) {
      fail(e);
    }
  }

  async function removeClient(likely: boolean) {
    try {
      const moved = await api.disableClient(server.id, likely);
      toast(moved.length ? t("mods.moved", { n: moved.length }) : t("mods.none"), moved.length ? "ok" : "info");
      await load();
    } catch (e) {
      fail(e);
    }
  }

  let fixing = $state("");
  async function fixDep(modId: string, range: string) {
    fixing = modId;
    try {
      const plan = await api.catFix(server.id, modId, range);
      app.plan = { id: server.id, kind: "mod", plan, onDone: load };
    } catch (e) {
      fail(e);
    } finally {
      fixing = "";
    }
  }

  async function enableFile(file: string) {
    try {
      await api.setMod(server.id, file, true);
      await load();
    } catch (e) {
      fail(e);
    }
  }

  const initial = (m: ModInfo) => (m.name || m.file).replace(/[^\p{L}\p{N}]/gu, "").slice(0, 2).toUpperCase();
  const counts = $derived(
    report
      ? {
          all: report.mods.length,
          enabled: report.enabled_count,
          disabled: report.disabled_count,
          problems: report.mods.filter((m) => m.problems.length || m.missing_deps.length).length,
          client: report.client_count,
        }
      : null,
  );
  const reasonKey = (r: string | null) => `mods.reason.${r ?? "user"}` as Key;
</script>

{#if !supportsMods(server.core) && server.core !== "unknown"}
  <div class="empty">{t("mods.unsupported")}</div>
{:else}
  <div class="wrapx">
    <div class="views">
      {#each ["installed", "find", "updates"] as const as v}
        <button class:on={view === v} onclick={() => (view = v)}>
          <Icon name={v === "installed" ? "puzzle" : v === "find" ? "search" : "download"} size={14} />{t(`cat.${v}` as Key)}
        </button>
      {/each}
    </div>
    {#if view === "find"}
      <Catalog {server} kind="mod" onInstalled={load} />
    {:else if view === "updates"}
      <Updates {server} kind="mod" onUpdated={load} />
    {:else}
    <div class="bar">
      <div class="search">
        <Icon name="search" size={14} />
        <input placeholder={t("mods.search")} bind:value={q} />
      </div>
      <div class="seg">
        {#each ["all", "enabled", "disabled", "problems", "client"] as const as f}
          <button class:on={filter === f} onclick={() => (filter = f)}>
            {t(`mods.${f}` as Key)}
            {#if counts}<span>{counts[f]}</span>{/if}
          </button>
        {/each}
      </div>
      <span class="grow"></span>
      <button class="btn sm" onclick={() => removeClient(false)} title={t("mods.disableClient")}>
        <Icon name="shield" size={14} />{t("mods.disableClient")}
      </button>
      <button class="btn ghost sm icon" title={t("act.openFolder")} onclick={() => api.openPath(`${server.path}${server.path.includes("\\") ? "\\" : "/"}mods`).catch(fail)}>
        <Icon name="folder" size={15} />
      </button>
      <button class="btn ghost sm icon" title={t("act.refresh")} onclick={load} disabled={loading}>
        <Icon name="restart" size={15} />
      </button>
    </div>

    {#if report && (report.missing.length || report.conflicts.length || report.wrong_versions?.length)}
      <div class="alerts">
        {#if report.missing.length}
          <div class="alert err">
            <h3><Icon name="alert" size={15} />{t("mods.missing")} · {report.missing.length}</h3>
            <ul>
              {#each report.missing as d}
                <li>
                  <b class="mono">{d.id}</b>
                  <span class="muted">{t("mods.missingFor", { list: d.needed_by.join(", ") })}</span>
                  {#if d.disabled_file}
                    <button class="btn sm" onclick={() => enableFile(d.disabled_file!)}>
                      {t("mods.enableIt")} {d.disabled_file}
                    </button>
                  {:else}
                    <button class="btn sm" disabled={fixing === d.id} onclick={() => fixDep(d.id, "")}>
                      <Icon name="download" size={13} />{t("fix.find")}
                    </button>
                  {/if}
                </li>
              {/each}
            </ul>
          </div>
        {/if}
        {#if report.wrong_versions?.length}
          <div class="alert err">
            <h3><Icon name="alert" size={15} />{t("mods.wrongVer")} · {report.wrong_versions.length}</h3>
            <ul>
              {#each report.wrong_versions as w}
                <li>
                  <b>{report.mods.find((m) => m.file === w.file)?.name ?? w.id}</b>
                  <span class="muted">{t("mods.wrongVerRow", { have: w.have, need: w.need_human, by: w.needed_by })}</span>
                  <button class="btn sm" disabled={fixing === w.id} onclick={() => fixDep(w.id, w.need)}>
                    <Icon name="download" size={13} />{t("fix.button")}
                  </button>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
        {#if report.conflicts.length}
          <div class="alert warn">
            <h3><Icon name="alert" size={15} />{t("mods.conflicts")} · {report.conflicts.length}</h3>
            <ul>
              {#each report.conflicts as c}
                <li>
                  <span class="chip {c.kind === 'definite' ? 'err' : 'warn'}">
                    {c.kind === "definite" ? t("mods.definite") : t("mods.possible")}
                  </span>
                  <span>{c.reason}</span>
                  <span class="muted mono small">{c.a} ↔ {c.b}</span>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
      </div>
    {/if}

    {#if selected.length}
      <div class="selbar">
        <span>{t("mods.selected", { n: selected.length })}</span>
        <button class="btn sm" onclick={() => bulk(true)}>{t("mods.enableSel")}</button>
        <button class="btn sm" onclick={() => bulk(false)}>{t("mods.disableSel")}</button>
        <button class="btn ghost sm" onclick={() => (sel = {})}>{t("act.cancel")}</button>
      </div>
    {/if}

    <div class="list scroll">
      {#if report && !report.mods.length}
        <div class="empty">{t("mods.empty")}</div>
      {/if}
      {#each shown as m (m.file + m.enabled)}
        <div class="item" class:off={!m.enabled} class:expanded={open === m.file}>
          <div class="main">
            <input type="checkbox" class="sel" bind:checked={sel[m.file]} aria-label={m.name} />
            <button class="info" onclick={() => (open = open === m.file ? "" : m.file)}>
              {#if m.icon}
                <img src={m.icon} alt="" width="36" height="36" />
              {:else}
                <span class="avatar">{initial(m)}</span>
              {/if}
              <span class="txt">
                <span class="name">{m.name || m.file}<span class="ver">{m.version}</span></span>
                <span class="meta">
                  {#if !m.enabled}<span class="chip">{t(reasonKey(m.disabled_reason))}</span>{/if}
                  {#if m.client === "definite"}<span class="chip warn">{t("mods.clientDefinite")}</span>
                  {:else if m.client === "likely"}<span class="chip">{t("mods.clientLikely")}</span>{/if}
                  {#if m.enabled && m.missing_deps.length}<span class="chip err">−{m.missing_deps.join(", ")}</span>{/if}
                  {#each m.enabled ? (m.wrong_deps ?? []) : [] as id}<span class="chip err">{t("mods.wrongChip", { id })}</span>{/each}
                  {#if m.enabled && m.problems.some((p) => p.includes("конфликт"))}<span class="chip err">{t("mods.conflicts")}</span>{/if}
                  <span class="file">{m.file}</span>
                </span>
              </span>
            </button>
            <input type="checkbox" class="toggle" checked={m.enabled} onchange={() => toggle(m)} aria-label="on/off" />
          </div>
          {#if open === m.file}
            <div class="details">
              {#if m.description}<p>{m.description}</p>{/if}
              <dl>
                <dt>id</dt><dd class="mono">{m.ids.join(", ") || "—"}</dd>
                <dt>{t("mods.loader")}</dt><dd>{m.loader ?? "—"}</dd>
                <dt>{t("mods.file")}</dt><dd class="mono">{m.file} · {bytes(m.size)}</dd>
                {#if m.authors.length}<dt>{t("mods.by", { a: "" }).trim()}</dt><dd>{m.authors.join(", ")}</dd>{/if}
                {#if m.client !== "no"}<dt>{t("mods.client")}</dt><dd>{m.client_reason}</dd>{/if}
                {#if m.deps.length}
                  <dt>{t("mods.requires")}</dt>
                  <dd class="deps">
                    {#each m.deps as d}
                      <span class="chip {m.missing_deps.includes(d.id) || m.wrong_deps?.includes(d.id) ? 'err' : ''}" title={d.version}>
                        {d.id}{d.required ? "" : ` (${t("mods.optional")})`}
                      </span>
                    {/each}
                  </dd>
                {/if}
                {#if m.required_by.length}<dt>{t("mods.requiredBy")}</dt><dd>{m.required_by.join(", ")}</dd>{/if}
                {#if m.provides.length}<dt>provides</dt><dd class="mono">{m.provides.join(", ")}</dd>{/if}
                {#if m.problems.length}<dt><Icon name="alert" size={13} /></dt><dd>{m.problems.join("; ")}</dd>{/if}
                {#if m.homepage}<dt>URL</dt><dd><a href={m.homepage} target="_blank" rel="noreferrer">{m.homepage}</a></dd>{/if}
              </dl>
            </div>
          {/if}
        </div>
      {/each}
    </div>
    {/if}
  </div>
{/if}

<style>
  .views { display: flex; gap: 2px; padding: 3px; background: var(--surface-2); border-radius: calc(var(--radius) * 0.8); width: max-content; }
  .views button { display: inline-flex; align-items: center; gap: 6px; padding: 5px 12px; border: 0; border-radius: calc(var(--radius) * 0.6); background: none; color: var(--muted); font: inherit; font-size: 0.9em; cursor: pointer; }
  .views button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.15); }
  .wrapx { display: flex; flex-direction: column; flex: 1; min-height: 0; padding: 12px 16px 0; gap: 10px; }
  .bar { display: flex; align-items: center; gap: 10px; }
  .search { position: relative; flex: 0 1 230px; min-width: 140px; color: var(--muted); }
  .search :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); }
  .search input { padding-left: 30px; height: 2.1em; }
  .seg { display: flex; flex: none; padding: 3px; background: var(--surface-2); border-radius: calc(var(--radius) * 0.8); }
  .seg button {
    display: inline-flex;
    gap: 6px;
    padding: 4px 10px;
    border: 0;
    border-radius: calc(var(--radius) * 0.6);
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 0.9em;
    cursor: pointer;
  }
  .seg button span { opacity: 0.7; font-variant-numeric: tabular-nums; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.15); }

  .alerts { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); align-items: start; gap: 10px; }
  .alert { padding: 10px 14px; border-radius: var(--radius); border: 1px solid; max-height: 170px; overflow: auto; }
  .alert.err { border-color: color-mix(in srgb, var(--err) 40%, transparent); background: color-mix(in srgb, var(--err) 7%, transparent); }
  .alert.warn { border-color: color-mix(in srgb, var(--warn) 40%, transparent); background: color-mix(in srgb, var(--warn) 7%, transparent); }
  .alert h3 { display: flex; align-items: center; gap: 7px; font-size: 0.92em; margin-bottom: 6px; }
  .alert.err h3 { color: var(--err); }
  .alert.warn h3 { color: var(--warn); }
  .alert ul { margin: 0; padding: 0; list-style: none; display: grid; gap: 5px; font-size: 0.9em; }
  .alert li { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .small { font-size: 0.85em; }

  .selbar { display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-radius: var(--radius); background: var(--accent-soft); font-size: 0.9em; }

  .list { flex: 1; display: grid; align-content: start; gap: 4px; padding-bottom: 16px; }
  .item { border-radius: var(--radius); border: 1px solid transparent; }
  .item:hover { background: var(--surface); border-color: var(--line); }
  .item.expanded { background: var(--surface); border-color: var(--line); }
  .item.off .info img, .item.off .avatar, .item.off .name { opacity: 0.5; }
  .main { display: flex; align-items: center; gap: 10px; padding: 6px 12px 6px 10px; }
  .sel { accent-color: var(--accent); opacity: 0.35; }
  .item:hover .sel, .sel:checked { opacity: 1; }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .info img { border-radius: 8px; image-rendering: pixelated; flex: none; }
  .avatar {
    width: 36px;
    height: 36px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--muted);
    font-weight: 700;
    font-size: 0.85em;
  }
  .txt { display: grid; min-width: 0; }
  .name { font-weight: 550; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ver { margin-left: 8px; color: var(--muted); font-weight: 400; font-size: 0.88em; }
  .meta { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .file { font-size: 0.8em; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .details { padding: 4px 16px 14px 80px; font-size: 0.9em; }
  .details p { margin: 0 0 10px; color: var(--muted); max-width: 75ch; }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 5px 16px; margin: 0; }
  dt { color: var(--muted); }
  dd { margin: 0; min-width: 0; overflow-wrap: anywhere; }
  .deps { display: flex; gap: 4px; flex-wrap: wrap; }
</style>
