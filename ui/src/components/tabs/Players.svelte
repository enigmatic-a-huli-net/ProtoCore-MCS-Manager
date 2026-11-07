<script lang="ts">
  import { onDestroy } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import Icon from "../Icon.svelte";
  import { api, type PlayerRow, type PlayersInfo, type ServerEntry } from "../../lib/api";
  import { app, status, t, toast, fail } from "../../lib/store.svelte";
  import type { Key } from "../../lib/i18n";

  let { server }: { server: ServerEntry } = $props();

  let info = $state<PlayersInfo | null>(null);
  let q = $state("");
  let open = $state("");
  let reason = $state("");
  let addName = $state("");
  let side = $state<"chat" | "audit">("chat");

  async function load() {
    try {
      info = await api.playersInfo(server.id);
    } catch (e) {
      fail(e);
    }
  }
  load();
  const timer = setInterval(() => !document.hidden && load(), 5000);
  onDestroy(() => clearInterval(timer));
  // сразу обновить, когда кто-то зашёл или вышел
  $effect(() => {
    status(server.id).players.length;
    load();
  });

  const running = $derived(info?.running ?? false);
  const shown = $derived((info?.rows ?? []).filter((r) => !q || r.name.toLowerCase().includes(q.toLowerCase())));
  const onlineN = $derived(info?.rows.filter((r) => r.online).length ?? 0);

  async function act(action: string, name: string, needReason = false) {
    if (action === "ban" && !(await ask(t("pl2.confirmBan", { name }), { kind: "warning" }))) return;
    try {
      await api.playerAction(server.id, action, name, needReason ? reason : "");
      reason = "";
      setTimeout(load, 700);
    } catch (e) {
      fail(e);
    }
  }

  async function addPlayer() {
    const n = addName.trim();
    if (!n) return;
    await act("whitelist_add", n);
    addName = "";
  }

  const ago = (sec: number) => {
    if (!sec) return "—";
    const d = Date.now() / 1000 - sec;
    const ru = app.config?.language !== "en";
    if (d < 3600) return ru ? `${Math.max(1, Math.round(d / 60))} мин назад` : `${Math.max(1, Math.round(d / 60))} min ago`;
    if (d < 86400) return ru ? `${Math.round(d / 3600)} ч назад` : `${Math.round(d / 3600)} h ago`;
    return new Date(sec * 1000).toLocaleDateString();
  };
  const hours = (r: PlayerRow) => {
    const s = r.total_secs + (r.online_since ? Date.now() / 1000 - r.online_since : 0);
    return s >= 3600 ? `${(s / 3600).toFixed(1)} ч` : `${Math.round(s / 60)} мин`;
  };
  const time = (sec: number) => new Date(sec * 1000).toLocaleString([], { hour: "2-digit", minute: "2-digit", day: "2-digit", month: "2-digit" });
  const head = "https://mc-heads.net/avatar/";
</script>

<div class="layout">
  <div class="main scroll">
    <div class="bar">
      <div class="search">
        <Icon name="search" size={14} />
        <input placeholder={t("mods.search")} bind:value={q} />
      </div>
      <span class="chip {onlineN ? 'ok' : ''}">{t("pl2.online", { n: onlineN })}</span>
      <span class="grow"></span>
      {#if info}
        <span class="muted small">{t("pl2.whitelist")}: <b>{info.lists.whitelist_on ? t("pl2.wlOn") : t("pl2.wlOff")}</b></span>
        <button class="btn sm" disabled={!running} onclick={() => act(info!.lists.whitelist_on ? "whitelist_off" : "whitelist_on", "")}>
          {info.lists.whitelist_on ? t("pl2.turnOff") : t("pl2.turnOn")}
        </button>
      {/if}
    </div>
    {#if !running}<p class="muted small note">{t("pl2.notRunning")}</p>{/if}

    <form class="add" onsubmit={(e) => { e.preventDefault(); addPlayer(); }}>
      <input placeholder={t("pl2.add")} bind:value={addName} disabled={!running} maxlength="17" />
      <button class="btn sm" type="submit" disabled={!running || !addName.trim()}><Icon name="plus" size={13} />{t("pl2.wlAdd")}</button>
    </form>

    {#if info && !info.rows.length}<div class="empty">{t("pl2.empty")}</div>{/if}

    <div class="list">
      {#each shown as r (r.name)}
        <div class="item" class:expanded={open === r.name}>
          <button class="row1" onclick={() => (open = open === r.name ? "" : r.name)}>
            <img src={`${head}${encodeURIComponent(r.name)}/32`} alt="" width="32" height="32" loading="lazy" onerror={(e) => ((e.currentTarget as HTMLImageElement).style.visibility = "hidden")} />
            <span class="dot" class:on={r.online}></span>
            <span class="name">{r.name}</span>
            {#if r.op}<span class="chip accent">OP</span>{/if}
            {#if r.whitelisted}<span class="chip">WL</span>{/if}
            {#if r.banned}<span class="chip err">ban</span>{/if}
            <span class="grow"></span>
            <span class="muted small">{r.online ? t("pl2.now") : t("pl2.seen", { t: ago(r.last_seen) })}</span>
            <span class="muted small w">{t("pl2.time", { h: hours(r) })}</span>
          </button>
          {#if open === r.name}
            <div class="details">
              <div class="muted small">
                {t("pl2.sessions", { n: r.sessions })}
                {#if r.ips.length} · IP: <span class="mono">{r.ips.join(", ")}</span>{/if}
              </div>
              <div class="row wrap">
                {#if r.op}
                  <button class="btn sm" disabled={!running} onclick={() => act("deop", r.name)}>{t("pl2.deop")}</button>
                {:else}
                  <button class="btn sm" disabled={!running} onclick={() => act("op", r.name)}>{t("pl2.op")}</button>
                {/if}
                {#if r.whitelisted}
                  <button class="btn sm" disabled={!running} onclick={() => act("whitelist_remove", r.name)}>{t("pl2.wlRemove")}</button>
                {:else}
                  <button class="btn sm" disabled={!running} onclick={() => act("whitelist_add", r.name)}>{t("pl2.wlAdd")}</button>
                {/if}
                {#if r.banned}
                  <button class="btn sm" disabled={!running} onclick={() => act("pardon", r.name)}>{t("pl2.pardon")}</button>
                {/if}
              </div>
              {#if !r.banned}
                <div class="row">
                  <input class="grow" placeholder={t("pl2.reason")} bind:value={reason} maxlength="120" />
                  <button class="btn sm" disabled={!running || !r.online} onclick={() => act("kick", r.name, true)}>{t("pl2.kick")}</button>
                  <button class="btn sm danger" disabled={!running} onclick={() => act("ban", r.name, true)}>{t("pl2.ban")}</button>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>

    {#if info?.shared_ips.length}
      <section class="panel alts">
        <h3><Icon name="alert" size={14} />{t("pl2.alts")}</h3>
        {#each info.shared_ips as [ip, names]}
          <div class="small"><span class="mono muted">{ip}</span> — {names.join(", ")}</div>
        {/each}
      </section>
    {/if}
  </div>

  <aside class="side">
    <div class="seg">
      <button class:on={side === "chat"} onclick={() => (side = "chat")}>{t("pl2.chat")}</button>
      <button class:on={side === "audit"} onclick={() => (side = "audit")}>{t("pl2.audit")}</button>
    </div>
    <div class="feed scroll">
      {#if side === "chat"}
        {#each info?.chat ?? [] as c}
          <div class="msg"><span class="muted ts">{time(c.ts)}</span><b>{c.name}</b> {c.msg}</div>
        {:else}
          <p class="muted small">{t("pl2.chatEmpty")}</p>
        {/each}
      {:else}
        {#each info?.audit ?? [] as a}
          <div class="msg"><span class="muted ts">{time(a.ts)}</span><span class="chip">{t(`pl2.by.${a.by}` as Key) === `pl2.by.${a.by}` ? a.by : t(`pl2.by.${a.by}` as Key)}</span> <span class="mono">{a.cmd}</span></div>
        {:else}
          <p class="muted small">{t("pl2.auditEmpty")}</p>
        {/each}
      {/if}
    </div>
  </aside>
</div>

<style>
  .layout { flex: 1; min-height: 0; display: grid; grid-template-columns: 1fr 340px; }
  .main { padding: 12px 16px 18px; display: grid; align-content: start; gap: 10px; }
  .bar { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .search { position: relative; width: 220px; color: var(--muted); }
  .search :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); }
  .search input { padding-left: 30px; height: 2.1em; }
  .small { font-size: 0.86em; }
  .note { margin: 0; }
  .add { display: flex; gap: 8px; max-width: 460px; }
  .add input { height: 2.1em; }
  .list { display: grid; gap: 2px; }
  .item { border-radius: var(--radius); border: 1px solid transparent; }
  .item:hover, .item.expanded { background: var(--surface); border-color: var(--line); }
  .row1 { display: flex; align-items: center; gap: 10px; width: 100%; padding: 6px 12px; border: 0; background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .row1 img { border-radius: 4px; image-rendering: pixelated; background: var(--surface-2); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--line); flex: none; }
  .dot.on { background: var(--ok); }
  .name { font-weight: 550; }
  .w { min-width: 7.5em; text-align: right; }
  .details { display: grid; gap: 8px; padding: 0 12px 12px 64px; }
  .details input { height: 2em; }
  .alts { padding: 10px 14px; display: grid; gap: 4px; }
  .alts h3 { display: flex; align-items: center; gap: 6px; font-size: 0.92em; color: var(--warn); }
  .side { display: flex; flex-direction: column; min-height: 0; border-left: 1px solid var(--line); padding: 12px; gap: 8px; }
  .seg { display: flex; padding: 3px; background: var(--surface-2); border-radius: calc(var(--radius) * 0.8); }
  .seg button { flex: 1; padding: 4px 10px; border: 0; border-radius: calc(var(--radius) * 0.6); background: none; color: var(--muted); font: inherit; font-size: 0.9em; cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.15); }
  .feed { flex: 1; display: grid; align-content: start; gap: 4px; font-size: 0.88em; }
  .msg { overflow-wrap: anywhere; user-select: text; }
  .ts { font-size: 0.85em; margin-right: 6px; font-variant-numeric: tabular-nums; }
  @media (max-width: 1100px) { .layout { grid-template-columns: 1fr; } .side { display: none; } }
</style>
