# Test Plan v1.0.82 — Validation Post-Publication

- Created 2026-06-15 ahead of v1.0.82 publication on GitHub and crates.io
- Focus: Layer 7 of `docs/TEST_PLAN.md` applied specifically to the v1.0.82 release
- Target: binary installed from crates.io (`cargo install sqlite-graphrag --version 1.0.82`)
- Environment: isolated database in `/tmp/test-v1-0-82-cli/` with namespace `test-cli-v1-0-82`


## Objective
### Purpose
- Validate end-to-end behavior of the v1.0.82 CLI installed from crates.io
- Confirm that the published binary responds to the documented JSON contract
- Detect regressions introduced between v1.0.81 and v1.0.82 before any production adoption
- Serve as a reproducible smoke test for users and CI
- Validate the 5 new ADR decisions (0036-0040) ship as specified
### Out of Scope
- Source code audit (already done during release via A1/A2)
- Performance comparison between versions
- Load or stress test
- Mutation coverage


## Prerequisites
### Environment
- `sqlite-graphrag` v1.0.82 binary installed in `~/.cargo/bin/`
- `PATH` with `~/.cargo/bin` before `/usr/bin` (prevents shadowing by `timeout`)
- `atomwrite` 0.1.18+ available for atomic write of the report
- `jaq` available for JSON parsing
- `rg` (ripgrep) available for log search
- `claude` OR `codex` CLI on `PATH` (OAuth required)
### Isolation
- Dedicated test directory: `/tmp/test-v1-0-82-cli/`
- Isolated database: `/tmp/test-v1-0-82-cli/test.sqlite`
- Dedicated namespace: `test-cli-v1-0-82`
- Variable `SQLITE_GRAPHRAG_DB_PATH` on every invocation
### Pre-flight: codex login
- Run `codex login` once to refresh the OAuth refresh token
- This is the operator action mandated by the 2026-06-14 codex OAuth 401 incident
- Failure to run `codex login` before this test plan risks intermittent OAuth failures


## Phases
### Phase 1 — Installation Verification
- Confirm `sqlite-graphrag --version` returns `1.0.82`
- List subcommands via `--help` and verify the 4 new subcommands: `pending`, `pending-embeddings`, `slots`, `embedding`
- Inspect global flags and validate the new `--llm-backend`, `--llm-fallback-mode`, `--llm-max-host-concurrency`, `--llm-slot-wait-secs`, `--llm-slot-no-wait`, `--graceful-shutdown-secs`
- Confirm `src/constants.rs::SHUTDOWN_EXIT_CODE == 19` via `grep -r "SHUTDOWN_EXIT_CODE" src/`

### Phase 2 — Schema Migrations V014 and V015
- Run `sqlite-graphrag migrate --json` on a v1.0.81 database
- Parse the response and confirm `v014_applied: true` and `v015_applied: true`
- Inspect `schema_meta.dim` and confirm it is still 64 (or the operator-configured value)
- Run `sqlite-graphrag health --json` and confirm `schema_ok: true` and `schema_version >= 15`
- Verify the `pending_memories` table exists via `sqlite3 test.sqlite ".schema pending_memories"`
- Verify the `pending_embeddings` table exists via `sqlite3 test.sqlite ".schema pending_embeddings"`

### Phase 3 — GAP-001 (ADR-0036): Three-Stage remember Checkpoint
- Run `sqlite-graphrag remember --name test-gap-001 --type decision --body "GAP-001 smoke test" --json`
- Run `sqlite-graphrag pending list --json` and confirm the response includes `action: "pending_list"`, `pending[]`, and `counts`
- Run `sqlite-graphrag pending show <id> --json` on the first queued or done row
- Run `sqlite-graphrag pending cleanup --filter-status done --yes --json`
- Send SIGTERM to a running `remember` invocation via `kill -SIGTERM $(pgrep -f "sqlite-graphrag remember")`
- Confirm the row stays in `queued` state via `sqlite-graphrag pending list --filter-status queued`
- Validate the response against `docs/schemas/pending-list.schema.json` via `jsonschema --instance`

### Phase 4 — GAP-002 (ADR-0037): Shutdown JSON Envelope at Exit Code 19
- Start a long `remember` in the background: `sqlite-graphrag remember --name test-gap-002 --type note --body-file ./big.md &`
- Send SIGTERM: `kill -SIGTERM $!`
- Capture the exit code and confirm it equals 19
- Parse stdout and confirm it matches the `shutdown-envelope.schema.json` contract: `error: true`, `code: 19`, `signal`, `graceful`
- Repeat with SIGINT and SIGHUP; confirm `signal` field reflects the actual signal name

### Phase 5 — GAP-003 (ADR-0038): --llm-backend User Choice
- Run `sqlite-graphrag remember --name test-gap-003 --type note --body "..." --llm-backend claude --json`
- Confirm the response does not invoke codex (no codex subprocess spawned)
- Run `sqlite-graphrag remember --name test-gap-003b --type note --body "..." --llm-backend codex,claude --json`
- Confirm the response succeeds even if codex fails (fallback to claude)
- Run `sqlite-graphrag remember --name test-gap-003c --type note --body "..." --llm-backend invalid-backend --json`
- Confirm exit code 1 (validation) and a clear error message
- Run `sqlite-graphrag remember --name test-gap-003d --type note --body "..." --llm-backend codex,claude,none --skip-embedding-on-failure --json`
- Confirm the memory is persisted with `embedding: null` in the database

### Phase 6 — GAP-004 (ADR-0039): fs4 Cross-Process Slot Semaphore
- Inspect the `Cargo.toml` and confirm `fs4 = "0.9"` (NOT `fs2`)
- Run `sqlite-graphrag slots status --json` and confirm the response includes `action: "slots_status"`, `max_concurrency`, `acquired`, `waiting`, `held_by_pid[]`, `p50_wait_ms`, `p99_wait_ms`
- Run two concurrent `remember` invocations: `sqlite-graphrag remember --name a --type note --body "..." --llm-max-host-concurrency 1 & sqlite-graphrag remember --name b --type note --body "..." --llm-max-host-concurrency 1 &`
- Run `sqlite-graphrag slots status --json` in a third terminal and confirm `acquired >= 1`
- Run `sqlite-graphrag slots release --slot-id 999 --yes --json` and confirm exit code 4 (no such slot)
- Validate the response against `docs/schemas/slots-status.schema.json` via `jsonschema --instance`

### Phase 7 — GAP-005 (ADR-0040): Stderr-Capture Fallback Chain
- Run `sqlite-graphrag remember --name test-gap-005 --type note --body "..." --llm-backend codex,claude --json`
- Inspect the `pending_embeddings` table: `sqlite3 test.sqlite "SELECT * FROM pending_embeddings"`
- Confirm the response is 200 OK (memory persisted)
- Temporarily set `ANTHROPIC_API_KEY=invalid` and `OPENAI_API_KEY=invalid` (must be removed after the test)
- Run `sqlite-graphrag remember --name test-gap-005b --type note --body "..." --llm-backend codex,claude,none --skip-embedding-on-failure --json`
- Confirm exit code 1 (oauth-only enforcement aborts the spawn) and the error envelope
- Reset the env vars before continuing
- Run `sqlite-graphrag embedding status --json` and confirm `action: "embedding_status"`, `counts`, `elapsed_ms`
- Run `sqlite-graphrag embedding list --json` and confirm the response is empty for the test namespace
- Run `sqlite-graphrag pending-embeddings list --json` and confirm the response includes `action: "pending_embeddings_list"`
- Validate both responses against their respective JSON schemas via `jsonschema --instance`

### Phase 8 — codex OAuth 401 Incident Mitigation
- Verify that `codex login` was run in the pre-flight step
- Inspect `~/.codex/config.toml` (or equivalent) and confirm the refresh token is fresh
- Run `sqlite-graphrag remember --name test-oauth-401 --type decision --body "OAuth 401 mitigation smoke test" --llm-backend codex,claude --json`
- Confirm the response is 200 OK (the fallback chain absorbs the 401 if it occurs)
- Run `sqlite-graphrag pending-embeddings list --filter-status failed --json` and confirm it is empty (no failed rows)

### Phase 9 — Library API Pinning
- Create a minimal Rust library that depends on `sqlite-graphrag = "=1.0.82"` in `Cargo.toml`
- Run `cargo check --tests` and confirm compilation succeeds
- Verify that `CommandKind` (or the equivalent) has 4 new variants: `Pending`, `Slots`, `Embedding`, `PendingEmbeddings`
- Confirm the exit code constants expose `SHUTDOWN_EXIT_CODE = 19` via the public API
- Document any non-additive changes in the `CHANGELOG.md` for the next release

### Phase 10 — Reporting
- Write the test report to `/tmp/test-v1-0-82-cli/results.md` using `atomwrite`
- Capture stdout and stderr from each phase
- Record any flakiness observed (e.g., `slot_enforces_max_concurrency`)
- Document any deviations from the expected output
- Mark the test plan as PASS or FAIL based on aggregate results
- If FAIL: file an incident in `gaps.md` with the reproduction steps


## Acceptance Criteria
- All 10 phases must complete without unrecoverable errors
- All JSON responses must validate against their respective JSON schemas
- The schema version must advance to 15 (or higher) after migration
- Exit code 19 must be observed during the GAP-002 phase
- The `fs4` crate (not `fs2`) must be present in the lockfile
- codex OAuth 401 incident mitigation must be verified end-to-end
- Library API must compile with the `=1.0.82` pin
- The binary must respond to all 4 new subcommands (`pending`, `pending-embeddings`, `slots`, `embedding`)


## Rollback
- If any phase fails, run `cargo install sqlite-graphrag --version 1.0.81 --force` to revert
- Restore the pre-upgrade database from `/var/backups/graphrag-pre-v1-0-82.sqlite`
- See `docs/MIGRATION.md` for the full rollback procedure
