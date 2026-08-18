# Design and implementation plan — Burp SQLite Viewer

## Brief

Burp SQLite Viewer is a keyboard-first desktop inspection and replay tool for security testers and developers working at a desk with captured HTTP request/response pairs. Its primary job is to let a user open an existing SQLite capture, find an interaction quickly, inspect the exact bytes without losing table context, annotate it, and safely prepare and send an explicit replay. The application is a Tauri 2 desktop app with a vanilla Svelte 5 + strict TypeScript frontend; British English is used throughout.

## Product boundaries and assumptions

- The supplied schema in `schema.txt` is authoritative for new-compatible databases. A sampled historical capture has the same core columns but lacks `notes` and `metadata`, so schema inspection and backward compatibility are required.
- Captured `request` and `response` BLOBs are evidence and are never rewritten by editing or replaying. Replay operates on a byte-preserving transient draft and stores replay history in a separate table in the active capture database.
- Notes and metadata are mutable. The metadata field has exactly this schema: `{"colour":"<colour-ID>","tag":"<user-defined-tag>","icon":"<icon-ID>"}`. Unknown or invalid JSON is preserved and reported rather than silently replaced. Legacy databases missing these columns require a transactional, user-approved schema migration before annotation is enabled.
- Request bodies and responses may contain binary or invalid UTF-8 data. Preserve bytes end-to-end, decode lossily only for the text presentation, provide hexadecimal fallback, and never send the displayed lossy text unless the user explicitly replaces the original draft.
- Following server redirects is a user-selectable option for replay, TLS certificate validation is always off, and proxy settings default to “No proxy”.
- The timestamp unit is epoch milliseconds, however, validate plausible ranges and expose the raw value in diagnostics.
- There will be only one active database with multiple detached detail windows.
- The supported release envelope is at most 100,000 interaction rows and 100 MB per request or response BLOB.

## Screens

### Traffic workspace

| | |
|---|---|
| **Primary focus** | Find and inspect one captured HTTP interaction without leaving the traffic list. |
| **Visible without acting** | Open database name and mode, row count, active filters/sort, interaction table with tag and icon columns plus colour-highlighted rows, current request/response, and operation status. |
| **Next action** | Select a table row; before a database is open, the prominent **Open database…** button occupies the same workspace focus. |
| **Empty state** | A short explanation of supported SQLite captures, an **Open database…** button, and drag-and-drop as a secondary route. |
| **Failure state** | An inline diagnostic names the file and failed validation step, preserves the previous open database, and offers **Choose another file** or **Retry**. |

### Detached interaction window

| | |
|---|---|
| **Primary focus** | Compare the complete request and response for one interaction in a resizable independent window. |
| **Visible without acting** | Method, URL, status, capture time, database identity, request/response panes, layout choice, notes, and row marker. |
| **Next action** | Read or select text; clearly labelled **Export request…**, **Export response…**, and **Edit and replay…** actions sit in the header without obscuring the reading surface. |
| **Empty state** | Not normally reachable; if the row or source disappears, show its identity and a **Close window** action. |
| **Failure state** | Keep any already-loaded bytes visible, explain that refresh failed, and provide **Retry** without closing the window. |

### Replay workspace

| | |
|---|---|
| **Primary focus** | Review and deliberately send an edited request, then inspect the exact replay result. |
| **Visible without acting** | Source interaction, target URL, editable request line/headers/body, proxy summary, redirect following policy, validation messages, and a clear “unsent draft” state. |
| **Next action** | **Send request** sits beside the target and remains disabled until parsing and URL validation succeed. |
| **Empty state** | Pre-populated from the selected captured request; no blank replay screen is exposed. |
| **Failure state** | Preserve the complete draft, show stage-specific error detail (parse, DNS, proxy, TLS, connect, timeout, send, receive), and offer **Retry**. |

### Settings sheet

| | |
|---|---|
| **Primary focus** | Configure replay networking and durable display preferences. |
| **Visible without acting** | Proxy on/off, proxy address, proxy authentication state, timeout, redirect policy, current pane layout orientation |
| **Next action** | **Save settings**; test-proxy feedback is adjacent to the proxy fields. |
| **Empty state** | Sensible secure defaults are already filled; configuration is not required to inspect a database. |
| **Failure state** | Inline field errors preserve entered values and identify a recovery action. |

## Colour

The restrained neutral palette keeps raw traffic and table state dominant. Marker colours are a separate finite metadata vocabulary and always pair colour with a named label and optional icon.

| Token | Light | Dark | Role | Light contrast | Dark contrast |
|---|---|---|---|---:|---:|
| `--surface` | `#ffffff` | `#0e1216` | Page ground | — | — |
| `--text` | `#14181c` | `#e6eaee` | Body copy | 17.84:1 | 15.55:1 |
| `--text-muted` | `#5a6570` | `#9aa5b1` | Secondary copy and hints | 5.95:1 | 7.51:1 |
| `--accent` | `#1a56c4` | `#7aa9ff` | Primary actions and selected focus | 6.62:1 | 8.01:1 |
| `--danger` | `#b3261e` | `#ff8a80` | Errors and destructive actions | 6.54:1 | 8.24:1 |
| `--success` | `#16653f` | `#5ddba0` | Confirmed save/send outcomes | 7.08:1 | 10.84:1 |
| `--border-strong` | `#7d868f` | `#616e7b` | Control and pane boundaries | 3.70:1 | 3.60:1 |

Ratios are measured against the corresponding surface. Body text exceeds 4.5:1 and meaningful boundaries exceed 3:1. Syntax tokens must also meet 4.5:1 in both themes and will be checked when the chosen highlighter theme is integrated.

## Type

| Role | Face | Used for | Sizes |
|---|---|---|---|
| Body / UI | system-ui stack | Controls, labels, messages and notes | `--text-sm` to `--text-lg` |
| Utility | platform monospace stack | Table data, timestamps, URLs, headers and raw bytes | `--text-xs`, `--text-sm` |

Raw message reading surfaces use a practical 100–120ch viewport where space permits, line wrapping defaults off, and horizontal scrolling is local to each message pane. Notes use the standard `--measure` of 68ch.

## Layout

The workspace follows the normative header/workspace/footer skeleton while using one vertical split between the table and the selected pair; request and response then use one shared, switchable split. This avoids stacked nested scroll regions: the table and each raw message are deliberate peer scroll containers with keyboard focus and visible labels.

```text
Desktop — traffic workspace
┌──────────────────────────────────────────────────────────────────────────┐
│ Burp SQLite Viewer  capture.sqlite · read-only  [Open…] [Columns] [⚙]    │
├──────────────────────────────────────────────────────────────────────────┤
│ Filter: [field ▾] [operator ▾] [value____________] [+ Add]  2 active     │
├──────────────────────────────────────────────────────────────────────────┤
│ ✓ │ Icon │ Tag │ Time ↕ │ Method │ Host │ URL          │ Status │ Size   │
│   │ … keyboard-selectable, virtualised rows; sticky sortable header …    │
├───────────────────────────────┬──────────────────────────────────────────┤
│ Request  [Raw|Hex] [Wrap]     │ Response [Raw|Hex] [Wrap]  [↔|↕]         │
│ GET /… HTTP/1.1               │ HTTP/1.1 200 OK                          │
│ Host: …                       │ Content-Type: …                          │
│ …independent scroll…          │ …independent scroll…                     │
├───────────────────────────────┴──────────────────────────────────────────┤
│ 1,248 rows · 2 filters · sorted newest first · Ready                     │
└──────────────────────────────────────────────────────────────────────────┘
```

Column customisation is a labelled anchored panel, not a modal: checkboxes show/hide columns, including first-class **Tag** and **Icon** columns, drag handles plus keyboard Move up/down controls reorder them, and **Reset columns** restores defaults. Column widths are resizable by pointer and keyboard and persist per user. Filters are visible chips with readable field/operator/value text; the editor supports typed operators appropriate to text, number, timestamp and nullable fields.

Rows support single selection and additive/range multi-selection using the platform-standard modifier and Shift keys. A labelled **Annotate selection** toolbar action opens one compact editor for colour, tag and icon; each field can be left unchanged when the selection contains mixed values. Applying an annotation updates every selected row in one transaction and reports the number changed. The metadata colour highlights the row using a low-saturation background and strong leading rail; the separate Tag and Icon columns ensure the annotation remains identifiable without colour. The icon always has an accessible text name.

When exactly one row is selected, **Export request…** and **Export response…** are available in both the main and detached detail headers; export is disabled for a multi-selection. Each action uses a save-file picker, proposes a safe name such as `interaction-184-request.http`, and writes the original BLOB byte-for-byte. Export is cancellable, never converts line endings or text encoding, never overwrites without the platform save dialogue's confirmation, and reports the final path and byte count. A 100 MB export is streamed in Rust rather than transferred through frontend IPC.

```text
Desktop — replay workspace
┌──────────────────────────────────────────────────────────────────────────┐
│ Replay #184  POST https://example.test/path        [Back to traffic]   │
├──────────────────────────────────────────────────────────────────────────┤
│ Target URL [https://example.test/path____________________] [Send request]│
│ Via SOCKS5 proxy: 127.0.0.1:1080 · redirects off            [Settings]   │
├──────────────────────────────────┬───────────────────────────────────────┤
│ Editable request                 │ Replay response / timing / error      │
│ POST /path HTTP/1.1              │ Not sent                              │
│ Host: example.test               │                                       │
│ …                                │                                       │
├──────────────────────────────────┴───────────────────────────────────────┤
│ Draft valid · original capture unchanged                                 │
└──────────────────────────────────────────────────────────────────────────┘
```

```text
Narrow — traffic workspace
┌────────────────────────────────┐
│ Viewer · capture.sqlite [Open] │
├────────────────────────────────┤
│ [Filters: 2] [Columns]         │
│ compact table (horizontal pan) │
│ selected row…                  │
├────────────────────────────────┤
│ Request [Raw|Hex]              │
│ …                              │
│ Response [Raw|Hex]             │
│ …                              │
├────────────────────────────────┤
│ 1,248 rows · Ready             │
└────────────────────────────────┘
```

At narrow widths the pair always stacks top-to-bottom, nonessential columns are hidden by the default responsive profile, and filters collapse to one labelled control above the table. Replay becomes a single column with the draft before the result. Nothing essential is placed behind an unlabelled icon.

## Signature

Every interaction has a slim “evidence rail” combining a named marker, note indicator and immutable/replayed state, making provenance scannable without relying on colour alone.

## Density

The traffic table is intentionally dense, with fixed utility typography, aligned timestamps and numbers, sticky headers, subtle row separators, and one selected-row treatment. Raw message panes are dense but calm, while replay settings and annotation editing use generous form spacing. Virtualisation and keyset pagination prevent large captures from degrading interaction latency.

Table density is a persisted display preference with **Compact** and **Comfortable** modes. Compact is the default for this traffic-analysis workspace and reduces row height and vertical cell padding while retaining the minimum keyboard focus treatment and legible line height. The density control changes table rows only; it does not compress forms, alerts, or request/response reading surfaces.

Routine status and diagnostics do not consume permanent space above the interaction table. Short success/progress messages appear in the footer status region. Warnings and errors appear in a footer-adjacent alert region with a visible dismiss action; actionable failures remain available until dismissed or superseded. A compact diagnostics disclosure provides access to technical details such as the raw epoch-millisecond timestamp without pushing the request/response panes below the initial viewport.

## Keyboard

- `Ctrl/Cmd+O`: open a database.
- `Ctrl/Cmd+F`: focus the first/global filter control.
- Arrow keys, Page Up/Down, Home/End: move table selection without moving focus into every cell.
- `Enter`: focus the selected interaction detail; `Ctrl/Cmd+Enter`: open it in a detached window.
- `Ctrl/Cmd+Shift+R`: create a replay draft from the selected row; never sends immediately.
- `Ctrl/Cmd+E`: export the selected interaction's request or response through a labelled choice panel.
- `Ctrl/Cmd+S`: save notes/metadata or settings in the active editable surface.
- `Ctrl/Cmd+Shift+P`: switch request/response pane orientation.
- `Escape`: close the current anchored panel or return from replay while preserving a dirty draft after warning inline.
- Splitter handles expose separator semantics and support arrow-key resizing; sortable headers and column ordering have explicit keyboard controls.

Checked workflows: open database, filter/sort/select one or many rows, customise columns, inspect both messages, export exact request/response bytes, detach details, annotate a single or multi-row selection, prepare replay, change target/proxy, send, inspect result, and recover from failure can all be completed without a mouse.

## Architecture

### Frontend

- Use vanilla Svelte 5 runes. Keep source state in small `.svelte.ts` stores: database session, query model, selection, layout preferences, annotations and replay draft/result. Use `$derived` only for pure computed presentation; the official Svelte documentation states derived expressions must be side-effect free (`/src/svelte/documentation/docs/02-runes/03-$derived.md:6`).
- Components: `AppShell`, `DatabaseEmptyState`, `TrafficToolbar`, `FilterBuilder`, `ColumnChooser`, `InteractionTable`, `MessagePair`, `RawMessagePane`, `AnnotationEditor`, `ExportMenu`, `ReplayWorkspace`, `ProxySettings`, and `StatusBar`.
- Use semantic HTML first: real table semantics (or an ARIA grid only if row virtualisation makes a semantic table impossible), buttons for sortable headers, labelled inputs, live regions for load/send status, and separators for split handles.
- Choose a small syntax-highlighting library only after an isolated benchmark. Prefer tokenisation in a Web Worker, grammar limited to HTTP start line/headers plus MIME-aware body highlighting, escaped output, incremental rendering, and a plain-text fallback. Do not use `contenteditable` for the replay editor; use a textarea-backed highlighted editor so selection, input methods and screen readers remain reliable.
- Persist UI-only preferences (column order/visibility/width, filters if opted in, theme, pane orientation and splitter positions) in the app configuration directory, not in the capture database.

### Rust/Tauri backend

- `database`: validate the selected file, inspect `PRAGMA table_info`, open through `rusqlite`, manage the active connection/session behind Tauri managed state, and return typed DTOs. Tauri documents registering managed state with `Builder::manage` and accessing it through `State` (`/src/tauri-docs/src/content/docs/develop/calling-rust.mdx:479`).
- `query`: accept a typed allow-list query AST, compile identifiers/operators to parameterised SQL, and never accept SQL fragments from the frontend. Default sort is `captured_at DESC, id DESC`; use keyset pagination, return total/filtered counts separately, fetch BLOBs only for the selected row, and cancel superseded queries.
- `annotations`: transactionally migrate legacy databases to add missing `notes` and `metadata` columns after user approval; validate the exact metadata shape `{ colour: string, tag: string, icon: string }`, allow only known colour/icon IDs, retain arbitrary user tag text within a documented length limit, and apply single- or multi-row changes in one transaction. Preserve invalid existing JSON until the user explicitly replaces it, surface write conflicts, and keep unsaved edits recoverable. Annotation controls are disabled with an explanation when the capture is read-only.
- `export`: stream the selected original request or response BLOB directly from SQLite to a user-selected file with byte counts, cancellation, atomic temporary-file replacement and actionable filesystem errors.
- `replay`: retain the original request as an opaque byte array and record edits as byte-range replacements so deliberately malformed syntax, duplicate headers, line endings and invalid UTF-8 survive unchanged unless explicitly edited. The initial structured HTTP sender derives routing information without normalising the stored draft, supports direct, HTTP proxy, HTTPS CONNECT proxy and SOCKS4/4a/5/5h routes, plus redirect, timeout and body-size settings. It must report whenever its HTTP library cannot transmit the malformed draft byte-for-byte. A future, separately permissioned **Expert raw socket replay** mode will provide literal on-wire transmission, with a prominent hazard state, destination preview and no accidental activation. Secrets are redacted in logs and never included in frontend error telemetry.
- `replay_history`: create and migrate a dedicated table inside the capture database, reference the source interaction ID, and store the send time, target, proxy mode, exact replay request BLOB, response BLOB when available, outcome/error and timings. History writes are transactional and capture files must be writable before replay is enabled.
- `windows`: create or focus a uniquely labelled detail window for an interaction and pass only a database-session token plus row ID, never raw traffic in a URL. Tauri supports programmatic `WebviewWindow` creation and creation/error events (`/src/tauri-docs/src/content/docs/learn/mobile-multiwindow.mdx:258`). Detail windows are resizable and restore bounds within the current monitor.
- `ipc.ts`: the only frontend import of `invoke` from `@tauri-apps/api/core`; every command has strict request/response types and maps Rust `snake_case` fields to TypeScript `camelCase` via serde attributes. Register all app commands in one `generate_handler!` call, as required by Tauri (`/src/tauri-docs/src/content/docs/develop/calling-rust.mdx:551`).

### File opening and capabilities

- Use the Tauri dialog plugin for a single-file chooser filtered to `.sqlite`, `.sqlite3` and `.db`; it returns a desktop filesystem path (`/src/tauri-docs/src/content/docs/plugin/dialog.mdx:78`, usage at `:161`). Use its save-file dialogue for request/response export (`/src/tauri-docs/src/content/docs/plugin/dialog.mdx:181`). Validate SQLite magic, required table/columns and readable BLOB types in Rust before replacing the active session.
- Grant the main window only dialog/window permissions it needs. Detail windows do not receive dialog or replay permissions unless a workflow requires them. Tauri recommends per-window least-privilege capabilities and separate capability files by category (`/src/tauri-docs/src/content/docs/learn/Security/capabilities-for-windows-and-platforms.mdx:86`); plugin commands are blocked until capability permissions are declared (`/src/tauri-docs/src/content/docs/learn/window-customization.mdx:52`).
- Custom Rust commands remain narrowly typed and registered; plugin ACL checks are run after capability changes.

## Data contracts

```text
DatabaseSummary { sessionId, displayName, path, mode, schemaVersion, rowCount }
InteractionPage { rows: InteractionSummary[], nextCursor?, filteredCount, queryRevision }
InteractionSummary { id, capturedAt, tool, scheme, host, port, method, url,
                     statusCode, mimeType, responseLength, notesPreview?, colour?, tag?, icon? }
InteractionMetadata { colour: ColourId, tag: string, icon: IconId }
InteractionDetail { summary, requestBytes, responseBytes, notes?, metadata?, fingerprint,
                    capturedAtRawMilliseconds }
QuerySpec { filters: FilterClause[], sort: SortClause[], cursor?, pageSize }
BulkAnnotationPatch { interactionIds, colour?: ColourId | null, tag?: string | null,
                      icon?: IconId | null }
ExportSpec { interactionId, part: "request" | "response", destinationPath }
ReplayDraft { sourceFingerprint, originalBytes, byteEdits, targetUrl, route }
ReplayResult { status?, headers, bodyBytes, timings, remoteAddress?, error? }
```

Filter operators are fixed by type: text (`contains`, `equals`, `starts with`, `ends with`, `is empty`), numbers (`=`, `!=`, `<`, `≤`, `>`, `≥`, `between`), timestamps (`before`, `after`, `between`) and metadata (`has note`, colour/tag/icon equals). Multiple filters combine with AND in version 1; an advanced AND/OR group builder is deferred until evidence shows it is needed. `captured_at` is interpreted as epoch milliseconds only after a plausible-range check; diagnostics include the untouched integer value.

## Safety and correctness decisions

- Replay is never available as a one-click action from the table. It always opens a visible draft, clearly distinguishes original and edited values, and requires an explicit send.
- A malformed captured request remains byte-identical in the replay draft. Structured replay never claims byte-perfect wire fidelity when the transport library normalises it; the future raw-socket mode is the explicit path for that guarantee.
- Warn before replay to loopback, link-local and private destinations when the edited target crosses trust boundaries; show resolved destination and proxy route. This is a security-testing tool, so allow an explicit per-send override rather than silently forbidding legitimate work.
- Support up to 100,000 rows and 100 MB request/response BLOBs. Stream large BLOB display and export, and offer “load remainder” rather than freezing the UI. Never truncate exported or replayed bytes; a display truncation is always visible.
- Sanitize syntax-highlighted output and keep a strict bundled-build CSP. No remote fonts, scripts or highlighting assets.
- Store proxy passwords in an OS credential facility if supported; otherwise keep them in memory for the session and state that clearly.
- Persist every replay attempt in the capture database, including time, source interaction, target, proxy route, exact sent request bytes, received response bytes, outcome and timings. Redact secrets only from logs, not from the UI or exact replay evidence BLOB. Replay is unavailable when history cannot be written atomically; history clearing is explicit and transactional.

## Implementation phases

### Phase 0 — scaffold and prove the toolchain

1. Scaffold from the harness template into `/src/burp-sqlite-viewer` without overwriting `schema.txt`; use the plain frontend path unless the user explicitly requests shadcn-svelte.
2. Run the harness doctor, install pinned dependencies, and verify `npm run check`, frontend build, `cargo check`, and a minimal `npm run tauri build` path.
3. Encode the agreed palette/type/spacing tokens in `src/app.css` before components.

**Exit:** empty application shell builds; light/dark token contrast passes; no SvelteKit dependency exists.

### Phase 1 — database opening and read-only table

1. Add dialog capability and typed `open_database`, `query_interactions`, `get_interaction`, `export_interaction_part` and `close_database` commands.
2. Validate schema variants and create fixtures for current, legacy-without-annotations, malformed, empty, large-BLOB and read-only files.
3. Implement keyset-paged table, sticky headers, selection and lazy detail loading.

**Exit:** a 100,000-row capture opens without mutation, rows remain responsive, selecting a row shows byte-faithful request/response data, 100 MB request/response BLOBs export byte-identically, and errors preserve the prior session.

### Phase 2 — table customisation, sorting and filters

1. Implement column catalogue, including visible Tag and Icon columns, visibility/order/width preferences and reset.
2. Implement allow-listed server-side sorts and typed filter compiler with debounced, cancellable queries.
3. Add single, additive and range row selection, table/grid keyboard navigation, and accessible announcements for selection/sort/filter/result changes.

**Exit:** every schema column except raw BLOBs is optionally visible; sorting/filtering combinations have SQL unit tests and keyboard acceptance tests.

### Phase 2.1 — workspace density and diagnostic ergonomics

1. Move routine operation status into the persistent footer and replace the top diagnostics block with a footer-adjacent, dismissible alert region plus a compact diagnostics disclosure for technical details.
2. Add persisted **Compact** and **Comfortable** table-density modes, defaulting to Compact, using design tokens for row height and vertical cell padding.
3. Add the text `ends with` operator to the typed frontend contract and Rust allow-list compiler, escaping SQL `LIKE` metacharacters identically to `contains` and `starts with`.
4. Add tests for alert dismissal and retention, density persistence and keyboard usability, and literal `%`, `_`, and `\` behaviour in all three pattern filters.

**Exit:** the selected request/response remains visible in the initial desktop viewport at the supported default window size; dismissed alerts stay out of the way until a new diagnostic occurs; Compact mode materially increases visible row count without clipping focus or text; and `ends with` is parameterised and behaves literally for escaped wildcard characters.

### Phase 3 — message inspection and detached windows

1. Build raw/hex panes, HTTP/JavaScript syntax highlighting, copy/save-selection and byte-exact request/response export affordances, wrapping control and accessible splitters.
2. Add left-right/top-bottom persistence and content-responsive fallback.
3. Add resizable detached windows, unique labels, focus-existing behaviour and lifecycle/session invalidation.

**Exit:** binary and invalid UTF-8 fixtures display safely; main and detached panes restore layout; no secondary window has excess capabilities.

### Phase 4 — notes and metadata

1. Implement the exact `{ colour, tag, icon }` metadata schema with preservation and diagnostics for invalid legacy values.
2. Implement single- and multi-selection annotation with visible pending/saved/error state, conflict detection, colour picker, user-defined tag field and named icon picker.
3. Add colour/tag/icon filters, colour row highlighting, dedicated Tag and Icon columns, and evidence-rail rendering.

**Exit:** original request/response bytes remain unchanged; annotations survive restart in writable captures, read-only captures clearly disable mutation, and colour is never the only marker cue.

### Phase 5 — safe replay

1. Build a byte-preserving HTTP/1 parser and byte-range draft editor with malformed-message diagnostics and original-vs-draft diff.
2. Add URL override; direct, HTTP, HTTPS CONNECT, SOCKS4/4a/5/5h routes; authentication; redirect/timeout controls; and OS-safe secret handling for proxy authentication.
3. Implement send, cancellation, response streaming/caps, timing/error taxonomy and transactional replay history in the capture database.
4. Threat-model request smuggling/framing, SSRF-like target changes, proxy credential exposure, and decompression bombs.
5. Specify, but do not enable in version 1, the separate capability boundary and hazardous UI for future expert raw-socket replay.

**Exit:** replay interoperability tests pass against local origin/proxy fixtures for HTTP, HTTPS CONNECT and SOCKS routes, bodies, duplicate/malformed headers, timeouts and cancellation; source/draft bytes remain preserved, normalisation is disclosed, every attempt is stored atomically in the capture, and sending always requires a visible draft and explicit action.

### Phase 6 — hardening and release

1. Run component checks, Rust tests, database property tests, end-to-end keyboard flows, accessibility audit and both-theme visual review.
2. Benchmark 10k and the supported maximum 100,000-row fixtures plus 100 MB BLOBs; define budgets for open, filter, multi-row selection, display, export and scroll latency.
3. Run capability/ACL checks, dependency audit, CSP verification in a bundled build, replay security tests and platform packaging smoke tests.
4. Document supported schemas, annotation modes, proxy semantics, byte handling, shortcuts and recovery/export procedures.

**Exit:** release checklist passes on Linux, macOS and Windows; known limitations are visible in-app and documented.

## Test strategy

- Rust unit/property tests: schema validation, epoch-millisecond range checks, query AST compilation, pagination stability, byte parsing/serialisation, exact metadata validation, export fidelity, replay routing and replay-history transactions.
- SQLite integration tests: legacy/current schemas, concurrent external writes, locked files, corrupt files, huge rows, null/invalid metadata and read-only media.
- Replay integration tests: controlled local HTTP/TLS origin plus HTTP, HTTPS CONNECT and SOCKS proxies; authentication, duplicate headers, chunked/fixed bodies, malformed messages, redirects, cancellation, history rollback and size limits. No public-network dependency.
- Frontend component tests: filter builder, sort state, Tag/Icon columns, column preferences, range selection, bulk annotation mixed states, splitter keyboard behaviour, dirty replay draft and status announcements.
- End-to-end tests: open → filter → multi-select → annotate → select → export request/response → detach → draft → configure each proxy type → send → inspect history; repeat entirely by keyboard.
- Visual/accessibility review: 200% zoom, narrow window, high contrast, reduced motion, both themes, long URLs, binary bodies and screen-reader labelling.

## Confirmed decisions

- Keep routine diagnostics out of the top workspace: use footer status, dismissible footer-adjacent alerts, and an on-demand technical-details disclosure.
- Default the interaction table to a persisted Compact density, with Comfortable available as a display preference.
- Include `ends with` among the allow-listed text filter operators.
- Export a selected interaction's request or response as its own byte-exact file.
- Metadata is exactly `{ "colour": "<colour-ID>", "tag": "<user-defined-tag>", "icon": "<icon-ID>" }`; Tag and Icon are table columns, colour highlights the row, and annotations support one or many selected rows.
- `captured_at` is epoch milliseconds; validate plausible ranges and expose raw values in diagnostics.
- Keep one active database and allow multiple detached detail windows.
- Preserve deliberately malformed request bytes throughout capture, editing and structured replay. Reserve guaranteed literal on-wire transmission for a future, separate hazardous raw-socket mode.
- Support HTTP, HTTPS CONNECT and SOCKS proxies.
- Store replay history in the capture database.
- Support up to 100,000 interactions and 100 MB per request or response BLOB.

## Plan review

- Every visible element supports opening, finding, inspecting, annotating or replaying traffic; replay/settings complexity is progressively disclosed.
- The traffic table remains the single focus of the main screen, and replay has its own focused workspace rather than a modal chain.
- Correctness and safety outrank replay convenience: original capture request/response evidence remains immutable, annotation/history writes are transactional, failures preserve work, and network effects require an explicit action.
- The design contains no carousel, desktop hamburger, hidden primary action, icon-only navigation or decorative animation.
- Implementation must not begin until this plan is reviewed and agreed; after agreement, update tokens first, build components through the component workflow, and run the UI review workflow before calling the interface finished.
