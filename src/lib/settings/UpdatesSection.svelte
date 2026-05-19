<script lang="ts">
  // Update settings: channel selector, auto-check toggle, manual check
  // button with state machine, and (for nightly-on-stable) a downgrade
  // entry point. Mirrors the brain UpdateSection shape but uses the
  // quick-capture inline-style CSS.

  import { onMount } from "svelte";
  import { invoke, Channel } from "@tauri-apps/api/core";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { updatesStore, type ReleaseChannel } from "./updates-store.svelte";

  // Canonical GitHub Releases page for the stable channel. Manual
  // fallback for the downgrade path if the in-app install fails.
  const STABLE_DOWNLOAD_URL =
    "https://github.com/diegorv/koko.quick-capture/releases/latest";

  type Status =
    | "idle"
    | "checking"
    | "downloading"
    | "ready"
    | "up-to-date"
    | "error";

  interface UpdateMetadata {
    rid: number;
    currentVersion: string;
    version: string;
    body: string | null;
  }

  // Mirrors the tauri-plugin-updater Channel event shape.
  type DownloadEvent =
    | { event: "Started"; data: { contentLength: number | null } }
    | { event: "Progress"; data: { chunkLength: number } }
    | { event: "Finished" };

  const CHANNEL_OPTIONS: Array<{
    value: ReleaseChannel;
    label: string;
    description: string;
  }> = [
    {
      value: "stable",
      label: "Stable",
      description: "Official tagged releases. Recommended for everyday use.",
    },
    {
      value: "nightly",
      label: "Nightly",
      description: "Built from the latest commit on main. May be unstable.",
    },
  ];

  let status = $state<Status>("idle");
  let errorMessage = $state("");
  let pendingUpdate = $state<UpdateMetadata | null>(null);
  let downloadedPercent = $state(0);
  let totalBytes = $state(0);

  onMount(() => {
    void updatesStore.load();
  });

  // True when the running build is nightly but the user picked stable
  // for updates. The in-app updater can't help here (nightly sorts
  // semver-higher than same-base stable) — the downgrade UI below
  // surfaces an explicit "install stable" path.
  const needsManualReinstall = $derived(
    __APP_CHANNEL__ === "nightly" &&
      updatesStore.prefs.channel === "stable",
  );

  function channelLabel(value: ReleaseChannel): string {
    return CHANNEL_OPTIONS.find((o) => o.value === value)?.label ?? value;
  }

  function channelDescription(value: ReleaseChannel): string {
    return CHANNEL_OPTIONS.find((o) => o.value === value)?.description ?? "";
  }

  function formatBytes(bytes: number): string {
    if (bytes <= 0) return "?";
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function formatLastChecked(ts: number | null): string {
    if (ts === null) return "Never";
    const diff = Date.now() - ts;
    if (diff < 60_000) return "Just now";
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} min ago`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} h ago`;
    return `${Math.floor(diff / 86_400_000)} d ago`;
  }

  function handleChannelChange(value: ReleaseChannel) {
    updatesStore.update({ channel: value });
    // Previous "Restart to update" pointed at an Update from the other
    // channel and is no longer valid.
    status = "idle";
    errorMessage = "";
    pendingUpdate = null;
    downloadedPercent = 0;
    totalBytes = 0;
  }

  async function checkForUpdates(
    opts: {
      allowDowngrades?: boolean;
      forceChannel?: ReleaseChannel;
    } = {},
  ) {
    const channel = opts.forceChannel ?? updatesStore.prefs.channel;
    const allowDowngrades = opts.allowDowngrades ?? false;
    status = "checking";
    errorMessage = "";
    try {
      const update = await invoke<UpdateMetadata | null>(
        "check_for_update_on_channel",
        { channel, allowDowngrades },
      );
      updatesStore.update({ lastCheckedAt: Date.now() });
      if (update) {
        pendingUpdate = update;
        status = "downloading";
        downloadedPercent = 0;
        totalBytes = 0;
        let downloadedBytes = 0;
        const onEvent = new Channel<DownloadEvent>();
        onEvent.onmessage = (msg) => {
          if (msg.event === "Started") {
            totalBytes = msg.data.contentLength ?? 0;
          } else if (msg.event === "Progress") {
            downloadedBytes += msg.data.chunkLength;
            if (totalBytes > 0) {
              downloadedPercent = Math.round(
                (downloadedBytes / totalBytes) * 100,
              );
            }
          } else if (msg.event === "Finished") {
            downloadedPercent = 100;
          }
        };
        await invoke("plugin:updater|download_and_install", {
          rid: update.rid,
          onEvent,
        });
        status = "ready";
      } else {
        status = "up-to-date";
      }
    } catch (err) {
      status = "error";
      errorMessage = err instanceof Error ? err.message : String(err);
    }
  }

  async function confirmInstallStable() {
    const ok = await ask(
      `This will replace your Nightly build (${__BUILD_INFO__}) with the latest Stable release. You may lose any changes that landed on main since the last Stable tag, until the next Stable release ships. Your captures and settings are unaffected.\n\nContinue?`,
      {
        title: "Install Stable (downgrade)",
        kind: "warning",
        okLabel: "Install Stable",
        cancelLabel: "Cancel",
      },
    );
    if (!ok) return;
    await checkForUpdates({ allowDowngrades: true, forceChannel: "stable" });
  }

  async function restartApp() {
    await relaunch();
  }
</script>

<section class="section" data-testid="updates-section">
  <h2>Update</h2>

  <div class="row">
    <div class="label">
      <div class="label-title">Release channel</div>
      <div class="label-desc">
        {channelDescription(updatesStore.prefs.channel)}
      </div>
    </div>
    <select
      class="control"
      value={updatesStore.prefs.channel}
      onchange={(e) =>
        handleChannelChange(
          (e.currentTarget as HTMLSelectElement).value as ReleaseChannel,
        )}
    >
      {#each CHANNEL_OPTIONS as opt (opt.value)}
        <option value={opt.value}>{opt.label}</option>
      {/each}
    </select>
  </div>

  {#if needsManualReinstall}
    <div class="row">
      <div class="label">
        <div class="label-title">Install Stable</div>
        <div class="label-desc">
          You're on a Nightly build but the updater is set to Stable. Use the
          in-app install below or open the Releases page as a fallback.
        </div>
      </div>
      <div class="control-group">
        <button type="button" class="btn primary" onclick={confirmInstallStable}>
          Install Stable (downgrade)
        </button>
        <button
          type="button"
          class="btn"
          onclick={() => openUrl(STABLE_DOWNLOAD_URL)}
        >
          Releases page
        </button>
      </div>
    </div>
  {/if}

  <div class="row">
    <div class="label">
      <div class="label-title">Current version</div>
      <div class="label-desc">The version currently installed</div>
    </div>
    <span class="value">{__BUILD_INFO__}</span>
  </div>

  <div class="row">
    <div class="label">
      <div class="label-title">Auto-check on launch</div>
      <div class="label-desc">
        Quietly check for an update when the app opens.
      </div>
    </div>
    <input
      type="checkbox"
      class="control"
      checked={updatesStore.prefs.autoCheck}
      onchange={(e) =>
        updatesStore.update({
          autoCheck: (e.currentTarget as HTMLInputElement).checked,
        })}
    />
  </div>

  <div class="row">
    <div class="label">
      <div class="label-title">Last checked</div>
      <div class="label-desc">When the app last asked GitHub</div>
    </div>
    <span class="value">
      {formatLastChecked(updatesStore.prefs.lastCheckedAt)}
    </span>
  </div>

  <div class="row">
    <div class="label">
      <div class="label-title">Check for updates</div>
      <div class="label-desc">
        Check the {channelLabel(updatesStore.prefs.channel).toLowerCase()} channel
        for a newer version
      </div>
    </div>
    <div class="control-group">
      {#if status === "idle"}
        <button type="button" class="btn" onclick={() => checkForUpdates()}>
          Check
        </button>
      {:else if status === "checking"}
        <span class="status">Checking…</span>
      {:else if status === "downloading"}
        <span class="status">
          Downloading v{pendingUpdate?.version} ({formatBytes(totalBytes)})… {downloadedPercent}%
        </span>
      {:else if status === "ready"}
        <button type="button" class="btn primary" onclick={restartApp}>
          Restart to update
        </button>
      {:else if status === "up-to-date"}
        <span class="status ok">You're up to date.</span>
      {:else if status === "error"}
        <span class="status error">{errorMessage}</span>
        <button type="button" class="btn" onclick={() => checkForUpdates()}>
          Retry
        </button>
      {/if}
    </div>
  </div>
</section>

<style>
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

  .row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.6rem 0;
    border-top: 1px solid rgba(0, 0, 0, 0.05);
  }
  .row:first-of-type {
    border-top: 0;
    padding-top: 0;
  }
  @media (prefers-color-scheme: dark) {
    .row {
      border-top-color: rgba(255, 255, 255, 0.06);
    }
  }

  .label {
    flex: 1;
    min-width: 0;
  }
  .label-title {
    font-size: 0.85rem;
    font-weight: 500;
  }
  .label-desc {
    margin-top: 0.15rem;
    font-size: 0.78rem;
    color: rgba(0, 0, 0, 0.55);
  }
  @media (prefers-color-scheme: dark) {
    .label-desc {
      color: rgba(255, 255, 255, 0.55);
    }
  }

  .control {
    flex: 0 0 auto;
    font: inherit;
    font-size: 0.85rem;
    padding: 0.25rem 0.4rem;
    border: 1px solid rgba(0, 0, 0, 0.15);
    border-radius: 6px;
    background: #fff;
    color: inherit;
  }
  @media (prefers-color-scheme: dark) {
    .control {
      background: #2a2a2e;
      border-color: rgba(255, 255, 255, 0.15);
    }
  }

  .control-group {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .btn {
    appearance: none;
    border: 1px solid rgba(76, 29, 149, 0.5);
    background: rgba(76, 29, 149, 0.1);
    color: rgba(76, 29, 149, 1);
    font: inherit;
    font-size: 0.78rem;
    padding: 0.25rem 0.7rem;
    border-radius: 6px;
    cursor: pointer;
    transition:
      background 80ms ease,
      border-color 80ms ease;
  }
  .btn:hover {
    background: rgba(76, 29, 149, 0.18);
  }
  .btn.primary {
    background: rgba(76, 29, 149, 1);
    color: #fff;
  }
  .btn.primary:hover {
    background: rgba(67, 26, 132, 1);
  }
  @media (prefers-color-scheme: dark) {
    .btn {
      border-color: rgba(167, 139, 250, 0.5);
      background: rgba(167, 139, 250, 0.12);
      color: rgba(167, 139, 250, 1);
    }
    .btn:hover {
      background: rgba(167, 139, 250, 0.22);
    }
    .btn.primary {
      background: rgba(167, 139, 250, 0.9);
      color: #1c1c1c;
    }
    .btn.primary:hover {
      background: rgba(167, 139, 250, 1);
    }
  }

  .value,
  .status {
    font-size: 0.85rem;
  }
  .status.ok {
    color: rgb(22, 163, 74);
  }
  .status.error {
    color: rgb(220, 38, 38);
  }
  @media (prefers-color-scheme: dark) {
    .status.ok {
      color: rgb(74, 222, 128);
    }
    .status.error {
      color: rgb(248, 113, 113);
    }
  }
</style>
