use anyhow::{anyhow, Result};
use chrono::{DateTime, Local, Utc};
use log::{debug, error, info};
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_specta::Event;

/// Database migrations for transcription history.
/// Each migration is applied in order. The library tracks which migrations
/// have been applied using SQLite's user_version pragma.
///
/// Note: For users upgrading from tauri-plugin-sql, migrate_from_tauri_plugin_sql()
/// converts the old _sqlx_migrations table tracking to the user_version pragma,
/// ensuring migrations don't re-run on existing databases.
static MIGRATIONS: &[M] = &[
    M::up(
        "CREATE TABLE IF NOT EXISTS transcription_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_name TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            saved BOOLEAN NOT NULL DEFAULT 0,
            title TEXT NOT NULL,
            transcription_text TEXT NOT NULL
        );",
    ),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_processed_text TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_process_prompt TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_process_requested BOOLEAN NOT NULL DEFAULT 0;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN adaptive_profile_id TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN adaptive_profile_name TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN adaptive_routing_json TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN adaptive_context_json TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN adaptive_language_json TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN adaptive_insertion_json TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN adaptive_parent_entry_id INTEGER;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN transform_action TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN transform_original_text TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN transform_result_text TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN transform_target_language TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN transform_provider_id TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN transform_model TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN transform_recovery_status TEXT;"),
];

const HISTORY_COLUMNS: &str = "id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, post_process_requested, adaptive_profile_id, adaptive_profile_name, adaptive_routing_json, adaptive_context_json, adaptive_language_json, adaptive_insertion_json, adaptive_parent_entry_id, transform_action, transform_original_text, transform_result_text, transform_target_language, transform_provider_id, transform_model, transform_recovery_status";

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct PaginatedHistory {
    pub entries: Vec<HistoryEntry>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(tag = "action")]
pub enum HistoryUpdatePayload {
    #[serde(rename = "added")]
    Added { entry: HistoryEntry },
    #[serde(rename = "updated")]
    Updated { entry: HistoryEntry },
    #[serde(rename = "deleted")]
    Deleted { id: i64 },
    #[serde(rename = "toggled")]
    Toggled { id: i64 },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HistoryEntry {
    pub id: i64,
    pub file_name: String,
    pub timestamp: i64,
    pub saved: bool,
    pub title: String,
    pub transcription_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
    pub post_process_requested: bool,
    pub adaptive_profile_id: Option<String>,
    pub adaptive_profile_name: Option<String>,
    pub adaptive_routing_json: Option<String>,
    pub adaptive_context_json: Option<String>,
    pub adaptive_language_json: Option<String>,
    pub adaptive_insertion_json: Option<String>,
    pub adaptive_parent_entry_id: Option<i64>,
    pub transform_action: Option<String>,
    pub transform_original_text: Option<String>,
    pub transform_result_text: Option<String>,
    pub transform_target_language: Option<String>,
    pub transform_provider_id: Option<String>,
    pub transform_model: Option<String>,
    pub transform_recovery_status: Option<String>,
}

pub struct AdaptiveHistoryMetadata {
    pub profile_id: Option<String>,
    pub profile_name: Option<String>,
    pub routing_json: Option<String>,
    pub context_json: Option<String>,
    pub language_json: Option<String>,
    pub insertion_json: Option<String>,
    pub parent_entry_id: Option<i64>,
}

impl AdaptiveHistoryMetadata {
    pub fn empty() -> Self {
        Self {
            profile_id: None,
            profile_name: None,
            routing_json: None,
            context_json: None,
            language_json: None,
            insertion_json: None,
            parent_entry_id: None,
        }
    }
}

pub struct TransformHistoryMetadata {
    pub action: Option<String>,
    pub original_text: Option<String>,
    pub result_text: Option<String>,
    pub target_language: Option<String>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub recovery_status: Option<String>,
}

impl TransformHistoryMetadata {
    pub fn empty() -> Self {
        Self {
            action: None,
            original_text: None,
            result_text: None,
            target_language: None,
            provider_id: None,
            model: None,
            recovery_status: None,
        }
    }
}

pub struct HistoryManager {
    app_handle: AppHandle,
    recordings_dir: PathBuf,
    db_path: PathBuf,
}

impl HistoryManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        // Create recordings directory in app data dir
        let app_data_dir = crate::portable::app_data_dir(app_handle)?;
        let recordings_dir = app_data_dir.join("recordings");
        let db_path = app_data_dir.join("history.db");

        // Ensure recordings directory exists
        if !recordings_dir.exists() {
            fs::create_dir_all(&recordings_dir)?;
            debug!("Created recordings directory: {:?}", recordings_dir);
        }

        let manager = Self {
            app_handle: app_handle.clone(),
            recordings_dir,
            db_path,
        };

        // Initialize database and run migrations synchronously
        manager.init_database()?;

        Ok(manager)
    }

    fn init_database(&self) -> Result<()> {
        info!("Initializing database at {:?}", self.db_path);

        let mut conn = Connection::open(&self.db_path)?;

        // Handle migration from tauri-plugin-sql to rusqlite_migration
        // tauri-plugin-sql used _sqlx_migrations table, rusqlite_migration uses user_version pragma
        self.migrate_from_tauri_plugin_sql(&conn)?;

        // Create migrations object and run to latest version
        let migrations = Migrations::new(MIGRATIONS.to_vec());

        // Validate migrations in debug builds
        #[cfg(debug_assertions)]
        migrations.validate().expect("Invalid migrations");

        // Get current version before migration
        let version_before: i32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        debug!("Database version before migration: {}", version_before);

        // Apply any pending migrations
        migrations.to_latest(&mut conn)?;

        // Get version after migration
        let version_after: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

        if version_after > version_before {
            info!(
                "Database migrated from version {} to {}",
                version_before, version_after
            );
        } else {
            debug!("Database already at latest version {}", version_after);
        }

        Ok(())
    }

    /// Migrate from tauri-plugin-sql's migration tracking to rusqlite_migration's.
    /// tauri-plugin-sql used a _sqlx_migrations table, while rusqlite_migration uses
    /// SQLite's user_version pragma. This function checks if the old system was in use
    /// and sets the user_version accordingly so migrations don't re-run.
    fn migrate_from_tauri_plugin_sql(&self, conn: &Connection) -> Result<()> {
        // Check if the old _sqlx_migrations table exists
        let has_sqlx_migrations: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_sqlx_migrations {
            return Ok(());
        }

        // Check current user_version
        let current_version: i32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

        if current_version > 0 {
            // Already migrated to rusqlite_migration system
            return Ok(());
        }

        // Get the highest version from the old migrations table
        let old_version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if old_version > 0 {
            info!(
                "Migrating from tauri-plugin-sql (version {}) to rusqlite_migration",
                old_version
            );

            // Set user_version to match the old migration state
            conn.pragma_update(None, "user_version", old_version)?;

            // Optionally drop the old migrations table (keeping it doesn't hurt)
            // conn.execute("DROP TABLE IF EXISTS _sqlx_migrations", [])?;

            info!(
                "Migration tracking converted: user_version set to {}",
                old_version
            );
        }

        Ok(())
    }

    fn get_connection(&self) -> Result<Connection> {
        Ok(Connection::open(&self.db_path)?)
    }

    fn map_history_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
        Ok(HistoryEntry {
            id: row.get("id")?,
            file_name: row.get("file_name")?,
            timestamp: row.get("timestamp")?,
            saved: row.get("saved")?,
            title: row.get("title")?,
            transcription_text: row.get("transcription_text")?,
            post_processed_text: row.get("post_processed_text")?,
            post_process_prompt: row.get("post_process_prompt")?,
            post_process_requested: row.get("post_process_requested")?,
            adaptive_profile_id: row.get("adaptive_profile_id")?,
            adaptive_profile_name: row.get("adaptive_profile_name")?,
            adaptive_routing_json: row.get("adaptive_routing_json")?,
            adaptive_context_json: row.get("adaptive_context_json")?,
            adaptive_language_json: row.get("adaptive_language_json")?,
            adaptive_insertion_json: row.get("adaptive_insertion_json")?,
            adaptive_parent_entry_id: row.get("adaptive_parent_entry_id")?,
            transform_action: row.get("transform_action")?,
            transform_original_text: row.get("transform_original_text")?,
            transform_result_text: row.get("transform_result_text")?,
            transform_target_language: row.get("transform_target_language")?,
            transform_provider_id: row.get("transform_provider_id")?,
            transform_model: row.get("transform_model")?,
            transform_recovery_status: row.get("transform_recovery_status")?,
        })
    }

    pub fn recordings_dir(&self) -> &std::path::Path {
        &self.recordings_dir
    }

    /// Save a new history entry to the database.
    /// The WAV file should already have been written to the recordings directory.
    pub fn save_entry(
        &self,
        file_name: String,
        transcription_text: String,
        post_process_requested: bool,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
    ) -> Result<HistoryEntry> {
        self.save_entry_with_metadata(
            file_name,
            transcription_text,
            post_process_requested,
            post_processed_text,
            post_process_prompt,
            AdaptiveHistoryMetadata::empty(),
        )
    }

    pub fn save_entry_with_metadata(
        &self,
        file_name: String,
        transcription_text: String,
        post_process_requested: bool,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
        metadata: AdaptiveHistoryMetadata,
    ) -> Result<HistoryEntry> {
        self.save_entry_with_all_metadata(
            file_name,
            transcription_text,
            post_process_requested,
            post_processed_text,
            post_process_prompt,
            metadata,
            TransformHistoryMetadata::empty(),
        )
    }

    pub fn save_transform_entry(
        &self,
        original_text: String,
        result_text: String,
        action: String,
        target_language: Option<String>,
        provider_id: Option<String>,
        model: Option<String>,
        recovery_status: String,
    ) -> Result<HistoryEntry> {
        let file_name = format!("transform-{}.txt", Utc::now().timestamp_millis());
        self.save_entry_with_all_metadata(
            file_name,
            original_text.clone(),
            false,
            Some(result_text.clone()),
            None,
            AdaptiveHistoryMetadata::empty(),
            TransformHistoryMetadata {
                action: Some(action),
                original_text: Some(original_text),
                result_text: Some(result_text),
                target_language,
                provider_id,
                model,
                recovery_status: Some(recovery_status),
            },
        )
    }

    fn save_entry_with_all_metadata(
        &self,
        file_name: String,
        transcription_text: String,
        post_process_requested: bool,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
        metadata: AdaptiveHistoryMetadata,
        transform_metadata: TransformHistoryMetadata,
    ) -> Result<HistoryEntry> {
        let timestamp = Utc::now().timestamp();
        let title = self.format_timestamp_title(timestamp);

        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                adaptive_profile_id,
                adaptive_profile_name,
                adaptive_routing_json,
                adaptive_context_json,
                adaptive_language_json,
                adaptive_insertion_json,
                adaptive_parent_entry_id,
                transform_action,
                transform_original_text,
                transform_result_text,
                transform_target_language,
                transform_provider_id,
                transform_model,
                transform_recovery_status
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
            params![
                &file_name,
                timestamp,
                false,
                &title,
                &transcription_text,
                &post_processed_text,
                &post_process_prompt,
                post_process_requested,
                &metadata.profile_id,
                &metadata.profile_name,
                &metadata.routing_json,
                &metadata.context_json,
                &metadata.language_json,
                &metadata.insertion_json,
                &metadata.parent_entry_id,
                &transform_metadata.action,
                &transform_metadata.original_text,
                &transform_metadata.result_text,
                &transform_metadata.target_language,
                &transform_metadata.provider_id,
                &transform_metadata.model,
                &transform_metadata.recovery_status,
            ],
        )?;

        let entry = HistoryEntry {
            id: conn.last_insert_rowid(),
            file_name,
            timestamp,
            saved: false,
            title,
            transcription_text,
            post_processed_text,
            post_process_prompt,
            post_process_requested,
            adaptive_profile_id: metadata.profile_id,
            adaptive_profile_name: metadata.profile_name,
            adaptive_routing_json: metadata.routing_json,
            adaptive_context_json: metadata.context_json,
            adaptive_language_json: metadata.language_json,
            adaptive_insertion_json: metadata.insertion_json,
            adaptive_parent_entry_id: metadata.parent_entry_id,
            transform_action: transform_metadata.action,
            transform_original_text: transform_metadata.original_text,
            transform_result_text: transform_metadata.result_text,
            transform_target_language: transform_metadata.target_language,
            transform_provider_id: transform_metadata.provider_id,
            transform_model: transform_metadata.model,
            transform_recovery_status: transform_metadata.recovery_status,
        };

        debug!("Saved history entry with id {}", entry.id);

        self.cleanup_old_entries()?;

        // Emit typed event for real-time frontend updates
        if let Err(e) = (HistoryUpdatePayload::Added {
            entry: entry.clone(),
        })
        .emit(&self.app_handle)
        {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(entry)
    }

    /// Update an existing history entry with new transcription results (used by retry).
    pub fn update_transcription(
        &self,
        id: i64,
        transcription_text: String,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
    ) -> Result<HistoryEntry> {
        let conn = self.get_connection()?;
        let updated = conn.execute(
            "UPDATE transcription_history
             SET transcription_text = ?1,
                 post_processed_text = ?2,
                 post_process_prompt = ?3
             WHERE id = ?4",
            params![
                transcription_text,
                post_processed_text,
                post_process_prompt,
                id
            ],
        )?;

        if updated == 0 {
            return Err(anyhow!("History entry {} not found", id));
        }

        let query = format!(
            "SELECT {} FROM transcription_history WHERE id = ?1",
            HISTORY_COLUMNS
        );
        let entry = conn.query_row(&query, params![id], Self::map_history_entry)?;

        debug!("Updated transcription for history entry {}", id);

        if let Err(e) = (HistoryUpdatePayload::Updated {
            entry: entry.clone(),
        })
        .emit(&self.app_handle)
        {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(entry)
    }

    pub fn cleanup_old_entries(&self) -> Result<()> {
        let retention_period = crate::settings::get_recording_retention_period(&self.app_handle);

        match retention_period {
            crate::settings::RecordingRetentionPeriod::Never => {
                // Don't delete anything
                return Ok(());
            }
            crate::settings::RecordingRetentionPeriod::PreserveLimit => {
                // Use the old count-based logic with history_limit
                let limit = crate::settings::get_history_limit(&self.app_handle);
                return self.cleanup_by_count(limit);
            }
            _ => {
                // Use time-based logic
                return self.cleanup_by_time(retention_period);
            }
        }
    }

    fn delete_entries_and_files(&self, entries: &[(i64, String)]) -> Result<usize> {
        if entries.is_empty() {
            return Ok(0);
        }

        let conn = self.get_connection()?;
        let mut deleted_count = 0;

        for (id, file_name) in entries {
            // Delete database entry
            conn.execute(
                "DELETE FROM transcription_history WHERE id = ?1",
                params![id],
            )?;

            // Delete WAV file
            let file_path = self.recordings_dir.join(file_name);
            if file_path.exists() {
                if let Err(e) = fs::remove_file(&file_path) {
                    error!("Failed to delete WAV file {}: {}", file_name, e);
                } else {
                    debug!("Deleted old WAV file: {}", file_name);
                    deleted_count += 1;
                }
            }
        }

        Ok(deleted_count)
    }

    fn cleanup_by_count(&self, limit: usize) -> Result<()> {
        let conn = self.get_connection()?;
        let entries_to_delete = Self::count_cleanup_candidates(&conn, limit)?;
        let deleted_count = self.delete_entries_and_files(&entries_to_delete)?;

        if deleted_count > 0 {
            debug!("Cleaned up {} old history entries by count", deleted_count);
        }

        Ok(())
    }

    fn count_cleanup_candidates(conn: &Connection, limit: usize) -> Result<Vec<(i64, String)>> {
        // Count-based retention protects recording history. Text-only transform
        // entries stay visible but must not evict older WAV-backed dictation rows.
        let mut stmt = conn.prepare(
            "SELECT id, file_name
             FROM transcription_history
             WHERE saved = 0 AND transform_action IS NULL
             ORDER BY timestamp DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, String>("file_name")?))
        })?;

        let mut entries: Vec<(i64, String)> = Vec::new();
        for row in rows {
            entries.push(row?);
        }

        if entries.len() <= limit {
            return Ok(Vec::new());
        }

        Ok(entries.into_iter().skip(limit).collect())
    }

    fn cleanup_by_time(
        &self,
        retention_period: crate::settings::RecordingRetentionPeriod,
    ) -> Result<()> {
        let conn = self.get_connection()?;

        // Calculate cutoff timestamp (current time minus retention period)
        let now = Utc::now().timestamp();
        let cutoff_timestamp = match retention_period {
            crate::settings::RecordingRetentionPeriod::Days3 => now - (3 * 24 * 60 * 60), // 3 days in seconds
            crate::settings::RecordingRetentionPeriod::Weeks2 => now - (2 * 7 * 24 * 60 * 60), // 2 weeks in seconds
            crate::settings::RecordingRetentionPeriod::Months3 => now - (3 * 30 * 24 * 60 * 60), // 3 months in seconds (approximate)
            _ => unreachable!("Should not reach here"),
        };

        // Get all unsaved entries older than the cutoff timestamp
        let mut stmt = conn.prepare(
            "SELECT id, file_name FROM transcription_history WHERE saved = 0 AND timestamp < ?1",
        )?;

        let rows = stmt.query_map(params![cutoff_timestamp], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, String>("file_name")?))
        })?;

        let mut entries_to_delete: Vec<(i64, String)> = Vec::new();
        for row in rows {
            entries_to_delete.push(row?);
        }

        let deleted_count = self.delete_entries_and_files(&entries_to_delete)?;

        if deleted_count > 0 {
            debug!(
                "Cleaned up {} old history entries based on retention period",
                deleted_count
            );
        }

        Ok(())
    }

    pub async fn get_history_entries(
        &self,
        cursor: Option<i64>,
        limit: Option<usize>,
    ) -> Result<PaginatedHistory> {
        let conn = self.get_connection()?;
        let limit = limit.map(|l| l.min(100));

        let mut entries: Vec<HistoryEntry> = match (cursor, limit) {
            (Some(cursor_id), Some(lim)) => {
                let fetch_count = (lim + 1) as i64;
                let query = format!(
                    "SELECT {}
                     FROM transcription_history
                     WHERE id < ?1
                     ORDER BY id DESC
                     LIMIT ?2",
                    HISTORY_COLUMNS
                );
                let mut stmt = conn.prepare(&query)?;
                let result = stmt
                    .query_map(params![cursor_id, fetch_count], Self::map_history_entry)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                result
            }
            (None, Some(lim)) => {
                let fetch_count = (lim + 1) as i64;
                let query = format!(
                    "SELECT {}
                     FROM transcription_history
                     ORDER BY id DESC
                     LIMIT ?1",
                    HISTORY_COLUMNS
                );
                let mut stmt = conn.prepare(&query)?;
                let result = stmt
                    .query_map(params![fetch_count], Self::map_history_entry)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                result
            }
            (_, None) => {
                let query = format!(
                    "SELECT {}
                     FROM transcription_history
                     ORDER BY id DESC",
                    HISTORY_COLUMNS
                );
                let mut stmt = conn.prepare(&query)?;
                let result = stmt
                    .query_map([], Self::map_history_entry)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                result
            }
        };

        let has_more = limit.is_some_and(|lim| entries.len() > lim);
        if has_more {
            entries.pop();
        }

        Ok(PaginatedHistory { entries, has_more })
    }

    #[cfg(test)]
    fn get_latest_entry_with_conn(conn: &Connection) -> Result<Option<HistoryEntry>> {
        let query = format!(
            "SELECT {}
             FROM transcription_history
             ORDER BY timestamp DESC
             LIMIT 1",
            HISTORY_COLUMNS
        );
        let mut stmt = conn.prepare(&query)?;

        let entry = stmt.query_row([], Self::map_history_entry).optional()?;
        Ok(entry)
    }

    /// Get the latest entry with non-empty transcription text.
    pub fn get_latest_completed_entry(&self) -> Result<Option<HistoryEntry>> {
        let conn = self.get_connection()?;
        Self::get_latest_completed_entry_with_conn(&conn)
    }

    pub fn get_latest_transform_entry(&self) -> Result<Option<HistoryEntry>> {
        let conn = self.get_connection()?;
        Self::get_latest_transform_entry_with_conn(&conn)
    }

    fn get_latest_transform_entry_with_conn(conn: &Connection) -> Result<Option<HistoryEntry>> {
        let query = format!(
            "SELECT {}
             FROM transcription_history
             WHERE transform_action IS NOT NULL
               AND transform_result_text IS NOT NULL
               AND transform_result_text != ''
             ORDER BY timestamp DESC, id DESC
             LIMIT 1",
            HISTORY_COLUMNS
        );
        let mut stmt = conn.prepare(&query)?;

        let entry = stmt.query_row([], Self::map_history_entry).optional()?;
        Ok(entry)
    }

    pub async fn get_latest_adaptive_entry(&self) -> Result<Option<HistoryEntry>> {
        let conn = self.get_connection()?;
        let query = format!(
            "SELECT {}
             FROM transcription_history
             WHERE transcription_text != ''
               AND transform_action IS NULL
             ORDER BY timestamp DESC, id DESC
             LIMIT 1",
            HISTORY_COLUMNS
        );
        let mut stmt = conn.prepare(&query)?;
        let entry = stmt.query_row([], Self::map_history_entry).optional()?;
        Ok(entry)
    }

    fn get_latest_completed_entry_with_conn(conn: &Connection) -> Result<Option<HistoryEntry>> {
        let query = format!(
            "SELECT {}
             FROM transcription_history
             WHERE transcription_text != ''
               AND transform_action IS NULL
             ORDER BY timestamp DESC
             LIMIT 1",
            HISTORY_COLUMNS
        );
        let mut stmt = conn.prepare(&query)?;

        let entry = stmt.query_row([], Self::map_history_entry).optional()?;
        Ok(entry)
    }

    pub async fn toggle_saved_status(&self, id: i64) -> Result<()> {
        let conn = self.get_connection()?;

        // Get current saved status
        let current_saved: bool = conn.query_row(
            "SELECT saved FROM transcription_history WHERE id = ?1",
            params![id],
            |row| row.get("saved"),
        )?;

        let new_saved = !current_saved;

        conn.execute(
            "UPDATE transcription_history SET saved = ?1 WHERE id = ?2",
            params![new_saved, id],
        )?;

        debug!("Toggled saved status for entry {}: {}", id, new_saved);

        // Emit history updated event
        if let Err(e) = (HistoryUpdatePayload::Toggled { id }).emit(&self.app_handle) {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(())
    }

    pub fn get_audio_file_path(&self, file_name: &str) -> PathBuf {
        self.recordings_dir.join(file_name)
    }

    pub async fn get_entry_by_id(&self, id: i64) -> Result<Option<HistoryEntry>> {
        let conn = self.get_connection()?;
        let query = format!(
            "SELECT {}
             FROM transcription_history
             WHERE id = ?1",
            HISTORY_COLUMNS
        );
        let mut stmt = conn.prepare(&query)?;

        let entry = stmt.query_row([id], Self::map_history_entry).optional()?;

        Ok(entry)
    }

    fn get_entry_by_id_blocking(&self, id: i64) -> Result<HistoryEntry> {
        let conn = self.get_connection()?;
        let query = format!(
            "SELECT {}
             FROM transcription_history
             WHERE id = ?1",
            HISTORY_COLUMNS
        );
        conn.query_row(&query, params![id], Self::map_history_entry)
            .map_err(Into::into)
    }

    pub fn update_insertion_receipt(
        &self,
        id: i64,
        insertion_json: String,
    ) -> Result<HistoryEntry> {
        let conn = self.get_connection()?;
        let updated = conn.execute(
            "UPDATE transcription_history SET adaptive_insertion_json = ?1 WHERE id = ?2",
            params![insertion_json, id],
        )?;

        if updated == 0 {
            return Err(anyhow!("History entry {} not found", id));
        }

        let entry = self.get_entry_by_id_blocking(id)?;
        if let Err(e) = (HistoryUpdatePayload::Updated {
            entry: entry.clone(),
        })
        .emit(&self.app_handle)
        {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(entry)
    }

    pub async fn delete_entry(&self, id: i64) -> Result<()> {
        let conn = self.get_connection()?;

        // Get the entry to find the file name
        if let Some(entry) = self.get_entry_by_id(id).await? {
            // Delete the audio file first
            let file_path = self.get_audio_file_path(&entry.file_name);
            if file_path.exists() {
                if let Err(e) = fs::remove_file(&file_path) {
                    error!("Failed to delete audio file {}: {}", entry.file_name, e);
                    // Continue with database deletion even if file deletion fails
                }
            }
        }

        // Delete from database
        conn.execute(
            "DELETE FROM transcription_history WHERE id = ?1",
            params![id],
        )?;

        debug!("Deleted history entry with id: {}", id);

        // Emit history updated event
        if let Err(e) = (HistoryUpdatePayload::Deleted { id }).emit(&self.app_handle) {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(())
    }

    fn format_timestamp_title(&self, timestamp: i64) -> String {
        if let Some(utc_datetime) = DateTime::from_timestamp(timestamp, 0) {
            // Convert UTC to local timezone
            let local_datetime = utc_datetime.with_timezone(&Local);
            local_datetime.format("%B %e, %Y - %l:%M%p").to_string()
        } else {
            format!("Recording {}", timestamp)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE transcription_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_name TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                saved BOOLEAN NOT NULL DEFAULT 0,
                title TEXT NOT NULL,
                transcription_text TEXT NOT NULL,
                post_processed_text TEXT,
                post_process_prompt TEXT,
                post_process_requested BOOLEAN NOT NULL DEFAULT 0,
                adaptive_profile_id TEXT,
                adaptive_profile_name TEXT,
                adaptive_routing_json TEXT,
                adaptive_context_json TEXT,
                adaptive_language_json TEXT,
                adaptive_insertion_json TEXT,
                adaptive_parent_entry_id INTEGER,
                transform_action TEXT,
                transform_original_text TEXT,
                transform_result_text TEXT,
                transform_target_language TEXT,
                transform_provider_id TEXT,
                transform_model TEXT,
                transform_recovery_status TEXT
            );",
        )
        .expect("create transcription_history table");
        conn
    }

    fn insert_entry(conn: &Connection, timestamp: i64, text: &str, post_processed: Option<&str>) {
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                adaptive_profile_id,
                adaptive_profile_name,
                adaptive_routing_json,
                adaptive_context_json,
                adaptive_language_json,
                adaptive_insertion_json,
                adaptive_parent_entry_id,
                transform_action,
                transform_original_text,
                transform_result_text,
                transform_target_language,
                transform_provider_id,
                transform_model,
                transform_recovery_status
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
            params![
                format!("verbatim-{}.wav", timestamp),
                timestamp,
                false,
                format!("Recording {}", timestamp),
                text,
                post_processed,
                Option::<String>::None,
                false,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<i64>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
            ],
        )
        .expect("insert history entry");
    }

    fn insert_transform_entry(
        conn: &Connection,
        timestamp: i64,
        original_text: &str,
        result_text: &str,
    ) -> i64 {
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                adaptive_profile_id,
                adaptive_profile_name,
                adaptive_routing_json,
                adaptive_context_json,
                adaptive_language_json,
                adaptive_insertion_json,
                adaptive_parent_entry_id,
                transform_action,
                transform_original_text,
                transform_result_text,
                transform_target_language,
                transform_provider_id,
                transform_model,
                transform_recovery_status
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
            params![
                format!("transform-{}.txt", timestamp),
                timestamp,
                false,
                format!("Transform {}", timestamp),
                original_text,
                result_text,
                Option::<String>::None,
                false,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<i64>::None,
                "polish",
                original_text,
                result_text,
                Option::<String>::None,
                "verbatim_local",
                "model.gguf",
                "replaced",
            ],
        )
        .expect("insert transform history entry");

        conn.last_insert_rowid()
    }

    #[test]
    fn get_latest_entry_returns_none_when_empty() {
        let conn = setup_conn();
        let entry = HistoryManager::get_latest_entry_with_conn(&conn).expect("fetch latest entry");
        assert!(entry.is_none());
    }

    #[test]
    fn get_latest_entry_returns_newest_entry() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "first", None);
        insert_entry(&conn, 200, "second", Some("processed"));

        let entry = HistoryManager::get_latest_entry_with_conn(&conn)
            .expect("fetch latest entry")
            .expect("entry exists");

        assert_eq!(entry.timestamp, 200);
        assert_eq!(entry.transcription_text, "second");
        assert_eq!(entry.post_processed_text.as_deref(), Some("processed"));
    }

    #[test]
    fn legacy_history_entry_has_no_adaptive_metadata() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "legacy raw", None);

        let entry = HistoryManager::get_latest_entry_with_conn(&conn)
            .expect("fetch latest entry")
            .expect("entry exists");

        assert_eq!(entry.transcription_text, "legacy raw");
        assert!(entry.post_processed_text.is_none());
        assert!(entry.adaptive_profile_id.is_none());
        assert!(entry.adaptive_profile_name.is_none());
        assert!(entry.adaptive_routing_json.is_none());
        assert!(entry.adaptive_context_json.is_none());
        assert!(entry.adaptive_language_json.is_none());
        assert!(entry.adaptive_insertion_json.is_none());
        assert!(entry.adaptive_parent_entry_id.is_none());
    }

    #[test]
    fn get_latest_completed_entry_skips_empty_entries() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "completed", None);
        insert_entry(&conn, 200, "", None);

        let entry = HistoryManager::get_latest_completed_entry_with_conn(&conn)
            .expect("fetch latest completed entry")
            .expect("completed entry exists");

        assert_eq!(entry.timestamp, 100);
        assert_eq!(entry.transcription_text, "completed");
    }

    #[test]
    fn get_latest_completed_entry_skips_transform_entries() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "latest dictation transcript", None);
        insert_transform_entry(&conn, 200, "selected text", "polished selected text");

        let entry = HistoryManager::get_latest_completed_entry_with_conn(&conn)
            .expect("fetch latest completed entry")
            .expect("completed entry exists");

        assert_eq!(entry.timestamp, 100);
        assert_eq!(entry.transcription_text, "latest dictation transcript");
        assert!(entry.transform_action.is_none());
    }

    #[test]
    fn get_latest_transform_entry_uses_id_tiebreaker_for_same_second_entries() {
        let conn = setup_conn();
        insert_transform_entry(&conn, 300, "first selection", "first result");
        let newest_id = insert_transform_entry(&conn, 300, "second selection", "second result");

        let entry = HistoryManager::get_latest_transform_entry_with_conn(&conn)
            .expect("fetch latest transform")
            .expect("latest transform entry");

        assert_eq!(entry.id, newest_id);
        assert_eq!(
            entry.transform_result_text.as_deref(),
            Some("second result")
        );
    }

    #[test]
    fn count_cleanup_does_not_let_transform_entries_evict_recording_history() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "oldest dictation transcript", None);
        insert_entry(&conn, 200, "middle dictation transcript", None);
        insert_entry(&conn, 300, "newest dictation transcript", None);
        insert_transform_entry(&conn, 400, "selected text", "polished selected text");
        insert_transform_entry(&conn, 500, "another selection", "another result");

        let candidates =
            HistoryManager::count_cleanup_candidates(&conn, 2).expect("cleanup candidates");

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].1, "verbatim-100.wav");
    }

    #[test]
    fn adaptive_history_entry_can_hold_routing_metadata() {
        let entry = HistoryEntry {
            id: 1,
            file_name: "verbatim-1.wav".to_string(),
            timestamp: 1,
            saved: false,
            title: "now".to_string(),
            transcription_text: "raw".to_string(),
            post_processed_text: Some("processed".to_string()),
            post_process_prompt: None,
            post_process_requested: true,
            adaptive_profile_id: Some("email".to_string()),
            adaptive_profile_name: Some("Email".to_string()),
            adaptive_routing_json: Some("{}".to_string()),
            adaptive_context_json: Some("{}".to_string()),
            adaptive_language_json: Some("{}".to_string()),
            adaptive_insertion_json: Some("{}".to_string()),
            adaptive_parent_entry_id: None,
            transform_action: None,
            transform_original_text: None,
            transform_result_text: None,
            transform_target_language: None,
            transform_provider_id: None,
            transform_model: None,
            transform_recovery_status: None,
        };

        assert_eq!(entry.transcription_text, "raw");
        assert_eq!(entry.post_processed_text.as_deref(), Some("processed"));
        assert_eq!(entry.adaptive_profile_id.as_deref(), Some("email"));
    }

    #[test]
    fn history_entry_preserves_raw_text_and_formatted_final_text() {
        let conn = setup_conn();
        insert_entry(
            &conn,
            300,
            "The deadline is Monday no, I mean Tuesday.",
            Some("The deadline is Tuesday."),
        );

        let entry = HistoryManager::get_latest_entry_with_conn(&conn)
            .expect("fetch latest entry")
            .expect("entry exists");

        assert_eq!(
            entry.transcription_text,
            "The deadline is Monday no, I mean Tuesday."
        );
        assert_eq!(
            entry.post_processed_text.as_deref(),
            Some("The deadline is Tuesday.")
        );
    }

    #[test]
    fn transform_history_entry_preserves_original_result_and_provider_metadata() {
        let entry = HistoryEntry {
            id: 1,
            file_name: "transform-1.txt".to_string(),
            timestamp: 1,
            saved: false,
            title: "Transform".to_string(),
            transcription_text: "selected text before transform".to_string(),
            post_processed_text: Some("selected text after transform".to_string()),
            post_process_prompt: None,
            post_process_requested: false,
            adaptive_profile_id: None,
            adaptive_profile_name: None,
            adaptive_routing_json: None,
            adaptive_context_json: None,
            adaptive_language_json: None,
            adaptive_insertion_json: None,
            adaptive_parent_entry_id: None,
            transform_action: Some("make_concise".to_string()),
            transform_original_text: Some("selected text before transform".to_string()),
            transform_result_text: Some("selected text after transform".to_string()),
            transform_target_language: None,
            transform_provider_id: Some("verbatim_local".to_string()),
            transform_model: Some("qwen2.5-0.5b.gguf".to_string()),
            transform_recovery_status: Some("replaced".to_string()),
        };

        assert_eq!(entry.transform_action.as_deref(), Some("make_concise"));
        assert_eq!(
            entry.transform_original_text.as_deref(),
            Some("selected text before transform")
        );
        assert_eq!(
            entry.transform_result_text.as_deref(),
            Some("selected text after transform")
        );
        assert_eq!(
            entry.transform_provider_id.as_deref(),
            Some("verbatim_local")
        );
        assert_eq!(entry.transform_recovery_status.as_deref(), Some("replaced"));
    }
}
