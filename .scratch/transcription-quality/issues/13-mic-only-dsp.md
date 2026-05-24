Status: done

# Mic-only DSP with system audio passthrough

## What to build

Currently DSP (high-pass filter, RNNoise denoise, EBU R128
normalize) runs on the mixed mic+system signal. System audio is
clean digital - running RNNoise on it can introduce artifacts, and
the 80Hz high-pass strips bass from music/system sounds.

Restructure the pipeline so DSP runs on mic audio only, before
mixing. System audio passes through the mixer raw. The mixed output
then goes to the VAD and whisper as today.

New flow:
```
mic raw -> DSP (hp, denoise, normalize) -> mixer.push_mic()
system raw -> mixer.push_system() (no DSP)
mixer.extract_mixed() -> resample 16k -> VAD -> whisper
```

This requires moving the DSP processors out of the chunker loop
and into a pre-mixer stage, or having the mixer accept pre-processed
mic samples.

Reference: meetily applies enhancement only to mic audio
(pipeline.rs lines 270-313), system audio is raw passthrough.

## Acceptance criteria

- [ ] Mic audio passes through hp filter, denoiser, normalizer before mixing
- [ ] System audio enters mixer without any DSP processing
- [ ] Mixed output still resampled to 16kHz for VAD + whisper
- [ ] Mic-only recordings (no system audio) work identically to before
- [ ] No regression in existing VAD + transcription tests

## Blocked by

- 12-mixer-cross-rate-resampling (mixer needs rate-aware push before restructuring DSP flow)
