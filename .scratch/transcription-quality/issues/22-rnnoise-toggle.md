Status: done

# 22 - RNNoise toggle (optional denoise)

## What to build

Make RNNoise noise suppression configurable via user settings. Meetily's team found Whisper handles noise well internally and ships with RNNoise disabled by default. Our pipeline currently always applies RNNoise in the DSP chain. Adding a toggle lets users disable it if they experience artifacts or prefer raw audio quality.

Design:
- Add a `denoise_enabled` boolean to the Tauri store (default: `true` to preserve current behavior)
- Read the setting at recording start and pass it through to the chunker loop
- When disabled, skip the `Denoiser::process()` call in the DSP chain but keep HPF and normalization active
- Expose the setting via existing settings UI

## Acceptance criteria

- [ ] `denoise_enabled` setting exists in Tauri store with default `true`
- [ ] When `denoise_enabled` is false, the DSP chain skips RNNoise but still applies HPF and normalization
- [ ] When `denoise_enabled` is true, behavior is identical to current (no regression)
- [ ] Setting is read once at recording start (not per-chunk) to avoid hot-path overhead
- [ ] Frontend settings UI has a toggle for "Noise suppression" under audio settings
- [ ] Manual test: toggle off, record in noisy environment, verify transcription still works (Whisper handles noise)
- [ ] Manual test: toggle on, verify noise reduction is audible in saved WAV

## Blocked by

None - can start immediately
