Status: done

# Add EBU R128 loudness normalization

## Problem

Different microphones, distances, and gain settings produce wildly
different input levels. Whisper's mel spectrogram is sensitive to
absolute level - too quiet means missed words, too loud means clipping
artifacts. Simple RMS normalization doesn't account for perceptual
loudness.

## Solution

Add `ebur128` crate for ITU-R BS.1770 loudness measurement. Apply gain
normalization targeting -23 LUFS with a true-peak limiter at -1 dBTP.

Process after RNNoise (issue 02) and before resampling to 16kHz.
Operates on 48kHz audio in blocks (every 512 samples, update gain).

Reference: meetily `LoudnessNormalizer` in `audio_processing.rs`.

## Dependencies

```toml
ebur128 = "0.1"
```

## Where

- New: `src-tauri/src/audio/normalize.rs`
- Edit: `src-tauri/src/audio/mod.rs`
- Edit: `src-tauri/src/recording/mod.rs` (wire after denoise)
- Edit: `src-tauri/Cargo.toml`

## Verify

- Unit test: quiet signal (-40 dBFS) normalized to ~-23 LUFS range
- Unit test: loud signal (0 dBFS) limited, no clipping
- `cargo test` passes
- Manual test: record whispering vs normal voice, both should transcribe
