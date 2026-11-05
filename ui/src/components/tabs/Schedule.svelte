<script lang="ts">
  import { onDestroy } from "svelte";
  import Icon from "../Icon.svelte";
  import { api, type MonitorInfo, type ServerEntry } from "../../lib/api";
  import { app, t, toast, fail, saveServer } from "../../lib/store.svelte";
  import { DEFAULT_SCHEDULE } from "../../lib/schedule";

  let { server }: { server: ServerEntry } = $props();

  let s = $state(structuredClone({ ...DEFAULT_SCHEDULE, ...($state.snapshot(server.schedule) ?? {}) }));
  let autoRestart = $state(server.auto_restart);
  let times = $state(s.restart_times.join(", "));
  let warns = $state(s.warn_minutes.join(", "));
  let info = $state<MonitorInfo | null>(null);

  async function load() {
    try {
      info = await api.monitor(server.id);
    } catch {
      /* сервер мог быть удалён */
    }
  }
  load();
  const timer = setInterval(() => !document.hidden && load(), 15000);
  onDestroy(() => clearInterval(timer));

  // новая точка TPS приходит событием — дописываем в историю
  $effect(() => {
    const p = app.tps[server.id];
    if (p && info && !info.history.some((h) => h.ts === p.ts)) info.history = [...info.history.slice(-239), p];
  });

  async function save() {
    try {
      s.restart_times = times.split(",").map((x) => x.trim()).filter((x) => /^\d{1,2}:\d{2}$/.test(x));
      s.warn_minutes = warns.split(",").map((x) => Number(x.trim())).filter((x) => x > 0 && x <= 120);
      s.tps_interval_sec = Math.max(10, Number(s.tps_interval_sec) || 30);
      s.restart_hours = Math.min(168, Math.max(1, Number(s.restart_hours) || 8));
      await saveServer({ ...server, auto_restart: autoRestart, schedule: $state.snapshot(s) });
      toast(t("ln.saved"), "ok");
      load();
    } catch (e) {
      fail(e);
    }
  }

  // мини-график TPS
  const W = 640, H = 90;
  const chart = $derived.by(() => {
    const h = info?.history ?? [];
    if (h.length < 2) return null;
    const pts = h.map((p, i) => `${(i / (h.length - 1)) * W},${H - (Math.min(20, p.tps) / 20) * (H - 6) - 3}`).join(" ");
    const last = h[h.length - 1];
    const min = Math.min(...h.map((p) => p.tps));
    const avgMs = h.filter((p) => p.mspt != null).reduce((a, p, _, arr) => a + (p.mspt ?? 0) / arr.length, 0);
    return { pts, last, min, avgMs };
  });
  const when = (ms: number | null) => (ms ? new Date(ms).toLocaleString([], { hour: "2-digit", minute: "2-digit", day: "2-digit", month: "2-digit" }) : "—");
</script>

<form class="wrapx scroll" onsubmit={(e) => { e.preventDefault(); save(); }}>
  <section class="panel">
    <h3>{t("sc.tps")}</h3>
    {#if chart}
      <div class="chart">
        <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true">
          <line x1="0" x2={W} y1={H - (15 / 20) * (H - 6) - 3} y2={H - (15 / 20) * (H - 6) - 3} class="ref" />
          <polyline points={chart.pts} class="line" />
        </svg>
        <div class="facts">
          <span>TPS <b class:bad={chart.last.tps < 15} class:mid={chart.last.tps >= 15 && chart.last.tps < 19.5}>{chart.last.tps.toFixed(1)}</b></span>
          <span>min <b>{chart.min.toFixed(1)}</b></span>
          {#if chart.last.mspt != null}<span>{t("hdr.mspt", { v: chart.last.mspt.toFixed(1) })}</span>{/if}
          {#if chart.last.mspt_peak != null}<span>95%: <b>{chart.last.mspt_peak.toFixed(1)}</b></span>{/if}
          <span class="muted">{chart.last.via}</span>
        </div>
      </div>
    {:else}
      <p class="muted small">{t("sc.noTps")}</p>
    {/if}
    <div class="row wrap">
      <label class="check grow"><input type="checkbox" bind:checked={s.tps_enabled} />{t("sc.tpsOn")}</label>
      <label class="field narrow"><span>{t("sc.tpsEvery")}</span><input type="number" min="10" bind:value={s.tps_interval_sec} /></label>
    </div>
  </section>

  <section class="panel">
    <h3>{t("sc.restart")}</h3>
    <label class="check"><input type="checkbox" bind:checked={s.restart_enabled} />{t("sc.restartOn")}</label>
    {#if s.restart_enabled}
      <div class="modes">
        <label class:on={s.restart_mode === "interval"}><input type="radio" bind:group={s.restart_mode} value="interval" />{t("sc.modeInterval")}</label>
        <label class:on={s.restart_mode === "times"}><input type="radio" bind:group={s.restart_mode} value="times" />{t("sc.modeTimes")}</label>
      </div>
      <div class="grid">
        {#if s.restart_mode === "interval"}
          <label class="field"><span>{t("sc.hours")}</span><input type="number" min="1" max="168" bind:value={s.restart_hours} /></label>
        {:else}
          <label class="field"><span>{t("sc.times")}</span><input bind:value={times} placeholder="06:00, 18:00" /></label>
        {/if}
        <label class="field"><span>{t("sc.warn")}</span><input bind:value={warns} placeholder="15, 5, 1" /></label>
      </div>
      <label class="field"><span>{t("sc.message")}</span><input bind:value={s.warn_message} /></label>
      <label class="check"><input type="checkbox" bind:checked={s.skip_if_players} />{t("sc.skip")}</label>
      {#if info?.next_restart}<p class="next"><Icon name="restart" size={14} />{t("sc.next", { t: when(info.next_restart) })}</p>{/if}
    {/if}
    <label class="check"><input type="checkbox" bind:checked={autoRestart} />{t("sc.autoRestart")}</label>
  </section>

  <section class="panel">
    <h3>{t("sc.backup")}</h3>
    <label class="check"><input type="checkbox" bind:checked={s.backup_enabled} />{t("sc.backupOn")}</label>
    {#if s.backup_enabled}
      <div class="row">
        <label class="field narrow"><span>{t("sc.backupAt")}</span><input type="time" bind:value={s.backup_time} /></label>
        <span class="muted small grow">{t("sc.keepNote", { n: server.backup.keep_last })}</span>
      </div>
      {#if info?.next_backup}<p class="next"><Icon name="archive" size={14} />{t("sc.nextBackup", { t: when(info.next_backup) })}</p>{/if}
    {/if}
  </section>

  <div><button class="btn primary" type="submit">{t("act.save")}</button></div>
</form>

<style>
  .wrapx { flex: 1; padding: 14px 16px 20px; display: grid; align-content: start; gap: 14px; max-width: 860px; }
  section { padding: 16px 18px; display: grid; gap: 12px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px 16px; }
  .narrow { width: 10em; }
  .small { font-size: 0.88em; margin: 0; }
  .modes { display: flex; gap: 8px; }
  .modes label { display: flex; gap: 8px; align-items: center; padding: 7px 12px; border: 1px solid var(--line); border-radius: calc(var(--radius) * 0.8); cursor: pointer; }
  .modes label.on { border-color: var(--accent); background: var(--accent-soft); }
  .modes input { accent-color: var(--accent); }
  .next { display: flex; align-items: center; gap: 8px; margin: 0; color: var(--accent); font-size: 0.92em; }
  input[type="time"] { color-scheme: inherit; }
  .chart { display: grid; gap: 6px; }
  svg { width: 100%; height: 90px; background: var(--console); border: 1px solid var(--line); border-radius: 8px; }
  .line { fill: none; stroke: var(--ok); stroke-width: 2; vector-effect: non-scaling-stroke; }
  .ref { stroke: var(--warn); stroke-dasharray: 4 4; stroke-width: 1; vector-effect: non-scaling-stroke; opacity: 0.6; }
  .facts { display: flex; gap: 16px; font-size: 0.88em; color: var(--muted); flex-wrap: wrap; }
  .facts b { color: var(--ok); font-variant-numeric: tabular-nums; }
  .facts b.mid { color: var(--warn); }
  .facts b.bad { color: var(--err); }
</style>
