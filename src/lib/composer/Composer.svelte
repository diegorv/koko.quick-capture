<script lang="ts">
  // Composer view: CodeMirror-backed single-text editor. ESC cancels,
  // Cmd+Enter saves. Swap from a plain <textarea> to CodeMirror was
  // accepted in ADR-0011 to host the [[ wikilink autocomplete; this
  // slice introduces the editor host without the completion source
  // yet (added in a follow-up slice).
  //
  // The component is decoupled from Tauri: the parent injects `save`
  // and listens for `onclose` so the component is testable in
  // isolation. `oneditorReady` is a test-only seam that exposes the
  // live EditorView so component tests can dispatch programmatic doc
  // changes without simulating contenteditable input (per ADR-0005).

  import { onMount, onDestroy } from "svelte";
  import {
    EditorView,
    keymap,
    placeholder as cmPlaceholder,
  } from "@codemirror/view";
  import { EditorState, Prec } from "@codemirror/state";
  import { defaultKeymap } from "@codemirror/commands";
  import { wikilinkCompletion } from "$lib/wikilink/completion";

  interface Props {
    save: (text: string) => void | Promise<void>;
    onclose?: () => void;
    /**
     * Bumped by the parent every time the window is shown so the
     * Composer can re-focus the editor and clear stale text. The
     * Composer window is created once at app startup and hidden /
     * shown on every shortcut press, so without an external focus
     * signal subsequent shows would leave focus wherever the OS
     * left it.
     */
    focusKey?: number;
    /** Test seam: receives the live EditorView once it mounts. */
    oneditorReady?: (view: EditorView) => void;
  }

  let { save, onclose, focusKey = 0, oneditorReady }: Props = $props();

  let host: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  // Flipped true for a beat after a successful save so the composer
  // can flash a brief green confirmation before the window hides.
  let saved = $state(false);
  // Re-entry guard. Holding Cmd+Enter (or hitting it twice quickly)
  // used to fire the save handler twice before the window hid,
  // double-persisting the note. We refuse new saves while the
  // current one is in flight.
  let saving = false;
  const SAVE_FLASH_MS = 180;

  async function handleSave(): Promise<boolean> {
    if (!view || saving) return true;
    saving = true;
    try {
      await save(view.state.doc.toString());
      saved = true;
      // Hold the green confirmation flash briefly so the user sees a
      // visual ack before the window is hidden by the parent.
      await new Promise<void>((resolve) => setTimeout(resolve, SAVE_FLASH_MS));
      saved = false;
    } finally {
      saving = false;
    }
    onclose?.();
    return true;
  }

  function handleEscape(): boolean {
    onclose?.();
    return true;
  }

  function resetDoc() {
    if (!view) return;
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: "" },
    });
    view.focus();
  }

  onMount(() => {
    if (!host) return;
    const state = EditorState.create({
      doc: "",
      extensions: [
        cmPlaceholder("Capture a note..."),
        // `[[ ` autocomplete against the configured Wikilink source
        // folder (ADR-0011). Installed *before* our composer keymap
        // so the autocompletion extension's own completionKeymap
        // (Enter, ESC, ↑↓, Tab) takes precedence when its popup is
        // open — ESC closes the popup first; the next ESC reaches
        // our handler and closes the Composer.
        wikilinkCompletion(),
        Prec.high(
          keymap.of([
            { key: "Escape", run: () => handleEscape() },
            {
              key: "Mod-Enter",
              run: () => {
                void handleSave();
                return true;
              },
            },
          ]),
        ),
        keymap.of(defaultKeymap),
        EditorView.lineWrapping,
        EditorView.contentAttributes.of({
          "aria-label": "Note text",
          autocorrect: "off",
          autocapitalize: "off",
          spellcheck: "false",
        }),
        EditorView.theme({
          "&": { height: "100%" },
          ".cm-scroller": {
            fontFamily:
              "Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif",
            fontSize: "1.05rem",
            lineHeight: "1.45",
          },
          ".cm-content": {
            padding: 0,
            caretColor: "currentColor",
          },
          ".cm-focused": { outline: "none" },
        }),
      ],
    });
    view = new EditorView({ state, parent: host });
    view.focus();
    oneditorReady?.(view);
  });

  onDestroy(() => {
    view?.destroy();
    view = undefined;
  });

  $effect(() => {
    // Re-run on every `focusKey` change. Reset the doc so each open
    // starts from an empty draft, refocus the editor, and clear any
    // leftover `saved` flash and the in-flight guard from a previous
    // capture.
    focusKey;
    if (view) {
      resetDoc();
      saved = false;
      saving = false;
    }
  });
</script>

<div class="composer" class:saved data-tauri-drag-region>
  <div class="editor" bind:this={host}></div>
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
     stay clean. Indigo matches the Inbox selection accent. */
  .composer.saved {
    box-shadow: inset 0 0 0 3px rgba(79, 70, 229, 0.6);
  }

  .editor {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  :global(.composer .cm-editor) {
    flex: 1;
    height: 100%;
    background: transparent;
    color: inherit;
  }
  :global(.composer .cm-editor.cm-focused) {
    outline: none;
  }
  :global(.composer .cm-content) {
    color: inherit;
  }

  /* CM autocomplete popup. Default theme is a white card with black
     text and a near-white selection background, which leaves the
     non-selected row invisible against the Composer's light-grey
     surface and unreadable against the dark surface. Re-style it to
     match the Composer's palette: light card on light mode, dark
     card on dark mode, indigo selection in both. */
  :global(.composer .cm-tooltip.cm-tooltip-autocomplete) {
    background: #ffffff;
    border: 1px solid rgba(0, 0, 0, 0.12);
    border-radius: 8px;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.12);
    font-family: inherit;
    font-size: 0.9rem;
    overflow: hidden;
  }
  :global(.composer .cm-tooltip.cm-tooltip-autocomplete > ul) {
    max-height: 14rem;
    font-family: inherit;
  }
  :global(.composer .cm-tooltip.cm-tooltip-autocomplete > ul > li) {
    padding: 0.25rem 0.6rem;
    color: #0f0f0f;
  }
  :global(.composer .cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]) {
    background: rgba(79, 70, 229, 0.85);
    color: #ffffff;
  }
  :global(.composer .cm-completionIcon) {
    opacity: 0.55;
    padding-right: 0.4em;
  }
  :global(.composer .cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected] .cm-completionIcon) {
    opacity: 0.9;
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
    :global(.composer .cm-tooltip.cm-tooltip-autocomplete) {
      background: #2a2a2e;
      border-color: rgba(255, 255, 255, 0.12);
      box-shadow: 0 4px 14px rgba(0, 0, 0, 0.4);
    }
    :global(.composer .cm-tooltip.cm-tooltip-autocomplete > ul > li) {
      color: #f4f4f4;
    }
    :global(.composer .cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]) {
      background: rgba(99, 91, 255, 0.85);
      color: #ffffff;
    }
  }
</style>
