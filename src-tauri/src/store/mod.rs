//! Capture store. The only module that talks to SQLite.
//!
//! Per ADR-0001 and ADR-0004 all persistence and ULID assignment lives here.
//! Slice 02 only writes `Note` captures, but the schema and the public
//! interface are complete so later slices and v1.0 do not need a migration.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Settings key for the ULID of the newest Capture at the moment the
/// user last opened the Inbox. The Dock's unread badge is the count of
/// non-deleted captures with `id > <this value>`.
pub const SETTING_LAST_INBOX_OPEN_ID: &str = "last_inbox_open_id";

/// ULID min (26 zero characters). Used as the default `count_after`
/// cursor when `SETTING_LAST_INBOX_OPEN_ID` has never been written, so
/// the first-launch badge equals the total non-deleted capture count.
pub const ULID_MIN: &str = "00000000000000000000000000";

/// Closed set of Capture kinds. See `CONTEXT.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureKind {
    Link,
    Clip,
    Shot,
    File,
    Note,
}

impl CaptureKind {
    fn as_str(&self) -> &'static str {
        match self {
            CaptureKind::Link => "Link",
            CaptureKind::Clip => "Clip",
            CaptureKind::Shot => "Shot",
            CaptureKind::File => "File",
            CaptureKind::Note => "Note",
        }
    }

    fn parse(value: &str) -> Result<CaptureKind, StoreError> {
        match value {
            "Link" => Ok(CaptureKind::Link),
            "Clip" => Ok(CaptureKind::Clip),
            "Shot" => Ok(CaptureKind::Shot),
            "File" => Ok(CaptureKind::File),
            "Note" => Ok(CaptureKind::Note),
            other => Err(StoreError::Decode(format!("unknown kind: {other}"))),
        }
    }
}

/// Where the image bytes for a `Shot` Capture come from.
///
/// Two sources end up as `Shot`: bytes pulled off the clipboard
/// (screenshot in `Cmd+Ctrl+Shift+4` style flows), and a file reference
/// to an existing image on disk (a Finder copy of `.png` etc.). We keep
/// them on one variant rather than two so the `kind()` mapping stays a
/// single arm and `kind_detect` returns a flat `CaptureInput::Shot`.
#[derive(Debug, Clone)]
pub enum ShotSource {
    /// Image bytes that need persisting under `blobs/<ulid>.<ext>`.
    Bytes { bytes: Vec<u8>, mime: String },
    /// An existing on-disk image; recorded by path, not copied.
    Path {
        source_path: PathBuf,
        mime: String,
    },
}

/// What the caller hands to `save`. Slice 02 added `Note`; slice 04
/// added `Link` and `Clip`; slice 05 adds `Shot` (clipboard image or
/// image file reference) and `File` (non-image file reference).
#[derive(Debug, Clone)]
pub enum CaptureInput {
    Note {
        text: String,
    },
    Link {
        url: String,
        raw_text: String,
        title: Option<String>,
    },
    Clip {
        text: String,
    },
    Shot {
        source: ShotSource,
        width: Option<u32>,
        height: Option<u32>,
    },
    File {
        source_path: PathBuf,
        mime: String,
        original_name: Option<String>,
    },
}

impl CaptureInput {
    fn kind(&self) -> CaptureKind {
        match self {
            CaptureInput::Note { .. } => CaptureKind::Note,
            CaptureInput::Link { .. } => CaptureKind::Link,
            CaptureInput::Clip { .. } => CaptureKind::Clip,
            CaptureInput::Shot { .. } => CaptureKind::Shot,
            CaptureInput::File { .. } => CaptureKind::File,
        }
    }
}

/// A row in the `captures` table, returned to the caller. The `payload`
/// is the kind-specific JSON object (per the PRD's per-kind shapes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capture {
    pub id: String,
    pub kind: CaptureKind,
    pub created_at: String,
    pub payload: serde_json::Value,
    pub source_app: Option<String>,
    pub starred: bool,
    pub deleted_at: Option<String>,
    pub read_at: Option<String>,
    pub source_title: Option<String>,
    pub source_url: Option<String>,
    /// ULID of the [Destination] this Capture is Routed to. `None`
    /// means the Capture is still in the Inbox. Set + reset by
    /// `capture_route` / `capture_unroute`.
    pub destination_id: Option<String>,
    /// ISO timestamp of the most recent routing event. Always paired
    /// with `destination_id`: both set together, both cleared on
    /// un-route, both updated on re-route.
    pub routed_at: Option<String>,
}

/// A row in the `destinations` table. User-managed label that a
/// Capture can be Routed to. See ADR-0010.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Destination {
    pub id: String,
    pub name: String,
    /// Palette key (e.g. "red", "teal"). `None` means no color picked.
    pub color: Option<String>,
    pub created_at: String,
    pub deleted_at: Option<String>,
}

/// Resolved context for a capture about to be persisted: macOS
/// bundle id of the source app, plus optional window title / URL for
/// known browsers. Defaults to all-`None` so callers without context
/// can use `CaptureContext::default()` instead of three explicit
/// `None`s.
#[derive(Debug, Clone, Default)]
pub struct CaptureContext {
    pub source_app: Option<String>,
    pub source_title: Option<String>,
    pub source_url: Option<String>,
}

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Db(rusqlite::Error),
    Decode(String),
    NotFound(String),
    /// A Destination operation failed because the name is already
    /// taken by another live Destination. Surfaces from create /
    /// rename / restore so the UI can prompt the user to pick a
    /// different name.
    DestinationNameConflict(String),
    /// Caller passed a value that violates a precondition that the
    /// command/UI layer is supposed to enforce (empty name, routing
    /// to a soft-deleted destination, etc.). Indicates a bug in the
    /// caller, not a user error.
    InvalidArgument(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "io error: {e}"),
            StoreError::Db(e) => write!(f, "db error: {e}"),
            StoreError::Decode(msg) => write!(f, "decode error: {msg}"),
            StoreError::NotFound(id) => write!(f, "capture not found: {id}"),
            StoreError::DestinationNameConflict(name) => {
                write!(f, "destination name already in use: {name}")
            }
            StoreError::InvalidArgument(msg) => write!(f, "invalid argument: {msg}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Db(e)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        StoreError::Decode(e.to_string())
    }
}

pub struct Store {
    conn: std::sync::Mutex<Connection>,
    blobs_dir: PathBuf,
    /// Per-capture JSON dump directory. Each save writes
    /// `dumps/<ulid>.json` with the full serialised `Capture`; every
    /// mutation (star toggle, soft-delete, mark-read) overwrites the
    /// same file. Soft-deleted rows are NOT removed from disk — the
    /// JSON's `deleted_at` field flips to a timestamp instead, so the
    /// folder doubles as a tombstone-aware trash log for future
    /// restore tooling.
    dumps_dir: PathBuf,
}

impl Store {
    /// Open the store at `path`. Creates the parent directory and runs
    /// the schema migration if needed. The `blobs/` directory sits next
    /// to the database file (created lazily on the first image save).
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, StoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::migrate(&conn)?;
        let blobs_dir = path
            .parent()
            .map(|p| p.join("blobs"))
            .unwrap_or_else(|| PathBuf::from("blobs"));
        let dumps_dir = path
            .parent()
            .map(|p| p.join("dumps"))
            .unwrap_or_else(|| PathBuf::from("dumps"));
        Ok(Store {
            conn: std::sync::Mutex::new(conn),
            blobs_dir,
            dumps_dir,
        })
    }

    /// Open the store at the default location:
    /// `~/Library/Application Support/com.koko.quick-capture/captures.db`.
    pub fn open_default() -> Result<Self, StoreError> {
        Self::open(default_db_path()?)
    }

    fn migrate(conn: &Connection) -> Result<(), StoreError> {
        // Foreign keys are off by default in SQLite. We rely on the
        // captures.destination_id FK to keep routed Captures pointing
        // at real Destination rows.
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS destinations (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                color TEXT,
                created_at TEXT NOT NULL,
                deleted_at TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_destinations_name_live
                ON destinations(name) WHERE deleted_at IS NULL;
            CREATE TABLE IF NOT EXISTS captures (
                id TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL,
                created_at TEXT NOT NULL,
                payload TEXT NOT NULL,
                source_app TEXT,
                starred INTEGER NOT NULL DEFAULT 0,
                deleted_at TEXT,
                read_at TEXT,
                source_title TEXT,
                source_url TEXT,
                destination_id TEXT REFERENCES destinations(id),
                routed_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_captures_created_at
                ON captures(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_captures_destination_id
                ON captures(destination_id);
            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );
            -- FTS5 index of the per-kind text payload (see
            -- `searchable_text_for_input`). We keep our own
            -- `capture_id` column (UNINDEXED) so the JOIN back to
            -- `captures` does not need rowid bookkeeping. Writes go
            -- through `Store::save` / `Store::soft_delete`; no
            -- triggers, so the index can never drift from the
            -- application's write surface.
            CREATE VIRTUAL TABLE IF NOT EXISTS captures_fts USING fts5(
                capture_id UNINDEXED,
                text,
                tokenize='unicode61 remove_diacritics 2'
            );",
        )?;

        Ok(())
    }

    /// Serialise a Capture into `dumps/<ulid>.json`. Best-effort:
    /// errors are logged but never surfaced to the caller — losing a
    /// dump must not fail the underlying SQLite write.
    ///
    /// The contract is "the JSON on disk reflects the latest known
    /// state of the row", so star toggles / soft-deletes / read
    /// flips all re-write the same file. The folder is the source of
    /// truth for the "trash" / restore tooling that may land later.
    fn write_dump(&self, capture: &Capture) {
        if let Err(e) = std::fs::create_dir_all(&self.dumps_dir) {
            eprintln!("dumps dir create failed: {e}");
            return;
        }
        let path = self.dumps_dir.join(format!("{}.json", capture.id));
        let json = match serde_json::to_string_pretty(capture) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("dump serialize failed for {}: {e}", capture.id);
                return;
            }
        };
        if let Err(e) = std::fs::write(&path, json) {
            eprintln!("dump write failed for {}: {e}", capture.id);
        }
    }

    /// Re-read a capture by id (deleted or not) and re-write its
    /// dump. Used by every mutation that does not already hold the
    /// post-mutation `Capture` value (set_star, soft_delete,
    /// mark_read). Failure to find the row after a mutation is a
    /// bug, but logged-and-skipped here rather than panicking — the
    /// dump system is best-effort.
    fn refresh_dump(&self, id: &Ulid) {
        match self.find_with_deleted(id) {
            Ok(Some(capture)) => self.write_dump(&capture),
            Ok(None) => {
                eprintln!("refresh_dump: capture {id} vanished post-mutation");
            }
            Err(e) => {
                eprintln!("refresh_dump: lookup for {id} failed: {e}");
            }
        }
    }

    /// Persist a new Capture. Equivalent to
    /// `save_with_context(input, Default::default())`; kept as the
    /// simple default for tests + paths that have no context to
    /// attribute (drag-drop, programmatic seeds).
    pub fn save(&self, input: CaptureInput) -> Result<Capture, StoreError> {
        self.save_with_context(input, CaptureContext::default())
    }

    /// Persist a new Capture with the resolved macOS context
    /// (source_app bundle id + optional window title + optional
    /// active URL for known browsers). The id is a freshly minted
    /// ULID and the `created_at` is now (UTC, RFC3339). For
    /// `Shot { source: Bytes }` the bytes are written to
    /// `blobs/<ulid>.<ext>` next to the DB and `payload.blob_path`
    /// records the resolved path.
    pub fn save_with_context(
        &self,
        input: CaptureInput,
        ctx: CaptureContext,
    ) -> Result<Capture, StoreError> {
        let id = Ulid::new().to_string();
        let kind = input.kind();
        let created_at = now_rfc3339();
        let payload = self.build_payload(&id, &input)?;
        let payload_str = serde_json::to_string(&payload)?;

        // Append context fields to the FTS index so a search for the
        // app name, page title, or URL host surfaces captures taken
        // in that context. Each is its own whitespace-separated
        // token group for the tokenizer.
        let mut search_text = searchable_text_for_input(&input);
        for extra in [&ctx.source_app, &ctx.source_title, &ctx.source_url] {
            if let Some(s) = extra.as_deref() {
                if !s.is_empty() {
                    if !search_text.is_empty() {
                        search_text.push(' ');
                    }
                    search_text.push_str(s);
                }
            }
        }

        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute(
            "INSERT INTO captures
             (id, kind, created_at, payload, source_app, starred, deleted_at,
              source_title, source_url, destination_id, routed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, ?6, ?7, NULL, NULL)",
            params![
                &id,
                kind.as_str(),
                &created_at,
                &payload_str,
                &ctx.source_app,
                &ctx.source_title,
                &ctx.source_url,
            ],
        )?;
        // Mirror the row into the FTS5 index. Errors here are
        // logged-and-ignored at the call site; failing the entire
        // save because the search index is unhappy would be worse
        // than the search index being slightly stale.
        if let Err(e) = conn.execute(
            "INSERT INTO captures_fts (capture_id, text) VALUES (?1, ?2)",
            params![&id, &search_text],
        ) {
            eprintln!("fts5 insert failed for {id}: {e}");
        }

        let capture = Capture {
            id,
            kind,
            created_at,
            payload,
            source_app: ctx.source_app,
            starred: false,
            deleted_at: None,
            read_at: None,
            source_title: ctx.source_title,
            source_url: ctx.source_url,
            destination_id: None,
            routed_at: None,
        };
        // Release the conn lock before touching the filesystem so a
        // slow disk write does not block other DB readers.
        drop(conn);
        self.write_dump(&capture);
        Ok(capture)
    }

    /// Build the JSON payload for a capture row, doing any side-effects
    /// (blob writes) the variant requires. Kept on `Store` because the
    /// `Shot { Bytes }` arm needs `blobs_dir`.
    fn build_payload(
        &self,
        id: &str,
        input: &CaptureInput,
    ) -> Result<serde_json::Value, StoreError> {
        Ok(match input {
            CaptureInput::Note { text } => serde_json::json!({ "text": text }),
            CaptureInput::Link {
                url,
                raw_text,
                title,
            } => serde_json::json!({
                "url": url,
                "raw_text": raw_text,
                "title": title,
            }),
            CaptureInput::Clip { text } => serde_json::json!({ "text": text }),
            CaptureInput::Shot {
                source,
                width,
                height,
            } => match source {
                ShotSource::Bytes { bytes, mime } => {
                    let ext = extension_for_mime(mime);
                    std::fs::create_dir_all(&self.blobs_dir)?;
                    let blob_path = self.blobs_dir.join(format!("{id}.{ext}"));
                    std::fs::write(&blob_path, bytes)?;
                    serde_json::json!({
                        "blob_path": blob_path.to_string_lossy(),
                        "mime": mime,
                        "width": width,
                        "height": height,
                    })
                }
                ShotSource::Path { source_path, mime } => serde_json::json!({
                    "source_path": source_path.to_string_lossy(),
                    "mime": mime,
                    "width": width,
                    "height": height,
                }),
            },
            CaptureInput::File {
                source_path,
                mime,
                original_name,
            } => serde_json::json!({
                "source_path": source_path.to_string_lossy(),
                "mime": mime,
                "original_name": original_name,
            }),
        })
    }

    /// Return up to `limit` Inbox captures (non-deleted, un-routed),
    /// newest first. Routed captures live in the Archive surface; use
    /// `list_archive_before` for those.
    pub fn list(&self, limit: u32) -> Result<Vec<Capture>, StoreError> {
        self.list_before(None, limit)
    }

    /// Cursor-paginated Inbox list. Returns up to `limit` Inbox
    /// captures (non-deleted, un-routed) strictly older than `cursor`
    /// (when present), ordered newest first. `cursor = None` returns
    /// the first page. ULIDs are time-sortable, so `WHERE id < cursor`
    /// is equivalent to "older than". Routed captures are excluded —
    /// see `list_archive_before` for those.
    pub fn list_before(
        &self,
        cursor: Option<Ulid>,
        limit: u32,
    ) -> Result<Vec<Capture>, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut out = Vec::new();
        match cursor {
            Some(c) => {
                let cursor_str = c.to_string();
                let mut stmt = conn.prepare(
                    "SELECT id, kind, created_at, payload, source_app, starred, deleted_at, read_at, source_title, source_url, destination_id, routed_at
                     FROM captures
                     WHERE deleted_at IS NULL AND destination_id IS NULL AND id < ?1
                     ORDER BY id DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![&cursor_str, limit], row_to_capture)?;
                for row in rows {
                    out.push(row??);
                }
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, kind, created_at, payload, source_app, starred, deleted_at, read_at, source_title, source_url, destination_id, routed_at
                     FROM captures
                     WHERE deleted_at IS NULL AND destination_id IS NULL
                     ORDER BY id DESC
                     LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit], row_to_capture)?;
                for row in rows {
                    out.push(row??);
                }
            }
        }
        Ok(out)
    }

    /// Set the `starred` flag on a capture.
    pub fn set_star(&self, id: &Ulid, starred: bool) -> Result<(), StoreError> {
        let id_str = id.to_string();
        let conn = self.conn.lock().expect("store mutex poisoned");
        let n = conn.execute(
            "UPDATE captures SET starred = ?1 WHERE id = ?2",
            params![starred as i64, &id_str],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound(id_str));
        }
        drop(conn);
        self.refresh_dump(id);
        Ok(())
    }

    /// Soft-delete a capture: stamp `deleted_at` so it stops surfacing
    /// in `list` but the row stays in the DB as a tombstone. The FTS5
    /// row is dropped so search results respect the deletion without
    /// needing to filter at query time.
    pub fn soft_delete(&self, id: &Ulid) -> Result<(), StoreError> {
        let id_str = id.to_string();
        let now = now_rfc3339();
        let conn = self.conn.lock().expect("store mutex poisoned");
        let n = conn.execute(
            "UPDATE captures SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![&now, &id_str],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound(id_str));
        }
        if let Err(e) = conn.execute(
            "DELETE FROM captures_fts WHERE capture_id = ?1",
            params![&id_str],
        ) {
            eprintln!("fts5 delete failed for {id_str}: {e}");
        }
        drop(conn);
        self.refresh_dump(id);
        Ok(())
    }

    /// Full-text search over the per-kind indexable text. Returns
    /// non-deleted captures ranked by FTS5's default bm25, newest-id
    /// tiebreaker so ULIDs sort naturally. Empty / no-token queries
    /// return an empty Vec — the caller is expected to fall back to
    /// `list_before` for "no search active".
    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<Capture>, StoreError> {
        let Some(match_expr) = build_fts_match(query) else {
            return Ok(Vec::new());
        };
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT c.id, c.kind, c.created_at, c.payload, c.source_app, c.starred, c.deleted_at, c.read_at, c.source_title, c.source_url, c.destination_id, c.routed_at
             FROM captures c
             JOIN captures_fts f ON f.capture_id = c.id
             WHERE captures_fts MATCH ?1
               AND c.deleted_at IS NULL
               AND c.destination_id IS NULL
             ORDER BY rank, c.id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![&match_expr, limit], row_to_capture)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row??);
        }
        Ok(out)
    }

    /// Count non-deleted, un-routed captures whose id is strictly
    /// greater than the given ULID string. Legacy cursor-based unread
    /// count; the per-item read tracking introduced in v1.0 replaces
    /// this for the Dock badge, but the method stays exposed because
    /// it is still referenced by older fixtures and may be useful for
    /// diagnostics. Routed captures are excluded — the user already
    /// decided what to do with them.
    pub fn count_after(&self, cursor: &str) -> Result<u64, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM captures
             WHERE id > ?1 AND deleted_at IS NULL AND destination_id IS NULL",
            params![cursor],
            |row| row.get(0),
        )?;
        Ok(n as u64)
    }

    /// Count Inbox captures the user has not yet interacted with
    /// (un-routed, non-deleted, never marked read). Drives the Dock's
    /// unread badge.
    pub fn count_unread(&self) -> Result<u64, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM captures
             WHERE deleted_at IS NULL
               AND read_at IS NULL
               AND destination_id IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(n as u64)
    }

    /// Count of all un-routed, non-deleted captures. Drives the
    /// Inbox/Archive switcher's "Inbox (N)" badge.
    pub fn count_inbox(&self) -> Result<u64, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM captures
             WHERE deleted_at IS NULL AND destination_id IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(n as u64)
    }

    /// Stamp `read_at` for a single capture. Idempotent: a row that
    /// already has `read_at` set is left alone (the original first-read
    /// timestamp is preserved). Returns `true` if this call actually
    /// flipped the row from unread to read, `false` otherwise.
    pub fn mark_read(&self, id: &Ulid) -> Result<bool, StoreError> {
        let id_str = id.to_string();
        let now = now_rfc3339();
        let conn = self.conn.lock().expect("store mutex poisoned");
        let n = conn.execute(
            "UPDATE captures SET read_at = ?1
             WHERE id = ?2 AND read_at IS NULL AND deleted_at IS NULL",
            params![&now, &id_str],
        )?;
        let flipped = n > 0;
        drop(conn);
        if flipped {
            self.refresh_dump(id);
        }
        Ok(flipped)
    }

    /// Total count of non-deleted captures. Used by the Inbox status
    /// bar; cheap (indexed COUNT(*)) and called only on mount + on
    /// `captures:changed`.
    pub fn count_all(&self) -> Result<u64, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM captures WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(n as u64)
    }

    /// Read a value from the `app_settings` table. Returns `None` when
    /// the key has never been written. Per ADR-0004 this is the only
    /// path the frontend has to persisted app-level scalars.
    pub fn settings_get(&self, key: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt = conn.prepare("SELECT value FROM app_settings WHERE key = ?1")?;
        let value: Option<String> = stmt
            .query_row(params![key], |row| row.get::<_, String>(0))
            .optional()?;
        Ok(value)
    }

    /// Upsert a value into the `app_settings` table.
    pub fn settings_set(&self, key: &str, value: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// List live (non-soft-deleted) Destinations, alpha-sorted by
    /// case-insensitive name. Used by the picker and by the live
    /// Settings list.
    pub fn destinations_list_live(&self) -> Result<Vec<Destination>, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, name, color, created_at, deleted_at
             FROM destinations
             WHERE deleted_at IS NULL
             ORDER BY LOWER(name) ASC",
        )?;
        let rows = stmt.query_map([], row_to_destination)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row??);
        }
        Ok(out)
    }

    /// List soft-deleted Destinations, alpha-sorted by name. Used by
    /// the Settings "Soft-deleted" collapsible section for restore.
    pub fn destinations_list_deleted(&self) -> Result<Vec<Destination>, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, name, color, created_at, deleted_at
             FROM destinations
             WHERE deleted_at IS NOT NULL
             ORDER BY LOWER(name) ASC",
        )?;
        let rows = stmt.query_map([], row_to_destination)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row??);
        }
        Ok(out)
    }

    /// Look up a single Destination by id, regardless of soft-delete
    /// state. Returns `None` when the id has no row at all.
    pub fn destination_find(&self, id: &str) -> Result<Option<Destination>, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, name, color, created_at, deleted_at
             FROM destinations
             WHERE id = ?1",
        )?;
        let row = stmt.query_row(params![id], row_to_destination).optional()?;
        match row {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    /// Create a new live Destination. `name` is trimmed; an empty
    /// result rejects with `InvalidArgument`. Returns
    /// `DestinationNameConflict` when the trimmed name matches another
    /// live Destination's name.
    pub fn destination_create(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<Destination, StoreError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::InvalidArgument(
                "destination name must not be blank".into(),
            ));
        }
        let color = normalize_color(color);
        let id = Ulid::new().to_string();
        let created_at = now_rfc3339();

        let conn = self.conn.lock().expect("store mutex poisoned");
        let result = conn.execute(
            "INSERT INTO destinations (id, name, color, created_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, NULL)",
            params![&id, name, &color, &created_at],
        );
        if let Err(e) = result {
            if is_unique_violation(&e) {
                return Err(StoreError::DestinationNameConflict(name.to_string()));
            }
            return Err(StoreError::Db(e));
        }
        Ok(Destination {
            id,
            name: name.to_string(),
            color,
            created_at,
            deleted_at: None,
        })
    }

    /// Rename / recolor a Destination. Accepts any state (live or
    /// soft-deleted) so the Settings UI can rename a soft-deleted
    /// row before restore if needed. `name` is trimmed; empty rejects
    /// with `InvalidArgument`. Returns `NotFound` when no row matches
    /// and `DestinationNameConflict` when the new name collides with
    /// another live Destination.
    pub fn destination_update(
        &self,
        id: &str,
        name: &str,
        color: Option<&str>,
    ) -> Result<(), StoreError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::InvalidArgument(
                "destination name must not be blank".into(),
            ));
        }
        let color = normalize_color(color);
        let conn = self.conn.lock().expect("store mutex poisoned");
        let result = conn.execute(
            "UPDATE destinations SET name = ?1, color = ?2 WHERE id = ?3",
            params![name, &color, id],
        );
        match result {
            Ok(0) => Err(StoreError::NotFound(id.to_string())),
            Ok(_) => Ok(()),
            Err(e) if is_unique_violation(&e) => {
                Err(StoreError::DestinationNameConflict(name.to_string()))
            }
            Err(e) => Err(StoreError::Db(e)),
        }
    }

    /// Soft-delete a Destination. Stamps `deleted_at` so the row is
    /// hidden from the picker but Captures already pointing at it
    /// keep their reference. Idempotent: re-deleting a soft-deleted
    /// row is a no-op (returns Ok).
    pub fn destination_soft_delete(&self, id: &str) -> Result<(), StoreError> {
        let now = now_rfc3339();
        let conn = self.conn.lock().expect("store mutex poisoned");
        let n = conn.execute(
            "UPDATE destinations SET deleted_at = ?1
             WHERE id = ?2 AND deleted_at IS NULL",
            params![&now, id],
        )?;
        if n == 0 {
            // Distinguish "doesn't exist" from "already deleted".
            if self.destination_find(id)?.is_none() {
                return Err(StoreError::NotFound(id.to_string()));
            }
        }
        Ok(())
    }

    /// Restore a previously soft-deleted Destination. Returns
    /// `DestinationNameConflict` when a live Destination already
    /// holds the same name (the UI is expected to prompt the user
    /// to rename the soft-deleted row first via `destination_update`
    /// and then re-attempt restore).
    pub fn destination_restore(&self, id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let result = conn.execute(
            "UPDATE destinations SET deleted_at = NULL
             WHERE id = ?1 AND deleted_at IS NOT NULL",
            params![id],
        );
        match result {
            Ok(0) => {
                drop(conn);
                if self.destination_find(id)?.is_none() {
                    Err(StoreError::NotFound(id.to_string()))
                } else {
                    Ok(())
                }
            }
            Ok(_) => Ok(()),
            Err(e) if is_unique_violation(&e) => {
                drop(conn);
                // Recover the conflicting name from the row we tried
                // to restore so the error carries useful context.
                let name = self
                    .destination_find(id)?
                    .map(|d| d.name)
                    .unwrap_or_else(|| id.to_string());
                Err(StoreError::DestinationNameConflict(name))
            }
            Err(e) => Err(StoreError::Db(e)),
        }
    }

    /// Route a Capture to a Destination. Sets `destination_id` and
    /// stamps `routed_at` with now. Also marks the Capture as read
    /// so a routed Capture never lingers in the unread Dock badge.
    /// Refuses to route to a soft-deleted Destination — the picker
    /// hides those, so reaching this branch indicates a UI bug.
    pub fn capture_route(&self, id: &Ulid, destination_id: &str) -> Result<(), StoreError> {
        let dest = self
            .destination_find(destination_id)?
            .ok_or_else(|| StoreError::NotFound(destination_id.to_string()))?;
        if dest.deleted_at.is_some() {
            return Err(StoreError::InvalidArgument(format!(
                "cannot route to soft-deleted destination {destination_id}"
            )));
        }

        let id_str = id.to_string();
        let now = now_rfc3339();
        let conn = self.conn.lock().expect("store mutex poisoned");
        let n = conn.execute(
            "UPDATE captures
             SET destination_id = ?1,
                 routed_at = ?2,
                 read_at = COALESCE(read_at, ?2)
             WHERE id = ?3 AND deleted_at IS NULL",
            params![destination_id, &now, &id_str],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound(id_str));
        }
        drop(conn);
        self.refresh_dump(id);
        Ok(())
    }

    /// Un-route a Capture back to the Inbox. Clears `destination_id`
    /// and `routed_at`. `read_at` is preserved — un-routing does not
    /// pretend the user never saw the capture. Returns `NotFound`
    /// when no matching non-deleted row exists.
    pub fn capture_unroute(&self, id: &Ulid) -> Result<(), StoreError> {
        let id_str = id.to_string();
        let conn = self.conn.lock().expect("store mutex poisoned");
        let n = conn.execute(
            "UPDATE captures
             SET destination_id = NULL, routed_at = NULL
             WHERE id = ?1 AND deleted_at IS NULL",
            params![&id_str],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound(id_str));
        }
        drop(conn);
        self.refresh_dump(id);
        Ok(())
    }

    /// List Archive captures (non-deleted, Routed) newest-routed
    /// first, with optional filter by Destination id. `limit` caps
    /// the result. Sort is `routed_at DESC, id DESC` so re-routing a
    /// Capture surfaces it at the top.
    pub fn list_archive(
        &self,
        destination_filter: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Capture>, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut out = Vec::new();
        match destination_filter {
            Some(dest_id) => {
                let mut stmt = conn.prepare(
                    "SELECT id, kind, created_at, payload, source_app, starred, deleted_at, read_at, source_title, source_url, destination_id, routed_at
                     FROM captures
                     WHERE deleted_at IS NULL
                       AND destination_id IS NOT NULL
                       AND destination_id = ?1
                     ORDER BY routed_at DESC, id DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![dest_id, limit], row_to_capture)?;
                for row in rows {
                    out.push(row??);
                }
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, kind, created_at, payload, source_app, starred, deleted_at, read_at, source_title, source_url, destination_id, routed_at
                     FROM captures
                     WHERE deleted_at IS NULL
                       AND destination_id IS NOT NULL
                     ORDER BY routed_at DESC, id DESC
                     LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit], row_to_capture)?;
                for row in rows {
                    out.push(row??);
                }
            }
        }
        Ok(out)
    }

    /// FTS search scoped to the Archive (Routed, non-deleted). Mirror
    /// of `search` but on the opposite side of the Inbox/Archive
    /// split. Optional `destination_filter` narrows to one
    /// Destination.
    pub fn search_archive(
        &self,
        query: &str,
        destination_filter: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Capture>, StoreError> {
        let Some(match_expr) = build_fts_match(query) else {
            return Ok(Vec::new());
        };
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut out = Vec::new();
        match destination_filter {
            Some(dest_id) => {
                let mut stmt = conn.prepare(
                    "SELECT c.id, c.kind, c.created_at, c.payload, c.source_app, c.starred, c.deleted_at, c.read_at, c.source_title, c.source_url, c.destination_id, c.routed_at
                     FROM captures c
                     JOIN captures_fts f ON f.capture_id = c.id
                     WHERE captures_fts MATCH ?1
                       AND c.deleted_at IS NULL
                       AND c.destination_id IS NOT NULL
                       AND c.destination_id = ?2
                     ORDER BY rank, c.routed_at DESC, c.id DESC
                     LIMIT ?3",
                )?;
                let rows =
                    stmt.query_map(params![&match_expr, dest_id, limit], row_to_capture)?;
                for row in rows {
                    out.push(row??);
                }
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT c.id, c.kind, c.created_at, c.payload, c.source_app, c.starred, c.deleted_at, c.read_at, c.source_title, c.source_url, c.destination_id, c.routed_at
                     FROM captures c
                     JOIN captures_fts f ON f.capture_id = c.id
                     WHERE captures_fts MATCH ?1
                       AND c.deleted_at IS NULL
                       AND c.destination_id IS NOT NULL
                     ORDER BY rank, c.routed_at DESC, c.id DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![&match_expr, limit], row_to_capture)?;
                for row in rows {
                    out.push(row??);
                }
            }
        }
        Ok(out)
    }

    /// Test helper: returns the row by id ignoring the deleted_at flag.
    /// Lives behind the public surface so soft-delete tests can assert
    /// the tombstone is still in the table.
    #[doc(hidden)]
    pub fn find_with_deleted(&self, id: &Ulid) -> Result<Option<Capture>, StoreError> {
        let id_str = id.to_string();
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, kind, created_at, payload, source_app, starred, deleted_at, read_at, source_title, source_url, destination_id, routed_at
             FROM captures
             WHERE id = ?1",
        )?;
        let row = stmt
            .query_row(params![&id_str], row_to_capture)
            .optional()?;
        match row {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }
}

fn row_to_capture(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Capture, StoreError>> {
    let id: String = row.get(0)?;
    let kind_str: String = row.get(1)?;
    let created_at: String = row.get(2)?;
    let payload_str: String = row.get(3)?;
    let source_app: Option<String> = row.get(4)?;
    let starred: i64 = row.get(5)?;
    let deleted_at: Option<String> = row.get(6)?;
    let read_at: Option<String> = row.get(7)?;
    let source_title: Option<String> = row.get(8)?;
    let source_url: Option<String> = row.get(9)?;
    let destination_id: Option<String> = row.get(10)?;
    let routed_at: Option<String> = row.get(11)?;

    let result = (|| -> Result<Capture, StoreError> {
        let kind = CaptureKind::parse(&kind_str)?;
        let payload: serde_json::Value = serde_json::from_str(&payload_str)?;
        Ok(Capture {
            id,
            kind,
            created_at,
            payload,
            source_app,
            starred: starred != 0,
            deleted_at,
            read_at,
            source_title,
            source_url,
            destination_id,
            routed_at,
        })
    })();
    Ok(result)
}

fn row_to_destination(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Result<Destination, StoreError>> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let color: Option<String> = row.get(2)?;
    let created_at: String = row.get(3)?;
    let deleted_at: Option<String> = row.get(4)?;
    Ok(Ok(Destination {
        id,
        name,
        color,
        created_at,
        deleted_at,
    }))
}

/// Normalize a user-supplied color into the storage shape: trim, drop
/// to `None` when blank. The palette key set is enforced by the UI;
/// the store accepts any non-blank string so the palette can grow
/// without a DB migration.
fn normalize_color(color: Option<&str>) -> Option<String> {
    color
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(String::from)
}

/// Detects a SQLite UNIQUE-constraint violation. Used to map a failed
/// destination INSERT/UPDATE into a `DestinationNameConflict`.
fn is_unique_violation(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::ConstraintViolation,
                extended_code: rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE,
            },
            _
        )
    )
}

/// Flatten a `CaptureInput` into the text we want indexed by FTS5.
/// Per-kind so we capture every searchable surface (URLs for Link,
/// file names for File, path for Shot, plain text for Note / Clip)
/// without dumping the raw JSON keys into the index.
fn searchable_text_for_input(input: &CaptureInput) -> String {
    match input {
        CaptureInput::Note { text } | CaptureInput::Clip { text } => text.clone(),
        CaptureInput::Link {
            url,
            raw_text,
            title,
        } => {
            let mut s = String::new();
            s.push_str(url);
            if raw_text != url {
                s.push(' ');
                s.push_str(raw_text);
            }
            if let Some(t) = title {
                s.push(' ');
                s.push_str(t);
            }
            s
        }
        CaptureInput::File {
            source_path,
            original_name,
            ..
        } => {
            let mut s = String::new();
            if let Some(name) = original_name {
                s.push_str(name);
                s.push(' ');
            }
            s.push_str(&source_path.to_string_lossy());
            s
        }
        CaptureInput::Shot { source, .. } => match source {
            ShotSource::Path { source_path, .. } => source_path.to_string_lossy().into_owned(),
            // Image bytes have no on-disk source name yet; the
            // blob_path the caller assigns is uninteresting to a
            // human search.
            ShotSource::Bytes { .. } => String::new(),
        },
    }
}

/// Sanitise an arbitrary user-typed query into a safe FTS5 MATCH
/// expression. We split on whitespace, strip non-alphanumeric chars
/// from each token (FTS5's grammar bristles at `/`, `:`, quotes, etc.
/// from a raw URL paste), append `*` for prefix matching, then AND
/// the tokens together. Returns `None` when the query has no
/// indexable tokens (whitespace only, punctuation only) so the
/// caller can short-circuit to an empty result instead of running a
/// MATCH that would error.
fn build_fts_match(query: &str) -> Option<String> {
    // Split on every non-alphanumeric run so a pasted URL like
    // "https://example.com" matches the same way FTS5's unicode61
    // tokenizer split the indexed text ("https", "example", "com").
    let tokens: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("{}*", w))
        .collect();
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" "))
    }
}

fn now_rfc3339() -> String {
    // We avoid pulling chrono in for one timestamp. Format: UTC ISO8601
    // with millisecond precision, e.g. "2025-05-16T12:34:56.789Z".
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let millis = dur.subsec_millis();

    let days_since_epoch = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400);
    let hour = (secs_of_day / 3600) as u32;
    let minute = ((secs_of_day % 3600) / 60) as u32;
    let second = (secs_of_day % 60) as u32;
    let (year, month, day) = civil_from_days(days_since_epoch);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hour, minute, second, millis
    )
}

/// Howard Hinnant's `civil_from_days` algorithm. Returns (year, month, day).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = (y + if m <= 2 { 1 } else { 0 }) as i32;
    (year, m as u32, d as u32)
}

/// Pick a file extension for a blob given its mime. Hand-rolled rather
/// than reaching for `mime_guess` because we only persist a handful of
/// mimes today; `mime_guess` is for the reverse direction (path -> mime)
/// that `kind_detect` already uses.
fn extension_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/heic" => "heic",
        "image/tiff" => "tiff",
        _ => "bin",
    }
}

pub fn default_db_path() -> Result<PathBuf, StoreError> {
    let home = std::env::var("HOME").map_err(|_| {
        StoreError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "HOME env var not set",
        ))
    })?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("com.koko.quick-capture")
        .join("captures.db"))
}
