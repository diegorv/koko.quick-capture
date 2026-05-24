Status: done

# Fix sys_active flag when system audio capture fails

## What to build

`RecordingHandle::sys_active` is set to `sys_device.is_some()` at construction time, before the audio thread attempts to start system audio capture. If `AudioCapture::start` fails for the system device (line 141-144 in recording/mod.rs), the error is logged but `sys_active` remains true. The frontend then shows a system audio VU meter for a channel that isn't actually capturing.

Fix so that `sys_active` reflects actual capture success. The audio thread already knows whether system audio started - communicate that result back (e.g. via the existing result channel, or a separate oneshot).

## Acceptance criteria

- [ ] When system audio capture fails, `sys_active` is false
- [ ] Frontend does not display system audio VU meter when capture failed
- [ ] When system audio capture succeeds, behavior unchanged (sys_active=true, VU meter shows)
- [ ] Log message still emitted on failure (existing behavior preserved)

## Blocked by

None - can start immediately
