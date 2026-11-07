<script lang="ts">
  import Icon from "../Icon.svelte";
  import Hint from "../Hint.svelte";
  import { api, type Kind, type Plan, type SandboxResult, type ServerEntry, type UpdatesReport } from "../../lib/api";
  import { app, t, toast, fail } from "../../lib/store.svelte";

  let { server, kind, onUpdated }: { server: ServerEntry; kind: Kind; onUpdated?: () => void } = $props();

  let report = $state<UpdatesReport | null>(null);
  let loading = $state(false);
  let sel = $state<Record<string, boolean>>({});
  let showUnknown = $state(false);

  async function check() {
    loading = true;
    try {
      report = await api.catUpdates(server.id, kind);
      sel = Object.fromEntries(report.updates.map((u) => [u.file, !u.prerelease]));
    } catch (e) {
      fail(e);
    } finally {
      loading = false;
    }
  }
  check();

  const chosen = $derived(report?.updates.filter((u) => sel[u.file]) ?? []);

  const makePlan = (): Plan => ({
    items: chosen.map((u) => ({
      title: u.title,
      icon_url: u.icon_url,
      file: u.latest,
      reason: "update" as const,
      replaces: u.file !== u.latest.file_name ? u.file : null,
    })),
    already: [],
    warnings: [],
  });

  function apply() {
    app.plan = {
      id: server.id,
      kind,
      plan: makePlan(),
      onDone: () => {
        check();
        onUpdated?.();
      },
    };
  }

  // проверка в тестовой копии: ставим, только если копия запустилась
  let testing = $state(false);
  let sandbox = $state<SandboxResult | null>(null);
  async function testApply() {
    testing = true;
    sandbox = null;
    try {
      sandbox = await api.sandboxUpdate(server.id, makePlan());
      if (sandbox.ok) {
        toast(t("upd.sbOk", { n: sandbox.installed.length }), "ok");
        check();
        onUpdated?.();
      }
    } catch (e) {
      fail(e);
    } finally {
      testing = false;
      app.catalogProgress = null;
    }
  }
</script>

<div class="wrapx">
  <div class="bar">
    <button class="btn sm" disabled={loading} onclick={check}><Icon name="restart" size={14} />{loading ? t("upd.checking") : t("upd.check")}</button>
    {#if report}<span class="muted small">{t("upd.checked", { n: report.checked })}</span>{/if}
    <span class="grow"></span>
    {#if report?.updates.length}
      <button class="btn ghost sm" onclick={() => (sel = Object.fromEntries(report!.updates.map((u) => [u.file, true])))}>{t("upd.selectAll")}</button>
      {#if kind === "mod"}
        <button class="btn sm" disabled={!chosen.length || testing} onclick={testApply}>
          <Icon name="shield" size={14} />{testing ? t("upd.sbRunning") : t("upd.sbTest", { n: chosen.length })}
        </button>
        <Hint text={t("upd.sbHint")} />
      {/if}
      <button class="btn primary sm" disabled={!chosen.length || testing} onclick={apply}><Icon name="download" size={14} />{t("upd.apply", { n: chosen.length })}</button>
    {/if}
  </div>
  <p class="muted small hint">{t("upd.note")}</p>
  {#if testing}
    <p class="small hint">{app.catalogProgress?.title && app.catalogProgress.done < app.catalogProgress.total ? `${t("upd.sbDownloading")} ${app.catalogProgress.title}` : t("upd.sbRunningLong")}</p>
  {:else if sandbox}
    <p class="small hint" class:warnline={!sandbox.ok} class:okline={sandbox.ok}>{sandbox.ok ? "✔" : "✖"} {sandbox.message}</p>
  {/if}
  {#if report?.updates.some((u) => u.prerelease)}<p class="small hint warnline">{t("upd.prerelease")}</p>{/if}

  <div class="list scroll">
    {#if report && !report.updates.length}
      <div class="ok"><Icon name="check" size={16} />{t("upd.none")}</div>
    {/if}
    {#each report?.updates ?? [] as u (u.file)}
      <label class="item">
        <input type="checkbox" bind:checked={sel[u.file]} />
        {#if u.icon_url}<img src={u.icon_url} alt="" width="32" height="32" loading="lazy" />{:else}<span class="av">{u.title.slice(0, 2)}</span>{/if}
        <span class="txt">
          <b>{u.title}</b>
          <span class="mono small"><span class="muted">{u.current}</span> → <span class="new">{u.latest.version_number}</span></span>
        </span>
        <span class="chip {u.latest.channel === 'release' ? 'ok' : 'warn'}" title={u.prerelease ? t("upd.prerelease") : ""}>{u.latest.channel}</span>
        <span class="muted small">{u.latest.date.slice(0, 10)}</span>
      </label>
    {/each}

    {#if report?.unknown.length}
      <button class="unk" onclick={() => (showUnknown = !showUnknown)}>
        <Icon name="chevron" size={12} />{t("upd.unknown", { n: report.unknown.length })}
      </button>
      {#if showUnknown}<div class="muted mono small unklist">{report.unknown.join("\n")}</div>{/if}
    {/if}
  </div>
</div>

<style>
  .wrapx { display: flex; flex-direction: column; flex: 1; min-height: 0; gap: 6px; }
  .bar { display: flex; align-items: center; gap: 10px; }
  .hint { margin: 0; }
  .warnline { color: var(--warn); }
  .okline { color: var(--ok); }
  .small { font-size: 0.85em; }
  .list { flex: 1; display: grid; align-content: start; gap: 4px; padding-bottom: 16px; }
  .item { display: flex; align-items: center; gap: 12px; padding: 7px 12px; border-radius: var(--radius); cursor: pointer; }
  .item:hover { background: var(--surface); }
  .item input { accent-color: var(--accent); }
  .item img { border-radius: 6px; }
  .av { width: 32px; height: 32px; display: grid; place-items: center; border-radius: 6px; background: var(--surface-2); color: var(--muted); font-weight: 700; font-size: 0.8em; }
  .txt { display: grid; flex: 1; min-width: 0; }
  .new { color: var(--ok); }
  .ok { display: flex; gap: 10px; align-items: center; padding: 12px 16px; border-radius: var(--radius); background: color-mix(in srgb, var(--ok) 10%, transparent); color: var(--ok); }
  .unk { display: flex; align-items: center; gap: 6px; padding: 8px 12px; border: 0; background: none; color: var(--muted); font: inherit; font-size: 0.88em; cursor: pointer; text-align: left; }
  .unklist { white-space: pre-wrap; padding: 0 12px 8px 30px; }
</style>
