<script lang="ts">
  // Inbox window root. Owns list state and the live-update subscription;
  // delegates row rendering to InboxList. The Tauri adapters (`invoke`,
  // `listen`) are injected as props so the page can be mounted in a
  // test without a Tauri runtime.
  import { onMount, onDestroy } from "svelte";
  import { invoke as tauriInvoke } from "@tauri-apps/api/core";
  import { listen as tauriListen } from "@tauri-apps/api/event";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import type { Capture } from "$lib/captures/types";
  import InboxList from "$lib/inbox/InboxList.svelte";

  const PAGE_SIZE = 50;
  const SCROLL_THRESHOLD_PX = 100;

  type ListFn = (
    cursor: string | null,
    limit: number,
  ) => Promise<Capture[]>;
  type ListenFn = (
    event: string,
    handler: (payload: Capture) => void,
  ) => Promise<UnlistenFn>;

  interface Props {
    listFn?: ListFn;
    listenFn?: ListenFn;
  }

  const defaultList: ListFn = (cursor, limit) =>
    tauriInvoke<Capture[]>("list_captures", { cursor, limit });

  const defaultListen: ListenFn = (event, handler) =>
    tauriListen<Capture>(event, (e) => handler(e.payload));

  const { listFn = defaultList, listenFn = defaultListen }: Props = $props();

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

  function onScroll(event: Event) {
    const el = event.currentTarget as HTMLElement;
    if (el.scrollHeight - el.scrollTop - el.clientHeight < SCROLL_THRESHOLD_PX) {
      loadNext();
    }
  }

  function prepend(c: Capture) {
    if (captures.some((existing) => existing.id === c.id)) return;
    captures = [c, ...captures];
  }

  function onSelect(id: string) {
    selectedId = id;
  }

  function onStarToggle(_id: string, _next: boolean) {
    // Slice 03 wires this to `star_capture`.
  }

  function onDelete(_id: string) {
    // Slice 03 wires this to `delete_capture`.
  }

  onMount(async () => {
    await loadNext();
    try {
      unlisten = await listenFn("captures.changed", prepend);
    } catch (err) {
      console.error("listen captures.changed failed", err);
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
    />
    {#if loading}
      <div class="spinner" aria-live="polite">Loading…</div>
    {/if}
  </section>
  <section class="detail-pane">
    <p class="placeholder">Select a Capture</p>
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
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .placeholder {
    opacity: 0.5;
    margin: 0;
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
