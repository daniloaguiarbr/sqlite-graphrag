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
- [decisions/](decisions/) for the 25 ADRs
