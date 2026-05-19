<script lang="ts">
  // Settings window. Two-pane layout: left nav lists categories, right
  // pane renders the active section. Mirrors the brain project's
  // SettingsDialog shape (sidebar + details).
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import DestinationsSection from "$lib/destinations/DestinationsSection.svelte";
  import WikilinkFolderSection from "$lib/wikilink/WikilinkFolderSection.svelte";
  import UpdatesSection from "$lib/settings/UpdatesSection.svelte";

  type SectionId =
    | "shortcuts"
    | "destinations"
    | "wikilink"
    | "storage"
    | "updates";

  const SECTIONS: Array<{ id: SectionId; label: string; group: string }> = [
    { id: "shortcuts", label: "Shortcuts", group: "General" },
    { id: "destinations", label: "Destinations", group: "Capture" },
    { id: "wikilink", label: "Wikilink folder", group: "Capture" },
    { id: "storage", label: "Storage", group: "Advanced" },
    { id: "updates", label: "Updates", group: "Advanced" },
  ];

  const GROUPED = SECTIONS.reduce<Array<{ group: string; items: typeof SECTIONS }>>(
    (acc, item) => {
      const last = acc[acc.length - 1];
      if (last && last.group === item.group) last.items.push(item);
      else acc.push({ group: item.group, items: [item] });
      return acc;
    },
    [],
  );

  let activeSection = $state<SectionId>("shortcuts");

  let totalCount = $state<number | null>(null);
  let unreadCount = $state<number | null>(null);
  let dbPath = $state<string | null>(null);

  const SHORTCUTS: Array<{ keys: string[]; label: string }> = [
    {
      keys: ["⌃", "⌥", "⌘", "Space"],
      label: "Open Composer (write a note)",
    },
    {
      keys: ["⌃", "⌥", "⌘", "C"],
      label: "Capture clipboard",
    },
    {
      keys: ["⌃", "⌥", "⌘", "I"],
      label: "Open Inbox",
    },
    { keys: ["⌘", "F"], label: "Focus search inside Inbox" },
    { keys: ["⌘", "W"], label: "Close current window" },
    { keys: ["⌘", "Q"], label: "Quit" },
  ];

  onMount(async () => {
    try {
      const [total, unread, path] = await Promise.all([
        invoke<number>("total_count"),
        invoke<number>("unread_count"),
        invoke<string>("get_db_path"),
      ]);
      totalCount = Number(total) || 0;
      unreadCount = Number(unread) || 0;
      dbPath = path;
    } catch (err) {
      console.error("settings load failed", err);
    }
  });

  async function revealDb() {
    try {
      await invoke("reveal_db_in_finder");
    } catch (err) {
      console.error("reveal_db_in_finder failed", err);
    }
  }
</script>

<div class="settings">
  <nav class="sidebar" aria-label="Settings sections">
    <h1 class="brand">Settings</h1>
    {#each GROUPED as group}
      <div class="group">
        <div class="group-label">{group.group}</div>
        {#each group.items as item}
          <button
            type="button"
            class="nav-item"
            class:active={activeSection === item.id}
            aria-current={activeSection === item.id ? "page" : undefined}
            onclick={() => (activeSection = item.id)}
          >
            {item.label}
          </button>
        {/each}
      </div>
    {/each}
  </nav>

  <div class="details">
    {#if activeSection === "shortcuts"}
      <section class="section">
        <h2>Shortcuts</h2>
        <p class="lede">
          Read-only for now. Editing comes in a future release.
        </p>
        <dl class="shortcuts">
          {#each SHORTCUTS as s}
            <dt>
              {#each s.keys as key}<kbd>{key}</kbd>{/each}
            </dt>
            <dd>{s.label}</dd>
          {/each}
        </dl>
      </section>
    {:else if activeSection === "destinations"}
      <DestinationsSection />
    {:else if activeSection === "wikilink"}
      <WikilinkFolderSection />
    {:else if activeSection === "storage"}
      <section class="section">
        <h2>Storage</h2>
        <dl class="kv">
          <dt>Captures</dt>
          <dd>{totalCount ?? "—"}</dd>
          <dt>Unread</dt>
          <dd>{unreadCount ?? "—"}</dd>
          <dt>Database</dt>
          <dd class="path-row">
            <span class="path">{dbPath ?? "…"}</span>
            <button
              type="button"
              class="reveal"
              onclick={revealDb}
              disabled={dbPath === null}
            >
              Reveal in Finder
            </button>
          </dd>
        </dl>
      </section>
    {:else if activeSection === "updates"}
      <UpdatesSection />
    {/if}
  </div>
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

  .settings {
    display: grid;
    grid-template-columns: 200px 1fr;
    min-height: 100vh;
  }

  .sidebar {
    padding: 1.25rem 0.75rem 1rem;
    background: #efefef;
    border-right: 1px solid rgba(0, 0, 0, 0.08);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  @media (prefers-color-scheme: dark) {
    .sidebar {
      background: #1f1f22;
      border-right-color: rgba(255, 255, 255, 0.08);
    }
  }

  .brand {
    margin: 0 0.5rem 0.75rem;
    font-size: 0.95rem;
    letter-spacing: -0.01em;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .group-label {
    padding: 0.4rem 0.5rem 0.25rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: rgba(0, 0, 0, 0.45);
  }
  @media (prefers-color-scheme: dark) {
    .group-label {
      color: rgba(255, 255, 255, 0.45);
    }
  }

  .nav-item {
    appearance: none;
    background: transparent;
    border: 0;
    text-align: left;
    padding: 0.4rem 0.6rem;
    font: inherit;
    font-size: 0.85rem;
    color: inherit;
    border-radius: 6px;
    cursor: pointer;
    transition: background 80ms ease;
  }
  .nav-item:hover {
    background: rgba(0, 0, 0, 0.06);
  }
  .nav-item.active {
    background: rgba(76, 29, 149, 0.12);
    color: rgba(76, 29, 149, 1);
    font-weight: 500;
  }
  @media (prefers-color-scheme: dark) {
    .nav-item:hover {
      background: rgba(255, 255, 255, 0.06);
    }
    .nav-item.active {
      background: rgba(167, 139, 250, 0.16);
      color: rgba(167, 139, 250, 1);
    }
  }

  .details {
    padding: 1.5rem 1.75rem 2rem;
    overflow-y: auto;
    max-width: 640px;
  }

  .lede {
    margin: 0.3rem 0 1rem;
    color: rgba(0, 0, 0, 0.55);
    font-size: 0.85rem;
  }
  @media (prefers-color-scheme: dark) {
    .lede {
      color: rgba(255, 255, 255, 0.55);
    }
  }

  .section {
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

  .section h2 {
    margin: 0 0 0.65rem;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: rgba(0, 0, 0, 0.55);
  }
  @media (prefers-color-scheme: dark) {
    .section h2 {
      color: rgba(255, 255, 255, 0.55);
    }
  }

  dl {
    margin: 0;
    display: grid;
    grid-template-columns: max-content 1fr;
    column-gap: 1rem;
    row-gap: 0.45rem;
    font-size: 0.85rem;
  }
  dt {
    font-weight: 500;
  }
  dd {
    margin: 0;
    word-break: break-word;
  }

  /* Storage's name/value list shares the InboxDetail meta style:
     uppercase muted `dt` labels paired with full-strength values so
     "field name" and "value" read at one consistent color/size pair
     across the whole app. Shortcuts keeps the default `dt` because
     its dt slot holds <kbd> chips (the value) not a text label. */
  .kv dt {
    text-transform: uppercase;
    font-size: 0.7rem;
    letter-spacing: 0.06em;
    font-weight: 600;
    opacity: 0.55;
    align-self: center;
  }

  .shortcuts dt {
    white-space: nowrap;
    display: flex;
    gap: 0.15rem;
  }
  .shortcuts kbd {
    display: inline-block;
    min-width: 1.5em;
    text-align: center;
    padding: 0.05em 0.4em;
    font-family: inherit;
    font-size: 0.8em;
    background: rgba(0, 0, 0, 0.06);
    border: 1px solid rgba(0, 0, 0, 0.1);
    border-bottom-width: 2px;
    border-radius: 4px;
    color: inherit;
  }
  @media (prefers-color-scheme: dark) {
    .shortcuts kbd {
      background: rgba(255, 255, 255, 0.08);
      border-color: rgba(255, 255, 255, 0.12);
    }
  }

  .path {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.8rem;
    word-break: break-all;
  }

  .path-row {
    display: flex;
    align-items: flex-start;
    gap: 0.6rem;
    flex-wrap: wrap;
  }

  .reveal {
    flex: 0 0 auto;
    appearance: none;
    border: 1px solid rgba(76, 29, 149, 0.5);
    background: rgba(76, 29, 149, 0.1);
    color: rgba(76, 29, 149, 1);
    font: inherit;
    font-size: 0.78rem;
    padding: 0.2rem 0.6rem;
    border-radius: 6px;
    cursor: pointer;
    transition:
      background 80ms ease,
      border-color 80ms ease;
  }
  .reveal:hover {
    background: rgba(76, 29, 149, 0.18);
  }
  .reveal:disabled {
    opacity: 0.5;
    cursor: default;
  }
  @media (prefers-color-scheme: dark) {
    .reveal {
      border-color: rgba(167, 139, 250, 0.5);
      background: rgba(167, 139, 250, 0.12);
      color: rgba(167, 139, 250, 1);
    }
    .reveal:hover {
      background: rgba(167, 139, 250, 0.22);
    }
  }
</style>
