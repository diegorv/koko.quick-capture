//! Builds `kokobrain://capture` deep-link URIs for the kokobrain
//! integration (ADR-0012). Pure functions only — emission of the URI
//! through `tauri-plugin-opener` lives in the command layer.
//!
//! Per ADR-0012:
//! - quick-capture sends `vault`, `content`, and a single `tags` value
//!   (the destination name in kebab-case).
//! - `Note` / `Clip` send raw payload text, `Link` sends a markdown
//!   `[title](url)` line, and `Shot` / `File` are not routable to a
//!   kokobrain destination in v1.

use crate::store::{Capture, CaptureKind, Destination, DestinationKind};

/// Error variants surfaced when a Capture cannot be turned into a
/// `kokobrain://` URI. Each variant maps to a user-visible message in
/// the command layer; the picker uses [`BuildError::UnsupportedKind`]
/// to disable kokobrain destinations for `Shot`/`File` rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    /// Destination is not a kokobrain destination. Caller should route
    /// via the normal label path instead.
    WrongDestinationKind,
    /// Kokobrain destination is missing its config (vault is required).
    MissingConfig,
    /// Config JSON is malformed or the vault string is blank.
    InvalidConfig(String),
    /// The Capture's `kind` cannot be expressed as note content.
    UnsupportedKind(CaptureKind),
    /// The Capture's payload JSON is missing an expected field for the
    /// content mapping (e.g. `Link` without a `url`).
    MalformedPayload(&'static str),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::WrongDestinationKind => {
                write!(f, "destination is not a kokobrain destination")
            }
            BuildError::MissingConfig => {
                write!(f, "kokobrain destination is missing its config")
            }
            BuildError::InvalidConfig(msg) => write!(f, "invalid kokobrain config: {msg}"),
            BuildError::UnsupportedKind(k) => {
                write!(f, "{k:?} captures cannot be routed to a kokobrain destination")
            }
            BuildError::MalformedPayload(field) => {
                write!(f, "capture payload missing field: {field}")
            }
        }
    }
}

impl std::error::Error for BuildError {}

/// Parsed kokobrain destination config. Re-derived from the JSON blob
/// at URI-build time so a hand-edited DB row cannot smuggle in a
/// malformed `vault` or `tags` past the store layer's normalization.
struct KokobrainConfig {
    vault: String,
    /// Optional user-supplied tags. Stored as raw strings; the URI
    /// builder kebab-cases each one before emitting.
    tags: Vec<String>,
}

/// Build the `kokobrain://capture?vault=...&content=...&tags=...` URI
/// for the given Capture + Destination pair. See module docs for
/// payload mapping per kind.
///
/// The `tags` query parameter is the kebab-cased destination name
/// followed by every user-supplied tag from the destination config
/// (kebab-cased, deduplicated, original order preserved). When the
/// merged list is empty — which only happens if both the destination
/// name and every configured tag kebab-case to an empty string — the
/// `tags` parameter is omitted entirely.
pub fn build_capture_uri(
    capture: &Capture,
    destination: &Destination,
) -> Result<String, BuildError> {
    if destination.kind != DestinationKind::Kokobrain {
        return Err(BuildError::WrongDestinationKind);
    }
    let cfg = parse_kokobrain_config(destination.config.as_deref())?;
    let content = content_for_capture(capture)?;
    let tags = merge_tags(&destination.name, &cfg.tags);
    let title = title_for_capture(capture);

    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer
        .append_pair("vault", &cfg.vault)
        .append_pair("content", &content);
    if !tags.is_empty() {
        serializer.append_pair("tags", &tags.join(","));
    }
    if let Some(t) = title {
        serializer.append_pair("title", &t);
    }
    let query = serializer.finish();
    Ok(format!("kokobrain://capture?{query}"))
}

/// Extract `vault` + `tags` from a Destination's config blob. The
/// store layer already validates this on write, but we re-validate at
/// read time so a hand-edited DB does not silently emit a broken URI.
fn parse_kokobrain_config(config: Option<&str>) -> Result<KokobrainConfig, BuildError> {
    let raw = config.ok_or(BuildError::MissingConfig)?;
    let parsed: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| BuildError::InvalidConfig(e.to_string()))?;
    let vault = parsed
        .get("vault")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| BuildError::InvalidConfig("vault must be a non-blank string".into()))?
        .to_string();

    let tags = match parsed.get("tags") {
        None => Vec::new(),
        Some(v) => {
            let arr = v.as_array().ok_or_else(|| {
                BuildError::InvalidConfig("tags must be an array of strings".into())
            })?;
            let mut out = Vec::with_capacity(arr.len());
            for entry in arr {
                let s = entry.as_str().ok_or_else(|| {
                    BuildError::InvalidConfig("tags entries must be strings".into())
                })?;
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
            }
            out
        }
    };

    Ok(KokobrainConfig { vault, tags })
}

/// Merge the destination name (as kebab) with the user-supplied tags
/// (each kebab-cased). Destination name comes first; duplicates are
/// dropped while preserving the first-seen position. Empty kebab
/// results are skipped so an all-symbol destination name does not
/// emit a phantom empty tag.
fn merge_tags(destination_name: &str, user_tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(1 + user_tags.len());
    let dest_kebab = kebab_case(destination_name);
    if !dest_kebab.is_empty() {
        out.push(dest_kebab);
    }
    for raw in user_tags {
        let kebab = kebab_case(raw);
        if kebab.is_empty() {
            continue;
        }
        if !out.iter().any(|existing| existing == &kebab) {
            out.push(kebab);
        }
    }
    out
}

/// Best-effort structured title for the capture. Only `Link` captures
/// carry a meaningful standalone title; for every other kind the
/// "title" is just the content itself, so emitting it would be
/// redundant. Falls back through `source_title` -> `payload.title`;
/// returns `None` when neither is populated rather than sending the
/// raw URL as a title (the URL already appears in the markdown body
/// produced by `content_for_capture`).
fn title_for_capture(capture: &Capture) -> Option<String> {
    if capture.kind != CaptureKind::Link {
        return None;
    }
    if let Some(t) = capture
        .source_title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        return Some(t.to_string());
    }
    capture
        .payload
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// Map a Capture to the markdown body that should land in kokobrain.
fn content_for_capture(capture: &Capture) -> Result<String, BuildError> {
    match capture.kind {
        CaptureKind::Note | CaptureKind::Clip => {
            let text = capture
                .payload
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or(BuildError::MalformedPayload("text"))?;
            Ok(text.to_string())
        }
        CaptureKind::Link => {
            let url = capture
                .payload
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or(BuildError::MalformedPayload("url"))?;
            let title = capture
                .source_title
                .as_deref()
                .or_else(|| capture.payload.get("title").and_then(|v| v.as_str()))
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .unwrap_or(url);
            Ok(format!("[{title}]({url})"))
        }
        CaptureKind::Shot | CaptureKind::File => {
            Err(BuildError::UnsupportedKind(capture.kind))
        }
    }
}

/// Kebab-case a destination name for use as a single brain tag.
/// Lowercases ASCII, collapses runs of non-alphanumeric chars into a
/// single `-`, and trims leading/trailing dashes. Non-ASCII letters
/// are preserved as-is so accented destination names still survive
/// the round trip.
pub fn kebab_case(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_dash = true;
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note_capture(text: &str) -> Capture {
        Capture {
            id: "01H000000000000000000000NOTE".into(),
            kind: CaptureKind::Note,
            created_at: "2026-05-18T12:00:00Z".into(),
            payload: serde_json::json!({ "text": text }),
            source_app: None,
            starred: false,
            deleted_at: None,
            read_at: None,
            source_title: None,
            source_url: None,
            destination_id: None,
            routed_at: None,
        }
    }

    fn link_capture(url: &str, payload_title: Option<&str>, source_title: Option<&str>) -> Capture {
        let payload = match payload_title {
            Some(t) => serde_json::json!({ "url": url, "title": t, "raw_text": url }),
            None => serde_json::json!({ "url": url, "raw_text": url }),
        };
        Capture {
            id: "01H000000000000000000000LINK".into(),
            kind: CaptureKind::Link,
            created_at: "2026-05-18T12:00:00Z".into(),
            payload,
            source_app: None,
            starred: false,
            deleted_at: None,
            read_at: None,
            source_title: source_title.map(String::from),
            source_url: Some(url.to_string()),
            destination_id: None,
            routed_at: None,
        }
    }

    fn shot_capture() -> Capture {
        Capture {
            id: "01H000000000000000000000SHOT".into(),
            kind: CaptureKind::Shot,
            created_at: "2026-05-18T12:00:00Z".into(),
            payload: serde_json::json!({ "source_path": "/tmp/x.png", "mime": "image/png" }),
            source_app: None,
            starred: false,
            deleted_at: None,
            read_at: None,
            source_title: None,
            source_url: None,
            destination_id: None,
            routed_at: None,
        }
    }

    fn kokobrain_dest(name: &str, vault: &str) -> Destination {
        Destination {
            id: "01H000000000000000000000DEST".into(),
            name: name.into(),
            color: None,
            created_at: "2026-05-18T12:00:00Z".into(),
            deleted_at: None,
            kind: DestinationKind::Kokobrain,
            config: Some(format!(r#"{{"vault":"{vault}"}}"#)),
        }
    }

    fn kokobrain_dest_with_tags(name: &str, vault: &str, tags: &[&str]) -> Destination {
        let tags_json = serde_json::to_string(tags).expect("tags json");
        Destination {
            id: "01H000000000000000000000DEST".into(),
            name: name.into(),
            color: None,
            created_at: "2026-05-18T12:00:00Z".into(),
            deleted_at: None,
            kind: DestinationKind::Kokobrain,
            config: Some(format!(r#"{{"vault":"{vault}","tags":{tags_json}}}"#)),
        }
    }

    fn label_dest(name: &str) -> Destination {
        Destination {
            id: "01H000000000000000000000LBL".into(),
            name: name.into(),
            color: None,
            created_at: "2026-05-18T12:00:00Z".into(),
            deleted_at: None,
            kind: DestinationKind::Label,
            config: None,
        }
    }

    #[test]
    fn kebab_case_lowercases_and_collapses_separators() {
        assert_eq!(kebab_case("Reading List"), "reading-list");
        assert_eq!(kebab_case("  Reading   List  "), "reading-list");
        assert_eq!(kebab_case("Read/Write Notes!"), "read-write-notes");
        assert_eq!(kebab_case("kokobrain"), "kokobrain");
        assert_eq!(kebab_case("---"), "");
    }

    #[test]
    fn note_payload_becomes_raw_content() {
        let cap = note_capture("hello world");
        let dest = kokobrain_dest("Reading List", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        assert!(uri.starts_with("kokobrain://capture?"));
        assert!(uri.contains("vault=Personal"));
        assert!(uri.contains("content=hello+world"));
        assert!(uri.contains("tags=reading-list"));
    }

    #[test]
    fn link_uses_source_title_when_present() {
        let cap = link_capture(
            "https://example.com/post",
            Some("Payload Title"),
            Some("Window Title"),
        );
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        // source_title wins over payload.title; markdown link wrapper.
        let content_part = uri
            .split("&content=")
            .nth(1)
            .and_then(|s| s.split('&').next())
            .expect("content param");
        let decoded = url::form_urlencoded::parse(format!("content={content_part}").as_bytes())
            .find(|(k, _)| k == "content")
            .map(|(_, v)| v.to_string())
            .expect("decoded");
        assert_eq!(decoded, "[Window Title](https://example.com/post)");
    }

    #[test]
    fn link_falls_back_to_payload_title_then_url() {
        let cap = link_capture("https://example.com/post", Some("Payload Title"), None);
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs: Vec<(String, String)> =
            url::form_urlencoded::parse(uri.split('?').nth(1).unwrap().as_bytes())
                .into_owned()
                .collect();
        let content = pairs
            .iter()
            .find(|(k, _)| k == "content")
            .map(|(_, v)| v.clone())
            .unwrap();
        assert_eq!(content, "[Payload Title](https://example.com/post)");

        let cap = link_capture("https://example.com/post", None, None);
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs: Vec<(String, String)> =
            url::form_urlencoded::parse(uri.split('?').nth(1).unwrap().as_bytes())
                .into_owned()
                .collect();
        let content = pairs
            .iter()
            .find(|(k, _)| k == "content")
            .map(|(_, v)| v.clone())
            .unwrap();
        assert_eq!(content, "[https://example.com/post](https://example.com/post)");
    }

    #[test]
    fn shot_capture_rejected_with_unsupported_kind() {
        let cap = shot_capture();
        let dest = kokobrain_dest("Brain", "Personal");
        let err = build_capture_uri(&cap, &dest).expect_err("shot rejected");
        assert_eq!(err, BuildError::UnsupportedKind(CaptureKind::Shot));
    }

    #[test]
    fn label_destination_rejected() {
        let cap = note_capture("hi");
        let dest = label_dest("Todoist");
        let err = build_capture_uri(&cap, &dest).expect_err("label rejected");
        assert_eq!(err, BuildError::WrongDestinationKind);
    }

    #[test]
    fn missing_or_invalid_config_rejected() {
        let cap = note_capture("hi");
        let mut dest = kokobrain_dest("Brain", "Personal");
        dest.config = None;
        assert_eq!(
            build_capture_uri(&cap, &dest),
            Err(BuildError::MissingConfig)
        );

        dest.config = Some("not json".into());
        assert!(matches!(
            build_capture_uri(&cap, &dest),
            Err(BuildError::InvalidConfig(_))
        ));

        dest.config = Some(r#"{"vault":""}"#.into());
        assert!(matches!(
            build_capture_uri(&cap, &dest),
            Err(BuildError::InvalidConfig(_))
        ));
    }

    #[test]
    fn user_tags_appended_to_destination_name_tag() {
        let cap = note_capture("hello");
        let dest = kokobrain_dest_with_tags(
            "Reading List",
            "Personal",
            &["source/quick-capture", "Triage Inbox"],
        );
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs: Vec<(String, String)> =
            url::form_urlencoded::parse(uri.split('?').nth(1).unwrap().as_bytes())
                .into_owned()
                .collect();
        let tags = pairs
            .iter()
            .find(|(k, _)| k == "tags")
            .map(|(_, v)| v.clone())
            .expect("tags param present");
        assert_eq!(tags, "reading-list,source-quick-capture,triage-inbox");
    }

    #[test]
    fn duplicate_user_tag_matching_destination_name_is_dropped() {
        let cap = note_capture("hello");
        let dest = kokobrain_dest_with_tags(
            "Reading List",
            "Personal",
            &["Reading List", "extra"],
        );
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs: Vec<(String, String)> =
            url::form_urlencoded::parse(uri.split('?').nth(1).unwrap().as_bytes())
                .into_owned()
                .collect();
        let tags = pairs
            .iter()
            .find(|(k, _)| k == "tags")
            .map(|(_, v)| v.clone())
            .expect("tags param present");
        assert_eq!(tags, "reading-list,extra");
    }

    #[test]
    fn duplicate_user_tags_are_collapsed_preserving_first_position() {
        let cap = note_capture("hello");
        let dest = kokobrain_dest_with_tags(
            "Brain",
            "Personal",
            &["alpha", "beta", "alpha", "gamma", "BETA"],
        );
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs: Vec<(String, String)> =
            url::form_urlencoded::parse(uri.split('?').nth(1).unwrap().as_bytes())
                .into_owned()
                .collect();
        let tags = pairs
            .iter()
            .find(|(k, _)| k == "tags")
            .map(|(_, v)| v.clone())
            .expect("tags param present");
        // dest-name first, then unique user tags in input order; case
        // is folded to lowercase by kebab_case so BETA collapses with beta.
        assert_eq!(tags, "brain,alpha,beta,gamma");
    }

    #[test]
    fn destination_name_only_when_no_user_tags() {
        let cap = note_capture("hello");
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        assert!(uri.contains("tags=brain"));
    }

    #[test]
    fn empty_kebab_destination_name_falls_back_to_user_tags() {
        let cap = note_capture("hello");
        let dest = kokobrain_dest_with_tags("---", "Personal", &["fallback"]);
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs: Vec<(String, String)> =
            url::form_urlencoded::parse(uri.split('?').nth(1).unwrap().as_bytes())
                .into_owned()
                .collect();
        let tags = pairs
            .iter()
            .find(|(k, _)| k == "tags")
            .map(|(_, v)| v.clone())
            .expect("tags param present");
        assert_eq!(tags, "fallback");
    }

    #[test]
    fn tags_param_omitted_when_merged_list_is_empty() {
        let cap = note_capture("hello");
        let dest = kokobrain_dest_with_tags("---", "Personal", &["---", "  "]);
        let uri = build_capture_uri(&cap, &dest).expect("build");
        assert!(
            !uri.contains("tags="),
            "tags should be omitted when nothing kebab-cases to a non-empty value, got: {uri}"
        );
    }

    #[test]
    fn invalid_tags_field_rejected() {
        let cap = note_capture("hello");
        let mut dest = kokobrain_dest("Brain", "Personal");
        dest.config = Some(r#"{"vault":"Personal","tags":"not-an-array"}"#.into());
        assert!(matches!(
            build_capture_uri(&cap, &dest),
            Err(BuildError::InvalidConfig(_))
        ));

        dest.config = Some(r#"{"vault":"Personal","tags":["ok",42]}"#.into());
        assert!(matches!(
            build_capture_uri(&cap, &dest),
            Err(BuildError::InvalidConfig(_))
        ));
    }

    #[test]
    fn link_with_source_title_emits_title_param() {
        let cap = link_capture(
            "https://example.com/post",
            Some("Payload Title"),
            Some("Window Title"),
        );
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs: Vec<(String, String)> =
            url::form_urlencoded::parse(uri.split('?').nth(1).unwrap().as_bytes())
                .into_owned()
                .collect();
        let title = pairs
            .iter()
            .find(|(k, _)| k == "title")
            .map(|(_, v)| v.clone())
            .expect("title param present");
        assert_eq!(title, "Window Title");
    }

    #[test]
    fn link_without_source_title_falls_back_to_payload_title() {
        let cap = link_capture("https://example.com/post", Some("Payload Title"), None);
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs: Vec<(String, String)> =
            url::form_urlencoded::parse(uri.split('?').nth(1).unwrap().as_bytes())
                .into_owned()
                .collect();
        let title = pairs
            .iter()
            .find(|(k, _)| k == "title")
            .map(|(_, v)| v.clone())
            .expect("title param present");
        assert_eq!(title, "Payload Title");
    }

    #[test]
    fn link_without_any_title_omits_title_param() {
        let cap = link_capture("https://example.com/post", None, None);
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        assert!(
            !uri.contains("title="),
            "title param should be omitted when no human-readable title exists, got: {uri}"
        );
    }

    #[test]
    fn note_capture_never_emits_title_param() {
        let cap = note_capture("hello world");
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        assert!(
            !uri.contains("title="),
            "title is link-only; note captures should not emit it, got: {uri}"
        );
    }

    #[test]
    fn special_chars_in_content_are_percent_encoded() {
        let cap = note_capture("a & b = c #tag");
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        // The literal `&`, `=`, `#` must not survive in the query
        // (they would break parsing on the brain side).
        let query = uri.split('?').nth(1).unwrap();
        let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        let content = pairs
            .iter()
            .find(|(k, _)| k == "content")
            .map(|(_, v)| v.clone())
            .unwrap();
        assert_eq!(content, "a & b = c #tag");
    }
}
