<script lang="ts">
  import { tick } from "svelte";
  import Icon from "../Icon.svelte";
  import { api, type ServerEntry } from "../../lib/api";
  import { getLog, logVersion, status, t, fail, clearLocalLog } from "../../lib/store.svelte";

  let { server }: { server: ServerEntry } = $props();

  // В DOM держим не больше этого числа строк — остальное в памяти ядра.
  const RENDER_LIMIT = 1500;

  let filter = $state("");
  let onlyProblems = $state(false);
  let follow = $state(true);
  let input = $state("");
  let box: HTMLDivElement;

  const history: string[] = [];
  let hIdx = -1;

  const live = $derived(["running", "starting", "stopping"].includes(status(server.id).status));

  const lines = $derived.by(() => {
    logVersion(); // подписка на новые строки
    let all = getLog(server.id);
    if (onlyProblems) all = all.filter((l) => l.level === "warn" || l.level === "error" || l.level === "system");
    if (filter.trim()) {
      const f = filter.toLowerCase();
      all = all.filter((l) => l.text.toLowerCase().includes(f));
    }
    return all.length > RENDER_LIMIT ? all.slice(all.length - RENDER_LIMIT) : all;
  });

  $effect(() => {
    lines;
    if (follow) tick().then(() => box && (box.scrollTop = box.scrollHeight));
  });

  function onScroll() {
    const atBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 40;
    if (follow !== atBottom) follow = atBottom;
  }

  async function send() {
    const cmd = input.trim();
    if (!cmd) return;
    history.unshift(cmd);
    if (history.length > 100) history.pop();
    hIdx = -1;
    input = "";
    await api.send(server.id, cmd).catch(fail);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter") send();
    else if (e.key === "ArrowUp" && history.length) {
      hIdx = Math.min(hIdx + 1, history.length - 1);
      input = history[hIdx];
      e.preventDefault();
    } else if (e.key === "ArrowDown") {
      hIdx = Math.max(hIdx - 1, -1);
      input = hIdx < 0 ? "" : history[hIdx];
      e.preventDefault();
    }
  }

  function clear() {
    api.clearLog(server.id).catch(fail);
    clearLocalLog(server.id);
  }

  const time = (ts: number) => new Date(ts).toLocaleTimeString([], { hour12: false });
</script>

<div class="con">
  <div class="bar">
    <div class="search">
      <Icon name="search" size={14} />
      <input placeholder={t("con.filter")} bind:value={filter} />
    </div>
    <label class="check"><input type="checkbox" bind:checked={onlyProblems} />{t("con.onlyProblems")}</label>
    <label class="check"><input type="checkbox" bind:checked={follow} />{t("con.follow")}</label>
    <span class="grow"></span>
    <button class="btn ghost sm" onclick={clear}><Icon name="trash" size={14} />{t("con.clear")}</button>
  </div>

  <div class="console-body" bind:this={box} onscroll={onScroll}>
    {#each lines as l (l.seq)}
      <div class="ln {l.level}"><span class="ts">{time(l.ts)}</span>{l.text}</div>
    {:else}
      <div class="placeholder">{t("con.empty")}</div>
    {/each}
  </div>

  <div class="cmd">
    <span class="prompt">&gt;</span>
    <input
      bind:value={input}
      onkeydown={onKey}
      disabled={!live}
      placeholder={live ? t("con.placeholder") : t("con.notRunning")}
      spellcheck="false"
    />
  </div>
</div>

<style>
  .con { display: flex; flex-direction: column; flex: 1; min-height: 0; padding: 12px 16px 16px; gap: 10px; }
  .bar { display: flex; align-items: center; gap: 16px; font-size: 0.9em; }
  .search { position: relative; width: 260px; color: var(--muted); }
  .search :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); }
  .search input { padding-left: 30px; height: 2.1em; }

  .console-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 10px 12px;
    background: var(--console);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    font-family: var(--mono);
    font-size: 0.86em;
    line-height: 1.5;
    user-select: text;
  }
  .ln { white-space: pre-wrap; word-break: break-word; }
  .ts { color: var(--muted); opacity: 0.6; margin-right: 10px; user-select: none; }
  .ln.warn { color: var(--warn); }
  .ln.error { color: var(--err); }
  .ln.system { color: var(--accent); font-weight: 600; }
  .placeholder { color: var(--muted); padding: 1em 0; font-family: var(--font); }

  .cmd {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    background: var(--console);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }
  .cmd:focus-within { border-color: var(--accent); }
  .prompt { color: var(--accent); font-family: var(--mono); font-weight: 700; }
  .cmd input { border: 0; background: transparent; font-family: var(--mono); padding: 0; height: 2.6em; }
</style>
