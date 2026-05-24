Status: done

# Synchronize mic + system audio mixing

## Problem

When both mic and system audio are enabled, both streams send into the
same unbounded mpsc channel unsynchronized. Samples interleave based on
thread scheduling, not time alignment. This can cause:
- Temporal misalignment between mic and system audio
- Volume imbalance (one source louder than the other)
- Phase cancellation on overlapping speech

## Solution

Use a ring-buffer mixer (like meetily's AudioMixerRingBuffer) that:
1. Maintains separate buffers for mic and system audio
2. Time-aligns by sample position
3. Mixes with gain balancing
4. Zero-pads when one stream is behind

Alternative simpler approach: keep separate channels, track sample
counts per source, interleave at chunk boundary based on sample count.

## Where

- New: `src-tauri/src/audio/mixer.rs`
- Edit: `src-tauri/src/recording/mod.rs` (use mixer instead of shared tx)

## Verify

- Record with both mic and system audio
- Transcription captures both sources coherently
- No duplicate or garbled audio
