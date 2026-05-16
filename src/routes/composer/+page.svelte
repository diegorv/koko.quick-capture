<script lang="ts">
  // Thin wrapper around the Composer component that wires it to the
  // Tauri side: `save` invokes the Rust `save_note` command, `onclose`
  // hides the window.
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Composer from "$lib/composer/Composer.svelte";

  async function save(text: string) {
    await invoke("save_note", { text });
  }

  async function close() {
    await getCurrentWindow().hide();
  }
</script>

<Composer {save} onclose={close} />
