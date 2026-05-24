Status: done

# Remove unnecessary unsafe impl Send for RecordingHandle

## What to build

`recording/mod.rs` line 110 has `unsafe impl Send for RecordingHandle {}`. All fields of RecordingHandle (Arc, AtomicBool, AtomicU32, bool, Instant, u32, String, UnboundedReceiver, Mutex, JoinHandle) auto-derive Send. The unsafe impl is a dead artifact that masks the compiler's Send analysis - if a non-Send field is added in the future, the compiler won't catch it.

Remove the line. If it doesn't compile without it, investigate which field isn't Send and fix that instead.

## Acceptance criteria

- [ ] `unsafe impl Send for RecordingHandle {}` removed
- [ ] Code compiles without it (proving all fields are Send)
- [ ] No other changes needed

## Blocked by

None - can start immediately
