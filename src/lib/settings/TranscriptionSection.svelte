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

  const LANGUAGES: Array<{ code: string; label: string }> = [
    { code: "pt", label: "Portuguese" },
    { code: "en", label: "English" },
  ];

  let modelStatus = $state<ModelStatus | null>(null);
  let downloading = $state(false);
  let downloadProgress = $state({ downloaded: 0, total: 0 });
  let devices = $state<AudioDevice[]>([]);
  let selectedMic = $state<string | null>(null);
  let selectedSysDevice = $state<string | null>(null);
  let sysAudioEnabled = $state(false);
  let denoiseEnabled = $state(false);
  let selectedLanguage = $state("pt");
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

    try {
      selectedMic = await invoke<string | null>("get_mic_device");
    } catch {
      selectedMic = null;
    }

    try {
      selectedSysDevice = await invoke<string | null>("get_sys_audio_device");
    } catch {
      selectedSysDevice = null;
    }

    try {
      sysAudioEnabled = await invoke<boolean>("get_sys_audio_enabled");
    } catch {
      sysAudioEnabled = false;
    }

    try {
      denoiseEnabled = await invoke<boolean>("get_denoise_enabled");
    } catch {
      denoiseEnabled = false;
    }

    try {
      selectedLanguage = await invoke<string>("get_transcription_language");
    } catch {
      selectedLanguage = "pt";
    }
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

  async function onMicChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value;
    const name = value === "" ? null : value;
    selectedMic = name;
    try {
      await invoke("set_mic_device", { name });
    } catch (err) {
      console.error("set_mic_device failed", err);
    }
  }

  async function onLanguageChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value;
    selectedLanguage = value;
    try {
      await invoke("set_transcription_language", { language: value });
    } catch (err) {
      console.error("set_transcription_language failed", err);
    }
  }

  async function onSysDeviceChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value;
    const name = value === "" ? null : value;
    selectedSysDevice = name;
    try {
      await invoke("set_sys_audio_device", { name });
    } catch (err) {
      console.error("set_sys_audio_device failed", err);
    }
  }

  async function onSysEnabledChange(e: Event) {
    const checked = (e.target as HTMLInputElement).checked;
    sysAudioEnabled = checked;
    try {
      await invoke("set_sys_audio_enabled", { enabled: checked });
    } catch (err) {
      console.error("set_sys_audio_enabled failed", err);
    }
  }

  async function onDenoiseChange(e: Event) {
    const checked = (e.target as HTMLInputElement).checked;
    denoiseEnabled = checked;
    try {
      await invoke("set_denoise_enabled", { enabled: checked });
    } catch (err) {
      console.error("set_denoise_enabled failed", err);
    }
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
    <h3>Language</h3>
    <select class="select" value={selectedLanguage} onchange={onLanguageChange}>
      {#each LANGUAGES as lang}
        <option value={lang.code}>{lang.label}</option>
      {/each}
    </select>
  </div>

  <div class="subsection">
    <h3>Microphone</h3>
    {#if devices.filter((d) => d.device_type === "Input").length === 0}
      <p class="status missing">No microphones found</p>
    {:else}
      <select class="select" value={selectedMic ?? ""} onchange={onMicChange}>
        <option value="">System default</option>
        {#each devices.filter((d) => d.device_type === "Input") as mic}
          <option value={mic.name}>
            {mic.name}{mic.is_default ? " (default)" : ""}
          </option>
        {/each}
      </select>
    {/if}
  </div>

  <div class="subsection">
    <h3>System audio</h3>
    {#if devices.filter((d) => d.device_type === "System").length === 0}
      <p class="status missing">Not available (requires macOS 13+ and Screen Recording permission)</p>
    {:else}
      <label class="toggle-row">
        <input
          type="checkbox"
          checked={sysAudioEnabled}
          onchange={onSysEnabledChange}
        />
        <span>Capture system audio alongside microphone</span>
      </label>
      {#if sysAudioEnabled}
        <select class="select" value={selectedSysDevice ?? ""} onchange={onSysDeviceChange}>
          <option value="">First available</option>
          {#each devices.filter((d) => d.device_type === "System") as device}
            <option value={device.name}>{device.name}</option>
          {/each}
        </select>
      {/if}
    {/if}
  </div>

  <div class="subsection">
    <h3>Audio processing</h3>
    <label class="toggle-row">
      <input
        type="checkbox"
        checked={denoiseEnabled}
        onchange={onDenoiseChange}
      />
      <span>Noise suppression (RNNoise)</span>
    </label>
    <p class="hint">Reduces background noise before transcription. Disable if you experience audio artifacts.</p>
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

  .select {
    appearance: none;
    background: rgba(0, 0, 0, 0.04);
    border: 1px solid rgba(0, 0, 0, 0.12);
    border-radius: 6px;
    padding: 0.4rem 0.6rem;
    font: inherit;
    font-size: 0.82rem;
    color: inherit;
    width: 100%;
    cursor: pointer;
  }
  @media (prefers-color-scheme: dark) {
    .select {
      background: rgba(255, 255, 255, 0.06);
      border-color: rgba(255, 255, 255, 0.12);
    }
  }

  .toggle-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.82rem;
    margin-bottom: 0.5rem;
    cursor: pointer;
  }

  .toggle-row input[type="checkbox"] {
    accent-color: rgba(79, 70, 229, 0.85);
  }

  .hint {
    font-size: 0.72rem;
    color: rgba(0, 0, 0, 0.45);
    margin: 0.15rem 0 0;
  }
  @media (prefers-color-scheme: dark) {
    .hint {
      color: rgba(255, 255, 255, 0.4);
    }
  }

</style>
