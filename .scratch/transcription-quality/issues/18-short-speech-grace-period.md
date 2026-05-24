Status: done

# 18 - Short-speech grace period on recording stop

## What to build

When a user releases the record key very quickly (fast tap), the buffered audio can be extremely short (<50ms), causing truncated or empty transcriptions. Add a grace period to the stop-recording flow: if the accumulated audio at stop time is shorter than a minimum threshold, wait a brief extra capture window before finalizing.

Parameters:
- Minimum buffered duration: 50ms (800 samples at 16kHz)
- Maximum extra capture time: 60ms
- Poll interval: 10ms

This should be applied in the recording stop path, after the stop signal is set but before the audio streams are torn down. The chunker thread should continue draining audio during the grace period.

## Acceptance criteria

- [ ] When `stop_and_transcribe()` is called and accumulated 16kHz samples < 800, the function waits up to 60ms for more audio to arrive before proceeding
- [ ] Grace period does NOT apply when accumulated audio is already >= 50ms (normal recordings unaffected)
- [ ] Grace period respects the 60ms cap (never waits longer)
- [ ] Unit test: verify grace period triggers when buffer is short
- [ ] Manual test: quick-tap record key produces a valid (non-empty) transcription for a short word like "hey"

## Blocked by

None - can start immediately
