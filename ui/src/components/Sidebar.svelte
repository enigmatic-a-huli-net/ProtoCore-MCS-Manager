<script lang="ts">
  // Список серверов. Быстрое переключение: клик или Ctrl+1…9.
  import Icon from "./Icon.svelte";
  import { api, CORE_LABEL } from "../lib/api";
  import { app, selected, status, t, fail } from "../lib/store.svelte";

  function pick(id: string) {
    app.view = "server";
    if (app.config!.selected === id) return;
    app.config!.selected = id;
    api.selectServer(id).catch(fail);
  }

  function onKey(e: KeyboardEvent) {
    if (!e.ctrlKey || e.altKey || e.shiftKey) return;
    const n = Number(e.key);
    if (n >= 1 && n <= 9) {
      const s = app.config?.servers[n - 1];
      if (s) {
        e.preventDefault();
        pick(s.id);
      }
    }
  }

  // перетаскивание для смены порядка
  let dragId = $state("");
  function drop(target: string) {
    const list = app.config!.servers;
    const from = list.findIndex((s) => s.id === dragId);
    const to = list.findIndex((s) => s.id === target);
    if (from < 0 || to < 0 || from === to) return;
    const [m] = list.splice(from, 1);
    list.splice(to, 0, m);
    api.reorder(list.map((s) => s.id)).catch(fail);
  }

  const cur = $derived(selected());
</script>

<svelte:window onkeydown={onKey} />

<aside>
  <div class="head">
    <h3>{t("side.servers")}</h3>
    <button class="btn ghost icon sm" title={t("side.add")} onclick={() => (app.addOpen = true)}>
      <Icon name="plus" />
    </button>
  </div>

  <nav class="scroll">
    {#each app.config?.servers ?? [] as s, i (s.id)}
      {@const st = status(s.id)}
      <button
        class="srv"
        class:active={app.view === "server" && cur?.id === s.id}
        draggable="true"
        ondragstart={() => (dragId = s.id)}
        ondragover={(e) => e.preventDefault()}
        ondrop={() => drop(s.id)}
        onclick={() => pick(s.id)}
        title={`${s.path}${i < 9 ? `  (Ctrl+${i + 1})` : ""}`}
      >
        <span class="ava">
          {#if app.icons[s.id]}<img src={app.icons[s.id]} alt="" />{:else}<span class="ini">{CORE_LABEL[s.core].slice(0, 2)}</span>{/if}
          <span class="dot {st.status}"></span>
        </span>
        <span class="txt">
          <span class="name">{s.name}</span>
          <span class="meta">{CORE_LABEL[s.core]} {s.mc_version}</span>
        </span>
        {#if st.players.length}
          <span class="players" title={st.players.join(", ")}><Icon name="user" size={12} />{st.players.length}</span>
        {/if}
      </button>
    {:else}
      <p class="muted none">{t("side.empty")}</p>
    {/each}
  </nav>

  <button class="foot" class:active={app.view === "settings"} onclick={() => (app.view = "settings")}>
    <Icon name="settings" />
    {t("side.settings")}
  </button>
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--surface);
    border-right: 1px solid var(--line);
  }
  .head { display: flex; align-items: center; justify-content: space-between; padding: 14px 10px 6px 16px; }
  .head h3 { font-size: 0.85em; color: var(--muted); font-weight: 600; }
  nav { flex: 1; padding: 4px 8px; display: grid; align-content: start; gap: 2px; }
  .srv {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: calc(var(--radius) * 0.8);
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .srv:hover { background: var(--hover); }
  .srv.active { background: var(--accent-soft); }
  .srv.active .name { color: var(--accent); }
  .txt { display: grid; min-width: 0; flex: 1; }
  .name { font-weight: 550; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { font-size: 0.82em; color: var(--muted); }
  .players { display: inline-flex; align-items: center; gap: 3px; font-size: 0.8em; color: var(--ok); }
  .ava { position: relative; width: 30px; height: 30px; flex: none; border-radius: 50%; background: var(--surface-2); border: 1px solid var(--line); }
  .ava img { width: 100%; height: 100%; border-radius: 50%; object-fit: cover; display: block; }
  .ini { display: grid; place-items: center; width: 100%; height: 100%; font-size: 0.72em; font-weight: 700; color: var(--muted); }
  .ava .dot { position: absolute; right: -2px; bottom: -2px; width: 10px; height: 10px; border: 2px solid var(--bg); box-sizing: content-box; }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex: none; background: var(--line); }
  .dot.running { background: var(--ok); box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 22%, transparent); }
  .dot.starting, .dot.stopping { background: var(--warn); animation: blink calc(1.8s * var(--anim-k)) ease-in-out infinite; }
  .dot.crashed { background: var(--err); }
  .none { padding: 8px 10px; font-size: 0.9em; }
  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 8px;
    padding: 9px 10px;
    border: 0;
    border-radius: calc(var(--radius) * 0.8);
    background: transparent;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }
  .foot:hover { background: var(--hover); color: var(--text); }
  .foot.active { background: var(--accent-soft); color: var(--accent); }
  @keyframes blink { 50% { opacity: 0.35; } }
</style>
