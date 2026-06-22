# ADR-0051: OpenCode Backend Integration (v1.0.90)

## Status
- Accepted (2026-06-22)

## Context
- The sqlite-graphrag CLI supported only `codex` and `claude` as LLM backends
- `src/spawn/opencode_adapter.rs` existed since v1.0.75 (G22) but was never connected to embedding, ingest, or enrich pipelines
- The factory pattern in `llm_backend.rs` was designed for extensibility but only had Codex/Claude/None implementations
- OpenCode CLI v1.17.7 provides free models (deepseek-v4-flash-free, mimo-v2.5-free, nemotron-3-ultra-free, north-mini-code-free, big-pickle)

## Decision
- Add OpenCode as a third LLM backend across all 3 pipelines: embedding, ingest, enrich
- Auto-detect priority: codex (1st) > claude (2nd) > opencode (3rd) > none (4th)
- Zero hardcode: binary path and model resolved via env var or CLI flag
- No OAuth enforcement for opencode (uses its own auth system)

## Interface
- Command: `opencode run --format json -m <provider/model> --dangerously-skip-permissions "<prompt>"`
- Output: NDJSON with 3 event types (step_start, text, step_finish)
- Response text in `.part.text` of `type=="text"` events
- No `--output-schema` equivalent: structured output via prompt + JSON parsing

## Consequences
- 6 enums expanded with `Opencode` variant (EmbeddingFlavour, LlmBackendKind, LlmBackendKindFactory, LlmBackendChoice, IngestMode, EnrichMode)
- 4 new files created (opencode_runner.rs, ingest_opencode.rs, mock-opencode, this ADR)
- 12 existing files modified
- 874 tests passing (up from 854)
- Fallback chain extended: `[Codex, Claude, Opencode, None]`

## Limitations
- OpenCode has no structured output flag (--output-schema / --json-schema)
- JSON enforcement relies on role-setting prompt ("You are an embedding function") + robust parsing (Strategy 3 in parse_llm_json)
- Parser extracts JSON from markdown fences, brace-matching, and direct parse as fallback strategies
- Prompt is passed as positional argument (argv limit ~128KB on Linux)

## v1.0.90 Audit Fixes
- Embedding prompt rewritten: role-setting ("You are an embedding function") produces real 64-dim vectors; prior generic prompt caused models to refuse
- Model cross-contamination: `opencode_embed_model()` and `resolve_opencode_model()` do NOT fall back to `SQLITE_GRAPHRAG_LLM_MODEL` — that var may contain codex/claude models (e.g. "gpt-5.4-mini") that opencode cannot resolve (ProviderModelNotFoundError)
- env propagation: `propagate_opencode_env()` forwards OPENCODE_*, OPENROUTER_*, XDG_*, LANG, TERM, USER, LOGNAME, TMPDIR into subprocess after env_clear()
- ingest pipeline: `run_opencode_ingest()` now executes full per-file extraction loop with entity/relationship persistence (was a stub returning Err)

## Env Vars
- `SQLITE_GRAPHRAG_OPENCODE_BINARY` — binary path override
- `SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL` — embedding model
- `SQLITE_GRAPHRAG_OPENCODE_MODEL` — extraction/enrichment model
- `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT` — timeout in seconds (default: 300)

## CLI Flags
- `--opencode-binary <PATH>` (global)
- `--llm-backend opencode` (global)
- `--mode opencode` (ingest, enrich)
- `--opencode-model <MODEL>` (ingest, enrich)
- `--opencode-timeout <SECONDS>` (ingest, enrich)
