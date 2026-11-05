<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import Icon from "../Icon.svelte";
  import { api, bytes, type ServerEntry, type Snapshot } from "../../lib/api";
  import { app, t, toast, fail, saveServer } from "../../lib/store.svelte";
  import type { Key } from "../../lib/i18n";

  let { server }: { server: ServerEntry } = $props();

  let list = $state<{ snapshots: Snapshot[]; repo_path: string; repo_size: number } | null>(null);
  let downloading = $state(false);
  let working = $state(false);
  let startAfter = $state(true);

  // локальная копия настроек для формы
  let form = $state({
    on: server.backup.snapshot_on_start,
    keep: server.backup.keep_last,
    repo: server.backup.repo_path,
    excl: server.backup.excludes.join("\n"),
  });

  async function load() {
    try {
      list = await api.snapshots(server.id);
    } catch (e) {
      fail(e);
    }
  }
  if (app.restic.found) load();

  // обновляем список, когда закончился снапшот/откат
  let wasBusy = false;
  $effect(() => {
    const b = !!app.backup[server.id];
    if (wasBusy && !b) load();
    wasBusy = b;
  });

  async function download() {
    downloading = true;
    try {
      app.restic = await api.downloadRestic();
      toast(app.restic.version || "restic OK", "ok");
      await load();
    } catch (e) {
      fail(e);
    } finally {
      downloading = false;
    }
  }

  async function now() {
    working = true;
    try {
      await api.backupNow(server.id);
      toast(t("bk.done"), "ok");
    } catch (e) {
      fail(e);
    } finally {
      working = false;
      load();
    }
  }

  const when = (s: string) => new Date(s).toLocaleString();

  async function rollback(s: Snapshot) {
    if (!(await ask(t("bk.confirmRollback", { id: s.short_id, time: when(s.time) }), { kind: "warning" }))) return;
    working = true;
    try {
      await api.rollback(server.id, s.id, startAfter);
      toast(t("bk.rolled"), "ok");
    } catch (e) {
      fail(e);
    } finally {
      working = false;
      load();
    }
  }

  async function del(s: Snapshot) {
    if (!(await ask(t("bk.confirmDelete", { id: s.short_id }), { kind: "warning" }))) return;
    try {
      await api.deleteSnapshot(server.id, s.id);
      await load();
    } catch (e) {
      fail(e);
    }
  }

  async function check() {
    working = true;
    try {
      await api.checkRepo(server.id);
      toast(t("bk.checkOk"), "ok");
    } catch (e) {
      fail(e);
    } finally {
      working = false;
    }
  }

  async function saveForm() {
    try {
      await saveServer({
        ...server,
        backup: {
          ...server.backup,
          snapshot_on_start: form.on,
          keep_last: Math.max(0, Number(form.keep) || 0),
          repo_path: form.repo.trim(),
          excludes: form.excl.split("\n").map((x) => x.trim()).filter(Boolean),
        },
      });
      toast(t("ln.saved"), "ok");
      load();
    } catch (e) {
      fail(e);
    }
  }

  const tagLabel = (s: Snapshot) => {
    const tg = s.tags.find((x) => x !== "pcm") ?? "";
    const k = `bk.tag.${tg}` as Key;
    const v = t(k);
    return v === k ? tg : v;
  };
</script>

<div class="wrapx scroll">
  {#if !app.restic.found}
    <div class="panel intro">
      <Icon name="archive" size={28} />
      <div>
        <h2>{t("bk.restic")}</h2>
        <p class="muted">{t("bk.resticText")}</p>
      </div>
      <button class="btn primary" disabled={downloading} onclick={download}>
        <Icon name="download" />{downloading ? "…" : t("bk.download")}
      </button>
    </div>
  {:else}
    <div class="top">
      <div class="facts">
        <div><span>{t("bk.count")}</span><b>{list?.snapshots.length ?? "—"}</b></div>
        <div><span>{t("bk.size")}</span><b>{list ? bytes(list.repo_size) : "—"}</b></div>
        <div class="repo">
          <span>{t("bk.repo")}</span>
          <button class="link mono" onclick={() => list && api.openPath(list.repo_path).catch(fail)}>{list?.repo_path ?? "—"}</button>
        </div>
      </div>
      <div class="row">
        <label class="check sm"><input type="checkbox" bind:checked={startAfter} />{t("bk.startAfter")}</label>
        <button class="btn sm" disabled={working} onclick={check}><Icon name="shield" size={14} />{t("bk.check")}</button>
        <button class="btn primary" disabled={working || !!app.backup[server.id]} onclick={now}>
          <Icon name="archive" />{t("bk.now")}
        </button>
      </div>
    </div>

    <div class="snaps">
      {#each list?.snapshots ?? [] as s, i (s.id)}
        <div class="snap" class:latest={i === 0}>
          <span class="when">{when(s.time)}</span>
          <span class="chip {s.tags.includes('pre-rollback') ? 'warn' : s.tags.includes('manual') ? 'accent' : ''}">{tagLabel(s)}</span>
          <span class="mono muted">{s.short_id}</span>
          <span class="muted size">
            {#if s.summary?.data_added != null}{t("bk.added", { size: bytes(s.summary.data_added) })}{/if}
          </span>
          <span class="grow"></span>
          <button class="btn sm" disabled={working} onclick={() => rollback(s)}><Icon name="back" size={14} />{t("bk.rollback")}</button>
          <button class="btn ghost sm icon danger" disabled={working} title={t("bk.delete")} onclick={() => del(s)}>
            <Icon name="trash" size={14} />
          </button>
        </div>
      {:else}
        <div class="empty">{t("bk.empty")}</div>
      {/each}
    </div>
  {/if}

  <form class="panel settings" onsubmit={(e) => { e.preventDefault(); saveForm(); }}>
    <h3>{t("bk.settings")}</h3>
    <label class="check"><input type="checkbox" bind:checked={form.on} />{t("bk.onStart")}</label>
    <div class="grid">
      <label class="field"><span>{t("bk.keep")}</span><input type="number" min="0" bind:value={form.keep} /></label>
      <label class="field"><span>{t("bk.repoPath")}</span><input bind:value={form.repo} placeholder={app.paths?.restic_root} /></label>
    </div>
    <label class="field"><span>{t("bk.excludes")}</span><textarea rows="4" bind:value={form.excl}></textarea></label>
    <div><button class="btn primary sm" type="submit">{t("act.save")}</button></div>
  </form>
</div>

<style>
  .wrapx { flex: 1; padding: 14px 16px 18px; display: grid; align-content: start; gap: 14px; }
  .intro { display: flex; align-items: center; gap: 18px; padding: 18px 20px; }
  .intro > :global(svg) { color: var(--accent); }
  .intro div { flex: 1; }
  .intro p { margin: 4px 0 0; max-width: 70ch; }
  .top { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; flex-wrap: wrap; }
  .facts { display: flex; gap: 28px; min-width: 0; }
  .facts div { display: grid; min-width: 0; }
  .facts span { font-size: 0.8em; color: var(--muted); }
  .facts b { font-size: 1.1em; font-variant-numeric: tabular-nums; }
  .repo { max-width: 420px; }
  .link { padding: 0; border: 0; background: none; color: var(--text); font-size: 0.85em; text-align: left; cursor: pointer; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .link:hover { color: var(--accent); }
  .sm { font-size: 0.9em; }

  .snaps { display: grid; gap: 2px; }
  .snap { display: flex; align-items: center; gap: 12px; padding: 7px 12px; border-radius: var(--radius); }
  .snap:hover { background: var(--surface); }
  .snap.latest .when { font-weight: 600; }
  .when { min-width: 11em; font-variant-numeric: tabular-nums; }
  .size { font-size: 0.88em; }

  .settings { padding: 16px 18px; display: grid; gap: 12px; max-width: 760px; }
  .grid { display: grid; grid-template-columns: 240px 1fr; gap: 12px; }
</style>
