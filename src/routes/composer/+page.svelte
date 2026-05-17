<script lang="ts">
  // Thin wrapper around the Composer component that wires it to the
  // Tauri side: `save` invokes the Rust `save_note` command, `onclose`
  // hides the window.
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import Composer from "$lib/composer/Composer.svelte";

  // Bumped on every `open_composer` event from Rust so the Composer
  // component re-focuses its textarea and clears stale text. The
  // window is created once at startup and only hidden / shown after
  // that, so the Svelte component never remounts.
  let focusKey = $state(0);
  let unlisten: UnlistenFn | undefined;

  async function save(text: string) {
    try {
      const result = await invoke("save_note", { text });
      console.log("save_note ok", result);
    } catch (err) {
      console.error("save_note failed", err);
    }
  }

  async function close() {
    // dismiss_composer hides the Composer AND yields macOS key status
    // back to the previously frontmost app (or focuses the Inbox if it
    // is on screen). A plain window.hide() leaves the user without
    // keyboard focus until they Cmd+Tab out.
    try {
      await invoke("dismiss_composer");
    } catch (err) {
      console.error("dismiss_composer failed", err);
    }
  }

  onMount(async () => {
    try {
      unlisten = await listen("open_composer", () => {
        focusKey += 1;
      });
    } catch (err) {
      console.error("listen open_composer failed", err);
    }
  });

  onDestroy(() => {
    unlisten?.();
  });
</script>

<Composer {save} onclose={close} {focusKey} />
