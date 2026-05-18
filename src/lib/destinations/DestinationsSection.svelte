<script lang="ts">
  // Settings panel for managing Destinations. Lists live destinations,
  // exposes inline create / rename / recolor / soft-delete and a
  // collapsible Soft-deleted section with restore. See ADR-0010.
  //
  // Tauri adapters (`invoke`, `listen`) are injected as props so the
  // component can be mounted in tests without a Tauri runtime, mirroring
  // the pattern used by the Inbox page.

  import { onMount, onDestroy } from "svelte";
  import { invoke as tauriInvoke } from "@tauri-apps/api/core";
  import { listen as tauriListen } from "@tauri-apps/api/event";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import type { Destination } from "$lib/captures/types";
  import { DESTINATIONS_CHANGED } from "$lib/events";
  import { PALETTE_KEYS, colorHex, type PaletteKey } from "./palette";

  type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
  type ListenFn = (event: string, handler: () => void) => Promise<UnlistenFn>;

  interface Props {
    invokeFn?: InvokeFn;
    listenFn?: ListenFn;
  }

  const defaultInvoke: InvokeFn = (cmd, args) => tauriInvoke(cmd, args);
  const defaultListen: ListenFn = (event, handler) =>
    tauriListen(event, () => handler());

  const { invokeFn = defaultInvoke, listenFn = defaultListen }: Props = $props();

  let live = $state<Destination[]>([]);
  let deleted = $state<Destination[]>([]);
  let showDeleted = $state(false);

  // Draft state for the inline "+ New" form. `null` = form hidden.
  type Draft = { name: string; color: PaletteKey | null };
  let createDraft = $state<Draft | null>(null);

  // Map of destination id -> in-progress edit draft. Multiple rows could
  // in theory be edited at once; the UI keeps it to one at a time but
  // the data shape stays generic so callers can edit-and-recover.
  let editDrafts = $state<Record<string, Draft>>({});

  // Id of the destination the user clicked Delete on. Inline confirm
  // bar shows under that row.
  let pendingDeleteId = $state<string | null>(null);

  // Last error message from a write. Cleared on the next successful
  // mutation or on input change in the offending form.
  let errorMessage = $state<string | null>(null);

  let unlisten: UnlistenFn | null = null;

  onMount(async () => {
    await refresh();
    unlisten = await listenFn(DESTINATIONS_CHANGED, () => {
      void refresh();
    });
  });

  onDestroy(() => {
    if (unlisten) unlisten();
  });

  async function refresh() {
    try {
      const [liveRows, deletedRows] = await Promise.all([
        invokeFn("list_destinations") as Promise<Destination[]>,
        invokeFn("list_deleted_destinations") as Promise<Destination[]>,
      ]);
      live = liveRows;
      deleted = deletedRows;
    } catch (err) {
      console.error("destinations refresh failed", err);
    }
  }

  function startCreate() {
    createDraft = { name: "", color: null };
    errorMessage = null;
  }

  function cancelCreate() {
    createDraft = null;
    errorMessage = null;
  }

  async function submitCreate() {
    if (!createDraft) return;
    const name = createDraft.name.trim();
    if (!name) {
      errorMessage = "Name required.";
      return;
    }
    try {
      await invokeFn("create_destination", {
        name,
        color: createDraft.color,
      });
      createDraft = null;
      errorMessage = null;
      await refresh();
    } catch (err) {
      errorMessage = formatError(err);
    }
  }

  function startEdit(dest: Destination) {
    editDrafts = {
      ...editDrafts,
      [dest.id]: {
        name: dest.name,
        color: (dest.color as PaletteKey | null) ?? null,
      },
    };
    errorMessage = null;
  }

  function cancelEdit(id: string) {
    const { [id]: _drop, ...rest } = editDrafts;
    editDrafts = rest;
    errorMessage = null;
  }

  async function submitEdit(id: string) {
    const draft = editDrafts[id];
    if (!draft) return;
    const name = draft.name.trim();
    if (!name) {
      errorMessage = "Name required.";
      return;
    }
    try {
      await invokeFn("update_destination", {
        id,
        name,
        color: draft.color,
      });
      cancelEdit(id);
      await refresh();
    } catch (err) {
      errorMessage = formatError(err);
    }
  }

  function askDelete(id: string) {
    pendingDeleteId = id;
    errorMessage = null;
  }

  function cancelDelete() {
    pendingDeleteId = null;
  }

  async function confirmDelete(id: string) {
    try {
      await invokeFn("soft_delete_destination", { id });
      pendingDeleteId = null;
      await refresh();
    } catch (err) {
      errorMessage = formatError(err);
    }
  }

  async function restore(id: string) {
    try {
      await invokeFn("restore_destination", { id });
      await refresh();
    } catch (err) {
      errorMessage = formatError(err);
    }
  }

  function formatError(err: unknown): string {
    if (err instanceof Error) return err.message;
    if (typeof err === "string") return err;
    return String(err);
  }
</script>

<section class="section" data-testid="destinations-section">
  <header class="head">
    <h2>Destinations</h2>
    {#if createDraft === null}
      <button
        type="button"
        class="new-btn"
        onclick={startCreate}
        data-testid="new-destination-btn"
      >
        + New destination
      </button>
    {/if}
  </header>

  {#if createDraft !== null}
    <div class="form" data-testid="create-form">
      <input
        type="text"
        class="name-input"
        placeholder="Destination name"
        bind:value={createDraft.name}
        onkeydown={(e) => {
          if (e.key === "Enter") submitCreate();
          if (e.key === "Escape") cancelCreate();
        }}
        data-testid="create-name-input"
      />
      <div class="swatches">
        <button
          type="button"
          class="swatch swatch-none"
          class:selected={createDraft.color === null}
          aria-label="No color"
          onclick={() => createDraft && (createDraft.color = null)}
        ></button>
        {#each PALETTE_KEYS as key}
          <button
            type="button"
            class="swatch"
            class:selected={createDraft.color === key}
            style="background-color: {colorHex(key)};"
            aria-label={key}
            onclick={() => createDraft && (createDraft.color = key)}
          ></button>
        {/each}
      </div>
      <div class="form-actions">
        <button type="button" class="primary" onclick={submitCreate}>Save</button>
        <button type="button" class="ghost" onclick={cancelCreate}>Cancel</button>
      </div>
    </div>
  {/if}

  {#if errorMessage}
    <p class="error" role="alert" data-testid="destinations-error">{errorMessage}</p>
  {/if}

  {#if live.length === 0 && createDraft === null}
    <p class="empty">No destinations yet. Add one to start routing captures.</p>
  {/if}

  <ul class="rows">
    {#each live as dest (dest.id)}
      <li class="row" data-testid="destination-row" data-id={dest.id}>
        {#if editDrafts[dest.id]}
          <div class="form inline">
            <input
              type="text"
              class="name-input"
              bind:value={editDrafts[dest.id].name}
              onkeydown={(e) => {
                if (e.key === "Enter") submitEdit(dest.id);
                if (e.key === "Escape") cancelEdit(dest.id);
              }}
            />
            <div class="swatches">
              <button
                type="button"
                class="swatch swatch-none"
                class:selected={editDrafts[dest.id].color === null}
                aria-label="No color"
                onclick={() => (editDrafts[dest.id].color = null)}
              ></button>
              {#each PALETTE_KEYS as key}
                <button
                  type="button"
                  class="swatch"
                  class:selected={editDrafts[dest.id].color === key}
                  style="background-color: {colorHex(key)};"
                  aria-label={key}
                  onclick={() => (editDrafts[dest.id].color = key)}
                ></button>
              {/each}
            </div>
            <div class="form-actions">
              <button
                type="button"
                class="primary"
                onclick={() => submitEdit(dest.id)}>Save</button
              >
              <button
                type="button"
                class="ghost"
                onclick={() => cancelEdit(dest.id)}>Cancel</button
              >
            </div>
          </div>
        {:else}
          <span class="row-main">
            {#if dest.color}
              <span
                class="dot"
                style="background-color: {colorHex(dest.color)};"
                aria-hidden="true"
              ></span>
            {:else}
              <span class="dot dot-empty" aria-hidden="true"></span>
            {/if}
            <span class="name">{dest.name}</span>
          </span>
          <span class="row-actions">
            <button
              type="button"
              class="ghost"
              onclick={() => startEdit(dest)}
              data-testid="edit-btn"
            >
              Edit
            </button>
            <button
              type="button"
              class="ghost danger"
              onclick={() => askDelete(dest.id)}
              data-testid="delete-btn"
            >
              Delete
            </button>
          </span>
          {#if pendingDeleteId === dest.id}
            <div
              class="confirm"
              role="alert"
              data-testid="delete-confirm"
            >
              <span>Hide from picker? Existing Captures keep the reference.</span>
              <span class="form-actions">
                <button
                  type="button"
                  class="danger"
                  onclick={() => confirmDelete(dest.id)}
                  data-testid="delete-confirm-btn"
                >
                  Delete
                </button>
                <button type="button" class="ghost" onclick={cancelDelete}>
                  Cancel
                </button>
              </span>
            </div>
          {/if}
        {/if}
      </li>
    {/each}
  </ul>

  {#if deleted.length > 0}
    <div class="deleted-block" data-testid="deleted-block">
      <button
        type="button"
        class="deleted-toggle"
        onclick={() => (showDeleted = !showDeleted)}
        aria-expanded={showDeleted}
      >
        {showDeleted ? "▼" : "▶"} Soft-deleted ({deleted.length})
      </button>
      {#if showDeleted}
        <ul class="rows">
          {#each deleted as dest (dest.id)}
            <li class="row deleted-row" data-testid="deleted-row">
              <span class="row-main">
                {#if dest.color}
                  <span
                    class="dot"
                    style="background-color: {colorHex(dest.color)};"
                    aria-hidden="true"
                  ></span>
                {:else}
                  <span class="dot dot-empty" aria-hidden="true"></span>
                {/if}
                <span class="name">{dest.name}</span>
              </span>
              <span class="row-actions">
                <button
                  type="button"
                  class="ghost"
                  onclick={() => restore(dest.id)}
                  data-testid="restore-btn"
                >
                  Restore
                </button>
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</section>

<style>
  .section {
    margin-top: 1.25rem;
    padding: 1rem 1.1rem;
    background: #ffffff;
    border: 1px solid rgba(0, 0, 0, 0.08);
    border-radius: 10px;
  }
  @media (prefers-color-scheme: dark) {
    .section {
      background: #232327;
      border-color: rgba(255, 255, 255, 0.08);
    }
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.6rem;
  }

  h2 {
    margin: 0;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: rgba(0, 0, 0, 0.55);
  }
  @media (prefers-color-scheme: dark) {
    h2 {
      color: rgba(255, 255, 255, 0.55);
    }
  }

  .new-btn,
  .primary,
  .ghost {
    appearance: none;
    font: inherit;
    font-size: 0.78rem;
    border-radius: 6px;
    padding: 0.25rem 0.65rem;
    cursor: pointer;
    transition:
      background 80ms ease,
      border-color 80ms ease;
  }

  .new-btn {
    border: 1px solid rgba(76, 29, 149, 0.5);
    background: rgba(76, 29, 149, 0.1);
    color: rgba(76, 29, 149, 1);
  }
  .new-btn:hover {
    background: rgba(76, 29, 149, 0.18);
  }
  @media (prefers-color-scheme: dark) {
    .new-btn {
      border-color: rgba(167, 139, 250, 0.5);
      background: rgba(167, 139, 250, 0.12);
      color: rgba(167, 139, 250, 1);
    }
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

  .danger {
    color: rgba(220, 38, 38, 0.95);
  }
  .danger:hover {
    background: rgba(220, 38, 38, 0.12);
  }
  @media (prefers-color-scheme: dark) {
    .danger {
      color: rgba(248, 113, 113, 1);
    }
    .danger:hover {
      background: rgba(248, 113, 113, 0.15);
    }
  }

  .form {
    display: grid;
    gap: 0.5rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid rgba(0, 0, 0, 0.05);
    margin-bottom: 0.5rem;
  }
  .form.inline {
    border-bottom: none;
    margin-bottom: 0;
    padding: 0;
  }
  @media (prefers-color-scheme: dark) {
    .form {
      border-bottom-color: rgba(255, 255, 255, 0.06);
    }
  }

  .name-input {
    appearance: none;
    border: 1px solid rgba(0, 0, 0, 0.15);
    background: transparent;
    padding: 0.35rem 0.55rem;
    font: inherit;
    font-size: 0.88rem;
    border-radius: 6px;
    color: inherit;
  }
  .name-input:focus {
    outline: 2px solid rgba(76, 29, 149, 0.45);
    outline-offset: 0;
  }
  @media (prefers-color-scheme: dark) {
    .name-input {
      border-color: rgba(255, 255, 255, 0.15);
    }
  }

  .swatches {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
  }
  .swatch {
    width: 1.2rem;
    height: 1.2rem;
    border-radius: 999px;
    border: 1px solid rgba(0, 0, 0, 0.12);
    cursor: pointer;
    padding: 0;
    transition: transform 80ms ease;
  }
  .swatch:hover {
    transform: scale(1.08);
  }
  .swatch.selected {
    outline: 2px solid currentColor;
    outline-offset: 1.5px;
  }
  .swatch-none {
    background:
      linear-gradient(
        45deg,
        rgba(0, 0, 0, 0.1) 0%,
        transparent 50%,
        rgba(0, 0, 0, 0.1) 100%
      );
    background-color: transparent !important;
  }

  .form-actions {
    display: flex;
    gap: 0.4rem;
  }

  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .row {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: 0.4rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid rgba(0, 0, 0, 0.05);
  }
  .row:last-child {
    border-bottom: none;
  }
  @media (prefers-color-scheme: dark) {
    .row {
      border-bottom-color: rgba(255, 255, 255, 0.06);
    }
  }

  .row-main {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    min-width: 0;
  }
  .name {
    font-size: 0.88rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dot {
    width: 0.7rem;
    height: 0.7rem;
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

  .row-actions {
    display: flex;
    gap: 0.25rem;
  }

  .confirm {
    grid-column: 1 / -1;
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: rgba(220, 38, 38, 0.08);
    border: 1px solid rgba(220, 38, 38, 0.25);
    border-radius: 6px;
    padding: 0.4rem 0.6rem;
    font-size: 0.82rem;
    color: rgba(155, 28, 28, 1);
    margin-top: 0.35rem;
  }
  @media (prefers-color-scheme: dark) {
    .confirm {
      background: rgba(248, 113, 113, 0.1);
      border-color: rgba(248, 113, 113, 0.3);
      color: rgba(252, 165, 165, 1);
    }
  }

  .empty {
    margin: 0.3rem 0 0;
    color: rgba(0, 0, 0, 0.45);
    font-size: 0.82rem;
  }
  @media (prefers-color-scheme: dark) {
    .empty {
      color: rgba(255, 255, 255, 0.5);
    }
  }

  .error {
    margin: 0.3rem 0;
    padding: 0.35rem 0.6rem;
    background: rgba(220, 38, 38, 0.08);
    border: 1px solid rgba(220, 38, 38, 0.25);
    border-radius: 6px;
    color: rgba(155, 28, 28, 1);
    font-size: 0.82rem;
  }
  @media (prefers-color-scheme: dark) {
    .error {
      background: rgba(248, 113, 113, 0.1);
      border-color: rgba(248, 113, 113, 0.3);
      color: rgba(252, 165, 165, 1);
    }
  }

  .deleted-block {
    margin-top: 0.6rem;
    padding-top: 0.5rem;
    border-top: 1px solid rgba(0, 0, 0, 0.06);
  }
  @media (prefers-color-scheme: dark) {
    .deleted-block {
      border-top-color: rgba(255, 255, 255, 0.08);
    }
  }
  .deleted-toggle {
    appearance: none;
    background: transparent;
    border: none;
    color: rgba(0, 0, 0, 0.55);
    font: inherit;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    cursor: pointer;
    padding: 0;
    margin-bottom: 0.4rem;
  }
  @media (prefers-color-scheme: dark) {
    .deleted-toggle {
      color: rgba(255, 255, 255, 0.55);
    }
  }
  .deleted-row .name {
    opacity: 0.7;
  }
</style>
