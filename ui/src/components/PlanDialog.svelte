<script lang="ts">
  // Подтверждение установки: что будет скачано, какие зависимости добавлены, что заменится.
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Icon from "./Icon.svelte";
  import { api, bytes } from "../lib/api";
  import { app, status, t, toast, fail } from "../lib/store.svelte";
  import type { Key } from "../lib/i18n";

  const p = app.plan!;
  let working = $state(false);
  const downloadable = p.plan.items.filter((i) => i.file.url);

  async function install() {
    working = true;
    app.catalogProgress = null;
    try {
      const files = await api.catApply(p.id, p.kind, p.plan);
      const live = !["stopped", "crashed"].includes(status(p.id).status);
      toast(t("plan.done", { n: files.length }) + (live ? `\n${t("plan.restart")}` : ""), "ok");
      p.onDone?.();
      app.plan = null;
    } catch (e) {
      fail(e);
    } finally {
      working = false;
    }
  }

  const close = () => !working && (app.plan = null);
  const pct = $derived(
    app.catalogProgress && app.catalogProgress.total ? Math.round((app.catalogProgress.done / app.catalogProgress.total) * 100) : 0,
  );
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="modal wide" role="dialog" aria-modal="true" aria-label={t("plan.title")}>
    <h2>{t("plan.title")}</h2>

    {#if !p.plan.items.length}
      <p class="muted">{t("plan.empty")}</p>
    {/if}

    <div class="items">
      {#each p.plan.items as it}
        <div class="it">
          {#if it.icon_url}<img src={it.icon_url} alt="" width="32" height="32" />{:else}<span class="av">{it.title.slice(0, 2)}</span>{/if}
          <div class="txt">
            <div class="row">
              <button class="title" onclick={() => openUrl(it.file.page_url).catch(fail)} title={t("cat.open")}>{it.title}</button>
              <span class="chip {it.reason === 'dependency' ? 'accent' : it.reason === 'selected' ? 'ok' : 'warn'}">{t(`plan.reason.${it.reason}` as Key)}</span>
              {#if !it.file.url}<span class="chip err">{t("plan.manual")}</span>{/if}
            </div>
            <span class="muted small mono">{it.file.version_number} · {it.file.file_name}{it.file.size ? ` · ${bytes(it.file.size)}` : ""}</span>
            {#if it.replaces}<span class="muted small">{t("plan.replaces", { f: it.replaces })}</span>{/if}
          </div>
          {#if !it.file.url && it.file.page_url}
            <button class="btn sm" onclick={() => openUrl(it.file.page_url).catch(fail)}>{t("cat.open")}</button>
          {/if}
        </div>
      {/each}
    </div>

    {#if p.plan.already.length}
      <div class="note">
        <span class="muted small">{t("plan.already")}</span>
        <div class="small">{p.plan.already.join(", ")}</div>
      </div>
    {/if}
    {#if p.plan.warnings.length}
      <div class="note warn">
        <span class="small"><Icon name="alert" size={13} /> {t("plan.warnings")}</span>
        {#each p.plan.warnings as w}<div class="small">{w}</div>{/each}
      </div>
    {/if}

    {#if working}
      <div class="bar"><div style:width={`${pct}%`}></div></div>
      <span class="muted small">{app.catalogProgress?.title ?? ""} {pct ? `${pct}%` : ""}</span>
    {/if}

    <div class="actions">
      <button class="btn" disabled={working} onclick={close}>{t("act.cancel")}</button>
      <button class="btn primary" disabled={working || !downloadable.length} onclick={install}>
        <Icon name="download" size={14} />{t("plan.install", { n: downloadable.length })}
      </button>
    </div>
  </div>
</div>

<style>
  .wide { width: min(640px, calc(100vw - 40px)); }
  .items { display: grid; gap: 6px; max-height: 340px; overflow: auto; }
  .it { display: flex; align-items: center; gap: 12px; padding: 8px 10px; border-radius: 8px; background: var(--bg); }
  .it img { border-radius: 6px; }
  .av { width: 32px; height: 32px; display: grid; place-items: center; border-radius: 6px; background: var(--surface-2); color: var(--muted); font-weight: 700; font-size: 0.8em; flex: none; }
  .txt { display: grid; gap: 2px; min-width: 0; flex: 1; }
  .small { font-size: 0.85em; }
  .title { padding: 0; border: 0; background: none; color: var(--text); font: inherit; font-weight: 600; cursor: pointer; text-align: left; }
  .title:hover { color: var(--accent); text-decoration: underline; }
  .note { display: grid; gap: 4px; padding: 8px 12px; border-radius: 8px; background: var(--surface-2); }
  .note.warn { background: color-mix(in srgb, var(--warn) 12%, transparent); color: var(--warn); }
  .bar { height: 6px; border-radius: 6px; background: var(--surface-2); overflow: hidden; }
  .bar div { height: 100%; background: var(--accent); transition: width 0.2s; }
</style>
