<script lang="ts">
  // Поиск модов/плагинов на Modrinth, Hangar и CurseForge.
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Icon from "../Icon.svelte";
  import { api, compact, type FileRef, type Kind, type Project, type ServerEntry, type Source } from "../../lib/api";
  import { app, t, fail } from "../../lib/store.svelte";
  import type { Key } from "../../lib/i18n";

  let { server, kind, onInstalled }: { server: ServerEntry; kind: Kind; onInstalled?: () => void } = $props();

  const sources = $derived(
    (kind === "plugin" ? ["modrinth", "hangar", "curseforge"] : ["modrinth", "curseforge"]) as Source[],
  );
  const srcLabel: Record<Source, string> = { modrinth: "Modrinth", hangar: "Hangar", curseforge: "CurseForge" };

  let source = $state<Source>("modrinth");
  let query = $state("");
  let sort = $state(kind === "mod" ? "relevance" : "downloads");
  let items = $state<Project[]>([]);
  let total = $state(0);
  let loading = $state(false);
  let error = $state("");
  let openVersions = $state("");
  let versions = $state<FileRef[] | null>(null);
  let busyId = $state("");

  const loaderLabel = $derived(
    kind === "mod"
      ? ({ fabric: "Fabric", quilt: "Quilt", forge: "Forge", mohist: "Forge", neoforge: "NeoForge", youer: "NeoForge", arclight: "NeoForge" } as Record<string, string>)[server.core] ?? server.core
      : "Paper",
  );

  let timer: ReturnType<typeof setTimeout>;
  function schedule() {
    clearTimeout(timer);
    timer = setTimeout(() => run(false), 350);
  }

  async function run(more: boolean) {
    if (source === "curseforge" && !app.config?.curseforge_key) {
      error = t("cat.cfKey");
      items = [];
      return;
    }
    loading = true;
    error = "";
    try {
      const r = await api.catSearch(server.id, kind, source, query, sort, more ? items.length : 0);
      items = more ? [...items, ...r.items] : r.items;
      total = r.total;
    } catch (e) {
      error = String(e);
      if (!more) items = [];
    } finally {
      loading = false;
    }
  }
  run(false);

  async function install(p: Project, version?: string) {
    busyId = p.id + (version ?? "");
    try {
      const plan = await api.catPlan(server.id, kind, p.source, p.id, version);
      app.plan = {
        id: server.id,
        kind,
        plan,
        onDone: () => {
          p.installed = true;
          onInstalled?.();
        },
      };
    } catch (e) {
      fail(e);
    } finally {
      busyId = "";
    }
  }

  async function toggleVersions(p: Project) {
    if (openVersions === p.id) {
      openVersions = "";
      return;
    }
    openVersions = p.id;
    versions = null;
    try {
      versions = await api.catVersions(server.id, kind, p.source, p.id);
    } catch (e) {
      versions = [];
      fail(e);
    }
  }
</script>

<div class="wrapx">
  <div class="bar">
    <div class="seg">
      {#each sources as s}
        <button class:on={source === s} onclick={() => { source = s; run(false); }}>{srcLabel[s]}</button>
      {/each}
    </div>
    <div class="search grow">
      <Icon name="search" size={14} />
      <input placeholder={t("cat.search", { src: srcLabel[source] })} bind:value={query} oninput={schedule} />
    </div>
    <select bind:value={sort} onchange={() => run(false)} style="width:auto">
      {#each ["relevance", "downloads", "updated", "newest"] as o}
        <option value={o}>{t(`cat.sort.${o}` as Key)}</option>
      {/each}
    </select>
  </div>
  <p class="muted small hint">{t("cat.for", { loader: loaderLabel, mc: server.mc_version })}</p>

  <div class="list scroll">
    {#if error}
      <div class="empty">{error}</div>
    {:else if !loading && !items.length}
      <div class="empty">{t("cat.empty", { loader: loaderLabel, mc: server.mc_version })}</div>
    {/if}

    {#each items as p (p.source + p.id)}
      <div class="item">
        <div class="main">
          {#if p.icon_url}<img src={p.icon_url} alt="" width="44" height="44" loading="lazy" />{:else}<span class="avatar">{p.title.slice(0, 2).toUpperCase()}</span>{/if}
          <div class="txt">
            <div class="row">
              <button class="name" onclick={() => openUrl(p.url).catch(fail)} title={t("cat.open")}>{p.title}</button>
              <span class="muted small">{p.author}</span>
              {#if p.installed}<span class="chip ok"><Icon name="check" size={11} />{t("cat.installedChip")}</span>{/if}
            </div>
            <span class="desc">{p.description}</span>
          </div>
          <span class="dl muted small"><Icon name="download" size={12} />{compact(p.downloads)}</span>
          <button class="btn ghost sm" onclick={() => toggleVersions(p)}>{t("cat.versions")}</button>
          <button class="btn sm primary" disabled={busyId === p.id} onclick={() => install(p)}>
            {busyId === p.id ? "…" : t("cat.install")}
          </button>
        </div>
        {#if openVersions === p.id}
          <div class="versions">
            {#if versions === null}
              <span class="muted small">…</span>
            {:else if !versions.length}
              <span class="muted small">{t("cat.noVersions", { loader: loaderLabel, mc: server.mc_version })}</span>
            {:else}
              {#each versions.slice(0, 12) as v (v.version_id)}
                <div class="v">
                  <span class="chip {v.channel === 'release' ? 'ok' : 'warn'}">{v.channel || "—"}</span>
                  <span class="mono small grow">{v.version_number}</span>
                  <span class="muted small">{v.date.slice(0, 10)}</span>
                  <button class="btn sm" disabled={busyId === p.id + v.version_id} onclick={() => install(p, v.version_id)}>{t("cat.install")}</button>
                </div>
              {/each}
            {/if}
          </div>
        {/if}
      </div>
    {/each}

    {#if items.length && items.length < total}
      <div class="more"><button class="btn sm" disabled={loading} onclick={() => run(true)}>{t("cat.more")}</button></div>
    {/if}
  </div>
</div>

<style>
  .wrapx { display: flex; flex-direction: column; flex: 1; min-height: 0; gap: 6px; }
  .bar { display: flex; align-items: center; gap: 10px; }
  .seg { display: flex; padding: 3px; background: var(--surface-2); border-radius: calc(var(--radius) * 0.8); flex: none; }
  .seg button { padding: 4px 12px; border: 0; border-radius: calc(var(--radius) * 0.6); background: none; color: var(--muted); font: inherit; font-size: 0.9em; cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.15); }
  .search { position: relative; color: var(--muted); }
  .search :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); }
  .search input { padding-left: 30px; height: 2.1em; }
  select { height: 2.1em; }
  .hint { margin: 0; }
  .small { font-size: 0.85em; }
  .list { flex: 1; display: grid; align-content: start; gap: 4px; padding-bottom: 16px; }
  .item { border-radius: var(--radius); border: 1px solid transparent; }
  .item:hover { background: var(--surface); border-color: var(--line); }
  .main { display: flex; align-items: center; gap: 12px; padding: 8px 12px; }
  .main img { border-radius: 9px; flex: none; }
  .avatar { width: 44px; height: 44px; flex: none; display: grid; place-items: center; border-radius: 9px; background: var(--surface-2); color: var(--muted); font-weight: 700; }
  .txt { display: grid; gap: 2px; min-width: 0; flex: 1; }
  .name { padding: 0; border: 0; background: none; color: var(--text); font: inherit; font-weight: 600; cursor: pointer; }
  .name:hover { color: var(--accent); }
  .desc { color: var(--muted); font-size: 0.88em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dl { display: inline-flex; align-items: center; gap: 4px; min-width: 4.5em; justify-content: flex-end; }
  .versions { display: grid; gap: 4px; padding: 0 12px 10px 68px; }
  .v { display: flex; align-items: center; gap: 10px; }
  .more { display: grid; place-items: center; padding: 8px; }
</style>
