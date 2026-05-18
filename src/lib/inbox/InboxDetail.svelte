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
  import {
    Link,
    Clipboard,
    Image as ImageIcon,
    File as FileIcon,
    StickyNote,
    MousePointerClick,
    type Icon as IconType,
  } from "@lucide/svelte";

  const KIND_ICONS: Record<Capture["kind"], typeof IconType> = {
    Link,
    Clip: Clipboard,
    Shot: ImageIcon,
    File: FileIcon,
    Note: StickyNote,
  };

  interface Props {
    capture: Capture | null;
    onOpenLink: (url: string) => void;
    onReveal: (id: string) => void;
    /** Optional star toggle. When omitted, the header star is
        display-only — useful for tests that do not exercise mutations. */
    onStarToggle?: (id: string, next: boolean) => void;
    /** Optional route action. Header shows a "Route" button when this
     * is provided so the user has a mouse-driven alternative to the
     * `R` keyboard shortcut. */
    onRoute?: (id: string) => void;
    /** Optional un-route action. Header shows a "Move to Inbox"
     * button when this is provided — drives the Archive's reverse
     * flow per ADR-0010. */
    onUnroute?: (id: string) => void;
  }

  const {
    capture,
    onOpenLink,
    onReveal,
    onStarToggle,
    onRoute,
    onUnroute,
  }: Props = $props();

  function str(value: unknown): string | null {
    return typeof value === "string" ? value : null;
  }

  function kindLabel(kind: Capture["kind"]): string {
    return kind;
  }

  function hostnameOf(url: string): string {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }

  function formatTimestamp(iso: string): string {
    const d = new Date(iso);
    const now = new Date();
    const sameDay =
      d.getFullYear() === now.getFullYear() &&
      d.getMonth() === now.getMonth() &&
      d.getDate() === now.getDate();
    if (sameDay) {
      return d.toLocaleTimeString(undefined, {
        hour: "2-digit",
        minute: "2-digit",
      });
    }
    return d.toLocaleString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }
</script>

<div class="detail" data-testid="inbox-detail">
  {#if capture === null}
    <div class="placeholder">
      <span class="placeholder-glyph" aria-hidden="true">
        <MousePointerClick size={36} strokeWidth={1.4} />
      </span>
      <p class="placeholder-text">Select a Capture</p>
      <p class="placeholder-hint">
        Use <kbd>↑</kbd><kbd>↓</kbd> to move,
        <kbd>↩</kbd> to open, <kbd>S</kbd> to star.
      </p>
    </div>
  {:else}
    {@const HeaderIcon = KIND_ICONS[capture.kind]}
    <header class="header">
      <div class="title-row">
        <span class="glyph" aria-hidden="true">
          <HeaderIcon size={22} strokeWidth={1.6} />
        </span>
        <h2 class="kind">{kindLabel(capture.kind)}</h2>
        {#if onStarToggle}
          <button
            type="button"
            class="star-btn"
            class:active={capture.starred}
            aria-label={capture.starred ? "Unstar capture" : "Star capture"}
            aria-pressed={capture.starred}
            onclick={() => onStarToggle(capture.id, !capture.starred)}
          >
            {capture.starred ? "★" : "☆"}
          </button>
        {:else if capture.starred}
          <span class="star-btn active" title="Starred" aria-hidden="true">★</span>
        {/if}
        {#if onRoute}
          <button
            type="button"
            class="route-btn"
            aria-label="Route capture"
            title={onUnroute ? "Re-route (R)" : "Route (R)"}
            onclick={() => onRoute(capture.id)}
            data-testid="detail-route-btn"
          >
            {onUnroute ? "Re-route" : "Route"}
          </button>
        {/if}
        {#if onUnroute}
          <button
            type="button"
            class="unroute-btn"
            aria-label="Move capture back to Inbox"
            title="Move to Inbox (⇧R)"
            onclick={() => onUnroute(capture.id)}
            data-testid="detail-unroute-btn"
          >
            Move to Inbox
          </button>
        {/if}
      </div>
      <p class="meta-line">
        {#if capture.routed_at}
          <span class="timestamp" title="Routed">
            routed {formatTimestamp(capture.routed_at)}
          </span>
          <span class="timestamp captured" title="Captured">
            · captured {formatTimestamp(capture.created_at)}
          </span>
        {:else}
          <span class="timestamp">{formatTimestamp(capture.created_at)}</span>
        {/if}
        {#if capture.source_app}
          <span class="source-app" title={capture.source_app}>
            from {capture.source_app}
          </span>
        {/if}
        {#if capture.source_title}
          <span class="source-title" title={capture.source_title}>
            “{capture.source_title}”
          </span>
        {/if}
        {#if capture.source_url}
          <button
            type="button"
            class="source-url"
            title={capture.source_url}
            onclick={() => onOpenLink(capture.source_url!)}
          >
            ↗ {hostnameOf(capture.source_url)}
          </button>
        {/if}
      </p>
    </header>

    {#if capture.kind === "Link"}
      {@const url = str(capture.payload.url) ?? ""}
      {@const rawText = str(capture.payload.raw_text)}
      {@const title = str(capture.payload.title)}
      <section class="body">
        <p class="url">{url}</p>
        <dl class="meta">
          {#if title}
            <dt>Title</dt>
            <dd>{title}</dd>
          {/if}
          {#if rawText !== null && rawText !== url}
            <dt>Raw</dt>
            <dd class="mono">{rawText}</dd>
          {/if}
        </dl>
      </section>
      <footer class="actions">
        <button
          type="button"
          class="action primary"
          onclick={() => onOpenLink(url)}
        >
          Open in Browser
        </button>
      </footer>
    {:else if capture.kind === "Clip" || capture.kind === "Note"}
      {@const text = str(capture.payload.text) ?? ""}
      <section class="body">
        <pre class="payload-text">{text}</pre>
      </section>
    {:else if capture.kind === "Shot"}
      {@const sourcePath = str(capture.payload.source_path)}
      {@const blobPath = str(capture.payload.blob_path)}
      {@const displayPath = sourcePath ?? blobPath ?? ""}
      {@const previewSrc = displayPath ? convertFileSrc(displayPath) : ""}
      {@const mime = str(capture.payload.mime)}
      <section class="body">
        {#if previewSrc}
          <img class="preview" src={previewSrc} alt="Shot preview" />
        {/if}
        <dl class="meta">
          {#if mime}
            <dt>Type</dt>
            <dd>{mime}</dd>
          {/if}
          <dt>Path</dt>
          <dd class="mono">{displayPath}</dd>
        </dl>
      </section>
      <footer class="actions">
        <button
          type="button"
          class="action primary"
          onclick={() => onReveal(capture.id)}
        >
          {sourcePath !== null ? "Reveal in Finder" : "Open Image"}
        </button>
      </footer>
    {:else if capture.kind === "File"}
      {@const originalName = str(capture.payload.original_name)}
      {@const mime = str(capture.payload.mime) ?? ""}
      {@const sourcePath = str(capture.payload.source_path) ?? ""}
      <section class="body">
        <p class="filename">{originalName ?? "(no name)"}</p>
        <dl class="meta">
          {#if mime}
            <dt>Type</dt>
            <dd>{mime}</dd>
          {/if}
          <dt>Path</dt>
          <dd class="mono">{sourcePath}</dd>
        </dl>
      </section>
      <footer class="actions">
        <button
          type="button"
          class="action primary"
          onclick={() => onReveal(capture.id)}
        >
          Reveal in Finder
        </button>
      </footer>
    {/if}
  {/if}
</div>

<style>
  .detail {
    display: flex;
    flex-direction: column;
    height: 100%;
    box-sizing: border-box;
    overflow-y: auto;
  }

  .placeholder {
    margin: auto;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.55rem;
    max-width: 28ch;
    padding: 0 1.5rem;
  }

  .placeholder-glyph {
    display: inline-flex;
    opacity: 0.35;
  }

  .placeholder-text {
    font-size: 0.95rem;
    margin: 0;
    opacity: 0.55;
  }

  .placeholder-hint {
    margin: 0;
    font-size: 0.78rem;
    line-height: 1.55;
    opacity: 0.45;
  }

  .placeholder-hint kbd {
    display: inline-block;
    padding: 0.05em 0.4em;
    margin: 0 1px;
    font-family: inherit;
    font-size: 0.82em;
    background: rgba(0, 0, 0, 0.06);
    border: 1px solid rgba(0, 0, 0, 0.1);
    border-radius: 3px;
  }

  .header {
    padding: 1.25rem 1.5rem 0.75rem;
    border-bottom: 1px solid rgba(0, 0, 0, 0.06);
    min-width: 0;
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  .glyph {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    opacity: 0.75;
  }

  .kind {
    margin: 0;
    font-size: 1.1rem;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .star-btn {
    margin-left: auto;
    background: transparent;
    border: none;
    padding: 0.15rem 0.4rem;
    font-size: 1.05rem;
    line-height: 1;
    color: inherit;
    opacity: 0.55;
    cursor: pointer;
    border-radius: 4px;
    transition: opacity 80ms ease, background 80ms ease;
  }

  .star-btn:hover {
    opacity: 1;
    background: rgba(0, 0, 0, 0.06);
  }

  .star-btn.active {
    color: #f59e0b;
    opacity: 1;
  }

  .route-btn,
  .unroute-btn {
    appearance: none;
    font: inherit;
    font-size: 0.72rem;
    padding: 0.18rem 0.55rem;
    border-radius: 6px;
    cursor: pointer;
    transition: background 80ms ease;
  }
  .route-btn {
    border: 1px solid rgba(76, 29, 149, 0.5);
    background: rgba(76, 29, 149, 0.1);
    color: rgba(76, 29, 149, 1);
  }
  .route-btn:hover {
    background: rgba(76, 29, 149, 0.18);
  }
  @media (prefers-color-scheme: dark) {
    .route-btn {
      border-color: rgba(167, 139, 250, 0.5);
      background: rgba(167, 139, 250, 0.12);
      color: rgba(167, 139, 250, 1);
    }
    .route-btn:hover {
      background: rgba(167, 139, 250, 0.22);
    }
  }
  .unroute-btn {
    border: 1px solid rgba(0, 0, 0, 0.18);
    background: transparent;
    color: rgba(0, 0, 0, 0.7);
  }
  .unroute-btn:hover {
    background: rgba(0, 0, 0, 0.06);
  }
  @media (prefers-color-scheme: dark) {
    .unroute-btn {
      border-color: rgba(255, 255, 255, 0.2);
      color: rgba(255, 255, 255, 0.75);
    }
    .unroute-btn:hover {
      background: rgba(255, 255, 255, 0.08);
    }
  }

  .meta-line {
    margin: 0.35rem 0 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.35rem;
    /* Without min-width:0 the chip max-width:100% rule resolves
       against the meta-line's content-based width, so a long title
       widens the column past the pane edge instead of clipping. */
    min-width: 0;
    max-width: 100%;
  }

  .timestamp {
    font-size: 0.78rem;
    opacity: 0.55;
  }
  .timestamp.captured {
    opacity: 0.4;
  }

  .source-app,
  .source-title,
  .source-url {
    font-size: 0.72rem;
    padding: 0.12rem 0.55rem;
    border-radius: 999px;
    background: rgba(76, 29, 149, 0.1);
    color: rgba(76, 29, 149, 0.95);
    border: 1px solid rgba(76, 29, 149, 0.3);
    white-space: nowrap;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .source-app {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }

  .source-url {
    appearance: none;
    font-family: inherit;
    cursor: pointer;
    transition:
      background 80ms ease,
      border-color 80ms ease;
  }
  .source-url:hover {
    background: rgba(76, 29, 149, 0.18);
    border-color: rgba(76, 29, 149, 0.55);
  }

  @media (prefers-color-scheme: dark) {
    .source-app,
    .source-title,
    .source-url {
      background: rgba(167, 139, 250, 0.15);
      color: rgba(167, 139, 250, 0.95);
      border-color: rgba(167, 139, 250, 0.35);
    }
    .source-url:hover {
      background: rgba(167, 139, 250, 0.25);
      border-color: rgba(167, 139, 250, 0.55);
    }
  }

  .body {
    padding: 1rem 1.5rem;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }

  .url {
    margin: 0;
    font-size: 0.95rem;
    word-break: break-all;
    color: #2563eb;
  }

  .filename {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 500;
    word-break: break-word;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.35rem 1rem;
    margin: 0;
    font-size: 0.85rem;
  }

  .meta dt {
    opacity: 0.5;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 0.7rem;
    padding-top: 0.15rem;
  }

  .meta dd {
    margin: 0;
    word-break: break-all;
  }

  .mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.8rem;
    opacity: 0.85;
  }

  .preview {
    max-width: 100%;
    max-height: 55vh;
    object-fit: contain;
    align-self: center;
    border-radius: 6px;
    border: 1px solid rgba(0, 0, 0, 0.08);
    background: rgba(0, 0, 0, 0.02);
  }

  .payload-text {
    flex: 1;
    overflow: auto;
    margin: 0;
    padding: 1rem;
    border: 1px solid rgba(0, 0, 0, 0.08);
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.02);
    white-space: pre-wrap;
    word-break: break-word;
    font-family:
      ui-monospace,
      SFMono-Regular,
      Menlo,
      monospace;
    font-size: 0.88rem;
    line-height: 1.5;
  }

  .actions {
    padding: 0.75rem 1.5rem 1.25rem;
    display: flex;
    gap: 0.5rem;
  }

  .action {
    padding: 0.5rem 1rem;
    font-size: 0.9rem;
    font-weight: 500;
    border: 1px solid rgba(0, 0, 0, 0.15);
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.03);
    cursor: pointer;
    color: inherit;
    font-family: inherit;
  }

  .action:hover {
    background: rgba(0, 0, 0, 0.07);
  }

  .action.primary {
    background: #4f46e5;
    border-color: #4f46e5;
    color: white;
  }

  .action.primary:hover {
    background: #4338ca;
    border-color: #4338ca;
  }

  @media (prefers-color-scheme: dark) {
    .header {
      border-bottom-color: rgba(255, 255, 255, 0.08);
    }
    .url {
      color: #93c5fd;
    }
    .preview {
      border-color: rgba(255, 255, 255, 0.1);
      background: rgba(255, 255, 255, 0.02);
    }
    .payload-text {
      border-color: rgba(255, 255, 255, 0.1);
      background: rgba(255, 255, 255, 0.03);
    }
    .action {
      border-color: rgba(255, 255, 255, 0.15);
      background: rgba(255, 255, 255, 0.05);
    }
    .action:hover {
      background: rgba(255, 255, 255, 0.1);
    }
    .placeholder-hint kbd {
      background: rgba(255, 255, 255, 0.08);
      border-color: rgba(255, 255, 255, 0.12);
    }
    .star-btn:hover {
      background: rgba(255, 255, 255, 0.08);
    }
  }
</style>
