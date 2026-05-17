<script lang="ts">
  // Settings window placeholder. v0.3 only displays static
  // information; editing shortcuts / storage path / etc. lands in
  // future slices.
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";

  let version = $state("…");
  let totalCount = $state<number | null>(null);
  let unreadCount = $state<number | null>(null);

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
      version = await getVersion();
    } catch (err) {
      console.error("read version failed", err);
    }
    try {
      const [total, unread] = await Promise.all([
        invoke<number>("total_count"),
        invoke<number>("unread_count"),
      ]);
      totalCount = Number(total) || 0;
      unreadCount = Number(unread) || 0;
    } catch (err) {
      console.error("counts failed", err);
    }
  });
</script>

<div class="settings">
  <header class="header">
    <h1>Settings</h1>
    <p class="lede">
      Read-only for now. Editing shortcuts and storage path comes in a
      future release.
    </p>
  </header>

  <section class="section">
    <h2>Shortcuts</h2>
    <dl class="shortcuts">
      {#each SHORTCUTS as s}
        <dt>
          {#each s.keys as key}<kbd>{key}</kbd>{/each}
        </dt>
        <dd>{s.label}</dd>
      {/each}
    </dl>
  </section>

  <section class="section">
    <h2>Storage</h2>
    <dl class="kv">
      <dt>Captures</dt>
      <dd>{totalCount ?? "—"}</dd>
      <dt>Unread</dt>
      <dd>{unreadCount ?? "—"}</dd>
      <dt>Database</dt>
      <dd class="path">~/Library/Application Support/com.koko.quick-capture/captures.db</dd>
    </dl>
  </section>

  <section class="section">
    <h2>About</h2>
    <dl class="kv">
      <dt>Version</dt>
      <dd>{version}</dd>
      <dt>Source</dt>
      <dd>Frictionless macOS capture inbox · Tauri 2 + SvelteKit + Rust</dd>
    </dl>
  </section>
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
    padding: 1.5rem 1.75rem 2rem;
    max-width: 560px;
    margin: 0 auto;
  }

  .header h1 {
    margin: 0;
    font-size: 1.25rem;
    letter-spacing: -0.01em;
  }
  .lede {
    margin: 0.3rem 0 1.5rem;
    color: rgba(0, 0, 0, 0.55);
    font-size: 0.85rem;
  }
  @media (prefers-color-scheme: dark) {
    .lede {
      color: rgba(255, 255, 255, 0.55);
    }
  }

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
    font-size: 0.88rem;
  }
  dt {
    color: rgba(0, 0, 0, 0.6);
    font-weight: 500;
  }
  @media (prefers-color-scheme: dark) {
    dt {
      color: rgba(255, 255, 255, 0.6);
    }
  }
  dd {
    margin: 0;
    word-break: break-word;
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
    font-size: 0.78rem;
    opacity: 0.85;
  }
</style>
