<script lang="ts">
  // Palette picker row used by both DestinationsSection (Settings) and
  // DestinationPicker (inline create-mode). Renders the "no color"
  // square plus every PALETTE_KEYS swatch as clickable chips. The
  // currently-selected swatch is outlined.

  import { PALETTE_KEYS, colorHex, type PaletteKey } from "./palette";

  interface Props {
    selected: PaletteKey | null;
    onSelect: (color: PaletteKey | null) => void;
  }

  const { selected, onSelect }: Props = $props();
</script>

<div class="swatches">
  <button
    type="button"
    class="swatch swatch-none"
    class:selected={selected === null}
    aria-label="No color"
    onclick={() => onSelect(null)}
  ></button>
  {#each PALETTE_KEYS as key}
    <button
      type="button"
      class="swatch"
      class:selected={selected === key}
      style="background-color: {colorHex(key)};"
      aria-label={key}
      onclick={() => onSelect(key)}
    ></button>
  {/each}
</div>

<style>
  .swatches {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
  }
  .swatch {
    width: 1.2rem;
    height: 1.2rem;
    border-radius: 999px;
    border: 1px solid rgba(0, 0, 0, 0.12);
    cursor: pointer;
    padding: 0;
    transition: transform 80ms ease;
  }
  .swatch:hover {
    transform: scale(1.08);
  }
  .swatch.selected {
    outline: 2px solid currentColor;
    outline-offset: 1.5px;
  }
  .swatch-none {
    background:
      linear-gradient(
        45deg,
        rgba(0, 0, 0, 0.1) 0%,
        transparent 50%,
        rgba(0, 0, 0, 0.1) 100%
      );
    background-color: transparent !important;
  }
</style>
