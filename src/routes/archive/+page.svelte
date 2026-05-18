<script lang="ts">
  // Archive view. Lists Routed Captures (per ADR-0010) inside the
  // main window's shared shell. Filter bar above the list scopes by
  // Destination. Re-route / un-route actions land in slice 6; this
  // page covers the read path + chip filter + the switcher.
  //
  // Mirrors the Inbox page's injectable-deps pattern so tests can
  // drive it without a Tauri runtime.

  import { onMount, onDestroy } from "svelte";
  import { invoke as tauriInvoke } from "@tauri-apps/api/core";
  import { listen as tauriListen } from "@tauri-apps/api/event";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { goto } from "$app/navigation";
  import type { Capture, Destination } from "$lib/captures/types";
  import {
    CAPTURES_CHANGED,
    DESTINATIONS_CHANGED,
    VIEW_OPEN_INBOX,
  } from "$lib/events";
  import InboxList from "$lib/inbox/InboxList.svelte";
  import InboxDetail from "$lib/inbox/InboxDetail.svelte";
  import MainNav from "$lib/main/MainNav.svelte";
  import DestinationPicker from "$lib/destinations/DestinationPicker.svelte";
  import { colorHex } from "$lib/destinations/palette";

  const PAGE_SIZE = 100;

  type InvokeFn = (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
  type ListenFn = (
    event: string,
    handler: (payload: unknown) => void,
  ) => Promise<UnlistenFn>;
  type HideFn = () => Promise<void>;

  interface Props {
    invokeFn?: InvokeFn;
    listenFn?: ListenFn;
    hideFn?: HideFn;
  }

  const defaultInvoke: InvokeFn = (cmd, args) => tauriInvoke(cmd, args);
  const defaultListen: ListenFn = (event, handler) =>
    tauriListen(event, (e) => handler(e.payload));
  const defaultHide: HideFn = () => tauriInvoke("hide_inbox");

  const {
    invokeFn = defaultInvoke,
    listenFn = defaultListen,
    hideFn = defaultHide,
  }: Props = $props();

  let captures = $state<Capture[]>([]);
  let destinations = $state<Destination[]>([]);
  let destinationFilter = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  let loading = $state(false);

  let unlistenCaptures: UnlistenFn | null = null;
  let unlistenDestinations: UnlistenFn | null = null;
  let unlistenNavigate: UnlistenFn | null = null;

  const destinationsById = $derived.by(() => {
    const map = new Map<string, Destination>();
    for (const d of destinations) map.set(d.id, d);
    return map;
  });

  // Count of routed Captures per destination — drives the chip badges.
  const countsByDestination = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const c of captures) {
      if (!c.destination_id) continue;
      counts.set(c.destination_id, (counts.get(c.destination_id) ?? 0) + 1);
    }
    return counts;
  });

  // Set of destination ids present in the current Archive page that
  // are NOT in the live destinations list — i.e. soft-deleted
  // destinations whose Captures still surface here.
  const hasDeletedDestinations = $derived.by(() => {
    for (const c of captures) {
      if (!c.destination_id) continue;
      if (!destinationsById.has(c.destination_id)) return true;
    }
    return false;
  });

  const visibleCaptures = $derived(
    destinationFilter === null
      ? captures
      : captures.filter((c) => c.destination_id === destinationFilter),
  );

  const selectedCapture = $derived(
    selectedId === null
      ? null
      : (visibleCaptures.find((c) => c.id === selectedId) ?? null),
  );

  async function refresh() {
    loading = true;
    try {
      const [archiveRows, liveDests] = await Promise.all([
        invokeFn("list_archive", {
          destinationId: null,
          limit: PAGE_SIZE,
        }) as Promise<Capture[]>,
        invokeFn("list_destinations", {}) as Promise<Destination[]>,
      ]);
      captures = archiveRows;
      destinations = liveDests;
      // Drop the filter if it now points at a soft-deleted destination
      // whose chip just disappeared.
      if (destinationFilter !== null && !destinationsById.has(destinationFilter)) {
        destinationFilter = null;
      }
    } catch (err) {
      console.error("archive refresh failed", err);
    } finally {
      loading = false;
    }
  }

  function selectFilter(id: string | null) {
    destinationFilter = id;
    // Clear selection if it was filtered out.
    if (
      selectedId !== null &&
      !visibleCaptures.find((c) => c.id === selectedId)
    ) {
      selectedId = null;
    }
  }

  function onSelect(id: string) {
    selectedId = id;
  }

  async function onStarToggle(id: string, next: boolean) {
    try {
      await invokeFn("star_capture", { id, starred: next });
    } catch (err) {
      console.error("star_capture failed", err);
    }
  }

  async function onDelete(id: string) {
    captures = captures.filter((c) => c.id !== id);
    if (selectedId === id) selectedId = null;
    try {
      await invokeFn("delete_capture", { id });
    } catch (err) {
      console.error("delete_capture failed", err);
    }
  }

  function onOpenLink(url: string) {
    invokeFn("open_link", { url }).catch((err) => {
      console.error("open_link failed", err);
    });
  }

  function onReveal(id: string) {
    invokeFn("reveal_capture", { id }).catch((err) => {
      console.error("reveal_capture failed", err);
    });
  }

  function onOpen(capture: Capture) {
    if (capture.kind === "Link") {
      const url = typeof capture.payload.url === "string" ? capture.payload.url : "";
      if (url) onOpenLink(url);
      return;
    }
    if (capture.kind === "Clip" || capture.kind === "Note") return;
    onReveal(capture.id);
  }

  async function onClose() {
    try {
      await hideFn();
    } catch (err) {
      console.error("hide window failed", err);
    }
  }

  // ── Re-route + un-route (ADR-0010 slice 6) ──────────────────────
  let pickerOpen = $state(false);
  let pickerCaptureId = $state<string | null>(null);
  let pickerCurrentDest = $state<string | null>(null);

  function onRoute(id: string) {
    const capture = captures.find((c) => c.id === id);
    pickerCaptureId = id;
    pickerCurrentDest = capture?.destination_id ?? null;
    pickerOpen = true;
  }

  function onPickerClose() {
    pickerOpen = false;
    pickerCaptureId = null;
    pickerCurrentDest = null;
  }

  function onPickerAssigned(_destinationId: string) {
    // No row removal here — re-routing keeps the Capture in the
    // Archive. Just clear selection if the filter no longer covers it
    // and let `captures:changed` trigger a refresh.
    const id = pickerCaptureId;
    if (id === null) return;
    if (selectedId === id) {
      // Selection stays; refresh updates the destination_id.
    }
  }

  async function onUnroute(id: string) {
    // Optimistic: yank the row from the Archive view. The Inbox
    // surfaces it again when the user switches tabs.
    captures = captures.filter((c) => c.id !== id);
    if (selectedId === id) selectedId = null;
    try {
      await invokeFn("unroute_capture", { id });
    } catch (err) {
      console.error("unroute_capture failed", err);
      // Refresh on failure to put the row back if it should still be
      // here.
      await refresh();
    }
  }

  onMount(async () => {
    await refresh();
    unlistenCaptures = await listenFn(CAPTURES_CHANGED, () => {
      void refresh();
    });
    unlistenDestinations = await listenFn(DESTINATIONS_CHANGED, () => {
      void refresh();
    });
    unlistenNavigate = await listenFn(VIEW_OPEN_INBOX, () => {
      void goto("/inbox");
    });
  });

  onDestroy(() => {
    if (unlistenCaptures) unlistenCaptures();
    if (unlistenDestinations) unlistenDestinations();
    if (unlistenNavigate) unlistenNavigate();
  });
</script>

<div class="archive" data-testid="archive">
  <header class="topbar">
    <MainNav active="archive" />
  </header>

  <div class="filter-bar" data-testid="archive-filter-bar">
    <button
      type="button"
      class="chip"
      class:selected={destinationFilter === null}
      onclick={() => selectFilter(null)}
      data-testid="filter-all"
    >
      All <span class="chip-count">{captures.length}</span>
    </button>
    {#each destinations as dest (dest.id)}
      <button
        type="button"
        class="chip"
        class:selected={destinationFilter === dest.id}
        onclick={() => selectFilter(dest.id)}
        data-testid="filter-chip"
      >
        {#if dest.color}
          <span
            class="dot"
            style="background-color: {colorHex(dest.color)};"
            aria-hidden="true"
          ></span>
        {:else}
          <span class="dot dot-empty" aria-hidden="true"></span>
        {/if}
        <span class="chip-name">{dest.name}</span>
        <span class="chip-count">{countsByDestination.get(dest.id) ?? 0}</span>
      </button>
    {/each}
    {#if hasDeletedDestinations}
      <span class="chip ghost" data-testid="filter-deleted-hint">
        (Some Captures reference soft-deleted destinations)
      </span>
    {/if}
  </div>

  <div class="panes">
    <section class="list-pane">
      {#if !loading && captures.length === 0}
        <div class="empty">
          <div class="empty-glyph" aria-hidden="true">📤</div>
          <h2 class="empty-title">Nothing routed yet</h2>
          <p class="empty-hint">
            Press <kbd>R</kbd> in the Inbox to send a Capture here.
          </p>
        </div>
      {:else if !loading && visibleCaptures.length === 0}
        <div class="empty">
          <h2 class="empty-title">No matches</h2>
          <p class="empty-hint">No Captures routed to this destination.</p>
        </div>
      {:else}
        <InboxList
          captures={visibleCaptures}
          {selectedId}
          {onSelect}
          {onStarToggle}
          {onDelete}
          {onOpen}
          {onClose}
          {onRoute}
          {onUnroute}
        />
      {/if}
    </section>
    <section class="detail-pane">
      <InboxDetail
        capture={selectedCapture}
        {onOpenLink}
        {onReveal}
        {onStarToggle}
        {onRoute}
        {onUnroute}
      />
    </section>
  </div>

  <DestinationPicker
    open={pickerOpen}
    captureId={pickerCaptureId}
    currentDestinationId={pickerCurrentDest}
    invokeFn={(cmd, args) => invokeFn(cmd, args ?? {})}
    onClose={onPickerClose}
    onAssigned={onPickerAssigned}
  />
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    background: #f6f6f6;
    color: #0f0f0f;
    font-family: Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  }
  @media (prefers-color-scheme: dark) {
    :global(html),
    :global(body) {
      background: #1c1c1c;
      color: #f4f4f4;
    }
  }

  .archive {
    display: grid;
    grid-template-rows: 28px auto 1fr;
    height: 100vh;
    overflow: hidden;
  }

  .topbar {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0.3rem 0.6rem 0;
    -webkit-app-region: drag;
  }
  .topbar :global(button),
  .topbar :global(input) {
    -webkit-app-region: no-drag;
  }

  .filter-bar {
    display: flex;
    gap: 0.35rem;
    overflow-x: auto;
    padding: 0.55rem 0.75rem;
    border-bottom: 1px solid rgba(0, 0, 0, 0.08);
  }
  @media (prefers-color-scheme: dark) {
    .filter-bar {
      border-bottom-color: rgba(255, 255, 255, 0.08);
    }
  }

  .chip {
    appearance: none;
    font: inherit;
    font-size: 0.78rem;
    border: 1px solid rgba(0, 0, 0, 0.12);
    background: rgba(255, 255, 255, 0.6);
    color: rgba(0, 0, 0, 0.75);
    padding: 0.2rem 0.55rem;
    border-radius: 999px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    flex-shrink: 0;
    transition:
      background 80ms ease,
      border-color 80ms ease;
  }
  .chip:hover {
    background: rgba(76, 29, 149, 0.08);
  }
  .chip.selected {
    background: rgba(76, 29, 149, 0.15);
    border-color: rgba(76, 29, 149, 0.6);
    color: rgba(76, 29, 149, 1);
  }
  @media (prefers-color-scheme: dark) {
    .chip {
      background: rgba(255, 255, 255, 0.04);
      border-color: rgba(255, 255, 255, 0.12);
      color: rgba(255, 255, 255, 0.7);
    }
    .chip:hover {
      background: rgba(167, 139, 250, 0.12);
    }
    .chip.selected {
      background: rgba(167, 139, 250, 0.2);
      border-color: rgba(167, 139, 250, 0.6);
      color: rgba(196, 181, 253, 1);
    }
  }
  .chip.ghost {
    cursor: default;
    opacity: 0.7;
    font-style: italic;
  }

  .chip-count {
    background: rgba(0, 0, 0, 0.07);
    border-radius: 999px;
    padding: 0.02rem 0.35rem;
    font-size: 0.72rem;
  }
  @media (prefers-color-scheme: dark) {
    .chip-count {
      background: rgba(255, 255, 255, 0.1);
    }
  }

  .dot {
    width: 0.6rem;
    height: 0.6rem;
    border-radius: 999px;
    flex-shrink: 0;
  }
  .dot-empty {
    border: 1px dashed rgba(0, 0, 0, 0.2);
    background: transparent;
  }
  @media (prefers-color-scheme: dark) {
    .dot-empty {
      border-color: rgba(255, 255, 255, 0.2);
    }
  }

  .panes {
    display: grid;
    grid-template-columns: minmax(260px, 40%) 1fr;
    min-height: 0;
  }
  .list-pane,
  .detail-pane {
    min-height: 0;
    overflow-y: auto;
    border-right: 1px solid rgba(0, 0, 0, 0.08);
  }
  .detail-pane {
    border-right: none;
  }
  @media (prefers-color-scheme: dark) {
    .list-pane {
      border-right-color: rgba(255, 255, 255, 0.08);
    }
  }

  .empty {
    padding: 2rem 1.5rem;
    text-align: center;
    color: rgba(0, 0, 0, 0.5);
  }
  @media (prefers-color-scheme: dark) {
    .empty {
      color: rgba(255, 255, 255, 0.55);
    }
  }
  .empty-glyph {
    font-size: 2rem;
    margin-bottom: 0.4rem;
  }
  .empty-title {
    margin: 0;
    font-size: 1rem;
    color: inherit;
  }
  .empty-hint {
    margin: 0.3rem 0 0;
    font-size: 0.85rem;
  }
  .empty kbd {
    display: inline-block;
    min-width: 1.5em;
    text-align: center;
    padding: 0.05em 0.4em;
    font: inherit;
    font-size: 0.78em;
    background: rgba(0, 0, 0, 0.06);
    border: 1px solid rgba(0, 0, 0, 0.1);
    border-bottom-width: 2px;
    border-radius: 4px;
  }
  @media (prefers-color-scheme: dark) {
    .empty kbd {
      background: rgba(255, 255, 255, 0.08);
      border-color: rgba(255, 255, 255, 0.12);
    }
  }
</style>
