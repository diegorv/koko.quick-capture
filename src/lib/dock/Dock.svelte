<script lang="ts">
  // Presentational Dock widget. Decoupled from Tauri so the unit tests
  // can mount it with plain `vi.fn()` callbacks. The route at
  // /dock wires `onComposer` to `invoke("open_composer_window")` and
  // `onContextMenu` to `invoke("open_dock_context_menu", { x, y })`.

  interface Props {
    onComposer: () => void;
    onContextMenu: (x: number, y: number) => void;
  }

  let { onComposer, onContextMenu }: Props = $props();

  function handleClick() {
    onComposer();
  }

  function handleContextMenu(event: MouseEvent) {
    event.preventDefault();
    onContextMenu(event.clientX, event.clientY);
  }
</script>

<button
  type="button"
  class="dock"
  aria-label="Open Composer"
  onclick={handleClick}
  oncontextmenu={handleContextMenu}
></button>

<style>
  /* The button is the entire 80x80 surface. Reset native chrome so it
     reads as a pure visual disc, not a system button. */
  .dock {
    appearance: none;
    border: none;
    margin: 0;
    padding: 0;
    width: 80px;
    height: 80px;
    border-radius: 50%;
    cursor: pointer;
    background: radial-gradient(circle at 30% 30%, #6ea8fe, #1e3a8a);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
  }

  .dock:hover {
    filter: brightness(1.1);
  }

  .dock:active {
    transform: scale(0.97);
  }
</style>
