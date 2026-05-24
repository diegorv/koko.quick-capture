use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperVadParams};

const MIN_AUDIO_SAMPLES_16KHZ: usize = 16_000;
pub const DEFAULT_LANGUAGE: &str = "pt";

pub fn create_whisper_context(model_path: &Path) -> Result<Arc<WhisperContext>> {
    let mut ctx_params = WhisperContextParameters::default();
    ctx_params.use_gpu(true);
    ctx_params.flash_attn(true);

    let ctx = WhisperContext::new_with_params(
        model_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid model path"))?,
        ctx_params,
    )
    .map_err(|e| anyhow::anyhow!("Failed to load whisper model: {}", e))?;

    Ok(Arc::new(ctx))
}

const TRAILING_SILENCE_SAMPLES: usize = 16_000;

fn pad_audio(samples: &[f32]) -> Vec<f32> {
    let min_len = samples.len().max(MIN_AUDIO_SAMPLES_16KHZ) + TRAILING_SILENCE_SAMPLES;
    let mut padded = samples.to_vec();
    padded.resize(min_len, 0.0);
    padded
}

pub fn transcribe(ctx: &WhisperContext, audio_data: &[f32]) -> Result<String> {
    transcribe_with_language(ctx, audio_data, DEFAULT_LANGUAGE, None, None)
}

pub fn transcribe_with_language(
    ctx: &WhisperContext,
    audio_data: &[f32],
    language: &str,
    initial_prompt: Option<&str>,
    vad_model_path: Option<&str>,
) -> Result<String> {
    let mut state = ctx
        .create_state()
        .map_err(|e| anyhow::anyhow!("Failed to create whisper state: {}", e))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

    let audio = pad_audio(audio_data);

    let cpus = std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4);
    params.set_n_threads(cpus.saturating_sub(2).clamp(1, 8));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_token_timestamps(false);
    params.set_language(Some(language));
    params.set_translate(false);
    params.set_temperature(0.0);
    params.set_temperature_inc(0.2);
    params.set_no_speech_thold(0.55);
    params.set_entropy_thold(2.4);
    params.set_logprob_thold(-1.0);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);

    if let Some(prompt) = initial_prompt {
        if !prompt.is_empty() {
            params.set_initial_prompt(prompt);
        }
    }

    if let Some(vad_path) = vad_model_path {
        params.set_vad_model_path(Some(vad_path));
        let mut vad_params = WhisperVadParams::new();
        vad_params.set_threshold(0.5);
        vad_params.set_min_speech_duration(250);
        vad_params.set_min_silence_duration(100);
        vad_params.set_speech_pad(30);
        vad_params.set_samples_overlap(0.1);
        params.set_vad_params(vad_params);
        params.enable_vad(true);
    }

    let audio_ctx = ((1500 * audio.len()) / (16_000 * 30) + 128).min(1500) as i32;
    params.set_audio_ctx(audio_ctx);

    state
        .full(params, &audio)
        .map_err(|e| anyhow::anyhow!("Whisper transcription failed: {}", e))?;

    let num_segments = state.full_n_segments();

    let mut transcript = String::new();
    for i in 0..num_segments {
        if let Some(segment) = state.get_segment(i) {
            if let Ok(text) = segment.to_str_lossy() {
                transcript.push_str(&text);
            }
        }
    }

    let trimmed = transcript.trim().to_string();

    if is_hallucination(&trimmed) {
        return Ok(String::new());
    }

    let cleaned = strip_hallucination_artifacts(&trimmed);
    if cleaned.is_empty() {
        return Ok(String::new());
    }

    Ok(cleaned)
}

const HALLUCINATION_PREFIXES: &[&str] = &[
    "E aí,",
    "E aí!",
    "E aí pessoal,",
    "E aí pessoal!",
    "E aí.",
    "E aí",
    "Fala pessoal,",
    "Fala pessoal!",
    "Fala pessoal",
    "Fala galera,",
    "Fala galera!",
    "Fala galera",
];

const HALLUCINATION_SUFFIXES: &[&str] = &[
    "Obrigado por assistir!",
    "Obrigado por assistir.",
    "Obrigado por assistir",
    "Até a próxima!",
    "Até a próxima.",
    "Até a próxima",
    "Até mais!",
    "Até mais.",
    "Até mais",
    "Legendas pela comunidade Amara.org",
    "Inscreva-se no canal!",
    "Inscreva-se no canal.",
    "Inscreva-se no canal",
];

fn strip_hallucination_artifacts(text: &str) -> String {
    let mut result = text.to_string();

    // Strip leading '!' (common whisper artifact, ref dsnote)
    if let Some(rest) = result.strip_prefix('!') {
        result = rest.trim_start().to_string();
    }

    for prefix in HALLUCINATION_PREFIXES {
        if let Some(rest) = result.strip_prefix(prefix) {
            result = rest.trim().to_string();
            break;
        }
    }

    for suffix in HALLUCINATION_SUFFIXES {
        if let Some(rest) = result.strip_suffix(suffix) {
            result = rest.trim().to_string();
            break;
        }
    }

    // Strip bracketed hallucination tokens: [MUSIC], [BLANK_AUDIO], (music), etc.
    result = strip_bracketed_tokens(&result);

    // Collapse 2+ consecutive identical words to one
    result = collapse_repeated_words(&result);

    result.trim().to_string()
}

fn strip_bracketed_tokens(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '[' || ch == '(' {
            let close = if ch == '[' { ']' } else { ')' };
            let mut inside = String::new();
            let mut found_close = false;
            for inner in chars.by_ref() {
                if inner == close {
                    found_close = true;
                    break;
                }
                inside.push(inner);
            }
            if !found_close {
                result.push(ch);
                result.push_str(&inside);
            }
            // else: drop the bracketed content
        } else {
            result.push(ch);
        }
    }
    result
}

fn collapse_repeated_words(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 2 {
        return text.to_string();
    }
    let mut out = vec![words[0]];
    for w in &words[1..] {
        if !w.eq_ignore_ascii_case(out.last().unwrap()) {
            out.push(w);
        }
    }
    out.join(" ")
}

fn is_hallucination(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 4 {
        return false;
    }

    for pattern_len in 1..=4 {
        if words.len() < pattern_len * 2 {
            continue;
        }
        let pattern = &words[..pattern_len];
        let repetitions = words.chunks(pattern_len).filter(|chunk| *chunk == pattern).count();
        let coverage = repetitions * pattern_len;
        if coverage * 100 / words.len() >= 80 {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hallucination_single_word_repeat() {
        assert!(is_hallucination(
            "Obrigado Obrigado Obrigado Obrigado Obrigado"
        ));
    }

    #[test]
    fn test_hallucination_two_word_repeat() {
        assert!(is_hallucination("E aí E aí E aí E aí E aí"));
    }

    #[test]
    fn test_not_hallucination_normal_text() {
        assert!(!is_hallucination(
            "Olá, como vai você? Tudo bem por aqui. Vamos começar a reunião."
        ));
    }

    #[test]
    fn test_not_hallucination_short() {
        assert!(!is_hallucination("Sim"));
        assert!(!is_hallucination("Olá pessoal"));
    }

    #[test]
    fn test_strip_both_prefix_and_suffix() {
        let result = strip_hallucination_artifacts(
            "E aí, conteúdo real aqui. Obrigado por assistir!"
        );
        assert_eq!(result, "conteúdo real aqui.");
    }

    #[test]
    fn test_strip_no_artifacts() {
        let result = strip_hallucination_artifacts("Uma frase normal sem artefatos.");
        assert_eq!(result, "Uma frase normal sem artefatos.");
    }

    #[test]
    fn pad_audio_adds_trailing_silence_and_meets_minimum() {
        let input = vec![0.5f32, -0.5, 0.25];
        let padded = pad_audio(&input);
        assert_eq!(padded.len(), MIN_AUDIO_SAMPLES_16KHZ + TRAILING_SILENCE_SAMPLES);
        assert_eq!(&padded[..3], &input[..]);
        assert!(padded[3..].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn pad_audio_appends_silence_to_long_buffer() {
        let input = vec![0.1f32; MIN_AUDIO_SAMPLES_16KHZ + 100];
        let padded = pad_audio(&input);
        assert_eq!(padded.len(), input.len() + TRAILING_SILENCE_SAMPLES);
    }

    #[test]
    fn strip_bracketed_tokens_removes_music_and_blank() {
        assert_eq!(strip_bracketed_tokens("[MUSIC] hello [BLANK_AUDIO]"), " hello ");
        assert_eq!(strip_bracketed_tokens("(music) test"), " test");
    }

    #[test]
    fn strip_bracketed_tokens_preserves_unclosed() {
        assert_eq!(strip_bracketed_tokens("hello [unclosed"), "hello [unclosed");
    }

    #[test]
    fn collapse_repeated_words_deduplicates() {
        assert_eq!(collapse_repeated_words("the the the dog"), "the dog");
        assert_eq!(collapse_repeated_words("hello"), "hello");
    }

    #[test]
    fn strip_leading_exclamation() {
        let result = strip_hallucination_artifacts("! Some text here");
        assert_eq!(result, "Some text here");
    }
}
