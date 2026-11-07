<script lang="ts">
  // Значок «?» с пояснением: при наведении или фокусе показывает текст.
  let { text }: { text: string } = $props();
  let open = $state(false);
</script>

<span class="hint" role="note">
  <button
    type="button"
    class="q"
    aria-label={text}
    onmouseenter={() => (open = true)}
    onmouseleave={() => (open = false)}
    onfocus={() => (open = true)}
    onblur={() => (open = false)}
    onclick={(e) => { e.preventDefault(); e.stopPropagation(); open = !open; }}>?</button>
  {#if open}<span class="pop">{text}</span>{/if}
</span>

<style>
  .hint { position: relative; display: inline-flex; vertical-align: middle; margin-left: 4px; }
  .q {
    width: 16px; height: 16px; padding: 0; display: grid; place-items: center;
    border-radius: 50%; border: 1px solid var(--line); background: var(--surface-2);
    color: var(--muted); font: 600 10px/1 var(--font, inherit); cursor: help;
  }
  .q:hover, .q:focus-visible { color: var(--text); border-color: var(--accent); outline: none; }
  .pop {
    position: absolute; z-index: 40; top: calc(100% + 6px); left: -8px;
    width: max-content; max-width: 320px; padding: 8px 10px;
    background: var(--surface); border: 1px solid var(--line); border-radius: 8px;
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.3);
    color: var(--text); font-size: 12.5px; font-weight: 400; line-height: 1.45; white-space: normal; text-transform: none; letter-spacing: 0;
  }
</style>
