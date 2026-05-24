Status: ready-for-agent

# Fix TOCTOU race in start_recording

## What to build

The `start_recording` Tauri command has a time-of-check-to-time-of-use race condition. The `rec_state` mutex is acquired to check if a recording is already active, then released. The lock is re-acquired much later to store the new handle. Between those two locks, a concurrent caller (e.g. user double-taps the shortcut) can also pass the check, resulting in two recordings starting and the first handle being silently overwritten (orphaned thread, leaked audio stream).

Fix by holding the `rec_state` lock from the initial check through handle assignment. The whisper model loading (which is async and potentially slow) should happen outside the lock, but the check-and-set must be atomic.

One approach: use a tri-state (Idle / Starting / Recording) so the lock can be released during model loading while still blocking concurrent callers.

## Acceptance criteria

- [ ] Two rapid concurrent calls to start_recording do not produce two active recordings
- [ ] The first recording is not orphaned (its audio thread is properly stopped)
- [ ] Whisper model loading does not hold the rec_state lock (avoids blocking stop_recording during model load)
- [ ] Existing tests and build pass

## Blocked by

None - can start immediately
