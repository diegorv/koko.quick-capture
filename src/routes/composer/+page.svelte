<script lang="ts">
  // Thin wrapper around the Composer component that wires it to the
  // Tauri side: `save` invokes the Rust `save_note` command, `onclose`
  // hides the window.
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Composer from "$lib/composer/Composer.svelte";

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
      await getCurrentWindow().hide();
    } catch (err) {
      console.error("hide window failed", err);
    }
  }
</script>

<Composer {save} onclose={close} />
