Status: ready-for-agent

# Detect corrupted model files

## Problem

If model download is interrupted after the tmp->final rename (e.g.,
disk full during write, killed process), the model file exists but is
corrupt. `WhisperContext::new_with_params` will fail with an opaque
error. No way to recover without manually deleting the file.

## Solution

Validate model file before loading:
1. Check file size >= expected minimum (e.g., 500MB for large-v3-turbo-q5_0)
2. Check GGML magic header bytes (first 4 bytes: 0x67676d6c or GGUF magic)
3. If invalid, delete and re-download

Same for VAD model (expected ~864KB).

Reference: meetily validates GGML/GGUF magic + file size >= 90% expected.

## Where

- Edit: `src-tauri/src/recording/mod.rs` (add validation before load)
- Edit: `src-tauri/src/commands/recording.rs` (handle corrupt state)

## Verify

- Truncate model file to 1KB, app detects corruption and re-downloads
- Valid model file passes validation
