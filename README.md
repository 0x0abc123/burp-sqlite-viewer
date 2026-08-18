# Burp SQLite Viewer

A local Tauri 2 desktop viewer for captured HTTP request/response pairs stored in SQLite.
It supports large captures, column filtering and sorting, detachable message views,
annotations, byte-exact export, and explicit structured request replay.

## Run and build

```sh
npm install
npm run check
npm run tauri dev
npm run tauri build
```

`npm run dev` runs only the browser frontend. Native database dialogs, detached windows,
exports and replay require `npm run tauri dev`.

Platform installers are written below `src-tauri/target/release/bundle/`. A non-bundled
debug executable is written to `src-tauri/target/debug/burp_sqlite_viewer` after a Tauri
development build, but it expects the Vite development server and is not a distributable
application.

Pushing a tag beginning with `v` runs `.github/workflows/release.yml`. It builds Linux and
Windows x86_64 bundles plus Apple Silicon and Intel macOS bundles, then uploads them to a
GitHub Release for that tag:

```sh
git tag v0.1.0
git push origin v0.1.0
```

Update the versions in `package.json` and `src-tauri/tauri.conf.json` before tagging.
Production Windows and macOS releases should use project-owned signing credentials; the CI
workflow currently applies only an ad-hoc macOS signature.

## Supported captures

The database must contain `http_interactions` with the fields described in `schema.txt`.
Both the legacy schema and the current schema with `notes` and `metadata` columns are
accepted. A writable legacy capture can be upgraded in place; reopening read-only media
keeps annotation and replay actions disabled.

- `captured_at` is epoch milliseconds. Values before 2000 or implausibly far in the future
  remain visible and their raw integer is included in diagnostics.
- Up to 100,000 interaction rows and request/response BLOBs up to 100 MB are supported.
- The table fetches bounded pages. Message panes preview the first 1 MB; export and unchanged
  replay read the complete BLOB.
- Original request and response BLOBs are never modified.

## Finding and inspecting traffic

Click anywhere on a row to select it. Use Ctrl/Cmd-click to toggle rows and Shift-click to
extend a selection. Column headings sort the table. The filter builder supports equality,
contains, starts-with, ends-with, empty, and numeric comparisons where applicable.

The column chooser and Compact/Comfortable density setting are persisted locally. The
request/response panes can be switched between side-by-side and stacked layouts. Detached
detail windows remain associated with the one active database and close when it closes.

All controls participate in the normal Tab order; Space activates a focused button and
Enter activates buttons or submits the focused form control according to platform webview
behaviour. There are no application-only hidden keyboard shortcuts.

## Annotations

One or several selected rows can be annotated with notes and this exact metadata object:

```json
{"colour":"<colour-ID>","tag":"<user-defined-tag>","icon":"<icon-ID>"}
```

Colours are `red`, `orange`, `yellow`, `green`, `blue`, `purple`, or empty. Icons are
`star`, `flag`, `bookmark`, `bug`, `check`, or empty. Tags are limited to 100 characters.
Colour highlights the row; tag and icon also have dedicated columns so colour is not the
only annotation cue.

## Export and recovery

Select an interaction and use its request or response export action. Export writes the
complete BLOB to a temporary sibling file and renames it into place, avoiding a partially
written destination. Export never changes the database.

The application has no destructive database operation. If annotation setup or a replay
history write fails, keep the original capture and retry from a writable copy. SQLite
sidecar files (`-wal` and `-shm`) should be copied together with the database when recovering
an actively written capture.

## Structured replay and proxies

Replay always opens a visible draft and requires **Send request**. Leaving the draft
unchanged sends the complete captured request BLOB through the structured HTTP client;
editing in hexadecimal mode preserves the entered byte sequence in replay history.

Direct connections and `http://`, `https://`, `socks4://`, `socks4a://`, `socks5://`, and
`socks5h://` proxy URLs are accepted. HTTPS targets through HTTP proxies use CONNECT.
Proxy usernames may be remembered locally; passwords remain in memory only. Redirects are
off by default and can be enabled up to ten hops. Timeouts are limited to 1–300 seconds.

Structured replay may normalise the request line, headers, framing, and deliberately
malformed syntax. TLS certificate validation is disabled for interception-proxy workflows.
Treat target overrides and proxy credentials as security-sensitive. The proposed future
raw-socket replay mode is intentionally unavailable because literal malformed traffic needs
a separate, prominently hazardous capability boundary.

Every attempt is appended to `replay_history` in the active capture, including target,
proxy route, exact draft bytes, response bytes, status, elapsed time, and error. Replay is
disabled when this evidence cannot be stored.

## Security and privacy

- Captures, annotations and replay history may contain credentials and personal data.
- No telemetry or automatic upload is performed.
- The bundled CSP allows only application resources and Tauri IPC.
- Replay is an explicit network side effect and can reach private or loopback addresses.
- Response bodies are capped at 100 MB; automatic content decoding is not enabled.

See [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md) for verified checks and remaining
platform sign-off work.

## Development layout

```text
src/app.css              design tokens and global styling
src/lib/ipc.ts           typed wrappers for Rust commands
src/lib/                 Svelte components
src-tauri/src/lib.rs     database, export, window and replay commands
src-tauri/capabilities/  least-privilege desktop ACL
```

Do not run `shadcn-svelte init`; it overwrites `src/app.css`. Add components with
`npm run ui add <component>`.
