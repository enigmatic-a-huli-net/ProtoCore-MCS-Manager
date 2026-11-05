<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api } from "../lib/api";
  import { app, t, fail, startServer } from "../lib/store.svelte";

  async function accept() {
    const id = app.eulaFor;
    app.eulaFor = "";
    try {
      await api.acceptEula(id);
      await startServer(id);
    } catch (e) {
      fail(e);
    }
  }
</script>

<div class="backdrop">
  <div class="modal" role="alertdialog" aria-modal="true" aria-label={t("eula.title")}>
    <h2>{t("eula.title")}</h2>
    <p class="muted">{t("eula.text")}</p>
    <div class="actions">
      <button class="btn ghost" onclick={() => openUrl("https://aka.ms/MinecraftEULA").catch(fail)}>{t("eula.read")}</button>
      <span class="grow"></span>
      <button class="btn" onclick={() => (app.eulaFor = "")}>{t("act.cancel")}</button>
      <button class="btn primary" onclick={accept}>{t("eula.accept")}</button>
    </div>
  </div>
</div>

<style>
  p { margin: 0; }
</style>
