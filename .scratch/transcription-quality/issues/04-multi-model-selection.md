Status: ready-for-agent

# Support multiple whisper model sizes

## Problem

Hardcoded to `ggml-large-v3-turbo-q5_0.bin` (547MB). Users with less
RAM or wanting faster response can't switch to smaller models. Users
wanting maximum accuracy on short clips can't switch to unquantized
large-v3-turbo or full large-v3.

## Solution

Add a model catalog with recommended models for PT/EN transcription:

| Model | Size | RAM | Speed | Quality | When |
|-------|------|-----|-------|---------|------|
| large-v3-turbo-q5_0 | 547MB | ~2GB | Fast | Good | Default |
| large-v3-turbo | 1.6GB | ~3GB | Medium | Better | High quality |
| medium | 1.5GB | ~2GB | Medium | Good | Fallback |
| small | 466MB | ~1GB | Fast | OK | Low RAM |

Frontend model selector in settings. Download on demand. Store all in
same models_dir(). Remember last-used model in store/settings.

## Where

- Edit: `src-tauri/src/recording/mod.rs` (model catalog, download_model takes model_id)
- Edit: `src-tauri/src/commands/recording.rs` (model selection commands)
- Edit: frontend settings component

## Verify

- Can download and switch between models
- Transcription works with each model
- Previous model selection persists across app restart
