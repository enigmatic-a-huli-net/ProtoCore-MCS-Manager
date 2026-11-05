<script lang="ts">
  import Icon from "../Icon.svelte";
  import { api, type Advice, type ServerEntry } from "../../lib/api";
  import { app, status, t, toast, fail, saveServer, type Tab } from "../../lib/store.svelte";

  let { server }: { server: ServerEntry } = $props();

  let list = $state<Advice[] | null>(null);
  let loading = $state(false);
  let busy = $state("");

  async function load() {
    loading = true;
    try {
      list = await api.advice(server.id);
    } catch (e) {
      fail(e);
    } finally {
      loading = false;
    }
  }
  load();

  const running = $derived(status(server.id).status === "running");

  async function act(a: Advice) {
    const x = a.action;
    if (!x) return;
    busy = a.id;
    try {
      switch (x.kind) {
        case "install_mod": {
          const plan = await api.catPlan(server.id, "mod", "modrinth", x.slug);
          app.plan = { id: server.id, kind: "mod", plan, onDone: load };
          break;
        }
        case "set_props":
          await api.writeProps(server.id, x.changes);
          toast(t("pr.saved"), "ok");
          await load();
          break;
        case "set_memory":
          await saveServer({ ...server, launch: { ...server.launch, memory_mb: x.mb } });
          toast(t("ln.saved"), "ok");
          await load();
          break;
        case "set_preset":
          await saveServer({ ...server, launch: { ...server.launch, jvm_preset: x.preset } });
          toast(t("ln.saved"), "ok");
          await load();
          break;
        case "command":
          if (!running) {
            toast(t("op.needRun"), "info");
            break;
          }
          await api.send(server.id, x.cmd);
          toast(t("op.sent", { c: x.cmd }), "ok");
          break;
        case "open_tab":
          app.tab = x.tab as Tab;
          break;
      }
    } catch (e) {
      fail(e);
    } finally {
      busy = "";
    }
  }

  const icon = (l: string) => (l === "warn" ? "alert" : l === "ok" ? "check" : "rocket");
</script>

<div class="wrapx scroll">
  <div class="bar">
    <h3 class="grow">{t("op.title")}</h3>
    <button class="btn sm" disabled={loading} onclick={load}><Icon name="restart" size={14} />{t("op.refresh")}</button>
  </div>

  {#if list && !list.length}
    <div class="empty">{t("op.empty")}</div>
  {/if}

  {#each list ?? [] as a (a.id)}
    <article class="card {a.level}">
      <Icon name={icon(a.level)} size={18} />
      <div class="txt">
        <b>{a.title}</b>
        {#if a.detail}<p>{a.detail}</p>{/if}
      </div>
      {#if a.action}
        <button class="btn sm {a.level === 'warn' ? 'primary' : ''}" disabled={busy === a.id} onclick={() => act(a)}>{a.action_label}</button>
      {/if}
    </article>
  {/each}
</div>

<style>
  .wrapx { flex: 1; padding: 14px 16px 20px; display: grid; align-content: start; gap: 8px; max-width: 900px; }
  .bar { display: flex; align-items: center; gap: 10px; margin-bottom: 4px; }
  .card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-left: 3px solid var(--info);
    border-radius: var(--radius);
  }
  .card.warn { border-left-color: var(--warn); }
  .card.warn > :global(svg) { color: var(--warn); }
  .card.tip > :global(svg) { color: var(--accent); }
  .card.ok { border-left-color: var(--ok); background: transparent; }
  .card.ok > :global(svg) { color: var(--ok); }
  .txt { flex: 1; min-width: 0; display: grid; gap: 3px; }
  .txt p { margin: 0; color: var(--muted); font-size: 0.9em; max-width: 80ch; }
</style>
