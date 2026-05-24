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
    onStartRecording?: () => void;
    onStopRecording?: () => Promise<void>;
    recordingActive?: boolean;
    recordingElapsed?: number;
    partialTranscript?: string;
    peakLevel?: number;
  }

  let {
    save,
    onclose,
    focusKey = 0,
    oneditorReady,
    onStartRecording,
    onStopRecording,
    recordingActive = false,
    recordingElapsed = 0,
    partialTranscript = "",
    peakLevel = 0,
  }: Props = $props();

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

  function formatElapsed(secs: number): string {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m}:${s.toString().padStart(2, "0")}`;
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
  {#if recordingActive}
    <div class="recording-overlay">
      <div class="recording-header">
        <div class="recording-pulse"></div>
        <span class="recording-timer">{formatElapsed(recordingElapsed)}</span>
      </div>
      <div class="vu-bar">
        <div class="vu-fill" style="width: {Math.min(peakLevel * 100, 100)}%"></div>
      </div>
      {#if partialTranscript}
        <p class="partial-transcript">{partialTranscript}</p>
      {/if}
      <button type="button" class="stop-btn" onclick={() => onStopRecording?.()}>
        Stop
      </button>
    </div>
  {:else}
    <div class="editor" bind:this={host}></div>
    <div class="bottom-bar">
      {#if onStartRecording}
        <button
          type="button"
          class="mic-btn"
          title="Record voice note"
          onclick={() => onStartRecording?.()}
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>
        </button>
      {/if}
      <div class="hint">ESC cancels · ⌘↩ saves</div>
    </div>
  {/if}
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
    border-radius: 6px;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.12);
    font-family: inherit;
    font-size: 0.78rem;
    overflow: hidden;
    /* Minimum width gives short names (1-2 chars) a usable hit area
       without making the popup feel cramped when names are long. */
    min-width: 9rem;
  }
  :global(.composer .cm-tooltip.cm-tooltip-autocomplete > ul) {
    max-height: 13rem;
    font-family: inherit;
  }
  :global(.composer .cm-tooltip.cm-tooltip-autocomplete > ul > li) {
    padding: 0.18rem 0.55rem;
    color: #0f0f0f;
    line-height: 1.35;
    letter-spacing: -0.005em;
  }
  :global(.composer .cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]) {
    background: rgba(79, 70, 229, 0.85);
    color: #ffffff;
  }
  /* Lucide `user` glyph in place of CM's "?" fallback. mask-image +
     currentColor lets the icon tint to the row's text colour, so it
     stays visible against both the unselected and selected (indigo)
     backgrounds. */
  :global(.composer .cm-completionIcon) {
    width: 1em;
    padding-right: 0.45em;
    opacity: 0.55;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  :global(.composer .cm-completionIcon::after) {
    content: "";
    display: block;
    width: 1em;
    height: 1em;
    background-color: currentColor;
    --wikilink-user-icon: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><path d='M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2'/><circle cx='12' cy='7' r='4'/></svg>");
    mask: var(--wikilink-user-icon) center / contain no-repeat;
    -webkit-mask: var(--wikilink-user-icon) center / contain no-repeat;
  }
  :global(.composer .cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected] .cm-completionIcon) {
    opacity: 0.95;
  }

  .bottom-bar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .hint {
    flex: 1;
    font-size: 0.75rem;
    opacity: 0.45;
    user-select: none;
    text-align: right;
  }

  .mic-btn {
    background: none;
    border: 1px solid rgba(128, 128, 128, 0.25);
    border-radius: 6px;
    padding: 0.25rem 0.4rem;
    cursor: pointer;
    color: inherit;
    opacity: 0.5;
    transition: opacity 100ms;
    display: flex;
    align-items: center;
  }
  .mic-btn:hover {
    opacity: 0.9;
  }

  .recording-overlay {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
    overflow: hidden;
  }

  .recording-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .vu-bar {
    width: 80%;
    height: 4px;
    background: rgba(128, 128, 128, 0.2);
    border-radius: 2px;
    overflow: hidden;
  }

  .vu-fill {
    height: 100%;
    background: #22c55e;
    border-radius: 2px;
  }

  .partial-transcript {
    font-size: 0.8rem;
    opacity: 0.6;
    text-align: center;
    max-height: 3.5rem;
    overflow-y: auto;
    padding: 0 0.5rem;
    margin: 0;
    line-height: 1.3;
  }

  .recording-pulse {
    width: 1rem;
    height: 1rem;
    border-radius: 50%;
    background: #ef4444;
    animation: pulse 1.2s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.5; transform: scale(1.3); }
  }

  .recording-timer {
    font-size: 1.8rem;
    font-variant-numeric: tabular-nums;
    font-weight: 500;
    opacity: 0.8;
  }

  .stop-btn {
    background: #ef4444;
    color: white;
    border: none;
    border-radius: 8px;
    padding: 0.5rem 1.5rem;
    font-size: 0.9rem;
    font-weight: 600;
    cursor: pointer;
    transition: background 100ms;
  }
  .stop-btn:hover {
    background: #dc2626;
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
