<script lang="ts">
  // Своя полоса заголовка (окно без системной рамки). Тянуть окно можно за пустое место.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Icon from "./Icon.svelte";
  import { t } from "../lib/store.svelte";
  import { logo } from "../lib/brand";

  const win = getCurrentWindow();
</script>

<header class="tb" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <img src={logo()} alt="" width="18" height="18" />
    <span data-tauri-drag-region>ProtoCore <b>MCS</b> Manager</span>
  </div>
  <div class="ctl">
    <button title={t("tb.minimize")} onclick={() => win.minimize()}><Icon name="min" size={14} /></button>
    <button title={t("tb.maximize")} onclick={() => win.toggleMaximize()}><Icon name="max" size={12} /></button>
    <button class="close" title={t("tb.close")} onclick={() => win.close()}><Icon name="x" size={14} /></button>
  </div>
</header>

<style>
  .tb {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 36px;
    padding-left: 14px;
    background: var(--surface);
    border-bottom: 1px solid var(--line);
    user-select: none;
  }
  .brand { display: flex; align-items: center; gap: 9px; font-size: 0.88em; color: var(--muted); }
  .brand b { color: var(--text); font-weight: 600; }
  .ctl { display: flex; height: 100%; }
  .ctl button {
    width: 46px;
    height: 100%;
    display: grid;
    place-items: center;
    border: 0;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .ctl button:hover { background: var(--hover); color: var(--text); }
  .ctl .close:hover { background: #e81123; color: #fff; }
</style>
