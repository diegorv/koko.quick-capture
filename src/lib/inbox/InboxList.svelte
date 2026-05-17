<script lang="ts">
  // Inbox list pane: one row per Capture. Pure presentational —
  // mutations (star toggle, delete) call back into the parent through
  // injected handlers so the component stays testable in isolation.
  // Slice 03 adds full keyboard navigation: arrow keys move selection,
  // Enter opens, S toggles star, Cmd+Delete soft-deletes, ESC / Cmd+W
  // close. The component owns no row state; selection is driven by the
  // `selectedId` prop and the `onSelect` callback.
  import type { Capture } from "$lib/captures/types";

  interface Props {
    captures: Capture[];
    selectedId: string | null;
    onSelect: (id: string) => void;
    onStarToggle: (id: string, next: boolean) => void;
    onDelete: (id: string) => void;
    onOpen?: (capture: Capture) => void;
    onClose?: () => void;
  }

  let {
    captures,
    selectedId,
    onSelect,
    onStarToggle,
    onDelete,
    onOpen,
    onClose,
  }: Props = $props();

  function kindIcon(kind: Capture["kind"]): string {
    switch (kind) {
      case "Link":
        return "L";
      case "Clip":
        return "C";
      case "Shot":
        return "I";
      case "File":
        return "F";
      case "Note":
        return "N";
    }
  }

  function basename(p: string): string {
    const trimmed = p.replace(/\/+$/, "");
    const slash = trimmed.lastIndexOf("/");
    return slash === -1 ? trimmed : trimmed.slice(slash + 1);
  }

  function preview(capture: Capture): string {
    const p = capture.payload;
    let raw = "";
    switch (capture.kind) {
      case "Note":
      case "Clip":
        raw = typeof p.text === "string" ? p.text : "";
        break;
      case "Link":
        raw = typeof p.url === "string" ? p.url : "";
        break;
      case "File": {
        const name = typeof p.original_name === "string" ? p.original_name : "";
        if (name) {
          raw = name;
        } else {
          raw = typeof p.source_path === "string" ? basename(p.source_path) : "";
        }
        break;
      }
      case "Shot": {
        const src = typeof p.source_path === "string" ? p.source_path : "";
        if (src) {
          raw = basename(src);
        } else if (typeof p.blob_path === "string") {
          raw = basename(p.blob_path);
        } else {
          raw = "Image";
        }
        break;
      }
    }
    const oneLine = raw.replace(/\s+/g, " ").trim();
    return oneLine.length > 80 ? oneLine.slice(0, 80) + "…" : oneLine;
  }

  function relativeTime(createdAt: string, now: number = Date.now()): string {
    const t = Date.parse(createdAt);
    if (Number.isNaN(t)) return "";
    const diff = Math.max(0, now - t);
    const sec = Math.floor(diff / 1000);
    if (sec < 60) return "just now";
    const min = Math.floor(sec / 60);
    if (min < 60) return `${min}m ago`;
    const hr = Math.floor(min / 60);
    if (hr < 24) return `${hr}h ago`;
    const day = Math.floor(hr / 24);
    return `${day}d ago`;
  }

  function selectedIndex(): number {
    if (selectedId === null) return -1;
    return captures.findIndex((c) => c.id === selectedId);
  }

  function selectAt(index: number) {
    if (captures.length === 0) return;
    const clamped = Math.max(0, Math.min(captures.length - 1, index));
    onSelect(captures[clamped].id);
  }

  function handleListKeydown(event: KeyboardEvent) {
    // Cmd+W and Escape close, regardless of selection.
    if (event.key === "Escape" || (event.metaKey && event.key === "w")) {
      event.preventDefault();
      onClose?.();
      return;
    }

    if (event.key === "ArrowDown") {
      event.preventDefault();
      const idx = selectedIndex();
      // No selection yet: pick the first row. Otherwise clamp at end.
      selectAt(idx === -1 ? 0 : idx + 1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      const idx = selectedIndex();
      selectAt(idx === -1 ? 0 : idx - 1);
      return;
    }

    // Remaining keys act on the selected row, if any.
    const idx = selectedIndex();
    if (idx === -1) return;
    const current = captures[idx];

    if (event.key === "Enter") {
      event.preventDefault();
      onOpen?.(current);
      return;
    }
    // `Cmd+Delete` on macOS sends `Backspace` with `metaKey`. We treat
    // both `Backspace` and `Delete` the same for parity with the issue.
    if (event.metaKey && (event.key === "Backspace" || event.key === "Delete")) {
      event.preventDefault();
      onDelete(current.id);
      return;
    }
    // Bare `S` (no modifier) toggles star on the selected row. Capital
    // `S` (shift) is intentionally accepted too — keep parity with the
    // PRD's plain-letter shortcut intent.
    if (
      (event.key === "s" || event.key === "S") &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.altKey
    ) {
      event.preventDefault();
      onStarToggle(current.id, !current.starred);
    }
  }
</script>

<ul
  class="inbox-list"
  role="listbox"
  aria-label="Captures"
  tabindex="0"
  onkeydown={handleListKeydown}
>
  {#each captures as capture (capture.id)}
    <li
      class="row"
      class:selected={capture.id === selectedId}
      role="option"
      aria-selected={capture.id === selectedId}
      tabindex="0"
      onclick={() => onSelect(capture.id)}
      onkeydown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect(capture.id);
        }
      }}
    >
      <span class="kind" aria-label={`kind ${capture.kind}`}>
        {kindIcon(capture.kind)}
      </span>
      <span class="payload">{preview(capture)}</span>
      <span class="time">{relativeTime(capture.created_at)}</span>
      <button
        class="icon star"
        type="button"
        aria-label={capture.starred ? "Unstar capture" : "Star capture"}
        aria-pressed={capture.starred}
        onclick={(e) => {
          e.stopPropagation();
          onStarToggle(capture.id, !capture.starred);
        }}
      >
        {capture.starred ? "★" : "☆"}
      </button>
      <button
        class="icon delete"
        type="button"
        aria-label="Delete capture"
        onclick={(e) => {
          e.stopPropagation();
          onDelete(capture.id);
        }}
      >
        ×
      </button>
    </li>
  {/each}
</ul>

<style>
  .inbox-list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    height: 100%;
    outline: none;
  }

  .row {
    display: grid;
    grid-template-columns: 1.5rem 1fr auto auto auto;
    gap: 0.5rem;
    align-items: center;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid rgba(0, 0, 0, 0.08);
    cursor: pointer;
    outline: none;
  }

  .row.selected {
    background: rgba(0, 0, 0, 0.06);
  }

  .row:focus-visible {
    background: rgba(0, 0, 0, 0.1);
  }

  .kind {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.85rem;
    opacity: 0.7;
  }

  .payload {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .time {
    font-size: 0.8rem;
    opacity: 0.6;
    white-space: nowrap;
  }

  .icon {
    background: transparent;
    border: none;
    padding: 0.1rem 0.35rem;
    cursor: pointer;
    font-size: 1rem;
    color: inherit;
    line-height: 1;
  }

  .icon:hover {
    background: rgba(0, 0, 0, 0.08);
    border-radius: 3px;
  }

  @media (prefers-color-scheme: dark) {
    .row {
      border-bottom-color: rgba(255, 255, 255, 0.08);
    }
    .row.selected {
      background: rgba(255, 255, 255, 0.08);
    }
    .row:focus-visible {
      background: rgba(255, 255, 255, 0.12);
    }
    .icon:hover {
      background: rgba(255, 255, 255, 0.1);
    }
  }
</style>
