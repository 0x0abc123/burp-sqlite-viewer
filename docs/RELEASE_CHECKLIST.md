# Release checklist

This file distinguishes automated evidence from checks that require a real packaged app or
another operating system. Never mark a platform passed based only on cross-compilation.

## Current Linux verification — 18 August 2026

- Passed: Svelte/TypeScript check, production Vite build, eight Rust tests, strict Clippy,
  Rust formatting, npm production dependency audit, UI mechanical audit, ACL audit, diff
  whitespace check, and a Tauri debug production-protocol build.
- Not available in this environment: `cargo audit`.
- Still requires manual evidence: performance measurements, replay proxy matrix, packaged
  CSP runtime inspection, screen reader/visual checks, signed bundles, and macOS/Windows.

## Automated gates

- [ ] `npm ci`
- [ ] `npm run check`
- [ ] `npm run build`
- [ ] `cargo fmt --check` in `src-tauri`
- [ ] `cargo test` in `src-tauri`
- [ ] `cargo clippy --all-targets -- -D warnings` in `src-tauri`
- [ ] `npm audit --omit=dev`
- [ ] Rust dependency advisory scan (`cargo audit`, when installed)
- [ ] Harness UI audit with `--include-ui`
- [ ] Harness ACL audit
- [ ] `git diff --check`

## Performance budgets

Measure a release build on representative hardware, with a cold open and five warm runs.
Record hardware, OS, database storage, median and worst result.

| Operation | Fixture | Budget |
|---|---:|---:|
| Open and validate | 100,000 rows | 2 s median, 5 s worst |
| First table page | 100,000 rows | 500 ms median, 1 s worst |
| Indexed/equality filter | 100,000 rows | 750 ms median, 2 s worst |
| Text contains filter | 100,000 rows | 2 s median, 5 s worst |
| Select/annotate | 10,000 rows | 2 s median, 5 s worst |
| Display 1 MB preview | 100 MB BLOB | 1 s median, 3 s worst |
| Export | 100 MB BLOB | 3 s median, 10 s worst |
| Table scrolling | populated viewport | no sustained frame below 30 fps |

Fail a budget rather than silently raising it. Capture query plans and profiling evidence
before changing a threshold.

## Replay security matrix

- [ ] Direct HTTP and HTTPS against disposable local origins
- [ ] HTTP proxy and HTTPS CONNECT proxy, with and without authentication
- [ ] SOCKS4, SOCKS4a, SOCKS5 and SOCKS5h routes
- [ ] Redirect disabled and ten-hop ceiling
- [ ] Timeout at both configured extremes
- [ ] Duplicate headers, fixed body, binary body and malformed-header rejection
- [ ] 100 MB request/response boundary and oversized-response rejection
- [ ] Failed attempts retained in `replay_history`
- [ ] Source BLOB unchanged after draft editing and replay
- [ ] Proxy password absent from local storage and diagnostic output
- [ ] Target override to loopback/private ranges visibly understood by tester

Use only controlled local origins and proxies. Do not point release tests at public systems.

## UI and accessibility sign-off

- [ ] Entire open → filter → multi-select → annotate → export → detach → replay flow by keyboard
- [ ] Visible focus at every stop and after errors/dismissals
- [ ] Screen-reader names and live error/status announcements
- [ ] 200% zoom without lost controls or two-dimensional page scrolling
- [ ] Narrowest supported window
- [ ] Light and dark system themes
- [ ] High-contrast and reduced-motion settings
- [ ] Long URLs, empty results, invalid metadata, binary content and 100 MB truncation states

## Packaging matrix

For each platform, install the produced artefact on a clean user account, open a fixture,
export a BLOB, open/close a detached window, and replay to a disposable local server.

- [ ] Linux package and executable smoke test
- [ ] macOS signed/notarised bundle smoke test
- [ ] Windows signed installer smoke test
- [ ] Production CSP verified from the bundled protocol, not the Vite dev server
- [ ] Application identifier changed from `com.example.burp-sqlite-viewer`
- [ ] Placeholder application icons replaced
- [ ] Version and release notes updated

## Known version 1 limitations

- Structured replay normalises HTTP and cannot guarantee malformed bytes on the wire.
- Raw-socket replay is deliberately unavailable.
- In-flight replay cancellation is not yet available; the configured timeout is the escape.
- Replay history is retained in the capture but has no history-browser or deletion UI.
- TLS certificate validation is disabled for replay to support interception proxies.
- Mobile targets are not supported or initialised.
