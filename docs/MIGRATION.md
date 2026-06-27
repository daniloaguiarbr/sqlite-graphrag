# MIGRATING TO v1.0.96 — Enrich Dead-Letter + REST Concurrency (ADR-0055)

> This guide covers upgrading to v1.0.96. No migration runs on the main database; schema remains at v15. The separate `.enrich-queue.sqlite` queue database is migrated in-place automatically on the first `enrich` — no operator action. Reinstall with `cargo install sqlite-graphrag --locked --force`.

## v1.0.96 — Enrich Dead-Letter + REST Concurrency (ADR-0055)

### What Changed
- **GAP-ENRICH-BACKLOG-CONVERGE**: `enrich` now drives the backlog to convergence via a dead-letter queue. The `.enrich-queue.sqlite` database gains two columns through an IDEMPOTENT `ALTER TABLE` — `error_class` and `next_retry_at` — plus the index `idx_enrich_queue_eligible ON queue(status, next_retry_at)` and a new terminal status `dead`. Transient failures (rate-limit/timeout/5xx) reschedule `next_retry_at` with exponential backoff; hard failures (validation/parse) go terminal immediately. An item turns `dead` after `--max-attempts` transient retries (default 5) or on the first hard failure. Dequeue honours `next_retry_at` and excludes `dead`, so the live set is strictly decreasing.
- **GAP-OPENROUTER-REST-CONCURRENCY**: REST embedding for `--mode openrouter` fans out per batch with a bounded `tokio::task::JoinSet` (no new dependency), in-flight clamp 1..16 (Cloudflare-safe range). Chunk order is preserved by index; SQLite writes stay serialized via WAL + atomic claim (single-writer intact).

### Queue Migration — Automatic and In-Place
- The `.enrich-queue.sqlite` columns and index are added by an IDEMPOTENT `ALTER TABLE` / `CREATE INDEX IF NOT EXISTS` on the first `enrich` invocation. Pre-existing queue databases are migrated in-place automatically — NO operator action required.
- The main `graphrag.sqlite` is untouched: schema stays at v15; no `ALTER TABLE` runs against it.

### New enrich Flags
- `--until-empty` — internal scan→drain loop until the queue runs out of eligible items or `--max-runtime` expires; replaces the external bash retry loop.
- `--max-runtime <SECONDS>` — wall-clock ceiling for `--until-empty`; default 3600.
- `--max-attempts <N>` — transient-retry budget before `dead`; default 5; range 1..=20.
- `--status` — read-only JSON report of queue counts (`unbound_backlog`, `queue_pending/done/failed/dead/skipped`, `eligible_now`, `waiting`); does NOT call the LLM, does NOT acquire the singleton.
- `--rest-concurrency <N>` — REST concurrency for `--mode openrouter`; clamp 1..=16; default 8; distinct from `--llm-parallelism`.

### Nothing Breaks
- No main-database migration; schema stays at v15.
- Existing `enrich --mode claude-code|codex|opencode|openrouter` invocations are untouched — the new flags are additive and the dead-letter columns default to NULL for in-flight rows.

```bash
# Drive the backlog to convergence headlessly (no external loop)
sqlite-graphrag enrich --operation memory-bindings --mode openrouter \
  --openrouter-model MODEL --until-empty --max-runtime 1800 \
  --max-attempts 5 --rest-concurrency 8 --json

# Inspect the queue without spawning the LLM or taking the singleton
sqlite-graphrag enrich --status --json
```

# MIGRATING TO v1.0.95 — OpenRouter Chat Enrich (ADR-0054)

> This guide covers upgrading to v1.0.95. No database migration runs. Schema remains at v15. Reinstall with `cargo install sqlite-graphrag --locked --force`.

## v1.0.95 — OpenRouter Chat Enrich (ADR-0054)

### What Changed
- **GAP-OR-ENRICH**: new opt-in `enrich --mode openrouter` routes the JUDGE step to the OpenRouter `/chat/completions` REST endpoint, so structured extraction no longer requires a locally installed `claude`/`codex`/`opencode` CLI. The SCAN→JUDGE→PERSIST pipeline is unchanged; only the JUDGE transport differs.
- New module `src/chat_api.rs` (`OpenRouterChatClient`) mirrors `src/embedding_api.rs` (same retry/backoff and minimal `Authorization: Bearer` header).
- The four enrich modes are now `claude-code`, `codex`, `opencode`, `openrouter`.

### Nothing Breaks
- No database migration; schema stays at v15.
- Existing `enrich --mode claude-code|codex|opencode` invocations are untouched — `openrouter` is purely additive.

### Required Flag
- `--openrouter-model` is REQUIRED with `--mode openrouter`; omitting it exits 1 before any network call.

```bash
sqlite-graphrag enrich --operation memory-bindings --mode openrouter \
  --openrouter-model MODEL --json
```

# MIGRATING TO v1.0.94 — Four-Gap Remediation (ADR-0053)

> This guide covers upgrading from v1.0.93 to v1.0.94. No database migration runs. Schema remains at v15.

## v1.0.94 — Four-Gap Remediation (ADR-0053)

### What Changed
- **GAP-OR-ENTITY-EMBED**: Entity embedding in `remember`/`remember-batch`/`ingest` now honours `--embedding-backend`/`--llm-backend`, routing via OpenRouter REST. `remember` with new entities drops from ~119s to ~0.9s (`embedder.rs`, `remember.rs:771`).
- **GAP-EMBED-DIM-64**: `DEFAULT_EMBEDDING_DIM` raised from 64 to **384** (`constants.rs:29`). New databases created via `init` write `dim=384` in `schema_meta`. Legacy databases at dim 64 are preserved via `schema_meta.dim` precedence — no forced re-embed.
- **GAP-EMBED-TIMEOUT-300**: `DEFAULT_EMBED_TIMEOUT_SECS` raised from 120 to **300** (`llm_embedding.rs:43`).
- **GAP-HEADLESS-DEFAULT**: `enrich --mode` is now **REQUIRED** (removed `default_value = "claude-code"` in `enrich.rs:379`). Omitting `--mode` is rejected by clap (exit 2), preventing accidental spawn of `claude -p` with the project `.mcp.json`.

### Breaking Change — `enrich --mode` is now required

**Before (v1.0.93 — fails in v1.0.94 with exit 2):**
```bash
sqlite-graphrag enrich --operation memory-bindings --mode codex --json
```

**After (v1.0.94):**
```bash
# Choose the mode matching your --llm-backend
sqlite-graphrag enrich --operation memory-bindings --mode codex --json
# or
sqlite-graphrag enrich --operation memory-bindings --mode claude-code --json
# or
sqlite-graphrag enrich --operation memory-bindings --mode opencode --json
```

**Canonical pairing:**
| `--llm-backend` | `--mode` |
|-----------------|----------|
| `codex`         | `codex`  |
| `claude`        | `claude-code` |
| `opencode`      | `opencode` |

### Who Is Affected
- All users running `enrich --operation ...` (any operation) without `--mode`. Update all invocations before upgrading.
- Automation scripts, CI pipelines, or scheduled jobs that call `enrich`.

### How to Upgrade
1. Audit all `enrich` calls: `rg "enrich --operation" your-scripts/ | rg -v -- "--mode"`
2. Add `--mode <value>` matching your `--llm-backend` to each invocation (see pairing table above).
3. No database migration — schema stays at v15.
4. Legacy dim-64 databases work unchanged; new databases default to dim 384.

### Migration Notes
- **No schema migration**: schema remains v15; no `ALTER TABLE` runs.
- **Default dim changed 64 → 384**: affects only new databases created with v1.0.94+. Existing databases retain their recorded dim via `schema_meta.dim` precedence.
- **Embed timeout changed 120s → 300s**: no user action needed; longer operations now have more headroom.

### Rollback
Roll back to v1.0.93 by reinstalling the previous binary. No database changes to undo — schema and dim are preserved in `schema_meta`.

```bash
# Inspect recorded dim in a database
sqlite-graphrag stats --json | jaq '.schema_meta'
```

# MIGRATING TO v1.0.93 — OpenRouter Embedding Backend (GAP-OR-INGEST)

> This guide covers upgrading from v1.0.92 to v1.0.93. No database migration runs. Schema remains at v15. Behavior is ADDITIVE.

## v1.0.93 — OpenRouter Embedding Backend (GAP-OR-INGEST)
### What Changed
- New embedding backend: OpenRouter REST API via `--embedding-backend openrouter`
- `EmbeddingBackendChoice` propagated to all 13 embedding paths
- **GAP-OR-PROPAGATION**: 5 additional embedding paths fixed in v1.0.93 — `enrich --operation re-embed`, `init` (dimension probe), `rename-entity`, `ingest --mode claude-code` (4 call sites), and `remember` (chunk parallel embedding)
- **BUG-OR-EXIT-CODE**: OpenRouter config errors now return exit code 78 (`EX_CONFIG`) instead of exit 1
- Exit code 78 covers: missing `OPENROUTER_API_KEY`, missing `--embedding-model`, invalid API key
- New `--enrich-after` flag for ingest
- New modules: `embedding_api.rs`, `config.rs`, `config_cmd.rs`
### Who Is Affected
- Users wanting faster embedding (~200ms vs 15s) via dedicated embedding models
- Users running bulk ingest operations
### How to Upgrade
- No migration needed — zero schema change, zero ALTER TABLE
- Existing databases work unchanged with `--embedding-backend llm` (default behavior)
- To use OpenRouter: set `OPENROUTER_API_KEY` and add `--embedding-backend openrouter --embedding-model MODEL`
### What Breaks
- Nothing — fully backward compatible
- `--embedding-backend auto` (default) falls back to LLM subprocess if OpenRouter not configured

# MIGRATING TO v1.0.91 — Spawn CWD Isolation, Degree Fix, Schema Corrections

> This guide covers upgrading from v1.0.90 to v1.0.91. No database migration runs. Schema stays at v15. Behaviour is ADDITIVE.

## v1.0.91 — Spawn CWD Isolation (GAP-SPAWN-001)

- ALL 10 LLM subprocess spawn sites now call `apply_cwd_isolation()` which sets `current_dir(temp_dir)` and `CLAUDE_CONFIG_DIR=temp_dir`
- This eliminates `.mcp.json` walk-up interference that caused timeout or 401 errors in projects with MCP servers
- The workaround `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1 CLAUDE_CONFIG_DIR=/tmp/graphrag-empty-config` is NO LONGER NEEDED for normal operation
- Spawn directories `/tmp/sqlite-graphrag-spawn-{PID}/` are cleaned up automatically at process exit (GAP-SPAWN-002)
- BUG-17 fixed: `entities.degree` no longer inflates on `remember` and `ingest` — `increment_degree()` replaced by `recalculate_degree()` after relationship insertion
- BUG-15 fixed: 7 JSON schemas now include `"opencode"` and `"auto"` in `backend_invoked` enum
- BUG-16 fixed: `deep-research.schema.json` includes `vec_degraded` in `ResearchStats`
- No schema change. No migration runs

```bash
# Smoke test after upgrade
sqlite-graphrag health --json | jaq '.integrity_ok'
sqlite-graphrag --llm-backend auto remember --name upgrade-test --type note --body "v1.0.91 test" --json
```

### Breaking changes

- None. All changes are additive
- If you relied on `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` or `CLAUDE_CONFIG_DIR` workaround, you can remove them — CWD isolation is now built-in

### If degree values look wrong after upgrade

- `graph stats` may have shown inflated `max_degree` values from BUG-17
- After upgrade, new `remember` and `ingest` operations will write correct degree values
- To fix existing inflated degrees: `sqlite-graphrag normalize-entities --yes --json` triggers recalculation


# MIGRATING TO v1.0.90 — OpenCode Backend Integration (ADR-0051)

> This guide covers upgrading from v1.0.89 to v1.0.90. No database migration runs. Schema stays at v15. Behaviour is ADDITIVE.

## v1.0.90 — OpenCode as Third LLM Backend

- OpenCode added as third backend: `codex > claude > opencode > none`
- New env vars: `SQLITE_GRAPHRAG_OPENCODE_BINARY`, `SQLITE_GRAPHRAG_OPENCODE_MODEL`, `SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL`
- New CLI flags: `--opencode-binary`, `--opencode-model`, `--opencode-timeout`
- 24 bugs/gaps closed (see `gaps.md` and `CHANGELOG.md` for full list)
- No schema change. No migration runs

```bash
# Smoke test after upgrade
sqlite-graphrag health --json | jaq '.integrity_ok'
sqlite-graphrag --llm-backend auto remember --name upgrade-test --type note --body "v1.0.90 test" --json
```

### Breaking changes

- None. All changes are additive. Existing `--llm-backend codex` and `--llm-backend claude` continue to work unchanged

### If you have opencode installed

- The auto-detect (`--llm-backend auto`) now probes opencode in PATH after codex and claude
- To exclude opencode from the fallback chain: `--llm-fallback codex,claude,none`


# MIGRATING TO v1.0.89 — Preflight Layer, BUG-11/12/13 Hotfixes, Schema Drift (ADR-0045, ADR-0046, ADR-0047, ADR-0048, ADR-0049)

> This guide is for operators on v1.0.82 who want to upgrade to v1.0.83 without losing data. This release is a PATCH bump with NO database migration. Schema stays at v15. Behaviour is ADDITIVE for default OAuth operators.

# MIGRATING TO v1.0.86 → v1.0.87 → v1.0.88 → v1.0.89 — Preflight + Hotfixes + Schema Drift

> This section guides operators on v1.0.85.2 who want to upgrade to v1.0.89 across four releases. No database migration runs in this cycle. The schema stays at v15.

## v1.0.86 — LLM-Heavy Surface

- Adds 10 subcommands: `pending list`, `pending show`, `pending cleanup`, `embedding status`, `embedding list`, `embedding abandon`, `pending-embeddings list`, `pending-embeddings process`, `slots status`, `slots release`
- Adds global flags: `--max-concurrency`, `--wait-lock`, `--llm-parallelism`, `--ingest-parallelism`, `--graceful-shutdown-secs`, `--skip-embedding-on-failure`
- No schema change. No migration runs

```bash
# Smoke test the new subcommands
sqlite-graphrag pending list --json
sqlite-graphrag slots status --json
sqlite-graphrag embedding status --json
```

## v1.0.87 — Pre-Flight Validation Layer (ADR-0045)

- Introduces `src/spawn/preflight.rs` (≥200 lines, 7 guards, 15 unit tests) gating every LLM subprocess spawn BEFORE the fork
- New `AppError::PreFlightFailed` variant. Exit code 16 (`EX_CONFIG`) is now permanent for pre-flight failures
- Bypass: `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` disables all 7 guards (emergency only)
- The 4 spawners (`claude_runner`, `codex_spawn`, `ingest_claude`, `extract/llm_embedding`) share this single module
- No schema change. No migration runs

```bash
# Diagnose a pre-flight failure (exit 16) — the JSON envelope carries the PreFlightError variant
sqlite-graphrag remember --name test --type note --description x --body y 2>&1
# Expected: exit 16, JSON envelope with code "PreFlightFailed" and variant details

# Bypass in emergencies
SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1 sqlite-graphrag remember --name test --body y
```

## v1.0.88 — Hotfixes BUG-11/12/13 (ADR-0046, ADR-0047)

- BUG-11 (CRITICAL): pre-flight failure in `extract/llm_embedding.rs:563-565` now propagates to `remember` via `embed_via_backend_strict`
- BUG-12 (MEDIUM): OAuth-only enforcement emits 1 stderr line (was 2)
- BUG-13 (MEDIUM): `link --create-missing` now respects entity-name validation
- No schema change. No migration runs

```bash
# BUG-11 repro
CLAUDE_CONFIG_DIR=/tmp/bad-config-with-mcp sqlite-graphrag remember --name test --body y
# Pre-v1.0.88: silent persistence with backend_invoked: "none"
# v1.0.88+: exit 11 with JSON error envelope

# BUG-12 verification
ANTHROPIC_API_KEY=sk-test sqlite-graphrag init
# Pre-v1.0.88: 2 stderr lines
# v1.0.88+: 1 stderr line
```

## v1.0.89 — Schema Drift + Flag Parity + Embedding Deadlock Remediation (ADR-0048, ADR-0049, ADR-0050)

- `health.schema.json` regenerated via `schemars` derive macro. `additionalProperties: true` (Must-Ignore policy per RFC 7493 I-JSON). 17 new fields added
- New subcommand acceptors for `--db <PATH>`: `embedding status`, `embedding list`, `embedding abandon`, `pending list`, `pending show`
- `migrate --dry-run --json` reports pending migrations without applying
- `codex-models --json` accepted as no-op; `pending list --db <PATH>` parity
- `ingest --auto-describe` (default true) extracts description from first meaningful body line
- `health --namespace <NS> --json` filters counts to a single namespace
- Binary size 14.6 MiB documented in `Cargo.toml:6`
- `BoolishValueParser`: boolean env vars now accept `1`/`yes`/`on` (and `0`/`no`/`off`), not just `true`/`false`
- New flags: `--codex-binary`, `--llm-model`, `--llm-fallback`, `--llm-max-host-concurrency`, `--llm-slot-wait-secs`, `--llm-slot-no-wait`
- Dead flag fix: 7 CLI flags previously parsed but never forwarded are now properly propagated to the LLM spawn path
- Default models: `gpt-5.5` for codex, `claude-sonnet-4-6` for claude. Override with `--llm-model` or the backend-specific `--codex-model` / `--claude-model`
- No schema change. No migration runs

### Embedding Deadlock Remediation (ADR-0050)

- GAP-RECALL-001 (CRITICAL): `recall`, `hybrid-search`, `deep-research` hung indefinitely at "Computing query embedding..." due to stalled LLM subprocesses saturating the host-wide slot semaphore. Fixed via explicit `drop(stdin)` before `wait_with_output`, timeout reduction 300s to 60s, stale slot cleanup on startup, and reaper expansion to kill orphan `sqlite-graphrag` processes
- GAP-DEEPRESEARCH-001: `deep-research` now degrades gracefully to FTS5-only when embedding fails (was hard-fail exit 11)
- BUG-SKIP-EMBED + BUG-SKIP-EMBED-INCOMPLETE: `--skip-embedding-on-failure` wired end-to-end in `remember`, `edit`, `restore`, `rename-entity`, `remember-batch`. Memory persists with NULL embedding for later `enrich --operation re-embed`
- BUG-MODEL-VAZIO: `codex_embed_model()` and `claude_embed_model()` return sensible defaults (`gpt-5.5`, `claude-sonnet-4-6`) instead of empty string
- BUG-YES-FLAG-IGNORED: `slots release`, `purge`, `cleanup-orphans` now enforce `--yes` before destructive operations
- BUG-BOOLISH-ENV: 4 boolean flags with `env = "SQLITE_GRAPHRAG_*"` now accept `1`/`yes`/`on` via `BoolishValueParser`
- BUG-BATCH-FTS-DESYNC: `remember-batch --force-merge` now calls `sync_fts_after_update`
- BUG-ENRICH-DESC-FTS-DESYNC + BUG-ENRICH-BODY-EXTRACT-FTS-DESYNC: `enrich` operations now sync FTS5 after description/body updates
- BUG-FORGET-DOUBLE-DELETE-VEC: removed redundant second `delete_vec` call in `forget`
- GAP-FLAGS-MORTAS: 7 global CLI flags now propagated via `set_var` in `main.rs`
- GAP-BACKEND-PROPAGATION: `deep-research` and `remember-batch` now honor `--llm-backend`
- GAP-ADAPTIVE-TIMEOUT: `embed_timeout_for_batch(batch_size)` scales: 60s base + 15s per additional item

```bash
# Force claude backend with explicit model (ADR-0050)
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  recall "test query" --k 5 --json

# Skip embedding on failure (persist memory without vector)
sqlite-graphrag --skip-embedding-on-failure \
  remember --name resilient --type note --body "text" --json

# Codex with explicit model
sqlite-graphrag --llm-backend codex --llm-model gpt-5.5 \
  hybrid-search "test" --k 10 --json
```

```bash
# Health namespace filter (GAP-E2E-002)
sqlite-graphrag health --namespace prod --json

# Dry-run migration report (GAP-E2E-009)
sqlite-graphrag migrate --dry-run --json

# Auto-describe from first body line (GAP-E2E-011)
sqlite-graphrag ingest ./docs --auto-describe --json

# --db parity on pending/embedding (GAP-E2E-008)
sqlite-graphrag pending list --db /tmp/test.sqlite --json
sqlite-graphrag embedding status --db /tmp/test.sqlite --json
```

## Library API Pinning Across v1.0.86-89

The lib API remains unstable within v1.x.y (ADR-0032). Pin exactly:

```toml
[dependencies]
sqlite-graphrag = "=1.0.89"
```

The `^1.0` shorthand keeps you on the CLI-stability track. CLI consumers who follow the JSON envelope contract in `docs/schemas/` are unaffected.

## What Breaks Across v1.0.86-89

- **NONE for default OAuth operators** — behaviour is additive at every step
- **Library consumers who enumerate `AppError` variants**: `PreFlightFailed` (exit 16) added in v1.0.87
- **Consumers validating `health` JSON against strict schemas**: `additionalProperties: true` (Must-Ignore) means strict validators using `additionalProperties: false` will now accept unknown keys. Update your validator or migrate to the schemars-derived schema

## Rollback Across v1.0.86-89

If any release breaks your pipeline, rollback to v1.0.85.2:

```bash
cargo install sqlite-graphrag --version 1.0.85.2 --force
```

Your database is unchanged. v1.0.86-89 made no schema modifications; v1.0.85.2 reads the same SQLite file.

## Verification Scenarios

### Scenario A — Preflight rejection

```bash
unset ANTHROPIC_API_KEY OPENAI_API_KEY
sqlite-graphrag remember --name test-preflight --body "x" 2>&1
# Expected: exit 0 in clean env; exit 16 with PreFlightError variant in dirty env
```

### Scenario B — Schema drift validation

```bash
sqlite-graphrag health --json | jaq '.schema_version, .fts_query_ok, .vec_memories_missing'
# Expected: integer schema_version, boolean fts_query_ok, integer vec_memories_missing (Must-Ignore accepts extra fields)
```

### Scenario C — Flag parity

```bash
sqlite-graphrag pending list --db /tmp/x.sqlite --json
sqlite-graphrag embedding status --db /tmp/x.sqlite --json
sqlite-graphrag codex-models --json
# Expected: all three exit 0
```

### Scenario D — Description heuristic

```bash
echo "# Header only" > /tmp/headers.md
sqlite-graphrag ingest /tmp/headers.md --auto-describe --json
# Expected: description derives from file stem "headers" (since >20-char body line absent)

echo "This is a meaningful first sentence for auto-describe" > /tmp/full.md
sqlite-graphrag ingest /tmp/full.md --auto-describe --json
# Expected: description is "This is a meaningful first sentence for auto-describe" truncated to ≤100 chars
```
## What Changed in v1.0.83

- **GAP-058 partial resolution (ADR-0041)** — six custom-provider env vars are now preserved when spawning `claude -p` or `codex exec` subprocesses. Enables Anthropic-compatible providers (Minimax/api.minimax.io, OpenRouter, AWS Bedrock, corporate gateways) without altering the OAuth-only mandate that continues to reject `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`. The preserved vars are `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, and `OTEL_EXPORTER_OTLP_ENDPOINT`.
- **Shared whitelist helper** — the previously duplicated `env_clear` + re-injection logic in `claude_runner.rs`, `codex_spawn.rs`, and `ingest_claude.rs` is consolidated in `src/spawn/env_whitelist.rs`. The three spawners delegate to `apply_env_whitelist(cmd, strict)` instead of inlining the array.
- **Compliance opt-out flag** — `--strict-env-clear` / `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` enables strict mode that preserves only `PATH`. Use in PCI-DSS, SOC2, HIPAA environments where credential forwarding via env vars is forbidden by policy. Without this flag, the default is to forward the six custom-provider vars alongside the OAuth-only rejection guard.
- **OAuth-only guard stays intact** — the four guards in `claude_runner.rs:273`, `codex_spawn.rs:259`, `ingest_claude.rs:282`, and `extract/llm_embedding.rs:237-253` still abort the spawn with `AppError::Validation` (exit 1) when `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` are set. The error message now points to `ANTHROPIC_AUTH_TOKEN` and `~/.codex/auth.json` as legitimate resolutions.
- **NO telemetry** — the fix is silent. No new `tracing::info!` macro logs which provider the operator is using. The no-leak audit test in `tests/claude_runner_env.rs` enforces that the literal token value NEVER appears in stdout or stderr even with `RUST_LOG=trace`.
- **6 new regression tests** — `tests/claude_runner_env.rs` covers custom-provider propagation, OAuth-only abort preservation, codex base-URL inheritance, strict-mode credential dropping, and the no-leak audit. All carry `#[serial_test::serial(env)]`.

## Who Is Affected

- All v1.0.82 users running custom Anthropic-compatible providers (Minimax, OpenRouter, AWS Bedrock, corporate gateways) — previously had `exit 11` embedding failures with `401 Invalid authentication credentials` in stderr (G58 S5 scenario)
- Default OAuth operators (Claude Pro/Max, ChatGPT Pro) are UNAFFECTED — the guard rejects `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` identically to v1.0.82
- Shared-host operators with strict credential policy should set `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` BEFORE running the new binary to avoid inadvertently forwarding secrets
- Library consumers see ONE additive public symbol: `crate::spawn::env_whitelist::{apply_env_whitelist, is_strict_env_clear, PRESERVED_ENV_VARS}` — re-pin to `=1.0.83`

## Semantic Distinction the Fix Resolves

- `ANTHROPIC_API_KEY` — paid Anthropic API key (`sk-ant-...`), PROHIBITED by ADR-0011 OAuth-only mandate
- `ANTHROPIC_AUTH_TOKEN` — OAuth token used by Claude Code with custom provider, semantically distinct and now PRESERVED
- `OPENAI_API_KEY` — paid OpenAI API key, PROHIBITED
- `OPENAI_BASE_URL` — endpoint override for custom OpenAI-compatible providers, now PRESERVED
- `ANTHROPIC_BASE_URL` — endpoint override for custom Anthropic-compatible providers, now PRESERVED

The v1.0.69 mandate was correct in rejecting the paid-API vars; the env-clear whitelist was overly broad and accidentally stripped the legitimate custom-provider vars too. v1.0.83 corrects the implementation while preserving the OAuth-only invariant.

## MIGRATING TO v1.0.84 — Claude Backend Split (ADR-0042, GAP-002)

If you relied on `--llm-backend claude` in v1.0.83 to force the Claude entry point, that flag now actually works as documented. Previously it was a synonym for codex (GAP-002). The split goes through `LlmEmbeddingBuilder` (new in v1.0.84) and the new `embed_via_claude_local` function in `src/embedder.rs:190+`. Use `--dry-run-backend` to verify which backend will be invoked before any embedding call.

## MIGRATING TO v1.0.85 — Five-Gap Remediation (ADR-0043)

The `FallbackReason` enum now distinguishes 7 causes via `reason_code`: `embedding_failed | slot_exhausted | oauth_quota | backend_mismatch | dim_zero | cancelled | timeout`. Scripts parsing the `vec_degraded: bool` field of `recall` and `hybrid-search` envelopes should be updated to read `vec_degraded_reason: Option<String>` for fine-grained diagnostics. The `try_embed_query_with_deterministic_fallback` path retries on `OAuthQuota` and applies a 750ms ceiling on `SlotExhausted` before falling back to FTS5-only mode.

The 12-14 `anthropic-ratelimit-*-remaining` HTTP headers returned by `claude -p` are now captured by `LlmEmbedding::invoke_claude` (G45-CR5). A value of `0` aborts the embed and triggers a codex fallback instead of waiting for circuit breaker activation.

Default embedding dimensionality is locked at 64 (Matryoshka Representation Learning, arXiv 2205.13147). Existing 384-dim databases continue to work unchanged; new databases created under v1.0.85 consume 6x fewer OAuth tokens per call (G56).

## HOTFIX v1.0.85.1 — `recall`/`hybrid-search` `--llm-backend none` Graceful Fallback (GAP-004)

If you pass `--llm-backend none` to `recall` or `hybrid-search`, the response now correctly emits `vec_degraded: true` + `source: "fts_fallback"` + `vec_degraded_reason: "dim_zero"` and exits 0. Before the hotfix, the failsafe from v1.0.80 was broken for this specific backend choice. The fix lives in `src/embedder.rs:351` as an intermediate arm `Ok((v, _backend)) if v.is_empty() => Err(FallbackReason::DimZero)`.

## HOTFIX v1.0.85.2 — `--dry-run-backend` Standalone + `embed_via_backend` Resolved Kind (ADR-0044)

`--dry-run-backend` now works as a standalone flag without requiring a subcommand. The fix is `pub command: Option<Commands>` in `src/cli.rs:248`. Calling `sqlite-graphrag --llm-backend claude --dry-run-backend` exits 0 with JSON `{action, backend, binary, model, flavour, chain, strict_env_clear}`.

`embed_via_backend` now returns `Result<(Vec<f32>, LlmBackendKind), AppError>` instead of just `Result<Vec<f32>, AppError>`. The `resolved_kind` propagates to 7 envelopes (edit, embedding-status, enrich-summary, hybrid-search, ingest-summary, recall, remember) which all now report `backend_invoked: "claude" | "codex" | "none"` consistently.

## How to Upgrade

```bash
# 1. Backup before upgrade (recommended, mirrors v1.0.82 pattern)
sqlite-graphrag backup --output /var/backups/graphrag-pre-v1-0-83.sqlite --json

# 2. Install the new version
cargo install sqlite-graphrag --version 1.0.83 --force
sqlite-graphrag --version   # should report 1.0.83

# 3. NO migration needed — schema stays at v15
sqlite-graphrag health --json | jaq '.schema_version'   # confirm 15

# 4. For Minimax operators (the canonical scenario for this fix)
export ANTHROPIC_AUTH_TOKEN="sk-cp-your-minimax-token"
export ANTHROPIC_BASE_URL="https://api.minimax.io/anthropic"

# 5. Smoke test — validates custom-provider env propagates to subprocess
sqlite-graphrag remember \
  --name v183-smoke \
  --type note \
  --description "v1.0.83 custom provider smoke test" \
  --body "if you can read this, the custom provider is wired correctly"

# 6. Verify embedding was written
sqlite-graphrag read --name v183-smoke --json | jaq '.body, .memory_id'
sqlite-graphrag health --json | jaq '.counts.memories'

# 7. For shared hosts with strict policy (compliance)
export SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1
# OR pass --strict-env-clear per-invocation
sqlite-graphrag remember --name v183-strict --body "x" --strict-env-clear
```

## What Happens Automatically

- All v1.0.82 commands behave identically for default OAuth operators — no flag changes required
- The six custom-provider vars are now forwarded ONLY when set in the operator's environment (no manual enabling needed)
- The strict-mode opt-out is the only operator-actionable change; default remains permissive
- The OAuth-only guard's error message now references `ANTHROPIC_AUTH_TOKEN` and `~/.codex/auth.json` as legitimate resolutions when an operator mistakenly sets `ANTHROPIC_API_KEY`
- Test count increases from 812 to 818 (6 new serial env tests)

## Library API Pinning

If you depend on the lib API, pin to the EXACT version in `Cargo.toml`:

```toml
[dependencies]
sqlite-graphrag = "=1.0.83"
```

The `^1.0` shorthand keeps you on the CLI-stability track. The `^1.0.83` shorthand allows 1.0.83..<1.1.0, which can include a future 1.0.84 with lib-breaking changes.

## What Breaks

- **NONE for default OAuth operators** — behaviour identical to v1.0.82
- **Library consumers who enumerate `PRESERVED_ENV_VARS` length** — the slice gained 4 entries (`ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`); non-exhaustive patterns are unaffected
- **Operators who relied on `ANTHROPIC_AUTH_TOKEN` being stripped** — unlikely scenario but possible: the var now reaches the subprocess, which may change LLM-side behaviour. Use `--strict-env-clear` to restore v1.0.82 semantics

## Verification Scenarios

### Scenario A — Default OAuth operator (no custom provider)

```bash
unset ANTHROPIC_AUTH_TOKEN ANTHROPIC_BASE_URL
sqlite-graphrag remember --name test-oauth-default --body "x"
# Expected: exit 0, OAuth subscription used, identical to v1.0.82
```

### Scenario B — Minimax custom provider

```bash
export ANTHROPIC_AUTH_TOKEN="sk-cp-minimax-test"
export ANTHROPIC_BASE_URL="https://api.minimax.io/anthropic"
sqlite-graphrag remember --name test-minimax --body "x"
# Expected: exit 0, custom provider routed, no 401 in stderr
```

### Scenario C — OAuth-only abort preserved

```bash
unset ANTHROPIC_AUTH_TOKEN ANTHROPIC_BASE_URL
export ANTHROPIC_API_KEY="sk-ant-violation"
sqlite-graphrag remember --name test-oauth-abort --body "x"
# Expected: exit 1, stderr mentions OAuth-only mandate and ANTHROPIC_AUTH_TOKEN as resolution
```

### Scenario D — Strict compliance mode

```bash
export ANTHROPIC_AUTH_TOKEN="sk-cp-strict-test"
export SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1
sqlite-graphrag remember --name test-strict --body "x"
# Expected: subprocess receives ONLY PATH; ANTHROPIC_AUTH_TOKEN is NOT forwarded
# Confirms compliance posture: secrets stay in parent process
```

### Scenario E — No token leak audit

```bash
export ANTHROPIC_AUTH_TOKEN="sk-cp-secret-value-XYZ-12345"
export RUST_LOG=trace
sqlite-graphrag remember --name test-no-leak --body "x" 2> /tmp/stderr.log
# Expected: literal token NEVER appears in /tmp/stderr.log
# Validated by audit_no_token_leak_in_subprocess_stderr in tests/claude_runner_env.rs
```

## Rollback

If v1.0.83 is not working for you:

```bash
cargo install sqlite-graphrag --version 1.0.82 --force
```

Your database is unchanged. v1.0.83 made no schema modifications; v1.0.82 reads the same SQLite file.

To restore v1.0.82 behaviour for shared hosts without rolling back, set `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` — only PATH will be forwarded.
# MIGRATING TO v1.0.80 — Stability Policy, Windows Infra, SHUTDOWN Resilience

> This guide is for operators on v1.0.79 who want to upgrade to v1.0.80 without losing data. This release is a PATCH bump with NO database migration.

## What Changed in v1.0.80

- **Stability policy declared** (ADR-0032, G53): the public contract is the CLI; the library API is unstable in v1.x.y. Library consumers must pin to `=1.0.80` and review CHANGELOG.md before bumping
- **CI semver-checks job** added in informational mode (becomes blocking in v1.0.81 once the 9 outstanding MAJOR violations are resolved)
- **G45 cross-process embedding singleton** (ADR-0032 follow-up): `acquire_embedding_singleton` serialises LLM embedding calls per `(namespace, db)` pair; `--wait-embed-singleton SECONDS` polls the lock; `AppError::EmbeddingSingletonLocked` is the new structural variant (exit 75, retryable)
- **G55 S2 structural MemoryNotFound**: replaces the legacy `NotFound(String)` path that masked which lookup target failed; pt-BR messages now carry the name and namespace explicitly
- **G56 entity-embed in-process cache**: `embed_entity_texts_cached` keyed by `blake3(model || \0 || text)`; high hit rate in `ingest`, modest in `remember`/`remember-batch`
- **G58 recall and hybrid-search FTS5 fallback**: `recall --fallback-fts-only` and `hybrid-search --fallback-fts-only` route the query through FTS5 BM25 when the LLM subprocess fails; new envelope fields `vec_degraded`, `vec_error`, `warning` are populated symmetrically
- **G53-WINDOWS-INFRA** (ADR-0033): the windows-2025 matrix jobs gained pre-warm and verify steps gated on `if: matrix.os == windows-2025`. The 2 historical infra failure modes (rustup download with transient network errors and `E0463 can't find crate for core` when the target stdlib is missing) are now recoverable on the first re-run
- **SHUTDOWN resilience** (ADR-0034): `src/signals.rs` is wrapped in a panic-catching boundary; the third consecutive Ctrl-C exits with code 130 and ZERO I/O, matching the canonical 3-layer SHUTDOWN bypass recipe (`nohup` then `setsid` then `disown`)

## Who Is Affected

- All v1.0.79 users; the changes are all additive at the binary and database level
- Library consumers (cargo crate users, not CLI users) are STRONGLY advised to pin to `=1.0.80` because the lib API is unstable within v1.x.y
- Multi-session operators (concurrent agents writing to the same database) benefit from the G45 singleton without any action

## How to Upgrade

```bash
cargo install sqlite-graphrag --version 1.0.80 --force
sqlite-graphrag --version   # should report 1.0.80
```

NO database migration is required. The schema is still v13, the G43 dim-adoption already runs in `open_rw` and `open_ro`, and the new library-API additions are all ADDITIVE (no removed re-exports, no renamed fields, no changed signatures in 1.0.80).

## What Happens Automatically

- All v1.0.79 commands behave identically; the new flags (`--wait-embed-singleton`, `--fallback-fts-only`, `--force-reembed` from v1.0.79) are opt-in
- The Windows pre-warm steps are no-op on ubuntu and macos; they only run on `matrix.os == windows-2025`
- The `semver-checks` CI job is informational in v1.0.80; it reports drift without failing the pipeline

## Library API Pinning

If you depend on the lib API, pin to the EXACT version in `Cargo.toml`:

```toml
[dependencies]
sqlite-graphrag = "=1.0.80"
```

The `^1.0` shorthand keeps you on the CLI-stability track. The `^1.0.80` shorthand allows 1.0.80..<1.1.0, which can include a future 1.0.81 with lib-breaking changes. For lib users, the exact pin is mandatory.

## What Breaks

- **Library consumers who depend on symbols NOT in the 1.0.80 lib surface**: none added beyond the 6 documented in CHANGELOG. All 6 are additive
- **CI workflows that reference `windows-latest`**: this release does not change the runner label; the explicit `windows-2025` reference (added in v1.0.73) remains the right call until the VS2026 redirect cutover (2026-06-15)

## Rollback

If v1.0.80 is not working for you:

```bash
cargo install sqlite-graphrag --version 1.0.79 --force
```

Your database is unchanged. v1.0.80 made no schema modifications; v1.0.79 reads the same SQLite file.


# MIGRATING TO v1.0.82 — Five Gaps Closed, Two Migrations, Four Subcommands, OAuth 401 Mitigation

> This guide is for operators on v1.0.80 or v1.0.81 who want to upgrade to v1.0.82 without losing data. This release is a PATCH bump but DOES carry two additive database migrations (V014 and V015) that run automatically on first `init` or `migrate`. The schema version advances from 13 to 15.

## What Changed in v1.0.82

- **GAP-001 closed (ADR-0036)** — three-stage `remember` checkpoint queue. The `pending_memories` table (V014) buffers the body, entities and relationships separately; if a SIGTERM/SIGINT arrives during stage 2 or 3, the row stays in `queued` state for later reprocessing via `sqlite-graphrag pending list|show|cleanup`. See `docs/decisions/adr-0036-pending-memories-staging.md`.
- **GAP-002 closed (ADR-0037)** — Shutdown JSON envelope at exit code 19. Any LLM-spawning command that receives SIGTERM, SIGINT or SIGHUP now emits a deterministic JSON envelope to stdout and exits with `SHUTDOWN_EXIT_CODE = 19`. The envelope fields `error`, `code`, `signal`, `graceful` and `message` are validated by `docs/schemas/shutdown-envelope.schema.json`.
- **GAP-003 closed (ADR-0038)** — `--llm-backend` user-choice flag. Operators can now pass `--llm-backend codex,claude,none` (or any subset) to control the backend chain tried in order. The first backend that does not error wins; `none` as the last entry writes the memory with embedding NULL when paired with `--skip-embedding-on-failure`.
- **GAP-004 closed (ADR-0039)** — Host-wide LLM slot semaphore via `fs4 = "0.9"` with `sync` feature. Cross-process coordination uses `fcntl(F_SETLK)` on Linux/macOS and `LockFileEx` on Windows. Default is `min(ncpus, oauth_tier_max)` (Pro=4, Max=8). Inspect with `sqlite-graphrag slots status --json`; reap orphans with `sqlite-graphrag slots release --slot-id <N> --yes`. Pair with `--llm-max-host-concurrency N` to override the default ceiling.
- **GAP-005 closed (ADR-0040)** — Stderr-capture fallback chain for embedding failures. The pending-embeddings table (V015) holds rows that failed every backend in the chain. The chain detects `refresh_token_reused` (the 2026-06-14 codex incident) and routes to the next backend; if all backends fail the row is enqueued for retry via `sqlite-graphrag pending-embeddings list|process`. The `LlmBackendError` struct gained 4 variants (`Codex401`, `CodexRateLimit`, `ClaudeTimeout`, `Generic`) and `EXIT_CODE_HINTS` documents 9 codes.

## Who Is Affected

- All v1.0.80 and v1.0.81 users
- Operators running `codex exec` heavily who experienced HTTP 401 `refresh_token_reused` in 2026-06-14 — they MUST run `codex login` after upgrading to refresh the OAuth refresh token; the fallback chain in GAP-005 mitigates but does not eliminate the failure mode
- Library consumers must re-pin to `=1.0.82`; the 4 new subcommand surfaces are additive but the new exit code 19 and the new `--llm-backend` global flag are visible to lib consumers that enumerate `CommandKind`
- CI workflows: the `codex-models` whitelist now includes `gpt-5.5` as the default; CI tests that pinned `gpt-4*`, `o4-mini` or `gpt-5-codex` need to switch to the whitelisted set

## How to Upgrade

```bash
# 1. Backup antes de upgrade (recomendado)
sqlite-graphrag backup --output /var/backups/graphrag-pre-v1-0-82.sqlite --json

# 2. Instalar a nova versão
cargo install sqlite-graphrag --version 1.0.82 --force
sqlite-graphrag --version   # should report 1.0.82

# 3. Aplicar migrations V014 e V015 (automático, mas pode ser explícito)
sqlite-graphrag migrate --json

# 4. codex login OBRIGATÓRIO pós-upgrade (mitigação do incidente 2026-06-14)
codex login

# 5. Smoke test — valida que subcomandos novos funcionam
sqlite-graphrag pending list --json
sqlite-graphrag slots status --json
sqlite-graphrag embedding status --json
sqlite-graphrag pending-embeddings list --json

# 6. Validar saúde geral
sqlite-graphrag health --json
```

## What Happens Automatically

- `V014__pending_memories.sql` and `V015__pending_embeddings.sql` run on the first `init` or `migrate` invocation; both use `CREATE TABLE IF NOT EXISTS` so re-running is safe
- The `--llm-backend` flag defaults to `codex` if unset; behavior is identical to v1.0.81 for operators who never set the flag
- The slot semaphore is created on demand at `${XDG_RUNTIME_DIR:-~/.local/share}/sqlite-graphrag/llm-slots/`; no operator action required
- The shutdown JSON envelope replaces the old "panic-on-third-Ctrl-C" exit (ADR-0034, v1.0.80) when the signal arrives during a LLM subprocess; the legacy 130 exit on third signal still applies for non-LLM paths
- The pending-embeddings table starts empty; existing v1.0.81 databases have zero rows in it

## Library API Pinning

If you depend on the lib API, pin to the EXACT version in `Cargo.toml`:

```toml
[dependencies]
sqlite-graphrag = "=1.0.82"
```

The `^1.0` shorthand keeps you on the CLI-stability track. The `^1.0.82` shorthand allows 1.0.82..<1.1.0, which can include a future 1.0.83 with lib-breaking changes. For lib users, the exact pin is mandatory.

## What Breaks

- **Library consumers who enumerate the `CommandKind` enum**: 4 new variants (`Pending`, `Slots`, `Embedding`, `PendingEmbeddings`) are appended; non-exhaustive patterns will fail to compile
- **CI workflows that reference `--llm-backend claude` or `--llm-backend codex` as exclusive choices**: the new flag is a comma-separated chain; pre-v1.0.82 invocations of `--llm-backend foo` will now fail validation with exit 1 (single backend must not contain commas; chain must contain at least one of `codex`, `claude`, `none`)
- **Shell pipelines that grep stderr for "panic"**: the v1.0.80 third-Ctrl-C panic message no longer appears in v1.0.82; instead a JSON envelope appears on stdout at exit 19

## Rollback

If v1.0.82 is not working for you:

```bash
cargo install sqlite-graphrag --version 1.0.81 --force
```

The two new migrations (V014, V015) are NOT auto-reverted on rollback. If you need a true schema revert, restore from the pre-upgrade backup:

```bash
sqlite-graphrag --version  # confirm rolled back to 1.0.81
cp /var/backups/graphrag-pre-v1-0-82.sqlite ./graphrag.sqlite
sqlite-graphrag health --json   # confirm schema_v13
```

WARNING: the v1.0.81 binary will not understand the V014 and V015 tables; they will be ignored but still present in the file. A subsequent re-upgrade to v1.0.82 will skip them via `CREATE TABLE IF NOT EXISTS`.


# MIGRATING TO v1.0.78 — G41 Phantom V013 Registration Fix

## What Changed

- `run_rehash` no longer inserts phantom rows for unapplied migrations
- New `ensure_v013_tables_exist` helper repairs databases where V013 was registered but its tables were never created
- Auto-repair runs unconditionally in `ensure_db_ready` — any command heals corrupted databases

## Who Is Affected

- Users who ran `migrate --rehash` or `migrate --to-llm-only --drop-vec-tables` on v1.0.76 or v1.0.77
- Symptoms: `no such table: memory_embeddings` (exit 10) on `recall`, `hybrid-search`, `remember`

## How to Upgrade

```bash
cargo install sqlite-graphrag --version 1.0.78 --force
sqlite-graphrag migrate --rehash   # explicit repair (optional — any command auto-repairs)
```

## What Happens Automatically

- Any CRUD command (`remember`, `recall`, `hybrid-search`, etc.) detects and repairs the corrupted state
- The `ensure_v013_tables_exist` helper checks if V013 is in `refinery_schema_history` but the BLOB-backed tables are missing, and executes the V013 SQL directly
- V013 SQL is idempotent (`CREATE TABLE IF NOT EXISTS`) — safe to execute multiple times


# MIGRATING TO v1.0.77 — G40 Fix

> This guide is for operators affected by the v1.0.76 G40 bug where `migrate --rehash` inserted rows with `applied_on = NULL`

## What Changed in v1.0.77

- Fixed the INSERT in `run_rehash` that omitted the `applied_on` field
- Automatic sanitization of rows with `applied_on = NULL` before running the migration runner
- Removal of vec virtual tables via `PRAGMA writable_schema` when the `vec0` module is absent
- Fixed `debug-schema` crashing on databases with `applied_on = NULL`

## Who Is Affected

- Operators who ran `migrate --rehash` or `migrate --to-llm-only` on v1.0.76
- Databases showing `InvalidColumnType(Null at index: 2, name: applied_on)` errors
- v1.0.74 databases with vec virtual tables present

## How to Upgrade

```bash
cargo install sqlite-graphrag --version 1.0.77 --force
sqlite-graphrag migrate
```

- No manual SQL intervention is needed
- v1.0.77 automatically detects and fixes rows with `applied_on = NULL`
- Vec virtual tables are automatically removed via `writable_schema` if `vec0` is absent


# MIGRATING TO v1.0.76 — LLM-Only One-Shot

> This guide is for operators on v1.0.74 or v1.0.75 who want to
> upgrade to v1.0.76 without losing data.

## What Changed in v1.0.76

The default build is now **LLM-only and one-shot**:

- Embedding generation: `claude code` (Anthropic OAuth) or `codex`
  (OpenAI ChatGPT Pro OAuth), spawned per call. No daemon. No ONNX
  runtime. No model download.
- NER: the `LlmBackend` extracts entities and relationships via
  tool-use JSON. The default `extract_graph_auto` is URL regex only;
  full NER runs on demand with `--extraction-backend llm`.
- Vector search: pure-Rust cosine similarity over the BLOB-backed
  `memory_embeddings` / `entity_embeddings` / `chunk_embeddings`
  tables. The `sqlite-vec` C extension is REMOVED.

## Prerequisites

You need ONE of these on `PATH` after `cargo install`:

- `claude` — Claude Code CLI 2.1.0+ ([docs](https://docs.claude.com/claude-code))
- `codex` — OpenAI Codex CLI 0.130.0+
  ([repo](https://github.com/openai/codex))

Both must be logged in with the OAuth flow (Claude Pro/Max or
ChatGPT Pro subscription). API keys are NOT supported and cause
the spawn to ABORT with `AppError::Validation`.

To check:

```bash
which claude || which codex
claude --version  # must report 2.1.0 or higher
codex --version   # must report 0.130.0 or higher
```

## Step 1 — Install the Current Binary (v1.0.79)

```bash
cargo install sqlite-graphrag --version 1.0.79 --force
```

Install v1.0.79 (not 1.0.76): it carries the G40/G41 migration
repairs and the G42/G43 embedding fixes the upgrade path relies on.

This installs the LLM-only default build (~14.6 MiB binary, no
ONNX runtime, no model download). If you want the legacy
fastembed pipeline for the transition window:

```bash
cargo install sqlite-graphrag --version 1.0.76 --features embedding-legacy --force
```

The `embedding-legacy` feature was REMOVED in v1.0.79 (ahead of the
v1.1.0 schedule); the command above only works when pinning 1.0.76-1.0.78.

## Step 2 — Migrate the Existing Database

The migration is automatic on the next `init` / `remember` /
`ingest`. Migration V013 drops the `vec_memories`, `vec_entities`,
`vec_chunks` virtual tables and creates the new BLOB-backed
embedding tables. Existing memories are kept; their embeddings
are recomputed lazily on the next write.

To force an explicit migration:

```bash
sqlite-graphrag init --force
```

The output includes `schema_version: 13` when the migration
completes. Existing v1.0.74 / v1.0.75 databases will report
`schema_version: 12` until `init` runs.

## Step 3 — Re-Embed (Optional)

If you have a large corpus, re-embed it with the canonical one-shot
loop (G42/S9, v1.0.79). Each invocation processes a SMALL batch and
EXITS, so the job survives any external supervisor window:

```bash
# Re-embed memories without a vector row, 5 per invocation.
# Repeat (external loop) until the summary reports 0 completed items.
sqlite-graphrag enrich --operation re-embed --limit 5 --resume --mode codex --json
```

To force ONE memory to re-embed without touching its body, use
`edit --force-reembed` (v1.0.79):

```bash
sqlite-graphrag edit --name my-memory --force-reembed
```

WARNING — the pre-v1.0.79 recipe (`edit --description "rewarm embedding"`)
was WRONG: description-only edits skip re-embedding by design (v1.0.63)
and leave `memory_embeddings` untouched.

## Step 4 — Verify the LLM Path

Run a single `remember` to confirm the LLM is wired correctly:

```bash
sqlite-graphrag remember \
    --name smoke-test \
    --type note \
    --description "smoke test" \
    --body "if you can read this, the LLM is working"
```

The first call takes 1-3 seconds (LLM subprocess spawn). Subsequent
calls in the same process are not amortized (the CLI is one-shot)
but the LLM side may cache the embedding model internally.

## What Breaks on v1.0.74 Databases

| v1.0.74 behaviour | v1.0.76 behaviour |
| --- | --- |
| `sqlite-graphrag daemon` keeps the embedding model in memory | `sqlite-graphrag daemon` was fully removed in v1.0.76; each embedding call spawns an LLM subprocess |
| `--enable-ner` triggers the GLiNER ONNX loader (~30s cold start, 1.1 GB model download) | `--enable-ner` triggers URL regex only. Use `--extraction-backend llm` to get full NER via the LLM. |
| `vec_memories`, `vec_entities`, `vec_chunks` are sqlite-vec virtual tables | `memory_embeddings`, `entity_embeddings`, `chunk_embeddings` are regular BLOB-backed tables |
| Fastembed model: `multilingual-e5-small` (local, deterministic) | LLM model: `claude-sonnet-4-6` (claude) or `gpt-5.4` (codex) (network round-trip) |
| First `init` downloads 1.1 GB of ONNX weights | First `init` does a 1-3 s LLM round-trip |
| Embedding dimensionality fixed at 384 | Default 64 since v1.0.79, configurable via `SQLITE_GRAPHRAG_EMBEDDING_DIM` (range [8, 4096]); migrated databases keep their recorded 384 on every command (G43) and stay searchable; `enrich --operation re-embed --mode codex` re-embeds at the active dim |

## Rollback

If v1.0.76 is not working for you, the escape hatch is:

```bash
cargo install sqlite-graphrag --version 1.0.75 --force
```

Your v1.0.76 database has already been migrated to the new
schema (migration V013 ran on the first `init`). Reverting to
v1.0.75 will require `init --force` to recreate the vec tables
— you will lose the embeddings you built on v1.0.76 unless you
dump them first.

To dump the v1.0.76 embeddings before rollback:

```bash
sqlite3 graphrag.sqlite "SELECT memory_id, embedding FROM memory_embeddings" > embeddings-v1076.json
```

After the v1.0.75 reinstall, you can re-import the embeddings by
running the v1.0.75 `init --force` and then a batch `ingest` of
the original memory bodies. The v1.0.75 fastembed pipeline will
re-embed everything from scratch.

## Removed Features

| Feature | Removed in | Replacement |
| --- | --- | --- |
| `--enable-ner` (GLiNER ONNX) | v1.0.76 default | `--extraction-backend llm` |
| `vec_memories` / `vec_entities` / `vec_chunks` (sqlite-vec) | v1.0.76 | `memory_embeddings` / `entity_embeddings` / `chunk_embeddings` (BLOB) |
| `daemon` (infrastructure fully removed) | v1.0.76 | None — the LLM subprocess is the new "model loader" |
| `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` env vars | v1.0.69 (still enforced) | OAuth via `claude login` / `codex login` |
