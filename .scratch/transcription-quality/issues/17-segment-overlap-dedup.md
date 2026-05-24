Status: ready-for-agent

# 17 - Segment overlap dedup via longest-common-substring

## What to build

When VAD segments are transcribed consecutively, the tail of one segment and the head of the next can produce overlapping text (Whisper uses context from the previous chunk, which can cause repeated phrases at boundaries). Implement a dedup pass that detects and removes the longest common word substring between consecutive transcript chunks before merging them.

The algorithm: given the previous chunk's text and the current chunk's text, find the longest contiguous sequence of words that appears in both (case-insensitive, punctuation-stripped). If found, trim the previous chunk at the overlap start and the current chunk after the overlap end, then join.

Wire this into the transcript accumulation step so that `ChunkedTranscript::merged()` produces clean, non-repetitive output.

## Acceptance criteria

- [ ] A `longest_common_word_substring(prev: &str, curr: &str) -> Option<(usize, usize)>` function exists in the transcription module, returning word-index positions of the overlap in each string
- [ ] `ChunkedTranscript` applies overlap removal when accumulating chunks (not just at final merge)
- [ ] Unit tests cover: no overlap, partial overlap, full overlap, case-insensitive match, punctuation-insensitive match, single-word overlap (should be ignored to avoid false positives - minimum 2 words)
- [ ] Existing transcription tests still pass
- [ ] Manual test: record a 30+ second utterance and verify no repeated phrases at chunk boundaries

## Blocked by

None - can start immediately
