<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import Composer from "$lib/composer/Composer.svelte";
  import { OPEN_COMPOSER } from "$lib/events";

  let focusKey = $state(0);
  let unlisten: UnlistenFn | undefined;
  let recordingActive = $state(false);
  let recordingElapsed = $state(0);
  let partialTranscript = $state("");
  let peakLevel = $state(0);
  let recordingTimer: ReturnType<typeof setInterval> | undefined;
  let vuTimer: ReturnType<typeof setInterval> | undefined;

  async function save(text: string) {
    try {
      const result = await invoke("save_note", { text });
      console.log("save_note ok", result);
    } catch (err) {
      console.error("save_note failed", err);
    }
  }

  async function close() {
    if (recordingActive) {
      await stopRecording();
      return;
    }
    try {
      await invoke("dismiss_composer");
    } catch (err) {
      console.error("dismiss_composer failed", err);
    }
  }

  async function startRecording() {
    try {
      const status = await invoke<{ downloaded: boolean }>("get_model_status");
      if (!status.downloaded) {
        console.log("Downloading transcription model...");
        await invoke("download_model");
      }
      await invoke("start_recording");
      recordingActive = true;
      recordingElapsed = 0;
      partialTranscript = "";
      recordingTimer = setInterval(async () => {
        try {
          const s = await invoke<{
            elapsed_secs: number;
            partial_transcript: string;
            peak_level: number;
          }>("get_recording_status");
          recordingElapsed = s.elapsed_secs;
          partialTranscript = s.partial_transcript;
        } catch {
          recordingElapsed += 1;
        }
      }, 2000);
      vuTimer = setInterval(async () => {
        try {
          const s = await invoke<{ peak_level: number }>("get_recording_status");
          peakLevel = s.peak_level;
        } catch {
          // ignore
        }
      }, 100);
    } catch (err) {
      console.error("start_recording failed", err);
    }
  }

  async function stopRecording() {
    if (recordingTimer) {
      clearInterval(recordingTimer);
      recordingTimer = undefined;
    }
    if (vuTimer) {
      clearInterval(vuTimer);
      vuTimer = undefined;
    }
    try {
      await invoke("stop_recording");
    } catch (err) {
      console.error("stop_recording failed", err);
    }
    recordingActive = false;
    recordingElapsed = 0;
    partialTranscript = "";
    try {
      await invoke("dismiss_composer");
    } catch (err) {
      console.error("dismiss_composer failed", err);
    }
  }

  onMount(async () => {
    try {
      unlisten = await listen(OPEN_COMPOSER, () => {
        focusKey += 1;
      });
    } catch (err) {
      console.error("listen open_composer failed", err);
    }
  });

  onDestroy(() => {
    unlisten?.();
    if (recordingTimer) clearInterval(recordingTimer);
    if (vuTimer) clearInterval(vuTimer);
  });
</script>

<Composer
  {save}
  onclose={close}
  {focusKey}
  onStartRecording={startRecording}
  onStopRecording={stopRecording}
  {recordingActive}
  {recordingElapsed}
  {partialTranscript}
  {peakLevel}
/>
