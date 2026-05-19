<script lang="ts">
  // Inline command-palette for routing a Capture to a Destination.
  // Opens from the Inbox (or Archive) on `R` press; the parent owns
  // open/closed state and supplies the target capture id. The picker
  // handles destination loading, fuzzy filter, keyboard navigation,
  // inline create-on-the-fly, and the actual `route_capture` invoke.
  //
  // Keyboard:
  //   ↑/↓     move highlight
  //   Enter   assign highlighted destination + close
  //   Esc     cancel + close
  //   ⌘N      switch to inline create form (auto-opens when no live
  //           destinations exist on first show)
  //
  // See ADR-0010 for the broader triage UX.

  import { tick } from "svelte";
  import { invoke as tauriInvoke } from "@tauri-apps/api/core";
  import type { Destination } from "$lib/captures/types";
  import { formatError } from "$lib/utils/format-error";
  import type { PaletteKey } from "./palette";
  import DestinationDot from "./DestinationDot.svelte";
  import PaletteSwatches from "./PaletteSwatches.svelte";

  type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

  interface Props {
    open: boolean;
    captureId: string | null;
    /** When set, the picker pre-selects this destination on open so the
     * user can ESC out of an accidental press. Drives the re-route UX
     * from the Archive. */
    currentDestinationId?: string | null;
    invokeFn?: InvokeFn;
    onClose: () => void;
    onAssigned: (destinationId: string) => void;
  }

  const defaultInvoke: InvokeFn = (cmd, args) => tauriInvoke(cmd, args);

  const {
    open,
    captureId,
    currentDestinationId = null,
    invokeFn = defaultInvoke,
    onClose,
    onAssigned,
  }: Props = $props();

  let destinations = $state<Destination[]>([]);
  let query = $state("");
  let highlightIdx = $state(0);
  let mode = $state<"list" | "create">("list");
  let createDraft = $state<{ name: string; color: PaletteKey | null }>({
    name: "",
    color: null,
  });
  let errorMessage = $state<string | null>(null);
  let inputEl: HTMLInputElement | undefined = $state();
  let createInputEl: HTMLInputElement | undefined = $state();
  let loaded = $state(false);

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const base = destinations;
    if (!q) return base;
    return base.filter((d) => d.name.toLowerCase().includes(q));
  });

  // Reset / reload whenever the picker is opened.
  $effect(() => {
    if (open) {
      void onOpen();
    } else {
      // Clear transient state when closed so reopens start clean.
      query = "";
      highlightIdx = 0;
      mode = "list";
      createDraft = { name: "", color: null };
      errorMessage = null;
      loaded = false;
    }
  });

  // Keep highlight in range as filtered shrinks.
  $effect(() => {
    if (filtered.length === 0) {
      highlightIdx = 0;
    } else if (highlightIdx >= filtered.length) {
      highlightIdx = filtered.length - 1;
    }
  });

  async function onOpen() {
    try {
      const rows = (await invokeFn("list_destinations")) as Destination[];
      destinations = rows;
      loaded = true;
      // Pre-select the current destination when given (re-route flow).
      if (currentDestinationId) {
        const idx = rows.findIndex((d) => d.id === currentDestinationId);
        highlightIdx = idx >= 0 ? idx : 0;
      } else {
        highlightIdx = 0;
      }
      // Zero live destinations + open press = jump straight to create.
      if (rows.length === 0) {
        mode = "create";
        await tick();
        createInputEl?.focus();
      } else {
        await tick();
        inputEl?.focus();
      }
    } catch (err) {
      errorMessage = formatError(err);
    }
  }

  function handleListKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (filtered.length === 0) return;
      highlightIdx = Math.min(highlightIdx + 1, filtered.length - 1);
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      if (filtered.length === 0) return;
      highlightIdx = Math.max(highlightIdx - 1, 0);
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      const target = filtered[highlightIdx];
      if (target) void assign(target.id);
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "n") {
      e.preventDefault();
      enterCreateMode();
    }
  }

  function handleCreateKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      // ESC from create: bounce back to the list when one exists,
      // otherwise close the picker entirely.
      if (destinations.length > 0) {
        mode = "list";
        errorMessage = null;
        void tick().then(() => inputEl?.focus());
      } else {
        onClose();
      }
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      void submitCreate();
    }
  }

  async function enterCreateMode() {
    mode = "create";
    createDraft = { name: query.trim(), color: null };
    errorMessage = null;
    await tick();
    createInputEl?.focus();
  }

  async function submitCreate() {
    const name = createDraft.name.trim();
    if (!name) {
      errorMessage = "Name required.";
      return;
    }
    try {
      const created = (await invokeFn("create_destination", {
        name,
        color: createDraft.color,
      })) as Destination;
      // Route the capture to the freshly-created destination in one
      // motion — the user picked R because they wanted to file this row.
      await assign(created.id);
    } catch (err) {
      errorMessage = formatError(err);
    }
  }

  async function assign(destinationId: string) {
    if (!captureId) {
      onClose();
      return;
    }
    const target = destinations.find((d) => d.id === destinationId);
    // KokoBrain destinations fire the deep-link side-effect via the
    // dedicated command; everything else goes through the plain
    // route_capture path. See ADR-0012.
    const command =
      target?.kind === "kokobrain" ? "route_to_kokobrain" : "route_capture";
    try {
      await invokeFn(command, {
        id: captureId,
        destinationId,
      });
      onAssigned(destinationId);
      onClose();
    } catch (err) {
      errorMessage = formatError(err);
    }
  }

</script>

{#if open}
  <button
    type="button"
    class="backdrop"
    aria-label="Close picker"
    onclick={onClose}
    data-testid="picker-backdrop"
  ></button>
  <div
    class="picker"
    role="dialog"
    aria-label="Route to destination"
    data-testid="destination-picker"
  >
    {#if mode === "list"}
      <input
        bind:this={inputEl}
        type="text"
        class="search"
        placeholder="Route to…"
        bind:value={query}
        onkeydown={handleListKeydown}
        data-testid="picker-search"
      />
      <ul class="results" role="listbox" aria-label="Destinations">
        {#if !loaded}
          <li class="hint">Loading…</li>
        {:else if filtered.length === 0}
          <li class="hint">
            {query.trim()
              ? `No match. ⌘N to create "${query.trim()}".`
              : "No destinations yet. ⌘N to create one."}
          </li>
        {:else}
          {#each filtered as dest, idx (dest.id)}
            <li
              class="result"
              class:active={idx === highlightIdx}
              role="option"
              aria-selected={idx === highlightIdx}
              data-testid="picker-result"
              onclick={() => assign(dest.id)}
              onkeydown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  void assign(dest.id);
                }
              }}
              onmouseenter={() => (highlightIdx = idx)}
            >
              <DestinationDot color={dest.color} />
              <span class="name">{dest.name}</span>
            </li>
          {/each}
        {/if}
      </ul>
      <footer class="footer">
        <span><kbd>↑</kbd><kbd>↓</kbd> nav</span>
        <span><kbd>↵</kbd> assign</span>
        <span><kbd>⌘N</kbd> new</span>
        <span><kbd>esc</kbd> cancel</span>
      </footer>
    {:else}
      <div class="create-pane" data-testid="picker-create">
        <input
          bind:this={createInputEl}
          type="text"
          class="search"
          placeholder="New destination name"
          bind:value={createDraft.name}
          onkeydown={handleCreateKeydown}
          data-testid="picker-create-input"
        />
        <div class="swatches-row">
          <PaletteSwatches
            selected={createDraft.color}
            onSelect={(c) => (createDraft.color = c)}
          />
        </div>
        <footer class="footer">
          <button
            type="button"
            class="primary"
            onclick={submitCreate}
            data-testid="picker-create-submit"
          >
            Create + route
          </button>
          <button type="button" class="ghost" onclick={onClose}>Cancel</button>
        </footer>
      </div>
    {/if}
    {#if errorMessage}
      <p class="error" role="alert" data-testid="picker-error">{errorMessage}</p>
    {/if}
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.25);
    z-index: 50;
    border: 0;
    padding: 0;
    cursor: default;
  }
  @media (prefers-color-scheme: dark) {
    .backdrop {
      background: rgba(0, 0, 0, 0.45);
    }
  }

  .picker {
    position: fixed;
    top: 20vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(420px, 92vw);
    background: #ffffff;
    border: 1px solid rgba(0, 0, 0, 0.12);
    border-radius: 10px;
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.18);
    z-index: 51;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  @media (prefers-color-scheme: dark) {
    .picker {
      background: #232327;
      border-color: rgba(255, 255, 255, 0.12);
      box-shadow: 0 18px 48px rgba(0, 0, 0, 0.5);
    }
  }

  .search {
    appearance: none;
    border: none;
    border-bottom: 1px solid rgba(0, 0, 0, 0.08);
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 0.95rem;
    padding: 0.7rem 0.9rem;
  }
  .search:focus {
    outline: none;
  }
  @media (prefers-color-scheme: dark) {
    .search {
      border-bottom-color: rgba(255, 255, 255, 0.08);
    }
  }

  .results {
    list-style: none;
    margin: 0;
    padding: 0.3rem 0;
    max-height: 50vh;
    overflow-y: auto;
  }
  .result {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.45rem 0.9rem;
    cursor: pointer;
  }
  .result.active {
    background: rgba(76, 29, 149, 0.12);
  }
  @media (prefers-color-scheme: dark) {
    .result.active {
      background: rgba(167, 139, 250, 0.18);
    }
  }
  .name {
    font-size: 0.9rem;
  }
  .hint {
    padding: 0.6rem 0.9rem;
    color: rgba(0, 0, 0, 0.55);
    font-size: 0.85rem;
  }
  @media (prefers-color-scheme: dark) {
    .hint {
      color: rgba(255, 255, 255, 0.55);
    }
  }

  .footer {
    display: flex;
    gap: 0.55rem;
    padding: 0.45rem 0.9rem;
    border-top: 1px solid rgba(0, 0, 0, 0.08);
    background: rgba(0, 0, 0, 0.02);
    font-size: 0.72rem;
    color: rgba(0, 0, 0, 0.55);
    align-items: center;
  }
  @media (prefers-color-scheme: dark) {
    .footer {
      border-top-color: rgba(255, 255, 255, 0.08);
      background: rgba(255, 255, 255, 0.03);
      color: rgba(255, 255, 255, 0.55);
    }
  }
  .footer kbd {
    display: inline-block;
    min-width: 1.3em;
    text-align: center;
    padding: 0.05em 0.3em;
    font: inherit;
    font-size: 0.7rem;
    background: rgba(0, 0, 0, 0.06);
    border: 1px solid rgba(0, 0, 0, 0.1);
    border-bottom-width: 2px;
    border-radius: 4px;
    margin-right: 0.15rem;
    color: inherit;
  }
  @media (prefers-color-scheme: dark) {
    .footer kbd {
      background: rgba(255, 255, 255, 0.08);
      border-color: rgba(255, 255, 255, 0.12);
    }
  }

  .create-pane {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    padding: 0 0 0.55rem;
  }
  .swatches-row {
    padding: 0 0.9rem;
  }

  .primary,
  .ghost {
    appearance: none;
    font: inherit;
    font-size: 0.78rem;
    border-radius: 6px;
    padding: 0.3rem 0.7rem;
    cursor: pointer;
    transition:
      background 80ms ease,
      border-color 80ms ease;
  }
  .primary {
    border: 1px solid rgba(76, 29, 149, 0.5);
    background: rgba(76, 29, 149, 0.18);
    color: rgba(76, 29, 149, 1);
  }
  .primary:hover {
    background: rgba(76, 29, 149, 0.3);
  }
  @media (prefers-color-scheme: dark) {
    .primary {
      border-color: rgba(167, 139, 250, 0.5);
      background: rgba(167, 139, 250, 0.22);
      color: rgba(167, 139, 250, 1);
    }
  }
  .ghost {
    border: 1px solid transparent;
    background: transparent;
    color: rgba(0, 0, 0, 0.6);
  }
  .ghost:hover {
    background: rgba(0, 0, 0, 0.06);
  }
  @media (prefers-color-scheme: dark) {
    .ghost {
      color: rgba(255, 255, 255, 0.7);
    }
    .ghost:hover {
      background: rgba(255, 255, 255, 0.08);
    }
  }

  .error {
    margin: 0;
    padding: 0.4rem 0.9rem;
    background: rgba(220, 38, 38, 0.08);
    color: rgba(155, 28, 28, 1);
    font-size: 0.8rem;
    border-top: 1px solid rgba(220, 38, 38, 0.25);
  }
  @media (prefers-color-scheme: dark) {
    .error {
      background: rgba(248, 113, 113, 0.12);
      color: rgba(252, 165, 165, 1);
      border-top-color: rgba(248, 113, 113, 0.3);
    }
  }
</style>
