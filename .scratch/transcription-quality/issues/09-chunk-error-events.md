Status: ready-for-agent

# Surface chunk transcription errors to frontend

## Problem

When a chunk fails to transcribe, the error is logged to stderr and
silently dropped. The user gets an incomplete transcript with no
indication that something went wrong.

## Solution

Emit a Tauri event (`transcription-chunk-error`) when a chunk fails.
Frontend can display a warning indicator that some audio may not have
been transcribed.

Also count successful vs total chunks and report the ratio on stop.

## Where

- Edit: `src-tauri/src/recording/mod.rs` (emit event on chunk error)
- Edit: frontend recording status component (show warning)

## Verify

- Simulate a chunk error (e.g., pass empty audio)
- Frontend shows warning
- Transcript still contains successful chunks
