Status: done

# Buffer health monitoring in mixer and chunker

## What to build

The mixer logs buffer overflow but without severity or context.
Add periodic health diagnostics: buffer fill levels, overflow
counts, zero-padding frequency. This helps diagnose audio issues
(glitches, gaps, one-sided audio) without guessing.

Emit a health summary every 30s during recording with:
- Mic buffer fill (samples / max)
- System buffer fill (samples / max)
- Overflow events since last report
- Zero-pad events since last report (system behind)

Reference: meetily tracks per-device chunk counts, gap detection,
silence insertion stats (ffmpeg_mixer.rs SourceBuffer).

## Acceptance criteria

- [ ] Mixer tracks overflow count and zero-pad count
- [ ] Health summary emitted every 30s during recording
- [ ] Overflow events logged at warn level with source (mic/system)
- [ ] Stats reset after each report
- [ ] No performance impact on normal operation (counters only)

## Blocked by

None - can start immediately
