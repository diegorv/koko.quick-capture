<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";

  interface ModelStatus {
    downloaded: boolean;
    path: string;
  }

  interface AudioDevice {
    name: string;
    device_type: "Input" | "System";
    is_default: boolean;
  }

  let modelStatus = $state<ModelStatus | null>(null);
  let downloading = $state(false);
  let downloadProgress = $state({ downloaded: 0, total: 0 });
  let devices = $state<AudioDevice[]>([]);
  let unlistenProgress: UnlistenFn | undefined;

  onMount(async () => {
    try {
      modelStatus = await invoke<ModelStatus>("get_model_status");
    } catch (err) {
      console.error("get_model_status failed", err);
    }

    try {
      unlistenProgress = await listen<[number, number]>(
        "model:download-progress",
        (event) => {
          downloadProgress = {
            downloaded: event.payload[0],
            total: event.payload[1],
          };
        },
      );
    } catch (err) {
      console.error("listen download-progress failed", err);
    }

    await refreshDevices();
  });

  onDestroy(() => {
    unlistenProgress?.();
  });

  async function refreshDevices() {
    try {
      devices = await invoke<AudioDevice[]>("list_audio_devices");
    } catch {
      devices = [];
    }
  }

  async function downloadModel() {
    downloading = true;
    downloadProgress = { downloaded: 0, total: 0 };
    try {
      modelStatus = await invoke<ModelStatus>("download_model");
    } catch (err) {
      console.error("download_model failed", err);
    }
    downloading = false;
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
</script>

<section class="section">
  <h2>Transcription</h2>

  <div class="subsection">
    <h3>Whisper model</h3>
    {#if modelStatus?.downloaded}
      <p class="status ok">Downloaded</p>
      <p class="model-path">{modelStatus.path}</p>
    {:else if downloading}
      <p class="status downloading">
        Downloading... {formatBytes(downloadProgress.downloaded)}
        {#if downloadProgress.total > 0}
          / {formatBytes(downloadProgress.total)}
        {/if}
      </p>
      {#if downloadProgress.total > 0}
        <div class="progress-bar">
          <div
            class="progress-fill"
            style="width: {(downloadProgress.downloaded / downloadProgress.total) * 100}%"
          ></div>
        </div>
      {/if}
    {:else}
      <p class="status missing">Not downloaded</p>
      <button type="button" class="action-btn" onclick={downloadModel}>
        Download model (~547 MB)
      </button>
    {/if}
  </div>

  <div class="subsection">
    <h3>Audio devices</h3>
    {#if devices.length === 0}
      <p class="status missing">No devices found</p>
    {:else}
      <ul class="device-list">
        {#each devices as device}
          <li class:default={device.is_default}>
            <span class="device-name">{device.name}</span>
            <span class="device-type">{device.device_type === "Input" ? "Mic" : "System"}</span>
            {#if device.is_default}
              <span class="device-badge">Default</span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
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

  h2 {
    margin: 0 0 0.65rem;
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

  .subsection {
    margin-bottom: 1rem;
  }
  .subsection:last-child {
    margin-bottom: 0;
  }

  h3 {
    margin: 0 0 0.4rem;
    font-size: 0.82rem;
    font-weight: 600;
  }

  .status {
    font-size: 0.82rem;
    margin: 0 0 0.3rem;
  }
  .status.ok {
    color: #16a34a;
  }
  .status.missing {
    color: rgba(0, 0, 0, 0.45);
  }
  .status.downloading {
    color: rgba(79, 70, 229, 0.9);
  }
  @media (prefers-color-scheme: dark) {
    .status.missing {
      color: rgba(255, 255, 255, 0.45);
    }
  }

  .model-path {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.72rem;
    opacity: 0.5;
    word-break: break-all;
    margin: 0;
  }

  .progress-bar {
    height: 4px;
    background: rgba(0, 0, 0, 0.08);
    border-radius: 2px;
    overflow: hidden;
    margin-top: 0.4rem;
  }
  @media (prefers-color-scheme: dark) {
    .progress-bar {
      background: rgba(255, 255, 255, 0.08);
    }
  }

  .progress-fill {
    height: 100%;
    background: rgba(79, 70, 229, 0.85);
    transition: width 200ms ease;
  }

  .action-btn {
    appearance: none;
    border: 1px solid rgba(79, 70, 229, 0.5);
    background: rgba(79, 70, 229, 0.1);
    color: rgba(79, 70, 229, 1);
    font: inherit;
    font-size: 0.8rem;
    padding: 0.35rem 0.8rem;
    border-radius: 6px;
    cursor: pointer;
    transition: background 80ms ease;
  }
  .action-btn:hover {
    background: rgba(79, 70, 229, 0.18);
  }
  @media (prefers-color-scheme: dark) {
    .action-btn {
      border-color: rgba(167, 139, 250, 0.5);
      background: rgba(167, 139, 250, 0.12);
      color: rgba(167, 139, 250, 1);
    }
    .action-btn:hover {
      background: rgba(167, 139, 250, 0.22);
    }
  }

  .device-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .device-list li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.82rem;
    padding: 0.3rem 0.5rem;
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.03);
  }
  @media (prefers-color-scheme: dark) {
    .device-list li {
      background: rgba(255, 255, 255, 0.04);
    }
  }

  .device-name {
    flex: 1;
  }

  .device-type {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    opacity: 0.5;
  }

  .device-badge {
    font-size: 0.65rem;
    padding: 0.1rem 0.35rem;
    background: rgba(79, 70, 229, 0.15);
    color: rgba(79, 70, 229, 1);
    border-radius: 4px;
    font-weight: 600;
  }
  @media (prefers-color-scheme: dark) {
    .device-badge {
      background: rgba(167, 139, 250, 0.2);
      color: rgba(167, 139, 250, 1);
    }
  }
</style>
