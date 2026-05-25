use anyhow::Result;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const MIN_AUDIO_SAMPLES_16KHZ: usize = 16_000;
const NO_SPEECH_SKIP_THOLD: f32 = 0.7;
pub const DEFAULT_LANGUAGE: &str = "pt";

pub fn language_seed_prompt(language: &str) -> Option<&'static str> {
    match language {
        "pt" => Some("Olá, tudo bem? Vamos conversar sobre o que aconteceu hoje. Então, o que você acha disso?"),
        "en" => Some("Hello, how are you doing? Let's talk about what happened today. So, what do you think?"),
        _ => None,
    }
}

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

pub fn warmup(ctx: &WhisperContext) {
    let silence = vec![0.0f32; 16_000];
    let _ = transcribe(ctx, &silence);
    eprintln!("[transcription] warmup complete");
}

pub fn transcribe(ctx: &WhisperContext, audio_data: &[f32]) -> Result<String> {
    transcribe_with_language(ctx, audio_data, DEFAULT_LANGUAGE, None)
}

pub fn transcribe_with_language(
    ctx: &WhisperContext,
    audio_data: &[f32],
    language: &str,
    initial_prompt: Option<&str>,
) -> Result<String> {
    transcribe_full(ctx, audio_data, language, initial_prompt, None)
}

pub fn transcribe_full(
    ctx: &WhisperContext,
    audio_data: &[f32],
    language: &str,
    initial_prompt: Option<&str>,
    abort_flag: Option<Arc<AtomicBool>>,
) -> Result<String> {
    let mut state = ctx
        .create_state()
        .map_err(|e| anyhow::anyhow!("Failed to create whisper state: {}", e))?;

    let mut params = FullParams::new(SamplingStrategy::BeamSearch { beam_size: 3, patience: -1.0 });

    let audio = pad_audio(audio_data);

    let cpus = std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4);
    params.set_n_threads(cpus.saturating_sub(2).clamp(1, 8));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_token_timestamps(true);
    params.set_max_len(200);
    params.set_language(Some(language));
    params.set_translate(false);
    params.set_temperature(0.0);
    params.set_temperature_inc(0.2);
    params.set_no_speech_thold(0.55);
    params.set_entropy_thold(2.4);
    params.set_logprob_thold(-1.0);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    params.set_max_initial_ts(1.0);

    let effective_prompt = initial_prompt
        .filter(|p| !p.is_empty())
        .or_else(|| language_seed_prompt(language));
    if let Some(prompt) = effective_prompt {
        params.set_initial_prompt(prompt);
    }

    if let Some(flag) = abort_flag {
        params.set_abort_callback_safe(move || !flag.load(Ordering::Relaxed));
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
            let no_speech = segment.no_speech_probability();
            if no_speech > NO_SPEECH_SKIP_THOLD {
                continue;
            }
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
    // PT-BR
    "E aí pessoal,",
    "E aí pessoal!",
    "E aí,",
    "E aí!",
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
    // PT-BR
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
    // EN
    "Thank you for watching!",
    "Thank you for watching.",
    "Thank you for watching",
    "Thanks for watching!",
    "Thanks for watching.",
    "Thanks for watching",
    "Please like and subscribe!",
    "Please like and subscribe.",
    "Please like and subscribe",
    "Like and subscribe!",
    "Like and subscribe.",
    "Like and subscribe",
    "Please subscribe!",
    "Please subscribe.",
    "Please subscribe",
    "See you in the next video!",
    "See you in the next video.",
    "See you in the next video",
    "See you next time!",
    "See you next time.",
    "See you next time",
    "Subtitles by the Amara.org community",
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

    result = strip_filler_words(&result);

    result.trim().to_string()
}

const FILLER_WORDS: &[&str] = &[
    "uh", "uhh", "uhhh", "um", "uhm", "umm", "ummm",
    "hm", "hmm", "hmmm", "mm", "mmm", "mh",
    "ah", "ahh", "eh", "ehh", "er", "err",
];

fn strip_filler_words(text: &str) -> String {
    text.split_whitespace()
        .filter(|w| {
            let lower = w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
            !FILLER_WORDS.contains(&lower.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_bracketed_tokens(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '[' || ch == '(' || ch == '{' {
            let close = match ch { '[' => ']', '(' => ')', _ => '}' };
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

    let after_2 = collapse_phrase_repeats(&words, 2);
    let after_3 = collapse_phrase_repeats(&after_2, 3);

    let mut final_out = vec![after_3[0].clone()];
    for w in &after_3[1..] {
        if !w.eq_ignore_ascii_case(final_out.last().unwrap()) {
            final_out.push(w.clone());
        }
    }
    final_out.join(" ")
}

fn collapse_phrase_repeats(words: &[impl AsRef<str>], phrase_len: usize) -> Vec<String> {
    if words.len() < phrase_len * 2 {
        return words.iter().map(|w| w.as_ref().to_string()).collect();
    }
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if i + phrase_len * 2 <= words.len() {
            let phrase = &words[i..i + phrase_len];
            let next = &words[i + phrase_len..i + phrase_len * 2];
            let matches = phrase.iter().zip(next.iter())
                .all(|(a, b)| a.as_ref().eq_ignore_ascii_case(b.as_ref()));
            if matches {
                for w in phrase { out.push(w.as_ref().to_string()); }
                i += phrase_len;
                while i + phrase_len <= words.len() {
                    let candidate = &words[i..i + phrase_len];
                    let still_matches = phrase.iter().zip(candidate.iter())
                        .all(|(a, b)| a.as_ref().eq_ignore_ascii_case(b.as_ref()));
                    if still_matches {
                        i += phrase_len;
                    } else {
                        break;
                    }
                }
                continue;
            }
        }
        out.push(words[i].as_ref().to_string());
        i += 1;
    }
    out
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

    #[test]
    fn seed_prompt_pt_returns_portuguese() {
        assert!(language_seed_prompt("pt").unwrap().contains("Olá"));
    }

    #[test]
    fn seed_prompt_en_returns_english() {
        assert!(language_seed_prompt("en").unwrap().contains("Hello"));
    }

    #[test]
    fn seed_prompt_unknown_returns_none() {
        assert!(language_seed_prompt("de").is_none());
    }

    #[test]
    fn strip_en_suffix_thank_you() {
        let result = strip_hallucination_artifacts("Real content here. Thank you for watching!");
        assert_eq!(result, "Real content here.");
    }

    #[test]
    fn strip_en_suffix_subscribe() {
        let result = strip_hallucination_artifacts("Some text. Like and subscribe!");
        assert_eq!(result, "Some text.");
    }

    // --- hallucination detection edge cases ---

    #[test]
    fn hallucination_three_word_repeat() {
        assert!(is_hallucination(
            "E aí pessoal E aí pessoal E aí pessoal E aí pessoal"
        ));
    }

    #[test]
    fn hallucination_four_word_repeat() {
        assert!(is_hallucination(
            "a b c d a b c d a b c d a b c d"
        ));
    }

    #[test]
    fn hallucination_empty_is_not_hallucination() {
        assert!(!is_hallucination(""));
    }

    #[test]
    fn hallucination_three_words_is_not_hallucination() {
        assert!(!is_hallucination("one two three"));
    }

    #[test]
    fn hallucination_partial_repeat_below_threshold() {
        assert!(!is_hallucination(
            "hello hello hello world foo bar baz qux"
        ));
    }

    // --- collapse_repeated_words edge cases ---

    #[test]
    fn collapse_case_insensitive() {
        assert_eq!(collapse_repeated_words("The the THE dog"), "The dog");
    }

    #[test]
    fn collapse_no_repeats() {
        assert_eq!(collapse_repeated_words("all different words"), "all different words");
    }

    #[test]
    fn collapse_empty() {
        assert_eq!(collapse_repeated_words(""), "");
    }

    // --- strip_bracketed_tokens edge cases ---

    #[test]
    fn strip_multiple_bracketed_tokens() {
        assert_eq!(
            strip_bracketed_tokens("[MUSIC] hello [BLANK_AUDIO] world (applause)"),
            " hello  world "
        );
    }

    #[test]
    fn strip_nested_brackets_treated_as_single() {
        assert_eq!(strip_bracketed_tokens("[a [b] c]"), " c]");
    }

    #[test]
    fn strip_empty_brackets() {
        assert_eq!(strip_bracketed_tokens("[]()text"), "text");
    }

    // --- strip_hallucination_artifacts combined ---

    #[test]
    fn strip_artifacts_all_removed_returns_empty() {
        let result = strip_hallucination_artifacts("Fala pessoal, Obrigado por assistir!");
        assert_eq!(result, "");
    }

    #[test]
    fn strip_artifacts_bracketed_plus_prefix() {
        let result = strip_hallucination_artifacts("E aí, [MUSIC] real content here");
        assert_eq!(result, "real content here");
    }

    #[test]
    fn strip_artifacts_repeated_words_collapsed() {
        let result = strip_hallucination_artifacts("hello hello hello world world");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn strip_pt_suffix_ate_a_proxima() {
        let result = strip_hallucination_artifacts("Conteúdo real. Até a próxima!");
        assert_eq!(result, "Conteúdo real.");
    }

    #[test]
    fn strip_pt_prefix_fala_galera() {
        let result = strip_hallucination_artifacts("Fala galera, conteúdo real aqui");
        assert_eq!(result, "conteúdo real aqui");
    }

    #[test]
    fn strip_en_see_you_next_time() {
        let result = strip_hallucination_artifacts("Good stuff. See you next time!");
        assert_eq!(result, "Good stuff.");
    }

    #[test]
    fn strip_legendas_amara() {
        let result = strip_hallucination_artifacts("Texto. Legendas pela comunidade Amara.org");
        assert_eq!(result, "Texto.");
    }

    #[test]
    fn collapse_two_word_phrase_repeat() {
        assert_eq!(collapse_repeated_words("foo bar foo bar"), "foo bar");
        assert_eq!(collapse_repeated_words("foo bar foo bar foo bar"), "foo bar");
    }

    #[test]
    fn collapse_three_word_phrase_repeat() {
        assert_eq!(collapse_repeated_words("a b c a b c a b c"), "a b c");
    }

    #[test]
    fn collapse_phrase_with_trailing() {
        assert_eq!(collapse_repeated_words("foo bar foo bar baz"), "foo bar baz");
    }

    #[test]
    fn collapse_mixed_single_and_phrase() {
        assert_eq!(
            collapse_repeated_words("hello hello foo bar foo bar world"),
            "hello foo bar world"
        );
    }

    #[test]
    fn strip_curly_braces() {
        assert_eq!(strip_bracketed_tokens("{inaudible} hello"), " hello");
        assert_eq!(strip_bracketed_tokens("text {noise}"), "text ");
    }

    #[test]
    fn strip_curly_preserves_unclosed() {
        assert_eq!(strip_bracketed_tokens("hello {unclosed"), "hello {unclosed");
    }

    #[test]
    fn filler_words_removed() {
        assert_eq!(strip_filler_words("uh hello um world"), "hello world");
        assert_eq!(strip_filler_words("hmm let me think ah yes"), "let me think yes");
    }

    #[test]
    fn filler_words_case_insensitive_with_punctuation() {
        assert_eq!(strip_filler_words("Uh, well then"), "well then");
        assert_eq!(strip_filler_words("right umm... okay"), "right okay");
    }

    #[test]
    fn filler_words_preserves_normal_text() {
        assert_eq!(strip_filler_words("this is normal text"), "this is normal text");
    }

    #[test]
    fn strip_artifacts_with_fillers() {
        let result = strip_hallucination_artifacts("uh hello um world ah yes");
        assert_eq!(result, "hello world yes");
    }
}
