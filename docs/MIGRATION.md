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

This installs the LLM-only default build (~6 MB binary, no
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
sqlite-graphrag enrich --operation re-embed --limit 5 --resume --json
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
| Embedding dimensionality fixed at 384 | Default 64 since v1.0.79, configurable via `SQLITE_GRAPHRAG_EMBEDDING_DIM` (range [8, 4096]); migrated databases keep their recorded 384 on every command (G43) and stay searchable; `enrich --operation re-embed` re-embeds at the active dim |

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
