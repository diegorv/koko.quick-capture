//! Extract `[[Name]]` wikilink mentions from capture payload text.
//!
//! Pure parser used by the store at save time + by the one-shot
//! backfill on store open. The Composer (ADR-0011) is the only path
//! that produces these tokens today; this module reads them back out
//! of the saved payload so the Inbox / Archive can filter captures
//! by person.

use std::collections::HashSet;

/// Extract unique `[[Name]]` mentions from `text` in first-occurrence
/// order. Trimmed; alias segments (`[[Name|alias]]`) and heading
/// anchors (`[[Name#heading]]`) are stripped to the base name. Empty
/// names are skipped. Dedupe is case-insensitive — the first form
/// encountered wins for the returned spelling.
pub fn extract_mentions(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            let rest = &text[i + 2..];
            if let Some(close_rel) = rest.find("]]") {
                let raw = &rest[..close_rel];
                let base = raw
                    .split('|')
                    .next()
                    .unwrap_or("")
                    .split('#')
                    .next()
                    .unwrap_or("")
                    .trim();
                if !base.is_empty() {
                    let key = base.to_lowercase();
                    if seen.insert(key) {
                        out.push(base.to_string());
                    }
                }
                i = i + 2 + close_rel + 2;
                continue;
            } else {
                break;
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_yields_no_mentions() {
        assert!(extract_mentions("").is_empty());
    }

    #[test]
    fn plain_text_with_no_brackets_yields_no_mentions() {
        assert!(extract_mentions("hello world").is_empty());
    }

    #[test]
    fn single_mention() {
        assert_eq!(extract_mentions("see [[Diego]]"), vec!["Diego"]);
    }

    #[test]
    fn multiple_mentions_in_order() {
        assert_eq!(
            extract_mentions("[[Ana]] then [[Diego]]"),
            vec!["Ana", "Diego"]
        );
    }

    #[test]
    fn dedupes_case_insensitively_keeping_first_form() {
        assert_eq!(
            extract_mentions("[[Diego]] and [[diego]] again"),
            vec!["Diego"]
        );
    }

    #[test]
    fn strips_alias_segment() {
        assert_eq!(extract_mentions("[[Diego|d]]"), vec!["Diego"]);
    }

    #[test]
    fn strips_heading_anchor() {
        assert_eq!(extract_mentions("[[Diego#notes]]"), vec!["Diego"]);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(extract_mentions("[[  Diego  ]]"), vec!["Diego"]);
    }

    #[test]
    fn ignores_unclosed_brackets() {
        assert!(extract_mentions("[[orphan").is_empty());
    }

    #[test]
    fn ignores_empty_brackets() {
        assert!(extract_mentions("hello [[]] world").is_empty());
    }

    #[test]
    fn names_with_spaces_preserved() {
        assert_eq!(extract_mentions("[[Ana Beatriz]]"), vec!["Ana Beatriz"]);
    }

    #[test]
    fn surrounding_text_does_not_leak_into_name() {
        assert_eq!(
            extract_mentions("prefix [[Diego]] suffix"),
            vec!["Diego"]
        );
    }
}
