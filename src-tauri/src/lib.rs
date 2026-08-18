use std::{
    collections::HashSet,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, params_from_iter, types::Value, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

const DEFAULT_PAGE_SIZE: u32 = 100;
const MAX_PAGE_SIZE: u32 = 200;
const MESSAGE_PREVIEW_BYTES: i64 = 1024 * 1024;
const EARLIEST_PLAUSIBLE_CAPTURE_MS: i64 = 946_684_800_000; // 2000-01-01 UTC
const COLOUR_IDS: &[&str] = &["", "red", "orange", "yellow", "green", "blue", "purple"];
const ICON_IDS: &[&str] = &["", "star", "flag", "bookmark", "bug", "check"];

#[derive(Default)]
struct DatabaseState(Mutex<Option<DatabaseSession>>);

#[derive(Clone, Debug)]
struct DatabaseSession {
    path: PathBuf,
    summary: DatabaseSummary,
    has_notes: bool,
    has_metadata: bool,
    writable: bool,
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
    offset: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InteractionPage {
    rows: Vec<InteractionSummary>,
    next_cursor: Option<PageCursorResponse>,
    filtered_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PageCursorResponse {
    offset: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum SortField {
    Id,
    #[default]
    CapturedAt,
    Tool,
    Scheme,
    Host,
    Port,
    Method,
    Url,
    StatusCode,
    MimeType,
    ResponseLength,
    Colour,
    Tag,
    Icon,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SortDirection {
    Asc,
    #[default]
    Desc,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum FilterField {
    Id,
    CapturedAt,
    Tool,
    Scheme,
    Host,
    Port,
    Method,
    Url,
    StatusCode,
    MimeType,
    ResponseLength,
    Colour,
    Tag,
    Icon,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum FilterOperator {
    Contains,
    Equals,
    StartsWith,
    EndsWith,
    IsEmpty,
    GreaterThan,
    LessThan,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FilterSpec {
    field: FilterField,
    operator: FilterOperator,
    value: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuerySpec {
    sort_field: SortField,
    sort_direction: SortDirection,
    #[serde(default)]
    filters: Vec<FilterSpec>,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredMetadata {
    colour: String,
    tag: String,
    icon: String,
}

fn parse_metadata(value: &str) -> Option<StoredMetadata> {
    let metadata = serde_json::from_str::<StoredMetadata>(value).ok()?;
    (COLOUR_IDS.contains(&metadata.colour.as_str())
        && ICON_IDS.contains(&metadata.icon.as_str())
        && metadata.tag.len() <= 100)
        .then_some(metadata)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnnotationInput {
    interaction_ids: Vec<i64>,
    update_notes: bool,
    notes: String,
    update_colour: bool,
    colour: String,
    update_tag: bool,
    tag: String,
    update_icon: bool,
    icon: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnnotationResult {
    updated_count: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplayInput {
    interaction_id: i64,
    target_url: String,
    draft_bytes: Option<Vec<u8>>,
    proxy_url: Option<String>,
    proxy_username: String,
    proxy_password: String,
    follow_redirects: bool,
    timeout_seconds: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReplayResult {
    history_id: i64,
    request_bytes: Vec<u8>,
    response_bytes: Vec<u8>,
    status_code: Option<u16>,
    elapsed_millis: u64,
    error: Option<String>,
    normalised_by_http_client: bool,
}

struct ParsedRequest {
    method: reqwest::Method,
    headers: reqwest::header::HeaderMap,
    body: Vec<u8>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InteractionDetail {
    id: i64,
    captured_at: i64,
    method: String,
    url: String,
    status_code: i64,
    mime_type: String,
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

fn open_read_write(path: &Path) -> Result<Connection, CommandError> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(|error| {
        CommandError::new(
            "databaseReadOnly",
            "The database cannot be opened for annotation.",
        )
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
    if has_metadata {
        let mut statement = connection
            .prepare("SELECT metadata FROM http_interactions WHERE metadata IS NOT NULL")
            .map_err(|error| {
                CommandError::new(
                    "queryFailed",
                    "Interaction metadata could not be validated.",
                )
                .detail(error.to_string())
            })?;
        let values = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| {
                CommandError::new(
                    "queryFailed",
                    "Interaction metadata could not be validated.",
                )
                .detail(error.to_string())
            })?;
        let mut invalid_metadata = 0_u64;
        for value in values {
            let value = value.map_err(|error| {
                CommandError::new("queryFailed", "Interaction metadata could not be read.")
                    .detail(error.to_string())
            })?;
            if parse_metadata(&value).is_none() {
                invalid_metadata += 1;
            }
        }
        if invalid_metadata > 0 {
            diagnostics.push(format!(
                "{invalid_metadata} metadata value(s) do not match the required colour/tag/icon schema; they remain preserved until explicitly replaced."
            ));
        }
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
    let writable =
        Connection::open_with_flags(&canonical_path, OpenFlags::SQLITE_OPEN_READ_WRITE).is_ok();
    if !writable {
        diagnostics.push("The database is read-only; notes and markers cannot be changed.".into());
    }
    let summary = DatabaseSummary {
        display_name,
        path: canonical_path.to_string_lossy().into_owned(),
        mode: if writable { "read-write" } else { "read-only" },
        row_count,
        schema_variant,
        diagnostics,
    };

    Ok(DatabaseSession {
        path: canonical_path,
        summary,
        has_notes,
        has_metadata,
        writable,
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
    app: tauri::AppHandle,
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
    close_detail_windows(&app);
    *state
        .0
        .lock()
        .map_err(|_| CommandError::new("stateUnavailable", "Database state is unavailable."))? =
        Some(session);
    Ok(summary)
}

#[tauri::command]
async fn enable_annotations(
    state: tauri::State<'_, DatabaseState>,
) -> Result<DatabaseSummary, CommandError> {
    let session = active_session(&state)?;
    if !session.writable {
        return Err(CommandError::new(
            "databaseReadOnly",
            "Make the capture writable before enabling annotations.",
        ));
    }
    let path = session.path.clone();
    let add_notes = !session.has_notes;
    let add_metadata = !session.has_metadata;
    tauri::async_runtime::spawn_blocking(move || {
        let mut connection = open_read_write(&path)?;
        let transaction = connection.transaction().map_err(|error| {
            CommandError::new(
                "migrationFailed",
                "The annotation migration could not start.",
            )
            .detail(error.to_string())
        })?;
        if add_notes {
            transaction
                .execute("ALTER TABLE http_interactions ADD COLUMN notes TEXT", [])
                .map_err(|error| {
                    CommandError::new("migrationFailed", "The notes column could not be added.")
                        .detail(error.to_string())
                })?;
        }
        if add_metadata {
            transaction
                .execute("ALTER TABLE http_interactions ADD COLUMN metadata TEXT", [])
                .map_err(|error| {
                    CommandError::new("migrationFailed", "The metadata column could not be added.")
                        .detail(error.to_string())
                })?;
        }
        transaction.commit().map_err(|error| {
            CommandError::new(
                "migrationFailed",
                "The annotation migration could not be committed.",
            )
            .detail(error.to_string())
        })
    })
    .await
    .map_err(|error| {
        CommandError::new("taskFailed", "The annotation migration did not complete.")
            .detail(error.to_string())
    })??;

    let mut guard = state
        .0
        .lock()
        .map_err(|_| CommandError::new("stateUnavailable", "Database state is unavailable."))?;
    let current = guard.as_mut().ok_or_else(|| {
        CommandError::new(
            "noDatabase",
            "The active database changed during migration.",
        )
    })?;
    if current.path != session.path {
        return Err(CommandError::new(
            "databaseChanged",
            "The active database changed during migration.",
        ));
    }
    current.has_notes = true;
    current.has_metadata = true;
    current.summary.schema_variant = "current";
    current
        .summary
        .diagnostics
        .retain(|message| !message.starts_with("This legacy database"));
    Ok(current.summary.clone())
}

#[tauri::command]
async fn annotate_interactions(
    annotation: AnnotationInput,
    state: tauri::State<'_, DatabaseState>,
) -> Result<AnnotationResult, CommandError> {
    let session = active_session(&state)?;
    if !session.writable || !session.has_notes || !session.has_metadata {
        return Err(CommandError::new(
            "annotationUnavailable",
            "Enable annotations in a writable capture before saving markers or notes.",
        ));
    }
    if annotation.interaction_ids.is_empty() || annotation.interaction_ids.len() > 1_000 {
        return Err(CommandError::new(
            "invalidSelection",
            "Annotate between one and 1,000 interactions at a time.",
        ));
    }
    let unique_ids = annotation
        .interaction_ids
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    if unique_ids.len() != annotation.interaction_ids.len() || unique_ids.iter().any(|id| *id < 1) {
        return Err(CommandError::new(
            "invalidSelection",
            "The annotation selection contains an invalid or duplicate row ID.",
        ));
    }
    if annotation.notes.len() > 10_000 || annotation.tag.len() > 100 {
        return Err(CommandError::new(
            "annotationTooLong",
            "Notes are limited to 10,000 characters and tags to 100 characters.",
        ));
    }
    if (annotation.update_colour && !COLOUR_IDS.contains(&annotation.colour.as_str()))
        || (annotation.update_icon && !ICON_IDS.contains(&annotation.icon.as_str()))
    {
        return Err(CommandError::new(
            "invalidMetadata",
            "Choose a supported colour and icon.",
        ));
    }
    let update_notes = annotation.update_notes;
    let notes = annotation.notes;
    let update_colour = annotation.update_colour;
    let colour = annotation.colour;
    let update_tag = annotation.update_tag;
    let tag = annotation.tag;
    let update_icon = annotation.update_icon;
    let icon = annotation.icon;
    let expected_count = annotation.interaction_ids.len() as u64;
    let ids = annotation.interaction_ids;
    tauri::async_runtime::spawn_blocking(move || {
        let mut connection = open_read_write(&session.path)?;
        let transaction = connection.transaction().map_err(|error| {
            CommandError::new("annotationFailed", "The annotation transaction could not start.")
                .detail(error.to_string())
        })?;
        let mut updated_count = 0_u64;
        for id in ids {
            let existing_metadata = transaction
                .query_row(
                    "SELECT metadata FROM http_interactions WHERE id = ?1",
                    [id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(|error| {
                    CommandError::new(
                        "annotationFailed",
                        "Existing interaction metadata could not be read.",
                    )
                    .detail(error.to_string())
                })?;
            let Some(existing_metadata) = existing_metadata else {
                return Err(CommandError::new(
                    "annotationConflict",
                    "A selected interaction disappeared; nothing was saved.",
                ));
            };
            let mut stored = match existing_metadata {
                Some(value) => match parse_metadata(&value) {
                    Some(stored) => stored,
                    None if update_colour && update_tag && update_icon => StoredMetadata {
                        colour: String::new(),
                        tag: String::new(),
                        icon: String::new(),
                    },
                    None => {
                        return Err(CommandError::new(
                            "invalidMetadata",
                            "A selected interaction has invalid metadata; replace all marker fields to repair it.",
                        ))
                    }
                },
                None => StoredMetadata {
                    colour: String::new(),
                    tag: String::new(),
                    icon: String::new(),
                },
            };
            if update_colour {
                stored.colour.clone_from(&colour);
            }
            if update_tag {
                stored.tag.clone_from(&tag);
            }
            if update_icon {
                stored.icon.clone_from(&icon);
            }
            let metadata = serde_json::to_string(&stored).map_err(|error| {
                CommandError::new("invalidMetadata", "The metadata could not be encoded.")
                    .detail(error.to_string())
            })?;
            let changed = if update_notes {
                transaction.execute(
                    "UPDATE http_interactions SET notes = ?1, metadata = ?2 WHERE id = ?3",
                    params![notes, metadata, id],
                )
            } else {
                transaction.execute(
                    "UPDATE http_interactions SET metadata = ?1 WHERE id = ?2",
                    params![metadata, id],
                )
            }
            .map_err(|error| {
                CommandError::new("annotationFailed", "An interaction could not be annotated.")
                    .detail(error.to_string())
            })?;
            updated_count += changed as u64;
        }
        if updated_count != expected_count {
            return Err(CommandError::new(
                "annotationConflict",
                "One or more selected interactions changed or disappeared; nothing was saved.",
            ));
        }
        transaction.commit().map_err(|error| {
            CommandError::new("annotationFailed", "The annotations could not be committed.")
                .detail(error.to_string())
        })?;
        Ok(AnnotationResult { updated_count })
    })
    .await
    .map_err(|error| {
        CommandError::new("taskFailed", "Saving annotations did not complete.")
            .detail(error.to_string())
    })?
}

#[tauri::command]
async fn query_interactions(
    query: QuerySpec,
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
        if query.filters.len() > 8 {
            return Err(CommandError::new(
                "tooManyFilters",
                "Use no more than eight column filters.",
            ));
        }
        let metadata_field = |name: &str| {
            if session.has_metadata {
                format!(
                    "CASE WHEN json_valid(metadata) THEN COALESCE(json_extract(metadata, '$.{name}'), '') ELSE '' END"
                )
            } else {
                "''".to_string()
            }
        };
        let field_expression = |field: FilterField| -> String {
            match field {
                FilterField::Id => "id".into(),
                FilterField::CapturedAt => "captured_at".into(),
                FilterField::Tool => "tool".into(),
                FilterField::Scheme => "scheme".into(),
                FilterField::Host => "host".into(),
                FilterField::Port => "port".into(),
                FilterField::Method => "method".into(),
                FilterField::Url => "url".into(),
                FilterField::StatusCode => "status_code".into(),
                FilterField::MimeType => "mime_type".into(),
                FilterField::ResponseLength => "length(response)".into(),
                FilterField::Colour => metadata_field("colour"),
                FilterField::Tag => metadata_field("tag"),
                FilterField::Icon => metadata_field("icon"),
            }
        };
        let is_numeric = |field: FilterField| {
            matches!(
                field,
                FilterField::Id
                    | FilterField::CapturedAt
                    | FilterField::Port
                    | FilterField::StatusCode
                    | FilterField::ResponseLength
            )
        };
        let mut clauses = Vec::new();
        let mut values = Vec::<Value>::new();
        for filter in &query.filters {
            if filter.value.len() > 500 {
                return Err(CommandError::new(
                    "filterTooLong",
                    "A filter value exceeds the 500-character limit.",
                ));
            }
            let expression = field_expression(filter.field);
            if is_numeric(filter.field) {
                let number = filter.value.trim().parse::<i64>().map_err(|_| {
                    CommandError::new(
                        "invalidFilter",
                        "Numeric filters require a whole-number value.",
                    )
                })?;
                let operator = match filter.operator {
                    FilterOperator::Equals => "=",
                    FilterOperator::GreaterThan => ">",
                    FilterOperator::LessThan => "<",
                    _ => {
                        return Err(CommandError::new(
                            "invalidFilter",
                            "That operator is not valid for a numeric column.",
                        ))
                    }
                };
                clauses.push(format!("{expression} {operator} ?"));
                values.push(Value::Integer(number));
            } else {
                match filter.operator {
                    FilterOperator::Contains => {
                        clauses.push(format!("LOWER({expression}) LIKE LOWER(?) ESCAPE '\\'"));
                        values.push(Value::Text(like_filter_value(
                            FilterOperator::Contains,
                            &filter.value,
                        )));
                    }
                    FilterOperator::StartsWith => {
                        clauses.push(format!("LOWER({expression}) LIKE LOWER(?) ESCAPE '\\'"));
                        values.push(Value::Text(like_filter_value(
                            FilterOperator::StartsWith,
                            &filter.value,
                        )));
                    }
                    FilterOperator::EndsWith => {
                        clauses.push(format!("LOWER({expression}) LIKE LOWER(?) ESCAPE '\\'"));
                        values.push(Value::Text(like_filter_value(
                            FilterOperator::EndsWith,
                            &filter.value,
                        )));
                    }
                    FilterOperator::Equals => {
                        clauses.push(format!("{expression} = ? COLLATE NOCASE"));
                        values.push(Value::Text(filter.value.clone()));
                    }
                    FilterOperator::IsEmpty => clauses.push(format!("{expression} = ''")),
                    _ => {
                        return Err(CommandError::new(
                            "invalidFilter",
                            "That operator is not valid for a text column.",
                        ))
                    }
                }
            }
        }
        let where_clause = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        let sort_expression = match query.sort_field {
            SortField::Id => "id".into(),
            SortField::CapturedAt => "captured_at".into(),
            SortField::Tool => "tool".into(),
            SortField::Scheme => "scheme".into(),
            SortField::Host => "host".into(),
            SortField::Port => "port".into(),
            SortField::Method => "method".into(),
            SortField::Url => "url".into(),
            SortField::StatusCode => "status_code".into(),
            SortField::MimeType => "mime_type".into(),
            SortField::ResponseLength => "length(response)".into(),
            SortField::Colour => metadata_field("colour"),
            SortField::Tag => metadata_field("tag"),
            SortField::Icon => metadata_field("icon"),
        };
        let direction = match query.sort_direction {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        };
        let filtered_count_raw = connection
            .query_row(
                &format!("SELECT count(*) FROM http_interactions {where_clause}"),
                params_from_iter(values.iter()),
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| {
                CommandError::new("queryFailed", "The filtered row count could not be read.")
                    .detail(error.to_string())
            })?;
        let filtered_count = filtered_count_raw.max(0) as u64;
        let sql = format!(
            "SELECT id, captured_at, tool, scheme, host, port, method, url, status_code, \
             mime_type, length(response), {notes_expression}, {metadata_expression} \
             FROM http_interactions {where_clause} \
             ORDER BY {sort_expression} {direction}, id {direction} LIMIT ? OFFSET ?"
        );
        let mut statement = connection.prepare(&sql).map_err(|error| {
            CommandError::new("queryFailed", "Interactions could not be queried.")
                .detail(error.to_string())
        })?;
        let limit = i64::from(page_size) + 1;
        let offset = cursor.map(|value| value.offset).unwrap_or_default();
        values.push(Value::Integer(limit));
        values.push(Value::Integer(offset.min(i64::MAX as u64) as i64));
        let mapped = statement
            .query_map(params_from_iter(values.iter()), |row| {
                let metadata_json = row.get::<_, Option<String>>(12)?;
                let parsed = metadata_json.as_deref().and_then(parse_metadata);
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
            Some(PageCursorResponse {
                offset: offset.saturating_add(page_size as u64),
            })
        } else {
            None
        };
        Ok(InteractionPage {
            rows,
            next_cursor,
            filtered_count,
        })
    })
    .await
    .map_err(|error| {
        CommandError::new("taskFailed", "The interaction query did not complete.")
            .detail(error.to_string())
    })?
}

fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn like_filter_value(operator: FilterOperator, value: &str) -> String {
    let escaped = escape_like_pattern(value);
    match operator {
        FilterOperator::Contains => format!("%{escaped}%"),
        FilterOperator::StartsWith => format!("{escaped}%"),
        FilterOperator::EndsWith => format!("%{escaped}"),
        _ => escaped,
    }
}

fn parse_replay_request(bytes: &[u8]) -> Result<ParsedRequest, String> {
    let (head, body_start) = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| (&bytes[..position], position + 4))
        .or_else(|| {
            bytes
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|position| (&bytes[..position], position + 2))
        })
        .ok_or_else(|| "The request has no complete HTTP header terminator.".to_string())?;
    let head = std::str::from_utf8(head)
        .map_err(|_| "The HTTP start line or headers contain invalid UTF-8.".to_string())?;
    let mut lines = head.lines().map(|line| line.trim_end_matches('\r'));
    let start_line = lines
        .next()
        .ok_or_else(|| "The request is empty.".to_string())?;
    let mut start_parts = start_line.split_whitespace();
    let method = start_parts
        .next()
        .ok_or_else(|| "The request method is missing.".to_string())?
        .parse::<reqwest::Method>()
        .map_err(|_| "The request method is not supported by the structured sender.".to_string())?;
    let _target = start_parts
        .next()
        .ok_or_else(|| "The request target is missing.".to_string())?;
    let _version = start_parts
        .next()
        .ok_or_else(|| "The HTTP version is missing.".to_string())?;
    let mut headers = reqwest::header::HeaderMap::new();
    for line in lines {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| format!("Malformed header line: {line}"))?;
        let name = reqwest::header::HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| format!("Invalid header name: {name}"))?;
        let value = reqwest::header::HeaderValue::from_bytes(value.trim_start().as_bytes())
            .map_err(|_| format!("Invalid value for header {name}."))?;
        headers.append(name, value);
    }
    Ok(ParsedRequest {
        method,
        headers,
        body: bytes[body_start..].to_vec(),
    })
}

fn ensure_replay_history(path: &Path) -> Result<(), CommandError> {
    let connection = open_read_write(path)?;
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS replay_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_interaction_id INTEGER NOT NULL,
                sent_at INTEGER NOT NULL,
                target_url TEXT NOT NULL,
                proxy_url TEXT,
                request BLOB NOT NULL,
                response BLOB,
                status_code INTEGER,
                elapsed_millis INTEGER NOT NULL,
                error TEXT,
                normalised_by_http_client INTEGER NOT NULL
            );",
        )
        .map_err(|error| {
            CommandError::new(
                "historyUnavailable",
                "Replay history could not be prepared in the capture database.",
            )
            .detail(error.to_string())
        })
}

#[tauri::command]
async fn replay_request(
    replay: ReplayInput,
    state: tauri::State<'_, DatabaseState>,
) -> Result<ReplayResult, CommandError> {
    let session = active_session(&state)?;
    if !session.writable {
        return Err(CommandError::new(
            "historyUnavailable",
            "Replay requires a writable capture so every attempt can be recorded.",
        ));
    }
    tauri::async_runtime::spawn_blocking(move || {
        ensure_replay_history(&session.path)?;
        let request_bytes = match replay.draft_bytes {
            Some(bytes) => bytes,
            None => open_read_only(&session.path)?
                .query_row(
                    "SELECT request FROM http_interactions WHERE id = ?1",
                    [replay.interaction_id],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()
                .map_err(|error| {
                    CommandError::new("queryFailed", "The source request could not be loaded.")
                        .detail(error.to_string())
                })?
                .ok_or_else(|| {
                    CommandError::new(
                        "interactionMissing",
                        "The source interaction no longer exists.",
                    )
                })?,
        };
        if request_bytes.len() > 100 * 1024 * 1024 {
            return Err(CommandError::new(
                "requestTooLarge",
                "Replay requests are limited to 100 MiB.",
            ));
        }
        let started = Instant::now();
        let outcome = (|| -> Result<(Option<u16>, Vec<u8>), String> {
            let target = reqwest::Url::parse(&replay.target_url)
                .map_err(|error| format!("Invalid target URL: {error}"))?;
            if !matches!(target.scheme(), "http" | "https") {
                return Err("The replay target must use HTTP or HTTPS.".into());
            }
            let parsed = parse_replay_request(&request_bytes)?;
            let mut builder = reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(
                    replay.timeout_seconds.clamp(1, 300),
                ))
                .tls_danger_accept_invalid_certs(true)
                .redirect(if replay.follow_redirects {
                    reqwest::redirect::Policy::limited(10)
                } else {
                    reqwest::redirect::Policy::none()
                });
            if let Some(proxy_url) = replay
                .proxy_url
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                let scheme = reqwest::Url::parse(proxy_url)
                    .map_err(|error| format!("Invalid proxy URL: {error}"))?
                    .scheme()
                    .to_string();
                if !matches!(
                    scheme.as_str(),
                    "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h"
                ) {
                    return Err("Proxy URLs must use HTTP, HTTPS, SOCKS4/4a or SOCKS5/5h.".into());
                }
                let mut proxy = reqwest::Proxy::all(proxy_url)
                    .map_err(|error| format!("Invalid proxy configuration: {error}"))?;
                if !replay.proxy_username.is_empty() {
                    proxy = proxy.basic_auth(&replay.proxy_username, &replay.proxy_password);
                }
                builder = builder.proxy(proxy);
            }
            let client = builder
                .build()
                .map_err(|error| format!("Client setup failed: {error}"))?;
            let response = client
                .request(parsed.method, target)
                .headers(parsed.headers)
                .body(parsed.body)
                .send()
                .map_err(|error| format!("Replay failed: {error}"))?;
            let status = response.status().as_u16();
            let mut response_bytes = format!("HTTP/1.1 {}\r\n", response.status()).into_bytes();
            for (name, value) in response.headers() {
                response_bytes.extend_from_slice(name.as_str().as_bytes());
                response_bytes.extend_from_slice(b": ");
                response_bytes.extend_from_slice(value.as_bytes());
                response_bytes.extend_from_slice(b"\r\n");
            }
            response_bytes.extend_from_slice(b"\r\n");
            response
                .take(100 * 1024 * 1024 + 1)
                .read_to_end(&mut response_bytes)
                .map_err(|error| format!("Reading the replay response failed: {error}"))?;
            if response_bytes.len() > 100 * 1024 * 1024 {
                return Err("The replay response exceeded the 100 MiB limit.".into());
            }
            Ok((Some(status), response_bytes))
        })();
        let elapsed_millis = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let (status_code, response_bytes, error) = match outcome {
            Ok((status, bytes)) => (status, bytes, None),
            Err(error) => (None, Vec::new(), Some(error)),
        };
        let connection = open_read_write(&session.path)?;
        connection
            .execute(
                "INSERT INTO replay_history
             (source_interaction_id, sent_at, target_url, proxy_url, request, response,
              status_code, elapsed_millis, error, normalised_by_http_client)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1)",
                params![
                    replay.interaction_id,
                    now_millis(),
                    replay.target_url,
                    replay.proxy_url,
                    request_bytes,
                    response_bytes,
                    status_code,
                    elapsed_millis.min(i64::MAX as u64) as i64,
                    error
                ],
            )
            .map_err(|db_error| {
                CommandError::new(
                    "historyUnavailable",
                    "The replay attempt could not be recorded.",
                )
                .detail(db_error.to_string())
            })?;
        Ok(ReplayResult {
            history_id: connection.last_insert_rowid(),
            request_bytes,
            response_bytes,
            status_code,
            elapsed_millis,
            error,
            normalised_by_http_client: true,
        })
    })
    .await
    .map_err(|error| {
        CommandError::new("taskFailed", "Replay did not complete.").detail(error.to_string())
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
            "SELECT captured_at, method, url, status_code, mime_type, \
             substr(request, 1, ?1), length(request), substr(response, 1, ?1), \
             length(response), {notes_expression}, {metadata_expression} \
             FROM http_interactions WHERE id = ?2"
        );
        connection
            .query_row(
                &sql,
                params![MESSAGE_PREVIEW_BYTES, interaction_id],
                |row| {
                    let request_bytes = row.get::<_, Vec<u8>>(5)?;
                    let request_length = row.get::<_, i64>(6)?.max(0) as u64;
                    let response_bytes = row.get::<_, Vec<u8>>(7)?;
                    let response_length = row.get::<_, i64>(8)?.max(0) as u64;
                    Ok(InteractionDetail {
                        id: interaction_id,
                        captured_at: row.get(0)?,
                        method: row.get(1)?,
                        url: row.get(2)?,
                        status_code: row.get(3)?,
                        mime_type: row.get(4)?,
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
                        notes: row.get(9)?,
                        metadata: row.get(10)?,
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
fn open_detail_window(interaction_id: i64, app: tauri::AppHandle) -> Result<(), CommandError> {
    if interaction_id < 1 {
        return Err(CommandError::new(
            "invalidInteraction",
            "Choose a valid interaction before opening a detail window.",
        ));
    }
    let label = format!("detail-{interaction_id}");
    if let Some(window) = app.get_webview_window(&label) {
        window.unminimize().map_err(|error| {
            CommandError::new("windowFailed", "The detail window could not be restored.")
                .detail(error.to_string())
        })?;
        window.set_focus().map_err(|error| {
            CommandError::new("windowFailed", "The detail window could not be focused.")
                .detail(error.to_string())
        })?;
        return Ok(());
    }
    // Keep the bundled resource URL query-free. The frontend obtains the interaction ID
    // from this window's `detail-<id>` label, avoiding the Windows WebView2 resource-request
    // hang observed while resolving `index.html?detail=<id>`.
    let url = tauri::WebviewUrl::App("index.html".into());
    tauri::WebviewWindowBuilder::new(&app, label, url)
        .title(format!("Interaction {interaction_id} — Burp SQLite Viewer"))
        .inner_size(960.0, 720.0)
        .min_inner_size(480.0, 400.0)
        .resizable(true)
        .build()
        .map_err(|error| {
            CommandError::new("windowFailed", "The detail window could not be opened.")
                .detail(error.to_string())
        })?;
    Ok(())
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
async fn save_message_selection(
    interaction_id: i64,
    part: MessagePart,
    text: String,
    app: tauri::AppHandle,
) -> Result<Option<ExportResult>, CommandError> {
    if interaction_id < 1 || text.is_empty() {
        return Err(CommandError::new(
            "invalidSelection",
            "Select message text before saving it.",
        ));
    }
    if text.len() > 4 * 1024 * 1024 {
        return Err(CommandError::new(
            "selectionTooLarge",
            "The selected text exceeds the 4 MiB save limit.",
        ));
    }
    let part_name = match part {
        MessagePart::Request => "request",
        MessagePart::Response => "response",
    };
    let chosen = app
        .dialog()
        .file()
        .set_file_name(format!(
            "interaction-{interaction_id}-{part_name}-selection.txt"
        ))
        .add_filter("Selected message text", &["txt"])
        .blocking_save_file();
    let Some(destination) = chosen.and_then(|path| path.into_path().ok()) else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = text.into_bytes();
        let temporary = destination.with_extension("burp-viewer-exporting");
        fs::write(&temporary, &bytes).map_err(|error| {
            CommandError::new(
                "exportWriteFailed",
                "The selected text could not be written.",
            )
            .detail(error.to_string())
        })?;
        fs::rename(&temporary, &destination).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            CommandError::new(
                "exportWriteFailed",
                "The selected text could not be saved atomically.",
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
        CommandError::new("taskFailed", "Saving the selected text did not complete.")
            .detail(error.to_string())
    })?
}

#[tauri::command]
fn close_database(
    app: tauri::AppHandle,
    state: tauri::State<'_, DatabaseState>,
) -> Result<(), CommandError> {
    *state
        .0
        .lock()
        .map_err(|_| CommandError::new("stateUnavailable", "Database state is unavailable."))? =
        None;
    close_detail_windows(&app);
    Ok(())
}

fn close_detail_windows(app: &tauri::AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with("detail-") {
            let _ = window.close();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(DatabaseState::default())
        .invoke_handler(tauri::generate_handler![
            open_database,
            enable_annotations,
            annotate_interactions,
            replay_request,
            query_interactions,
            get_interaction,
            open_detail_window,
            export_interaction_part,
            save_message_selection,
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

    #[test]
    fn validates_the_exact_metadata_schema_and_ids() {
        assert!(parse_metadata(r#"{"colour":"blue","tag":"review","icon":"flag"}"#).is_some());
        assert!(parse_metadata(r#"{"colour":"cyan","tag":"review","icon":"flag"}"#).is_none());
        assert!(
            parse_metadata(r#"{"colour":"blue","tag":"review","icon":"flag","extra":true}"#)
                .is_none()
        );
    }

    #[test]
    fn escapes_like_metacharacters_for_literal_pattern_filters() {
        let value = r"path%_\tail";
        assert_eq!(
            like_filter_value(FilterOperator::Contains, value),
            r"%path\%\_\\tail%"
        );
        assert_eq!(
            like_filter_value(FilterOperator::StartsWith, value),
            r"path\%\_\\tail%"
        );
        assert_eq!(
            like_filter_value(FilterOperator::EndsWith, value),
            r"%path\%\_\\tail"
        );
    }

    #[test]
    fn parses_replay_request_without_changing_its_body() {
        let raw = b"POST /submit HTTP/1.1\r\nHost: example.test\r\nX-Test: one\r\nX-Test: two\r\n\r\nbody\0bytes";
        let parsed = parse_replay_request(raw).expect("request parses");

        assert_eq!(parsed.method, "POST");
        assert_eq!(parsed.body, b"body\0bytes");
        assert_eq!(parsed.headers.get_all("x-test").iter().count(), 2);
    }

    #[test]
    fn rejects_malformed_structured_replay_headers() {
        let raw = b"GET / HTTP/1.1\r\nnot-a-header\r\n\r\n";
        assert!(parse_replay_request(raw).is_err());
    }

    #[test]
    fn prepares_replay_history_without_changing_capture_rows() {
        let path = fixture_path("replay-history");
        create_fixture(&path, true);
        ensure_replay_history(&path).expect("history table prepared");
        ensure_replay_history(&path).expect("history setup is idempotent");

        let connection = Connection::open(&path).expect("fixture reopens");
        let capture_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM http_interactions", [], |row| {
                row.get(0)
            })
            .expect("capture rows counted");
        let history_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM replay_history", [], |row| row.get(0))
            .expect("history rows counted");
        assert_eq!(capture_count, 1);
        assert_eq!(history_count, 0);

        drop(connection);
        fs::remove_file(path).expect("fixture removed");
    }
}
