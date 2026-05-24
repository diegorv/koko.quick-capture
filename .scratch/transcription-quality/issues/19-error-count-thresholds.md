Status: ready-for-agent

# 19 - Error count thresholds for auto-stop recording

## What to build

Track error counts during recording and auto-stop when thresholds are exceeded, preventing infinite error loops that degrade UX and waste resources.

Two categories:
- **Recoverable errors**: device stream hiccups, transient transcription failures, buffer overflows. Threshold: 10.
- **Total errors** (recoverable + non-recoverable): Threshold: 15.
- **Non-recoverable errors** (device disconnected, permission denied): immediate stop on first occurrence.

Add atomic counters to `RecordingHandle` for recoverable and total error counts. Increment in `transcribe_segment()` on failure and in the chunker loop on DSP/mixer errors. Check thresholds after each increment; if exceeded, set `is_recording = false` and log a summary.

Emit a frontend event when auto-stop triggers so the UI can show a user-facing message.

## Acceptance criteria

- [ ] `RecordingHandle` has `error_count_recoverable: Arc<AtomicU32>` and `error_count_total: Arc<AtomicU32>`
- [ ] Recoverable errors (transcription failure, DSP error, mixer error) increment both counters
- [ ] Recording auto-stops when recoverable count >= 10 or total count >= 15
- [ ] Non-recoverable errors (if any are identified) trigger immediate stop
- [ ] Error counts are exposed via `get_recording_status()` for frontend visibility
- [ ] Log message on auto-stop includes error summary (count by type)
- [ ] Existing tests pass

## Blocked by

None - can start immediately
