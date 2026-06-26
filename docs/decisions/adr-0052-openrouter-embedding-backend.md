# ADR-0052: OpenRouter Embedding Backend

- Status: ACCEPTED
- Date: 2026-06-25
- Supersedes: none
- Related: ADR-0010 (LLM-only build), ADR-0039 (slot semaphore), ADR-0041 (custom provider env)

## Context

Prior to v1.0.93, every embedding call spawned a headless subprocess — either `codex exec`, `claude -p`, or `opencode` — to compute the embedding vector. Subprocess cold-start on a fresh OAuth token routinely exceeds 15 seconds per call. For commands that embed many chunks (e.g. `ingest`, `enrich --operation re-embed`), the accumulated subprocess overhead dominated wall-clock time.

OpenRouter exposes a REST API at `https://openrouter.ai/api/v1/embeddings` that accepts an OpenAI-compatible request body and returns a float vector. A single HTTP round-trip to OpenRouter completes in roughly 200ms, eliminating the subprocess penalty entirely.

The challenge was to add this REST path without conflating it with the existing `LlmBackendChoice` enum (which governs text-generation backends, not embedding backends) and without breaking the OAuth-only enforcement that prevents `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` from leaking into subprocess environments.

## Decision

Introduce a separate `EmbeddingBackendChoice` enum in `src/embed/backend.rs` with variants:

- `Codex` — existing codex subprocess path (default)
- `Claude` — existing claude subprocess path
- `OpenCode` — existing opencode subprocess path
- `OpenRouter` — new REST API path (this ADR)
- `None` — null embedding (skip-on-failure path)

The OpenRouter variant is implemented via `reqwest` with `rustls-tls` (no native TLS dependency). The HTTP client sends a POST to `https://openrouter.ai/api/v1/embeddings` with the model name and input text, receives a float array, and truncates to 64 dimensions using MRL (Matryoshka Representation Learning) truncation — the same 64-dim target used by all other embedding backends.

Three new CLI flags are added to every subcommand that accepts `--embedding-backend`:

- `--embedding-backend openrouter` — selects the REST path
- `--embedding-model <MODEL>` — selects the embedding model (REQUIRED; no default model — the user MUST specify)
- `--openrouter-api-key <KEY>` — API key for OpenRouter (NOT forwarded to subprocesses; stored only in the reqwest client)

The `--openrouter-api-key` flag is NEVER written to logs, NEVER echoed in error messages, and NEVER passed via environment variables. It is consumed exclusively by the in-process reqwest client and dropped after the embedding call completes.

The existing `--llm-backend` flags and `LlmBackendChoice` enum are UNCHANGED. OpenRouter as an embedding backend does not affect text generation.

## MRL Truncation Rationale

OpenRouter models return variable-length vectors (e.g. `text-embedding-3-small` returns 1536 dimensions by default). sqlite-graphrag stores all embeddings in a fixed-width `BLOB` column sized for 64 float32 values (256 bytes). MRL truncation to the first 64 dimensions preserves the highest-information components of the embedding while keeping the schema unchanged. The same truncation is applied by the codex and claude subprocess paths.

## Consequences

### Positive
- Embedding latency drops from ~15s (subprocess cold-start) to ~200ms (HTTP round-trip) for OpenRouter models.
- No new native dependencies — `reqwest` + `rustls-tls` is already present in the dependency tree for `duckduckgo-search-cli` patterns; adding it here adds zero new compilation units.
- `EmbeddingBackendChoice` separation ensures that operators who use OpenRouter for embeddings can still use codex or claude for text generation without interference.
- The OpenRouter API key is never exposed to subprocesses, preserving the OAuth-only subprocess security model.

### Negative
- OpenRouter embedding requires a paid API key (`OPENROUTER_API_KEY`). The subprocess backends (codex, claude, opencode) are OAuth-only and require no paid credentials.
- E2E tests in `tests/openrouter_embedding.rs` require `OPENROUTER_API_KEY` to be set. These tests are excluded from the `ci` nextest profile and must be run manually or in a separate CI job with the secret injected.
- MRL truncation to 64 dimensions means semantic fidelity is lower than full-resolution embedding. This tradeoff is accepted because all existing embeddings in the database use the same 64-dim truncation, and cross-backend cosine similarity is only valid when all vectors share the same dimension and truncation strategy.

## Alternatives Considered

### Embed via subprocess with OpenRouter credentials
Passing `OPENROUTER_API_KEY` as an environment variable to a child process was rejected because it violates the OAuth-only subprocess security model. Any subprocess could read and exfiltrate the key.

### Extend `LlmBackendChoice` with an `OpenRouter` variant
Rejected because `LlmBackendChoice` governs text-generation backends. Merging embedding selection into the same enum would make it impossible to independently configure the text-generation backend and the embedding backend — a common operator need (e.g. use codex for generation, OpenRouter for fast embedding).

### Store full-resolution vectors and resize the schema
Rejected because it requires a schema migration that invalidates all existing embeddings and changes the `BLOB` column width. The 64-dim fixed schema is a stable contract; changing it would break backward compatibility for all databases created before v1.0.93.

## Test Coverage

- `tests/openrouter_embedding.rs` — live API tests; excluded from `ci` nextest profile; require `OPENROUTER_API_KEY`
- `src/embed/backend.rs` unit tests — verify `EmbeddingBackendChoice` parsing, display, and fallback chain
- `src/embed/openrouter.rs` unit tests — verify MRL truncation, error handling, and API key masking in logs
- Mock LLM scripts in `tests/mock-llm/` are NOT extended for OpenRouter (the REST path is not a subprocess)

## Post-Release Fixes (v1.0.93 — GAP-OR-PROPAGATION)

After the initial implementation propagated `EmbeddingBackendChoice` to 8 commands, 5 additional embedding paths were discovered that still called the old `embed_passage_with_choice()` function, silently ignoring `--embedding-backend openrouter`:

1. `enrich.rs` — `reembed_memory_vector()` called old function; fixed to use `embed_passage_with_embedding_choice()`
2. `init.rs` — dimension probe called `embed_passage_with_choice(..., None)`; fixed to propagate both backends
3. `rename_entity.rs` — entity re-embedding called old function; fixed
4. `ingest_claude.rs` — 4 call sites with `None` embedding backend; all fixed to propagate `embedding_backend`
5. `remember.rs` — chunk parallel embedding called `embed_passages_parallel_local()`; fixed to use `embed_passages_parallel_with_embedding_choice()`

Total embedding paths after fix: 13 (8 original + 5 fixed in GAP-OR-PROPAGATION).

### BUG-OR-EXIT-CODE

Three OpenRouter config validation points in `main.rs` emitted exit code 1 instead of 78 (`EX_CONFIG`). Fixed to use `ExitCode::from(78_u8)` and `emit_error_json(78, msg)`, consistent with BSD sysexits convention used throughout the project.

### E2E Recall Score Ranking (dim=64 MRL)

All 10 models validated end-to-end with `--embedding-dim 64`:

| Model | Recall Score |
|---|---|
| google/gemini-embedding-001 | 0.892 |
| google/gemini-embedding-2 | 0.868 |
| mistralai/mistral-embed-2312 | 0.832 |
| qwen/qwen3-embedding-8b | 0.814 |
| qwen/qwen3-embedding-4b | 0.754 |
| openai/text-embedding-3-small | 0.668 |
| nvidia/llama-nemotron-embed-vl-1b-v2:free | 0.662 |
| baai/bge-m3 | 0.537 |
| openai/text-embedding-3-large | 0.449 |
| perplexity/pplx-embed-v1-0.6b | 0.415 |

Key findings:
- ALL 10 models accept `dimensions: 64` natively via MRL — no Rust-side truncation needed
- OpenAI large (0.449) performs WORSE than small (0.668) at dim=64 — high-dimensional embeddings (3072) lose more information when truncated to 64 dims
- Google Gemini 001 and Mistral are the best choices for semantic search at this reduced dimensionality
