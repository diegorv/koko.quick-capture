Status: done

# Log batching on recording hot paths

## What to build

The chunker loop and VAD processor log via eprintln on every speech
segment and every VAD transition. During long recordings with
frequent speech, this generates high log volume and can impact
performance (eprintln holds a lock on stderr).

Add batched logging: suppress per-segment logs and emit a summary
periodically (every 200 chunks or 60 seconds, whichever comes
first). Keep error-level logs immediate.

Reference: meetily uses AudioMetricsBatcher and only logs every
200 chunks or 60s (pipeline.rs lines 799-815).

## Acceptance criteria

- [ ] Per-segment transcription logs batched (not per-event)
- [ ] Summary emitted every 60s or 200 segments
- [ ] Error/warning logs still emitted immediately
- [ ] VAD speech-start/end transitions logged only on state change (not repeated)
- [ ] No regression in recording behavior

## Blocked by

None - can start immediately
