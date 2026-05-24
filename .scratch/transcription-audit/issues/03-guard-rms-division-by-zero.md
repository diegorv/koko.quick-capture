Status: done

# Guard against division-by-zero in RMS calculation

## What to build

Two locations compute RMS (root mean square) of resampled audio by dividing by `resampled.len()`:

1. `process_chunk` function (recording/mod.rs, RMS silence check)
2. `stop_and_transcribe` method (recording/mod.rs, remaining samples handling)

If `resample_to_16khz` ever returns an empty vector for non-empty input (edge case in the resampling library, or future refactor changes caller assumptions), this panics with division by zero.

Add an early-return guard (`if resampled.is_empty() { return; }`) before the RMS calculation in both locations.

## Acceptance criteria

- [ ] `process_chunk` returns early if resampled is empty (after extending all_samples_16k - existing behavior)
- [ ] `stop_and_transcribe` skips RMS check if resampled is empty
- [ ] No behavioral change for normal (non-empty) audio paths
- [ ] Build passes

## Blocked by

None - can start immediately
