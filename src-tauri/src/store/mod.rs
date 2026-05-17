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
}

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Db(rusqlite::Error),
    Decode(String),
    NotFound(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "io error: {e}"),
            StoreError::Db(e) => write!(f, "db error: {e}"),
            StoreError::Decode(msg) => write!(f, "decode error: {msg}"),
            StoreError::NotFound(id) => write!(f, "capture not found: {id}"),
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
        Ok(Store {
            conn: std::sync::Mutex::new(conn),
            blobs_dir,
        })
    }

    /// Open the store at the default location:
    /// `~/Library/Application Support/com.koko.quick-capture/captures.db`.
    pub fn open_default() -> Result<Self, StoreError> {
        Self::open(default_db_path()?)
    }

    fn migrate(conn: &Connection) -> Result<(), StoreError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS captures (
                id TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL,
                created_at TEXT NOT NULL,
                payload TEXT NOT NULL,
                source_app TEXT,
                starred INTEGER NOT NULL DEFAULT 0,
                deleted_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_captures_created_at
                ON captures(created_at DESC);
            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    /// Persist a new Capture. The id is a freshly minted ULID and the
    /// `created_at` is now (UTC, RFC3339). For `Shot { source: Bytes }`
    /// the bytes are written to `blobs/<ulid>.<ext>` next to the DB and
    /// `payload.blob_path` records the resolved path.
    pub fn save(&self, input: CaptureInput) -> Result<Capture, StoreError> {
        let id = Ulid::new().to_string();
        let kind = input.kind();
        let created_at = now_rfc3339();
        let payload = self.build_payload(&id, &input)?;
        let payload_str = serde_json::to_string(&payload)?;

        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute(
            "INSERT INTO captures (id, kind, created_at, payload, source_app, starred, deleted_at)
             VALUES (?1, ?2, ?3, ?4, NULL, 0, NULL)",
            params![&id, kind.as_str(), &created_at, &payload_str],
        )?;

        Ok(Capture {
            id,
            kind,
            created_at,
            payload,
            source_app: None,
            starred: false,
            deleted_at: None,
        })
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

    /// Return up to `limit` non-deleted captures, newest first.
    pub fn list(&self, limit: u32) -> Result<Vec<Capture>, StoreError> {
        self.list_before(None, limit)
    }

    /// Cursor-paginated list. Returns up to `limit` non-deleted captures
    /// strictly older than `cursor` (when present), ordered newest first.
    /// `cursor = None` returns the first page. ULIDs are time-sortable, so
    /// `WHERE id < cursor` is equivalent to "older than".
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
                    "SELECT id, kind, created_at, payload, source_app, starred, deleted_at
                     FROM captures
                     WHERE deleted_at IS NULL AND id < ?1
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
                    "SELECT id, kind, created_at, payload, source_app, starred, deleted_at
                     FROM captures
                     WHERE deleted_at IS NULL
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
        Ok(())
    }

    /// Soft-delete a capture: stamp `deleted_at` so it stops surfacing
    /// in `list` but the row stays in the DB as a tombstone.
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
        Ok(())
    }

    /// Count non-deleted captures whose id is strictly greater than the
    /// given ULID string. Used by the Dock's unread-since-last-Inbox-open
    /// badge: the cursor is persisted in `app_settings` so the count
    /// survives restarts (PRD user story 24). Computed on demand rather
    /// than cached as a counter so deletes and sibling-process writes
    /// stay correct without a separate invalidation path.
    pub fn count_after(&self, cursor: &str) -> Result<u64, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM captures
             WHERE id > ?1 AND deleted_at IS NULL",
            params![cursor],
            |row| row.get(0),
        )?;
        Ok(n as u64)
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

    /// Test helper: returns the row by id ignoring the deleted_at flag.
    /// Lives behind the public surface so soft-delete tests can assert
    /// the tombstone is still in the table.
    #[doc(hidden)]
    pub fn find_with_deleted(&self, id: &Ulid) -> Result<Option<Capture>, StoreError> {
        let id_str = id.to_string();
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, kind, created_at, payload, source_app, starred, deleted_at
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
        })
    })();
    Ok(result)
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

fn default_db_path() -> Result<PathBuf, StoreError> {
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
