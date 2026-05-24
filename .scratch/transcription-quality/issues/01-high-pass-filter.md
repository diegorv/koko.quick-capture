Status: ready-for-agent

# Add high-pass filter to audio pipeline

## Problem

Low-frequency rumble (AC hum, handling noise, HVAC, traffic) degrades
whisper transcription quality. Frequencies below 80 Hz contain no speech
information but add noise to the mel spectrogram.

## Solution

Add a first-order IIR high-pass filter at 80 Hz cutoff, applied to the
raw audio stream before resampling to 16kHz.

Implementation: ~40 lines, zero external deps. First-order RC digital
filter with coefficient `alpha = 1 / (1 + 2*pi*cutoff/sample_rate)`.

Reference: meetily `audio_processing.rs` uses identical approach.

## Where

- New: `src-tauri/src/audio/filter.rs` (high-pass filter struct)
- Edit: `src-tauri/src/audio/mod.rs` (re-export)
- Edit: `src-tauri/src/recording/mod.rs` (apply filter in chunker before resample)

## Verify

- Unit test: sine wave at 50 Hz should be attenuated >6dB
- Unit test: sine wave at 200 Hz should pass through ~unattenuated
- `cargo test` passes
- Manual test: record with fan/AC noise, compare transcript quality
