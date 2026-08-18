use std::{
    collections::HashSet,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri_plugin_dialog::DialogExt;

const DEFAULT_PAGE_SIZE: u32 = 100;
const MAX_PAGE_SIZE: u32 = 200;
const MESSAGE_PREVIEW_BYTES: i64 = 1024 * 1024;
const EARLIEST_PLAUSIBLE_CAPTURE_MS: i64 = 946_684_800_000; // 2000-01-01 UTC

#[derive(Default)]
struct DatabaseState(Mutex<Option<DatabaseSession>>);

#[derive(Clone, Debug)]
struct DatabaseSession {
    path: PathBuf,
    summary: DatabaseSummary,
    has_notes: bool,
    has_metadata: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DatabaseSummary {
    display_name: String,
    path: String,
    mode: &'static str,
    row_count: u64,
    schema_variant: &'static str,
    diagnostics: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandError {
    code: &'static str,
    message: String,
    detail: Option<String>,
}

impl CommandError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageCursor {
    captured_at: i64,
    id: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InteractionPage {
    rows: Vec<InteractionSummary>,
    next_cursor: Option<PageCursorResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PageCursorResponse {
    captured_at: i64,
    id: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InteractionSummary {
    id: i64,
    captured_at: i64,
    captured_at_valid: bool,
    tool: String,
    scheme: String,
    host: String,
    port: i64,
    method: String,
    url: String,
    status_code: i64,
    mime_type: String,
    response_length: i64,
    notes_preview: Option<String>,
    colour: Option<String>,
    tag: Option<String>,
    icon: Option<String>,
    metadata_valid: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredMetadata {
    colour: String,
    tag: String,
    icon: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InteractionDetail {
    id: i64,
    request: MessagePreview,
    response: MessagePreview,
    notes: Option<String>,
    metadata: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MessagePreview {
    bytes: Vec<u8>,
    total_length: u64,
    truncated: bool,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum MessagePart {
    Request,
    Response,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportResult {
    path: String,
    byte_count: u64,
}

fn open_read_only(path: &Path) -> Result<Connection, CommandError> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|error| {
        CommandError::new("databaseOpenFailed", "The database could not be opened.")
            .detail(error.to_string())
    })
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

fn timestamp_is_plausible(value: i64) -> bool {
    let latest = now_millis().saturating_add(366 * 24 * 60 * 60 * 1000);
    (EARLIEST_PLAUSIBLE_CAPTURE_MS..=latest).contains(&value)
}

fn inspect_database(path: PathBuf) -> Result<DatabaseSession, CommandError> {
    if !path.is_file() {
        return Err(CommandError::new(
            "notAFile",
            "Choose an existing SQLite database file.",
        ));
    }

    let canonical_path = path.canonicalize().map_err(|error| {
        CommandError::new(
            "pathUnavailable",
            "The selected path could not be resolved.",
        )
        .detail(error.to_string())
    })?;

    let mut file = File::open(&canonical_path).map_err(|error| {
        CommandError::new("fileReadFailed", "The selected file could not be read.")
            .detail(error.to_string())
    })?;
    let mut header = [0_u8; 16];
    if file.read_exact(&mut header).is_err() || &header != b"SQLite format 3\0" {
        return Err(CommandError::new(
            "notSqlite",
            "The selected file is not a SQLite 3 database.",
        ));
    }

    let connection = open_read_only(&canonical_path)?;
    let integrity: String = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|error| {
            CommandError::new(
                "integrityCheckFailed",
                "SQLite could not validate the database.",
            )
            .detail(error.to_string())
        })?;
    if integrity != "ok" {
        return Err(CommandError::new(
            "integrityCheckFailed",
            "The database failed SQLite's integrity check.",
        )
        .detail(integrity));
    }

    let mut column_statement = connection
        .prepare("PRAGMA table_xinfo(http_interactions)")
        .map_err(|error| {
            CommandError::new(
                "schemaReadFailed",
                "The interaction schema could not be read.",
            )
            .detail(error.to_string())
        })?;
    let columns = column_statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| {
            CommandError::new(
                "schemaReadFailed",
                "The interaction schema could not be read.",
            )
            .detail(error.to_string())
        })?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|error| {
            CommandError::new(
                "schemaReadFailed",
                "The interaction schema could not be read.",
            )
            .detail(error.to_string())
        })?;

    let required = [
        "id",
        "captured_at",
        "tool",
        "scheme",
        "host",
        "port",
        "method",
        "url",
        "status_code",
        "mime_type",
        "request",
        "response",
    ];
    let missing = required
        .iter()
        .filter(|name| !columns.contains(**name))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(CommandError::new(
            "unsupportedSchema",
            "The database does not contain the required HTTP interaction schema.",
        )
        .detail(format!("Missing columns: {}", missing.join(", "))));
    }

    let row_count_raw = connection
        .query_row("SELECT count(*) FROM http_interactions", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|error| {
            CommandError::new("queryFailed", "The interaction count could not be read.")
                .detail(error.to_string())
        })?;
    let row_count = row_count_raw.max(0) as u64;
    if row_count > 100_000 {
        return Err(CommandError::new(
            "rowLimitExceeded",
            "This database exceeds the supported limit of 100,000 interactions.",
        )
        .detail(format!("Found {row_count} interactions.")));
    }

    let invalid_timestamps_raw = connection
        .query_row(
            "SELECT count(*) FROM http_interactions WHERE captured_at < ?1 OR captured_at > ?2",
            params![
                EARLIEST_PLAUSIBLE_CAPTURE_MS,
                now_millis().saturating_add(366 * 24 * 60 * 60 * 1000)
            ],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| {
            CommandError::new("queryFailed", "Capture timestamps could not be validated.")
                .detail(error.to_string())
        })?;
    let invalid_timestamps = invalid_timestamps_raw.max(0) as u64;

    let has_notes = columns.contains("notes");
    let has_metadata = columns.contains("metadata");
    let schema_variant = if has_notes && has_metadata {
        "current"
    } else {
        "legacy"
    };
    let mut diagnostics = Vec::new();
    if schema_variant == "legacy" {
        diagnostics.push(
            "This legacy database has no notes and/or metadata column; annotation is unavailable."
                .to_string(),
        );
    }
    if invalid_timestamps > 0 {
        diagnostics.push(format!(
            "{invalid_timestamps} captured_at value(s) fall outside the plausible epoch-millisecond range; raw values remain available."
        ));
    }

    let display_name = canonical_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("SQLite database")
        .to_string();
    let summary = DatabaseSummary {
        display_name,
        path: canonical_path.to_string_lossy().into_owned(),
        mode: "read-only",
        row_count,
        schema_variant,
        diagnostics,
    };

    Ok(DatabaseSession {
        path: canonical_path,
        summary,
        has_notes,
        has_metadata,
    })
}

fn active_session(state: &DatabaseState) -> Result<DatabaseSession, CommandError> {
    state
        .0
        .lock()
        .map_err(|_| CommandError::new("stateUnavailable", "Database state is unavailable."))?
        .clone()
        .ok_or_else(|| CommandError::new("noDatabase", "Open a database first."))
}

#[tauri::command]
async fn open_database(
    path: String,
    state: tauri::State<'_, DatabaseState>,
) -> Result<DatabaseSummary, CommandError> {
    let session =
        tauri::async_runtime::spawn_blocking(move || inspect_database(PathBuf::from(path)))
            .await
            .map_err(|error| {
                CommandError::new("taskFailed", "Database validation did not complete.")
                    .detail(error.to_string())
            })??;
    let summary = session.summary.clone();
    *state
        .0
        .lock()
        .map_err(|_| CommandError::new("stateUnavailable", "Database state is unavailable."))? =
        Some(session);
    Ok(summary)
}

#[tauri::command]
async fn query_interactions(
    cursor: Option<PageCursor>,
    page_size: Option<u32>,
    state: tauri::State<'_, DatabaseState>,
) -> Result<InteractionPage, CommandError> {
    let session = active_session(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_read_only(&session.path)?;
        let page_size = page_size
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .clamp(1, MAX_PAGE_SIZE);
        let notes_expression = if session.has_notes {
            "CASE WHEN notes IS NULL THEN NULL ELSE substr(notes, 1, 120) END"
        } else {
            "NULL"
        };
        let metadata_expression = if session.has_metadata {
            "metadata"
        } else {
            "NULL"
        };
        let cursor_clause = if cursor.is_some() {
            "WHERE (captured_at < ?1 OR (captured_at = ?1 AND id < ?2))"
        } else {
            ""
        };
        let sql = format!(
            "SELECT id, captured_at, tool, scheme, host, port, method, url, status_code, \
             mime_type, length(response), {notes_expression}, {metadata_expression} \
             FROM http_interactions {cursor_clause} \
             ORDER BY captured_at DESC, id DESC LIMIT ?3"
        );
        let mut statement = connection.prepare(&sql).map_err(|error| {
            CommandError::new("queryFailed", "Interactions could not be queried.")
                .detail(error.to_string())
        })?;
        let limit = i64::from(page_size) + 1;
        let (captured_at, id) = cursor
            .map(|value| (value.captured_at, value.id))
            .unwrap_or((i64::MAX, i64::MAX));
        let mapped = statement
            .query_map(params![captured_at, id, limit], |row| {
                let metadata_json = row.get::<_, Option<String>>(12)?;
                let parsed = metadata_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<StoredMetadata>(value).ok());
                let metadata_valid = metadata_json.is_none() || parsed.is_some();
                let (colour, tag, icon) = parsed
                    .map(|value| (Some(value.colour), Some(value.tag), Some(value.icon)))
                    .unwrap_or_default();
                let captured_at = row.get(1)?;
                Ok(InteractionSummary {
                    id: row.get(0)?,
                    captured_at,
                    captured_at_valid: timestamp_is_plausible(captured_at),
                    tool: row.get(2)?,
                    scheme: row.get(3)?,
                    host: row.get(4)?,
                    port: row.get(5)?,
                    method: row.get(6)?,
                    url: row.get(7)?,
                    status_code: row.get(8)?,
                    mime_type: row.get(9)?,
                    response_length: row.get(10)?,
                    notes_preview: row.get(11)?,
                    colour,
                    tag,
                    icon,
                    metadata_valid,
                })
            })
            .map_err(|error| {
                CommandError::new("queryFailed", "Interactions could not be queried.")
                    .detail(error.to_string())
            })?;
        let mut rows = mapped.collect::<Result<Vec<_>, _>>().map_err(|error| {
            CommandError::new("queryFailed", "An interaction row could not be read.")
                .detail(error.to_string())
        })?;
        let has_more = rows.len() > page_size as usize;
        if has_more {
            rows.pop();
        }
        let next_cursor = if has_more {
            rows.last().map(|row| PageCursorResponse {
                captured_at: row.captured_at,
                id: row.id,
            })
        } else {
            None
        };
        Ok(InteractionPage { rows, next_cursor })
    })
    .await
    .map_err(|error| {
        CommandError::new("taskFailed", "The interaction query did not complete.")
            .detail(error.to_string())
    })?
}

#[tauri::command]
async fn get_interaction(
    interaction_id: i64,
    state: tauri::State<'_, DatabaseState>,
) -> Result<InteractionDetail, CommandError> {
    let session = active_session(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_read_only(&session.path)?;
        let notes_expression = if session.has_notes { "notes" } else { "NULL" };
        let metadata_expression = if session.has_metadata {
            "metadata"
        } else {
            "NULL"
        };
        let sql = format!(
            "SELECT substr(request, 1, ?1), length(request), substr(response, 1, ?1), \
             length(response), {notes_expression}, {metadata_expression} \
             FROM http_interactions WHERE id = ?2"
        );
        connection
            .query_row(
                &sql,
                params![MESSAGE_PREVIEW_BYTES, interaction_id],
                |row| {
                    let request_bytes = row.get::<_, Vec<u8>>(0)?;
                    let request_length = row.get::<_, i64>(1)?.max(0) as u64;
                    let response_bytes = row.get::<_, Vec<u8>>(2)?;
                    let response_length = row.get::<_, i64>(3)?.max(0) as u64;
                    Ok(InteractionDetail {
                        id: interaction_id,
                        request: MessagePreview {
                            truncated: request_length > request_bytes.len() as u64,
                            total_length: request_length,
                            bytes: request_bytes,
                        },
                        response: MessagePreview {
                            truncated: response_length > response_bytes.len() as u64,
                            total_length: response_length,
                            bytes: response_bytes,
                        },
                        notes: row.get(4)?,
                        metadata: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(|error| {
                CommandError::new("queryFailed", "The interaction could not be loaded.")
                    .detail(error.to_string())
            })?
            .ok_or_else(|| {
                CommandError::new(
                    "interactionMissing",
                    "The selected interaction no longer exists.",
                )
            })
    })
    .await
    .map_err(|error| {
        CommandError::new("taskFailed", "The interaction load did not complete.")
            .detail(error.to_string())
    })?
}

#[tauri::command]
async fn export_interaction_part(
    app: tauri::AppHandle,
    interaction_id: i64,
    part: MessagePart,
    state: tauri::State<'_, DatabaseState>,
) -> Result<Option<ExportResult>, CommandError> {
    let session = active_session(&state)?;
    let part_name = match part {
        MessagePart::Request => "request",
        MessagePart::Response => "response",
    };
    let suggested_name = format!("interaction-{interaction_id}-{part_name}.http");
    let chosen = app
        .dialog()
        .file()
        .set_file_name(&suggested_name)
        .add_filter("Raw HTTP message", &["http", "bin"])
        .blocking_save_file();
    let Some(destination) = chosen.and_then(|path| path.into_path().ok()) else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_read_only(&session.path)?;
        let column = match part {
            MessagePart::Request => "request",
            MessagePart::Response => "response",
        };
        let bytes = connection
            .query_row(
                &format!("SELECT {column} FROM http_interactions WHERE id = ?1"),
                [interaction_id],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(|error| {
                CommandError::new("queryFailed", "The interaction could not be exported.")
                    .detail(error.to_string())
            })?
            .ok_or_else(|| {
                CommandError::new(
                    "interactionMissing",
                    "The selected interaction no longer exists.",
                )
            })?;
        let temporary = destination.with_extension("burp-viewer-exporting");
        fs::write(&temporary, &bytes).map_err(|error| {
            CommandError::new("exportWriteFailed", "The export file could not be written.")
                .detail(error.to_string())
        })?;
        fs::rename(&temporary, &destination).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            CommandError::new(
                "exportWriteFailed",
                "The export could not be completed atomically.",
            )
            .detail(error.to_string())
        })?;
        Ok(Some(ExportResult {
            path: destination.to_string_lossy().into_owned(),
            byte_count: bytes.len() as u64,
        }))
    })
    .await
    .map_err(|error| {
        CommandError::new("taskFailed", "The export did not complete.").detail(error.to_string())
    })?
}

#[tauri::command]
fn close_database(state: tauri::State<'_, DatabaseState>) -> Result<(), CommandError> {
    *state
        .0
        .lock()
        .map_err(|_| CommandError::new("stateUnavailable", "Database state is unavailable."))? =
        None;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(DatabaseState::default())
        .invoke_handler(tauri::generate_handler![
            open_database,
            query_interactions,
            get_interaction,
            export_interaction_part,
            close_database
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "burp-sqlite-viewer-{label}-{}.sqlite",
            std::process::id()
        ))
    }

    fn create_fixture(path: &Path, include_annotations: bool) {
        let connection = Connection::open(path).expect("fixture database opens");
        let annotation_columns = if include_annotations {
            ", notes TEXT, metadata TEXT"
        } else {
            ""
        };
        connection
            .execute_batch(&format!(
                "CREATE TABLE http_interactions (
                    id INTEGER PRIMARY KEY,
                    captured_at INTEGER NOT NULL,
                    tool TEXT NOT NULL,
                    scheme TEXT NOT NULL,
                    host TEXT NOT NULL,
                    port INTEGER NOT NULL,
                    method TEXT NOT NULL,
                    url TEXT NOT NULL,
                    status_code INTEGER NOT NULL,
                    mime_type TEXT NOT NULL,
                    request BLOB NOT NULL,
                    response BLOB NOT NULL
                    {annotation_columns}
                );"
            ))
            .expect("fixture schema created");
        connection
            .execute(
                "INSERT INTO http_interactions
                 (id, captured_at, tool, scheme, host, port, method, url, status_code,
                  mime_type, request, response)
                 VALUES (1, ?1, 'Proxy', 'https', 'example.test', 443, 'GET',
                         'https://example.test/', 200, 'text/plain', ?2, ?3)",
                params![
                    now_millis(),
                    b"GET / HTTP/1.1\r\n\r\n",
                    b"HTTP/1.1 200 OK\r\n\r\nhello"
                ],
            )
            .expect("fixture row inserted");
    }

    #[test]
    fn accepts_current_and_legacy_schemas() {
        for (label, annotations, expected_variant) in
            [("current", true, "current"), ("legacy", false, "legacy")]
        {
            let path = fixture_path(label);
            create_fixture(&path, annotations);
            let session = inspect_database(path.clone()).expect("valid fixture accepted");
            assert_eq!(session.summary.schema_variant, expected_variant);
            assert_eq!(session.summary.row_count, 1);
            fs::remove_file(path).expect("fixture removed");
        }
    }

    #[test]
    fn rejects_non_sqlite_files() {
        let path = fixture_path("invalid");
        fs::write(&path, b"not sqlite").expect("invalid fixture written");
        let error = inspect_database(path.clone()).expect_err("invalid file rejected");
        assert_eq!(error.code, "notSqlite");
        fs::remove_file(path).expect("fixture removed");
    }

    #[test]
    fn validates_timestamp_ranges() {
        assert!(timestamp_is_plausible(now_millis()));
        assert!(!timestamp_is_plausible(0));
    }
}
