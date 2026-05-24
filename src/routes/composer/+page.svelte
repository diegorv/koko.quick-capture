<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import Composer from "$lib/composer/Composer.svelte";
  import { OPEN_COMPOSER } from "$lib/events";

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
    try {
      await invoke("dismiss_composer");
    } catch (err) {
      console.error("dismiss_composer failed", err);
    }
  }

  onMount(async () => {
    try {
      unlisten = await listen(OPEN_COMPOSER, () => {
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
