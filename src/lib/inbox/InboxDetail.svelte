<script lang="ts">
  // Inbox detail pane: renders the full payload of the selected Capture
  // and the kind-appropriate "Open" action. Pure presentational — the
  // Tauri `invoke` calls live in the parent route so this component
  // stays mountable without a Tauri runtime, mirroring the
  // InboxList.svelte split.
  //
  // The image preview uses `convertFileSrc` so the webview can load
  // local files (path-flavor `Shot` uses `source_path`, bytes-flavor
  // `Shot` uses the persisted `blob_path`).
  import { convertFileSrc } from "@tauri-apps/api/core";
  import type { Capture } from "$lib/captures/types";

  interface Props {
    capture: Capture | null;
    onOpenLink: (url: string) => void;
    onReveal: (id: string) => void;
  }

  const { capture, onOpenLink, onReveal }: Props = $props();

  // Narrow the loosely-typed payload fields the Rust side hands us
  // through serde_json. Each helper returns `null` rather than `""`
  // so missing values do not render as empty strings.
  function str(value: unknown): string | null {
    return typeof value === "string" ? value : null;
  }
</script>

<div class="detail" data-testid="inbox-detail">
  {#if capture === null}
    <p class="placeholder">Select a Capture</p>
  {:else if capture.kind === "Link"}
    {@const url = str(capture.payload.url) ?? ""}
    {@const rawText = str(capture.payload.raw_text)}
    {@const title = str(capture.payload.title)}
    <section class="link">
      <h2>Link</h2>
      <p class="url">{url}</p>
      {#if rawText !== null && rawText !== url}
        <p class="raw">Raw: {rawText}</p>
      {/if}
      <p class="title">Title: {title ?? "(none)"}</p>
      <button
        type="button"
        class="action"
        onclick={() => onOpenLink(url)}
      >
        Open in Browser
      </button>
    </section>
  {:else if capture.kind === "Clip"}
    {@const text = str(capture.payload.text) ?? ""}
    <section class="text">
      <h2>Clip</h2>
      <pre class="payload-text">{text}</pre>
    </section>
  {:else if capture.kind === "Note"}
    {@const text = str(capture.payload.text) ?? ""}
    <section class="text">
      <h2>Note</h2>
      <pre class="payload-text">{text}</pre>
    </section>
  {:else if capture.kind === "Shot"}
    {@const sourcePath = str(capture.payload.source_path)}
    {@const blobPath = str(capture.payload.blob_path)}
    {@const displayPath = sourcePath ?? blobPath ?? ""}
    {@const previewSrc = displayPath ? convertFileSrc(displayPath) : ""}
    <section class="shot">
      <h2>Shot</h2>
      {#if previewSrc}
        <img class="preview" src={previewSrc} alt="Shot preview" />
      {/if}
      <p class="path">{displayPath}</p>
      <button
        type="button"
        class="action"
        onclick={() => onReveal(capture.id)}
      >
        {sourcePath !== null ? "Reveal in Finder" : "Open Image"}
      </button>
    </section>
  {:else if capture.kind === "File"}
    {@const originalName = str(capture.payload.original_name)}
    {@const mime = str(capture.payload.mime) ?? ""}
    {@const sourcePath = str(capture.payload.source_path) ?? ""}
    <section class="file">
      <h2>File</h2>
      <p class="name">{originalName ?? "(no name)"}</p>
      <p class="mime">{mime}</p>
      <p class="path">{sourcePath}</p>
      <button
        type="button"
        class="action"
        onclick={() => onReveal(capture.id)}
      >
        Reveal in Finder
      </button>
    </section>
  {/if}
</div>

<style>
  .detail {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    justify-content: flex-start;
    padding: 1rem 1.25rem;
    height: 100%;
    box-sizing: border-box;
    overflow-y: auto;
  }

  .placeholder {
    opacity: 0.5;
    margin: auto;
    align-self: center;
  }

  h2 {
    margin: 0 0 0.75rem;
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    opacity: 0.6;
  }

  .url {
    word-break: break-all;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.9rem;
    margin: 0 0 0.5rem;
  }

  .raw,
  .title,
  .mime,
  .name,
  .path {
    margin: 0 0 0.5rem;
    font-size: 0.85rem;
    opacity: 0.75;
    word-break: break-all;
  }

  .path {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }

  .payload-text {
    flex: 1;
    overflow: auto;
    margin: 0;
    padding: 0.75rem;
    border: 1px solid rgba(0, 0, 0, 0.12);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.03);
    white-space: pre-wrap;
    word-break: break-word;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.85rem;
  }

  .preview {
    max-width: 100%;
    max-height: 50vh;
    object-fit: contain;
    margin-bottom: 0.75rem;
    border: 1px solid rgba(0, 0, 0, 0.12);
    border-radius: 4px;
  }

  .action {
    align-self: flex-start;
    padding: 0.4rem 0.9rem;
    font-size: 0.9rem;
    border: 1px solid rgba(0, 0, 0, 0.2);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.04);
    cursor: pointer;
  }

  .action:hover {
    background: rgba(0, 0, 0, 0.08);
  }

  @media (prefers-color-scheme: dark) {
    .payload-text {
      border-color: rgba(255, 255, 255, 0.12);
      background: rgba(255, 255, 255, 0.04);
    }
    .preview {
      border-color: rgba(255, 255, 255, 0.12);
    }
    .action {
      border-color: rgba(255, 255, 255, 0.2);
      background: rgba(255, 255, 255, 0.06);
    }
    .action:hover {
      background: rgba(255, 255, 255, 0.12);
    }
  }
</style>
