use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use super::model::{EntryId, EntryKind, EntryMeta, Flavor, PREVIEW_CHARS, Thumbnail, Timestamp};
use super::settings::Settings;

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS entries (
    id            INTEGER PRIMARY KEY,
    hash          BLOB    NOT NULL UNIQUE,
    kind          INTEGER NOT NULL,
    preview       TEXT    NOT NULL,
    byte_size     INTEGER NOT NULL,
    source_app    TEXT,
    created_at    INTEGER NOT NULL,
    last_used_at  INTEGER NOT NULL,
    use_count     INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_entries_last_used ON entries(last_used_at DESC);
CREATE INDEX IF NOT EXISTS idx_entries_created   ON entries(created_at DESC);

CREATE TABLE IF NOT EXISTS contents (
    entry_id INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    mime     TEXT    NOT NULL,
    ordinal  INTEGER NOT NULL,
    body     BLOB    NOT NULL,
    PRIMARY KEY (entry_id, mime)
);

CREATE TABLE IF NOT EXISTS thumbnails (
    entry_id INTEGER PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE,
    width    INTEGER NOT NULL,
    height   INTEGER NOT NULL,
    png      BLOB    NOT NULL
);

CREATE TABLE IF NOT EXISTS pins (
    entry_id INTEGER PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE,
    position INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

pub struct NewEntry<'a> {
    pub hash: &'a [u8],
    pub kind: EntryKind,
    pub preview: &'a str,
    pub source_app: Option<&'a str>,
    pub flavors: &'a [Flavor],
    pub thumbnail: Option<&'a Thumbnail>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stored {
    Inserted(EntryId),
    Repeated(EntryId),
}

impl Stored {
    pub fn id(self) -> EntryId {
        match self {
            Self::Inserted(id) | Self::Repeated(id) => id,
        }
    }

    pub fn is_new(self) -> bool {
        matches!(self, Self::Inserted(_))
    }
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| io_to_sqlite(&error))?;
            restrict(parent, 0o700);
        }

        let conn = Connection::open(path)?;
        restrict(path, 0o600);
        Self::prepare(conn)
    }

    pub fn in_memory() -> rusqlite::Result<Self> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(conn: Connection) -> rusqlite::Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.execute_batch(SCHEMA)?;

        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .and_then(|value| value.parse().ok());

        match found {
            Some(version) if version == SCHEMA_VERSION => Ok(()),
            Some(version) => {
                tracing::warn!(
                    version,
                    expected = SCHEMA_VERSION,
                    "unexpected schema version"
                );
                Ok(())
            }
            None => {
                self.conn.execute(
                    "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
                    params![SCHEMA_VERSION.to_string()],
                )?;
                Ok(())
            }
        }
    }

    pub fn data_version(&self) -> rusqlite::Result<i64> {
        self.conn
            .pragma_query_value(None, "data_version", |row| row.get(0))
    }

    pub fn store(&mut self, entry: &NewEntry<'_>, now: Timestamp) -> rusqlite::Result<Stored> {
        let tx = self.conn.transaction()?;

        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM entries WHERE hash = ?1",
                params![entry.hash],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(id) = existing {
            tx.execute(
                "UPDATE entries SET last_used_at = ?2, use_count = use_count + 1 WHERE id = ?1",
                params![id, now],
            )?;
            tx.commit()?;
            return Ok(Stored::Repeated(EntryId(id)));
        }

        let byte_size: i64 = entry
            .flavors
            .iter()
            .filter_map(|flavor| i64::try_from(flavor.body.len()).ok())
            .sum();

        tx.execute(
            "INSERT INTO entries
                 (hash, kind, preview, byte_size, source_app, created_at, last_used_at, use_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 1)",
            params![
                entry.hash,
                entry.kind as i64,
                truncate_preview(entry.preview),
                byte_size,
                entry.source_app,
                now,
            ],
        )?;
        let id = tx.last_insert_rowid();

        {
            let mut insert = tx.prepare(
                "INSERT OR REPLACE INTO contents (entry_id, mime, ordinal, body)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (ordinal, flavor) in entry.flavors.iter().enumerate() {
                let ordinal = i64::try_from(ordinal).unwrap_or(i64::MAX);
                insert.execute(params![id, flavor.mime, ordinal, flavor.body])?;
            }
        }

        if let Some(thumbnail) = entry.thumbnail {
            tx.execute(
                "INSERT OR REPLACE INTO thumbnails (entry_id, width, height, png)
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, thumbnail.width, thumbnail.height, thumbnail.png],
            )?;
        }

        tx.commit()?;
        Ok(Stored::Inserted(EntryId(id)))
    }

    pub fn kind(&self, id: EntryId) -> rusqlite::Result<Option<EntryKind>> {
        self.conn
            .query_row(
                "SELECT kind FROM entries WHERE id = ?1",
                params![id.0],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map(|kind| kind.and_then(EntryKind::from_i64))
    }

    pub fn contains(&self, hash: &[u8]) -> rusqlite::Result<bool> {
        self.conn
            .query_row(
                "SELECT 1 FROM entries WHERE hash = ?1",
                params![hash],
                |_| Ok(()),
            )
            .optional()
            .map(|found| found.is_some())
    }

    pub fn list(&self, settings: &Settings) -> rusqlite::Result<Vec<EntryMeta>> {
        let sql = "SELECT e.id, e.kind, e.preview, e.byte_size, e.source_app, e.created_at,
                    e.last_used_at, e.use_count, p.position, t.width, t.height
             FROM entries e
             LEFT JOIN pins p       ON p.entry_id = e.id
             LEFT JOIN thumbnails t ON t.entry_id = e.id
             ORDER BY (p.entry_id IS NULL) ASC, p.position ASC, e.last_used_at DESC, e.id DESC
             LIMIT ?1 + (SELECT COUNT(*) FROM pins)";

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![i64::from(settings.max_entries)], |row| {
            let width: Option<u32> = row.get(9)?;
            let height: Option<u32> = row.get(10)?;
            Ok(EntryMeta {
                id: EntryId(row.get(0)?),
                kind: EntryKind::from_i64(row.get(1)?).unwrap_or(EntryKind::Text),
                preview: row.get(2)?,
                byte_size: row.get::<_, i64>(3)?.unsigned_abs(),
                source_app: row.get(4)?,
                created_at: row.get(5)?,
                last_used_at: row.get(6)?,
                use_count: row.get::<_, i64>(7)?.try_into().unwrap_or(u32::MAX),
                pinned: row
                    .get::<_, Option<i64>>(8)?
                    .map(|p| p.try_into().unwrap_or(0)),
                image_size: width.zip(height),
            })
        })?;

        rows.collect()
    }

    pub fn load(&self, id: EntryId, mime: Option<&str>) -> rusqlite::Result<Option<Flavor>> {
        let row = match mime {
            Some(mime) => self
                .conn
                .query_row(
                    "SELECT mime, body FROM contents WHERE entry_id = ?1 AND mime = ?2",
                    params![id.0, mime],
                    |row| Ok(Flavor::new(row.get::<_, String>(0)?, row.get(1)?)),
                )
                .optional()?,
            None => self
                .conn
                .query_row(
                    "SELECT mime, body FROM contents WHERE entry_id = ?1
                     ORDER BY ordinal ASC LIMIT 1",
                    params![id.0],
                    |row| Ok(Flavor::new(row.get::<_, String>(0)?, row.get(1)?)),
                )
                .optional()?,
        };
        Ok(row)
    }

    pub fn load_all(&self, id: EntryId) -> rusqlite::Result<Vec<Flavor>> {
        let mut stmt = self
            .conn
            .prepare("SELECT mime, body FROM contents WHERE entry_id = ?1 ORDER BY ordinal ASC")?;
        let rows = stmt.query_map(params![id.0], |row| {
            Ok(Flavor::new(row.get::<_, String>(0)?, row.get(1)?))
        })?;
        rows.collect()
    }

    pub fn thumbnail(&self, id: EntryId) -> rusqlite::Result<Option<Thumbnail>> {
        self.conn
            .query_row(
                "SELECT width, height, png FROM thumbnails WHERE entry_id = ?1",
                params![id.0],
                |row| {
                    Ok(Thumbnail {
                        width: row.get(0)?,
                        height: row.get(1)?,
                        png: row.get(2)?,
                    })
                },
            )
            .optional()
    }

    pub fn touch(&self, id: EntryId, now: Timestamp) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE entries SET last_used_at = ?2, use_count = use_count + 1 WHERE id = ?1",
            params![id.0, now],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: EntryId) -> rusqlite::Result<()> {
        self.conn
            .execute("DELETE FROM entries WHERE id = ?1", params![id.0])?;
        Ok(())
    }

    pub fn clear(&self, include_pinned: bool) -> rusqlite::Result<()> {
        if include_pinned {
            self.conn.execute("DELETE FROM entries", [])?;
        } else {
            self.conn.execute(
                "DELETE FROM entries
                 WHERE id NOT IN (SELECT entry_id FROM pins)",
                [],
            )?;
        }
        Ok(())
    }

    pub fn set_pinned(&self, id: EntryId, pinned: bool) -> rusqlite::Result<()> {
        if pinned {
            self.conn.execute(
                "INSERT OR IGNORE INTO pins (entry_id, position)
                 VALUES (?1, COALESCE((SELECT MAX(position) + 1 FROM pins), 0))",
                params![id.0],
            )?;
        } else {
            self.conn
                .execute("DELETE FROM pins WHERE entry_id = ?1", params![id.0])?;
        }
        Ok(())
    }

    pub fn prune(&self, settings: &Settings, now: Timestamp) -> rusqlite::Result<usize> {
        let mut removed = self.conn.execute(
            "DELETE FROM entries
             WHERE id NOT IN (SELECT entry_id FROM pins)
               AND id NOT IN (
                   SELECT id FROM entries
                   WHERE id NOT IN (SELECT entry_id FROM pins)
                   ORDER BY last_used_at DESC, id DESC
                   LIMIT ?1
               )",
            params![i64::from(settings.max_entries)],
        )?;

        if let Some(days) = settings.max_age_days {
            let cutoff = now - i64::from(days) * 86_400_000;
            removed += self.conn.execute(
                "DELETE FROM entries
                 WHERE id NOT IN (SELECT entry_id FROM pins) AND last_used_at < ?1",
                params![cutoff],
            )?;
        }

        Ok(removed)
    }
}

pub fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir);

    base.join("cosmic-clip-keep").join("history.db")
}

fn truncate_preview(preview: &str) -> String {
    match preview.char_indices().nth(PREVIEW_CHARS) {
        Some((cut, _)) => preview[..cut].to_owned(),
        None => preview.to_owned(),
    }
}

#[cfg(unix)]
fn restrict(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;

    if let Err(error) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)) {
        tracing::warn!(path = %path.display(), %error, "could not restrict permissions");
    }
}

#[cfg(not(unix))]
fn restrict(_path: &Path, _mode: u32) {}

fn io_to_sqlite(error: &std::io::Error) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
        Some(error.to_string()),
    )
}
