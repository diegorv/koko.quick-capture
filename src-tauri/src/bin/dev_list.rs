//! Dev-only CLI to list recent Captures from the store.
//!
//! Per ADR-0004 this binary calls `quick_capture_lib::store::Store::list`
//! directly. No SQL strings appear in this source. It exists so we can
//! verify v0.1's write path before the Inbox window lands; see
//! `.scratch/v0-1-mvp/issues/03-dev-cli-list-captures.md`.

use std::process::ExitCode;

use quick_capture_lib::store::{Capture, CaptureKind, Store, StoreError};

const DEFAULT_LIMIT: u32 = 20;
const NOTE_PREVIEW_MAX: usize = 60;
const SHORT_ULID_LEN: usize = 8;
const COL_SEP: &str = "  ";

#[derive(Debug)]
struct Args {
    limit: u32,
    db: Option<String>,
}

#[derive(Debug)]
enum CliError {
    BadArgs(String),
    Store(StoreError),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::BadArgs(msg) => write!(f, "{msg}"),
            CliError::Store(e) => write!(f, "{e}"),
        }
    }
}

impl From<StoreError> for CliError {
    fn from(e: StoreError) -> Self {
        CliError::Store(e)
    }
}

fn parse_args(argv: &[String]) -> Result<Args, CliError> {
    let mut limit = DEFAULT_LIMIT;
    let mut db: Option<String> = None;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--limit" => {
                let v = argv
                    .get(i + 1)
                    .ok_or_else(|| CliError::BadArgs("--limit requires a value".to_string()))?;
                limit = v
                    .parse::<u32>()
                    .map_err(|_| CliError::BadArgs(format!("invalid --limit value: {v}")))?;
                i += 2;
            }
            "--db" => {
                let v = argv
                    .get(i + 1)
                    .ok_or_else(|| CliError::BadArgs("--db requires a value".to_string()))?;
                db = Some(v.clone());
                i += 2;
            }
            other => {
                return Err(CliError::BadArgs(format!("unknown argument: {other}")));
            }
        }
    }
    Ok(Args { limit, db })
}

fn format_line(c: &Capture) -> String {
    let short = if c.id.len() >= SHORT_ULID_LEN {
        &c.id[..SHORT_ULID_LEN]
    } else {
        &c.id
    };
    let preview = payload_preview(c);
    format!(
        "{short}{sep}{kind}{sep}{created}{sep}{preview}",
        short = short,
        sep = COL_SEP,
        kind = kind_label(c.kind),
        created = c.created_at,
        preview = preview,
    )
}

fn kind_label(k: CaptureKind) -> &'static str {
    match k {
        CaptureKind::Link => "Link",
        CaptureKind::Clip => "Clip",
        CaptureKind::Shot => "Shot",
        CaptureKind::File => "File",
        CaptureKind::Note => "Note",
        CaptureKind::Transcription => "Transcription",
    }
}

fn payload_preview(c: &Capture) -> String {
    match c.kind {
        CaptureKind::Note => {
            let text = c
                .payload
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            truncate_single_line(text, NOTE_PREVIEW_MAX)
        }
        other => format!("{}: ?", kind_label(other)),
    }
}

fn truncate_single_line(s: &str, max_chars: usize) -> String {
    let escaped: String = s
        .chars()
        .map(|c| match c {
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            other => other.to_string(),
        })
        .collect();
    let char_count = escaped.chars().count();
    if char_count <= max_chars {
        return escaped;
    }
    let mut out: String = escaped.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

fn run(argv: &[String]) -> Result<(), CliError> {
    let args = parse_args(argv)?;
    let store = match args.db {
        Some(path) => Store::open(path)?,
        None => Store::open_default()?,
    };
    let rows = store.list(args.limit)?;
    for c in &rows {
        println!("{}", format_line(c));
    }
    Ok(())
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match run(&argv) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("dev_list: {e}");
            ExitCode::FAILURE
        }
    }
}
