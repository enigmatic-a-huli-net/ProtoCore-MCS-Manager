<script lang="ts">
  // Цвет: образец (открывает палитру) + поле для кода (#rgb, #rrggbb, #rrggbbaa).
  import { normHex } from "../lib/theme";
  import Icon from "./Icon.svelte";

  let { value = $bindable(""), placeholder = "", label = "", allowEmpty = true }: { value: string; placeholder?: string; label?: string; allowEmpty?: boolean } = $props();
  let draft = $state(value);
  let bad = $state(false);
  $effect(() => {
    // внешние изменения (пресет, сброс) — обновить поле
    draft = value;
    bad = false;
  });

  function commit() {
    if (!draft.trim() && allowEmpty) {
      value = "";
      bad = false;
      return;
    }
    const h = normHex(draft);
    bad = !h;
    if (h) value = h;
  }
  // input[type=color] знает только #rrggbb
  const swatch = $derived((normHex(value) ?? normHex(placeholder) ?? "#808080").slice(0, 7));
</script>

<div class="ci" class:bad>
  <label class="sw" style:background-color={value || placeholder || "transparent"} title={label}>
    <input type="color" value={swatch} oninput={(e) => (value = (e.currentTarget as HTMLInputElement).value)} aria-label={label} />
  </label>
  <input
    class="hex mono"
    bind:value={draft}
    placeholder={placeholder || "#rrggbb"}
    maxlength="9"
    spellcheck="false"
    onkeydown={(e) => e.key === "Enter" && commit()}
    onblur={commit}
    aria-label={label}
  />
  {#if allowEmpty}
    <button type="button" class="btn ghost sm icon" style:visibility={value ? "visible" : "hidden"} onclick={() => (value = "")} aria-label="reset" title="reset"><Icon name="x" size={12} /></button>
  {/if}
</div>

<style>
  .ci { display: inline-flex; align-items: center; gap: 6px; }
  .sw { position: relative; width: 26px; height: 26px; flex: none; border-radius: 50%; border: 1px solid var(--line); cursor: pointer; overflow: hidden; }
  .sw input { position: absolute; inset: 0; opacity: 0; cursor: pointer; }
  .hex { width: 7.4em; height: 1.9em; padding: 0 8px; text-transform: lowercase; }
  .bad .hex { border-color: var(--err); }
</style>
