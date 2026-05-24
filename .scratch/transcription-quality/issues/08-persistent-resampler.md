Status: ready-for-agent

# Use persistent resampler across chunks

## Problem

Current code creates a new `rubato::SincFixedIn` resampler per chunk
in `resample_to_16khz()`. This discards the resampler's internal filter
state between chunks, causing:
- Energy discontinuities at chunk boundaries
- Potential artifacts from filter ramp-up on each chunk
- Wasted CPU on repeated initialization

## Solution

Create the resampler once per recording session and reuse it across
chunks. The resampler should be created in `RecordingHandle::start()`
(or `start_chunker()`) and passed to the chunker loop.

Reference: meetily uses persistent resampler instances, preserved
across chunk boundaries.

## Where

- Edit: `src-tauri/src/audio/resample.rs` (make resampler reusable)
- Edit: `src-tauri/src/recording/mod.rs` (create once, pass to chunker)

## Verify

- Record >20s (spans multiple chunks)
- No audible artifacts at chunk boundaries in saved WAV
- `cargo test` passes
