<script lang="ts">
  import Icon from "./Icon.svelte";
  import { api, DEFAULT_NOTIFY, type NotifySettings, type Theme } from "../lib/api";
  import { app, t, toast, fail } from "../lib/store.svelte";
  import { applyTheme, ACCENTS, DEFAULT_THEME } from "../lib/theme";
  import { logo } from "../lib/brand";

  const c = app.config!;
  let lang = $state(c.language);
  let theme = $state<Theme>({ ...DEFAULT_THEME, ...c.theme });
  let tray = $state(c.close_to_tray);
  let lines = $state(c.console_lines);
  let checkUpdates = $state(c.check_updates ?? true);
  let cfKey = $state(c.curseforge_key ?? "");
  let serversRoot = $state(c.servers_root ?? "");
  let netmon = $state(c.network_monitor ?? true);
  let nt = $state<NotifySettings>({ ...DEFAULT_NOTIFY, ...(c.notify ?? {}) });
  let testing = $state(false);
  let admins = $state((nt.telegram_admins ?? []).join(", "));
  $effect(() => {
    nt.telegram_admins = admins.split(/[,\s]+/).map((x) => x.trim()).filter((x) => /^-?\d+$/.test(x));
  });
  let finding = $state(false);
  async function findChat() {
    finding = true;
    try {
      const c = await api.telegramFindChat(nt.telegram_token);
      nt.telegram_chat = c.id;
      toast(t("nt.found", { name: c.name || c.id }), "ok");
    } catch (e) {
      fail(e);
    } finally {
      finding = false;
    }
  }
  async function testNotify() {
    testing = true;
    try {
      const ok = await api.notifyTest($state.snapshot(nt) as NotifySettings);
      toast(t("nt.testOk", { ok }), "ok");
    } catch (e) {
      fail(e);
    } finally {
      testing = false;
    }
  }

  // предпросмотр темы сразу, без сохранения
  $effect(() => applyTheme($state.snapshot(theme) as Theme));
  // язык меняется сразу
  $effect(() => {
    app.config!.language = lang;
  });

  async function checkCf() {
    try {
      await api.checkCfKey(cfKey);
      toast(t("set.cfOk"), "ok");
    } catch (e) {
      fail(e);
    }
  }

  async function save() {
    try {
      const settings = { language: lang, theme: $state.snapshot(theme) as Theme, close_to_tray: tray, console_lines: Number(lines), check_updates: checkUpdates, curseforge_key: cfKey, servers_root: serversRoot, network_monitor: netmon, notify: $state.snapshot(nt) as NotifySettings };
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
    {#if theme.animations}
      <div class="grid">
        <label class="field">
          <span>{t("set.animSpeed")} — ×{theme.anim_speed.toFixed(2)}</span>
          <div class="row small muted">
            {t("set.animSlow")}
            <input type="range" min="0.5" max="2" step="0.25" bind:value={theme.anim_speed} class="grow" />
            {t("set.animFast")}
          </div>
        </label>
        <label class="check" style="align-self:end"><input type="checkbox" bind:checked={theme.anim_force} />{t("set.animForce")}</label>
      </div>
    {/if}
    <div><button type="button" class="btn sm" onclick={reset}>{t("set.reset")}</button></div>
  </section>

  <section class="panel">
    <label class="check"><input type="checkbox" bind:checked={tray} />{t("set.tray")}</label>
    <label class="check"><input type="checkbox" bind:checked={checkUpdates} />{t("up.auto")}</label>
    <label class="check"><input type="checkbox" bind:checked={netmon} />{t("set.netmon")}</label>
    <div class="grid">
      <label class="field"><span>{t("set.serversRoot")}</span><input bind:value={serversRoot} placeholder="D:\\Minecraft" /></label>
      <label class="field">
        <span>{t("set.cfKey")}</span>
        <div class="row">
          <input type="password" bind:value={cfKey} autocomplete="off" />
          <button type="button" class="btn sm" disabled={!cfKey.trim()} onclick={checkCf}>{t("set.cfCheck")}</button>
        </div>
      </label>
    </div>
    <p class="muted small">{t("set.cfKeyHint")}</p>
    <label class="field narrow"><span>{t("set.lines")}</span><input type="number" min="500" max="50000" step="500" bind:value={lines} /></label>
  </section>

  <section class="panel">
    <h3>{t("nt.title")}</h3>
    <p class="muted small" style="margin:0">{t("nt.text")}</p>
    <div class="grid">
      <label class="field"><span>{t("nt.tgToken")}</span><input type="password" bind:value={nt.telegram_token} autocomplete="off" placeholder="123456:ABC…" /></label>
      <label class="field"><span>{t("nt.tgChat")}</span><div class="row"><input class="grow" bind:value={nt.telegram_chat} placeholder="123456789" /><button type="button" class="btn sm" disabled={finding || !nt.telegram_token.trim()} onclick={findChat}>{finding ? "…" : t("nt.find")}</button></div></label>
    </div>
    <p class="muted small" style="margin:0">{t("nt.tgTokenHint")} {t("nt.tgChatHint")}</p>
    <label class="check"><input type="checkbox" bind:checked={nt.telegram_bot} disabled={!nt.telegram_token || !nt.telegram_chat} />{t("nt.bot")}</label>
    {#if nt.telegram_bot}
      <label class="field"><span>{t("nt.admins")}</span><input bind:value={admins} placeholder="123456789, 987654321" /></label>
      <p class="muted small" style="margin:0">{t("nt.adminsHint")}</p>
    {/if}
    <label class="field"><span>{t("nt.discord")}</span><input type="password" bind:value={nt.discord_webhook} autocomplete="off" placeholder="https://discord.com/api/webhooks/…" /></label>
    <p class="muted small" style="margin:0">{t("nt.discordHint")}</p>
    <span class="muted small">{t("nt.events")}</span>
    <div class="events">
      <label class="check"><input type="checkbox" bind:checked={nt.on_crash} />{t("nt.crash")}</label>
      <label class="check"><input type="checkbox" bind:checked={nt.on_backup_fail} />{t("nt.backupFail")}</label>
      <label class="check"><input type="checkbox" bind:checked={nt.on_start} />{t("nt.start")}</label>
      <label class="check"><input type="checkbox" bind:checked={nt.on_stop} />{t("nt.stop")}</label>
      <label class="check"><input type="checkbox" bind:checked={nt.on_join} />{t("nt.join")}</label>
      <label class="check"><input type="checkbox" bind:checked={nt.on_net_bad} />{t("nt.net")}</label>
      <label class="check num"><input type="checkbox" bind:checked={nt.on_low_tps} />{t("nt.lowTps")} <input type="number" min="5" max="19" step="1" bind:value={nt.low_tps} /></label>
      <label class="check num"><input type="checkbox" bind:checked={nt.on_disk_low} />{t("nt.disk")} <input type="number" min="1" max="500" bind:value={nt.disk_low_gb} /></label>
    </div>
    <div class="row wrap quiet">
      <span class="muted small">{t("nt.quiet")}</span>
      <input type="time" bind:value={nt.quiet_from} /> — <input type="time" bind:value={nt.quiet_to} />
      <label class="check"><input type="checkbox" bind:checked={nt.quiet_allow_crash} />{t("nt.quietCrash")}</label>
    </div>
    <label class="check num"><span class="muted small">{t("nt.minInterval")}</span> <input type="number" min="0" max="1440" bind:value={nt.min_interval_min} /> <span class="muted small">{t("nt.min")}</span></label>
    <p class="muted small" style="margin:0">{t("nt.quietHint")}</p>
    <div><button type="button" class="btn sm" disabled={testing || (!nt.telegram_token && !nt.discord_webhook)} onclick={testNotify}><Icon name="chevron" size={13} />{t("nt.test")}</button></div>
  </section>

  <section class="panel brand">
    <img src={logo()} alt="" width="56" height="56" />
    <div class="grow">
      <h3>{t("set.branding")}</h3>
      <p class="muted">{t("set.brandingText")}</p>
    </div>
    <button type="button" class="btn sm" onclick={() => app.paths && api.openPath(app.paths.branding).catch(fail)}>
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
  .quiet { gap: 8px; align-items: center; }
  .quiet input[type="time"] { width: 7.5em; }
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
  .events { display: grid; grid-template-columns: 1fr 1fr; gap: 6px 16px; }
  .num input[type="number"] { width: 4.5em; height: 1.8em; margin-left: 4px; }
  input[type="range"] { accent-color: var(--accent); height: auto; padding: 0; border: 0; background: none; }
  .brand { display: flex; align-items: center; gap: 16px; }
  .brand p { margin: 4px 0 0; font-size: 0.9em; }
  .small { font-size: 0.85em; }
</style>
