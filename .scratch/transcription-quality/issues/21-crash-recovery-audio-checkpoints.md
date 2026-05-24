Status: ready-for-agent

# 21 - Crash recovery via incremental audio checkpoints

## What to build

Periodically save audio data to checkpoint files during recording so that if the app crashes mid-recording, the user can recover their audio. This prevents data loss for long recordings.

Design:
- Checkpoint directory: `<app_data>/checkpoints/<session-id>/`
- Write a checkpoint WAV file every N seconds (e.g., every 30s) containing the accumulated 16kHz samples since last checkpoint
- On normal recording stop: delete checkpoint directory after successful save
- On app startup: check for orphaned checkpoint directories; if found, offer recovery
- Recovery: concatenate checkpoint WAVs in order, run final transcription on recovered audio
- Expose `recover_from_checkpoints` and `discard_checkpoints` Tauri commands

## Acceptance criteria

- [ ] During recording, checkpoint WAV files are written to a session-specific directory every 30 seconds
- [ ] Checkpoints contain sequential audio segments (no gaps, no overlaps)
- [ ] On successful `stop_and_transcribe()`, checkpoint directory is cleaned up
- [ ] On app startup, orphaned checkpoints are detected and a `has_recovery_data` command returns true
- [ ] `recover_from_checkpoints` command concatenates checkpoint audio and returns the recovered WAV path
- [ ] `discard_checkpoints` command removes orphaned checkpoint directories
- [ ] Checkpoint writes do not block the audio thread (use a dedicated writer thread or async task)
- [ ] Manual test: force-kill app during recording, relaunch, verify recovery works

## Blocked by

None - can start immediately
