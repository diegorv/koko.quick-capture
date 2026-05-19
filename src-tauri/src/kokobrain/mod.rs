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

/// Build the `kokobrain://capture?vault=...&content=...&tags=...` URI
/// for the given Capture + Destination pair. See module docs for
/// payload mapping per kind.
pub fn build_capture_uri(
    capture: &Capture,
    destination: &Destination,
) -> Result<String, BuildError> {
    if destination.kind != DestinationKind::Kokobrain {
        return Err(BuildError::WrongDestinationKind);
    }
    let vault = parse_vault(destination.config.as_deref())?;
    let content = content_for_capture(capture)?;
    let tag = kebab_case(&destination.name);

    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("vault", &vault)
        .append_pair("content", &content)
        .append_pair("tags", &tag)
        .finish();
    Ok(format!("kokobrain://capture?{query}"))
}

/// Extract the vault string from a Destination's config blob. The
/// store layer already validates this on write, but we re-validate at
/// read time so a hand-edited DB does not silently emit a broken URI.
fn parse_vault(config: Option<&str>) -> Result<String, BuildError> {
    let raw = config.ok_or(BuildError::MissingConfig)?;
    let parsed: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| BuildError::InvalidConfig(e.to_string()))?;
    let vault = parsed
        .get("vault")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| BuildError::InvalidConfig("vault must be a non-blank string".into()))?;
    Ok(vault.to_string())
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
