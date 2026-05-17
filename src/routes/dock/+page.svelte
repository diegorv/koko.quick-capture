<script lang="ts">
  // Dock window route. Wires the presentational `Dock` component to
  // Tauri:
  //  - click  -> `open_composer_window` command
  //  - rclick -> `open_dock_context_menu` command (Rust builds the
  //               native popup menu and dispatches the chosen item
  //               via the app-level `on_menu_event` registered in
  //               `lib::run` setup; see `commands::open_dock_context_menu`)
  //  - fullscreen-enter / -exit -> hide / show this window.
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Dock from "$lib/dock/Dock.svelte";

  // Visual hover state for Finder drags. Driven by Rust events
  // emitted from the Dock window's native drag-drop handler (see
  // ADR-0008). No HTML5 `dragover` / `drop` listeners on this route.
  let dragActive = $state(false);

  async function openComposer() {
    try {
      await invoke("open_composer_window");
    } catch (err) {
      console.error("open_composer_window failed", err);
    }
  }

  async function openContextMenu(x: number, y: number) {
    try {
      await invoke("open_dock_context_menu", { x, y });
    } catch (err) {
      console.error("open_dock_context_menu failed", err);
    }
  }

  onMount(() => {
    const win = getCurrentWindow();
    const unlisteners: Promise<UnlistenFn>[] = [
      listen("dock.fullscreen.entered", async () => {
        try {
          await win.hide();
        } catch (err) {
          console.error("dock hide failed", err);
        }
      }),
      listen("dock.fullscreen.exited", async () => {
        try {
          await win.show();
        } catch (err) {
          console.error("dock show failed", err);
        }
      }),
      listen("dock.drag.enter", () => {
        dragActive = true;
      }),
      listen("dock.drag.leave", () => {
        dragActive = false;
      }),
    ];

    return () => {
      for (const u of unlisteners) {
        u.then((fn) => fn()).catch(() => {});
      }
    };
  });
</script>

<Dock
  onComposer={openComposer}
  onContextMenu={openContextMenu}
  {dragActive}
/>

<style>
  :global(html),
  :global(body) {
    /* The Dock window is decoration-less and 80x80; the body should
       be transparent so the rounded button reads as a free-floating
       widget rather than a square panel. */
    margin: 0;
    padding: 0;
    background: transparent;
    overflow: hidden;
  }
</style>
