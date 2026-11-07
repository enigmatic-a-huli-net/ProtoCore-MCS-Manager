<script lang="ts">
  import Icon from "../Icon.svelte";
  import Hint from "../Hint.svelte";
  import { api, type PreCheck, type ServerEntry } from "../../lib/api";
  import { app, status, t, toast, fail, type Tab } from "../../lib/store.svelte";

  let { server }: { server: ServerEntry } = $props();
  let checks = $state<PreCheck[] | null>(null);
  let busy = $state(false);
  let jdl = $state(false);

  async function run() {
    busy = true;
    try {
      checks = await api.preflight(server.id);
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }
  run();
  // перепроверить после остановки/запуска
  let last = "";
  $effect(() => {
    const s = status(server.id).status;
    if (last && s !== last && (s === "stopped" || s === "running")) run();
    last = s;
  });

  async function doFix(c: PreCheck) {
    const [kind, arg] = c.fix.split(":");
    if (kind === "open_tab") {
      app.tab = (arg === "props" ? "properties" : arg) as Tab;
    } else if (kind === "download_java") {
      jdl = true;
      try {
        const j = await api.javaDownload(Number(arg));
        toast(t("jv.downloaded", { v: j.version }), "ok");
        await run();
      } catch (e) {
        fail(e);
      } finally {
        jdl = false;
      }
    }
  }
  const bad = $derived(checks?.filter((c) => c.level !== "ok") ?? []);
  const fixLabel = (c: PreCheck) => (c.fix.startsWith("download_java") ? t("jv.download", { v: c.fix.split(":")[1] }) : t("pf.open"));
</script>

<section class="panel pf">
  <div class="row">
    <h3 class="grow">{t("pf.title")}<Hint text={t("pf.hint")} /></h3>
    <button class="btn sm ghost" disabled={busy} onclick={run}><Icon name="restart" size={13} />{t("pf.again")}</button>
  </div>
  {#if checks}
    <div class="items">
      {#each checks as c (c.id + c.title)}
        <div class="it {c.level}">
          <span class="ic">
            {#if c.level === "ok"}<Icon name="check" size={13} />{:else}<Icon name="alert" size={13} />{/if}
          </span>
          <div class="grow">
            <b>{c.title}</b>
            {#if c.detail && c.level !== "ok"}<p>{c.detail}</p>{/if}
          </div>
          {#if c.fix && c.level !== "ok"}
            <button class="btn sm" disabled={jdl} onclick={() => doFix(c)}>
              {jdl && c.fix.startsWith("download_java") ? (app.javaProgress?.total ? `${Math.round((app.javaProgress.done / app.javaProgress.total) * 100)}%` : "…") : fixLabel(c)}
            </button>
          {/if}
        </div>
      {/each}
    </div>
    {#if !bad.length}<p class="muted small">{t("pf.allGood")}</p>{/if}
  {/if}
</section>

<style>
  .pf { padding: 12px 16px; display: grid; gap: 8px; }
  h3 { font-size: 0.98em; display: flex; align-items: center; }
  .items { display: flex; flex-wrap: wrap; gap: 6px 18px; }
  .it { display: flex; align-items: flex-start; gap: 8px; font-size: 0.9em; }
  .it.error, .it.warn { flex-basis: 100%; padding: 8px 10px; border-radius: 8px; }
  .it.error { background: color-mix(in srgb, var(--err) 12%, transparent); }
  .it.warn { background: color-mix(in srgb, var(--warn) 12%, transparent); }
  .it p { margin: 2px 0 0; color: var(--muted); max-width: 90ch; }
  .ic { margin-top: 2px; display: inline-flex; }
  .ok .ic { color: var(--ok); }
  .error .ic { color: var(--err); }
  .warn .ic { color: var(--warn); }
  .small { font-size: 0.86em; margin: 0; }
</style>
