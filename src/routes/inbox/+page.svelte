<script lang="ts">
  // Inbox window root. Owns list state and the live-update subscription;
  // delegates row rendering to InboxList. The Tauri adapters (`invoke`,
  // `listen`, `hideWindow`) are injected as props so the page can be
  // mounted in a test without a Tauri runtime.
  //
  // Slice 03 wires star + delete to the Rust commands and reacts to
  // mutation events on `captures.changed` by refetching the first page.
  // The event payload is either a full `Capture` (slice 02 on save) or
  // a `MutationNotice` `{id, kind: "starred" | "deleted"}` (slice 03
  // mutations).
  import { onMount, onDestroy } from "svelte";
  import { invoke as tauriInvoke } from "@tauri-apps/api/core";
  import { listen as tauriListen } from "@tauri-apps/api/event";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import type { Capture } from "$lib/captures/types";
  import { CAPTURES_CHANGED } from "$lib/events";
  import InboxList from "$lib/inbox/InboxList.svelte";
  import InboxDetail from "$lib/inbox/InboxDetail.svelte";

  const PAGE_SIZE = 50;
  const SCROLL_THRESHOLD_PX = 100;

  type MutationNotice = { id: string; kind: "starred" | "deleted" };
  type ChangedPayload = Capture | MutationNotice;

  type ListFn = (
    cursor: string | null,
    limit: number,
  ) => Promise<Capture[]>;
  type ListenFn = (
    event: string,
    handler: (payload: ChangedPayload) => void,
  ) => Promise<UnlistenFn>;
  type InvokeFn = (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
  type HideFn = () => Promise<void>;

  interface Props {
    listFn?: ListFn;
    listenFn?: ListenFn;
    invokeFn?: InvokeFn;
    hideFn?: HideFn;
  }

  const defaultList: ListFn = (cursor, limit) =>
    tauriInvoke<Capture[]>("list_captures", { cursor, limit });

  const defaultListen: ListenFn = (event, handler) =>
    tauriListen<ChangedPayload>(event, (e) => handler(e.payload));

  const defaultInvoke: InvokeFn = (cmd, args) => tauriInvoke(cmd, args);

  // hide_inbox (Rust command) hides the window AND reverts the macOS
  // activation policy to Accessory. A plain getCurrentWindow().hide()
  // would leave the system Dock icon stuck because Tauri does not
  // fire CloseRequested for a programmatic hide.
  const defaultHide: HideFn = () => tauriInvoke("hide_inbox");

  const {
    listFn = defaultList,
    listenFn = defaultListen,
    invokeFn = defaultInvoke,
    hideFn = defaultHide,
  }: Props = $props();

  let captures = $state<Capture[]>([]);
  let selectedId = $state<string | null>(null);
  let loading = $state(false);
  let exhausted = $state(false);
  let unlisten: UnlistenFn | null = null;
  let totalCount = $state<number | null>(null);
  let unreadCount = $state<number | null>(null);
  let now = $state(Date.now());
  let nowTimer: ReturnType<typeof setInterval> | null = null;

  // Search state. `searchResults === null` means "not searching, show
  // the paginated `captures` list". Non-null means search is active
  // and InboxList renders the results array. The debounce timer
  // collapses bursts of keystrokes into one search call.
  let searchQuery = $state("");
  let searchResults = $state<Capture[] | null>(null);
  let searchInputEl: HTMLInputElement | undefined = $state();
  let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  const SEARCH_DEBOUNCE_MS = 150;

  const searching = $derived(searchResults !== null);
  const searchOrPageResults = $derived(searchResults ?? captures);

  // Client-side filters layered on top of whatever list (search
  // results OR paginated page) is currently active. Kind filter is
  // single-select; `null` means "any kind". Starred is a boolean
  // toggle. Both apply post-hoc so swapping search on/off does not
  // re-trigger them.
  let kindFilter = $state<Capture["kind"] | null>(null);
  let starredOnly = $state(false);

  const filtersActive = $derived(kindFilter !== null || starredOnly);
  const visibleCaptures = $derived(
    !filtersActive
      ? searchOrPageResults
      : searchOrPageResults.filter((c) => {
          if (kindFilter !== null && c.kind !== kindFilter) return false;
          if (starredOnly && !c.starred) return false;
          return true;
        }),
  );

  const KIND_OPTIONS: Array<{ value: Capture["kind"] | null; label: string }> = [
    { value: null, label: "All" },
    { value: "Note", label: "Notes" },
    { value: "Link", label: "Links" },
    { value: "Clip", label: "Clips" },
    { value: "Shot", label: "Shots" },
    { value: "File", label: "Files" },
  ];

  function setKindFilter(value: Capture["kind"] | null) {
    kindFilter = value;
    // If current selection was filtered out, clear it.
    if (
      selectedId !== null &&
      !visibleCaptures.some((c) => c.id === selectedId)
    ) {
      selectedId = null;
    }
  }

  function toggleStarredOnly() {
    starredOnly = !starredOnly;
    if (
      selectedId !== null &&
      !visibleCaptures.some((c) => c.id === selectedId)
    ) {
      selectedId = null;
    }
  }

  async function refreshStats() {
    try {
      const [total, unread] = await Promise.all([
        invokeFn("total_count", {}) as Promise<number>,
        invokeFn("unread_count", {}) as Promise<number>,
      ]);
      totalCount = total;
      unreadCount = unread;
    } catch (err) {
      console.error("refresh stats failed", err);
    }
  }

  function relativeTime(createdAt: string, ref: number): string {
    const t = Date.parse(createdAt);
    if (Number.isNaN(t)) return "";
    const diff = Math.max(0, ref - t);
    const sec = Math.floor(diff / 1000);
    if (sec < 60) return "just now";
    const min = Math.floor(sec / 60);
    if (min < 60) return `${min}m ago`;
    const hr = Math.floor(min / 60);
    if (hr < 24) return `${hr}h ago`;
    const day = Math.floor(hr / 24);
    return `${day}d ago`;
  }

  const lastCaptureLabel = $derived(
    captures.length === 0 ? null : relativeTime(captures[0].created_at, now),
  );
  const totalLabel = $derived(
    filtersActive
      ? `${visibleCaptures.length} of ${searchOrPageResults.length} showing`
      : searching
        ? `${visibleCaptures.length} ${visibleCaptures.length === 1 ? "result" : "results"}`
        : totalCount === null
          ? null
          : `${totalCount} ${totalCount === 1 ? "capture" : "captures"}`,
  );
  const unreadLabel = $derived(
    unreadCount && unreadCount > 0 ? `${unreadCount} new` : null,
  );

  async function loadNext() {
    if (loading || exhausted || searching) return;
    loading = true;
    try {
      const cursor = captures.length > 0 ? captures[captures.length - 1].id : null;
      const page = await listFn(cursor, PAGE_SIZE);
      if (page.length === 0) {
        exhausted = true;
      } else {
        captures = [...captures, ...page];
        if (page.length < PAGE_SIZE) {
          exhausted = true;
        }
      }
    } catch (err) {
      console.error("list_captures failed", err);
    } finally {
      loading = false;
    }
  }

  async function refetchFirstPage() {
    try {
      const page = await listFn(null, PAGE_SIZE);
      captures = page;
      exhausted = page.length < PAGE_SIZE;
      if (selectedId !== null && !page.some((c) => c.id === selectedId)) {
        selectedId = null;
      }
    } catch (err) {
      console.error("refetch first page failed", err);
    }
  }

  function isMutation(payload: ChangedPayload): payload is MutationNotice {
    return (
      typeof payload === "object" &&
      payload !== null &&
      "kind" in payload &&
      (payload.kind === "starred" || payload.kind === "deleted")
    );
  }

  function onScroll(event: Event) {
    const el = event.currentTarget as HTMLElement;
    if (el.scrollHeight - el.scrollTop - el.clientHeight < SCROLL_THRESHOLD_PX) {
      loadNext();
    }
  }

  function scheduleSearch() {
    if (searchDebounceTimer !== null) {
      clearTimeout(searchDebounceTimer);
    }
    searchDebounceTimer = setTimeout(runSearch, SEARCH_DEBOUNCE_MS);
  }

  async function runSearch() {
    searchDebounceTimer = null;
    const q = searchQuery.trim();
    if (q === "") {
      searchResults = null;
      return;
    }
    try {
      const results = (await invokeFn("search_captures", {
        query: q,
        limit: PAGE_SIZE,
      })) as Capture[];
      searchResults = results;
      if (
        selectedId !== null &&
        !results.some((c) => c.id === selectedId)
      ) {
        selectedId = null;
      }
    } catch (err) {
      console.error("search_captures failed", err);
      searchResults = [];
    }
  }

  function onSearchInput(event: Event) {
    searchQuery = (event.currentTarget as HTMLInputElement).value;
    scheduleSearch();
  }

  function clearSearch() {
    searchQuery = "";
    if (searchDebounceTimer !== null) {
      clearTimeout(searchDebounceTimer);
      searchDebounceTimer = null;
    }
    searchResults = null;
  }

  function onSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      if (searchQuery !== "") {
        event.preventDefault();
        clearSearch();
      }
    }
  }

  function onWindowKeydown(event: KeyboardEvent) {
    // Cmd+F focuses the search input from anywhere in the Inbox
    // window. The native browser find UI is meaningless inside a
    // Tauri webview, so co-opting the shortcut is fine.
    if ((event.metaKey || event.ctrlKey) && event.key === "f") {
      event.preventDefault();
      searchInputEl?.focus();
      searchInputEl?.select();
    }
  }

  function onChanged(payload: ChangedPayload) {
    refreshStats();
    if (isMutation(payload)) {
      // Star / delete: refetch the first page to reconcile the row.
      refetchFirstPage();
      return;
    }
    // New row from `save_note` / clipboard capture: prepend with dedup.
    if (captures.some((existing) => existing.id === payload.id)) return;
    captures = [payload, ...captures];
  }

  function onSelect(id: string) {
    selectedId = id;
    // Per-item read tracking: stamp `read_at` on the row the user
    // just interacted with. The Rust command returns the live unread
    // count so the status bar updates without a follow-up round-trip.
    // Local optimistic update keeps the unread dot from lingering
    // while the IPC is in flight.
    const target = captures.find((c) => c.id === id);
    if (target && target.read_at === null) {
      const stamped = { ...target, read_at: new Date().toISOString() };
      captures = captures.map((c) => (c.id === id ? stamped : c));
    }
    invokeFn("mark_read", { id })
      .then((remaining) => {
        unreadCount = Number(remaining) || 0;
      })
      .catch((err) => {
        console.error("mark_read failed", err);
      });
  }

  async function onStarToggle(id: string, next: boolean) {
    try {
      await invokeFn("star_capture", { id, starred: next });
    } catch (err) {
      console.error("star_capture failed", err);
    }
  }

  async function onDelete(id: string) {
    // Optimistic: remove the row locally before the round-trip. The
    // captures.changed event will refetch the first page as a backstop.
    captures = captures.filter((c) => c.id !== id);
    if (selectedId === id) {
      selectedId = null;
    }
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

  // Dispatch the per-kind Open action. Used both by `Enter` on the
  // list pane and by the detail pane's action button (which calls
  // `onOpenLink` / `onReveal` directly with its own arguments). Mirrors
  // the routing in `commands::reveal_capture_with` — Link uses
  // `open_link` so we do not pay a store round-trip for a URL the JS
  // already has; everything else routes through `reveal_capture`.
  function onOpen(capture: Capture) {
    if (capture.kind === "Link") {
      const url = typeof capture.payload.url === "string" ? capture.payload.url : "";
      if (url) onOpenLink(url);
      return;
    }
    if (capture.kind === "Clip" || capture.kind === "Note") {
      // No reveal target for text-only kinds. The detail pane shows
      // the full text; pressing Enter is a no-op here.
      return;
    }
    onReveal(capture.id);
  }

  const selectedCapture = $derived(
    selectedId === null
      ? null
      : (captures.find((c) => c.id === selectedId) ?? null),
  );

  async function onClose() {
    try {
      await hideFn();
    } catch (err) {
      console.error("hide window failed", err);
    }
  }

  onMount(async () => {
    await loadNext();
    await refreshStats();
    // Re-render "last Xm ago" once a minute so the status bar does not
    // get stale while the Inbox sits open.
    nowTimer = setInterval(() => {
      now = Date.now();
    }, 60_000);
    if (typeof window !== "undefined") {
      window.addEventListener("keydown", onWindowKeydown);
    }
    try {
      unlisten = await listenFn(CAPTURES_CHANGED, onChanged);
    } catch (err) {
      console.error("listen captures:changed failed", err);
    }
  });

  onDestroy(() => {
    unlisten?.();
    if (nowTimer !== null) {
      clearInterval(nowTimer);
      nowTimer = null;
    }
    if (searchDebounceTimer !== null) {
      clearTimeout(searchDebounceTimer);
      searchDebounceTimer = null;
    }
    if (typeof window !== "undefined") {
      window.removeEventListener("keydown", onWindowKeydown);
    }
  });
</script>

<div class="inbox" data-testid="inbox">
  <div class="titlebar" data-tauri-drag-region aria-hidden="true"></div>
  <div class="panes">
    <div class="list-column">
      <div class="searchbar">
        <input
          bind:this={searchInputEl}
          type="search"
          class="search-input"
          placeholder="Search captures"
          aria-label="Search captures"
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
      <div class="filterbar" role="toolbar" aria-label="Inbox filters">
        {#each KIND_OPTIONS as option}
          <button
            type="button"
            class="chip"
            class:active={kindFilter === option.value}
            aria-pressed={kindFilter === option.value}
            onclick={() => setKindFilter(option.value)}
          >
            {option.label}
          </button>
        {/each}
        <span class="filter-sep" aria-hidden="true"></span>
        <button
          type="button"
          class="chip star"
          class:active={starredOnly}
          aria-pressed={starredOnly}
          onclick={toggleStarredOnly}
          title="Show only starred captures"
        >
          {starredOnly ? "★" : "☆"} Starred
        </button>
      </div>
      <section class="list-pane" onscroll={onScroll}>
        {#if searching && visibleCaptures.length === 0}
          <div class="empty">
            <div class="empty-glyph" aria-hidden="true">🔍</div>
            <h2 class="empty-title">No matches</h2>
            <p class="empty-hint">Nothing matched “{searchQuery}”.</p>
          </div>
        {:else if !loading && !searching && captures.length === 0}
          <div class="empty">
            <div class="empty-glyph" aria-hidden="true">📥</div>
            <h2 class="empty-title">No captures yet</h2>
            <p class="empty-hint">
              Press <kbd>⌃</kbd><kbd>⌥</kbd><kbd>⌘</kbd><kbd>Space</kbd> to write a note,
              or <kbd>⌃</kbd><kbd>⌥</kbd><kbd>⌘</kbd><kbd>C</kbd> to capture the clipboard.
            </p>
            <p class="empty-hint">Drag a file onto the Dock to save it here.</p>
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
          />
          {#if loading}
            <div class="spinner" aria-live="polite">Loading…</div>
          {/if}
        {/if}
      </section>
    </div>
    <section class="detail-pane">
      <InboxDetail
        capture={selectedCapture}
        {onOpenLink}
        {onReveal}
        {onStarToggle}
      />
    </section>
  </div>
  <footer class="statusbar" aria-label="Inbox stats">
    {#if totalLabel}
      <span class="stat">{totalLabel}</span>
    {/if}
    {#if lastCaptureLabel}
      <span class="sep" aria-hidden="true">·</span>
      <span class="stat">last {lastCaptureLabel}</span>
    {/if}
    {#if unreadLabel}
      <span class="sep" aria-hidden="true">·</span>
      <span class="stat new">{unreadLabel}</span>
    {/if}
  </footer>
</div>

<style>
  .inbox {
    display: grid;
    grid-template-rows: 28px 1fr 24px;
    height: 100vh;
    width: 100vw;
    font-family: Inter, -apple-system, BlinkMacSystemFont, "Segoe UI",
      sans-serif;
    color: #0f0f0f;
    background-color: #f6f6f6;
  }

  /* Thin draggable strip under macOS traffic-light buttons. The window
     uses titleBarStyle="Overlay" so the OS chrome floats above content;
     this strip reserves vertical room for the buttons and is wired
     to Tauri's native drag via the data-tauri-drag-region attribute
     on the element (more reliable than the CSS -webkit-app-region
     rule, which is a no-op in some Tauri 2 webview configurations). */
  .titlebar {
    background-color: transparent;
  }

  .panes {
    display: grid;
    grid-template-columns: 40% 60%;
    min-height: 0;
  }

  /* The list column is split into a static search bar on top of a
     scrolling list pane. Keeping the search bar OUTSIDE the scrolling
     container means browser-driven scroll (e.g. Tab focus into the
     listbox) cannot tuck rows underneath a sticky overlay — there is
     no overlay anymore. */
  .list-column {
    display: grid;
    grid-template-rows: auto 1fr;
    min-height: 0;
    border-right: 1px solid rgba(0, 0, 0, 0.08);
  }

  .list-pane {
    overflow-y: auto;
    min-height: 0;
    position: relative;
  }

  .searchbar {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid rgba(0, 0, 0, 0.06);
  }

  .filterbar {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.4rem 0.75rem;
    border-bottom: 1px solid rgba(0, 0, 0, 0.06);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .filterbar::-webkit-scrollbar {
    display: none;
  }

  .filterbar .chip {
    appearance: none;
    flex: 0 0 auto;
    border: 1px solid rgba(0, 0, 0, 0.12);
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 0.72rem;
    padding: 0.18rem 0.55rem;
    border-radius: 999px;
    cursor: pointer;
    opacity: 0.7;
    transition:
      background 80ms ease,
      opacity 80ms ease,
      border-color 80ms ease;
    white-space: nowrap;
  }

  .filterbar .chip:hover {
    opacity: 1;
  }

  .filterbar .chip.active {
    background: rgba(76, 29, 149, 0.12);
    border-color: rgba(76, 29, 149, 0.55);
    color: rgba(76, 29, 149, 1);
    opacity: 1;
  }

  .filterbar .chip.star.active {
    background: rgba(245, 158, 11, 0.15);
    border-color: rgba(245, 158, 11, 0.65);
    color: rgba(180, 110, 0, 1);
  }

  .filter-sep {
    flex: 0 0 auto;
    width: 1px;
    height: 1rem;
    background: rgba(0, 0, 0, 0.12);
    margin: 0 0.3rem;
  }

  .search-input {
    flex: 1;
    min-width: 0;
    appearance: none;
    border: 1px solid rgba(0, 0, 0, 0.12);
    border-radius: 6px;
    padding: 0.35rem 0.6rem;
    font: inherit;
    font-size: 0.85rem;
    background: rgba(0, 0, 0, 0.02);
    color: inherit;
    outline: none;
  }

  .search-input:focus {
    border-color: rgba(76, 29, 149, 0.55);
    background: rgba(76, 29, 149, 0.04);
  }

  .search-input::-webkit-search-cancel-button {
    display: none;
  }

  .search-clear {
    appearance: none;
    border: none;
    background: transparent;
    color: inherit;
    opacity: 0.55;
    cursor: pointer;
    padding: 0.2rem 0.4rem;
    font-size: 1.1rem;
    line-height: 1;
    border-radius: 4px;
  }

  .search-clear:hover {
    opacity: 1;
    background: rgba(0, 0, 0, 0.06);
  }

  .detail-pane {
    overflow: hidden;
  }

  .statusbar {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    padding: 0 0.85rem;
    border-top: 1px solid rgba(0, 0, 0, 0.08);
    font-size: 0.72rem;
    color: rgba(0, 0, 0, 0.55);
    user-select: none;
  }

  .statusbar .sep {
    opacity: 0.5;
  }

  .statusbar .stat.new {
    color: rgba(79, 70, 229, 0.85);
    font-weight: 500;
  }

  .spinner {
    padding: 0.5rem 0.75rem;
    font-size: 0.85rem;
    opacity: 0.6;
  }

  .empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 2rem 1.5rem;
    text-align: center;
    gap: 0.6rem;
  }

  .empty-glyph {
    font-size: 3rem;
    opacity: 0.6;
  }

  .empty-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
  }

  .empty-hint {
    margin: 0;
    font-size: 0.82rem;
    opacity: 0.7;
    line-height: 1.5;
    max-width: 28ch;
  }

  .empty-hint kbd {
    display: inline-block;
    padding: 0.05em 0.4em;
    margin: 0 1px;
    font-family: inherit;
    font-size: 0.78em;
    background: rgba(0, 0, 0, 0.06);
    border: 1px solid rgba(0, 0, 0, 0.1);
    border-radius: 3px;
  }

  @media (prefers-color-scheme: dark) {
    .inbox {
      color: #f6f6f6;
      background-color: #1c1c1c;
    }
    .list-column {
      border-right-color: rgba(255, 255, 255, 0.08);
    }
    .searchbar {
      border-bottom-color: rgba(255, 255, 255, 0.06);
    }
    .filterbar {
      border-bottom-color: rgba(255, 255, 255, 0.06);
    }
    .filterbar .chip {
      border-color: rgba(255, 255, 255, 0.18);
    }
    .filterbar .chip.active {
      background: rgba(167, 139, 250, 0.18);
      border-color: rgba(167, 139, 250, 0.6);
      color: rgba(167, 139, 250, 1);
    }
    .filterbar .chip.star.active {
      background: rgba(245, 158, 11, 0.2);
      border-color: rgba(245, 158, 11, 0.6);
      color: rgba(252, 211, 77, 1);
    }
    .filter-sep {
      background: rgba(255, 255, 255, 0.12);
    }
    .search-input {
      border-color: rgba(255, 255, 255, 0.12);
      background: rgba(255, 255, 255, 0.04);
    }
    .search-input:focus {
      border-color: rgba(167, 139, 250, 0.6);
      background: rgba(167, 139, 250, 0.08);
    }
    .search-clear:hover {
      background: rgba(255, 255, 255, 0.1);
    }
    .empty-hint kbd {
      background: rgba(255, 255, 255, 0.08);
      border-color: rgba(255, 255, 255, 0.12);
    }
    .statusbar {
      border-top-color: rgba(255, 255, 255, 0.08);
      color: rgba(255, 255, 255, 0.55);
    }
    .statusbar .stat.new {
      color: rgba(165, 180, 252, 0.95);
    }
  }
</style>
