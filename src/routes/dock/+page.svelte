<script lang="ts">
  // Dock window route. Wires the presentational `Dock` component to
  // Tauri:
  //  - click  -> `open_composer_window` command
  //  - rclick -> `open_dock_context_menu` command (Rust builds the
  //               native popup menu and dispatches the chosen item
  //               via the app-level `on_menu_event` registered in
  //               `lib::run` setup; see `commands::open_dock_context_menu`)
  //  - fullscreen-enter / -exit -> hide / show this window.
  //  - `captures.changed` -> increment the unread badge.
  //  - `dock.pulse` -> bump `pulseKey` to re-fire the disc animation.
  //  - `dock.badge.cleared` -> reset the unread badge to 0.
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Dock from "$lib/dock/Dock.svelte";

  // Visual hover state for Finder drags. Driven by Rust events
  // emitted from the Dock window's native drag-drop handler (see
  // ADR-0008). No HTML5 `dragover` / `drop` listeners on this route.
  let dragActive = $state(false);

  // Unread-since-last-Inbox-open count. Initialised on mount from the
  // store (so the count survives a restart per PRD user story 24);
  // incremented per `captures.changed`; zeroed on `dock.badge.cleared`.
  let unread = $state(0);

  // Monotonic counter the Dock component watches to re-fire its pulse
  // animation. Each `dock.pulse` event bumps this by 1.
  let pulseKey = $state(0);

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

    // Initialise the badge from the persisted cursor. Default to 0
    // on failure so a transient error never leaves the badge in a
    // visibly-stuck state.
    (async () => {
      try {
        const n = await invoke<number>("unread_count");
        unread = Number(n) || 0;
      } catch (err) {
        console.error("unread_count failed", err);
        unread = 0;
      }
    })();

    const unlisteners: Promise<UnlistenFn>[] = [
      listen("dock:fullscreen:entered", async () => {
        try {
          await win.hide();
        } catch (err) {
          console.error("dock hide failed", err);
        }
      }),
      listen("dock:fullscreen:exited", async () => {
        try {
          await win.show();
        } catch (err) {
          console.error("dock show failed", err);
        }
      }),
      listen("dock:drag:enter", () => {
        dragActive = true;
      }),
      listen("dock:drag:leave", () => {
        dragActive = false;
      }),
      listen<unknown>("captures:changed", (evt) => {
        // `captures.changed` carries a full `Capture` on save (slice 02
        // contract) and a thin `MutationNotice { id, kind }` on star /
        // soft-delete (slice 03). Only saves should bump the unread
        // count, so we discriminate on the presence of `created_at`,
        // which only the full `Capture` payload carries.
        const payload = evt.payload as Record<string, unknown> | null;
        if (payload && "created_at" in payload) {
          unread += 1;
        }
      }),
      listen("dock:pulse", () => {
        pulseKey += 1;
      }),
      listen("dock:badge:cleared", () => {
        unread = 0;
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
  {unread}
  {pulseKey}
/>

<style>
  :global(html),
  :global(body) {
    /* The Dock window is decoration-less and 96x96 (80x80 disc
       centered with an 8px ring of slack around it for the unread
       badge to overflow into); the body should be transparent so the
       rounded button reads as a free-floating widget rather than a
       square panel. */
    margin: 0;
    padding: 0;
    background: transparent;
    overflow: hidden;
  }
</style>
