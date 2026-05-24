Status: done

# Bluetooth-adaptive buffering in mixer

## What to build

Bluetooth audio devices deliver samples in irregular bursts with
higher jitter than wired devices (80-200ms vs 20-50ms). The mixer
uses a fixed 600ms window which works but isn't optimal - BT
devices may need larger windows to avoid gaps, while wired devices
could use smaller windows for lower latency.

Detect device type (wired vs Bluetooth) and adjust mixer window
and max buffer accordingly. On macOS, device name or transport
type can indicate Bluetooth.

Reference: meetily's InputDeviceKind with adaptive timeout
(ffmpeg_mixer.rs lines 42-48: wired 20-50ms, BT 80-200ms).

## Acceptance criteria

- [ ] Device type detection (Bluetooth vs wired) from cpal device info
- [ ] Mixer window size adjusted per device type
- [ ] Bluetooth: larger max buffer to absorb jitter
- [ ] Wired: default window (or smaller for lower latency)
- [ ] Fallback to default window if device type unknown
- [ ] No regression for standard wired mic recordings

## Blocked by

- 12-mixer-cross-rate-resampling (mixer needs rate-aware construction before adding device-aware params)
