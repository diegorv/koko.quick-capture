<script lang="ts">
  // Settings panel for the Wikilink source folder (see CONTEXT.md and
  // ADR-0011). Shows the current configured path, a "Choose folder…"
  // button that opens the native folder picker via the Rust command,
  // and a "Clear" button that unsets the path. Validation happens on
  // the Rust side; this component only renders the error inline.
  //
  // Tauri adapters (`invoke`) are injected as a prop so the component
  // can mount in tests without a Tauri runtime, mirroring the
  // DestinationsSection pattern.

  import { onMount } from "svelte";
  import { invoke as tauriInvoke } from "@tauri-apps/api/core";
  import { formatError } from "$lib/utils/format-error";

  type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

  interface Props {
    invokeFn?: InvokeFn;
  }

  const defaultInvoke: InvokeFn = (cmd, args) => tauriInvoke(cmd, args);

  const { invokeFn = defaultInvoke }: Props = $props();

  let folder = $state<string | null>(null);
  let errorMessage = $state<string | null>(null);
  // Disables the buttons during the native picker / a save round-trip
  // so a second click cannot race the first.
  let busy = $state(false);

  onMount(async () => {
    try {
      folder = (await invokeFn("get_wikilink_source_folder")) as string | null;
    } catch (err) {
      errorMessage = formatError(err);
    }
  });

  async function chooseFolder() {
    if (busy) return;
    busy = true;
    errorMessage = null;
    try {
      const picked = (await invokeFn(
        "pick_wikilink_source_folder",
      )) as string | null;
      // User cancelled the dialog — leave the existing value untouched.
      if (picked === null) {
        busy = false;
        return;
      }
      await invokeFn("set_wikilink_source_folder", { path: picked });
      folder = picked;
    } catch (err) {
      errorMessage = formatError(err);
    } finally {
      busy = false;
    }
  }

  async function clearFolder() {
    if (busy) return;
    busy = true;
    errorMessage = null;
    try {
      await invokeFn("set_wikilink_source_folder", { path: null });
      folder = null;
    } catch (err) {
      errorMessage = formatError(err);
    } finally {
      busy = false;
    }
  }

  async function revealFolder() {
    if (busy || folder === null) return;
    errorMessage = null;
    try {
      await invokeFn("reveal_wikilink_source_folder");
    } catch (err) {
      errorMessage = formatError(err);
    }
  }
</script>

<section class="section" data-testid="wikilink-folder-section">
  <header class="head">
    <h2>Wikilink source folder</h2>
  </header>

  <p class="lede">
    Folder whose top-level <code>.md</code> filenames feed the Composer's
    <code>[[</code> autocomplete. Leave unset to keep the feature off.
  </p>

  <div class="row">
    {#if folder === null}
      <span
        class="path unset"
        data-testid="wikilink-folder-path"
      >
        Pick a folder to enable <code>[[</code> autocomplete &rarr;
      </span>
    {:else}
      <span class="path" data-testid="wikilink-folder-path">{folder}</span>
    {/if}
    <span class="row-actions">
      <button
        type="button"
        class="primary"
        disabled={busy}
        onclick={chooseFolder}
        data-testid="wikilink-choose-btn"
      >
        Choose folder…
      </button>
      {#if folder !== null}
        <button
          type="button"
          class="ghost"
          disabled={busy}
          onclick={revealFolder}
          data-testid="wikilink-reveal-btn"
        >
          Reveal in Finder
        </button>
        <button
          type="button"
          class="ghost"
          disabled={busy}
          onclick={clearFolder}
          data-testid="wikilink-clear-btn"
        >
          Clear
        </button>
      {/if}
    </span>
  </div>

  {#if errorMessage}
    <p class="error" role="alert" data-testid="wikilink-folder-error">
      {errorMessage}
    </p>
  {/if}
</section>

<style>
  .section {
    margin-top: 1.25rem;
    padding: 1rem 1.1rem;
    background: #ffffff;
    border: 1px solid rgba(0, 0, 0, 0.08);
    border-radius: 10px;
  }
  @media (prefers-color-scheme: dark) {
    .section {
      background: #232327;
      border-color: rgba(255, 255, 255, 0.08);
    }
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.4rem;
  }

  h2 {
    margin: 0;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: rgba(0, 0, 0, 0.55);
  }
  @media (prefers-color-scheme: dark) {
    h2 {
      color: rgba(255, 255, 255, 0.55);
    }
  }

  .lede {
    margin: 0 0 0.75rem;
    color: rgba(0, 0, 0, 0.55);
    font-size: 0.85rem;
  }
  @media (prefers-color-scheme: dark) {
    .lede {
      color: rgba(255, 255, 255, 0.55);
    }
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.78em;
    padding: 0.05em 0.3em;
    background: rgba(0, 0, 0, 0.06);
    border-radius: 4px;
  }
  @media (prefers-color-scheme: dark) {
    code {
      background: rgba(255, 255, 255, 0.08);
    }
  }

  .row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.6rem;
    flex-wrap: wrap;
  }

  .path {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.8rem;
    word-break: break-all;
    flex: 1 1 16rem;
    min-width: 0;
  }
  .path.unset {
    opacity: 0.7;
    font-family: inherit;
    font-style: italic;
    font-size: 0.85rem;
  }
  .path.unset code {
    font-style: normal;
  }

  .row-actions {
    display: flex;
    gap: 0.3rem;
    flex: 0 0 auto;
  }

  .primary,
  .ghost {
    appearance: none;
    font: inherit;
    font-size: 0.78rem;
    border-radius: 6px;
    padding: 0.25rem 0.65rem;
    cursor: pointer;
    transition:
      background 80ms ease,
      border-color 80ms ease;
  }
  .primary {
    border: 1px solid rgba(76, 29, 149, 0.5);
    background: rgba(76, 29, 149, 0.1);
    color: rgba(76, 29, 149, 1);
  }
  .primary:hover:not(:disabled) {
    background: rgba(76, 29, 149, 0.18);
  }
  @media (prefers-color-scheme: dark) {
    .primary {
      border-color: rgba(167, 139, 250, 0.5);
      background: rgba(167, 139, 250, 0.12);
      color: rgba(167, 139, 250, 1);
    }
    .primary:hover:not(:disabled) {
      background: rgba(167, 139, 250, 0.22);
    }
  }

  .ghost {
    border: 1px solid transparent;
    background: transparent;
    color: rgba(0, 0, 0, 0.6);
  }
  .ghost:hover:not(:disabled) {
    background: rgba(0, 0, 0, 0.06);
  }
  @media (prefers-color-scheme: dark) {
    .ghost {
      color: rgba(255, 255, 255, 0.7);
    }
    .ghost:hover:not(:disabled) {
      background: rgba(255, 255, 255, 0.08);
    }
  }

  .primary:disabled,
  .ghost:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .error {
    margin: 0.6rem 0 0;
    padding: 0.35rem 0.6rem;
    background: rgba(220, 38, 38, 0.08);
    border: 1px solid rgba(220, 38, 38, 0.25);
    border-radius: 6px;
    color: rgba(155, 28, 28, 1);
    font-size: 0.78rem;
  }
  @media (prefers-color-scheme: dark) {
    .error {
      background: rgba(248, 113, 113, 0.1);
      border-color: rgba(248, 113, 113, 0.3);
      color: rgba(252, 165, 165, 1);
    }
  }
</style>
