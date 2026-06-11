# ADR-0019: LLM-Only One-Shot Architecture (v1.0.76)

- Status: Accepted (2026-06-07)
- Update (v1.0.79): the `embedding-legacy` escape hatch mentioned below was removed ahead of the v1.1.0 schedule; the transition window is closed
- Deciders: Danilo Aguiar
- Scope: src/embedder.rs, src/extraction.rs, src/similarity.rs, src/storage/connection.rs, src/storage/memories.rs, src/storage/entities.rs, src/storage/chunks.rs, migrations/V002, migrations/V013, Cargo.toml

## Context

The default v1.0.74 build bundled five heavy model/extension dependencies:

- `fastembed` 5.13.4 (text-embedding + ONNX runtime)
- `ort` 2.0.0-rc.12 (ONNX runtime)
- `ndarray` 0.16 (tensor library)
- `tokenizers` 0.22 (Hugging Face tokenizer)
- `huggingface-hub` 0.4 (model downloader)
- `sqlite-vec` 0.1.9 (vec0 virtual table extension)

These produced a 39 MB release binary, required a 1.1 GB model download on first
use (or 349 MB with int8), and locked the CLI to a single embedding model
(`multilingual-e5-small`). The download blocked CI jobs and made `cargo install`
heavy.

In addition, the `daemon` mode kept the model in memory across CLI invocations
to amortize the load cost. This was a fragile design: the daemon could be killed
mid-request, the model could fail to load on a particular host, and the
separation between daemon and CLI added a stateful protocol that made
debugging difficult.

## Decision

v1.0.76 removes all of these dependencies. The default build is **LLM-only and
one-shot**:

- Embedding generation: `claude code` (Anthropic OAuth) or `codex` (OpenAI
  ChatGPT Pro OAuth), spawned per call, killed when the JSON response is
  parsed. No daemon. No ONNX runtime.
- NER: the `LlmBackend` in `src/extract/llm_backend.rs` extracts entities and
  relationships via tool-use JSON. The default build is URL regex only; the LLM
  NER runs on demand when the operator uses `--extraction-backend llm`.
- Vector search: cosine similarity is computed in pure Rust over the BLOB
  embeddings stored in `memory_embeddings`, `entity_embeddings`, and
  `chunk_embeddings` (see migration V013). The `sqlite-vec` extension is gone.

The CLI is therefore a thin orchestrator that:

1. Stores memories in SQLite + FTS5.
2. Calls the LLM headless (claude / codex) for embedding and extraction.
3. Uses FTS5 for exact-match search; uses pure-Rust cosine for similarity.

The LLM hardening flags from v1.0.69 are inherited unchanged: 7 flags for
claude (`--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}'
--dangerously-skip-permissions --output-schema ...`), 7 for codex. OAuth is
the only supported credential flow; `ANTHROPIC_API_KEY` and `OPENAI_API_KEY`
in the environment cause the spawn to ABORT with `AppError::Validation`.

## Consequences

### Positive

- Release binary drops from 39 MB to ~6 MB (rustc + rusqlite + clap only).
- `cargo install sqlite-graphrag` no longer requires C build tools, ONNX
  runtime, or any system library beyond a C compiler.
- The cold-start cost of the first `remember` is dominated by the LLM
  subprocess spawn (~1-3 s) rather than the ONNX model load (~30 s on cold
  cache).
- The CLI is now one-shot. There is no daemon to leak memory, no socket file
  to leave behind on crash, no state to inspect with `daemon --ping`.
- Operators with one of the supported LLM CLIs (`claude` or `codex`) get
  working embedding + NER without any model download.

### Negative

- Every embedding call now incurs a LLM subprocess spawn (1-3 s overhead).
  Operators who batch many `remember` calls should use the LLM-side
  batching (one prompt with N passages) — the `embed_passages_controlled`
  helper already groups chunks for this.
- The CI test environment must have an LLM CLI on PATH to exercise the
  embedding + NER paths. CI without an LLM is documented to fail
  `v1044_features` / `signal_handling_integration` / `v2_breaking_integration`
  with `embedding failed: no LLM CLI found on PATH`.
- The default build no longer has a local fallback for users who cannot
  or will not install `claude` or `codex`. The `embedding-legacy` feature
  restores the fastembed pipeline for the v1.0.76 → v1.1.0 transition
  window; it is removed in v1.1.0.

## Migration from v1.0.74 / v1.0.75

Existing databases lose their vec-table embeddings when migration V013 runs.
The new `memory_embeddings` table is empty after the migration; the next
`remember`, `edit`, or `ingest` re-embeds the memory via the LLM. Operators
with millions of pre-existing memories who want to avoid the first-call
re-embedding spike can:

1. Run `migrate --to-llm-only --keep-vec` (a future subcommand) to dump
   the vec tables to JSON.
2. After the binary upgrade, run a one-time `ingest --namespace *` to
   re-embed everything via the LLM.

The `embedding-legacy` feature is the escape hatch for operators who want
to keep the v1.0.74 model pipeline for the transition window:

```
cargo install sqlite-graphrag --features embedding-legacy --version 1.0.76
```

This is removed in v1.1.0.
