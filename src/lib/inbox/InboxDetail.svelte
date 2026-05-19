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
  import type { Capture, Destination } from "$lib/captures/types";
  import { parseMentionSegments } from "$lib/mentions/parse-mention-segments";
  import DestinationDot from "$lib/destinations/DestinationDot.svelte";
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
    /** Optional [[Name]] click handler. When provided, mentions
     * inside Note / Clip text render as clickable spans that
     * surface the picked name to the parent (which sets the list's
     * mention filter). When omitted, mentions render as plain text. */
    onMentionClick?: (name: string) => void;
    /** Destination this Capture was Routed to, when applicable. The
     * Archive page resolves it from `capture.destination_id` against
     * its live destinations map; the Inbox page omits the prop. Pass
     * `null` to hide the chip — also what the Archive does when the
     * routed destination has been soft-deleted and dropped out of
     * the live map (orphans surface via the filter-bar hint instead). */
    destination?: Destination | null;
  }

  const {
    capture,
    onOpenLink,
    onReveal,
    onStarToggle,
    onRoute,
    onUnroute,
    onMentionClick,
    destination = null,
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

  function hasSource(c: Capture): boolean {
    return Boolean(c.source_app || c.source_title || c.source_url);
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
      <p class="meta-time" data-testid="detail-timestamps">
        {#if capture.routed_at}
          <span class="meta-label">routed</span>
          <span class="meta-value" title="Routed">
            {formatTimestamp(capture.routed_at)}
          </span>
          <span class="meta-sep" aria-hidden="true">·</span>
          <span class="meta-label">captured</span>
          <span class="meta-value" title="Captured">
            {formatTimestamp(capture.created_at)}
          </span>
        {:else}
          <span class="meta-label">captured</span>
          <span class="meta-value" title="Captured">
            {formatTimestamp(capture.created_at)}
          </span>
        {/if}
      </p>

      {#if hasSource(capture) || (destination && capture.destination_id)}
        <dl class="meta-rel">
          {#if hasSource(capture)}
            <dt>From</dt>
            <dd data-testid="detail-source">
              {#if capture.source_app}
                <span class="rel-app" title={capture.source_app}>
                  {capture.source_app}
                </span>
              {/if}
              {#if capture.source_app && (capture.source_title || capture.source_url)}
                <span class="rel-sep" aria-hidden="true">·</span>
              {/if}
              {#if capture.source_title && capture.source_url}
                <!-- Title doubles as the link to source_url so the user
                     does not have to scan a separate hostname token to
                     follow the source. -->
                <button
                  type="button"
                  class="rel-link"
                  title={capture.source_url}
                  onclick={() => onOpenLink(capture.source_url!)}
                >
                  “{capture.source_title}” ↗
                </button>
              {:else if capture.source_title}
                <span class="rel-title" title={capture.source_title}>
                  “{capture.source_title}”
                </span>
              {:else if capture.source_url}
                <button
                  type="button"
                  class="rel-link"
                  title={capture.source_url}
                  onclick={() => onOpenLink(capture.source_url!)}
                >
                  {hostnameOf(capture.source_url)} ↗
                </button>
              {/if}
            </dd>
          {/if}
          {#if destination && capture.destination_id}
            <dt>To</dt>
            <dd
              data-testid="detail-destination"
              title={destination.deleted_at ? `${destination.name} (deleted)` : destination.name}
            >
              <DestinationDot color={destination.color} size="0.55rem" />
              <span class="rel-dest-name">{destination.name}</span>
              {#if destination.deleted_at}
                <span class="rel-dest-deleted">(deleted)</span>
              {/if}
            </dd>
          {/if}
        </dl>
      {/if}
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
      {@const segments = parseMentionSegments(text)}
      <section class="body">
        <pre class="payload-text" data-testid="payload-text">{#each segments as segment, i (i)}{#if segment.kind === "mention" && onMentionClick}<button
              type="button"
              class="mention"
              data-testid="mention-chip"
              data-mention={segment.value}
              onclick={() => onMentionClick(segment.value)}
            >[[{segment.value}]]</button>{:else if segment.kind === "mention"}[[{segment.value}]]{:else}{segment.value}{/if}{/each}</pre>
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
  /* Type scale — keep all rem sizes on this ladder so the detail
     pane reads as one piece of typography instead of a patchwork.
       0.7rem  — uppercase labels (dt) only
       0.8rem  — meta / secondary body / secondary buttons
       0.9rem  — primary body text + primary action button
       1.05rem — header title (`.kind`)
     The `.star-btn` glyph and the `.placeholder-hint kbd` use em
     units intentionally (they scale with their containing line).

     Color/contrast scale — every field name (timestamp labels,
     uppercase dt) renders at one muted opacity; every value (the
     timestamp itself, source app, source title, destination name,
     mime, path) renders at full foreground. The single shared rule
     for each tier sits a few blocks below this header — search for
     `.meta-label`. Separators (`·`) sit further down still and
     keep their own lighter weight so they read as gaps, not data. */
  .detail {
    display: flex;
    flex-direction: column;
    height: 100%;
    box-sizing: border-box;
    /* Header and footer stay pinned; only the body scrolls. Without
       this the whole pane scrolls together, so a tall Shot preview
       can push the Reveal / Open button off-screen or overlap it on
       short windows. */
    overflow: hidden;
    min-height: 0;
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
    font-size: 0.9rem;
    margin: 0;
    opacity: 0.55;
  }

  .placeholder-hint {
    margin: 0;
    font-size: 0.8rem;
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
    font-size: 1.05rem;
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
    font-size: 0.8rem;
    font-weight: 500;
    padding: 0.22rem 0.65rem;
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

  /* Timestamps sit on their own line so "when" reads as one fact,
     separate from the relational "where" metadata below. Per-span
     `.meta-label` / `.meta-value` opacities create the label/value
     contrast — see the type-and-color scale comment at the top. */
  .meta-time {
    margin: 0.4rem 0 0;
    font-size: 0.8rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0 0.35rem;
    align-items: baseline;
    min-width: 0;
    max-width: 100%;
  }

  /* Every field name (inline timestamp labels + `dt` labels in the
     two definition lists) renders with the same case, size, weight,
     letter-spacing, and opacity so they read as a single visual
     layer. Per-context overrides below stick to non-label concerns
     (column padding for `.meta dt`, etc.). */
  .meta-label,
  .meta-rel dt,
  .meta dt {
    text-transform: uppercase;
    font-size: 0.7rem;
    letter-spacing: 0.06em;
    font-weight: 600;
    opacity: 0.55;
  }
  /* Value-side text inherits the pane's foreground at full strength
     so timestamps, source app, destination name, mime and path all
     read at the same brightness. Italic or mono variants tweak the
     family/style without touching opacity. */
  .meta-value,
  .meta-rel dd,
  .meta dd {
    opacity: 1;
  }
  .meta-sep,
  .rel-sep {
    opacity: 0.3;
  }

  /* Definition-list grid for relational metadata (From / To). Left
     column is an aligned uppercase label, right column is the dense
     value. Sharing one grid keeps both rows' labels aligned even
     when only one of them is present. */
  .meta-rel {
    margin: 0.5rem 0 0;
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    column-gap: 0.7rem;
    row-gap: 0.3rem;
    font-size: 0.8rem;
    align-items: baseline;
    min-width: 0;
    max-width: 100%;
  }
  .meta-rel dt {
    padding-top: 0.05rem;
  }
  .meta-rel dd {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 0.35rem;
    min-width: 0;
    overflow: hidden;
  }

  .rel-app {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.8rem;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .rel-title {
    font-style: italic;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .rel-link {
    appearance: none;
    border: none;
    background: transparent;
    padding: 0;
    margin: 0;
    color: rgba(76, 29, 149, 0.95);
    font: inherit;
    font-style: italic;
    cursor: pointer;
    text-align: left;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rel-link:hover {
    text-decoration: underline;
  }
  @media (prefers-color-scheme: dark) {
    .rel-link {
      color: rgba(167, 139, 250, 0.95);
    }
  }

  .rel-dest-name {
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .rel-dest-deleted {
    opacity: 0.55;
    font-style: italic;
    font-size: 0.8rem;
    flex-shrink: 0;
  }

  .body {
    padding: 1rem 1.5rem;
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }

  .url {
    margin: 0;
    font-size: 0.9rem;
    word-break: break-all;
    color: #2563eb;
  }

  .filename {
    margin: 0;
    font-size: 0.9rem;
    font-weight: 500;
    word-break: break-word;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.35rem 1rem;
    margin: 0;
    font-size: 0.8rem;
  }

  .meta dt {
    padding-top: 0.15rem;
  }

  .meta dd {
    margin: 0;
    word-break: break-all;
  }

  .mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.8rem;
  }

  .preview {
    max-width: 100%;
    /* Cap to a fraction of the viewport so a vertical screenshot on
       a tall window does not consume the entire body and collide with
       the Reveal in Finder button. The body scrolls if the image plus
       metadata exceeds the available space. */
    max-height: 40vh;
    object-fit: contain;
    align-self: center;
    flex-shrink: 0;
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
    font-size: 0.9rem;
    line-height: 1.5;
  }

  /* Mention chips inside the Note/Clip payload pre. Rendered as
     inline buttons so the user can click to set the list filter
     without disturbing the surrounding monospace layout. */
  .mention {
    appearance: none;
    border: none;
    background: transparent;
    padding: 0;
    margin: 0;
    font: inherit;
    color: rgba(79, 70, 229, 0.95);
    cursor: pointer;
    border-radius: 3px;
  }
  .mention:hover,
  .mention:focus-visible {
    background: rgba(79, 70, 229, 0.12);
    outline: none;
  }
  @media (prefers-color-scheme: dark) {
    .mention {
      color: rgba(165, 180, 252, 0.95);
    }
    .mention:hover,
    .mention:focus-visible {
      background: rgba(165, 180, 252, 0.18);
    }
  }

  .actions {
    padding: 0.75rem 1.5rem 1.25rem;
    display: flex;
    gap: 0.5rem;
    flex-shrink: 0;
    /* Top border separates the pinned footer from the scrolling
       body so a tall preview that scrolled into the footer area is
       visually distinct from the action buttons. */
    border-top: 1px solid rgba(0, 0, 0, 0.06);
    background: inherit;
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
    .actions {
      border-top-color: rgba(255, 255, 255, 0.08);
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
