<script lang="ts">
  // Экран загрузки: анимация логотипа (видео с прозрачностью) на всю ширину окна.
  // Своя анимация: положи branding/splash.gif рядом с .exe.
  import { app, t } from "../lib/store.svelte";
  import { logo } from "../lib/brand";
  import video from "../assets/splash.webm";

  let failed = $state(false);
  // без анимаций (настройка или «уменьшить движение» в Windows) — статичный логотип
  const still = $derived(
    app.config?.theme.animations === false ||
      (!app.config?.theme.anim_force && typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches),
  );
</script>

<div class="splash" data-tauri-drag-region>
  {#if app.branding.splash}
    <img class="custom" src={app.branding.splash} alt="" />
  {:else if !still && !failed}
    <!-- ширина всегда по окну, верх и низ могут обрезаться -->
    <video class="anim" src={video} autoplay muted playsinline preload="auto" onerror={() => (failed = true)}></video>
  {:else}
    <div class="core">
      <img src={logo()} alt="" width="96" height="96" />
      <span class="ring"></span>
    </div>
  {/if}
  <p>{t("app.loading")}</p>
</div>

<style>
  .splash {
    position: relative;
    height: 100%;
    overflow: hidden;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 22px;
    background: var(--bg);
    color: var(--muted);
  }
  .anim {
    position: absolute;
    left: 0;
    top: 50%;
    width: 100%;
    height: auto;
    transform: translateY(-50%);
    pointer-events: none;
  }
  .splash p { position: absolute; left: 0; right: 0; bottom: 28px; text-align: center; }
  .custom { max-width: 320px; max-height: 240px; }
  .core { position: relative; width: 132px; height: 132px; display: grid; place-items: center; }
  .ring {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    border: 2px solid transparent;
    border-top-color: var(--accent);
    border-right-color: var(--accent-line);
    animation: spin calc(2.2s * var(--anim-k)) linear infinite;
  }
  p { margin: 0; font-size: 0.9em; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
