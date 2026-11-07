<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Icon from "../Icon.svelte";
  import { api, type ConnectInfo, type NetStats, type ServerEntry } from "../../lib/api";
  import { app, status, t, toast, fail } from "../../lib/store.svelte";
  import type { Key } from "../../lib/i18n";

  let { server }: { server: ServerEntry } = $props();

  let info = $state<ConnectInfo | null>(null);
  let loading = $state(false);
  let busy = $state("");
  const running = $derived(status(server.id).status === "running");

  async function load(check: boolean) {
    loading = true;
    try {
      info = await api.connectInfo(server.id, check);
    } catch (e) {
      fail(e);
    } finally {
      loading = false;
    }
  }
  load(false);

  async function upnp(open: boolean) {
    busy = open ? "open" : "close";
    try {
      if (open) {
        const p = await api.upnpOpen(server.id);
        toast(t("net.upnpOk", { p }), "ok");
      } else {
        await api.upnpClose(server.id);
        toast(t("net.upnpClosed"), "ok");
      }
      await load(running);
    } catch (e) {
      fail(e);
    } finally {
      busy = "";
    }
  }

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast(t("net.copied"), "ok");
    } catch (e) {
      fail(e);
    }
  }

  const addr = (ip: string, port: number) => (port === 25565 ? ip : `${ip}:${port}`);

  // ---------- график качества: задержка (линия) и потери (красные отметки)
  const W = 640, H = 110;
  const chart = $derived.by(() => {
    const s = app.net?.samples ?? [];
    if (s.length < 3) return null;
    const vals = s.map((x) => {
      const ok = x.rtt.filter((r): r is number => r != null);
      return { ok: ok.length ? ok.reduce((a, b) => a + b, 0) / ok.length : null, lost: x.rtt.length - ok.length };
    });
    const max = Math.max(60, ...vals.map((v) => v.ok ?? 0)) * 1.15;
    const x = (i: number) => (i / (vals.length - 1)) * W;
    const y = (v: number) => H - (v / max) * (H - 8) - 4;
    let d = "";
    vals.forEach((v, i) => {
      if (v.ok == null) return;
      d += `${d && vals[i - 1]?.ok != null ? "L" : "M"}${x(i).toFixed(1)},${y(v.ok).toFixed(1)}`;
    });
    const losses = vals.map((v, i) => (v.lost ? x(i) : null)).filter((v): v is number => v != null);
    return { d, losses, max: Math.round(max) };
  });

  const gradeKey = (g: string) => (g ? (`net.${g}` as Key) : ("net.noData" as Key));
  const periods: [keyof NonNullable<typeof app.net>, Key][] = [
    ["last_min", "net.min1"],
    ["last_10min", "net.min10"],
    ["last_hour", "net.hour"],
  ];
  const st = (k: string) => (app.net as unknown as Record<string, NetStats> | null)?.[k];
</script>

<div class="wrapx scroll">
  <!-- ================= качество интернета ================= -->
  <section class="panel">
    <div class="row">
      <h3 class="grow">{t("net.quality")}</h3>
      {#if app.net?.last_10min.grade}
        <span class="chip {app.net.last_10min.grade === 'good' ? 'ok' : app.net.last_10min.grade === 'fair' ? 'warn' : 'err'}">
          {t(gradeKey(app.net.last_10min.grade))}
        </span>
      {/if}
    </div>
    <p class="muted small">{t("net.qualityText")}</p>

    {#if app.net && !app.net.enabled}
      <p class="muted">{t("net.off")}</p>
    {:else if !chart}
      <p class="muted">{t("net.noData")}</p>
    {:else}
      <div class="chart">
        <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true">
          {#each chart.losses as lx}<line x1={lx} x2={lx} y1="0" y2={H} class="loss" />{/each}
          <path d={chart.d} class="rtt" />
        </svg>
        <span class="ymax muted">{chart.max} мс</span>
      </div>
      <div class="grid3">
        {#each periods as [k, label]}
          {@const s = st(k as string)}
          {#if s && s.samples}
            <div class="stat">
              <span class="muted small">{t(label)}</span>
              <div class="nums">
                <span>{t("net.loss")} <b class:bad={s.loss >= 2} class:mid={s.loss >= 0.5 && s.loss < 2}>{s.loss.toFixed(1)}%</b></span>
                <span>{t("net.ping")} <b>{Math.round(s.avg_rtt)} мс</b></span>
                <span>{t("net.jitter")} <b>{Math.round(s.jitter)} мс</b></span>
              </div>
            </div>
          {/if}
        {/each}
      </div>
      {#if app.net && app.net.last_10min.grade && app.net.last_10min.grade !== "good"}
        <p class="hint warnline">{t("net.badHint")}</p>
      {/if}
      <p class="hint muted">{t("net.lagCompare")}</p>
    {/if}
  </section>

  <!-- ================= подключение ================= -->
  <section class="panel">
    <div class="row">
      <h3 class="grow">{t("net.howTo")}</h3>
      <button class="btn sm" disabled={loading} onclick={() => load(running)} title={running ? "" : t("net.needRun")}>
        <Icon name="search" size={14} />{loading ? t("net.checking") : t("net.check")}
      </button>
    </div>

    {#if info}
      <div class="addr">
        <span class="muted">{t("net.lan")}</span>
        <code>{info.local_ip ? addr(info.local_ip, info.port) : "—"}</code>
        {#if info.local_ip}<button class="btn ghost sm icon" title={t("net.copy")} onclick={() => copy(addr(info!.local_ip, info!.port))}><Icon name="copy" size={14} /></button>{/if}
      </div>
      <div class="addr">
        <span class="muted">{t("net.wan")}</span>
        <code>{info.public_ip ? addr(info.public_ip, info.port) : "—"}</code>
        {#if info.public_ip}<button class="btn ghost sm icon" title={t("net.copy")} onclick={() => copy(addr(info!.public_ip, info!.port))}><Icon name="copy" size={14} /></button>{/if}
        {#if info.reachable === true}
          <span class="chip ok"><Icon name="check" size={11} />{t("net.open")}</span>
        {:else if info.reachable === false}
          <span class="chip err">{t("net.closed")}</span>
        {:else if !running}
          <span class="muted small">{t("net.needRun")}</span>
        {/if}
      </div>

      {#if info.cgnat}
        <div class="note err">
          <p>{t("net.cgnat", { r: info.router_ip, p: info.public_ip })}</p>
          <p>{t("net.tunnel")}</p>
          <div><button class="btn sm" onclick={() => openUrl("https://playit.gg").catch(fail)}>{t("net.openPlayit")}</button></div>
        </div>
      {:else}
        <div class="row wrap">
          <button class="btn sm primary" disabled={!!busy || !info.upnp} onclick={() => upnp(true)}>
            <Icon name="plug" size={14} />{t("net.upnp")}
          </button>
          <button class="btn sm" disabled={!!busy || !info.upnp} onclick={() => upnp(false)}>{t("net.upnpClose")}</button>
        </div>
        {#if !info.upnp}
          <p class="hint muted">{t("net.upnpNo", { p: info.port, ip: info.local_ip || "этот ПК" })}</p>
        {/if}
        {#if info.reachable === false}
          <p class="hint muted">{t("net.firewall", { p: info.port })}</p>
          <p class="hint muted">{t("net.tunnel")}</p>
        {/if}
      {/if}
    {/if}
  </section>
</div>

<style>
  .wrapx { flex: 1; padding: 14px 16px 20px; display: grid; align-content: start; gap: 14px; max-width: 900px; }
  section { padding: 16px 18px; display: grid; gap: 10px; }
  .small { font-size: 0.86em; }
  p { margin: 0; }
  .hint { font-size: 0.88em; max-width: 80ch; }
  .warnline { color: var(--warn); }
  .chart { position: relative; }
  svg { width: 100%; height: 110px; background: var(--console); border: 1px solid var(--line); border-radius: 8px; display: block; }
  .rtt { fill: none; stroke: var(--info); stroke-width: 2; vector-effect: non-scaling-stroke; }
  .loss { stroke: var(--err); stroke-width: 2; vector-effect: non-scaling-stroke; opacity: 0.55; }
  .ymax { position: absolute; top: 4px; left: 8px; font-size: 0.75em; }
  .grid3 { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
  .stat { display: grid; gap: 4px; padding: 8px 10px; border-radius: 8px; background: var(--surface-2); }
  .nums { display: grid; gap: 2px; font-size: 0.9em; color: var(--muted); }
  .nums b { color: var(--text); font-variant-numeric: tabular-nums; }
  .nums b.mid { color: var(--warn); }
  .nums b.bad { color: var(--err); }
  .addr { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .addr > span:first-child { width: 11em; }
  code { padding: 4px 10px; border-radius: 6px; background: var(--console); border: 1px solid var(--line); font-family: var(--mono); user-select: text; }
  .note { display: grid; gap: 8px; padding: 10px 12px; border-radius: 8px; font-size: 0.92em; }
  .note.err { background: color-mix(in srgb, var(--warn) 12%, transparent); }
</style>
