# v1.0.61 — 15 Bug Fixes in `ingest --mode claude-code`

## Status: ALL FIXED — awaiting release

| ID | Severity | Bug | Fix Status |
|----|----------|-----|------------|
| B00 | CRITICAL | `--bare` disables OAuth — `claude -p` always fails for Pro/Max users | FIXED: `--dangerously-skip-permissions` for OAuth, `--bare` only when `ANTHROPIC_API_KEY` set |
| B00a | HIGH | `--max-turns 1` insufficient — Claude returns `error_max_turns` | FIXED: changed to `--max-turns 3` |
| B01 | HIGH | `--resume` flag accepted but not implemented | FIXED: resets stuck `processing` to `pending` |
| B02 | HIGH | `--retry-failed` flag accepted but not implemented | FIXED: resets `failed` to `pending` |
| B03 | HIGH | `--dry-run` ignored in claude-code mode | FIXED: emits preview events without spawning Claude |
| B04 | HIGH | No subprocess timeout on `claude -p` | FIXED: `wait-timeout` crate, `--claude-timeout` flag (default 300s) |
| B05 | HIGH | Error message lost when claude exits non-zero | FIXED: parse stdout JSON for real error, fallback to stderr |
| B06 | HIGH | No deduplication — duplicate names crash with UNIQUE | FIXED: `find_by_name_any_state` + `update` + FTS5 sync |
| B07 | HIGH | No retry on `--json-schema` cold-start failure | FIXED: retry once after 2s delay (Issue #23265 workaround) |
| B07a | HIGH | source CHECK constraint rejects `"claude-code"` | FIXED: changed to `"agent"` |
| B08 | MEDIUM | Subprocess inherits full environment including secrets | FIXED: `env_clear()` + selective injection of 14 vars |
| B09 | MEDIUM | Plugins loaded per invocation increase startup latency | FIXED: conditional `--bare` when `ANTHROPIC_API_KEY` is set |
| B10 | MEDIUM | `structured_output` field sometimes absent in response | FIXED: fallback parsing of `result` field as JSON |
| B11 | LOW | FileEvent index counter off-by-one | FIXED: consistent 0-based indexing before counter increment |
| B12 | LOW | Invalid `entity_type` from Claude silently discarded | FIXED: `tracing::warn!` with entity name and type |
| B13 | LOW | Non-canonical relationship types not validated | FIXED: `warn_if_non_canonical()` before insertion |

## Validation Results

- `cargo fmt --all --check` — 0 diffs
- `cargo clippy --all-targets --all-features -- -D warnings` — 0 warnings
- `cargo test --all-features` — 596 PASS, 0 FAILED (587 unit + 9 integration)
- `cargo doc --no-deps --all-features` (RUSTDOCFLAGS="-D warnings") — 0 warnings
- Anti-pattern audit — 0 println, 0 dbg, 0 todo, 0 unwrap (outside tests), 0 Portuguese in code

## Files Changed

| File | Lines Changed | Changes |
|------|--------------|---------|
| `Cargo.toml` | 2 | version 1.0.60 → 1.0.61, added `wait-timeout = "0.2"` |
| `src/commands/ingest_claude.rs` | ~160 | All 15 bug fixes: rewrite extract_with_claude(), add dry-run/resume/retry-failed, dedup, retry, index, warnings |
| `src/commands/ingest.rs` | 7 | `--claude-timeout` flag, `pub(crate) derive_kebab_name`, claude-code EXAMPLES in help |
| `CHANGELOG.md` | 22 | v1.0.61 section |
| `CHANGELOG.pt-BR.md` | 22 | v1.0.61 section (PT mirror) |

## New Dependency

- `wait-timeout = "0.2"` (v0.2.1) — cross-platform subprocess timeout, MIT/Apache-2.0, 0 transitive deps beyond libc

## New Flag

- `--claude-timeout <SECONDS>` (default 300) — per-file subprocess timeout for `ingest --mode claude-code`

## Architectural Root Cause

All 15 bugs shared one root cause: `ingest_claude.rs` reimplemented the persistence pipeline from scratch instead of reusing the `remember` command's infrastructure. The v1.0.61 fix reuses: `memories::find_by_name_any_state()`, `memories::update()`, `memories::sync_fts_after_update()`, `parsers::warn_if_non_canonical()`.
