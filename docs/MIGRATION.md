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

## Step 1 — Install the v1.0.76 Binary

```bash
cargo install sqlite-graphrag --version 1.0.76 --force
```

This installs the LLM-only default build (~6 MB binary, no
ONNX runtime, no model download). If you want the legacy
fastembed pipeline for the transition window:

```bash
cargo install sqlite-graphrag --version 1.0.76 --features embedding-legacy --force
```

The `embedding-legacy` feature is REMOVED in v1.1.0.

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

If you have a large corpus and want to avoid the first-call
re-embedding spike, you can pre-warm the embeddings:

```bash
# List all memory names in the namespace
sqlite-graphrag list --namespace myproject --json | jaq -r '.items[].name' | \
  xargs -I {} sqlite-graphrag edit --name {} --description "rewarm embedding"
```

This re-embeds every memory via the LLM. The `edit` command
triggers a re-embedding even when only the description changes;
see the `--description` flag for the idempotent path.

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
| `sqlite-graphrag daemon` keeps the embedding model in memory | `sqlite-graphrag daemon` is deprecated; the daemon's embedding request now spawns the LLM per call (no speedup) |
| `--enable-ner` triggers the GLiNER ONNX loader (~30s cold start, 1.1 GB model download) | `--enable-ner` triggers URL regex only. Use `--extraction-backend llm` to get full NER via the LLM. |
| `vec_memories`, `vec_entities`, `vec_chunks` are sqlite-vec virtual tables | `memory_embeddings`, `entity_embeddings`, `chunk_embeddings` are regular BLOB-backed tables |
| Fastembed model: `multilingual-e5-small` (local, deterministic) | LLM model: `claude-sonnet-4-6` (claude) or `gpt-5.4` (codex) (network round-trip) |
| First `init` downloads 1.1 GB of ONNX weights | First `init` does a 1-3 s LLM round-trip |

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
| `daemon` (as a performance optimization) | v1.0.76 default, REMOVED in v1.1.0 | None — the LLM subprocess is the new "model loader" |
| `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` env vars | v1.0.69 (still enforced) | OAuth via `claude login` / `codex login` |
