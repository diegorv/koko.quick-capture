<script lang="ts">
  // Composer view: single autofocused textarea, ESC cancels, Cmd+Enter saves.
  // The component is decoupled from Tauri: the parent injects `save` and
  // listens for the `close` callback so the component is testable in isolation.

  interface Props {
    save: (text: string) => void | Promise<void>;
    onclose?: () => void;
    /**
     * Bumped by the parent every time the window is shown so the
     * Composer can re-focus the textarea and clear stale text. The
     * Composer window is created once at app startup and hidden/shown
     * on every shortcut press, so the `use:focusOnMount` action only
     * fires the first time; without an external focus signal,
     * subsequent shows leave focus wherever the OS left it.
     */
    focusKey?: number;
  }

  let { save, onclose, focusKey = 0 }: Props = $props();

  let text = $state("");
  let textarea: HTMLTextAreaElement | undefined = $state();
  // Flipped true for a beat after a successful save so the composer
  // can flash a brief green confirmation before the window hides.
  let saved = $state(false);
  const SAVE_FLASH_MS = 180;

  function focusOnMount(node: HTMLTextAreaElement) {
    node.focus();
  }

  $effect(() => {
    // Re-run on every `focusKey` change. Reset text so each open
    // starts from an empty draft, and re-focus the textarea. Also
    // clear any leftover `saved` flash from a previous capture.
    focusKey;
    text = "";
    saved = false;
    textarea?.focus();
  });

  function quietTextarea(node: HTMLTextAreaElement) {
    // WebKit-specific attributes that the Svelte type system does not
    // know about. Setting them imperatively keeps them out of the
    // textarea props type while still applying the behavior macOS
    // WebView honors.
    node.setAttribute("autocorrect", "off");
    node.setAttribute("autocapitalize", "off");
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
      saved = true;
      // Hold the green confirmation flash briefly so the user sees a
      // visual ack before the window is hidden by the parent.
      await new Promise<void>((resolve) => setTimeout(resolve, SAVE_FLASH_MS));
      saved = false;
      onclose?.();
    }
  }
</script>

<div class="composer" class:saved>
  <textarea
    bind:this={textarea}
    bind:value={text}
    onkeydown={handleKeydown}
    use:focusOnMount
    use:quietTextarea
    placeholder="Capture a note..."
    aria-label="Note text"
    autocomplete="off"
    spellcheck="false"
  ></textarea>
  <div class="hint">ESC cancels · ⌘↩ saves</div>
</div>

<style>
  .composer {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 1.25rem 1.5rem 1rem;
    box-sizing: border-box;
    background: rgba(248, 248, 248, 0.98);
    color: #0f0f0f;
    font-family:
      Inter,
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      sans-serif;
    border-radius: 12px;
    transition: box-shadow 100ms ease-out;
  }

  /* Brief success ring after Cmd+Enter, held for ~180ms in the script
     before the parent hides the window. Inset so the rounded corners
     stay clean. */
  .composer.saved {
    box-shadow: inset 0 0 0 3px rgba(34, 197, 94, 0.55);
  }

  textarea {
    flex: 1;
    width: 100%;
    resize: none;
    border: none;
    outline: none;
    font-size: 1.05rem;
    line-height: 1.45;
    font-family: inherit;
    background: transparent;
    color: inherit;
  }

  .hint {
    margin-top: 0.5rem;
    font-size: 0.75rem;
    opacity: 0.45;
    user-select: none;
    text-align: right;
  }

  @media (prefers-color-scheme: dark) {
    .composer {
      background: rgba(30, 30, 30, 0.98);
      color: #f4f4f4;
    }
  }
</style>
