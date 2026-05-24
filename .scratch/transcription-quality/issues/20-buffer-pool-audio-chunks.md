Status: ready-for-agent

# 20 - Buffer pool for audio chunk allocation

## What to build

Pre-allocate a pool of reusable `Vec<f32>` buffers to avoid heap allocation in the audio processing hot path (cpal callback -> mono conversion -> DSP -> mixer). Currently every audio callback allocates a fresh `Vec<f32>`.

Design:
- Pool of ~16 pre-allocated buffers, each sized for one mixer window (e.g., 28800 samples at 48kHz for 600ms)
- RAII guard (`PooledBuffer`) that auto-returns the buffer to the pool on drop
- Lock-free or low-contention checkout (crossbeam channel or atomic index)
- Fallback: if pool is exhausted, allocate a fresh Vec (never block the audio thread)

Wire into the audio capture path where `audio_to_mono()` output and DSP intermediates are produced.

## Acceptance criteria

- [ ] `BufferPool` struct with `checkout() -> PooledBuffer` and auto-return on drop
- [ ] Pool pre-allocates 16 buffers at recording start
- [ ] Fallback allocation when pool is exhausted (no panics, no blocking)
- [ ] At least the mono conversion output in the stream callback uses pooled buffers
- [ ] Pool usage stats available via health reporting (checkout count, fallback count)
- [ ] No measurable regression in audio latency (pool overhead < 1us per checkout)
- [ ] Existing tests pass

## Blocked by

None - can start immediately
