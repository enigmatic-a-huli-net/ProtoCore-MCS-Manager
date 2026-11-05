<script lang="ts">
  // Экран загрузки. Своя анимация: положи branding/splash.gif рядом с .exe.
  import { app, t } from "../lib/store.svelte";
  import { logo } from "../lib/brand";
</script>

<div class="splash" data-tauri-drag-region>
  {#if app.branding.splash}
    <img class="custom" src={app.branding.splash} alt="" />
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
    height: 100%;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 22px;
    background: var(--bg);
    color: var(--muted);
  }
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
