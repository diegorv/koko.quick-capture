<script lang="ts">
  // Small colored dot used wherever a Destination is rendered (Settings
  // list, picker results, Archive filter chips, Archive row pills).
  // A null `color` renders a dashed-outline placeholder so the dot
  // always reserves the same space regardless of whether the user
  // picked a swatch.
  import { colorHex } from "./palette";

  interface Props {
    color: string | null | undefined;
    /** Pixel size override. Defaults to the standard 0.7rem dot used
     * across the app — the Archive chip variant passes 0.6rem so the
     * rounded-pill chip stays compact. */
    size?: string;
  }

  const { color, size = "0.7rem" }: Props = $props();
  const hex = $derived(colorHex(color));
</script>

{#if hex}
  <span
    class="dot"
    style="background-color: {hex}; width: {size}; height: {size};"
    aria-hidden="true"
  ></span>
{:else}
  <span
    class="dot dot-empty"
    style="width: {size}; height: {size};"
    aria-hidden="true"
  ></span>
{/if}

<style>
  .dot {
    display: inline-block;
    border-radius: 999px;
    flex-shrink: 0;
  }
  .dot-empty {
    border: 1px dashed rgba(0, 0, 0, 0.2);
    background: transparent;
  }
  @media (prefers-color-scheme: dark) {
    .dot-empty {
      border-color: rgba(255, 255, 255, 0.2);
    }
  }
</style>
