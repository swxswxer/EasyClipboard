use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use tokio_rusqlite::{
    rusqlite::{params, OptionalExtension},
    Connection,
};
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{
        CapturedClipboard, ClipboardItemDetail, ClipboardItemSummary, ClipboardKind, ClipboardPage,
        Group, Settings,
    },
};

#[derive(Clone)]
pub struct Database {
    connection: Connection,
    blob_dir: PathBuf,
}

impl Database {
    pub async fn open(app_data_dir: &Path) -> Result<Self, AppError> {
        std::fs::create_dir_all(app_data_dir)?;
        let blob_dir = app_data_dir.join("blobs");
        std::fs::create_dir_all(&blob_dir)?;
        let connection = Connection::open(app_data_dir.join("easyclipboard.sqlite3")).await?;
        let database = Self {
            connection,
            blob_dir,
        };
        database.migrate().await?;
        database.cleanup_orphan_blobs().await?;
        Ok(database)
    }

    async fn migrate(&self) -> Result<(), AppError> {
        let default_settings = serde_json::to_string(&Settings::default())
            .map_err(|error| AppError::Storage(error.to_string()))?;
        self.connection.call(move |connection| {
            connection.execute_batch(
                r#"
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = NORMAL;
                PRAGMA foreign_keys = ON;
                PRAGMA busy_timeout = 3000;

                CREATE TABLE IF NOT EXISTS groups (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    sort_order INTEGER NOT NULL,
                    created_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS clipboard_items (
                    id TEXT PRIMARY KEY,
                    kind TEXT NOT NULL CHECK(kind IN ('text', 'image', 'files')),
                    title TEXT NOT NULL,
                    content TEXT NOT NULL DEFAULT '',
                    source_name TEXT NOT NULL DEFAULT '',
                    source_bundle_id TEXT,
                    content_hash TEXT NOT NULL,
                    copied_at INTEGER NOT NULL,
                    byte_size INTEGER NOT NULL DEFAULT 0,
                    pinned INTEGER NOT NULL DEFAULT 0,
                    group_id TEXT REFERENCES groups(id) ON DELETE SET NULL,
                    files_json TEXT NOT NULL DEFAULT '[]',
                    image_path TEXT,
                    thumbnail_path TEXT,
                    missing_files INTEGER NOT NULL DEFAULT 0
                );

                CREATE INDEX IF NOT EXISTS idx_clipboard_items_time ON clipboard_items(copied_at DESC);
                CREATE INDEX IF NOT EXISTS idx_clipboard_items_group_time ON clipboard_items(group_id, copied_at DESC);

                CREATE TABLE IF NOT EXISTS representations (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    item_id TEXT NOT NULL REFERENCES clipboard_items(id) ON DELETE CASCADE,
                    uti TEXT NOT NULL,
                    inline_text TEXT,
                    blob_data BLOB,
                    relative_path TEXT
                );

                CREATE VIRTUAL TABLE IF NOT EXISTS clipboard_fts USING fts5(
                    item_id UNINDEXED,
                    search_text,
                    tokenize='unicode61'
                );

                CREATE TABLE IF NOT EXISTS settings (
                    id INTEGER PRIMARY KEY CHECK(id = 1),
                    value_json TEXT NOT NULL
                );

                "#,
            )?;
            let schema_version: i64 =
                connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            if schema_version < 2 {
                let transaction = connection.transaction()?;
                transaction.execute_batch(
                    r#"
                    CREATE TEMP TABLE dedupe_survivors (
                        content_hash TEXT PRIMARY KEY,
                        item_id TEXT NOT NULL
                    );

                    INSERT INTO dedupe_survivors(content_hash, item_id)
                    SELECT content_hash, id
                    FROM (
                        SELECT id, content_hash,
                               ROW_NUMBER() OVER (
                                   PARTITION BY content_hash
                                   ORDER BY (pinned = 1 OR group_id IS NOT NULL) DESC,
                                            copied_at ASC,
                                            id ASC
                               ) AS duplicate_rank
                        FROM clipboard_items
                    )
                    WHERE duplicate_rank = 1;

                    UPDATE clipboard_items AS survivor
                    SET copied_at = (
                            SELECT MAX(item.copied_at)
                            FROM clipboard_items item
                            WHERE item.content_hash = survivor.content_hash
                        ),
                        source_name = (
                            SELECT item.source_name
                            FROM clipboard_items item
                            WHERE item.content_hash = survivor.content_hash
                            ORDER BY item.copied_at DESC, item.id DESC
                            LIMIT 1
                        ),
                        source_bundle_id = (
                            SELECT item.source_bundle_id
                            FROM clipboard_items item
                            WHERE item.content_hash = survivor.content_hash
                            ORDER BY item.copied_at DESC, item.id DESC
                            LIMIT 1
                        ),
                        pinned = (
                            SELECT MAX(item.pinned)
                            FROM clipboard_items item
                            WHERE item.content_hash = survivor.content_hash
                        ),
                        group_id = COALESCE(
                            survivor.group_id,
                            (
                                SELECT item.group_id
                                FROM clipboard_items item
                                WHERE item.content_hash = survivor.content_hash
                                  AND item.group_id IS NOT NULL
                                ORDER BY item.copied_at DESC, item.id DESC
                                LIMIT 1
                            )
                        )
                    WHERE survivor.id IN (SELECT item_id FROM dedupe_survivors);

                    DELETE FROM clipboard_fts
                    WHERE item_id IN (
                        SELECT item.id
                        FROM clipboard_items item
                        JOIN dedupe_survivors survivor USING(content_hash)
                        WHERE item.id <> survivor.item_id
                    );

                    DELETE FROM clipboard_items
                    WHERE id IN (
                        SELECT item.id
                        FROM clipboard_items item
                        JOIN dedupe_survivors survivor USING(content_hash)
                        WHERE item.id <> survivor.item_id
                    );

                    DROP TABLE dedupe_survivors;
                    CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_items_content_hash
                        ON clipboard_items(content_hash);
                    PRAGMA user_version = 2;
                    "#,
                )?;
                transaction.commit()?;
            }
            if schema_version < 3 {
                let transaction = connection.transaction()?;
                transaction.execute_batch(
                    r#"
                    ALTER TABLE clipboard_items RENAME COLUMN source_bundle_id TO source_app_id;
                    ALTER TABLE representations RENAME COLUMN uti TO format_name;
                    UPDATE representations
                    SET format_name = CASE format_name
                        WHEN 'public.utf8-plain-text' THEN 'text/plain'
                        WHEN 'public.html' THEN 'text/html'
                        WHEN 'public.rtf' THEN 'text/rtf'
                        ELSE format_name
                    END;
                    PRAGMA user_version = 3;
                    "#,
                )?;
                transaction.commit()?;
            }
            connection.execute(
                "INSERT OR IGNORE INTO settings(id, value_json) VALUES(1, ?1)",
                params![default_settings],
            )?;
            Ok(())
        }).await?;
        Ok(())
    }

    pub async fn list_items(
        &self,
        query: String,
        group_id: Option<String>,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<ClipboardPage, AppError> {
        let offset = cursor
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let limit = limit.clamp(1, 100);
        let trimmed = query.trim().to_owned();
        self.connection.call(move |connection| {
            let mut items = Vec::new();
            if trimmed.is_empty() {
                let mut statement = connection.prepare(
                    "SELECT id, kind, title, source_name, source_app_id, copied_at, byte_size, pinned, group_id, missing_files
                     FROM clipboard_items
                     WHERE (?1 IS NULL OR group_id = ?1)
                     ORDER BY copied_at DESC, id DESC
                     LIMIT ?2 OFFSET ?3",
                )?;
                let rows = statement.query_map(params![group_id, limit, offset], summary_from_row)?;
                for row in rows { items.push(row?); }
            } else {
                let fts_query = format!("\"{}\"*", trimmed.replace('"', "\"\""));
                let mut statement = connection.prepare(
                    "SELECT i.id, i.kind, i.title, i.source_name, i.source_app_id, i.copied_at, i.byte_size, i.pinned, i.group_id, i.missing_files
                     FROM clipboard_items i
                     JOIN clipboard_fts f ON f.item_id = i.id
                     WHERE f.search_text MATCH ?1 AND (?2 IS NULL OR i.group_id = ?2)
                     ORDER BY i.copied_at DESC, i.id DESC
                     LIMIT ?3 OFFSET ?4",
                )?;
                let rows = statement.query_map(params![fts_query, group_id, limit, offset], summary_from_row)?;
                for row in rows { items.push(row?); }
            }
            let next_cursor = (items.len() == limit as usize).then(|| (offset + limit).to_string());
            Ok(ClipboardPage { items, next_cursor })
        }).await.map_err(AppError::from)
    }

    pub async fn get_item(&self, id: String) -> Result<ClipboardItemDetail, AppError> {
        let blob_dir = self.blob_dir.clone();
        self.connection.call(move |connection| {
            let result = connection.query_row(
                "SELECT id, kind, title, source_name, source_app_id, copied_at, byte_size, pinned, group_id,
                        missing_files, content, files_json, thumbnail_path
                 FROM clipboard_items WHERE id = ?1",
                params![id],
                |row| {
                    let summary = summary_from_row(row)?;
                    let content: String = row.get(10)?;
                    let files_json: String = row.get(11)?;
                    let thumbnail_path: Option<String> = row.get(12)?;
                    let files = serde_json::from_str(&files_json).unwrap_or_default();
                    let preview_data_url = thumbnail_path.and_then(|path| {
                        std::fs::read(blob_dir.join(path)).ok().map(|bytes| {
                            format!("data:image/png;base64,{}", STANDARD.encode(bytes))
                        })
                    });
                    Ok(ClipboardItemDetail { summary, content, preview_data_url, files })
                },
            ).optional()?;
            result.ok_or(tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows)
        }).await.map_err(|error| match error {
            tokio_rusqlite::Error::Error(tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows) => AppError::NotFound,
            other => AppError::from(other),
        })
    }

    pub async fn insert_capture(&self, captured: CapturedClipboard) -> Result<String, AppError> {
        let now = unix_millis();
        let id = Uuid::new_v4().to_string();
        let files_json = serde_json::to_string(&captured.files)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let mut image_path = None;
        let mut thumbnail_path = None;

        if let Some(image_png) = captured.image_png.as_ref() {
            let original_name = format!("{id}.png");
            let thumbnail_name = format!("{id}-thumb.png");
            std::fs::write(self.blob_dir.join(&original_name), image_png)?;
            let decoded = image::load_from_memory(image_png)
                .map_err(|error| AppError::Storage(error.to_string()))?;
            decoded
                .thumbnail(512, 512)
                .save(self.blob_dir.join(&thumbnail_name))
                .map_err(|error| AppError::Storage(error.to_string()))?;
            image_path = Some(original_name);
            thumbnail_path = Some(thumbnail_name);
        }

        let kind = captured.kind.as_str().to_owned();
        let title = captured.title.clone();
        let content = captured.content.clone();
        let source_name = captured.source_name.clone();
        let source_app_id = captured.source_app_id.clone();
        let hash = captured.hash.clone();
        let byte_size = captured.byte_size as i64;
        let html = captured.html.clone();
        let rtf = captured.rtf.clone();
        let search_text = format!(
            "{} {} {}",
            captured.title,
            captured.content,
            captured.files.join(" ")
        );
        let new_id = id.clone();
        let new_image_path = image_path.clone();
        let new_thumbnail_path = thumbnail_path.clone();

        let stored_id = self.connection.call(move |connection| {
            let effective_now: i64 = connection.query_row(
                "SELECT MAX(?1, COALESCE(MAX(copied_at), 0) + 1) FROM clipboard_items",
                params![now],
                |row| row.get(0),
            )?;
            let duplicate: Option<String> = connection.query_row(
                "SELECT id FROM clipboard_items
                 WHERE content_hash = ?1
                 LIMIT 1",
                params![hash],
                |row| row.get(0),
            ).optional()?;

            if let Some(existing_id) = duplicate {
                connection.execute(
                    "UPDATE clipboard_items SET copied_at = ?2, source_name = ?3, source_app_id = ?4 WHERE id = ?1",
                    params![existing_id, effective_now, source_name, source_app_id],
                )?;
                return Ok(existing_id);
            }

            let transaction = connection.transaction()?;
            transaction.execute(
                "INSERT INTO clipboard_items(
                    id, kind, title, content, source_name, source_app_id, content_hash, copied_at,
                    byte_size, files_json, image_path, thumbnail_path
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![new_id, kind, title, content, source_name, source_app_id, hash, effective_now,
                        byte_size, files_json, new_image_path, new_thumbnail_path],
            )?;
            transaction.execute(
                "INSERT INTO clipboard_fts(item_id, search_text) VALUES(?1, ?2)",
                params![new_id, search_text],
            )?;
            if !content.is_empty() {
                transaction.execute(
                    "INSERT INTO representations(item_id, format_name, inline_text) VALUES(?1, 'text/plain', ?2)",
                    params![new_id, content],
                )?;
            }
            if let Some(bytes) = html {
                transaction.execute(
                    "INSERT INTO representations(item_id, format_name, blob_data) VALUES(?1, 'text/html', ?2)",
                    params![new_id, bytes],
                )?;
            }
            if let Some(bytes) = rtf {
                transaction.execute(
                    "INSERT INTO representations(item_id, format_name, blob_data) VALUES(?1, 'text/rtf', ?2)",
                    params![new_id, bytes],
                )?;
            }
            transaction.commit()?;
            Ok(new_id)
        }).await?;

        if stored_id != id {
            if let Some(path) = image_path {
                let _ = std::fs::remove_file(self.blob_dir.join(path));
            }
            if let Some(path) = thumbnail_path {
                let _ = std::fs::remove_file(self.blob_dir.join(path));
            }
        }
        Ok(stored_id)
    }

    pub async fn touch_item(&self, id: String) -> Result<(), AppError> {
        let now = unix_millis();
        self.connection
            .call(move |connection| {
                let copied_at: i64 = connection.query_row(
                    "SELECT MAX(?1, COALESCE(MAX(copied_at), 0) + 1) FROM clipboard_items",
                    params![now],
                    |row| row.get(0),
                )?;
                let changed = connection.execute(
                    "UPDATE clipboard_items SET copied_at = ?2 WHERE id = ?1",
                    params![id, copied_at],
                )?;
                if changed == 0 {
                    return Err(tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows);
                }
                Ok::<(), tokio_rusqlite::rusqlite::Error>(())
            })
            .await
            .map_err(|error| match error {
                tokio_rusqlite::Error::Error(
                    tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows,
                ) => AppError::NotFound,
                other => AppError::from(other),
            })
    }

    pub async fn delete_item(&self, id: String) -> Result<(), AppError> {
        let paths = self
            .connection
            .call(move |connection| {
                let paths: Option<(Option<String>, Option<String>)> = connection
                    .query_row(
                        "SELECT image_path, thumbnail_path FROM clipboard_items WHERE id = ?1",
                        params![id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;
                let Some(paths) = paths else {
                    return Err(tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows);
                };
                let transaction = connection.transaction()?;
                transaction.execute("DELETE FROM clipboard_fts WHERE item_id = ?1", params![id])?;
                transaction.execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])?;
                transaction.commit()?;
                Ok(paths)
            })
            .await
            .map_err(|error| match error {
                tokio_rusqlite::Error::Error(
                    tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows,
                ) => AppError::NotFound,
                other => AppError::from(other),
            })?;
        for path in [paths.0, paths.1].into_iter().flatten() {
            let _ = std::fs::remove_file(self.blob_dir.join(path));
        }
        Ok(())
    }

    pub async fn set_pinned(&self, id: String, pinned: bool) -> Result<(), AppError> {
        self.connection
            .call(move |connection| {
                let changed = connection.execute(
                    "UPDATE clipboard_items SET pinned = ?2 WHERE id = ?1",
                    params![id, pinned],
                )?;
                if changed == 0 {
                    return Err(tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows);
                }
                Ok(())
            })
            .await
            .map_err(|error| match error {
                tokio_rusqlite::Error::Error(
                    tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows,
                ) => AppError::NotFound,
                other => AppError::from(other),
            })
    }

    pub async fn list_groups(&self) -> Result<Vec<Group>, AppError> {
        self.connection.call(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, name, sort_order, created_at FROM groups ORDER BY sort_order, created_at",
            )?;
            let rows = statement.query_map([], |row| Ok(Group {
                id: row.get(0)?,
                name: row.get(1)?,
                sort_order: row.get(2)?,
                created_at: millis_to_iso(row.get(3)?),
            }))?;
            rows.collect()
        }).await.map_err(AppError::from)
    }

    pub async fn create_group(&self, name: String) -> Result<Group, AppError> {
        let group = Group {
            id: Uuid::new_v4().to_string(),
            name: name.trim().chars().take(20).collect(),
            sort_order: 0,
            created_at: millis_to_iso(unix_millis()),
        };
        let result = group.clone();
        self.connection
            .call(move |connection| {
                let next_order: i64 = connection.query_row(
                    "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM groups",
                    [],
                    |row| row.get(0),
                )?;
                connection.execute(
                    "INSERT INTO groups(id, name, sort_order, created_at) VALUES(?1, ?2, ?3, ?4)",
                    params![group.id, group.name, next_order, unix_millis()],
                )?;
                Ok(())
            })
            .await?;
        Ok(result)
    }

    pub async fn rename_group(&self, id: String, name: String) -> Result<(), AppError> {
        let name: String = name.trim().chars().take(20).collect();
        self.connection
            .call(move |connection| {
                let changed = connection.execute(
                    "UPDATE groups SET name = ?2 WHERE id = ?1",
                    params![id, name],
                )?;
                if changed == 0 {
                    return Err(tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows);
                }
                Ok(())
            })
            .await
            .map_err(|error| match error {
                tokio_rusqlite::Error::Error(
                    tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows,
                ) => AppError::NotFound,
                other => AppError::from(other),
            })
    }

    pub async fn delete_group(&self, id: String) -> Result<(), AppError> {
        self.connection
            .call(move |connection| {
                let transaction = connection.transaction()?;
                transaction.execute(
                    "UPDATE clipboard_items SET group_id = NULL WHERE group_id = ?1",
                    params![id],
                )?;
                let changed =
                    transaction.execute("DELETE FROM groups WHERE id = ?1", params![id])?;
                if changed == 0 {
                    return Err(tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows);
                }
                transaction.commit()?;
                Ok(())
            })
            .await
            .map_err(|error| match error {
                tokio_rusqlite::Error::Error(
                    tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows,
                ) => AppError::NotFound,
                other => AppError::from(other),
            })
    }

    pub async fn move_item(
        &self,
        item_id: String,
        group_id: Option<String>,
    ) -> Result<(), AppError> {
        self.connection
            .call(move |connection| {
                if let Some(ref group_id) = group_id {
                    let exists: bool = connection.query_row(
                        "SELECT EXISTS(SELECT 1 FROM groups WHERE id = ?1)",
                        params![group_id],
                        |row| row.get(0),
                    )?;
                    if !exists {
                        return Err(tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows);
                    }
                }
                let changed = connection.execute(
                    "UPDATE clipboard_items SET group_id = ?2 WHERE id = ?1",
                    params![item_id, group_id],
                )?;
                if changed == 0 {
                    return Err(tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows);
                }
                Ok(())
            })
            .await
            .map_err(|error| match error {
                tokio_rusqlite::Error::Error(
                    tokio_rusqlite::rusqlite::Error::QueryReturnedNoRows,
                ) => AppError::NotFound,
                other => AppError::from(other),
            })
    }

    pub async fn get_settings(&self) -> Result<Settings, AppError> {
        self.connection
            .call(|connection| {
                let json: String = connection.query_row(
                    "SELECT value_json FROM settings WHERE id = 1",
                    [],
                    |row| row.get(0),
                )?;
                serde_json::from_str(&json).map_err(|error| {
                    tokio_rusqlite::rusqlite::Error::FromSqlConversionFailure(
                        0,
                        tokio_rusqlite::rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .await
            .map_err(AppError::from)
    }

    pub async fn save_settings(&self, settings: Settings) -> Result<Settings, AppError> {
        let json = serde_json::to_string(&settings)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let result = settings.clone();
        self.connection
            .call(move |connection| {
                connection.execute(
                    "UPDATE settings SET value_json = ?1 WHERE id = 1",
                    params![json],
                )?;
                Ok(())
            })
            .await?;
        Ok(result)
    }

    pub async fn clear_recent(&self) -> Result<(), AppError> {
        let paths = self.connection.call(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, image_path, thumbnail_path FROM clipboard_items WHERE group_id IS NULL AND pinned = 0",
            )?;
            let entries: Vec<(String, Option<String>, Option<String>)> = statement.query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?.collect::<Result<_, _>>()?;
            drop(statement);
            let transaction = connection.transaction()?;
            for (id, _, _) in &entries {
                transaction.execute("DELETE FROM clipboard_fts WHERE item_id = ?1", params![id])?;
            }
            transaction.execute("DELETE FROM clipboard_items WHERE group_id IS NULL AND pinned = 0", [])?;
            transaction.commit()?;
            Ok(entries)
        }).await?;
        self.remove_blob_paths(paths.into_iter().flat_map(|(_, a, b)| [a, b]));
        self.cleanup_orphan_blobs().await?;
        Ok(())
    }

    pub async fn delete_all_data(&self) -> Result<(), AppError> {
        let defaults = serde_json::to_string(&Settings::default())
            .map_err(|error| AppError::Storage(error.to_string()))?;
        self.connection
            .call(move |connection| {
                let transaction = connection.transaction()?;
                transaction.execute("DELETE FROM clipboard_fts", [])?;
                transaction.execute("DELETE FROM clipboard_items", [])?;
                transaction.execute("DELETE FROM groups", [])?;
                transaction.execute(
                    "UPDATE settings SET value_json = ?1 WHERE id = 1",
                    params![defaults],
                )?;
                transaction.commit()?;
                Ok(())
            })
            .await?;
        for entry in std::fs::read_dir(&self.blob_dir)?.flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
        Ok(())
    }

    pub async fn cleanup(&self, max_items: u32, retention_days: u32) -> Result<(), AppError> {
        let cutoff =
            (retention_days > 0).then(|| unix_millis() - i64::from(retention_days) * 86_400_000);
        let paths = self
            .connection
            .call(move |connection| {
                let mut ids = Vec::new();
                if let Some(cutoff) = cutoff {
                    let mut statement = connection.prepare(
                        "SELECT id, image_path, thumbnail_path FROM clipboard_items
                     WHERE pinned = 0 AND group_id IS NULL AND copied_at < ?1",
                    )?;
                    ids.extend(
                        statement
                            .query_map(params![cutoff], |row| {
                                Ok((
                                    row.get::<_, String>(0)?,
                                    row.get::<_, Option<String>>(1)?,
                                    row.get::<_, Option<String>>(2)?,
                                ))
                            })?
                            .collect::<Result<Vec<_>, _>>()?,
                    );
                }
                let mut statement = connection.prepare(
                    "SELECT id, image_path, thumbnail_path FROM clipboard_items
                 WHERE pinned = 0 AND group_id IS NULL
                 ORDER BY copied_at DESC LIMIT -1 OFFSET ?1",
                )?;
                ids.extend(
                    statement
                        .query_map(params![max_items], |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, Option<String>>(1)?,
                                row.get::<_, Option<String>>(2)?,
                            ))
                        })?
                        .collect::<Result<Vec<_>, _>>()?,
                );
                ids.sort_by(|left, right| left.0.cmp(&right.0));
                ids.dedup_by(|left, right| left.0 == right.0);
                drop(statement);
                let transaction = connection.transaction()?;
                for (id, _, _) in &ids {
                    transaction
                        .execute("DELETE FROM clipboard_fts WHERE item_id = ?1", params![id])?;
                    transaction
                        .execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])?;
                }
                transaction.commit()?;
                Ok(ids)
            })
            .await?;
        self.remove_blob_paths(paths.into_iter().flat_map(|(_, a, b)| [a, b]));
        self.cleanup_orphan_blobs().await?;
        Ok(())
    }

    pub async fn original_image(&self, id: String) -> Result<Vec<u8>, AppError> {
        let path = self
            .connection
            .call(move |connection| {
                connection.query_row(
                    "SELECT image_path FROM clipboard_items WHERE id = ?1",
                    params![id],
                    |row| row.get::<_, Option<String>>(0),
                )
            })
            .await?
            .ok_or(AppError::NotFound)?;
        Ok(std::fs::read(self.blob_dir.join(path))?)
    }

    pub async fn text_representations(
        &self,
        id: String,
    ) -> Result<(Option<Vec<u8>>, Option<Vec<u8>>), AppError> {
        self.connection.call(move |connection| {
            let mut html = None;
            let mut rtf = None;
            let mut statement = connection.prepare(
                "SELECT format_name, blob_data FROM representations WHERE item_id = ?1 AND format_name IN ('text/html', 'text/rtf')",
            )?;
            let rows = statement.query_map(params![id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<Vec<u8>>>(1)?))
            })?;
            for row in rows {
                let (format_name, bytes) = row?;
                if format_name == "text/html" {
                    html = bytes;
                } else if format_name == "text/rtf" {
                    rtf = bytes;
                }
            }
            Ok((html, rtf))
        }).await.map_err(AppError::from)
    }

    fn remove_blob_paths(&self, paths: impl Iterator<Item = Option<String>>) {
        for path in paths.flatten() {
            let _ = std::fs::remove_file(self.blob_dir.join(path));
        }
    }

    async fn cleanup_orphan_blobs(&self) -> Result<(), AppError> {
        let referenced: HashSet<String> = self.connection.call(|connection| {
            let mut statement = connection.prepare(
                "SELECT image_path, thumbnail_path FROM clipboard_items WHERE image_path IS NOT NULL OR thumbnail_path IS NOT NULL",
            )?;
            let mut paths = HashSet::new();
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?))
            })?;
            for row in rows {
                let (image, thumbnail) = row?;
                paths.extend([image, thumbnail].into_iter().flatten());
            }
            Ok(paths)
        }).await?;
        for entry in std::fs::read_dir(&self.blob_dir)? {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !referenced.contains(&name) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    }
}

fn summary_from_row(
    row: &tokio_rusqlite::rusqlite::Row<'_>,
) -> tokio_rusqlite::rusqlite::Result<ClipboardItemSummary> {
    let kind: String = row.get(1)?;
    let group_id: Option<String> = row.get(8)?;
    let pinned: bool = row.get(7)?;
    Ok(ClipboardItemSummary {
        id: row.get(0)?,
        kind: match kind.as_str() {
            "image" => ClipboardKind::Image,
            "files" => ClipboardKind::Files,
            _ => ClipboardKind::Text,
        },
        title: row.get(2)?,
        source_name: row.get(3)?,
        source_app_id: row.get(4)?,
        copied_at: millis_to_iso(row.get(5)?),
        byte_size: row.get::<_, i64>(6)? as u64,
        pinned,
        retained: pinned || group_id.is_some(),
        group_id,
        missing_files: row.get(9)?,
    })
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn millis_to_iso(value: i64) -> String {
    chrono::DateTime::from_timestamp_millis(value)
        .unwrap_or_default()
        .to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(text: &str) -> CapturedClipboard {
        CapturedClipboard {
            kind: ClipboardKind::Text,
            title: text.into(),
            content: text.into(),
            html: None,
            rtf: None,
            image_png: None,
            files: vec![],
            source_name: "Tests".into(),
            source_app_id: Some("tests".into()),
            byte_size: text.len() as u64,
            hash: format!("hash-{text}"),
        }
    }

    #[tokio::test]
    async fn image_file_capture_keeps_a_thumbnail_blob_and_original_path_representation() {
        let root = std::env::temp_dir().join(format!("easyclipboard-test-{}", Uuid::new_v4()));
        let db = Database::open(&root).await.unwrap();
        let mut writer = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            3,
            2,
            image::Rgba([30, 60, 90, 255]),
        ))
        .write_to(&mut writer, image::ImageFormat::Png)
        .unwrap();
        let png = writer.into_inner();
        let original_file = "C:\\Temp\\chat-image.png".to_owned();
        let id = db
            .insert_capture(CapturedClipboard {
                kind: ClipboardKind::Image,
                title: "chat-image.png · 3 × 2".into(),
                content: String::new(),
                html: None,
                rtf: None,
                image_png: Some(png.clone()),
                files: vec![original_file.clone()],
                source_name: "Chat".into(),
                source_app_id: Some("chat.exe".into()),
                byte_size: png.len() as u64,
                hash: "image-file-hash".into(),
            })
            .await
            .unwrap();

        let detail = db.get_item(id.clone()).await.unwrap();
        assert_eq!(detail.summary.kind, ClipboardKind::Image);
        assert_eq!(detail.files, vec![original_file]);
        assert!(detail
            .preview_data_url
            .as_deref()
            .is_some_and(|value| value.starts_with("data:image/png;base64,")));
        assert_eq!(db.original_image(id).await.unwrap(), png);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn grouped_and_pinned_items_survive_clear_recent() {
        let root = std::env::temp_dir().join(format!("easyclipboard-test-{}", Uuid::new_v4()));
        let db = Database::open(&root).await.unwrap();
        let ordinary = db.insert_capture(capture("ordinary")).await.unwrap();
        let grouped = db.insert_capture(capture("grouped")).await.unwrap();
        let pinned = db.insert_capture(capture("pinned")).await.unwrap();
        let group = db.create_group("Saved".into()).await.unwrap();
        db.move_item(grouped.clone(), Some(group.id)).await.unwrap();
        db.set_pinned(pinned.clone(), true).await.unwrap();
        db.clear_recent().await.unwrap();
        assert!(matches!(
            db.get_item(ordinary).await,
            Err(AppError::NotFound)
        ));
        assert!(db.get_item(grouped).await.is_ok());
        assert!(db.get_item(pinned).await.is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn deleting_group_moves_item_to_recent() {
        let root = std::env::temp_dir().join(format!("easyclipboard-test-{}", Uuid::new_v4()));
        let db = Database::open(&root).await.unwrap();
        let id = db.insert_capture(capture("saved")).await.unwrap();
        let group = db.create_group("Saved".into()).await.unwrap();
        db.move_item(id.clone(), Some(group.id.clone()))
            .await
            .unwrap();
        db.delete_group(group.id).await.unwrap();
        assert!(db.get_item(id).await.unwrap().summary.group_id.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn duplicate_hashes_always_merge_and_move_the_original_to_the_top() {
        let root = std::env::temp_dir().join(format!("easyclipboard-test-{}", Uuid::new_v4()));
        let db = Database::open(&root).await.unwrap();
        let first = db.insert_capture(capture("same")).await.unwrap();
        let other = db.insert_capture(capture("other")).await.unwrap();
        let second = db.insert_capture(capture("same")).await.unwrap();
        assert_eq!(first, second);
        let items = db
            .list_items(String::new(), None, None, 100)
            .await
            .unwrap()
            .items;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, first);
        assert_eq!(items[1].id, other);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn touching_a_pasted_item_moves_it_to_the_top_without_changing_its_identity() {
        let root = std::env::temp_dir().join(format!("easyclipboard-test-{}", Uuid::new_v4()));
        let db = Database::open(&root).await.unwrap();
        let old = db.insert_capture(capture("old")).await.unwrap();
        db.insert_capture(capture("new")).await.unwrap();
        db.touch_item(old.clone()).await.unwrap();
        let items = db
            .list_items(String::new(), None, None, 100)
            .await
            .unwrap()
            .items;
        assert_eq!(items[0].id, old);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn migration_collapses_existing_duplicates_and_preserves_retained_metadata() {
        let root = std::env::temp_dir().join(format!("easyclipboard-test-{}", Uuid::new_v4()));
        let db = Database::open(&root).await.unwrap();
        let original = db
            .insert_capture(capture("legacy duplicate"))
            .await
            .unwrap();
        let group = db.create_group("Saved".into()).await.unwrap();
        db.move_item(original.clone(), Some(group.id.clone()))
            .await
            .unwrap();
        let duplicate = Uuid::new_v4().to_string();
        let original_for_setup = original.clone();
        db.connection
            .call(move |connection| {
                connection.execute(
                    "DROP INDEX idx_clipboard_items_content_hash",
                    [],
                )?;
                connection.execute(
                    "INSERT INTO clipboard_items(
                        id, kind, title, content, source_name, source_app_id, content_hash,
                        copied_at, byte_size, pinned, group_id, files_json, image_path,
                        thumbnail_path, missing_files
                     )
                     SELECT ?1, kind, title, content, 'New Source', source_app_id, content_hash,
                            copied_at + 100, byte_size, 0, NULL, files_json, image_path,
                            thumbnail_path, missing_files
                     FROM clipboard_items WHERE id = ?2",
                    params![duplicate, original_for_setup],
                )?;
                connection.execute(
                    "INSERT INTO clipboard_fts(item_id, search_text) VALUES(?1, 'legacy duplicate')",
                    params![duplicate],
                )?;
                connection.execute_batch(
                    "ALTER TABLE clipboard_items RENAME COLUMN source_app_id TO source_bundle_id;
                     ALTER TABLE representations RENAME COLUMN format_name TO uti;",
                )?;
                connection.pragma_update(None, "user_version", 1)?;
                Ok::<(), tokio_rusqlite::rusqlite::Error>(())
            })
            .await
            .unwrap();
        drop(db);

        let reopened = Database::open(&root).await.unwrap();
        let items = reopened
            .list_items(String::new(), None, None, 100)
            .await
            .unwrap()
            .items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, original);
        assert_eq!(items[0].group_id.as_deref(), Some(group.id.as_str()));
        assert_eq!(items[0].source_name, "New Source");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn schema_v2_migrates_platform_identifiers_and_format_names() {
        let root = std::env::temp_dir().join(format!("easyclipboard-test-{}", Uuid::new_v4()));
        let db = Database::open(&root).await.unwrap();
        let id = db.insert_capture(capture("legacy schema")).await.unwrap();
        db.connection
            .call(|connection| {
                connection.execute_batch(
                    "ALTER TABLE clipboard_items RENAME COLUMN source_app_id TO source_bundle_id;
                     ALTER TABLE representations RENAME COLUMN format_name TO uti;
                     UPDATE representations SET uti = 'public.utf8-plain-text' WHERE uti = 'text/plain';
                     PRAGMA user_version = 2;",
                )?;
                Ok::<(), tokio_rusqlite::rusqlite::Error>(())
            })
            .await
            .unwrap();
        drop(db);

        let reopened = Database::open(&root).await.unwrap();
        let item = reopened.get_item(id.clone()).await.unwrap();
        assert_eq!(item.summary.source_app_id.as_deref(), Some("tests"));
        let format_name: String = reopened
            .connection
            .call(move |connection| {
                connection.query_row(
                    "SELECT format_name FROM representations WHERE item_id = ?1",
                    params![id],
                    |row| row.get(0),
                )
            })
            .await
            .unwrap();
        assert_eq!(format_name, "text/plain");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn fts_search_and_cursor_pagination_are_stable() {
        let root = std::env::temp_dir().join(format!("easyclipboard-test-{}", Uuid::new_v4()));
        let db = Database::open(&root).await.unwrap();
        db.insert_capture(capture("alpha one")).await.unwrap();
        db.insert_capture(capture("alpha two")).await.unwrap();
        db.insert_capture(capture("beta")).await.unwrap();
        let first = db.list_items("alpha".into(), None, None, 1).await.unwrap();
        assert_eq!(first.items.len(), 1);
        let second = db
            .list_items("alpha".into(), None, first.next_cursor, 1)
            .await
            .unwrap();
        assert_eq!(second.items.len(), 1);
        assert_ne!(first.items[0].id, second.items[0].id);
        let _ = std::fs::remove_dir_all(root);
    }
}
