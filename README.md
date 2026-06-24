# sqlite-graphrag

[![Crates.io](https://img.shields.io/crates/v/sqlite-graphrag.svg)](https://crates.io/crates/sqlite-graphrag)
[![Docs.rs](https://docs.rs/sqlite-graphrag/badge.svg)](https://docs.rs/sqlite-graphrag)
[![CI](https://github.com/daniloaguiarbr/sqlite-graphrag/actions/workflows/ci.yml/badge.svg)](https://github.com/daniloaguiarbr/sqlite-graphrag/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](LICENSE)
[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg)](CODE_OF_CONDUCT.md)

> Persistent memory for AI agents in a single Rust binary with built-in GraphRAG.
> **Current release: v1.0.91 — Spawn CWD isolation, degree recalculation fix and 6-gap remediation.** Every build embeds through `claude -p`, `codex exec` or `opencode run` (OAuth, no MCP, no hooks). No daemon, no ONNX runtime, ~14.6 MiB binary. v1.0.91 fixes GAP-SPAWN-001 (LLM subprocesses no longer inherit `.mcp.json` from the caller's CWD — embedding works in any project without manual env vars), BUG-17 (`entities.degree` inflation via `increment_degree` replaced by `recalculate_degree`), BUG-15 (7 JSON schemas missing `"opencode"` and `"auto"` in `backend_invoked` enum), BUG-16 (`vec_degraded` field missing from `deep-research` schema), GAP-SPAWN-002 (orphan spawn dir cleanup) and BUG-14 (test assertion fix). Library consumers must pin to `=1.0.91`; see the `Stability Policy` below.

- Read this document in [Portuguese (pt-BR)](README.pt-BR.md).

- Portuguese version available at [README.pt-BR.md](README.pt-BR.md)
- Public package and repository are live on GitHub and crates.io
- Install the latest published release with `cargo install sqlite-graphrag --locked`
- Upgrade an existing install with `cargo install sqlite-graphrag --locked --force`
- Verify the active binary with `sqlite-graphrag --version`
- See [CHANGELOG.md](CHANGELOG.md) for the full release history
- Release-grade validation includes the `slow-tests` contract suites documented in `docs/TESTING.md`
- Build directly from the local checkout with `cargo install --path .`
- **Upgrading from v1.0.74 / v1.0.75?** See [docs/MIGRATION.md](docs/MIGRATION.md) for the v1.0.76 / v1.0.77 / v1.0.78 / v1.0.79 migration procedure
- **Upgrading from v1.0.79 to v1.0.80?** No database migration required; just `cargo install sqlite-graphrag --locked --force`. v1.0.80 adds the CI `semver-checks` job (informational), the Windows pre-warm steps (ADR-0033), and the panic-free third-signal exit (ADR-0034). Library consumers must pin to `=1.0.80`; see the `Stability Policy` below.
- **Upgrading from v1.0.80 / v1.0.81 to v1.0.82?** Two new migrations run automatically on first `init`/`migrate`: `V014__pending_memories` (pending `remember` checkpoint queue) and `V015__pending_embeddings` (pending embedding retry queue). After upgrading, run `codex login` once to refresh the OAuth refresh token — the 2026-06-14 incident showed that `codex exec` returning HTTP 401 `refresh_token_reused` is now caught by the new fallback chain (ADR-0040) and routed to the next backend in `--llm-backend codex,claude`. See [docs/MIGRATION.md](docs/MIGRATION.md) for the full 6-step procedure including rollback.
- **Upgrading from v1.0.82 / v1.0.83 to v1.0.85?** No database migration required; just `cargo install sqlite-graphrag --locked --force`. v1.0.84 (ADR-0042, GAP-002) added the real Claude backend split via `LlmEmbeddingBuilder` so `--llm-backend claude` invokes `claude` and never `codex`, the `backend_invoked` field in 7 JSON envelopes, the `vec_degraded_reason` field in `hybrid-search` and `recall`, the global `--dry-run-backend` flag for CI pre-flight, and `apply_env_whitelist_for_claude` for hardened providers. v1.0.85 (ADR-0043) extended `FallbackReason` from 3 to 7 variants with a `reason_code` discriminator (catches quota exhaustion, slot exhaustion, backend mismatch, dim zero, cancellation, timeout), `try_embed_query_with_deterministic_fallback` retries the alternative backend on `OAuthQuota` and sleeps 750ms on `SlotExhausted`, and `LlmEmbedding::invoke_claude` now captures 12-14 `anthropic-ratelimit-*-remaining` headers BEFORE checking the subprocess exit (G45-CR5). v1.0.85.1 (hotfix) restored the FTS5 failsafe for --llm-backend none (GAP-004, ADR-0043 hotfix). v1.0.85.2 (hotfix) made --dry-run-backend work standalone (BUG-001, ADR-0044), propagated resolved_kind from embed_via_backend so backend_invoked is populated in all 7 envelopes (BUG-002), and aligned the test mock JSON shape (BUG-003). Library consumers must pin to `=1.0.85.2`; see the `Stability Policy` below.
- **Upgrading from v1.0.85 / v1.0.86 / v1.0.87 / v1.0.88 / v1.0.89 / v1.0.90 to v1.0.91?** No database migration required; just `cargo install sqlite-graphrag --locked --force`. v1.0.91 fixes GAP-SPAWN-001 (LLM subprocesses no longer inherit `.mcp.json` — embedding works zero-config in any project), BUG-17 (`entities.degree` inflation replaced by `recalculate_degree`), BUG-15 (7 schema enums), BUG-16 (`deep-research` schema), GAP-SPAWN-002 (orphan dir cleanup) and BUG-14 (test fix). Library consumers must pin to `=1.0.91`.

```bash
cargo install sqlite-graphrag --locked --force
sqlite-graphrag --version
```


## What is it?
### sqlite-graphrag delivers durable memory for AI agents
- Stores memories, entities and relationships inside a single SQLite file under 25 MB
- **Build (v1.0.91):** LLM-only and one-shot — embeddings are generated by spawning `claude -p`, `codex exec` or `opencode run` with OAuth; no local model, no daemon, no ONNX runtime, ~14.6 MiB binary. LLM subprocesses run in an isolated temp directory (GAP-SPAWN-001) so `.mcp.json` from the caller's project is never inherited
- **Legacy build:** REMOVED in v1.0.79 — the `embedding-legacy` feature and the local fastembed/ONNX path no longer exist
- Combines FTS5 full-text search with pure-Rust cosine similarity into a hybrid Reciprocal Rank Fusion ranker
- Stores and traverses an explicit entity graph with typed edges for multi-hop recall across memories
- Preserves every edit through an immutable version history table for full audit
- Runs on Linux, macOS and Windows natively with zero external services required (default build needs `claude`, `codex` or `opencode` CLI on `PATH`)


## Why sqlite-graphrag?
### Differentiators against cloud RAG stacks
- **OAuth-only LLM flow** — no API keys ever in the environment; the spawn ABORTS if `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is set (defence in depth since v1.0.69)
- **Custom Anthropic-compatible providers (v1.0.83+)** — preserves `ANTHROPIC_AUTH_TOKEN` and `ANTHROPIC_BASE_URL` so Claude Code can route to MiniMax, OpenRouter or corporate gateways without breaking the OAuth-only mandate. Set `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` (or `--strict-env-clear`) for compliance environments that forbid credential forwarding.
- **No recurring embedding fees** — embeddings come from your existing Claude Pro / Max or ChatGPT Pro subscription
- Single-file SQLite storage replaces Docker clusters of vector databases entirely
- Graph-native retrieval beats pure vector RAG on multi-hop questions by design
- Deterministic JSON output unlocks clean orchestration by LLM agents in pipelines
- Native cross-platform binary ships without Python, Node or Docker dependencies (default build needs only `claude`, `codex` or `opencode` CLI)


## Stability Policy (G53, v1.0.80)

- The **public contract is the CLI**. The `--json` envelopes documented in `docs/schemas/*.schema.json` and the environment variables listed in `llms.txt` and `llms-full.txt` are stable across all v1.x.y releases. Consumers who depend on the CLI alone are not affected by minor or patch bumps.
- The **library API is unstable** in v1.x.y. Re-exports, public struct fields and function signatures may change in any v1.x.y release without a major version bump.
- Breaking changes to the library API ship as a **minor** bump, never patch (e.g. 1.0.79 -> 1.1.0 for a removed re-export). Patch bumps (1.0.79 -> 1.0.80) are limited to additive, non-breaking changes.
- Consumers who depend on the library API must pin to an exact version (`sqlite-graphrag = "=1.0.80"`) and review CHANGELOG.md before bumping.
- This stance is recorded in `docs/decisions/adr-0032-g53-lib-api-policy.md`.

## Superpowers for AI Agents
### First-class CLI contract for orchestration
- Every subcommand accepts `--json` producing deterministic stdout payloads
- **v1.0.76 is one-shot by default** — no background process; each embedding call spawns a fresh `claude -p`, `codex exec` or `opencode run`
- Every write is idempotent through `--name` kebab-case uniqueness constraints
- Stdin is explicit: use `--body-stdin` for body text or `--graph-stdin` for one `{body?, entities, relationships}` object; raw entity and relationship arrays use `--entities-file` and `--relationships-file`
- `remember` accepts body payloads up to `512000` bytes and up to `512` chunks
- Relationship payloads use `strength` in `[0.0, 1.0]`, mapped to `weight` in outputs
- Stderr carries tracing output under `SQLITE_GRAPHRAG_LOG_LEVEL=debug` only
- `--help` is English-first by design; use `--lang` for human-facing runtime messages, not static clap help text
- Cross-platform behavior is identical across Linux, macOS and Windows hosts


## Graph Schema
### Entity types, relation labels and edge strength
- `entity_type` accepts exactly 13 values: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- `relation` (CLI input) accepts any kebab-case or snake_case string. 12 canonical values are well-known: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`. Custom values (e.g., `implements`, `tested-by`, `blocks`) are accepted with a `tracing::warn!`. JSON output normalizes to underscores (e.g., `applies_to`).
- `strength` is a float in `[0.0, 1.0]` representing edge weight; mapped to `weight` in all read outputs
- Unlisted `entity_type` values are rejected at write time with exit code 1. Custom `relation` values are accepted since v1.0.49.
- Use `sqlite-graphrag graph --format json` to inspect the full stored graph at any time


### 27 AI agents and IDEs supported out of the box (21 catalogued + 6 community)
| Agent | Vendor | Minimum version | Integration pattern |
| --- | --- | --- | --- |
| Claude Code | Anthropic | 1.0 | Subprocess with `--json` stdout |
| Codex | OpenAI | 1.0 | Tool call wrapping `cargo run -- recall` |
| Gemini CLI | Google | 1.0 | Function call returning JSON |
| Opencode | Opencode | 1.0 | Shell tool with `hybrid-search --json` |
| OpenClaw | Community | 0.1 | Subprocess pipe into `jaq` filters |
| Paperclip | Community | 0.1 | Direct CLI invocation per message |
| VS Code Copilot | Microsoft | 1.85 | Terminal subprocess via tasks |
| Google Antigravity | Google | 1.0 | Agent tool with structured JSON |
| Windsurf | Codeium | 1.0 | Custom command registration |
| Cursor | Anysphere | 0.42 | Terminal integration or MCP wrapper |
| Zed | Zed Industries | 0.160 | Extension wrapping subprocess |
| Aider | Paul Gauthier | 0.60 | Shell command hook per turn |
| Jules | Google Labs | 1.0 | Workspace shell integration |
| Kilo Code | Community | 1.0 | Subprocess invocation |
| Roo Code | Community | 1.0 | Custom command via CLI |
| Cline | Saoud Rizwan | 3.0 | Terminal tool registered manually |
| Continue | Continue Dev | 0.9 | Context provider via shell |
| Factory | Factory AI | 1.0 | Tool call with JSON response |
| Augment Code | Augment | 1.0 | Terminal command wrapping |
| JetBrains AI Assistant | JetBrains | 2024.3 | External tool per IDE |
| OpenRouter | OpenRouter | 1.0 | Function routing through shell |
| Minimax | Minimax | 1.0 | Subprocess invocation |
| Z.ai | Z.ai | 1.0 | Subprocess invocation |
| Ollama | Ollama | 0.1 | Subprocess invocation |
| Hermes Agent | Community | 1.0 | Subprocess invocation |
| LangChain | LangChain | 0.3 | Subprocess via tool |
| LangGraph | LangChain | 0.2 | Subprocess via node |


## Quick Start
### Install and record your first memory in four commands
```bash
cargo install sqlite-graphrag --locked --force
sqlite-graphrag init
sqlite-graphrag remember --name onboarding-note --type user --description "first memory" --body "hello graphrag"
sqlite-graphrag recall "graphrag" --k 5 --json
```
> **Required flags for `remember`:** `--name`, `--type`, `--description`. Body via `--body "text"`, `--body-file <path>`, or `--body-stdin` (pipe from stdin).
> **Body limit: 500 KB (512000 bytes).** Larger inputs are rejected with exit code 6 (`limit exceeded`); split into multiple memories or trim before sending.
> **Windows users (G29):** v1.0.68 is the first release since v1.0.65 that successfully compiles via `cargo install` on Windows. If you must stay on v1.0.66 or v1.0.67, see [docs/CROSS_PLATFORM.md](./docs/CROSS_PLATFORM.md) for the manual workaround.
- **GraphRAG is enabled by default and runs automatically.** Every subcommand auto-initializes `graphrag.sqlite` in the current working directory if it does not exist. Entity/relationship extraction comes from the LLM backend (`--extraction-backend llm`, the default) or from curated graph input (`--graph-stdin`, `--entities-file`).

### Automatic extraction (`--enable-ner`)
- Pass `--enable-ner` or set `SQLITE_GRAPHRAG_ENABLE_NER=1` to activate automatic extraction on `remember` and `ingest`
- Since v1.0.79 this runs URL-regex extraction ONLY — the local GLiNER zero-shot pipeline was removed together with the `ner-legacy` feature
- `--gliner-variant`, `SQLITE_GRAPHRAG_GLINER_MODEL` and `SQLITE_GRAPHRAG_GLINER_THRESHOLD` are still accepted for compatibility but have NO effect
- Response field `extraction_method` reports `url-regex`, `regex-only`, or `none:extraction-failed`
- For high-quality entity/relationship extraction prefer `ingest --mode claude-code`/`--mode codex` (LLM-curated) or pass curated entities via `--graph-stdin`
- `--skip-extraction` is deprecated since v1.0.45 and has no effect

- **`sqlite-graphrag init` is OPTIONAL** but recommended on first use because it creates the database, applies migrations and validates that a `claude`, `codex` or `opencode` CLI is reachable on `PATH` (there is no model download since v1.0.76 — embeddings come from the LLM subprocess).
- **`graphrag.sqlite` is created in the current working directory by default** (override with `--db <path>` or `SQLITE_GRAPHRAG_DB_PATH`)
- For the local checkout, `cargo install --path .` is enough
- Re-run `sqlite-graphrag --version` after any upgrade to confirm the active binary
- After the public release, prefer `--locked` to preserve the tested MSRV dependency graph


## Version Highlights
- **v1.0.91**: Spawn CWD isolation (GAP-SPAWN-001) — LLM subprocesses run in an isolated temp directory so `.mcp.json` from the caller's project is never inherited; `entities.degree` inflation fix (BUG-17) via `recalculate_degree`; 7 JSON schema enum fixes (BUG-15); `deep-research` schema fix (BUG-16); orphan spawn dir cleanup (GAP-SPAWN-002); 877+ tests, 0 failures
- **v1.0.90**: OpenCode backend integration (GAP-OPENCODE-001/002) — third LLM backend alongside codex and claude; `--llm-backend opencode`, `--mode opencode` for ingest/enrich, `--opencode-binary/model/timeout` flags, env vars `SQLITE_GRAPHRAG_OPENCODE_*`; fallback chain extended to `codex → claude → opencode → none`; Windows compilation fix (BUG-WINDOWS-001); embedding timeout hardcode fix (BUG-TIMEOUT-HARDCODE-001); `list` pagination fix (BUG-LIST-TOTAL-COUNT-001); 24 total bug/gap fixes; 875+ tests, 0 failures
- **v1.0.85**: Five-gap remediation (ADR-0043) — `FallbackReason` extended from 3 to 7 variants (`EmbeddingFailed | SlotExhausted | OAuthQuota { backend } | BackendMismatch { requested, resolved } | DimZero | Cancelled | Timeout`) with a `reason_code` discriminator in `hybrid-search` and `recall` envelopes for granular diagnosis; `try_embed_query_with_deterministic_fallback` retries the alternative backend (codex ↔ claude) on `OAuthQuota` and sleeps 750ms on `SlotExhausted` before yielding to FTS5-pure; `LlmEmbedding::invoke_claude` captures 12-14 `anthropic-ratelimit-*-remaining` headers BEFORE checking the subprocess exit (G45-CR5 — quota exhaustion aborts the embed and triggers immediate fallback); `.github/workflows/embedder-ignore.yml` runs `#[ignore]` tests in a hermetic env (no API keys); 5 new regression tests in `tests/embedder.rs` covering GAP-003, G58, G45-CR5, G55, G56
- **v1.0.85.1 (2026-06-17, hotfix)**: recall --llm-backend none and hybrid-search --llm-backend none return exit 0 with envelope vec_degraded=true + source=fts_fallback + vec_degraded_reason=dim_zero (GAP-004, ADR-0043 hotfix).
- **v1.0.85.2 (2026-06-17, hotfix)**: --dry-run-backend works standalone without a subcommand (pub command: Option<Commands> at src/cli.rs:248); embed_via_backend returns Result<(Vec<f32>, LlmBackendKind), AppError> propagating resolved_kind (BUG-002); setup_mock_path() in tests/embedder.rs:37-77 aligned to JSON (BUG-003). 945 tests green.
- **v1.0.84**: GAP-002 real Claude backend split (ADR-0042) — `--llm-backend claude` no longer delegates to `codex` via `LlmEmbedding::detect_available`; new `embed_via_claude_local` entry point and `LlmEmbeddingBuilder` with `with_claude_builder`/`with_codex_builder`/`override_binary`/`override_model`; `backend_invoked` field in 7 JSON envelopes (`embedding status`, `remember`, `edit`, `ingest`, `recall`, `hybrid-search`, `enrich`); `vec_degraded_reason` field in `hybrid-search` and `recall`; global `--dry-run-backend` flag (ADR-0042 S6) resolves and prints backend without spawning subprocess; `apply_env_whitelist_for_claude` helper for hardened providers; `LlmBackendKind::as_str` and `FallbackReason::reason_code` for envelope canonical serialization; 5 new regression tests in `tests/embedder.rs`
- **v1.0.83**: Custom Anthropic-compatible providers (ADR-0041) — `claude_runner`, `codex_spawn` and `ingest_claude` preserve `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, and `OTEL_EXPORTER_OTLP_ENDPOINT` in the subprocess environment; enables Anthropic-compatible providers (MiniMax/api.minimax.io, OpenRouter, corporate gateways) without breaking the OAuth-only mandate; new global `--strict-env-clear` flag (`SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1`) for compliance environments that forbid credential forwarding; new helper module `src/spawn/env_whitelist.rs` consolidating the duplicated whitelist logic across three spawners; 5 new integration tests in `tests/claude_runner_env.rs` covering custom-provider propagation, OAuth-only abort, codex base-url inheritance, strict-mode credential dropping, and audit-of-no-token-leak

- **v1.0.79**: G42 closed — the LLM embedding pipeline is no longer slow, serialized or fragile. **(S1)** configurable embedding dimensionality, default 64 (`--embedding-dim`, `SQLITE_GRAPHRAG_EMBEDDING_DIM`, range [8, 4096]; precedence flag > env > `schema_meta.dim` > 64; existing 384-dim databases keep working unchanged, ZERO schema change). **(S2)** batched LLM calls (`{items:[{i,v}]}` — chunks at 8, entity names at 25 at dim 64, dim-adaptive via clamp(base×64/dim, 1, base) since G44; 39 spawns collapse into 4-5). **(S3)** real bounded parallelism via `Semaphore` + `JoinSet` with the new `--llm-parallelism` flag on `remember` (default 4), `ingest` (default 2) and `edit`; results stream through a bounded mpsc channel. **(S4)** codex schema tempfiles are RAII `NamedTempFile`s; the reaper also removes stale `codex-home-{pid}` dirs. **(S5)** `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` env override. **(S6)** empty `CLAUDE_CONFIG_DIR` by default on the embedding path (~40-50s → ~10-15s per call). **(S7)** actionable codex headless error. **(S8)** panic-free signal handler (second signal exits 130 with ZERO I/O). **(S9)** canonical re-embed: `enrich --operation re-embed` plus `edit --force-reembed`. **(C5)** `validate_dim` errors on divergent vectors instead of silently normalising. Every LLM subprocess uses `kill_on_drop` plus `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` (default 300s). Also REMOVED: the daemon infrastructure and the legacy `embedding-legacy`/`ner-legacy`/`full` features with the fastembed/ort/ndarray/tokenizers/hf-hub optional dependencies — every build is LLM-only.
- **v1.0.78**: G41 fix — `migrate --rehash` no longer inserts phantom rows for unapplied migrations (V013 was being registered without executing its SQL)
- **v1.0.77**: G40 fix — the `run_rehash` INSERT now writes `applied_on` (RFC3339); a NULL there blocked every subsequent migration
- **v1.0.76**: **Breaking architectural change** — the default build becomes LLM-only and one-shot: no daemon, no ONNX runtime, no local model download; embeddings/NER delegate to `claude -p` or `codex exec` headless (OAuth). Migration V013 drops the `vec_*` virtual tables in favour of BLOB-backed embedding tables with pure-Rust cosine similarity. New `migrate --rehash` and `migrate --to-llm-only --drop-vec-tables` upgrade paths. 7 new ADRs (0019-0025) plus ADR-0026 documenting the V002 drift root cause
- **v1.0.75**: new `ExtractionBackend` trait (G21) behind the global `--extraction-backend llm|embedding|none|both` flag; LLM-backed extraction becomes the default
- **v1.0.74**: `--skip-extraction` no-op compatibility restored (v1.0.45 promise honored) — the hard validation error introduced in v1.0.67 reverted to `tracing::warn!`
- **v1.0.73**: CI fix — `clang`/`mold`/`lld` installed inside the `cross` container for `aarch64-unknown-linux-gnu` builds
- **v1.0.72**: CI fix — mold linker installed on `ubuntu-latest` runners (12+ jobs failed with `invalid linker name in argument`)
- **v1.0.71**: CI fix — `Swatinem/rust-cache` repinned from the non-existent `v2.8` ref to `v2.9.1` across 17 call-sites
- **v1.0.70**: i18n fix — manual POSIX locale precedence `LC_ALL > LC_MESSAGES > LANG` (the cached system locale ignored runtime env vars)
- **v1.0.69**: 12 gaps closed (G28-G39) with full OAuth-only enforcement. **(OAuth-only behaviour change)** `claude -p` and `codex exec` spawns now ABORT with `AppError::Validation` if `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` are set; the `--bare` flag is REMOVED from all executable code. Operators using API keys MUST migrate to OAuth. **(G28 CRITICAL)** 4 reinforcing fixes for process proliferation: 7 flags hardening in `claude_runner::build_claude_command` (always passes `--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions`), `SIGTERM` on timeout, new `src/reaper.rs` walking `/proc` at startup, and `src/system_load.rs` plus `CircuitBreaker` integration. **(G29)** `enrich --operation body-enrich` now succeeds 100% (was 100% CHECK constraint failure), with audit trail via `memory_versions`, type-safe `MemorySource` enum, Jaccard preservation gate (10 tests, default 0.7), and `blake3` idempotency skip. **(G30)** Singleton lock scoped per `(job_type, namespace, db_hash)` with new `--wait-job-singleton` and `--force-job-singleton` flags. **(G31+G32+G33)** New `src/commands/codex_spawn.rs` (~700 lines, 11 tests) unifies spawn pipeline, JSONL parser, and ChatGPT Pro OAuth model validation; `enrich --mode codex` and `ingest --mode codex` share the same canonical command (was divergent, motivated the `~/.local/bin/codex-clean` wrapper). **(G34)** Worker warning is conditional to mode (Claude > 4, Codex > 16). **(G35)** `--preflight-check`, `--fallback-mode`, `--rate-limit-buffer` prevent batch loss on Claude rate limit. **(G36)** `optimize` pre-checks FTS5 health before rebuilding, plus new `--fts-dry-run`, `--fts-progress`, `--yes`. **(G37)** `--names <NAME>` and `--names-file <PATH>` for selective enrichment. **(G38)** Backup defaults 25x faster (1000/5ms vs 100/50ms) with 4 new tuning flags. **(G39)** New `vec orphan-list`/`vec purge-orphan`/`vec stats` subcommand family plus `forget` hook to prevent new orphans. **+53 tests** (692 → 745). 7 new ADRs (`docs/decisions/adr-0011-0017-*.md`) document every architectural decision.
- **v1.0.68**: 2 CRITICAL fixes for Windows + process proliferation.  **(G29)** `cargo install` no Windows was breaking with `error[E0308]` in `src/terminal.rs:29` because `HANDLE` in `windows-sys >= 0.59` is `*mut c_void` (was `isize` in 0.48/0.52).  Replaced with the type-safe idiom `!handle.is_null() && handle != INVALID_HANDLE_VALUE`, pinned `windows-sys` to `=0.59.0` exact, and added CI job `windows-build-check` that runs `cargo check --target x86_64-pc-windows-msvc` on every push.  **(G28-B)** Added `lock::acquire_job_singleton` per `(job_type, namespace)` so two parallel `enrich`/`ingest --mode claude-code|codex` invocations against the same database now fail fast with the new exit-75 `AppError::JobSingletonLocked { job_type, namespace }` instead of stacking 4 × N workers × 10 MCP processes (root cause of the 2026-06-03 276-load-average incident).  **(G28-A)** `claude_runner::build_claude_command` now respects `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` — when set to an empty directory, the subprocess is spawned with `CLAUDE_CONFIG_DIR=<that dir>`, suppressing user-scoped MCP servers and their 8-10-process fan-out.  Deliberately avoids `--strict-mcp-config` / `--mcp-config '{}'` because [anthropics/claude-code#10787] documents that Claude Code CLI ignores both flags.  **(G28-D)** `retry::CircuitBreaker` helper plus a `tracing::warn!` when `--llm-parallelism > 4` (combine with `CLAUDE_CONFIG_DIR` override to keep subprocess fan-out manageable).  Also fixed 3 pre-existing test failures in `src/commands/{history,list,read}.rs` that were leaking the `SQLITE_GRAPHRAG_DISPLAY_TZ` env var between parallel tests.
- **v1.0.67**: 2 NEW commands: `remember-batch` (NDJSON batch memory creation with `--transaction`/`--force-merge`), `completions` (shell completions for Bash/Zsh/Fish/PowerShell/Elvish); `read --id` for direct memory_id lookup, `enrich --llm-parallelism` for parallel LLM workers, `health` super-hub detection (degree > 50), `edit` skip-embed optimization via body_hash comparison, `rename` ghost purge for soft-deleted name conflicts, flag validation in hybrid-search/recall/ingest, V012 relationship timestamps migration, 24 gap fixes total
- **v1.0.66**: 35 BUG/GAP fixes including 3 CRITICAL (reclassify-relation crash, evidence chain flooding, link weight), `edit --type` flag, `graph_context` in deep-research, LLM-friendly aliases for graph/list JSON, full doc audit
- **v1.0.65**: 3 NEW commands: `reclassify-relation` (bulk relationship type renames with UNIQUE collision handling), `normalize-entities` (normalize entity names to kebab-case with auto-merge), `enrich` (LLM-augmented graph quality: memory-bindings, entity-descriptions, body-enrich); CRITICAL deep-research fixes: per-sub-query embeddings (was sharing one), RRF fusion for KNN+FTS5 (was hardcoded 0.5), directed evidence chains (was flat global dump); new deep-research flags `--rrf-k`, `--graph-decay`, `--graph-min-score`, `--max-neighbors-per-hop`; entity name normalization on all write paths; `health` reports relation concentration; `--max-entity-degree` warning on link/remember
- **v1.0.64**: NEW `deep-research` command for parallel multi-hop GraphRAG research via query decomposition (up to 7 sub-queries) with bounded JoinSet + Semaphore fan-out and evidence chain assembly; ingest claude-code disables hooks via `--settings` for OAuth (was failing 65% of files), detects OAuth and omits misleading `cost_usd`, validates body size BEFORE LLM extraction (files >512 KB skipped); rename/rename-entity reject same-name with exit 1
- **v1.0.63**: restore preserves current name after rename (was reverting to version's original name), ingest claude-code/codex normalizes relation strings before DB insertion, edit re-generates vector embeddings when body changes, OAuth-first auth docs
- **v1.0.62**: 10 bug fixes for ingest --mode claude-code (G01 CRITICAL: recall now works), NEW --mode codex for OpenAI Codex CLI extraction, new flags --codex-binary/--codex-model/--codex-timeout
- **v1.0.61**: 15 bug fixes for ingest --mode claude-code (B00-B13), new --claude-timeout flag, wait-timeout subprocess management
- **v1.0.60**: NEW ingest --mode claude-code for LLM-curated extraction via Claude Code CLI, queue DB for resume/retry, 7 new ingest flags
- **v1.0.59**: rename-entity name validation, unlink schema fix, reclassify `description_updated` field, contract+schema tests for rename-entity, E2E entity validation tests, doc audit (6 files)
- **v1.0.58**: FTS5 sync fix (CRITICAL: remember --force-merge was silently corrupting FTS5 index), merge-entities UNIQUE fix for memory_entities, new `rename-entity` command, entity name validation, `memory-entities --entity` reverse lookup, `reclassify --description`, purge response `action` field, fts help EXAMPLES, health tracing
- **v1.0.57**: 16 fixes — merge-entities UNIQUE constraint, memory-entities column name, --clear-body validation, WAL checkpoint for fts rebuild/check, degree recalculation for delete-entity/merge-entities adjacents, atomic backup via tempfile-rename, 18 new contract+schema tests
- **v1.0.56**: 9 new commands (fts, backup, delete-entity, reclassify, merge-entities, memory-entities, prune-ner), 7 new flags, 19 new JSON fields, FTS5 graceful degradation, JSON error envelope
- **v1.0.55**: Full doc audit — export summary `total`→`exported`, list response fields corrected, `--tz` exit code 1→2, exit 2 added to exit code table, stats legacy aliases documented
- **v1.0.54**: WAL checkpoint for `prune-relations` (last missing command), `--graph-stdin` empty body validation, `memory_type` JSON field in `list`/`export`, `Vec::with_capacity` in 9 cold paths
- **v1.0.53**: WAL checkpoint TRUNCATE after every write command for Dropbox/cloud-sync safety, `export --json` contract fix, `Vec::with_capacity` in 12 hot paths
- **v1.0.52**: 12 gaps fixed, new `export` subcommand, exit code Duplicate 2→9 (breaking), `forget` not-found no JSON (breaking)
- **v1.0.51**: Namespace env var fix (8 commands), remember on soft-deleted fix, per-chunk RSS watchdog (`--max-rss-mb`), daemon test coverage
- **v1.0.50**: `prune-relations` subcommand, daemon auto-restart on version mismatch, V011 index, 37 doc gaps fixed
- **v1.0.49**: Extensible relation vocabulary, V010 migration, 15 doc updates
- **v1.0.48**: GLiNER NER functional, 5 bug fixes, full doc audit
- **v1.0.47**: Replace BERT NER with GLiNER zero-shot, 13 custom entity types, `--gliner-variant` flag
- **v1.0.35**: Flag aliases (`--from`/`--to`, `--old`/`--new`, `--limit` as alias of `--k`)


## Memory Lifecycle
### Runnable sequence: init → remember → recall → forget → purge
```bash
# 1. Initialize (once per database)
sqlite-graphrag init

# 2. Store a memory
sqlite-graphrag remember --name my-note --type user --description "demo" --body "first entry"

# 3. Retrieve by semantic similarity
sqlite-graphrag recall "first entry" --k 5 --json

# 4. Soft-delete (reversible)
sqlite-graphrag forget my-note

# 5. Permanently remove soft-deleted memories older than 0 days
sqlite-graphrag purge --retention-days 0 --yes
```
> All five commands above are safe to run in sequence on a fresh database.


## Installation
### Minimum supported toolchain
- Rust 1.88 or newer (`rust-version = "1.88"` in `Cargo.toml`); older toolchains will fail with an MSRV error during `cargo install`.
### Multiple distribution channels
- Install the latest published release with `cargo install sqlite-graphrag --locked`
- Upgrade an existing published binary with `cargo install sqlite-graphrag --locked --force`
- Pin to a specific version with `cargo install sqlite-graphrag --version <X.Y.Z> --locked`
- Install from the local checkout with `cargo install --path .`
- Build from the local checkout with `cargo build --release`


## Usage
### Initialize the database
```bash
sqlite-graphrag init
sqlite-graphrag init --namespace project-foo
```
- Without `--db` or `SQLITE_GRAPHRAG_DB_PATH`, every CRUD command in that directory uses `./graphrag.sqlite`
### Remember a memory with an optional explicit entity graph
- By default, `remember` does NOT run automatic URL extraction (off by default)
- Pass `--enable-ner` to activate URL-regex extraction for that call, or set `SQLITE_GRAPHRAG_ENABLE_NER=1` (the GLiNER pipeline was removed in v1.0.79)
```bash
sqlite-graphrag remember \
  --name integration-tests-postgres \
  --type feedback \
  --description "prefer real Postgres over SQLite mocks" \
  --body "Integration tests must hit a real database."
```
- `remember` JSON response includes `urls_persisted` (URLs routed to `memory_urls` table) and `relationships_truncated` (bool, set when relationships were capped)
- URLs are stored in `memory_urls` via schema V007 and never pollute the entity graph
- Sample JSON output illustrating extracted entities and relationships:
```json
{
  "memory": {"id": 42, "name": "audit-note", "type": "project"},
  "extracted_entities": [
    {"name": "OpenAI", "kind": "organization", "saliency": 0.92},
    {"name": "Rust", "kind": "technology", "saliency": 0.85}
  ],
  "extracted_relationships": [
    {"source": "OpenAI", "target": "GPT-4", "relation": "develops"}
  ],
  "urls_persisted": [],
  "relationships_truncated": false
}
```
### Automatic extraction status (GLiNER removed in v1.0.79)
- The local GLiNER zero-shot NER pipeline was REMOVED in v1.0.79 with the `ner-legacy` feature; `--enable-ner` now performs URL-regex extraction only
- For LLM-curated entity/relationship extraction use `ingest --mode claude-code` or `ingest --mode codex`
- For exact control pass curated entities via `--graph-stdin`, `--entities-file` and `--relationships-file`
- The `extraction_method` field in the JSON response reports which path ran

```bash
sqlite-graphrag remember \
  --name release-notes-v1 \
  --type document \
  --description "release notes for v1.0.0" \
  --enable-ner \
  --llm-parallelism 4 \
  --body-stdin < notes.md
```
### Read, forget, edit and rename using positional name argument
<!-- skip-test: forget soft-deletes the memory mid-block, which then invalidates the subsequent edit/rename. The block is a lifecycle illustration, not a runnable script. -->
```bash
sqlite-graphrag read integration-tests-postgres --json
sqlite-graphrag forget integration-tests-postgres
sqlite-graphrag history integration-tests-postgres --json
sqlite-graphrag edit integration-tests-postgres --body "Updated body text."
sqlite-graphrag rename integration-tests-postgres --new postgres-tests
```
- Positional name is equivalent to `--name <name>` for `read`, `forget`, `history`, `edit` and `rename`

### Recall memories by semantic similarity
```bash
sqlite-graphrag recall "postgres integration tests" --k 3 --json
```
### Hybrid search combining FTS5 and vector KNN
```bash
sqlite-graphrag hybrid-search "postgres migration rollback" --k 10 --json
```
### Deep research with parallel multi-hop query decomposition (v1.0.64)
```bash
sqlite-graphrag deep-research "auth architecture decisions and incidents" --k 20 --json
```
- Decomposes the query into up to 7 sub-queries, runs them in parallel via bounded `JoinSet` + `Semaphore`, merges results with cross-query deduplication, and assembles evidence chains from graph traversal
- Defaults calibrated against NovelHopQA, StepChain, HopRAG benchmarks: `--k 20`, `--max-sub-queries 7`, `--max-hops 3`
### Inspect database health and stats
```bash
sqlite-graphrag health --json
sqlite-graphrag stats --json
```
### Purge soft-deleted memories after retention period
```bash
sqlite-graphrag purge --retention-days 90 --dry-run --json
sqlite-graphrag purge --retention-days 90 --yes
```
> **Default retention: 90 days.** To purge ALL forgotten memories regardless of age, pass `--retention-days 0`.

### Bulk-ingest every Markdown file under a directory
<!-- skip-test: requires a `./docs` directory containing Markdown files relative to the invocation cwd. -->
```bash
sqlite-graphrag ingest ./docs --type document --pattern '*.md' --recursive
```
### Bulk-ingest with low-memory mode (single worker)
<!-- skip-test: requires a `./docs` directory; demonstrates the --low-memory flag. -->
```bash
# Force single-threaded ingest to reduce RSS pressure (recommended for <4 GB RAM
# environments and container/cgroup constraints). Trade-off: 3-4x longer wall time.
sqlite-graphrag ingest ./docs --type document --pattern '*.md' --low-memory

# Or via env var (CLI flag takes precedence):
SQLITE_GRAPHRAG_LOW_MEMORY=1 sqlite-graphrag ingest ./docs --type document
```
### Bulk-ingest with LLM-curated entities via Claude Code (v1.0.61)
<!-- skip-test: requires Claude Code installed with Pro/Max subscription. -->
```bash
# Extract entities and relationships using locally installed Claude Code CLI
sqlite-graphrag ingest ./docs --mode claude-code --recursive --json

# Resume interrupted ingestion
sqlite-graphrag ingest ./docs --mode claude-code --resume --json

# Set budget limit
sqlite-graphrag ingest ./docs --mode claude-code --max-cost-usd 5.00 --json

# Extract entities and relationships using locally installed OpenAI Codex CLI
sqlite-graphrag ingest ./docs --mode codex --recursive --json
```
> **Authentication:** OAuth is the ONLY accepted credential flow. API keys are PROHIBITED.
> `--mode claude-code` reads OAuth from `~/.claude/.credentials.json` (Claude Pro/Max/Team).
> `--mode codex` reads device auth from `codex login` (OpenAI ChatGPT).
> Defining `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` in the environment ABORTS the spawn with `AppError::Validation` and exit code 1. The `--bare` flag (which would also demand an API key) is REMOVED from all executable code paths.
> See `docs/decisions/adr-0011-oauth-only-enforcement.md` for the full rationale.
> `ingest` emits NDJSON on stdout: one JSON line per file, then a summary line.
> Per-file `status` values: `indexed` (created), `skipped` (duplicate or invalid name), `failed` (error).
> Duplicates emit `status: "skipped"` with `action: "duplicate"` and do not count as failures.
> Pass `--dry-run` to preview the name mapping (kebab-cased basenames) without writing anything to the database.
> Schema: `docs/schemas/ingest-file-event.schema.json`, `docs/schemas/ingest-summary.schema.json`.

### Rename a memory while keeping its version history
<!-- skip-test: illustrative names (`old-name`, `new-name`) — the source memory does not exist in this isolated test database. -->
```bash
sqlite-graphrag rename old-name --new-name new-name --json
```
### Edit a memory body or description (creates a new version)
<!-- skip-test: depends on the memory not having been soft-deleted by an earlier example block. -->
```bash
sqlite-graphrag edit integration-tests-postgres --body "Updated body."
sqlite-graphrag edit integration-tests-postgres --description "Updated description."
```
### Restore a memory to a previous version
<!-- skip-test: `restore --version 2` requires the memory to have at least two versions, which is not the case in the isolated example database. -->
```bash
sqlite-graphrag history integration-tests-postgres --json
sqlite-graphrag restore --name integration-tests-postgres --version 2 --json
```
### Apply pending schema migrations
```bash
sqlite-graphrag migrate --status --json
sqlite-graphrag migrate --json
```
### Resolve namespace precedence for the current invocation
```bash
sqlite-graphrag namespace-detect --json
sqlite-graphrag namespace-detect --namespace project-foo --json
```
### Refresh SQLite query planner statistics
```bash
sqlite-graphrag optimize --json
```
### Reclaim disk space and checkpoint the WAL
```bash
sqlite-graphrag vacuum --json
```
### Create a typed relationship between two entities
<!-- skip-test: requires the `OpenAI` and `GPT-4` entities to already exist in the namespace. -->
```bash
sqlite-graphrag link --from "OpenAI" --to "GPT-4" --relation uses --weight 0.8 --json
```
### Remove a specific relationship between two entities
<!-- skip-test: requires the relationship created by the preceding `link` example. -->
```bash
sqlite-graphrag unlink --from "OpenAI" --to "GPT-4" --relation uses --json
```
### Traverse memories connected via the entity graph
```bash
sqlite-graphrag related onboarding-note --max-hops 2 --limit 10 --json
```
> **Empty results are normal** for memories without graph edges yet — extract entities first via `remember` or `ingest`. Edges form when ≥2 entities co-occur in the same memory body.

### Export a graph snapshot in json, dot or mermaid
<!-- skip-test: `--output graph.json` writes a file relative to the invocation cwd; pollutes the test workspace. The remaining read-only graph subcommands are exercised by the cookbook integration tests. -->
```bash
sqlite-graphrag graph --format json --output graph.json
sqlite-graphrag graph stats --json
sqlite-graphrag graph traverse --from "OpenAI" --depth 2 --json
sqlite-graphrag graph entities --entity-type organization --limit 50 --json
```
### Remove orphan entities with no memories and no relationships
```bash
sqlite-graphrag cleanup-orphans --dry-run --json
sqlite-graphrag cleanup-orphans --yes --json
```
### Bulk-delete relationships by type
<!-- skip-test: requires relationships to exist in the namespace. -->
```bash
sqlite-graphrag prune-relations --relation mentions --dry-run --show-entities --json
sqlite-graphrag prune-relations --relation mentions --yes --json
```
### Clear cached embedding/NER models from the XDG cache
<!-- skip-test: deletes the embedding model cache; safe in production but slows the integration suite by forcing a re-download on later commands. -->
```bash
sqlite-graphrag cache clear-models --yes
```
### List every version of a memory
<!-- skip-test: depends on the lifecycle state established by earlier illustrative blocks (which are themselves marked `skip-test`). -->
```bash
sqlite-graphrag history integration-tests-postgres --no-body --json
```


## Commands
### Core database lifecycle
| Command | Arguments | Description |
| --- | --- | --- |
| `init` | `--namespace <ns>` | Initialize database, apply migrations and validate that a `claude`/`codex` CLI is reachable (no model download) |
| `health` | `--json` | Show database integrity, FTS5 functional check, sqlite version, super-hub detection (degree > 50) |
| `stats` | `--json` | Count memories, entities and relationships |
| `migrate` | `--json` | Apply pending schema migrations via `refinery` |
| `vacuum` | `--json` | Checkpoint WAL and reclaim disk space |
| `optimize` | `--json`, `--skip-fts` | Run `PRAGMA optimize` and rebuild FTS5 index (skip with `--skip-fts`) |
| `backup` | `--output <path>` | Back up the database using the SQLite Online Backup API |
| `sync-safe-copy` | `--dest <path>` (alias `--output`) | Checkpoint then copy a sync-safe snapshot |
### Memory content lifecycle
| Command | Arguments | Description |
| --- | --- | --- |
| `remember` | `--name`, `--type`, `--description`, `--body` (or `--body-file`/`--body-stdin`), `--entities-file`, `--relationships-file`, `--graph-stdin`, `--llm-parallelism <N>` (default 4), `--enable-ner` (URL-regex only since v1.0.79), `--force-merge`, `--clear-body`, `--dry-run` | Save a memory with optional entity graph; `--type`/`--description` optional with `--force-merge` (inherited from existing); `--dry-run` validates without persisting |
| `remember-batch` | `--transaction`, `--force-merge`, `--fail-fast` | Batch-create memories from NDJSON stdin; one invocation, one slot, one DB connection |
| `recall` | `<query>`, `-k`/`--k` (alias `--limit`), `--type`, `--max-hops`, `--max-distance`, `--all-namespaces`, `--no-graph` | Search memories semantically via KNN + graph traversal |
| `read` | `[name]` or `--name <name>`, `--id <N>`, `--with-graph` | Fetch a memory by exact name or integer memory_id; `--with-graph` includes linked entities and relationships |
| `list` | `--type`, `--limit`, `--offset`, `--include-deleted` | Paginate memories sorted by `updated_at`; default limit is all with `--json`, 50 for text; response includes `total_count`, `truncated`, `body_length` |
| `forget` | `[name]` or `--name <name>` | Soft-delete a memory preserving history |
| `rename` | `[old]`, or `--name`/`--old`/`--from <NAME>`, `--new-name`/`--new`/`--to <NAME>` | Rename a memory while keeping versions |
| `edit` | `[name]` or `--name`, `--body`, `--description`, `--type`, `--force-reembed`, `--llm-parallelism <N>` | Edit body, description or memory type creating new version; skips re-embedding when body content is unchanged; `--force-reembed` (v1.0.79) regenerates the embedding without changing the body |
| `history` | `[name]` or `--name <name>`, `--diff` | List all versions of a memory; `--diff` includes character-level change summary |
| `memory-entities` | `[name]` or `--name <name>`, `--entity <name>` | List entities linked to a memory, or memories linked to an entity (reverse lookup via `--entity`) |
| `restore` | `--name`, `--version` | Restore a memory to a previous version |
| `ingest` | `<DIR>`, `--type`, `--pattern <GLOB>` (default `*.md`), `--recursive`, `--mode` (`none`/`claude-code`/`codex`; `gliner` accepted but URL-regex only since v1.0.79), `--ingest-parallelism N`, `--llm-parallelism N` (default 2, embedding workers), `--low-memory`, `--enable-ner` (URL-regex only since v1.0.79), `--fail-fast`, `--dry-run`, `--claude-binary`, `--claude-model`, `--resume`, `--retry-failed`, `--max-cost-usd`, `--claude-timeout`, `--rate-limit-wait`, `--keep-queue`, `--queue-db` | Bulk-ingest every matching file as a separate memory (NDJSON output); `--mode claude-code` uses locally installed Claude Code CLI for LLM-curated entity/relationship extraction; `--dry-run` previews name mapping without writing; `--claude-timeout` sets per-file subprocess timeout (default 300s) |
| `export` | `--namespace`, `--type`, `--include-deleted`, `--limit`, `--offset` | Export memories as NDJSON for backup or migration |
| `cache clear-models` | `--yes` | Remove model files cached by versions ≤ v1.0.75 from the XDG cache directory (no build downloads models since v1.0.76) |

> **Memory name validation.** Names must match `[a-z0-9-]+` (kebab-case, ASCII only).
> Unicode and uppercase are rejected with exit code 1. Names longer than 60 chars
> emitted by `ingest` are truncated to fit; review the WARN log to spot mangled names.
### Retrieval and graph
| Command | Arguments | Description |
| --- | --- | --- |
| `hybrid-search` | `<query>`, `--k`, `--rrf-k`, `--with-graph`, `--max-hops`, `--min-weight`, `--weight-vec`, `--weight-fts` | FTS5 plus vector fused via Reciprocal Rank Fusion; graceful degradation when FTS5 is corrupted (`fts_degraded`, auto-rebuild); `normalized_score` for cross-method comparability |
| `namespace-detect` | `--namespace <name>` | Resolve namespace precedence for invocation |
| `link` | `--from`, `--to`, `--relation`, `--weight`, `--create-missing`, `--entity-type`, `--strict-relations` | Create a relationship; `--strict-relations` rejects non-canonical types; warnings in JSON for non-canonical |
| `unlink` | `--from`, `--to`, `--relation`, `--entity`, `--all` | Remove relationships; `--relation` now optional (removes all between pair); `--entity X --all` removes all edges of entity |
| `related` | `--name`, `--limit`, `--hops` | Traverse graph-connected memories from a seed memory |
| `graph` | `--format`, `--output` | Export a graph snapshot in `json`, `dot` or `mermaid` |

> **Breaking change in v1.0.44.** `graph entities` JSON output renamed top-level array
> from `items` to `entities`. Update jaq/jq filters: `.items[]` becomes `.entities[]`.
> The `list` command still uses `items`.

### Graph subcommands
| Subcommand | Description | Key flags |
| --- | --- | --- |
| `graph traverse --from <ENTITY>` | Walk the entity graph from a starting node using BFS | `--depth` (default 2), `--namespace` |
| `graph stats` | Print graph statistics (node count, edge count, degree distribution) | `--namespace` |
| `graph entities` | List entities with degree count and sorting | `--limit` (default 50), `--entity-type`, `--namespace`, `--sort-by degree\|name\|created_at`, `--order asc\|desc` |

### Maintenance
| Command | Arguments | Description |
| --- | --- | --- |
| `purge` | `--retention-days <n>`, `--dry-run`, `--yes` | Permanently delete soft-deleted memories |
| `cleanup-orphans` | `--namespace`, `--dry-run`, `--yes` | Remove entities that have no memories and no relationships |
| `prune-relations` | `--relation <type>`, `--namespace`, `--dry-run`, `--yes`, `--show-entities` | Bulk-delete all relationships of a given type; `--show-entities` lists affected entities in the dry-run preview |
| `delete-entity` | `--name <entity>`, `--cascade` | Delete an entity and cascade-remove all its relationships and bindings |
| `rename-entity` | `--name <entity>`, `--new-name <name>` | Rename an entity preserving all relationships and memory bindings; re-embeds vector |
| `reclassify` | `--name <entity> --new-type <type>`, `--description <text>`, or `--from-type <old> --to-type <new> --batch` | Reclassify entity types individually or in bulk; `--description` updates entity description in single mode |
| `merge-entities` | `--names <a,b,c> --into <target>` | Merge source entities into target, moving all edges |
| `prune-ner` | `--entity <name>` or `--all`, `--dry-run`, `--yes` | Remove NER bindings from memory_entities table |
| `fts rebuild` | `--json` | Rebuild the FTS5 full-text search index from scratch |
| `fts check` | `--json` | Run FTS5 integrity-check without modifying the index |
| `fts stats` | `--json` | Show FTS5 index statistics (row count, shadow pages) |
| `completions` | `bash`, `zsh`, `fish`, `powershell`, `elvish` | Generate shell completions for the specified shell |
| `enrich` | `--operation <op>` (memory-bindings, entity-descriptions, body-enrich, re-embed, weight-calibrate, relation-reclassify, entity-connect, entity-type-validate, description-enrich, cross-domain-bridges, domain-classify, graph-audit, deep-research-synth, body-extract), `--mode <claude-code\|codex>`, `--llm-parallelism <N>`, `--preserve-threshold <FLOAT>`, `--preflight-check`, `--fallback-mode <mode>`, `--rate-limit-buffer <SECONDS>`, `--names <NAMES>`, `--names-file <PATH>`, `--max-load-check`, `--circuit-breaker-threshold <N>`, `--codex-model-validate`, `--codex-model-fallback <MODEL>`, `--resume`, `--retry-failed`, `--max-cost-usd <USD>`, `--claude-binary/--claude-model/--claude-timeout`, `--codex-binary/--codex-model/--codex-timeout`, `--db <DB>`, `--wait-job-singleton <SECONDS>`, `--force-job-singleton` | LLM-augmented graph quality pipeline (G29 + G35 + G37); three fully implemented operations and 11 scan-only operations; OAuth-only via `--mode claude-code` (Anthropic) or `--mode codex` (ChatGPT Pro) |
| `vec orphan-list` | `--json` | List orphan memory embedding rows (G39) with `vector_hash` for traceability |
| `vec purge-orphan` | `--yes`, `--dry-run`, `--json` | Delete orphan memory embedding rows from `vec_memories`, `vec_entities`, `vec_chunks` (G39); `--yes` required as safety guard |
| `vec stats` | `--json` | Show statistics for `vec_memories`, `vec_entities`, `vec_chunks` tables (G39) |
| `codex-models` | `--json`, `--suggest <substring>` | List the ChatGPT Pro OAuth accepted-model whitelist (G33) or return the closest match via substring + Levenshtein |
| `remember-batch` | `--json`, `--transaction`, `--force-merge`, `--fail-fast` | Batch-create memories from NDJSON stdin (one invocation, one slot, one DB connection) |
| `namespace-detect` | `--json`, `--namespace <name>` | Resolve namespace precedence for the current invocation |
| `deep-research` | `<query>`, `--k`, `--max-sub-queries`, `--max-hops`, `--min-weight`, `--max-results`, `--with-bodies`, `--max-concurrency`, `--timeout`, `--rrf-k`, `--graph-decay`, `--graph-min-score`, `--max-neighbors-per-hop`, `--json` | Parallel multi-hop GraphRAG research via query decomposition; returns `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context?`, `stats` |

### v1.0.82 / v1.0.85 subcommands (no new subcommands added in v1.0.83/84/85; new fields and flags only)
| Command | Arguments | Description |
| --- | --- | --- |
| `pending` | `list`, `show <id>`, `cleanup`, `--filter-status queued\|processing\|done\|failed`, `--limit`, `--json` | Inspect and process the three-stage `remember` checkpoint queue (GAP-001, ADR-0036); `cleanup` removes terminal-state rows |
| `pending-embeddings` | `list`, `process`, `--filter-status queued\|processing\|done\|failed\|skipped`, `--limit`, `--json` | Inspect and process the embedding retry queue (GAP-005, ADR-0040); `process` retries failed embeddings with the next backend in `--llm-backend` |
| `slots` | `status`, `release --slot-id <N> --yes`, `--json` | Cross-process LLM slot semaphore inspection and cleanup (GAP-004, ADR-0039); `status` returns `max_concurrency`, `acquired`, `waiting`, `held_by_pid[]`, `p50_wait_ms`, `p99_wait_ms`; `release` reaps orphan slots from dead PIDs |
| `embedding` | `status`, `list`, `--filter-status queued\|processing\|done\|failed\|skipped`, `--limit`, `--json` | Health and per-entry inspection of the pending-embeddings queue (GAP-005) |

### v1.0.82 / v1.0.85 global flags
| Flag | Applies to | Description |
| --- | --- | --- |
| `--llm-backend <codex\|claude\|none,codex,...>` | `remember`, `edit`, `ingest`, `enrich` | Comma-separated backend chain tried in order; first non-error wins (ADR-0038, ADR-0040) |
| `--llm-fallback-mode <claude\|codex>` | `remember`, `edit`, `enrich` | Swap backend on rate-limit; requires `--llm-backend` chain with at least 2 entries |
| `--llm-max-host-concurrency <N>` | All LLM-spawning commands | Cap concurrent LLM subprocesses host-wide via `fs4` flock (ADR-0039); default derived from CPU and OAuth tier |
| `--llm-slot-wait-secs <N>` | All LLM-spawning commands | Seconds to wait for a free slot before failing (default 30s); pair with `--llm-slot-no-wait` for fail-fast |
| `--strict-env-clear` | `remember`, `edit`, `ingest`, `enrich`, `embedding`, `pending-embeddings` | Drop ALL credential env vars from the subprocess; preserve only `PATH` for binary resolution. Honours env `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` (ADR-0041, v1.0.83) |
| `--dry-run-backend` | Top-level global flag | Resolve and print the resolved LLM backend (binary path, model, flavour, chain) WITHOUT spawning the subprocess. Honour env `SQLITE_GRAPHRAG_DRY_RUN_BACKEND=1` (ADR-0042 S6, v1.0.84). Use for CI pre-flight audit; exit 0 indicates successful resolution |

### v1.0.82 / v1.0.85 exit codes
| Code | Meaning | Emitted by |
| --- | --- | --- |
| `19` | Shutdown signal received; partial work discarded; see `shutdown-envelope.schema.json` for stdout envelope | Any LLM-spawning command on SIGTERM/SIGINT/SIGHUP (ADR-0037) |

### `cache` subcommands
| Subcommand | Description |
| --- | --- |
| `clear-models` | Remove cached embedding/NER model files (forces re-download on next `init`) |


## Environment Variables
### Runtime configuration overrides
| Variable | Description | Default | Example |
| --- | --- | --- | --- |
| `SQLITE_GRAPHRAG_DB_PATH` | Path to the SQLite database file override | `./graphrag.sqlite` in the invocation directory | `/data/graphrag.sqlite` |
| `SQLITE_GRAPHRAG_HOME` | Override base directory for `graphrag.sqlite` (used when `--db` and `SQLITE_GRAPHRAG_DB_PATH` are absent) | unset | `/var/lib/sqlite-graphrag` |
| `SQLITE_GRAPHRAG_CACHE_DIR` | Directory override for model cache and lock files | XDG cache dir | `~/.cache/sqlite-graphrag` |
| `SQLITE_GRAPHRAG_LANG` | CLI output language as `en` or `pt` (aliases: `pt-BR`, `portuguese`) | `en` | `pt` |
| `SQLITE_GRAPHRAG_LOG_LEVEL` | Tracing filter level for stderr output | `info` | `debug` |
| `SQLITE_GRAPHRAG_LOG_FORMAT` | Tracing output format on stderr (`pretty` or `json`) | `pretty` | `json` |
| `SQLITE_GRAPHRAG_NAMESPACE` | Namespace override bypassing detection | none | `project-foo` |
| `SQLITE_GRAPHRAG_DISPLAY_TZ` | IANA timezone for `*_iso` JSON fields | `UTC` | `America/Sao_Paulo` |
| `SQLITE_GRAPHRAG_EMBEDDING_DIM` | Embedding dimensionality override (v1.0.79); precedence: `--embedding-dim` flag > this env > `schema_meta.dim` > 64; range [8, 4096] | `64` (new databases) | `384` |
| `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` | Model override for `claude -p` embedding calls (v1.0.79, symmetric to the codex variable) | CLI default model | `claude-haiku-4-5-20251001` |
| `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` | Timeout per LLM embedding subprocess call (v1.0.79) | `300` | `600` |
| `SQLITE_GRAPHRAG_ENABLE_NER` | Enable automatic extraction on `remember`/`ingest`. Since v1.0.79 this runs URL-regex extraction only (the GLiNER pipeline was removed). Accepts `1`/`true`/`yes`/`on` | unset (off) | `1` |
| `SQLITE_GRAPHRAG_GLINER_VARIANT` | NO EFFECT since v1.0.79 (GLiNER removed) — accepted for compatibility, ignored | — | — |
| `SQLITE_GRAPHRAG_GLINER_THRESHOLD` | NO EFFECT since v1.0.79 (GLiNER removed) — accepted for compatibility, ignored | — | — |
| `SQLITE_GRAPHRAG_GLINER_MODEL` | NO EFFECT since v1.0.79 (GLiNER removed) — accepted for compatibility, ignored | — | — |
| `SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS` | Token budget for entity/relationship extraction per memory; values outside [512, 100 000] fall back to default | `5000` | `8000` |
| `SQLITE_GRAPHRAG_MAX_ENTITIES_PER_MEMORY` | Maximum distinct entities persisted per memory; values outside [1, 1 000] fall back to default. Note: the extraction pipeline internally caps candidates at 30 before deduplication, so the persistence cap (default 50) acts as a safety ceiling and is only reached when the extractor is extended or replaced. | `50` | `100` |
| `SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY` | Maximum distinct relationships persisted per memory; values outside [1, 10 000] fall back to default | `50` | `200` |
| `SQLITE_GRAPHRAG_LOW_MEMORY` | Force single-threaded ingest to reduce RSS. Accepts `1`/`true`/`yes`/`on` (case-insensitive) | unset (multi-thread) | `1` |
| `SQLITE_GRAPHRAG_CLAUDE_BINARY` | Explicit path to the Claude Code binary; affects ALL LLM commands (`recall`, `hybrid-search`, `remember`, `edit`, `ingest --mode claude-code`, `enrich`, `deep-research`). v1.0.89: now propagated from `--claude-binary` CLI flag | PATH lookup | `/usr/local/bin/claude` |
| `SQLITE_GRAPHRAG_CODEX_BINARY` | Explicit path to the Codex CLI binary; affects ALL LLM commands (`recall`, `hybrid-search`, `remember`, `edit`, `ingest --mode codex`, `enrich`, `deep-research`). v1.0.89: new flag `--codex-binary` | PATH lookup | `/usr/local/bin/codex` |
| `SQLITE_GRAPHRAG_SKIP_EMBEDDING_ON_FAILURE` | When set, commands persist memories with NULL embedding instead of aborting with exit 11 on LLM failure. Use `enrich --operation re-embed` to backfill later. Accepts `1`/`true`/`yes`/`on` (v1.0.89) | unset (abort on failure) | `1` |
| `SQLITE_GRAPHRAG_LLM_MODEL` | Default model for embedding LLM calls; overridden by backend-specific vars (`SQLITE_GRAPHRAG_CODEX_EMBED_MODEL`, `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL`). Maps to `--llm-model` CLI flag (v1.0.89) | `gpt-5.5` (codex) / `claude-sonnet-4-6` (claude) | `gpt-5.4` |
| `SQLITE_GRAPHRAG_LLM_FALLBACK` | Comma-separated fallback chain for `--llm-backend auto`. Tokens: `codex`, `claude`, `none`. Maps to `--llm-fallback` CLI flag (v1.0.89) | `codex,claude,none` | `claude,none` |
| `SQLITE_GRAPHRAG_LLM_MAX_HOST_CONCURRENCY` | Maximum concurrent LLM subprocesses host-wide. Maps to `--llm-max-host-concurrency` CLI flag (v1.0.89) | `4` | `8` |
| `SQLITE_GRAPHRAG_LLM_SLOT_NO_WAIT` | When set, abort immediately instead of waiting for an LLM slot. Accepts `1`/`true`/`yes`/`on`. Maps to `--llm-slot-no-wait` CLI flag (v1.0.89) | unset (wait) | `1` |
| `ORT_DYLIB_PATH` | HISTORICAL (≤ v1.0.75) — no build loads ONNX since v1.0.76; the variable is ignored | — | — |


## Integration Patterns
### Compose with Unix pipelines and tools
```bash
sqlite-graphrag recall "auth tests" --k 5 --json | jaq -r '.results[].name'
```
### Feed hybrid search into a summarizer endpoint
```bash
sqlite-graphrag hybrid-search "postgres migration" --k 10 --json \
  | jaq -c '.results[] | {name, combined_score}' \
  | xh POST http://localhost:8080/summarize
```
### Backup with atomic snapshot and compression
```bash
sqlite-graphrag sync-safe-copy --dest /tmp/ng.sqlite
ouch compress /tmp/ng.sqlite /tmp/ng-$(date +%Y%m%d).tar.zst
```
### Claude Code subprocess example in Node
```javascript
const { spawn } = require('child_process');
const proc = spawn('sqlite-graphrag', ['recall', query, '--k', '5', '--json']);
```
### Docker Debian build for CI pipelines
```dockerfile
FROM rust:1.88-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo install --path .
```


## Exit Codes
### Deterministic status codes for orchestration
| Code | Meaning | Possible Cause |
| --- | --- | --- |
| `0` | Success | Command completed and JSON payload printed when requested |
| `1` | Validation error or runtime failure | Invalid `--type`, malformed `--relation` (empty or non-snake_case), kebab-case violation, generic anyhow error |
| `2` | CLI usage error | Invalid flag, missing required argument, invalid `--tz` timezone (Clap `FromStr` rejects before app code) |
| `9` | Duplicate detected | Existing `--name` without `--force-merge`; `ingest` skips the file and emits `status: "skipped"` with `action: "duplicate"` instead |
| `3` | Conflict during optimistic update | `edit` or `restore` raced against another writer |
| `4` | Memory or entity not found | `read`, `forget`, `edit`, `rename`, `restore` or `graph traverse` target missing |
| `5` | Namespace could not be resolved | No `SQLITE_GRAPHRAG_NAMESPACE`, no flag, no detected default |
| `6` | Payload exceeded configured limits | `--name` longer than 80 bytes, body over `512000` bytes, more than `512` chunks |
| `10` | SQLite database error | Corrupted file, schema mismatch, missing migration |
| `11` | Embedding generation failed | LLM subprocess error or model load failure |
| `12` | `sqlite-vec` extension failed to load | Missing native extension or unsupported SQLite build |
| `13` | Batch partial failure | `import`, `reindex` or stdin batch with at least one failing record |
| `14` | Filesystem I/O error | Cache or database directory not writable, nonexistent `ingest` target directory |
| `15` | Database busy after retries | WAL contention exceeded `with_busy_retry` budget |
| `20` | Internal or JSON serialization error | Unexpected serde failure or invariant violation |
| `75` | `EX_TEMPFAIL` lock timeout or all concurrency slots busy | Five-plus concurrent invocations or `flock` waited longer than 300s |
| `77` | Available RAM below minimum required | Less than 2 GB free RAM detected before model load |


## Performance
### Measured on a 1000-memory database
- Embedding latency is dominated by the headless LLM round-trip (~1-3 s per batched call); pure reads (`read`, `list`, `graph`) stay in the low milliseconds
- Since v1.0.79 LLM calls are BATCHED (calibration bases of 8 chunks / 25 entity names at dim 64, dim-adaptive — G44) and PARALLEL (`--llm-parallelism`, bounded `Semaphore` + `JoinSet`), so a 39-item memory embeds in 4-5 calls instead of 39 serialized spawns
- `--embedding-dim 64` (the default) cuts the LLM output per vector ~6x compared to the old 384-dim payload
- `init` performs no model download — it only creates the database and validates that a `claude`/`codex` CLI is reachable
- **Build (v1.0.79):** each embedding call spawns `claude -p` or `codex exec` — RSS is ~350 MB per LLM worker (the 1100 MB ONNX model load no longer exists in any build)


## Memory Requirements
### Sizing RAM for ingest and recall workloads
- The CLI itself is lightweight (~14.6 MiB binary); RAM is dominated by the LLM subprocesses at roughly 350 MB RSS per worker (`LLM_WORKER_RSS_MB`)
- Worker budget: effective parallelism is `min(--llm-parallelism, cpus, free_ram × 0.5 / 350 MB, 32)` — the concurrency gate adapts to available memory automatically
- Default parallelism increases RSS roughly linearly per worker (`--llm-parallelism 4` ≈ 4 × 350 MB of subprocess RSS on top of the CLI)
- Low-memory mode: pass `--low-memory` (or set `SQLITE_GRAPHRAG_LOW_MEMORY=1`) to force single-threaded ingest. Equivalent to `--ingest-parallelism 1` and overrides any explicit value, at the cost of 3-4x wall time.
- Container/cgroup users: budget `MemoryMax` for the CLI plus N × 350 MB LLM workers (the old 3 GB ONNX floor no longer exists)


## Storage Footprint
### Expected DB size relative to ingested content
> **Expected overhead: roughly 8× the total ingested body size** (e.g., 7.6 MB of text → ~62.9 MB DB).
> Overhead comes from float embeddings (default 64-dim since v1.0.79; pre-existing databases keep their recorded dimensionality, e.g. 384), FTS5 full-text index, and the entities/relationships graph.
> Run `sqlite-graphrag vacuum --json` after bulk `forget`+`purge` cycles to reclaim reclaimed space.


## Safe Parallel Invocation
### Counting semaphore with up to four simultaneous slots
- Each LLM embedding worker (`claude -p`/`codex exec` subprocess) consumes roughly 350 MB of RSS — the budget unit used by the concurrency gate since v1.0.79
- `MAX_CONCURRENT_CLI_INSTANCES` remains the hard ceiling at 4 cooperating subprocesses
- Heavy commands `init`, `remember`, `recall`, and `hybrid-search` are clamped lower dynamically when available RAM cannot sustain the requested parallelism safely
- Lock files live at `~/.cache/sqlite-graphrag/cli-slot-{1..4}.lock` using `flock`
- A fifth concurrent invocation waits up to 300 seconds then exits with code 75
- Use `--max-concurrency N` to request the slot limit for the current invocation; heavy commands may still be reduced automatically
- Memory guard aborts with exit 77 when less than 2 GB of RAM is available
- SIGINT and SIGTERM trigger graceful shutdown via `shutdown_requested()` atomic
- Exit code 130 when interrupted by SIGINT (Ctrl+C)
- Exit code 141 when SIGPIPE fires (stdout closed by downstream consumer in pipeline)
- Exit code 143 when terminated by SIGTERM
- Second signal forces immediate exit without waiting for current operation


## Troubleshooting FAQ
### Cloud sync safety (Dropbox, iCloud, OneDrive)
- sqlite-graphrag uses WAL mode by default for high-concurrency writes
- Since v1.0.54, every write command runs `PRAGMA wal_checkpoint(TRUNCATE)` after committing (v1.0.53 covered 11 of 12; v1.0.54 added the missing `prune-relations`)
- This ensures the `.sqlite` file is always self-contained when cloud sync tools read it
- If corruption occurs despite the checkpoint, recover with `sqlite3 broken.sqlite ".recover" | sqlite3 repaired.sqlite`

### Common issues and fixes
- Default behavior always creates or opens `graphrag.sqlite` in the current working directory
- Database locked after crash requires `sqlite-graphrag vacuum` to checkpoint the WAL
- `init` is near-instant since v1.0.76 — there is no model download; if it fails, check that a `claude` or `codex` CLI is reachable on `PATH`
- Embedding calls failing with exit 11 usually mean the LLM CLI is missing, unauthenticated (OAuth required) or timing out — raise `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` (default 300) for slow links
- `ORT_DYLIB_PATH`/`libonnxruntime.so` guidance is HISTORICAL (≤ v1.0.75) — no build loads ONNX since v1.0.76
- Permission denied on Linux means the cache directory lacks write access for your user
- Namespace detection falls back to `global` when no explicit override is present
- Parallel invocations that exceed the effective safe limit receive exit 75 and SHOULD retry with backoff; during audits start heavy commands with `--max-concurrency 1`


## Compatible Rust Crates
### Invoke sqlite-graphrag from any Rust AI framework via subprocess
- Each crate calls the binary through `std::process::Command` with `--json` flag
- No shared memory or FFI required: the contract is pure stdout JSON
- Pin the binary version in your `Cargo.toml` workspace for reproducible builds
- All 18 crates below work identically on Linux, Apple Silicon macOS and Windows

### rig-core
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "project goals", "--k", "5", "--json"])
    .output().unwrap();
```

### swarms-rs
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "agent memory", "--k", "10", "--json"])
    .output().unwrap();
```

### autoagents
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "task-context", "--type", "project",
           "--description", "current sprint goal", "--body", "finish auth module"])
    .output().unwrap();
```

### graphbit
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "decision log", "--k", "3", "--json"])
    .output().unwrap();
```

### agentai
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "previous decisions", "--k", "5", "--json"])
    .output().unwrap();
```

### llm-agent-runtime
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "user preferences", "--k", "5", "--json"])
    .output().unwrap();
```

### anda
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["stats", "--json"])
    .output().unwrap();
```

### adk-rust
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "tool outputs", "--k", "5", "--json"])
    .output().unwrap();
```

### rs-graph-llm
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "graph relations", "--k", "10", "--json"])
    .output().unwrap();
```

### genai
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "model context", "--k", "5", "--json"])
    .output().unwrap();
```

### liter-llm
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "session-notes", "--type", "user",
           "--description", "session recap", "--body", "discussed architecture"])
    .output().unwrap();
```

### llm-cascade
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "fallback context", "--k", "3", "--json"])
    .output().unwrap();
```

### async-openai
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "system prompt history", "--k", "5", "--json"])
    .output().unwrap();
```

### async-llm
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "chat context", "--k", "5", "--json"])
    .output().unwrap();
```

### anthropic-sdk
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "tool use patterns", "--k", "5", "--json"])
    .output().unwrap();
```

### ollama-rs
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "local model outputs", "--k", "5", "--json"])
    .output().unwrap();
```

### mistral-rs
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "inference context", "--k", "10", "--json"])
    .output().unwrap();
```

### llama-cpp-rs
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "llama session context", "--k", "5", "--json"])
    .output().unwrap();
```


## Contributing
### Pull requests are welcome
- Read the contribution guidelines in [CONTRIBUTING.md](CONTRIBUTING.md)
- Open issues at the GitHub repository for bugs or feature requests
- Follow the code of conduct described in [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)


## Security
### Responsible disclosure policy
- Security reports follow the policy described in [SECURITY.md](SECURITY.md)
- Contact the maintainer privately before disclosing vulnerabilities publicly


## JSON Schemas
### Canonical contracts for every subcommand response
- Authoritative JSON Schemas for every `--json` response live under [`docs/schemas/`](docs/schemas/) and are versioned alongside the crate
- 64 schemas cover `init`, `remember`, `remember-batch` (+ summary), `recall`, `hybrid-search`, `deep-research`, `list`, `read`, `forget`, `purge`, `rename`, `edit`, `history`, `restore`, `link`, `unlink`, `prune-relations`, `health`, `stats`, `migrate` (+ `migrate-rehash` + `migrate-to-llm-only`), `vacuum`, `optimize`, `cleanup-orphans`, `sync-safe-copy`, `backup`, `graph` (+ stats/traverse/entities), `related`, `namespace-detect`, `debug-schema`, `entities-input`, `relationships-input`, `ingest-file-event` (+ `ingest-summary`), `ingest-claude-phase` (+ file-event + summary), `export-memory-line` (+ summary), `enrich-phase` (+ item-event + summary), `fts rebuild` (+ `fts check` + `fts stats`), `vec orphan-list` (+ `vec purge-orphan` + `vec stats`), `codex-models`, `error-envelope`
- Treat these schemas as the agent contract; SKILL.md documents the same shapes in human-readable form
- Validate downstream consumers with any standard JSON Schema validator (e.g. `ajv`, `jsonschema`)


## Changelog
### Release history tracked separately
- [PRD](docs/PRD.md) — Product Requirements Document (source of truth for the 31 behavioral contracts)
- Read the full release history in [CHANGELOG.md](CHANGELOG.md)


## Acknowledgments
### Built on top of excellent open source
- `fastembed` and `sqlite-vec` powered the local embedding pipeline up to v1.0.75 (removed since — embeddings now come from `claude`/`codex` subprocesses)
- `refinery` runs schema migrations with transactional safety guarantees
- `clap` powers the CLI argument parsing with derive macros
- `rusqlite` wraps SQLite with safe Rust bindings and bundled build


## License
### Dual license MIT OR Apache-2.0
- Licensed under either of Apache License 2.0 or MIT License at your option
- See `LICENSE-APACHE` and `LICENSE-MIT` in the repository root for full text
