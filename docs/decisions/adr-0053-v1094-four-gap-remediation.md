# ADR-0053 — v1.0.94 Four-Gap Remediation

**Status**: Accepted
**Date**: 2026-06-26
**Context**: sqlite-graphrag v1.0.94 — GAP-OR-ENTITY-EMBED, GAP-EMBED-DIM-64, GAP-EMBED-TIMEOUT-300, GAP-HEADLESS-DEFAULT

## Problem

v1.0.93 shipped the OpenRouter REST embedding backend (ADR-0052) but
left four gaps open, documented in `gaps.md`. They shared a common
theme: the entity-embedding path and several defaults were still
calibrated for the legacy codex subprocess era, not for the OpenRouter
REST default.

1. **GAP-OR-ENTITY-EMBED** — Entity embedding in `remember`,
   `remember-batch` and `ingest` ignored `--embedding-backend` and
   `--llm-backend`, calling the codex embedder directly. A `remember`
   with new entities waited on the codex subprocess until the internal
   timeout (~119s), even when the user requested OpenRouter.

2. **GAP-EMBED-DIM-64** — `DEFAULT_EMBEDDING_DIM` was 64
   (`src/constants.rs`), while the production corpus is indexed at 384.
   Worse, `src/main.rs` froze the OpenRouter client dimension with a
   hardcoded `unwrap_or(64)` at eager startup — before the database
   opened — so neither the env var nor the database `schema_meta.dim`
   could correct it. Every operation without an explicit
   `--embedding-dim 384` produced 64-dim vectors that collided with the
   384-dim index and aborted KNN with exit 11.

3. **GAP-EMBED-TIMEOUT-300** — `DEFAULT_EMBED_TIMEOUT_SECS` was 120s
   (`src/extract/llm_embedding.rs`), the only LLM subprocess left
   behind when `ingest`, `enrich` and `opencode` adopted 300s.

4. **GAP-HEADLESS-DEFAULT** — `enrich --mode` defaulted to
   `claude-code` (`src/commands/enrich.rs`). Omitting `--mode` silently
   spawned `claude -p`, which inherits the caller's project `.mcp.json`
   and fails in headless contexts.

## Decision

Apply four surgical fixes in v1.0.94.

### FIX-1: Entity embedding honours the selected backends

`embed_entity_texts_cached` in `src/embedder.rs` now takes
`embedding_backend: EmbeddingBackendChoice` and
`llm_backend: LlmBackendChoice`. Cache misses route through
`embed_passages_parallel_with_embedding_choice` (OpenRouter REST when
the resolved chain starts with OpenRouter, local LLM otherwise) instead
of the codex-only `embed_texts_parallel`. A `none`-chain short-circuit
returns empty vectors WITHOUT spawning any subprocess. The entity cache
key is now backend-aware (`openrouter:{dim}`) so codex and OpenRouter
vectors never collide. Callers updated: `remember.rs`,
`remember_batch.rs`, `ingest.rs`. `remember` with new entities drops
from ~119s to ~0.9s under OpenRouter.

### FIX-2: Default embedding dimension raised 64 -> 384

`DEFAULT_EMBEDDING_DIM` changed to 384 in `src/constants.rs`, and
`src/main.rs` now calls `constants::embedding_dim()` (env > ACTIVE >
default) instead of the hardcoded `unwrap_or(64)`. New databases via
`init` stamp `dim=384` in `schema_meta`, matching the production corpus.
Legacy 64-dim databases are preserved via `schema_meta.dim` precedence
— no forced re-embed. The 64 default was a deliberate G42/v1.0.79
choice to cut autoregressive token cost on the codex embedding path; it
is moot now that OpenRouter REST is the operational default, where MRL
truncation happens server-side at zero token cost.

### FIX-3: Embedding subprocess timeout raised 120s -> 300s

`DEFAULT_EMBED_TIMEOUT_SECS` changed to 300 in
`src/extract/llm_embedding.rs`, aligning the embedding subprocess with
`ingest`/`enrich`/`opencode`. The env override
`SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` and the `[10, 3600]` clamp are
unchanged.

### FIX-4: `enrich --mode` is now required

Removed `default_value = "claude-code"` from the `mode` argument in
`src/commands/enrich.rs`; the field stays `EnrichMode` (not `Option`),
so clap makes `--mode` mandatory. Omitting it is rejected with exit 2,
preventing accidental `claude -p` spawns. Valid values: `claude-code`,
`codex`, `opencode`.

## Alternatives Considered

### A. Add a new `--entity-embedding-backend` flag (GAP-OR-ENTITY-EMBED)

Rejected (YAGNI). The existing `--embedding-backend`/`--llm-backend`
already express intent; the entity path simply has to honour them by
reusing `embed_passages_parallel_with_embedding_choice`.

### B. Reorder the eager OpenRouter init after the DB opens (GAP-EMBED-DIM-64)

Deferred. Raising the default to 384 fixes the common case at zero
risk. Reordering startup for non-384 legacy databases is a future
improvement; users with such databases still pass `--embedding-dim`.

### C. Make `enrich --mode` an `Option` with a hard error when absent

Rejected. clap's required-argument behaviour (no `default_value` on a
non-`Option` field) already yields exit 2 with a clear message, mirror-
ing the existing required `operation` argument — no custom error path
needed.

## Consequences

- `remember`/`remember-batch`/`ingest` embed entities via the selected
  backend; new-entity writes finish in sub-second time under OpenRouter.
- `recall`, `hybrid-search`, `deep-research`, `remember` and `ingest`
  work without an explicit `--embedding-dim 384` on fresh 384-dim
  databases; the exit-11 dimension mismatch leaves the default flow.
- The embedding subprocess no longer aborts early under cold start or
  large batches.
- `enrich` can no longer silently spawn `claude -p`; the mode is an
  explicit, auditable choice.
- Breaking change for scripts: every `enrich` invocation MUST now pass
  `--mode`. Canonical pairing with `--llm-backend`: `codex` -> `codex`,
  `claude` -> `claude-code`, `opencode` -> `opencode`.

## Validation

- Build: `cargo build --release` 0 errors; `cargo clippy -- -D warnings`
  0 warnings; `cargo fmt --check` 0 diffs.
- Test suite: `cargo test` exit 0; regression tests renamed
  (`init_default_dim_is_384`, `embed_timeout_default_is_300`) and a
  contract test asserting `enrich` without `--mode` is rejected (clap
  exit 2).
- E2E: `init` stamps `dim=384`; `remember` + new entity via OpenRouter
  = 913ms with `backend_invoked=openrouter`; `enrich` rejects missing
  `--mode`.

## Cross-references

- `gaps.md` — the four gaps marked RESOLVIDO em v1.0.94
- ADR-0052 (OpenRouter embedding backend) — the v1.0.93 predecessor
- ADR-0050 (embedding deadlock remediation) — prior timeout/flag work
- `src/constants.rs` (DEFAULT_EMBEDDING_DIM), `src/main.rs` (eager init),
  `src/extract/llm_embedding.rs` (DEFAULT_EMBED_TIMEOUT_SECS),
  `src/commands/enrich.rs` (mode argument), `src/embedder.rs`
  (`embed_entity_texts_cached`)
