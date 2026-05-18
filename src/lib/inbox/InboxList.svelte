<script lang="ts">
  // Inbox list pane: one row per Capture. Pure presentational —
  // mutations (star toggle, delete) call back into the parent through
  // injected handlers so the component stays testable in isolation.
  // Slice 03 adds full keyboard navigation: arrow keys move selection,
  // Enter opens, S toggles star, Cmd+Delete soft-deletes, ESC / Cmd+W
  // close. The component owns no row state; selection is driven by the
  // `selectedId` prop and the `onSelect` callback.
  // Tab order: only the listbox itself is tab-stoppable (roving
  // tabindex pattern for `role="listbox"`). Individual rows are not
  // focusable — selection is owned by the listbox via `aria-activedescendant`.
  import type { Capture } from "$lib/captures/types";
  import {
    Link,
    Clipboard,
    Image as ImageIcon,
    File as FileIcon,
    StickyNote,
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
    captures: Capture[];
    selectedId: string | null;
    onSelect: (id: string) => void;
    onStarToggle: (id: string, next: boolean) => void;
    onDelete: (id: string) => void;
    onOpen?: (capture: Capture) => void;
    onClose?: () => void;
    /** Bare `R` on the selected row triggers triage (ADR-0010). The
     * parent owns the picker; the list just signals "user wants to
     * route this id." */
    onRoute?: (id: string) => void;
    /** `Shift+R` on the selected row un-routes it (Archive only).
     * The parent removes the row from the Archive list and the
     * Capture re-surfaces in the Inbox. */
    onUnroute?: (id: string) => void;
  }

  let {
    captures,
    selectedId,
    onSelect,
    onStarToggle,
    onDelete,
    onOpen,
    onClose,
    onRoute,
    onUnroute,
  }: Props = $props();

  let listEl: HTMLUListElement | undefined = $state();
  // First-time autofocus guard: focus the listbox once captures are
  // available so arrow keys work without requiring a click. Subsequent
  // updates (new captures, mutations) must NOT refocus or we would
  // steal focus from other UI the user has moved to.
  let autofocused = false;

  $effect(() => {
    if (captures.length > 0 && !autofocused && listEl) {
      autofocused = true;
      listEl.focus({ preventScroll: true });
    }
  });

  // Keep the selected row in view as arrow nav advances. `block:
  // "nearest"` is a no-op when the row is already on screen, so a
  // mouse click on a visible row does not jolt the viewport. Reads
  // `selectedId` so the effect re-runs on every selection change.
  // ULIDs are [0-9A-Z]+ so no CSS escaping is needed (jsdom in tests
  // does not implement CSS.escape anyway).
  $effect(() => {
    if (!selectedId || !listEl) return;
    const row = listEl.querySelector<HTMLElement>(
      `#capture-row-${selectedId}`,
    );
    // jsdom (used in unit tests) does not implement scrollIntoView.
    if (row && typeof row.scrollIntoView === "function") {
      row.scrollIntoView({ block: "nearest" });
    }
  });

  // When the inbox window is hidden + reshown, focus is reset on the
  // OS side. Re-grab it on every window-level focus so arrow keys
  // keep working without an extra click. Guarded so we only steal
  // focus when nothing inside the inbox already holds it (e.g. user
  // tabbed to the detail-pane star button).
  $effect(() => {
    if (typeof window === "undefined") return;
    const onWindowFocus = () => {
      if (!listEl) return;
      const active = document.activeElement;
      if (active === document.body || active === null) {
        listEl.focus({ preventScroll: true });
      }
    };
    window.addEventListener("focus", onWindowFocus);
    return () => window.removeEventListener("focus", onWindowFocus);
  });

  function selectAndFocus(id: string) {
    onSelect(id);
    // Restore focus to the listbox after a row click so the keyboard
    // shortcuts (arrows, Enter, S, Cmd+Backspace) work immediately
    // without an extra Tab.
    listEl?.focus({ preventScroll: true });
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

  // Bucket each capture by recency for the sticky date headers
  // rendered between rows. Resolution: Today / Yesterday / This week
  // / This month / Older. The cutoffs are coarse on purpose — minute
  // drift between renders does not change a bucket.
  function dateBucket(iso: string, ref: number): string {
    const t = Date.parse(iso);
    if (Number.isNaN(t)) return "Older";
    const diff = ref - t;
    const day = 86_400_000;
    if (diff < day) return "Today";
    if (diff < day * 2) return "Yesterday";
    if (diff < day * 7) return "This week";
    if (diff < day * 30) return "This month";
    return "Older";
  }

  type GroupItem =
    | { kind: "header"; label: string }
    | { kind: "row"; capture: Capture };

  const groupedItems = $derived.by<GroupItem[]>(() => {
    const now = Date.now();
    const items: GroupItem[] = [];
    let last: string | null = null;
    for (const capture of captures) {
      const bucket = dateBucket(capture.created_at, now);
      if (bucket !== last) {
        items.push({ kind: "header", label: bucket });
        last = bucket;
      }
      items.push({ kind: "row", capture });
    }
    return items;
  });

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
      return;
    }
    // `R` opens the triage picker for the selected row (ADR-0010).
    // `Shift+R` un-routes (only meaningful in the Archive view, where
    // the parent provides `onUnroute`).
    if (
      (event.key === "r" || event.key === "R") &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.altKey
    ) {
      if (event.shiftKey) {
        if (!onUnroute) return;
        event.preventDefault();
        onUnroute(current.id);
        return;
      }
      if (!onRoute) return;
      event.preventDefault();
      onRoute(current.id);
    }
  }
</script>

<ul
  bind:this={listEl}
  class="inbox-list"
  role="listbox"
  aria-label="Captures"
  tabindex="0"
  aria-activedescendant={selectedId ? `capture-row-${selectedId}` : undefined}
  onkeydown={handleListKeydown}
>
  {#each groupedItems as item (item.kind === "row" ? item.capture.id : `h:${item.label}`)}
    {#if item.kind === "header"}
      <li class="date-header" role="presentation" aria-hidden="true">
        {item.label}
      </li>
    {:else}
      {@const capture = item.capture}
      {@const KindIcon = KIND_ICONS[capture.kind]}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <li
        id={`capture-row-${capture.id}`}
        class="row"
        class:selected={capture.id === selectedId}
        class:unread={capture.read_at === null}
        role="option"
        aria-selected={capture.id === selectedId}
        onclick={() => selectAndFocus(capture.id)}
      >
      <span class="unread-dot" aria-hidden="true"></span>
      <span class="kind" aria-label={`kind ${capture.kind}`}>
        <KindIcon size={16} strokeWidth={1.75} />
      </span>
      <span class="payload">{preview(capture)}</span>
      <span class="time">{relativeTime(capture.created_at)}</span>
      <button
        class="icon star"
        class:active={capture.starred}
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
    {/if}
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
    grid-template-columns: 0.5rem 1.5rem 1fr auto auto auto;
    gap: 0.5rem;
    align-items: center;
    padding: 0.65rem 0.75rem 0.65rem 0.5rem;
    border-bottom: 1px solid rgba(0, 0, 0, 0.05);
    cursor: pointer;
    transition: background 80ms ease;
  }

  /* Date-bucket separators rendered between rows. Sticky so the
     header for the current group stays pinned to the top of the
     listbox viewport as the user scrolls within that group. */
  .date-header {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 0.3rem 0.85rem;
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-weight: 600;
    color: rgba(0, 0, 0, 0.5);
    background: #f6f6f6;
    border-bottom: 1px solid rgba(0, 0, 0, 0.05);
    user-select: none;
  }

  /* Per-item unread dot. Hidden by default; only visible on rows
     whose `read_at` is still null. Reserves its grid column on every
     row so payloads never reflow when a row flips read. */
  .unread-dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: transparent;
  }

  .row.unread .unread-dot {
    background: #4c1d95;
  }

  .row.unread .payload {
    font-weight: 600;
  }

  .row:hover {
    background: rgba(0, 0, 0, 0.03);
  }

  .row.selected {
    background: rgba(79, 70, 229, 0.1);
  }

  .inbox-list:focus-visible .row.selected {
    background: rgba(79, 70, 229, 0.18);
    box-shadow: inset 2px 0 0 rgba(79, 70, 229, 0.9);
  }

  .kind {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    opacity: 0.7;
  }

  .payload {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.9rem;
  }

  .time {
    font-size: 0.75rem;
    opacity: 0.55;
    white-space: nowrap;
    margin-right: 0.25rem;
  }

  .icon {
    background: transparent;
    border: none;
    padding: 0.2rem 0.4rem;
    cursor: pointer;
    font-size: 0.95rem;
    color: inherit;
    opacity: 0;
    line-height: 1;
    border-radius: 4px;
    transition: opacity 80ms ease, background 80ms ease;
  }

  /* Reveal action icons when the row is hovered or selected. Starred
     rows keep the star visible at all times so the affordance does
     not disappear on idle. */
  .row:hover .icon,
  .row.selected .icon,
  .icon.star.active {
    opacity: 0.55;
  }

  .icon.star.active {
    color: #f59e0b;
  }

  .icon:hover {
    opacity: 1;
    background: rgba(0, 0, 0, 0.08);
  }

  @media (prefers-color-scheme: dark) {
    .row {
      border-bottom-color: rgba(255, 255, 255, 0.06);
    }
    .row.unread .unread-dot {
      background: #a78bfa;
    }
    .date-header {
      background: #1c1c1c;
      color: rgba(255, 255, 255, 0.5);
      border-bottom-color: rgba(255, 255, 255, 0.06);
    }
    .row:hover {
      background: rgba(255, 255, 255, 0.04);
    }
    .row.selected {
      background: rgba(99, 102, 241, 0.18);
    }
    .inbox-list:focus-visible .row.selected {
      background: rgba(99, 102, 241, 0.28);
      box-shadow: inset 2px 0 0 rgba(129, 140, 248, 0.95);
    }
    .icon:hover {
      background: rgba(255, 255, 255, 0.1);
    }
  }
</style>
