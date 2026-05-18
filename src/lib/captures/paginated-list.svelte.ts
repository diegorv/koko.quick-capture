// Shared cursor-paginated list state. Used by both /inbox and
// /archive page-level components, which differ only in their
// `pageFn` + `cursorOf` (Inbox keys on the ULID id; Archive keys on
// an opaque `routed_at|id` tuple — see Rust `cursor_for_archive`).
//
// Svelte 5 idiom: a state-bearing factory in a `.svelte.ts` file
// returns runes-backed state via getters/setters on a plain object,
// plus async actions. Consuming components keep a single reference
// and read `pager.items`, call `pager.loadNext()`, etc.

import type { Capture } from "./types";

export type PageFn = (
  cursor: string | null,
  limit: number,
) => Promise<Capture[]>;

export type CursorOf = (last: Capture) => string | null;

export interface PaginatedListOptions {
  pageFn: PageFn;
  /** Compute the cursor string for the last item of a page. Returning
   * `null` short-circuits subsequent loads. */
  cursorOf: CursorOf;
  pageSize: number;
  /** Distance from the bottom of the scroll container at which a new
   * page is requested. Defaults to 100px. */
  scrollThresholdPx?: number;
}

export interface PaginatedList {
  readonly items: Capture[];
  readonly loading: boolean;
  readonly exhausted: boolean;
  setItems(items: Capture[], opts?: { exhausted?: boolean }): void;
  prepend(item: Capture): void;
  remove(id: string): void;
  loadNext(): Promise<void>;
  /** Reset cursor + reload the first page. Used after a filter
   * change or when an external mutation invalidates the cache. */
  refetchFirst(): Promise<void>;
  /** Hook the container's `onscroll`. Triggers `loadNext` when the
   * user reaches the threshold. */
  onScroll(event: Event): void;
}

export function createPaginatedList(opts: PaginatedListOptions): PaginatedList {
  const { pageFn, cursorOf, pageSize, scrollThresholdPx = 100 } = opts;

  let items = $state<Capture[]>([]);
  let loading = $state(false);
  let exhausted = $state(false);

  async function loadNext() {
    if (loading || exhausted) return;
    loading = true;
    try {
      const cursor =
        items.length > 0 ? cursorOf(items[items.length - 1]) : null;
      const page = await pageFn(cursor, pageSize);
      if (page.length === 0) {
        exhausted = true;
      } else {
        items = [...items, ...page];
        if (page.length < pageSize) exhausted = true;
      }
    } catch (err) {
      console.error("paginated-list loadNext failed", err);
    } finally {
      loading = false;
    }
  }

  async function refetchFirst() {
    try {
      const page = await pageFn(null, pageSize);
      items = page;
      exhausted = page.length < pageSize;
    } catch (err) {
      console.error("paginated-list refetchFirst failed", err);
    }
  }

  return {
    get items() {
      return items;
    },
    get loading() {
      return loading;
    },
    get exhausted() {
      return exhausted;
    },
    setItems(next, options) {
      items = next;
      if (options?.exhausted !== undefined) exhausted = options.exhausted;
    },
    prepend(item) {
      // De-dup by id so a captures:changed event for an item already
      // on screen doesn't create a duplicate row.
      if (items.some((c) => c.id === item.id)) return;
      items = [item, ...items];
    },
    remove(id) {
      items = items.filter((c) => c.id !== id);
    },
    loadNext,
    refetchFirst,
    onScroll(event) {
      const el = event.currentTarget as HTMLElement;
      if (
        el.scrollHeight - el.scrollTop - el.clientHeight <
        scrollThresholdPx
      ) {
        void loadNext();
      }
    },
  };
}
