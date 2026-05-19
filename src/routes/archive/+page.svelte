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
  import DestinationDot from "$lib/destinations/DestinationDot.svelte";
  import { createPaginatedList } from "$lib/captures/paginated-list.svelte";

  const PAGE_SIZE = 50;

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

  let destinations = $state<Destination[]>([]);
  let destinationFilter = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  // Active [[Name]] mention filter. Closures inside the pager + the
  // search invocation read this reactively so toggling the filter +
  // calling refetchFirst() picks up the new value on the next IPC.
  let activeMention = $state<string | null>(null);

  let unlistenCaptures: UnlistenFn | null = null;
  let unlistenDestinations: UnlistenFn | null = null;
  let unlistenNavigate: UnlistenFn | null = null;

  // Cursor-paginated row store. The pager fetches the whole Archive
  // chronologically; the destination filter is applied client-side
  // over the loaded rows so chip counts (which reflect the loaded
  // subset) keep their meaning across filter switches. Server-side
  // filtering + per-destination totals can move down a future slice
  // when archives grow past a few pages.
  const pager = createPaginatedList({
    pageFn: (cursor, limit) =>
      invokeFn("list_archive", {
        destinationId: null,
        mention: activeMention,
        cursor,
        limit,
      }) as Promise<Capture[]>,
    cursorOf: (last) =>
      last.routed_at ? `${last.routed_at}|${last.id}` : null,
    pageSize: PAGE_SIZE,
  });

  // Search state. `searchResults === null` means "not searching, show
  // the paginated list". Non-null = render results array.
  let searchQuery = $state("");
  let searchResults = $state<Capture[] | null>(null);
  let searchInputEl: HTMLInputElement | undefined = $state();
  let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  const SEARCH_DEBOUNCE_MS = 150;

  const searching = $derived(searchResults !== null);
  const searchOrPaged = $derived(searchResults ?? pager.items);

  const visibleCaptures = $derived(
    destinationFilter === null
      ? searchOrPaged
      : searchOrPaged.filter((c) => c.destination_id === destinationFilter),
  );

  async function runSearch(query: string) {
    const trimmed = query.trim();
    if (!trimmed) {
      searchResults = null;
      return;
    }
    try {
      const results = (await invokeFn("search_archive", {
        query: trimmed,
        destinationId: null,
        mention: activeMention,
        limit: 100,
      })) as Capture[];
      searchResults = results;
    } catch (err) {
      console.error("search_archive failed", err);
      searchResults = [];
    }
  }

  function scheduleSearch() {
    if (searchDebounceTimer !== null) clearTimeout(searchDebounceTimer);
    const snapshot = searchQuery;
    searchDebounceTimer = setTimeout(() => {
      void runSearch(snapshot);
    }, SEARCH_DEBOUNCE_MS);
  }

  function onSearchInput(event: Event) {
    searchQuery = (event.currentTarget as HTMLInputElement).value;
    scheduleSearch();
  }

  function onSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      clearSearch();
    }
  }

  function clearSearch() {
    searchQuery = "";
    searchResults = null;
    if (searchDebounceTimer !== null) {
      clearTimeout(searchDebounceTimer);
      searchDebounceTimer = null;
    }
  }

  const destinationsById = $derived.by(() => {
    const map = new Map<string, Destination>();
    for (const d of destinations) map.set(d.id, d);
    return map;
  });

  // Count of routed Captures per destination on the loaded page —
  // drives the chip badges. (May undercount very long destinations
  // until the user scrolls further; acceptable for v1 since chips
  // exist primarily to surface which buckets contain anything.)
  const countsByDestination = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const c of pager.items) {
      if (!c.destination_id) continue;
      counts.set(c.destination_id, (counts.get(c.destination_id) ?? 0) + 1);
    }
    return counts;
  });

  // Whether any loaded Capture references a destination that isn't in
  // the live list (i.e. soft-deleted destination still holding orphans).
  const hasDeletedDestinations = $derived.by(() => {
    for (const c of pager.items) {
      if (!c.destination_id) continue;
      if (!destinationsById.has(c.destination_id)) return true;
    }
    return false;
  });

  const selectedCapture = $derived(
    selectedId === null
      ? null
      : (visibleCaptures.find((c) => c.id === selectedId) ?? null),
  );

  // Destination row that the selected Capture is currently routed to.
  // `null` covers three cases — no selection, selection not yet routed
  // (e.g. transient state during un-route), and selection pointing at
  // a soft-deleted destination that no longer appears in `destinations`.
  // The detail pane treats `null` as "hide the destination chip".
  const selectedDestination = $derived(
    selectedCapture?.destination_id
      ? (destinationsById.get(selectedCapture.destination_id) ?? null)
      : null,
  );

  async function refresh() {
    try {
      const [, liveDests] = await Promise.all([
        pager.refetchFirst(),
        invokeFn("list_destinations", {}) as Promise<Destination[]>,
      ]);
      destinations = liveDests;
      // Drop the filter if it now points at a soft-deleted destination
      // whose chip just disappeared.
      if (destinationFilter !== null && !destinationsById.has(destinationFilter)) {
        destinationFilter = null;
      }
    } catch (err) {
      console.error("archive refresh failed", err);
    }
  }

  function selectFilter(id: string | null) {
    destinationFilter = id;
    if (
      selectedId !== null &&
      !visibleCaptures.find((c) => c.id === selectedId)
    ) {
      selectedId = null;
    }
  }

  async function setMention(name: string | null) {
    if (activeMention === name) return;
    activeMention = name;
    selectedId = null;
    await pager.refetchFirst();
    if (searching) void runSearch(searchQuery);
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
    pager.remove(id);
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
  let pickerCaptureKind = $state<Capture["kind"] | null>(null);
  let pickerCurrentDest = $state<string | null>(null);

  function onRoute(id: string) {
    const capture = pager.items.find((c) => c.id === id);
    pickerCaptureId = id;
    pickerCaptureKind = capture?.kind ?? null;
    pickerCurrentDest = capture?.destination_id ?? null;
    pickerOpen = true;
  }

  function onPickerClose() {
    pickerOpen = false;
    pickerCaptureId = null;
    pickerCaptureKind = null;
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
    pager.remove(id);
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
    // Listener registration is fire-and-forget after setup — fire all
    // three in parallel so mount completes faster.
    [unlistenCaptures, unlistenDestinations, unlistenNavigate] =
      await Promise.all([
        listenFn(CAPTURES_CHANGED, () => {
          void refresh();
        }),
        listenFn(DESTINATIONS_CHANGED, () => {
          void refresh();
        }),
        listenFn(VIEW_OPEN_INBOX, () => {
          void goto("/inbox");
        }),
      ]);
  });

  onDestroy(() => {
    unlistenCaptures?.();
    unlistenDestinations?.();
    unlistenNavigate?.();
  });
</script>

<div class="archive" data-testid="archive">
  <header class="topbar" data-tauri-drag-region>
    <MainNav active="archive" />
  </header>
  <div class="panes">
    <div class="list-column" class:has-mention={activeMention !== null}>
      <div class="searchbar">
        <input
          bind:this={searchInputEl}
          type="search"
          class="search-input"
          placeholder="Search archive"
          aria-label="Search archive"
          value={searchQuery}
          oninput={onSearchInput}
          onkeydown={onSearchKeydown}
          autocomplete="off"
          spellcheck="false"
        />
        {#if searchQuery}
          <button
            type="button"
            class="search-clear"
            aria-label="Clear search"
            onclick={clearSearch}
          >
            ×
          </button>
        {/if}
      </div>
      {#if activeMention !== null}
        <div class="mention-bar" data-testid="mention-bar">
          <button
            type="button"
            class="mention-pill"
            data-testid="mention-pill-clear"
            onclick={() => void setMention(null)}
            aria-label={`Clear mention filter: ${activeMention}`}
          >
            <span class="mention-pill-label">[[{activeMention}]]</span>
            <span class="mention-pill-close" aria-hidden="true">×</span>
          </button>
        </div>
      {/if}
      <div class="filterbar" role="toolbar" aria-label="Archive filters" data-testid="archive-filter-bar">
        <button
          type="button"
          class="chip"
          class:active={destinationFilter === null}
          aria-pressed={destinationFilter === null}
          onclick={() => selectFilter(null)}
          data-testid="filter-all"
        >
          All <span class="chip-count">{pager.items.length}</span>
        </button>
        {#each destinations as dest (dest.id)}
          <button
            type="button"
            class="chip"
            class:active={destinationFilter === dest.id}
            aria-pressed={destinationFilter === dest.id}
            onclick={() => selectFilter(dest.id)}
            data-testid="filter-chip"
          >
            <DestinationDot color={dest.color} size="0.6rem" />
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
      <section class="list-pane" onscroll={pager.onScroll}>
        {#if searching && visibleCaptures.length === 0}
          <div class="empty">
            <div class="empty-glyph" aria-hidden="true">🔍</div>
            <h2 class="empty-title">No matches</h2>
            <p class="empty-hint">Nothing matched “{searchQuery}”.</p>
          </div>
        {:else if !pager.loading && !searching && pager.items.length === 0}
          <div class="empty">
            <div class="empty-glyph" aria-hidden="true">📤</div>
            <h2 class="empty-title">Nothing routed yet</h2>
            <p class="empty-hint">
              Press <kbd>R</kbd> in the Inbox to send a Capture here.
            </p>
          </div>
        {:else if visibleCaptures.length === 0}
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
          {#if pager.loading}
            <div class="spinner" aria-live="polite">Loading…</div>
          {/if}
        {/if}
      </section>
    </div>
    <section class="detail-pane">
      <InboxDetail
        capture={selectedCapture}
        destination={selectedDestination}
        {onOpenLink}
        {onReveal}
        {onStarToggle}
        {onRoute}
        {onUnroute}
        onMentionClick={(name) => void setMention(name)}
      />
    </section>
  </div>
  <footer class="statusbar" aria-label="Archive stats">
    {#if pager.items.length > 0}
      <span class="stat">{pager.items.length} routed</span>
    {/if}
  </footer>

  <DestinationPicker
    open={pickerOpen}
    captureId={pickerCaptureId}
    captureKind={pickerCaptureKind}
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
    /* Mirror the Inbox grid exactly: 40px titlebar (MainNav over OS
     * chrome) + 1fr panes + 24px statusbar so switching between
     * /inbox and /archive doesn't shift the viewport vertically. */
    grid-template-rows: 40px 1fr 24px;
    height: 100vh;
    width: 100vw;
    overflow: hidden;
  }

  /* Draggable strip — see the Inbox .titlebar comment for the
   * dual-mechanism rationale (CSS app-region for OS-level drag,
   * data-tauri-drag-region attr for the JS fallback). */
  .topbar {
    display: flex;
    align-items: center;
    justify-content: center;
    padding-top: 0.25rem;
    -webkit-app-region: drag;
  }
  .topbar :global(button),
  .topbar :global(input),
  .topbar :global(a) {
    -webkit-app-region: no-drag;
  }

  /* List column mirrors the Inbox layout: searchbar (auto) +
   * filterbar (auto) + scrolling list-pane (1fr). The grid is
   * explicit so the filterbar does not get squished by the 1fr cell
   * when search is empty. */
  .list-column {
    display: grid;
    grid-template-rows: auto auto 1fr;
    min-height: 0;
    border-right: 1px solid rgba(0, 0, 0, 0.08);
  }
  /* When the mention pill is active, expand to a 4-row grid so the
     pill takes its own auto row without pushing list-pane out of
     the 1fr cell. See the Inbox page for the same fix + rationale. */
  .list-column.has-mention {
    grid-template-rows: auto auto auto 1fr;
  }
  @media (prefers-color-scheme: dark) {
    .list-column {
      border-right-color: rgba(255, 255, 255, 0.08);
    }
  }

  .searchbar {
    position: relative;
    padding: 0.55rem 0.75rem 0.4rem;
  }
  .search-input {
    width: 100%;
    appearance: none;
    border: 1px solid rgba(0, 0, 0, 0.12);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.7);
    color: inherit;
    font: inherit;
    font-size: 0.85rem;
    padding: 0.35rem 1.75rem 0.35rem 0.55rem;
  }
  .search-input:focus {
    outline: 2px solid rgba(76, 29, 149, 0.45);
    outline-offset: 0;
    border-color: rgba(76, 29, 149, 0.5);
  }
  @media (prefers-color-scheme: dark) {
    .search-input {
      background: rgba(255, 255, 255, 0.04);
      border-color: rgba(255, 255, 255, 0.15);
    }
  }
  .search-clear {
    position: absolute;
    right: 1rem;
    top: 50%;
    transform: translateY(-50%);
    width: 1.2rem;
    height: 1.2rem;
    border: none;
    background: rgba(0, 0, 0, 0.1);
    border-radius: 999px;
    color: inherit;
    font-size: 0.95rem;
    line-height: 1;
    cursor: pointer;
  }
  @media (prefers-color-scheme: dark) {
    .search-clear {
      background: rgba(255, 255, 255, 0.15);
    }
  }

  .filterbar {
    display: flex;
    gap: 0.35rem;
    overflow-x: auto;
    padding: 0 0.75rem 0.55rem;
  }

  .mention-bar {
    display: flex;
    align-items: center;
    padding: 0 0.75rem 0.5rem;
  }
  .mention-pill {
    appearance: none;
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 1px solid rgba(79, 70, 229, 0.45);
    background: rgba(79, 70, 229, 0.12);
    color: rgba(79, 70, 229, 0.95);
    font: inherit;
    font-size: 0.72rem;
    padding: 0.18rem 0.55rem;
    border-radius: 999px;
    cursor: pointer;
    transition:
      background 80ms ease,
      border-color 80ms ease;
  }
  .mention-pill:hover {
    background: rgba(79, 70, 229, 0.2);
    border-color: rgba(79, 70, 229, 0.65);
  }
  .mention-pill-label {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }
  .mention-pill-close {
    opacity: 0.7;
    font-size: 0.95em;
    line-height: 1;
  }
  @media (prefers-color-scheme: dark) {
    .mention-pill {
      border-color: rgba(165, 180, 252, 0.55);
      background: rgba(165, 180, 252, 0.18);
      color: rgba(165, 180, 252, 0.95);
    }
    .mention-pill:hover {
      background: rgba(165, 180, 252, 0.28);
      border-color: rgba(165, 180, 252, 0.75);
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
  .chip.active {
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
    .chip.active {
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

  .panes {
    display: grid;
    grid-template-columns: minmax(260px, 40%) 1fr;
    min-height: 0;
  }
  .list-pane,
  .detail-pane {
    min-height: 0;
    overflow-y: auto;
  }

  .statusbar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0 0.75rem;
    border-top: 1px solid rgba(0, 0, 0, 0.08);
    font-size: 0.76rem;
    color: rgba(0, 0, 0, 0.55);
  }
  @media (prefers-color-scheme: dark) {
    .statusbar {
      border-top-color: rgba(255, 255, 255, 0.08);
      color: rgba(255, 255, 255, 0.55);
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
  .spinner {
    text-align: center;
    padding: 0.75rem;
    font-size: 0.78rem;
    color: rgba(0, 0, 0, 0.5);
  }
  @media (prefers-color-scheme: dark) {
    .spinner {
      color: rgba(255, 255, 255, 0.5);
    }
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
