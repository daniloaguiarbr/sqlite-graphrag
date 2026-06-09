# HOW TO USE sqlite-graphrag (v1.0.76 — LLM-Only)

> Ship persistent memory to any AI agent with one local binary, a
> single SQLite file, and the LLM CLI you already trust.

- Versão em português: [HOW_TO_USE.pt-BR.md](HOW_TO_USE.pt-BR.md)
- Voltar ao [README.md](../README.md) para referência de comandos


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
cargo install sqlite-graphrag --version 1.0.76 --force
```

This installs the LLM-only default build. Verify:

```bash
sqlite-graphrag --version
# sqlite-graphrag 1.0.76
```

For the legacy fastembed pipeline (transition window, REMOVED
in v1.1.0):

```bash
cargo install sqlite-graphrag --version 1.0.76 --features embedding-legacy --force
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

1. Calls the LLM to embed the body (1-3 s).
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
- `--llm-parallelism <N>` spawns N parallel LLM worker threads (default 1, max 32). Codex tolerates up to 16 in production; Claude warns above 4 because of the OAuth-MCP fan-out.
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
### `migrate` Subcommand Family (v1.0.76)
- `migrate --rehash --json` rewrites recorded migration checksums to match the current file content. Idempotent. Required for v1.0.74 → v1.0.76 upgrades where the V002 migration was intentionally emptied to a no-op.
- `migrate --to-llm-only --drop-vec-tables --json` is the one-shot upgrade for v1.0.74 / v1.0.75 databases. Combines `--rehash` with the V013 vec-table drop. The `--drop-vec-tables` flag is REQUIRED as an explicit safety guard. The BLOB-backed `memory_embeddings` / `entity_embeddings` / `chunk_embeddings` tables remain and are the source of truth going forward; embeddings are recomputed lazily on the next `remember` / `edit` / `ingest`.


## Migration from v1.0.74 / v1.0.75

See [MIGRATION.md](MIGRATION.md) for the full step-by-step. The
short version:

1. Install v1.0.76 (LLM-only).
2. Run `sqlite-graphrag init` — migration V013 runs automatically.
3. Old vec tables are dropped; new `memory_embeddings` is empty.
4. Memories are re-embedded lazily on the next `edit` / `ingest`.

For a large corpus, batch-pre-warm with:

```bash
sqlite-graphrag list --json | jaq -r '.items[].name' | \
    xargs -I {} sqlite-graphrag edit --name {} \
        --description "$(sqlite-graphrag read --name {} --json | jaq -r .description)"
```


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
2. Build with `--features embedding-legacy` to restore the
   fastembed pipeline; the relevant tests then pass without an
   LLM. The CI workflow is updated in v1.0.76 to test all three
   configurations (default, llm-only, embedding-legacy).
3. Use a mock LLM CLI that returns a fixed JSON response for
   the embedding prompt (used internally for the unit tests in
   `src/extract/llm_embedding.rs`).


## See Also

- [COOKBOOK.md](COOKBOOK.md) for common recipes
- [MIGRATION.md](MIGRATION.md) for v1.0.74 → v1.0.76 upgrade
- [CROSS_PLATFORM.md](CROSS_PLATFORM.md) for Windows / macOS
- [AGENTS.md](AGENTS.md) for agent integration
- [HEADLESS_INVOCATION.md](HEADLESS_INVOCATION.md) for OAuth-safe Claude/Codex/OpenCode headless invocation
- [decisions/](decisions/) for the 26 ADRs
