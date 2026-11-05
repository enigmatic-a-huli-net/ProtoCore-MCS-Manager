<script lang="ts">
  import { onMount } from "svelte";
  import { app, init, selected, fail, t } from "./lib/store.svelte";
  import { logo } from "./lib/brand";
  import TitleBar from "./components/TitleBar.svelte";
  import Splash from "./components/Splash.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import ServerView from "./components/ServerView.svelte";
  import Settings from "./components/Settings.svelte";
  import AddServer from "./components/AddServer.svelte";
  import EulaDialog from "./components/EulaDialog.svelte";
  import Toasts from "./components/Toasts.svelte";

  let splashDone = $state(false);

  onMount(() => {
    const minShow = new Promise((r) => setTimeout(r, 700));
    Promise.all([init(), minShow])
      .catch(fail)
      .finally(() => (splashDone = true));
  });

  const server = $derived(selected());
</script>

{#if !splashDone || !app.ready}
  <Splash />
{:else}
  <div class="shell">
    <TitleBar />
    <div class="body">
      <Sidebar />
      <main>
        {#if app.view === "settings"}
          <Settings />
        {:else if server}
          {#key server.id}
            <ServerView {server} />
          {/key}
        {:else}
          <div class="welcome">
            <img src={logo()} alt="" width="88" height="88" />
            <h1>ProtoCore-MCS-Manager</h1>
            <p class="muted">{t("side.empty")}</p>
            <button class="btn primary" onclick={() => (app.addOpen = true)}>{t("side.add")}</button>
          </div>
        {/if}
      </main>
    </div>
  </div>
{/if}

{#if app.addOpen}<AddServer />{/if}
{#if app.eulaFor}<EulaDialog />{/if}
<Toasts />

<style>
  .shell { display: grid; grid-template-rows: auto 1fr; height: 100%; }
  .body { display: grid; grid-template-columns: 252px 1fr; min-height: 0; }
  main { min-width: 0; min-height: 0; display: flex; flex-direction: column; }
  .welcome { margin: auto; display: grid; justify-items: center; gap: 0.9em; text-align: center; max-width: 30em; padding: 2em; }
  .welcome h1 { font-size: 1.6em; }
  .welcome p { margin: 0; }
</style>
