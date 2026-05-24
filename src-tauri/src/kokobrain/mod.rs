//! Builds `kokobrain://capture` deep-link URIs for the kokobrain
//! integration (ADR-0012). Pure functions only — emission of the URI
//! through `tauri-plugin-opener` lives in the command layer.
//!
//! Schema v2 (current). The brain side owns markdown rendering; this
//! module emits each capture field as its own query parameter so the
//! brain can build the note (YAML title, source footer, link card,
//! attachments) without re-parsing a pre-rendered body. Hard cut from
//! the v1 `content=<md>&title=` shape — coordinate the rollout with
//! the matching brain release.
//!
//! Common params on every URI: `v=2`, `kind`, `vault`, `captured_at`,
//! and `tags` (omitted when the merged list is empty). `source_app`,
//! `source_title`, `source_url` ride along whenever the capture has
//! them — except on `Link`, which omits `source_title` / `source_url`
//! because they are redundant with the canonical `url` + `title` from
//! the payload.
//!
//! Per-kind required params:
//! - `Note` / `Clip` send `text` (the raw payload).
//! - `Link` sends `url` plus an optional `title` (resolved from
//!   `source_title` -> `payload.title`; omitted when neither exists so
//!   the brain can fall back on its own).
//! - `Shot` / `File` send `path` (prefers `payload.blob_path` for
//!   pasted bytes, falls back to `payload.source_path` for drag-in or
//!   external files). `mime` rides along when present; `File` also
//!   sends `original_name` so the brain can preserve the user-facing
//!   filename. The path is emitted as a raw filesystem string — the
//!   brain side resolves it.

use crate::store::{Capture, CaptureKind, Destination, DestinationKind};

/// Error variants surfaced when a Capture cannot be turned into a
/// `kokobrain://` URI. Each variant maps to a user-visible message in
/// the command layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    /// Destination is not a kokobrain destination. Caller should route
    /// via the normal label path instead.
    WrongDestinationKind,
    /// Kokobrain destination is missing its config (vault is required).
    MissingConfig,
    /// Config JSON is malformed or the vault string is blank.
    InvalidConfig(String),
    /// The Capture's payload JSON is missing an expected field for the
    /// content mapping (e.g. `Link` without a `url`, `Shot`/`File`
    /// without a path).
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

/// Build the v2 `kokobrain://capture?v=2&kind=...&vault=...&...` URI
/// for the given Capture + Destination pair. See module docs for the
/// schema and per-kind field mapping.
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
    let tags = merge_tags(&destination.name, &cfg.tags);

    let mut s = url::form_urlencoded::Serializer::new(String::new());
    s.append_pair("v", "2");
    s.append_pair("kind", capture_kind_param(capture.kind));
    s.append_pair("vault", &cfg.vault);

    match capture.kind {
        CaptureKind::Note | CaptureKind::Clip | CaptureKind::Transcription => {
            let text = capture
                .payload
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or(BuildError::MalformedPayload("text"))?;
            s.append_pair("text", text);
            if let Some(v) = non_blank(capture.source_app.as_deref()) {
                s.append_pair("source_app", v);
            }
            if let Some(v) = non_blank(capture.source_title.as_deref()) {
                s.append_pair("source_title", v);
            }
            if let Some(v) = non_blank(capture.source_url.as_deref()) {
                s.append_pair("source_url", v);
            }
        }
        CaptureKind::Link => {
            let url = capture
                .payload
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or(BuildError::MalformedPayload("url"))?;
            s.append_pair("url", url);
            if let Some(t) = link_title(capture) {
                s.append_pair("title", &t);
            }
            if let Some(v) = non_blank(capture.source_app.as_deref()) {
                s.append_pair("source_app", v);
            }
            // source_title / source_url intentionally omitted: for Link
            // captures they duplicate `url` / `title` from the payload,
            // which the brain treats as authoritative.
        }
        CaptureKind::Shot | CaptureKind::File => {
            let path = capture
                .payload
                .get("blob_path")
                .and_then(|v| v.as_str())
                .or_else(|| capture.payload.get("source_path").and_then(|v| v.as_str()))
                .ok_or(BuildError::MalformedPayload("path"))?;
            s.append_pair("path", path);
            if let Some(mime) = capture.payload.get("mime").and_then(|v| v.as_str()) {
                s.append_pair("mime", mime);
            }
            if capture.kind == CaptureKind::File {
                if let Some(name) = capture
                    .payload
                    .get("original_name")
                    .and_then(|v| v.as_str())
                {
                    s.append_pair("original_name", name);
                }
            }
            if let Some(v) = non_blank(capture.source_app.as_deref()) {
                s.append_pair("source_app", v);
            }
            if let Some(v) = non_blank(capture.source_title.as_deref()) {
                s.append_pair("source_title", v);
            }
            if let Some(v) = non_blank(capture.source_url.as_deref()) {
                s.append_pair("source_url", v);
            }
        }
    }

    s.append_pair("captured_at", &capture.created_at);
    if !tags.is_empty() {
        s.append_pair("tags", &tags.join(","));
    }

    Ok(format!("kokobrain://capture?{}", s.finish()))
}

fn capture_kind_param(kind: CaptureKind) -> &'static str {
    match kind {
        CaptureKind::Note => "note",
        CaptureKind::Clip => "clip",
        CaptureKind::Link => "link",
        CaptureKind::Shot => "shot",
        CaptureKind::File => "file",
        CaptureKind::Transcription => "transcription",
    }
}

fn non_blank(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|t| !t.is_empty())
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

/// Resolve the optional `title` param for a Link capture. Prefers the
/// browser's window/tab title (`source_title`) captured at click time,
/// falls back to `payload.title` when the source title is absent or
/// blank. Returns `None` when neither is populated so the brain can
/// fall back on its own (e.g. deriving from the URL).
fn link_title(capture: &Capture) -> Option<String> {
    if let Some(t) = non_blank(capture.source_title.as_deref()) {
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

    fn shot_bytes_capture(blob_path: &str, mime: &str) -> Capture {
        Capture {
            id: "01H000000000000000000000SHOB".into(),
            kind: CaptureKind::Shot,
            created_at: "2026-05-18T12:00:00Z".into(),
            payload: serde_json::json!({
                "blob_path": blob_path,
                "mime": mime,
                "width": 800,
                "height": 600,
            }),
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

    fn file_capture(source_path: &str, mime: &str, original_name: &str) -> Capture {
        Capture {
            id: "01H000000000000000000000FILE".into(),
            kind: CaptureKind::File,
            created_at: "2026-05-18T12:00:00Z".into(),
            payload: serde_json::json!({
                "source_path": source_path,
                "mime": mime,
                "original_name": original_name,
            }),
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

    fn parse_pairs(uri: &str) -> Vec<(String, String)> {
        url::form_urlencoded::parse(uri.split('?').nth(1).unwrap().as_bytes())
            .into_owned()
            .collect()
    }

    fn get_pair<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
        pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    fn clip_capture_with_context(
        text: &str,
        source_app: Option<&str>,
        source_title: Option<&str>,
        source_url: Option<&str>,
    ) -> Capture {
        Capture {
            id: "01H000000000000000000000CLIP".into(),
            kind: CaptureKind::Clip,
            created_at: "2026-05-18T12:00:00Z".into(),
            payload: serde_json::json!({ "text": text }),
            source_app: source_app.map(String::from),
            starred: false,
            deleted_at: None,
            read_at: None,
            source_title: source_title.map(String::from),
            source_url: source_url.map(String::from),
            destination_id: None,
            routed_at: None,
        }
    }

    fn note_capture_with_context(
        text: &str,
        source_app: Option<&str>,
        source_title: Option<&str>,
        source_url: Option<&str>,
    ) -> Capture {
        Capture {
            id: "01H000000000000000000000NOTE".into(),
            kind: CaptureKind::Note,
            created_at: "2026-05-18T12:00:00Z".into(),
            payload: serde_json::json!({ "text": text }),
            source_app: source_app.map(String::from),
            starred: false,
            deleted_at: None,
            read_at: None,
            source_title: source_title.map(String::from),
            source_url: source_url.map(String::from),
            destination_id: None,
            routed_at: None,
        }
    }

    #[test]
    fn note_payload_becomes_text_param() {
        let cap = note_capture("hello world");
        let dest = kokobrain_dest("Reading List", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        assert!(uri.starts_with("kokobrain://capture?"));
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "v"), Some("2"));
        assert_eq!(get_pair(&pairs, "kind"), Some("note"));
        assert_eq!(get_pair(&pairs, "vault"), Some("Personal"));
        assert_eq!(get_pair(&pairs, "text"), Some("hello world"));
        assert_eq!(get_pair(&pairs, "tags"), Some("reading-list"));
        assert_eq!(get_pair(&pairs, "captured_at"), Some("2026-05-18T12:00:00Z"));
    }

    #[test]
    fn link_emits_url_and_source_title_as_title() {
        let cap = link_capture(
            "https://example.com/post",
            Some("Payload Title"),
            Some("Window Title"),
        );
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "kind"), Some("link"));
        assert_eq!(get_pair(&pairs, "url"), Some("https://example.com/post"));
        assert_eq!(get_pair(&pairs, "title"), Some("Window Title"));
    }

    #[test]
    fn link_falls_back_to_payload_title_then_omits_title() {
        let dest = kokobrain_dest("Brain", "Personal");

        // payload.title used when source_title is absent
        let cap = link_capture("https://example.com/post", Some("Payload Title"), None);
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "url"), Some("https://example.com/post"));
        assert_eq!(get_pair(&pairs, "title"), Some("Payload Title"));

        // neither source_title nor payload.title -> title param omitted
        // entirely (the brain derives its own fallback from `url`).
        let cap = link_capture("https://example.com/post", None, None);
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "url"), Some("https://example.com/post"));
        assert_eq!(get_pair(&pairs, "title"), None);
    }

    #[test]
    fn shot_bytes_emits_blob_path_and_mime() {
        let cap = shot_bytes_capture("/var/koko/blobs/01HSHOT.png", "image/png");
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "kind"), Some("shot"));
        assert_eq!(get_pair(&pairs, "path"), Some("/var/koko/blobs/01HSHOT.png"));
        assert_eq!(get_pair(&pairs, "mime"), Some("image/png"));
        assert_eq!(get_pair(&pairs, "original_name"), None);
    }

    #[test]
    fn shot_drag_falls_back_to_source_path() {
        let cap = shot_capture();
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "kind"), Some("shot"));
        assert_eq!(get_pair(&pairs, "path"), Some("/tmp/x.png"));
        assert_eq!(get_pair(&pairs, "mime"), Some("image/png"));
    }

    #[test]
    fn file_emits_path_mime_and_original_name() {
        let cap = file_capture("/tmp/notes.pdf", "application/pdf", "notes.pdf");
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "kind"), Some("file"));
        assert_eq!(get_pair(&pairs, "path"), Some("/tmp/notes.pdf"));
        assert_eq!(get_pair(&pairs, "mime"), Some("application/pdf"));
        assert_eq!(get_pair(&pairs, "original_name"), Some("notes.pdf"));
    }

    #[test]
    fn shot_without_any_path_rejected_as_malformed_payload() {
        let mut cap = shot_capture();
        cap.payload = serde_json::json!({ "mime": "image/png" });
        let dest = kokobrain_dest("Brain", "Personal");
        let err = build_capture_uri(&cap, &dest).expect_err("rejected");
        assert_eq!(err, BuildError::MalformedPayload("path"));
    }

    #[test]
    fn shot_prefers_blob_path_over_source_path() {
        let mut cap = shot_bytes_capture("/var/koko/blobs/x.png", "image/png");
        let payload = cap.payload.as_object_mut().unwrap();
        payload.insert("source_path".into(), serde_json::json!("/should/be/ignored.png"));
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "path"), Some("/var/koko/blobs/x.png"));
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
    fn special_chars_in_text_are_percent_encoded() {
        let cap = note_capture("a & b = c #tag");
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        // The literal `&`, `=`, `#` must not survive in the query
        // (they would break parsing on the brain side).
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "text"), Some("a & b = c #tag"));
    }

    #[test]
    fn note_with_source_context_emits_source_params() {
        let cap = note_capture_with_context(
            "remember XYZ",
            Some("com.google.Chrome"),
            Some("Some Page"),
            Some("https://example.com"),
        );
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "source_app"), Some("com.google.Chrome"));
        assert_eq!(get_pair(&pairs, "source_title"), Some("Some Page"));
        assert_eq!(get_pair(&pairs, "source_url"), Some("https://example.com"));
    }

    #[test]
    fn clip_with_source_context_emits_text_and_source_params() {
        let cap = clip_capture_with_context(
            "Whether I will remain open-minded...",
            Some("com.google.Chrome"),
            Some("Four Notes to My Future Self"),
            Some("https://medium.com/post"),
        );
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "kind"), Some("clip"));
        assert_eq!(
            get_pair(&pairs, "text"),
            Some("Whether I will remain open-minded...")
        );
        assert_eq!(get_pair(&pairs, "source_app"), Some("com.google.Chrome"));
        assert_eq!(
            get_pair(&pairs, "source_title"),
            Some("Four Notes to My Future Self")
        );
        assert_eq!(
            get_pair(&pairs, "source_url"),
            Some("https://medium.com/post")
        );
    }

    #[test]
    fn link_omits_source_title_and_source_url() {
        // Even when the Capture carries source_title / source_url, Link
        // URIs strip them because the payload's `url` / `title` are
        // canonical. `source_app` still rides along for provenance.
        let mut cap = link_capture(
            "https://example.com/post",
            Some("Payload Title"),
            Some("Window Title"),
        );
        cap.source_app = Some("com.google.Chrome".into());
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "source_app"), Some("com.google.Chrome"));
        assert_eq!(get_pair(&pairs, "source_title"), None);
        assert_eq!(get_pair(&pairs, "source_url"), None);
    }

    #[test]
    fn blank_source_fields_are_omitted() {
        // Whitespace-only source fields must not leak into the URI as
        // empty values; the brain treats `source_*` presence as "we
        // know this", so blanks would be misleading.
        let cap = note_capture_with_context("hi", Some("  "), Some(""), Some("   "));
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "source_app"), None);
        assert_eq!(get_pair(&pairs, "source_title"), None);
        assert_eq!(get_pair(&pairs, "source_url"), None);
    }

    #[test]
    fn captured_at_passes_through_from_capture_created_at() {
        let cap = note_capture("hi");
        let dest = kokobrain_dest("Brain", "Personal");
        let uri = build_capture_uri(&cap, &dest).expect("build");
        let pairs = parse_pairs(&uri);
        assert_eq!(get_pair(&pairs, "captured_at"), Some("2026-05-18T12:00:00Z"));
    }

    #[test]
    fn all_supported_kinds_carry_v2_and_kind_params() {
        let dest = kokobrain_dest("Brain", "Personal");

        let note = build_capture_uri(&note_capture("x"), &dest).expect("note");
        let np = parse_pairs(&note);
        assert_eq!(get_pair(&np, "v"), Some("2"));
        assert_eq!(get_pair(&np, "kind"), Some("note"));

        let clip = build_capture_uri(
            &clip_capture_with_context("x", None, None, None),
            &dest,
        )
        .expect("clip");
        let cp = parse_pairs(&clip);
        assert_eq!(get_pair(&cp, "v"), Some("2"));
        assert_eq!(get_pair(&cp, "kind"), Some("clip"));

        let link =
            build_capture_uri(&link_capture("https://x.test", None, None), &dest).expect("link");
        let lp = parse_pairs(&link);
        assert_eq!(get_pair(&lp, "v"), Some("2"));
        assert_eq!(get_pair(&lp, "kind"), Some("link"));
    }
}
