<script lang="ts">
  import { openPath } from "@tauri-apps/plugin-opener";
  import Icon from "./Icon.svelte";
  import { api, type Theme } from "../lib/api";
  import { app, t, toast, fail } from "../lib/store.svelte";
  import { applyTheme, ACCENTS, DEFAULT_THEME } from "../lib/theme";
  import { logo } from "../lib/brand";

  const c = app.config!;
  let lang = $state(c.language);
  let theme = $state<Theme>({ ...c.theme });
  let tray = $state(c.close_to_tray);
  let lines = $state(c.console_lines);

  // предпросмотр темы сразу, без сохранения
  $effect(() => applyTheme($state.snapshot(theme) as Theme));
  // язык меняется сразу
  $effect(() => {
    app.config!.language = lang;
  });

  async function save() {
    try {
      const settings = { language: lang, theme: $state.snapshot(theme) as Theme, close_to_tray: tray, console_lines: Number(lines) };
      await api.saveSettings(settings);
      Object.assign(app.config!, settings);
      toast(t("set.saved"), "ok");
    } catch (e) {
      fail(e);
    }
  }

  function reset() {
    theme = { ...DEFAULT_THEME };
  }

  const colorFields = [
    ["background", "set.bg"],
    ["surface", "set.surface"],
    ["text", "set.text"],
  ] as const;
</script>

<div class="page scroll">
  <h1>{t("set.title")}</h1>

  <section class="panel">
    <div class="grid">
      <label class="field">
        <span>{t("set.language")}</span>
        <select bind:value={lang}>
          <option value="ru">Русский</option>
          <option value="en">English</option>
        </select>
      </label>
      <label class="field">
        <span>{t("set.theme")}</span>
        <div class="seg">
          {#each [["dark", "set.dark"], ["light", "set.light"], ["system", "set.system"]] as const as [m, k]}
            <button type="button" class:on={theme.mode === m} onclick={() => (theme.mode = m)}>{t(k)}</button>
          {/each}
        </div>
      </label>
    </div>

    <div class="field">
      <span>{t("set.accent")}</span>
      <div class="row">
        {#each ACCENTS as a}
          <button
            type="button"
            class="swatch"
            class:on={theme.accent.toLowerCase() === a}
            style:background={a}
            aria-label={a}
            onclick={() => (theme.accent = a)}
          ></button>
        {/each}
        <input type="color" bind:value={theme.accent} title={theme.accent} />
      </div>
    </div>

    <div class="field">
      <span>{t("set.custom")}</span>
      <div class="colors">
        {#each colorFields as [key, label]}
          <div class="row">
            <input type="color" value={theme[key] || "#808080"} oninput={(e) => (theme[key] = e.currentTarget.value)} />
            <span class="grow">{t(label)}</span>
            {#if theme[key]}<button type="button" class="btn ghost sm icon" onclick={() => (theme[key] = "")} aria-label="reset"><Icon name="x" size={13} /></button>{/if}
          </div>
        {/each}
      </div>
    </div>

    <div class="grid">
      <label class="field"><span>{t("set.radius")} — {theme.radius}</span><input type="range" min="0" max="20" bind:value={theme.radius} /></label>
      <label class="field"><span>{t("set.font")} — {theme.font_size}</span><input type="range" min="12" max="18" bind:value={theme.font_size} /></label>
    </div>
    <label class="check"><input type="checkbox" bind:checked={theme.animations} />{t("set.anim")}</label>
    <div><button type="button" class="btn sm" onclick={reset}>{t("set.reset")}</button></div>
  </section>

  <section class="panel">
    <label class="check"><input type="checkbox" bind:checked={tray} />{t("set.tray")}</label>
    <label class="field narrow"><span>{t("set.lines")}</span><input type="number" min="500" max="50000" step="500" bind:value={lines} /></label>
  </section>

  <section class="panel brand">
    <img src={logo()} alt="" width="56" height="56" />
    <div class="grow">
      <h3>{t("set.branding")}</h3>
      <p class="muted">{t("set.brandingText")}</p>
    </div>
    <button type="button" class="btn sm" onclick={() => app.paths && openPath(app.paths.branding).catch(fail)}>
      <Icon name="folder" size={14} />{t("set.openBranding")}
    </button>
  </section>

  <div class="row">
    <button class="btn primary" onclick={save}>{t("act.save")}</button>
    <span class="grow"></span>
    <span class="muted small">ProtoCore-MCS-Manager {app.version} · restic {app.restic.found ? app.restic.version.split(" ")[1] ?? "✓" : "—"}</span>
  </div>
</div>

<style>
  .page { flex: 1; padding: 22px 26px; display: grid; align-content: start; gap: 16px; max-width: 820px; }
  h1 { font-size: 1.35em; }
  section { padding: 18px 20px; display: grid; gap: 14px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px 18px; }
  .seg { display: flex; padding: 3px; background: var(--surface-2); border-radius: calc(var(--radius) * 0.8); width: max-content; }
  .seg button { padding: 5px 14px; border: 0; border-radius: calc(var(--radius) * 0.6); background: none; color: var(--muted); font: inherit; cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.15); }
  .swatch { width: 26px; height: 26px; border-radius: 50%; border: 2px solid transparent; cursor: pointer; outline-offset: 2px; }
  .swatch.on { border-color: var(--text); }
  .colors { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
  .narrow { max-width: 220px; }
  input[type="range"] { accent-color: var(--accent); height: auto; padding: 0; border: 0; background: none; }
  .brand { display: flex; align-items: center; gap: 16px; }
  .brand p { margin: 4px 0 0; font-size: 0.9em; }
  .small { font-size: 0.85em; }
</style>
