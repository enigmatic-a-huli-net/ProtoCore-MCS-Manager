<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import Icon from "../Icon.svelte";
  import Hint from "../Hint.svelte";
  import { api, bytes, type LogHit, type ServerEntry } from "../../lib/api";
  import { t, toast, fail } from "../../lib/store.svelte";

  let { server }: { server: ServerEntry } = $props();
  let total = $state(0);
  let count = $state(0);
  let q = $state("");
  let hits = $state<LogHit[] | null>(null);
  let busy = $state(false);
  let days = $state(30);

  async function info() {
    try {
      const r = await api.logsInfo(server.id);
      total = r.total;
      count = r.files.length;
    } catch (e) {
      fail(e);
    }
  }
  info();

  async function search() {
    if (!q.trim()) return;
    busy = true;
    try {
      hits = await api.logsSearch(server.id, q);
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }

  async function clean() {
    if (!(await ask(t("lg.confirm", { d: days }), { kind: "info" }))) return;
    busy = true;
    try {
      const r = await api.logsClean(server.id, Number(days) || 0);
      toast(t("lg.cleaned", { c: r.compressed, d: r.deleted, s: bytes(r.freed) }), "ok");
      info();
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }

  // подсветка найденного
  function parts(text: string) {
    const i = text.toLowerCase().indexOf(q.trim().toLowerCase());
    if (i < 0 || !q.trim()) return [text, "", ""];
    return [text.slice(0, i), text.slice(i, i + q.trim().length), text.slice(i + q.trim().length)];
  }
</script>

<section class="panel lg">
  <div class="row wrap">
    <h3 class="grow">{t("lg.title")}<Hint text={t("lg.hint")} /></h3>
    <span class="muted small">{t("lg.size", { n: count, s: bytes(total) })}</span>
    <label class="small days">{t("lg.delete")} <input type="number" min="0" max="3650" bind:value={days} /> {t("lg.days")}</label>
    <button class="btn sm" disabled={busy || !count} onclick={clean}><Icon name="archive" size={13} />{t("lg.clean")}</button>
  </div>
  <form class="row" onsubmit={(e) => { e.preventDefault(); search(); }}>
    <div class="search grow">
      <Icon name="search" size={14} />
      <input placeholder={t("lg.search")} bind:value={q} />
    </div>
    <button class="btn sm" type="submit" disabled={busy || !q.trim()}>{busy ? "…" : t("lg.find")}</button>
  </form>
  {#if hits}
    {#if !hits.length}
      <p class="muted small">{t("lg.none")}</p>
    {:else}
      <p class="muted small">{t("lg.found", { n: hits.length >= 500 ? "500+" : hits.length })}</p>
      <div class="hits scroll">
        {#each hits as h, i (i)}
          {@const [a, b, c] = parts(h.text)}
          <div class="hit"><span class="where">{h.rel}:{h.line}</span><span class="txt">{a}<mark>{b}</mark>{c}</span></div>
        {/each}
      </div>
    {/if}
  {/if}
</section>

<style>
  .lg { padding: 12px 16px; display: grid; gap: 8px; }
  h3 { font-size: 0.98em; display: flex; align-items: center; }
  .small { font-size: 0.86em; margin: 0; }
  .days { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); }
  .days input { width: 4.5em; height: 1.9em; }
  .search { position: relative; color: var(--muted); }
  .search :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); }
  .search input { padding-left: 30px; height: 2.1em; width: 100%; }
  .hits { max-height: 340px; display: grid; align-content: start; gap: 1px; font-family: var(--mono); font-size: 0.8em; background: var(--console); border: 1px solid var(--line); border-radius: 8px; padding: 6px 8px; }
  .hit { display: flex; gap: 10px; user-select: text; }
  .where { color: var(--muted); flex: none; min-width: 12em; }
  .txt { overflow-wrap: anywhere; }
  mark { background: color-mix(in srgb, var(--accent) 35%, transparent); color: inherit; border-radius: 2px; }
</style>
