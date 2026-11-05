<script lang="ts">
  import Icon from "../Icon.svelte";
  import { api, type ServerEntry } from "../../lib/api";
  import { t, toast, fail } from "../../lib/store.svelte";

  let { server }: { server: ServerEntry } = $props();

  let rows = $state<{ k: string; v: string; orig: string }[]>([]);
  let q = $state("");
  let loaded = $state(false);

  async function load() {
    try {
      rows = (await api.readProps(server.id)).map(([k, v]) => ({ k, v, orig: v }));
    } catch (e) {
      fail(e);
    } finally {
      loaded = true;
    }
  }
  load();

  const changed = $derived(rows.filter((r) => r.v !== r.orig));
  const shown = $derived(rows.filter((r) => !q || r.k.includes(q.toLowerCase())));

  async function save() {
    try {
      await api.writeProps(server.id, changed.map((r) => [r.k, r.v] as [string, string]));
      toast(t("pr.saved"), "ok");
      await load();
    } catch (e) {
      fail(e);
    }
  }

  // Рекомендации для нестабильного канала (обсуждались ранее)
  const RECOMMENDED: Record<string, string> = {
    "view-distance": "10",
    "simulation-distance": "6",
    "network-compression-threshold": "256",
  };
  function recommend() {
    for (const [k, v] of Object.entries(RECOMMENDED)) {
      const r = rows.find((x) => x.k === k);
      if (r) r.v = v;
      else rows.push({ k, v, orig: "" });
    }
  }
  const isBool = (v: string) => v === "true" || v === "false";
</script>

<div class="wrapx">
  {#if loaded && !rows.length}
    <div class="empty">{t("pr.empty")}</div>
  {:else}
    <div class="bar">
      <div class="search">
        <Icon name="search" size={14} />
        <input placeholder="view-distance, motd…" bind:value={q} />
      </div>
      <button class="btn sm" onclick={recommend} title={t("pr.recommendText")}>
        <Icon name="rocket" size={14} />{t("pr.recommend")}
      </button>
      <span class="grow"></span>
      <button class="btn primary sm" disabled={!changed.length} onclick={save}>{t("act.save")} {changed.length ? `(${changed.length})` : ""}</button>
    </div>
    <div class="table scroll">
      {#each shown as r (r.k)}
        <label class="prop" class:dirty={r.v !== r.orig}>
          <span class="k mono">{r.k}</span>
          {#if isBool(r.orig || r.v)}
            <input type="checkbox" class="toggle" checked={r.v === "true"} onchange={(e) => (r.v = e.currentTarget.checked ? "true" : "false")} />
          {:else}
            <input bind:value={r.v} spellcheck="false" />
          {/if}
        </label>
      {/each}
    </div>
  {/if}
</div>

<style>
  .wrapx { display: flex; flex-direction: column; flex: 1; min-height: 0; padding: 12px 16px 0; gap: 10px; }
  .bar { display: flex; align-items: center; gap: 10px; }
  .search { position: relative; width: 250px; color: var(--muted); }
  .search :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); }
  .search input { padding-left: 30px; height: 2.1em; }
  .table { flex: 1; display: grid; align-content: start; gap: 2px; padding-bottom: 16px; max-width: 900px; }
  .prop { display: grid; grid-template-columns: 320px 1fr; align-items: center; gap: 12px; padding: 4px 10px; border-radius: 8px; }
  .prop:hover { background: var(--surface); }
  .prop.dirty .k { color: var(--accent); }
  .k { font-size: 0.88em; }
  .prop input:not(.toggle) { height: 2em; }
</style>
