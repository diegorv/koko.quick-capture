Status: ready-for-agent

# Add English hallucination patterns to filter

## Problem

Current hallucination filter only covers PT-BR YouTube phrases. When
language is set to English, common whisper hallucination phrases pass
through: "Thank you for watching", "Please subscribe", "Like and
subscribe", "Thanks for watching", etc.

App targets PT and EN only, so both languages need coverage.

## Solution

Add English hallucination prefixes/suffixes alongside the existing
PT-BR ones. Gate by language parameter so PT patterns only match PT
transcriptions and EN patterns only match EN.

Known EN hallucination phrases (from meetily + VoiceInk):
- "Thank you for watching"
- "Thanks for watching"
- "Please like and subscribe"
- "Like and subscribe"
- "Please subscribe"
- "See you in the next video"
- "See you next time"
- "Subtitles by the Amara.org community"

## Where

- Edit: `src-tauri/src/transcription/mod.rs`

## Verify

- Add unit tests for EN patterns
- Existing PT-BR tests still pass
- `cargo test` passes
