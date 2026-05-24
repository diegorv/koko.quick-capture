Status: done

# Cross-rate resampling in AudioMixerRingBuffer

## What to build

The mixer assumes mic and system audio share the same sample rate.
On macOS, mic devices can run at 44.1kHz while ScreenCaptureKit
system audio runs at 48kHz. When rates differ, sample-by-sample
mixing produces temporal misalignment and pitch distortion.

Capture the system audio sample rate from AudioCapture, pass it to
the mixer, and resample system audio to the mic's rate before
buffering. Use a PersistentResampler inside the mixer for
energy-preserving cross-chunk resampling.

The mixer constructor should accept separate mic_rate and sys_rate
parameters. When they match, skip resampling (current behavior).

## Acceptance criteria

- [ ] AudioMixerRingBuffer accepts separate mic_rate and sys_rate
- [ ] System audio resampled to mic rate before push_system buffers it
- [ ] PersistentResampler used (not one-shot) for continuity
- [ ] RecordingHandle captures and forwards system sample rate
- [ ] No regression when mic-only (no system audio)
- [ ] Unit test: mix 44.1kHz mic + 48kHz system produces correct duration output

## Blocked by

None - can start immediately
