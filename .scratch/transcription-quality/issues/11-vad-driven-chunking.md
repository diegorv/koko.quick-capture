Status: ready-for-agent

# Replace time-based chunking with VAD-driven chunking

## Problem

Current chunker accumulates 20s of audio then transcribes. This has
drawbacks:
- Silence is included in the 20s budget (wastes compute)
- Chunk boundaries may still cut mid-sentence even with silence-split
- No partial transcript until first 20s chunk completes
- For short recordings (<20s), all transcription happens at stop time

## Solution

Replace the fixed-interval chunker with a VAD-driven architecture
similar to meetily's:

1. Silero VAD runs continuously on the audio stream (via `silero-rs`
   crate, not whisper.cpp internal VAD)
2. Detects speech start/end transitions in real time
3. When speech ends (silence > 2s redemption_time), send the speech
   segment to whisper immediately
4. Only speech frames reach whisper - silence is discarded upstream
5. Partial transcript updates happen per natural speech segment

This is a larger architectural change to the chunker loop. The current
whisper-internal VAD (via `WhisperVadParams`) would become redundant
since VAD happens before whisper sees the audio.

Key parameters from meetily (calibrated for natural speech):
- `positive_speech_threshold = 0.50`
- `negative_speech_threshold = 0.35`
- `redemption_time = 2000ms` (batch), 400ms (streaming)
- `pre_speech_pad = 300ms`
- `post_speech_pad = 400ms`
- `min_speech_time = 250ms`

Reference: meetily `audio/vad.rs` + `ContinuousVadProcessor`.

## Dependencies

```toml
silero-rs = "..."  # or use whisper-rs VAD API with custom chunker
```

Alternative: keep whisper.cpp internal VAD but restructure the chunker
to use shorter time windows (5-10s) with VAD pre-check. Simpler but
less optimal.

## Where

- Rewrite: `src-tauri/src/recording/mod.rs` (chunker_loop architecture)
- New: `src-tauri/src/audio/vad.rs` (VAD processor wrapper)
- Edit: `src-tauri/src/audio/mod.rs`

## Verify

- Record 30s with 10s of silence in the middle
- Transcript should arrive in 2 segments (before and after silence)
- No transcript during silence portions
- Partial transcript appears after first speech segment ends
- Total transcription time should be faster (less audio to process)
