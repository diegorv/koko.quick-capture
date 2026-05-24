<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";

  interface RecordingStatus {
    active: boolean;
    elapsed_secs: number;
    mic_peak: number;
    sys_peak: number;
    sys_active: boolean;
    partial_transcript: string;
  }

  let recording = $state(false);
  let elapsed = $state(0);
  let micPeak = $state(0);
  let sysPeak = $state(0);
  let sysActive = $state(false);
  let sysEnabled = $state(false);
  let transcript = $state("");
  let processing = $state(false);

  let vuTimer: ReturnType<typeof setInterval> | undefined;
  let statusTimer: ReturnType<typeof setInterval> | undefined;

  function levelColor(level: number): string {
    if (level > 0.8) return "#e74c3c";
    if (level > 0.4) return "#f39c12";
    return "#2ecc71";
  }

  function formatElapsed(secs: number): string {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  onMount(async () => {
    try {
      sysEnabled = await invoke<boolean>("get_sys_audio_enabled");
    } catch {
      sysEnabled = false;
    }
  });

  onDestroy(() => {
    clearTimers();
  });

  function clearTimers() {
    if (vuTimer) { clearInterval(vuTimer); vuTimer = undefined; }
    if (statusTimer) { clearInterval(statusTimer); statusTimer = undefined; }
  }

  async function startRecording() {
    try {
      const status = await invoke<{ downloaded: boolean }>("get_model_status");
      if (!status.downloaded) {
        await invoke("download_model");
      }
      await invoke("start_recording");
      recording = true;
      elapsed = 0;
      transcript = "";

      vuTimer = setInterval(async () => {
        try {
          const s = await invoke<RecordingStatus>("get_recording_status");
          micPeak = s.mic_peak;
          sysPeak = s.sys_peak;
          sysActive = s.sys_active;
        } catch { /* ignore */ }
      }, 100);

      statusTimer = setInterval(async () => {
        try {
          const s = await invoke<RecordingStatus>("get_recording_status");
          elapsed = s.elapsed_secs;
          transcript = s.partial_transcript;
        } catch {
          elapsed += 2;
        }
      }, 2000);
    } catch (err) {
      console.error("start_recording failed", err);
    }
  }

  async function stopRecording() {
    clearTimers();
    processing = true;
    try {
      await invoke("stop_recording");
    } catch (err) {
      console.error("stop_recording failed", err);
    }
    recording = false;
    processing = false;
    elapsed = 0;
    micPeak = 0;
    sysPeak = 0;
    transcript = "";
    try {
      await invoke("dismiss_recording");
    } catch { /* ignore - command may not exist yet */ }
  }

  async function toggleSysAudio() {
    sysEnabled = !sysEnabled;
    try {
      await invoke("set_sys_audio_enabled", { enabled: sysEnabled });
    } catch (err) {
      console.error("set_sys_audio_enabled failed", err);
    }
  }

  async function dismiss() {
    if (recording) {
      await stopRecording();
      return;
    }
    try {
      await invoke("dismiss_recording");
    } catch {
      // fallback: hide via window API
    }
  }
</script>

<svelte:window on:keydown={(e) => { if (e.key === "Escape") dismiss(); }} />

<div class="recording">
  <div class="drag-strip" data-tauri-drag-region></div>
  <div class="controls">
    <div class="toggle-row">
      <button
        type="button"
        class="toggle-pill"
        class:active={true}
        title="Microphone"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>
        Mic
      </button>
      <button
        type="button"
        class="toggle-pill"
        class:active={sysEnabled}
        onclick={toggleSysAudio}
        title="System audio"
        disabled={recording}
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/></svg>
        System
      </button>
    </div>

    <div class="vu-section">
      <div class="vu-row">
        <span class="vu-label">MIC</span>
        <div class="vu-track">
          <div
            class="vu-fill"
            style="width: {Math.min(micPeak * 100, 100)}%; background: {levelColor(micPeak)}"
          ></div>
        </div>
      </div>
      {#if sysActive || sysEnabled}
        <div class="vu-row">
          <span class="vu-label">SYS</span>
          <div class="vu-track">
            <div
              class="vu-fill"
              style="width: {Math.min(sysPeak * 100, 100)}%; background: {levelColor(sysPeak)}"
            ></div>
          </div>
        </div>
      {/if}
    </div>
  </div>

  <div class="transcript-area">
    {#if processing}
      <p class="processing">Transcribing...</p>
    {:else if transcript}
      <p class="transcript-text">{transcript}</p>
    {:else if !recording}
      <p class="placeholder">Press Record to start capturing voice</p>
    {/if}
  </div>

  <div class="footer">
    {#if recording}
      <button type="button" class="record-btn stop" onclick={stopRecording}>
        <span class="stop-icon"></span>
        Stop
      </button>
    {:else}
      <button type="button" class="record-btn" onclick={startRecording} disabled={processing}>
        <span class="record-icon"></span>
        Record
      </button>
    {/if}
    <span class="timer">{formatElapsed(elapsed)}</span>
  </div>
</div>

<style>
  .recording {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 1rem 1.25rem 0.75rem;
    box-sizing: border-box;
    background: rgba(248, 248, 248, 0.98);
    color: #0f0f0f;
    font-family: Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    border-radius: 12px;
  }

  .drag-strip {
    height: 12px;
    cursor: grab;
  }

  .controls {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .toggle-row {
    display: flex;
    gap: 0.4rem;
  }

  .toggle-pill {
    appearance: none;
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.25rem 0.6rem;
    font: inherit;
    font-size: 0.75rem;
    font-weight: 500;
    border-radius: 12px;
    cursor: pointer;
    border: 1px solid rgba(128, 128, 128, 0.25);
    background: transparent;
    color: inherit;
    opacity: 0.5;
    transition: all 100ms;
  }
  .toggle-pill.active {
    opacity: 1;
    border-color: #2ecc71;
    background: rgba(46, 204, 113, 0.15);
    color: #2ecc71;
  }
  .toggle-pill:disabled {
    cursor: default;
  }

  .vu-section {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .vu-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .vu-label {
    font-size: 0.6rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    opacity: 0.45;
    width: 2rem;
    text-align: right;
  }

  .vu-track {
    flex: 1;
    height: 4px;
    background: rgba(128, 128, 128, 0.15);
    border-radius: 2px;
    overflow: hidden;
  }

  .vu-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.08s linear;
  }

  .transcript-area {
    flex: 1;
    overflow-y: auto;
    margin: 0.6rem 0;
    padding: 0 0.25rem;
    min-height: 0;
  }

  .transcript-text {
    font-size: 0.82rem;
    line-height: 1.4;
    margin: 0;
    white-space: pre-wrap;
  }

  .placeholder {
    font-size: 0.82rem;
    opacity: 0.35;
    margin: 0;
    text-align: center;
    padding-top: 1.5rem;
  }

  .processing {
    font-size: 0.82rem;
    opacity: 0.6;
    margin: 0;
    text-align: center;
    padding-top: 1.5rem;
    animation: pulse-text 1.5s ease-in-out infinite;
  }

  @keyframes pulse-text {
    0%, 100% { opacity: 0.6; }
    50% { opacity: 0.3; }
  }

  .footer {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
    padding-top: 0.25rem;
  }

  .record-btn {
    appearance: none;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.4rem 1.2rem;
    font: inherit;
    font-size: 0.85rem;
    font-weight: 600;
    border: none;
    border-radius: 8px;
    cursor: pointer;
    background: rgba(79, 70, 229, 0.12);
    color: rgba(79, 70, 229, 1);
    transition: background 100ms;
  }
  .record-btn:hover {
    background: rgba(79, 70, 229, 0.2);
  }
  .record-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .record-btn.stop {
    background: rgba(239, 68, 68, 0.12);
    color: #ef4444;
  }
  .record-btn.stop:hover {
    background: rgba(239, 68, 68, 0.2);
  }

  .record-icon {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: currentColor;
  }

  .stop-icon {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    background: currentColor;
  }

  .timer {
    font-size: 1.1rem;
    font-variant-numeric: tabular-nums;
    font-weight: 500;
    opacity: 0.6;
    min-width: 3rem;
  }

  @media (prefers-color-scheme: dark) {
    .recording {
      background: rgba(30, 30, 30, 0.98);
      color: #f4f4f4;
    }
  }
</style>
