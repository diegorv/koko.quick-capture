Status: done

# Add RNNoise neural noise suppression

## Problem

Background noise (keyboard, people talking nearby, outdoor ambient)
reduces transcription accuracy. Whisper can handle mild noise but
degrades on moderate-to-loud environments. RNNoise provides 10-15 dB
noise reduction using a tiny neural network (<100KB model, built-in).

## Solution

Add `nnnoiseless` crate (pure Rust port of RNNoise). Process audio
at 48kHz in 480-sample (10ms) frames before resampling to 16kHz.

RNNoise operates at 48kHz. Current pipeline resamples from native rate
directly to 16kHz. New pipeline order:
1. Capture at native rate
2. Resample to 48kHz (if not already)
3. High-pass filter at 80 Hz (issue 01)
4. RNNoise denoise at 48kHz
5. Resample to 16kHz for whisper

Side benefit: RNNoise returns per-frame VAD probability (0.0-1.0) which
could supplement or replace the RMS silence gate.

Reference: meetily `pipeline.rs` and dsnote `rnnoise-nu` use same approach.

## Dependencies

```toml
nnnoiseless = "0.5"
```

## Where

- New: `src-tauri/src/audio/denoise.rs`
- Edit: `src-tauri/src/audio/mod.rs`
- Edit: `src-tauri/src/audio/resample.rs` (add resample_to_48khz or make generic)
- Edit: `src-tauri/src/recording/mod.rs` (wire denoise into pipeline)
- Edit: `src-tauri/Cargo.toml`

## Verify

- Unit test: white noise input -> output RMS significantly lower
- `cargo test` passes
- Manual test: record with keyboard typing nearby, compare before/after
