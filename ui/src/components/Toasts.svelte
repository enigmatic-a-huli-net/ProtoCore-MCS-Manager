<script lang="ts">
  import Icon from "./Icon.svelte";
  import { app } from "../lib/store.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as x (x.id)}
    <div class="toast {x.kind}">
      <Icon name={x.kind === "error" ? "alert" : x.kind === "ok" ? "check" : "chevron"} size={15} />
      <span>{x.text}</span>
    </div>
  {/each}
</div>

<style>
  .toasts { position: fixed; right: 18px; bottom: 18px; z-index: 60; display: grid; gap: 8px; max-width: 420px; }
  .toast {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 10px 14px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-left: 3px solid var(--info);
    border-radius: var(--radius);
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.25);
    font-size: 0.92em;
    animation: slide 0.18s ease-out;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .toast :global(svg) { margin-top: 2px; }
  .toast.ok { border-left-color: var(--ok); }
  .toast.ok :global(svg) { color: var(--ok); }
  .toast.error { border-left-color: var(--err); }
  .toast.error :global(svg) { color: var(--err); }
  @keyframes slide { from { opacity: 0; transform: translateX(12px); } }
</style>
