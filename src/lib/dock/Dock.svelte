<script lang="ts">
  // Presentational Dock widget. Decoupled from Tauri so the unit tests
  // can mount it with plain `vi.fn()` callbacks. The route at
  // /dock wires `onComposer` to `invoke("open_composer_window")` and
  // `onContextMenu` to `invoke("open_dock_context_menu", { x, y })`.

  import { tick } from "svelte";

  interface Props {
    onComposer: () => void;
    onContextMenu: (x: number, y: number) => void;
    // Driven from the Dock route, which subscribes to the Rust-side
    // `dock.drag.enter` / `dock.drag.leave` events emitted by the
    // Tauri-native drag-drop handler (see ADR-0008). No HTML5 drop
    // listeners on this surface in v1.0.
    dragActive?: boolean;
    // Number of Captures created since the user last opened the Inbox.
    // The route reads it from `unread_count()` on mount and adjusts it
    // in response to `captures.changed` / `dock.badge.cleared` events.
    // Hidden when 0; rendered as "99+" when > 99.
    unread?: number;
    // Bumped by the parent on every `dock.pulse` event. A change in
    // value re-applies the `pulse` class to the disc, restarting the
    // CSS keyframes. The class is present after a bump (one tick) and
    // absent in the initial render so the animation is one-shot.
    pulseKey?: number;
  }

  let {
    onComposer,
    onContextMenu,
    dragActive = false,
    unread = 0,
    pulseKey = 0,
  }: Props = $props();

  // Track whether the disc should currently be carrying the pulse
  // class. We start `false` so the initial render does not pulse;
  // the route only bumps `pulseKey` in response to `dock.pulse` events.
  let pulsing = $state(false);
  // Effect-local bookkeeping. Seeded lazily on first run so the
  // initial render does not animate; only later changes do.
  let lastPulseKey: number | undefined = undefined;

  $effect(() => {
    const current = pulseKey;
    if (lastPulseKey === undefined) {
      lastPulseKey = current;
      return;
    }
    if (current !== lastPulseKey) {
      lastPulseKey = current;
      // Re-trigger: toggle off then on so consecutive bumps still
      // restart the keyframes (a class transition with the same name
      // is otherwise a no-op for the browser animation system).
      pulsing = false;
      tick().then(() => {
        pulsing = true;
      });
    }
  });

  function handleClick() {
    onComposer();
  }

  function handleContextMenu(event: MouseEvent) {
    event.preventDefault();
    onContextMenu(event.clientX, event.clientY);
  }

  // Badge copy: hide entirely when 0, cap at "99+" so the disc is
  // never wider than the Dock surface itself.
  let badgeLabel = $derived(unread > 99 ? "99+" : String(unread));
</script>

<button
  type="button"
  class="dock"
  class:drag-active={dragActive}
  class:pulse={pulsing}
  aria-label="Open Composer"
  onclick={handleClick}
  oncontextmenu={handleContextMenu}
>
  {#if unread > 0}
    <span class="badge" data-testid="dock-badge">{badgeLabel}</span>
  {/if}
</button>

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
    position: relative;
  }

  .dock:hover {
    filter: brightness(1.1);
  }

  .dock:active {
    transform: scale(0.97);
  }

  /* Visual "wake" while a Finder drag is hovering. Driven by the
     `dragActive` prop, which the /dock route toggles from the Rust-side
     `dock.drag.enter` / `dock.drag.leave` events. */
  .dock.drag-active {
    filter: brightness(1.2);
    box-shadow: 0 0 0 3px rgba(110, 168, 254, 0.6),
      0 4px 20px rgba(0, 0, 0, 0.4);
  }

  /* One-shot pulse triggered on every successful Capture save. The
     class is toggled off then on across a microtask in the script
     above so consecutive bumps restart the keyframes. */
  .dock.pulse {
    animation: dock-pulse 450ms ease-out;
  }

  @keyframes dock-pulse {
    0% {
      transform: scale(1);
      box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
    }
    40% {
      transform: scale(1.08);
      box-shadow: 0 0 0 8px rgba(110, 168, 254, 0.45),
        0 4px 20px rgba(0, 0, 0, 0.4);
    }
    100% {
      transform: scale(1);
      box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
    }
  }

  /* Unread-count badge: small disc anchored to the top-right of the
     Dock surface. Only rendered when `unread > 0`. */
  .badge {
    position: absolute;
    top: 4px;
    right: 4px;
    min-width: 20px;
    height: 20px;
    padding: 0 6px;
    border-radius: 10px;
    background: #ef4444;
    color: white;
    font-size: 11px;
    font-weight: 600;
    line-height: 20px;
    text-align: center;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.35);
    pointer-events: none;
  }
</style>
