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
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { Capture } from "$lib/captures/types";
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

  const defaultHide: HideFn = () => getCurrentWindow().hide();

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

  async function loadNext() {
    if (loading || exhausted) return;
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

  function onChanged(payload: ChangedPayload) {
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
    try {
      unlisten = await listenFn("captures:changed", onChanged);
    } catch (err) {
      console.error("listen captures:changed failed", err);
    }
  });

  onDestroy(() => {
    unlisten?.();
  });
</script>

<div class="inbox" data-testid="inbox">
  <section class="list-pane" onscroll={onScroll}>
    <InboxList
      {captures}
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
  </section>
  <section class="detail-pane">
    <InboxDetail capture={selectedCapture} {onOpenLink} {onReveal} />
  </section>
</div>

<style>
  .inbox {
    display: grid;
    grid-template-columns: 40% 60%;
    height: 100vh;
    width: 100vw;
    font-family: Inter, -apple-system, BlinkMacSystemFont, "Segoe UI",
      sans-serif;
    color: #0f0f0f;
    background-color: #f6f6f6;
  }

  .list-pane {
    overflow-y: auto;
    border-right: 1px solid rgba(0, 0, 0, 0.08);
    position: relative;
  }

  .detail-pane {
    overflow: hidden;
  }

  .spinner {
    padding: 0.5rem 0.75rem;
    font-size: 0.85rem;
    opacity: 0.6;
  }

  @media (prefers-color-scheme: dark) {
    .inbox {
      color: #f6f6f6;
      background-color: #1c1c1c;
    }
    .list-pane {
      border-right-color: rgba(255, 255, 255, 0.08);
    }
  }
</style>
