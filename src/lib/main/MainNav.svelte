<script lang="ts">
  // Segmented control at the top of the main window that switches
  // between the Inbox and Archive views. Both views share the same
  // Tauri window (per ADR-0009); navigation goes through SvelteKit's
  // client router so the WebView state survives the switch.
  //
  // Carries an inbox-count badge that listens to `captures:changed`
  // so it stays accurate as Captures are created, routed, deleted.

  import { onMount, onDestroy } from "svelte";
  import { goto } from "$app/navigation";
  import { invoke as tauriInvoke } from "@tauri-apps/api/core";
  import { listen as tauriListen } from "@tauri-apps/api/event";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { CAPTURES_CHANGED } from "$lib/events";

  type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
  type ListenFn = (event: string, handler: () => void) => Promise<UnlistenFn>;

  interface Props {
    /** Which view is currently active. Drives the highlight + the
     * aria-current attribute on the active tab. */
    active: "inbox" | "archive";
    /** Optional override so callers can navigate via their own router
     * (used by tests + the archive page's "back to inbox" UX). */
    onNavigate?: (target: "inbox" | "archive") => void;
    invokeFn?: InvokeFn;
    listenFn?: ListenFn;
  }

  const defaultInvoke: InvokeFn = (cmd, args) => tauriInvoke(cmd, args);
  const defaultListen: ListenFn = (event, handler) =>
    tauriListen(event, () => handler());

  const {
    active,
    onNavigate,
    invokeFn = defaultInvoke,
    listenFn = defaultListen,
  }: Props = $props();

  let inboxCount = $state<number | null>(null);
  let unlisten: UnlistenFn | null = null;

  onMount(async () => {
    await refresh();
    try {
      unlisten = await listenFn(CAPTURES_CHANGED, () => {
        void refresh();
      });
    } catch (err) {
      // Outside Tauri (e.g. vitest / jsdom) `listen` throws because
      // the runtime hooks are not attached. The component still
      // renders without the live badge refresh, which is fine for
      // tests.
      console.error("MainNav listen failed", err);
    }
  });

  onDestroy(() => {
    if (unlisten) unlisten();
  });

  async function refresh() {
    try {
      const n = (await invokeFn("inbox_count")) as number;
      inboxCount = Number(n) || 0;
    } catch (err) {
      console.error("inbox_count failed", err);
    }
  }

  function navigate(target: "inbox" | "archive") {
    if (target === active) return;
    if (onNavigate) {
      onNavigate(target);
      return;
    }
    void goto(`/${target}`);
  }

  function onKeydown(e: KeyboardEvent) {
    // Cmd+1 / Cmd+2 keyboard shortcuts for the two tabs.
    if (!(e.metaKey || e.ctrlKey)) return;
    if (e.key === "1") {
      e.preventDefault();
      navigate("inbox");
    } else if (e.key === "2") {
      e.preventDefault();
      navigate("archive");
    }
  }

  $effect(() => {
    if (typeof window === "undefined") return;
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  });
</script>

<nav class="main-nav" aria-label="Main views" data-testid="main-nav">
  <button
    type="button"
    class="tab"
    class:active={active === "inbox"}
    aria-current={active === "inbox" ? "page" : undefined}
    onclick={() => navigate("inbox")}
    data-testid="nav-inbox"
  >
    Inbox
    {#if inboxCount !== null && inboxCount > 0}
      <span class="badge" data-testid="inbox-badge">{inboxCount}</span>
    {/if}
  </button>
  <button
    type="button"
    class="tab"
    class:active={active === "archive"}
    aria-current={active === "archive" ? "page" : undefined}
    onclick={() => navigate("archive")}
    data-testid="nav-archive"
  >
    Archive
  </button>
</nav>

<style>
  .main-nav {
    display: inline-flex;
    align-items: center;
    gap: 0.15rem;
    padding: 0.18rem;
    border-radius: 8px;
    background: rgba(0, 0, 0, 0.05);
  }
  @media (prefers-color-scheme: dark) {
    .main-nav {
      background: rgba(255, 255, 255, 0.08);
    }
  }

  .tab {
    appearance: none;
    font: inherit;
    font-size: 0.78rem;
    background: transparent;
    border: none;
    color: rgba(0, 0, 0, 0.6);
    padding: 0.25rem 0.8rem;
    border-radius: 6px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    transition:
      background 80ms ease,
      color 80ms ease;
  }
  .tab:hover {
    color: rgba(0, 0, 0, 0.9);
  }
  .tab.active {
    background: rgba(255, 255, 255, 0.95);
    color: rgba(0, 0, 0, 0.95);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }
  @media (prefers-color-scheme: dark) {
    .tab {
      color: rgba(255, 255, 255, 0.65);
    }
    .tab:hover {
      color: rgba(255, 255, 255, 0.95);
    }
    .tab.active {
      background: rgba(40, 40, 45, 0.95);
      color: rgba(255, 255, 255, 0.95);
      box-shadow: 0 1px 2px rgba(0, 0, 0, 0.4);
    }
  }

  .badge {
    background: rgba(76, 29, 149, 0.18);
    color: rgba(76, 29, 149, 1);
    border-radius: 999px;
    font-size: 0.7rem;
    padding: 0.05rem 0.45rem;
    line-height: 1.1;
    font-weight: 500;
  }
  @media (prefers-color-scheme: dark) {
    .badge {
      background: rgba(167, 139, 250, 0.25);
      color: rgba(196, 181, 253, 1);
    }
  }
</style>
