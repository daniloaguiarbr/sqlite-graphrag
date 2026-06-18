## Custom Providers (v1.0.83+)
- sqlite-graphrag supports Anthropic-compatible providers (Minimax/api.minimax.io, OpenRouter, AWS Bedrock, corporate gateways) by preserving the following env vars when spawning `claude -p` or `codex exec`
- Preserved vars: `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT`
- The OAuth-only mandate remains active: `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` still abort the spawn with exit 1
- The four OAuth-only guards at `claude_runner.rs:273`, `codex_spawn.rs:259`, `ingest_claude.rs:282`, `extract/llm_embedding.rs:237-253` are unchanged; only the env-clear whitelist was extended
- Shared helper `src/spawn/env_whitelist.rs` exposes `apply_env_whitelist(cmd, strict)`; the three spawners delegate instead of inlining the array
- For compliance environments that require strict env-clear (PCI-DSS, SOC2, HIPAA), set `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` or pass `--strict-env-clear`; strict mode preserves only `PATH`
- No new telemetry: the fix is silent. No `tracing::info!` macro logs which provider is in use. The no-leak audit test `audit_no_token_leak_in_subprocess_stderr` in `tests/claude_runner_env.rs` enforces that the literal token value NEVER appears in stdout or stderr even with `RUST_LOG=trace`
- See `docs/decisions/adr-0041-preserve-custom-provider-env.md` and `docs/COOKBOOK.md#how-to-use-custom-anthropic-compatible-providers-v1083` for the full recipe
- Resolves GAP-058 partially: custom-provider env vars route around OAuth quota contention; `recall`/`hybrid-search` stay deterministic under official OAuth fatigue
# HOW TO USE sqlite-graphrag (v1.0.85 — Five-Gap Remediation, Claude Split, Hotfixes 85.1/85.2)

> Ship persistent memory to any AI agent with one local binary, a
> single SQLite file, and the LLM CLI you already trust.

- Versão em português: [HOW_TO_USE.pt-BR.md](HOW_TO_USE.pt-BR.md)
- Voltar ao [README.md](../README.md) para referência de comandos
X


## What v1.0.82 Changed (Five Gaps, Two Migrations, Four Subcommands)

v1.0.82 is a **patch** bump that DOES carry two additive database migrations (`V014__pending_memories`, `V015__pending_embeddings`). The schema version advances from 13 to 15. Library consumers must pin to `=1.0.82` per the stability policy (ADR-0032). The 5 gaps closed: GAP-001 three-stage `remember` checkpoint queue (ADR-0036), GAP-002 shutdown JSON envelope at exit code 19 (ADR-0037), GAP-003 `--llm-backend` user-choice flag (ADR-0038), GAP-004 host-wide LLM slot semaphore via `fs4` (ADR-0039), GAP-005 stderr-capture fallback chain that mitigates the codex OAuth 401 incident of 2026-06-14 (ADR-0040).

- **GAP-001 (ADR-0036)**: `pending_memories` table (V014) buffers the body, entities and relationships separately; SIGTERM during stage 2 or 3 leaves the row in `queued` for reprocessing. Inspect with `sqlite-graphrag pending list|show|cleanup --json`.
- **GAP-002 (ADR-0037)**: `SHUTDOWN_EXIT_CODE = 19` constant in `src/constants.rs`; any LLM-spawning command that receives SIGTERM/SIGINT/SIGHUP emits a deterministic JSON envelope on stdout. Envelope fields: `error`, `code`, `signal`, `graceful`, `message`. Schema: `docs/schemas/shutdown-envelope.schema.json`.
- **GAP-003 (ADR-0038)**: `--llm-backend <codex|claude|none,codex,...>` global flag; first non-error backend wins. `--llm-backend codex,claude,none` paired with `--skip-embedding-on-failure` allows null embedding when both backends fail.
- **GAP-004 (ADR-0039)**: Host-wide LLM slot semaphore via `fs4 = "0.9"` with `sync` feature (NOT `fs2`); `fcntl(F_SETLK)` on Linux/macOS, `LockFileEx` on Windows. Default `min(ncpus, oauth_tier_max)`. Inspect with `sqlite-graphrag slots status --json`; reap with `sqlite-graphrag slots release --slot-id <N> --yes`.
- **GAP-005 (ADR-0040)**: `pending_embeddings` table (V015) holds rows that failed every backend; the stderr-capture chain detects `refresh_token_reused` (2026-06-14 codex incident) and routes to the next backend. Inspect with `sqlite-graphrag embedding status|list --json`; retry with `sqlite-graphrag pending-embeddings process`.
## What Changed in v1.0.85, v1.0.85.1, v1.0.85.2 (ADR-0043, ADR-0044)

Since v1.0.84 (GAP-002 Claude backend split, ADR-0042), three further releases tightened the embedder:

### v1.0.85 — Five-Gap Remediation (ADR-0043)
- `FallbackReason` enum extended from 3 to 7 variants: `embedding_failed | slot_exhausted | oauth_quota | backend_mismatch | dim_zero | cancelled | timeout`
- `reason_code` discriminator in `recall` and `hybrid-search` envelopes distinguishes quota vs mismatch vs timeout
- `try_embed_query_with_deterministic_fallback` retries on `OAuthQuota` and applies 750ms ceiling on `SlotExhausted` before falling back to FTS5
- 12-14 `anthropic-ratelimit-*-remaining` headers captured in `LlmEmbedding::invoke_claude` (G45-CR5); `0` aborts embed and triggers codex fallback
- `dim 64` lock (Matryoshka Representation Learning, arXiv 2205.13147) reduces OAuth token spend by 6x (G56)
- 5 regression tests in `tests/embedder.rs`: `slot_exhaustion_returns_typed_error`, `oauth_quota_fallback_deterministic`, `anthropic_ratelimit_headers_captured`, `read_notfound_preserves_identifier`, `embedding_dim_reduces_token_cost`

### v1.0.85.1 — `recall`/`hybrid-search` `--llm-backend none` Graceful Fallback (GAP-004 hotfix)
- `--llm-backend none` now returns exit 0 with `vec_degraded: true` + `source: "fts_fallback"` + `vec_degraded_reason: "dim_zero"`
- Failsafe of v1.0.80 restored for the `--llm-backend none` case
- Intermediate arm `Ok((v, _backend)) if v.is_empty() => Err(FallbackReason::DimZero)` in `try_embed_query_with_choice`

### v1.0.85.2 — `embed_via_backend` Resolved Kind, `--dry-run-backend` Standalone (BUG-001/002/003, ADR-0044)
- `--dry-run-backend` works standalone (no subcommand required) thanks to `pub command: Option<Commands>` in `src/cli.rs:248`
- `embed_via_backend` returns `Result<(Vec<f32>, LlmBackendKind), AppError>` propagating `resolved_kind`
- 7 envelopes now report `backend_invoked: "claude" | "codex" | "none"` consistently
- `setup_mock_path()` in `tests/embedder.rs:37-77` aligned to emit JSON (not JSONL)

### v1.0.84 — Claude Backend Split (ADR-0042, GAP-002)
- `--llm-backend claude` now forces `claude -p` invocation, no silent codex fallback
- `LlmEmbeddingBuilder` in `src/extract/llm_embedding.rs` with `with_claude_builder`, `with_codex_builder`, `override_binary`, `override_model`
- `embed_via_claude_local` in `src/embedder.rs:190+` is the real split entry point
- `apply_env_whitelist_for_claude` in `src/spawn/env_whitelist.rs` (shared by `invoke_claude` and `embed_via_claude_local`)
- 5 regression tests in `tests/embedder.rs`: `embed_via_backend_claude_does_not_invoke_codex`, `embed_via_backend_codex_does_not_invoke_claude`, `embed_via_backend_none_returns_empty_vector`, `cli_dry_run_backend_prints_resolved_path`, `claude_invocation_uses_isolated_config_dir`

## What Changed in v1.0.85, v1.0.85.1, v1.0.85.2 (ADR-0043, ADR-0044)

Since v1.0.84 (GAP-002 Claude backend split, ADR-0042), three further releases tightened the embedder:

### v1.0.85 — Five-Gap Remediation (ADR-0043)
- `FallbackReason` enum extended from 3 to 7 variants: `embedding_failed | slot_exhausted | oauth_quota | backend_mismatch | dim_zero | cancelled | timeout`
- `reason_code` discriminator in `recall` and `hybrid-search` envelopes distinguishes quota vs mismatch vs timeout
- `try_embed_query_with_deterministic_fallback` retries on `OAuthQuota` and applies 750ms ceiling on `SlotExhausted` before falling back to FTS5
- 12-14 `anthropic-ratelimit-*-remaining` headers captured in `LlmEmbedding::invoke_claude` (G45-CR5); `0` aborts embed and triggers codex fallback
- `dim 64` lock (Matryoshka Representation Learning, arXiv 2205.13147) reduces OAuth token spend by 6x (G56)
- 5 regression tests in `tests/embedder.rs`: `slot_exhaustion_returns_typed_error`, `oauth_quota_fallback_deterministic`, `anthropic_ratelimit_headers_captured`, `read_notfound_preserves_identifier`, `embedding_dim_reduces_token_cost`

### v1.0.85.1 — `recall`/`hybrid-search` `--llm-backend none` Graceful Fallback (GAP-004 hotfix)
- `--llm-backend none` now returns exit 0 with `vec_degraded: true` + `source: "fts_fallback"` + `vec_degraded_reason: "dim_zero"`
- Failsafe of v1.0.80 restored for the `--llm-backend none` case
- Intermediate arm `Ok((v, _backend)) if v.is_empty() => Err(FallbackReason::DimZero)` in `try_embed_query_with_choice`

### v1.0.85.2 — `embed_via_backend` Resolved Kind, `--dry-run-backend` Standalone (BUG-001/002/003, ADR-0044)
- `--dry-run-backend` works standalone (no subcommand required) thanks to `pub command: Option<Commands>` in `src/cli.rs:248`
- `embed_via_backend` returns `Result<(Vec<f32>, LlmBackendKind), AppError>` propagating `resolved_kind`
- 7 envelopes now report `backend_invoked: "claude" | "codex" | "none"` consistently
- `setup_mock_path()` in `tests/embedder.rs:37-77` aligned to emit JSON (not JSONL)

### v1.0.84 — Claude Backend Split (ADR-0042, GAP-002)
- `--llm-backend claude` now forces `claude -p` invocation, no silent codex fallback
- `LlmEmbeddingBuilder` in `src/extract/llm_embedding.rs` with `with_claude_builder`, `with_codex_builder`, `override_binary`, `override_model`
- `embed_via_claude_local` in `src/embedder.rs:190+` is the real split entry point
- `apply_env_whitelist_for_claude` in `src/spawn/env_whitelist.rs` (shared by `invoke_claude` and `embed_via_claude_local`)
- 5 regression tests in `tests/embedder.rs`: `embed_via_backend_claude_does_not_invoke_codex`, `embed_via_backend_codex_does_not_invoke_claude`, `embed_via_backend_none_returns_empty_vector`, `cli_dry_run_backend_prints_resolved_path`, `claude_invocation_uses_isolated_config_dir`


### Migration Procedure (Operators on v1.0.80 / v1.0.81)
```bash
# 1. Backup before upgrade (recommended)
sqlite-graphrag backup --output /var/backups/graphrag-pre-v1-0-82.sqlite --json

# 2. Install v1.0.82
cargo install sqlite-graphrag --version 1.0.82 --force
sqlite-graphrag --version   # should report 1.0.82

# 3. Migrations V014 and V015 run automatically on first init/migrate
sqlite-graphrag migrate --json

# 4. codex login is MANDATORY after upgrade (OAuth 401 mitigation)
codex login

# 5. Smoke test the new subcommands
sqlite-graphrag pending list --json
sqlite-graphrag slots status --json
sqlite-graphrag embedding status --json
sqlite-graphrag pending-embeddings list --json
```

See [MIGRATION.md](MIGRATION.md) for the full 6-step procedure including rollback.


## What v1.0.80 Changed (G45, G53, G55 S2, G56, G58, ADR-0033, ADR-0034)

v1.0.80 is a **patch** bump with NO database migration. The schema
is still v13, the G43 dim-adoption already runs in every
`open_rw` and `open_ro`, and the changes are all additive at
the binary and database level. Library consumers must pin to
`=1.0.80` because the lib API is unstable within v1.x.y
(ADR-0032).

- **G45 cross-process embedding singleton**: `acquire_embedding_singleton(namespace, db_path, wait_seconds, force)` serialises LLM embedding calls per `(namespace, db)` pair across concurrent CLI invocations. A second CLI trying to embed against the same database receives `AppError::EmbeddingSingletonLocked { namespace }` (exit 75, retryable). Pass `--wait-embed-singleton <SECONDS>` to poll until the lock drops; distinct databases or namespaces acquire independent locks. Operationally prevents the "two remember invocations, two LLM subprocesses, two parallel batches" pathology that v1.0.79's in-process cache could not address.
- **G53 stability policy and `semver-checks` CI gate**: the public contract is the CLI; the library API is unstable in v1.x.y. New CI job `semver-checks` runs `cargo semver-checks check-baseline --baseline-version 1.0.79` in informational mode (becomes blocking in v1.0.81 once the 9 outstanding MAJOR violations are resolved). README and CHANGELOG carry the `Stability Policy` section. Pin to `=1.0.80` for lib consumers; use `^1.0` to stay on the CLI-stable track.
- **G55 S2 structural `MemoryNotFound`**: the legacy `NotFound(String)` path that masked which lookup target failed is replaced by `AppError::MemoryNotFound { name, namespace }` and `AppError::MemoryNotFoundById { id }` inside `read` and `hybrid-search`. The identifier is now part of the variant, eliminating the `not found: unknown` class of bugs. pt-BR messages carry the name and namespace explicitly.
- **G56 entity-embed in-process cache**: `embed_entity_texts_cached` sits in front of `embed_passages_parallel_local` for entity-name batches. Cache key is `blake3(model || "\0" || text)`. High hit rate in `ingest` (canonical entities re-embedded across many memories), modest in `remember` and `remember-batch`. `remember.rs`, `ingest.rs` and `remember_batch.rs` all route entity embeds through the cache; chunk embeds continue through the raw path. Stats are emitted via `tracing::debug!` (hit / miss / request counts).
- **G58 FTS5 fallback for `recall` and `hybrid-search`**: `recall --fallback-fts-only` and `hybrid-search --fallback-fts-only` route the query through FTS5 BM25 when the LLM subprocess fails (rate limit, OAuth contention, divergent dim). New envelope fields `vec_degraded` (bool), `vec_error` (string) and `warning` (string) are populated symmetrically across both commands. The `recall` and `hybrid-search` tests gained coverage for the FTS5-only path; 1 test is `#[ignore]` because the G58 S1 stub requires `PATH` without `codex` or `claude` to exercise `EmbeddingFailed`.
- **G53-WINDOWS-INFRA (ADR-0033)**: the `clippy` and `test` jobs of the windows-2025 matrix gained 2 new steps each (gated `if: matrix.os == 'windows-2025'`, no-op on ubuntu/macos): a pre-warm that downloads the rustup toolchain into the runner cache before the build, and a verify step that re-checks `rustup show active-toolchain` after install. The 2 historical infra failure modes (rustup download with transient network errors and `E0463 can't find crate for core` when the target stdlib is missing) are now recoverable on the first re-run instead of accumulating as red CI. Local cross-compile validation: `cargo check --target x86_64-pc-windows-msvc --lib --all-features` reproduces and `E0463` is fixed by `rustup target add x86_64-pc-windows-msvc --toolchain 1.88`; the build then reaches the `cc-rs: failed to find tool "lib.exe"` frontier, which is the expected host-Linux cross-compile limit.
- **SHUTDOWN resilience (ADR-0034)**: `src/signals.rs` is wrapped in a panic-catching boundary; even when the parent's stderr is a closed pipe (the orphaned-process scenario that the G42/C2 audit identified), the handler returns cleanly instead of `SIGABRT`-ing on `BrokenPipe`. The third consecutive Ctrl-C exits with code 130 and ZERO I/O, matching the contract documented in ADR-0034 and the recipe in `docs/HEADLESS_INVOCATION.md`. The 3-layer SHUTDOWN bypass recipe (`nohup` then `setsid` then `disown`) is the canonical reference for the agent harness when running long embedding jobs in background.

## What v1.0.79 Changed (G42 + G43)

The G42 work made the embedding pipeline fast, parallel and
batched; G43 made the dimensionality adoption universal:

- Default embedding dimensionality dropped from 384 to 64
  (configurable via `SQLITE_GRAPHRAG_EMBEDDING_DIM`, range
  [8, 4096]); pre-existing databases keep their recorded
  `schema_meta.dim` on every command (`open_rw`/`open_ro`
  adoption, G43).
- Embedding calls are batched (`{items:[{i,v}]}`; chunks at 8,
  entity names at 25 at dim 64; dim-adaptive — G44) and run in parallel under a bounded
  semaphore: `--llm-parallelism` on `remember` (default 4),
  `ingest` (default 2) and `edit` (default 4), clamp [1, 32].
- `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` selects the claude
  embedding model; `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS`
  (default 300) bounds each LLM call.
- `enrich --operation re-embed` and `edit --force-reembed` are
  the canonical re-embed paths.
- The remaining daemon code was deleted; the `embedding-legacy`
  and `ner-legacy` features were removed; `--enable-ner` is
  URL-regex only and the GLiNER-era flags warn as no-ops.


## What v1.0.76 Changed

The default build is now **LLM-only and one-shot**. There is no
local embedding model, no GLiNER NER, no ONNX runtime, no
`sqlite-vec` C extension. Every `remember` / `ingest` / `edit`
spawns a headless LLM subprocess (claude code or codex CLI) that
returns the embedding and (optionally) the extracted entities.

The CLI is one-shot: there is no daemon, no model to keep in
memory, no socket to clean up. The release binary is ~6 MB (was
39 MB) and the cold start is 1-3 s (was 30 s with the ONNX model
load).


## Prerequisites

You need ONE of these CLIs installed and on `PATH`:

- `claude` — Claude Code CLI 2.1.0+
  ([install](https://docs.claude.com/claude-code))
- `codex` — OpenAI Codex CLI 0.130.0+
  ([repo](https://github.com/openai/codex))

Both must be logged in with the **OAuth flow** (Claude Pro/Max
or ChatGPT Pro subscription). API keys are NOT supported — see
the "OAuth enforcement" section below.

To check:

```bash
which claude || which codex
claude --version
codex --version
```


## OAuth Enforcement

v1.0.76 inherits the OAuth-only mandate from v1.0.69. If
`ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is set in the
environment, the LLM spawn ABORTS with `AppError::Validation`
and the CLI exits with code 1.

To unset:

```bash
unset ANTHROPIC_API_KEY
unset OPENAI_API_KEY
```

The two API-key env vars are also excluded from the
env-clear whitelist, so they cannot bypass the check even when
set in a parent process.


## Install

```bash
cargo install sqlite-graphrag --version 1.0.79 --force
```

This installs the LLM-only default build. Verify:

```bash
sqlite-graphrag --version
# sqlite-graphrag 1.0.80
```

For the legacy fastembed pipeline (REMOVED in v1.0.79):

```bash
# REMOVED in v1.0.79: the embedding-legacy feature no longer exists.
# Versions 1.0.76-1.0.78 accepted it; pin one of those versions if you
# absolutely need the legacy fastembed pipeline (unsupported).
```


## Initialize a Database

```bash
sqlite-graphrag init --namespace my-project
```

The `init` command:

1. Creates `graphrag.sqlite` in the current directory.
2. Runs all migrations including V013 (drops vec tables, creates
   `memory_embeddings` / `entity_embeddings` / `chunk_embeddings`).
3. Spawns the LLM once to confirm the OAuth session is valid.
4. Reports `schema_version: 13` on success.

The first `init` is slow (1-3 s LLM round-trip). Subsequent
`init` calls are no-ops (the schema is already at the target
version).


## Persist Your First Memory

```bash
sqlite-graphrag remember \
    --name auth-decision-2026-06 \
    --type decision \
    --description "JWT token rotation strategy with 15-min expiry" \
    --body "We picked JWT with a 15-minute access token and a
    7-day refresh token. The refresh flow uses HttpOnly cookies.
    See https://auth0.com/docs/refresh-tokens for the spec." \
    --entities-file entities.json
```

Where `entities.json` is:

```json
[
  {"name": "JWT", "entity_type": "concept"},
  {"name": "Auth0", "entity_type": "tool"}
]
```

The `remember` command:

1. Calls the LLM to embed the body — batched and parallel since
   v1.0.79 (`--llm-parallelism`, default 4; 1-3 s per call).
2. Stores the memory in `memories` (FTS5 indexed).
3. Stores the embedding as a BLOB in `memory_embeddings`.
4. Links the entities via the `entities` table.
5. Returns JSON with `memory_id`, `version`, `elapsed_ms`.


## Search Memories

The two main search commands are:

```bash
# Exact-token + semantic search, fused via RRF
sqlite-graphrag hybrid-search "auth jwt design" --k 10 --json

# Semantic-only (no FTS5 component)
sqlite-graphrag recall "auth jwt design" --k 5 --no-graph --json
```

For the default namespace size (10k memories or fewer), the
cosine refinement over the embedding BLOB is fast enough
(single-digit ms). For larger namespaces, prefer
`hybrid-search` so FTS5 does the coarse filtering.


## Extract Entities via the LLM

The default `remember` does URL extraction only. For full NER
(entities + typed relationships), use the LLM backend:

```bash
sqlite-graphrag remember \
    --name design-review-q2 \
    --type note \
    --description "Q2 design review notes" \
    --body "$(cat design-review.md)" \
    --extraction-backend llm
```

The LLM returns structured JSON with entities and relationships
in the same prompt that produces the embedding. The total round-trip
is 3-8 s (longer than the embed-only path because the prompt
includes the schema and the response is larger).


## LLM Quality Tools (inherited from v1.0.69)
### `enrich` — LLM-Augmented Graph Quality
- The `enrich` subcommand runs LLM-curated graph-quality operations. Three are fully implemented: `memory-bindings` (extract entities from orphan memories), `entity-descriptions` (fill NULL/empty entity descriptions), and `body-enrich` (expand short memory bodies into richer content).
- Two more operations are scan-only and surface candidate lists without rewriting: `weight-calibrate`, `relation-reclassify`, `entity-connect`, `entity-type-validate`, `description-enrich`, `cross-domain-bridges`, `domain-classify`, `graph-audit`, `deep-research-synth`, `body-extract`.
- `--mode claude-code` or `--mode codex` selects the LLM provider. The default is `claude-code`. Both providers are OAuth-only since v1.0.69.
- `--preflight-check` issues a 1-turn ping BEFORE scanning the candidate set. On a Claude OAuth rate limit the probe aborts with a clear error (or switches to `--fallback-mode` when supplied). Default off to keep `--dry-run` and CI flows zero-cost.
- `--fallback-mode <claude-code|codex>` automatically switches provider when the preflight probe or an in-flight call hits a rate limit. Ignored when `--mode` is already `codex`.
- `--rate-limit-buffer <SECONDS>` defaults to 300. When the preflight probe detects that the OAuth rate-limit reset is less than the buffer away, it aborts with a suggestion to wait.
- `--names <a,b,c>` and `--names-file <PATH>` select a specific subset of memory names instead of scanning all candidates. `--names-file` accepts `#` comments and blank lines. Both flags combine as a union when both are set.
- `--preserve-threshold <FLOAT>` (default 0.7) controls the Jaccard trigram similarity gate for `body-enrich`. When the LLM rewrite scores below the threshold, the enriched body is REJECTED and emitted as `EnrichItemResult::PreservationFailed`. Protects against LLM invention.
- `--llm-parallelism <N>` spawns N parallel LLM worker threads (default 1, max 32). Codex tolerates up to 16 in production; Claude warns above 4 because of the OAuth-MCP fan-out. Since v1.0.79 the same flag also exists on `remember` (default 4), `ingest` (default 2) and `edit` (default 4) for the embedding fan-out.
- `--max-load-check` refuses to start when the 1-minute load average exceeds `2 × ncpus`. Set to false on contended CI runners.
- `--circuit-breaker-threshold <N>` (default 5) aborts the job after N consecutive `HardFailure` outcomes. Transient rate-limit and timeout errors do not count.
- `--codex-model-validate` (default true) checks `--codex-model` against the ChatGPT Pro OAuth accepted-model list BEFORE the subprocess is spawned. Use `--codex-model-fallback <MODEL>` to auto-substitute a known-good model instead of aborting.
- `--dry-run` previews the candidate set without spawning any LLM. Output is NDJSON with one event per memory and a final summary.
- `--resume` continues a previously interrupted batch from the queue DB. `--retry-failed` retries only the failed items.
### `vec` — Vector Index Maintenance (G39)
- `vec orphan-list --json` lists memory embedding rows whose `memory_id` no longer exists in the `memories` table. Each row reports the `vector_hash` (BLAKE3 of the embedding blob) for traceability.
- `vec purge-orphan --yes --dry-run --json` previews the deletion count without removing anything.
- `vec purge-orphan --yes --json` purges the THREE vec tables (`vec_memories`, `vec_entities`, `vec_chunks`) in a single implicit transaction. The response reports `deleted`, `deleted_entities`, `deleted_chunks`, and `elapsed_ms`.
- `vec stats --json` exposes `vec_memories_rows`, `vec_entities_rows`, `vec_chunks_rows`, `orphans`, and the last vacuum timestamp. Use it to audit vector-table health after bulk `forget` cycles.
- The `forget` subcommand now calls `memories::delete_vec` BEFORE the soft-delete, preventing new orphans in the steady state.
### `codex-models` — Discover ChatGPT Pro OAuth Models (G33)
- `codex-models --json` returns the accepted-model list, the count, and the default. Currently: `codex-auto-review`, `gpt-5.3-codex-spark`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.5`.
- `codex-models --suggest <substring> --json` returns the closest match via substring lookup with a Levenshtein fallback. Useful when an operator types `o4-mini` and wants to know the closest accepted alternative.
### `optimize` and `backup` Hardening (G36 + G38)
- `optimize` now pre-checks FTS5 health via `check_fts_functional` BEFORE rebuilding. A healthy index is no longer rebuilt (saves ~10 minutes on a 4.3 GB database). Force a rebuild with `--no-fts-skip-when-functional`.
- `optimize --fts-dry-run --json` exits 1 if the FTS5 index needs a rebuild, 0 otherwise. CI-friendly.
- `optimize --fts-progress <N>` (default 30) emits a progress line every N seconds during the rebuild. Set to 0 to disable.
- `optimize --yes` skips the confirmation prompt. Required for non-interactive CI.
- `backup` defaults to `run_to_completion(1000, Duration::from_millis(5), None)` (was 100/50ms). For a 4.3 GB database this is a 25x speedup (~21s vs ~9 min).
- `backup --backup-step-size <PAGES>` and `--backup-step-sleep-ms <MS>` tune the page-copy granularity. `--backup-no-sleep` removes the inter-step sleep entirely for maximum throughput. `--backup-progress <PAGES>` (default 100) emits a progress line every N pages.
### `migrate` Subcommand Family (v1.0.76, updated v1.0.77 and v1.0.78)
- `migrate --rehash --json` rewrites recorded migration checksums to match the current file content. Idempotent. Required for v1.0.74 → v1.0.76 upgrades where the V002 migration was intentionally emptied to a no-op.
- `migrate --to-llm-only --drop-vec-tables --json` is the one-shot upgrade for v1.0.74 / v1.0.75 databases. Combines `--rehash` with the V013 vec-table drop. The `--drop-vec-tables` flag is REQUIRED as an explicit safety guard. The BLOB-backed `memory_embeddings` / `entity_embeddings` / `chunk_embeddings` tables remain and are the source of truth going forward; embeddings are recomputed lazily on the next `remember` / `edit` / `ingest`.
- v1.0.77 fix (G40): JSON response for both commands now includes `null_rows_fixed` (integer) and `vec_tables_removed_via_writable_schema` (integer). Databases with `applied_on = NULL` rows are auto-sanitized before the migration runner executes.
- v1.0.78 fix (G41): JSON response for both commands now includes `v013_tables_created` (boolean). Databases where V013 was registered in `refinery_schema_history` but the BLOB-backed embedding tables were never created are auto-repaired. Any CRUD command also triggers this repair unconditionally via `ensure_db_ready`.


## Migration from v1.0.74 / v1.0.75

See [MIGRATION.md](MIGRATION.md) for the full step-by-step. The
short version:

1. Install v1.0.76 (LLM-only).
2. Run `sqlite-graphrag init` — migration V013 runs automatically.
3. Old vec tables are dropped; new `memory_embeddings` is empty.
4. Memories are re-embedded lazily on the next `edit` / `ingest`.

For a large corpus, use the canonical one-shot re-embed loop
(G42/S9, v1.0.79) — each invocation processes a small batch and exits:

```bash
sqlite-graphrag enrich --operation re-embed --limit 5 --resume --json
```

Note: the old `edit --description "<same>"` recipe never re-embedded
anything (description-only edits are a no-op for embeddings); use
`edit --force-reembed` for a single memory.


## CI Test Environment

If you want to run the full test suite in CI, you need an LLM
CLI on `PATH`. The v1.0.76 build does not embed via fastembed in
the default configuration, so `v1044_features` /
`signal_handling_integration` / `v2_breaking_integration` will
fail with `no LLM CLI found on PATH` when neither `claude` nor
`codex` is installed.

Workarounds:

1. Install `claude` in the CI image and authenticate via OAuth
   (requires storing OAuth tokens in CI secrets).
2. Use a mock LLM CLI that returns a fixed JSON response for
   the embedding prompt (used internally for the unit tests in
   `src/extract/llm_embedding.rs`).


## See Also

- [COOKBOOK.md](COOKBOOK.md) for common recipes
- [MIGRATION.md](MIGRATION.md) for v1.0.74 → v1.0.76 upgrade
- [CROSS_PLATFORM.md](CROSS_PLATFORM.md) for Windows / macOS
- [AGENTS.md](AGENTS.md) for agent integration
- [HEADLESS_INVOCATION.md](HEADLESS_INVOCATION.md) for OAuth-safe Claude/Codex/OpenCode headless invocation
- [decisions/](decisions/) for the 26 ADRs
