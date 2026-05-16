<script lang="ts">
  // Composer view: single autofocused textarea, ESC cancels, Cmd+Enter saves.
  // The component is decoupled from Tauri: the parent injects `save` and
  // listens for the `close` callback so the component is testable in isolation.

  interface Props {
    save: (text: string) => void | Promise<void>;
    onclose?: () => void;
  }

  let { save, onclose }: Props = $props();

  let text = $state("");
  let textarea: HTMLTextAreaElement | undefined = $state();

  function focusOnMount(node: HTMLTextAreaElement) {
    node.focus();
  }

  async function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onclose?.();
      return;
    }
    if (event.key === "Enter" && event.metaKey) {
      event.preventDefault();
      await save(text);
      onclose?.();
    }
  }
</script>

<div class="composer">
  <textarea
    bind:this={textarea}
    bind:value={text}
    onkeydown={handleKeydown}
    use:focusOnMount
    placeholder="Capture a note..."
    aria-label="Note text"
  ></textarea>
</div>

<style>
  .composer {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 1rem;
    box-sizing: border-box;
  }

  textarea {
    flex: 1;
    width: 100%;
    resize: none;
    border: none;
    outline: none;
    font-size: 1rem;
    font-family: inherit;
    background: transparent;
    color: inherit;
  }
</style>
