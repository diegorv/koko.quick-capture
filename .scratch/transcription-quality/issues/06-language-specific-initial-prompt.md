Status: done

# Add language-seeded initial prompt

## Problem

Whisper sometimes auto-detects the wrong language or starts
transcribing in a different script/encoding on the first chunk.
A language-specific seed prompt biases the decoder toward the correct
language from the start.

App only supports PT and EN.

## Solution

Prepend a short language-appropriate greeting as initial prompt when
no rolling prompt from a previous chunk exists (i.e., first chunk only).

PT: "Olá, como você está? Prazer em conhecê-lo."
EN: "Hello, how are you doing? Nice to meet you."

This matches VoiceInk's `WhisperPrompt` pattern - a natural sentence
in the target language that primes the tokenizer and language model.

The rolling initial_prompt (already implemented) takes over from chunk 2
onward, so this only affects the first chunk.

## Where

- Edit: `src-tauri/src/transcription/mod.rs` (add seed prompt logic)
- Edit: `src-tauri/src/recording/mod.rs` (pass seed prompt when no rolling prompt)

## Verify

- Unit test: seed prompt selected by language code
- Manual test: short PT recording correctly identified as PT from start
- Manual test: short EN recording correctly identified as EN from start
