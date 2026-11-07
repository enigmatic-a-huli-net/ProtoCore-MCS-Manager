<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import Icon from "../Icon.svelte";
  import Hint from "../Hint.svelte";
  import { api, bytes, type Change, type ServerEntry, type SnapDiff, type SnapEntry, type Snapshot } from "../../lib/api";
  import { app, status, t, toast, fail, saveServer } from "../../lib/store.svelte";
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
    copyRepo: server.backup.copy_repo ?? "",
    copyEnv: (server.backup.copy_env ?? []).join("\n"),
    copyAfter: server.backup.copy_after_backup ?? true,
    verify: server.backup.verify_weekly ?? true,
  });

  // ---------- просмотр снапшота
  let browse = $state<Snapshot | null>(null);
  let path = $state<string[]>([]);
  let entries = $state<SnapEntry[]>([]);
  let names = $state<Record<string, string>>({});
  let lsLoading = $state(false);
  const running = $derived(status(server.id).status !== "stopped");

  async function openSnap(s: Snapshot) {
    browse = s;
    path = [];
    await ls();
  }
  async function ls() {
    if (!browse) return;
    lsLoading = true;
    try {
      const r = await api.snapshotLs(server.id, browse.id, path.join("/"));
      entries = r.entries;
      names = r.names;
    } catch (e) {
      fail(e);
      entries = [];
    } finally {
      lsLoading = false;
    }
  }
  function go(i: number) {
    path = path.slice(0, i);
    ls();
  }
  function enter(e: SnapEntry) {
    path = e.rel.split("/").filter(Boolean);
    ls();
  }
  // playerdata/<uuid>.dat → имя игрока
  const label = (e: SnapEntry) => {
    const m = e.name.match(/^([0-9a-f]{8}-[0-9a-f-]{27})(\..+)?$/i);
    const n = m && names[m[1].toLowerCase()];
    return n ? t("sb.player", { n }) : "";
  };
  async function restore(e: SnapEntry, inPlace: boolean) {
    if (!browse) return;
    if (inPlace && !(await ask(t("sb.confirmPlace", { rel: e.rel, id: browse.short_id }), { kind: "warning" }))) return;
    working = true;
    try {
      const p = await api.snapshotRestore(server.id, browse.id, e.rel, inPlace);
      toast(t("sb.restored", { p }), "ok");
      jload();
    } catch (err) {
      fail(err);
    } finally {
      working = false;
    }
  }

  // ---------- сравнение с предыдущим
  let diff = $state<{ a: Snapshot; b: Snapshot; d: SnapDiff | null } | null>(null);
  async function compare(i: number) {
    const snaps = list?.snapshots ?? [];
    const b = snaps[i], a = snaps[i + 1];
    if (!a || !b) return;
    diff = { a, b, d: null };
    try {
      const d = await api.snapshotDiff(server.id, a.id, b.id);
      if (diff) diff.d = d;
    } catch (e) {
      fail(e);
      diff = null;
    }
  }

  // ---------- журнал
  let changes = $state<Change[]>([]);
  async function jload() {
    try {
      changes = await api.getChanges(server.id);
    } catch {
      /* пусто */
    }
  }
  jload();

  async function copyNow() {
    working = true;
    try {
      await saveForm(true);
      await api.copyNow(server.id);
      toast(t("sb.copied"), "ok");
    } catch (e) {
      fail(e);
    } finally {
      working = false;
    }
  }
  async function verify() {
    working = true;
    try {
      await api.verifyRepo(server.id);
      toast(t("sb.verifyOk"), "ok");
      jload();
    } catch (e) {
      fail(e);
    } finally {
      working = false;
    }
  }
  const jtime = (s: number) => new Date(s * 1000).toLocaleString([], { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" });
  const kindIcon: Record<string, string> = { mods: "puzzle", plugins: "plug", core: "rocket", settings: "sliders", props: "file", backup: "archive", quarantine: "alert", start: "play" };

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

  async function saveForm(quiet = false) {
    try {
      await saveServer({
        ...server,
        backup: {
          ...server.backup,
          snapshot_on_start: form.on,
          keep_last: Math.max(0, Number(form.keep) || 0),
          repo_path: form.repo.trim(),
          excludes: form.excl.split("\n").map((x) => x.trim()).filter(Boolean),
          copy_repo: form.copyRepo.trim(),
          copy_env: form.copyEnv.split("\n").map((x) => x.trim()).filter((x) => x.includes("=")),
          copy_after_backup: form.copyAfter,
          verify_weekly: form.verify,
        },
      });
      if (quiet) return;
      toast(t("ln.saved"), "ok");
      load();
    } catch (e) {
      if (quiet) throw e;
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
        <button class="btn sm" disabled={working || !list?.snapshots.length} onclick={verify} title={t("sb.verifyWeekly")}><Icon name="search" size={14} />{t("sb.verify")}</button>
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
          <button class="btn ghost sm" onclick={() => openSnap(s)}><Icon name="folder" size={14} />{t("sb.browse")}</button>
          {#if i < (list?.snapshots.length ?? 0) - 1}
            <button class="btn ghost sm" onclick={() => compare(i)}>{t("sb.diff")}</button>
          {/if}
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
    <label class="check"><input type="checkbox" bind:checked={form.on} />{t("bk.onStart")}<Hint text={t("hn.snapshot")} /></label>
    <div class="grid">
      <label class="field"><span>{t("bk.keep")}<Hint text={t("hn.keep")} /></span><input type="number" min="0" bind:value={form.keep} /></label>
      <label class="field"><span>{t("bk.repoPath")}</span><input bind:value={form.repo} placeholder={app.paths?.restic_root} /></label>
    </div>
    <label class="field"><span>{t("bk.excludes")}</span><textarea rows="4" bind:value={form.excl}></textarea></label>
    <div><button class="btn primary sm" type="submit">{t("act.save")}</button></div>
  </form>

  <form class="panel settings" onsubmit={(e) => { e.preventDefault(); saveForm(); }}>
    <h3>{t("sb.copyTitle")}<Hint text={t("hn.321")} /></h3>
    <p class="muted small">{t("sb.copyText")}</p>
    <label class="field"><span>{t("sb.copyRepo")}</span><input bind:value={form.copyRepo} placeholder="D:\Backups\{server.name}  ·  s3:s3.amazonaws.com/bucket/mc" /></label>
    <label class="field"><span>{t("sb.copyEnv")}</span><textarea rows="2" bind:value={form.copyEnv} placeholder="AWS_ACCESS_KEY_ID=…&#10;AWS_SECRET_ACCESS_KEY=…" spellcheck="false"></textarea></label>
    <label class="check"><input type="checkbox" bind:checked={form.copyAfter} />{t("sb.copyAfter")}</label>
    <label class="check"><input type="checkbox" bind:checked={form.verify} />{t("sb.verifyWeekly")}</label>
    <div class="row">
      <button class="btn primary sm" type="submit">{t("act.save")}</button>
      <button class="btn sm" type="button" disabled={working || !form.copyRepo.trim() || !app.restic.found} onclick={copyNow}>{t("sb.copyNow")}</button>
    </div>
  </form>

  <section class="panel settings">
    <h3>{t("sb.journal")}</h3>
    <p class="muted small">{t("sb.journalText")}</p>
    <div class="journal">
      {#each changes as c}
        <div class="jrow">
          <span class="muted ts">{jtime(c.ts)}</span>
          <Icon name={(kindIcon[c.kind] ?? "file") as any} size={13} />
          <span class="jtext">{c.text}</span>
        </div>
      {:else}
        <p class="muted small">{t("sb.journalEmpty")}</p>
      {/each}
    </div>
  </section>
</div>

{#if browse}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={(e) => e.target === e.currentTarget && (browse = null)}>
    <div class="modal wide browser" role="dialog" aria-modal="true" aria-label={t("sb.browse")}>
      <div class="row">
        <h2 class="grow">{t("sb.title", { id: browse.short_id, time: when(browse.time) })}</h2>
        <button class="btn ghost sm icon" onclick={() => (browse = null)}><Icon name="x" size={14} /></button>
      </div>
      <div class="crumbs">
        <button class="link" onclick={() => go(0)}>{t("sb.root")}</button>
        {#each path as p, i}<span class="muted">/</span><button class="link" onclick={() => go(i + 1)}>{p}</button>{/each}
      </div>
      <div class="files scroll" class:dim={lsLoading}>
        {#each entries as e (e.rel)}
          <div class="frow">
            {#if e.dir}
              <button class="fname link" onclick={() => enter(e)}><Icon name="folder" size={14} />{e.name}</button>
            {:else}
              <span class="fname"><Icon name="file" size={14} />{e.name}</span>
            {/if}
            {#if label(e)}<span class="chip accent">{label(e)}</span>{/if}
            <span class="grow"></span>
            <span class="muted small num">{e.dir ? "" : bytes(e.size)}</span>
            <button class="btn ghost sm" disabled={working} title={t("sb.restoreSideHint")} onclick={() => restore(e, false)}>{t("sb.restoreSide")}</button>
            <button class="btn sm" disabled={working || running} title={running ? t("sb.restorePlaceHint") : t("sb.restorePlaceHint")} onclick={() => restore(e, true)}><Icon name="back" size={13} />{t("sb.restorePlace")}</button>
          </div>
        {:else}
          <p class="muted small">{lsLoading ? "…" : t("sb.empty")}</p>
        {/each}
      </div>
      <p class="muted small" style="margin:0">{t("sb.legend")}</p>
    </div>
  </div>
{/if}

{#if diff}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={(e) => e.target === e.currentTarget && (diff = null)}>
    <div class="modal wide" role="dialog" aria-modal="true" aria-label={t("sb.diff")}>
      <div class="row">
        <h2 class="grow">{t("sb.diffTitle", { a: diff.a.short_id, b: diff.b.short_id })}</h2>
        <button class="btn ghost sm icon" onclick={() => (diff = null)}><Icon name="x" size={14} /></button>
      </div>
      {#if !diff.d}
        <p class="muted">…</p>
      {:else if !diff.d.added.length && !diff.d.removed.length && !diff.d.modified.length}
        <p class="muted">{t("sb.noDiff")}</p>
      {:else}
        {#each [["added", "ok", "+"], ["removed", "err", "−"], ["modified", "warn", "~"]] as [k, cls, sign]}
          {@const items = (diff.d as unknown as Record<string, string[]>)[k]}
          {#if items.length}
            <div class="dgroup">
              <h3><span class="chip {cls}">{sign} {items.length}</span> {t(`sb.${k}` as Key)}</h3>
              {#each items.slice(0, 200) as f}<div class="mono small dline">{f}</div>{/each}
              {#if items.length > 200}<div class="muted small">{t("sb.more", { n: items.length - 200 })}</div>{/if}
            </div>
          {/if}
        {/each}
      {/if}
    </div>
  </div>
{/if}

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
  .small { font-size: 0.86em; }
  .settings p { margin: 0; }
  .journal { display: grid; gap: 3px; max-height: 300px; overflow: auto; }
  .jrow { display: flex; align-items: baseline; gap: 8px; font-size: 0.9em; }
  .jrow :global(svg) { color: var(--muted); flex: none; transform: translateY(2px); }
  .jtext { overflow-wrap: anywhere; user-select: text; }
  .ts { min-width: 7.5em; font-variant-numeric: tabular-nums; font-size: 0.9em; }
  .browser { width: min(860px, calc(100vw - 40px)); }
  .crumbs { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; font-size: 0.92em; }
  .crumbs .link { font-size: 1em; }
  .files { display: grid; align-content: start; gap: 1px; max-height: 52vh; min-height: 200px; }
  .files.dim { opacity: 0.55; }
  .frow { display: flex; align-items: center; gap: 8px; padding: 3px 6px; border-radius: 6px; }
  .frow:hover { background: var(--surface-2); }
  .fname { display: inline-flex; align-items: center; gap: 6px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 0.92em; }
  .fname :global(svg) { color: var(--muted); flex: none; }
  .num { min-width: 5.5em; text-align: right; font-variant-numeric: tabular-nums; }
  .dgroup { display: grid; gap: 2px; }
  .dgroup h3 { display: flex; align-items: center; gap: 8px; font-size: 0.95em; margin: 4px 0; }
  .dline { overflow-wrap: anywhere; user-select: text; }
</style>
