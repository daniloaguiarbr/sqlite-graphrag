Read this document in [Portuguese (pt-BR)](CONTRIBUTING.pt-BR.md).


# Contributing to sqlite-graphrag


## Welcome
- Thank you for considering a contribution: every pull request strengthens local GraphRAG memory
- Your improvements directly affect developers using LLMs with durable memory in a single SQLite file
- Code, documentation, tests, bug reports, and ideas are equally valued contributions
- This guide keeps your onboarding under 10 minutes from clone to first local test run


## Quick Start
- Use this repository normally; the public `sqlite-graphrag` repository already exists
- The same validation commands apply locally and in the public repository workflow
- No command should print errors on a clean checkout of `main`
```bash
timeout 120 cargo check --all-targets
timeout 300 cargo nextest run --profile ci
RUSTDOCFLAGS="-D warnings" timeout 120 cargo doc --no-deps --all-features
```


## Development Setup
### Toolchain requirements
- MSRV is Rust 1.88 declared in `rust-version` inside `Cargo.toml`
- JAMAIS bump MSRV without opening an RFC-style issue for discussion first
- Install Rust via `rustup` and pin the toolchain with `rustup default 1.88.0` when reproducing CI
### Dependency pinning
- Direct pin `constant_time_eq = "=0.4.2"` protects MSRV 1.88 from transitive drift via `blake3`
- JAMAIS run `cargo update` indiscriminately; always open a PR explaining the version bump
- Lockfile `Cargo.lock` MUST be committed because this repository ships a binary CLI
### Runtime requirements
- SQLite 3.40 or newer is required at runtime due to `sqlite-vec` and FTS5 external-content
- On Linux you may need `libssl-dev` and `pkg-config` for some transitive dev dependencies


## Branching Strategy
- Branch `main` is protected and requires a passing CI pipeline for merge
- Feature branches SHOULD use the prefix `feature/<short-kebab-case-description>`
- Bug fix branches SHOULD use the prefix `fix/<short-kebab-case-description>`
- Documentation-only branches SHOULD use the prefix `docs/<short-kebab-case-description>`
- Maintenance branches SHOULD use the prefix `chore/<short-kebab-case-description>`


## Commit Convention
- Follow the Conventional Commits 1.0.0 specification for every commit message on shared branches
- Use `feat` for new user-visible features
- Use `fix` for bug fixes landing on main
- Use `perf` for performance improvements without user-visible behavior changes
- Use `refactor` for code restructuring that neither adds features nor fixes bugs
- Use `docs` for documentation-only changes
- Use `chore` for tooling, CI, or repository maintenance
- Use `test` for adding or improving tests
- Use `ci` for CI pipeline changes
- JAMAIS add `Co-authored-by` for AI agents in commit messages: this is enforced by CI


## Pull Request Process
### Before opening the PR
- Rebase onto the latest `main` and resolve conflicts locally
- Keep the PR scope focused on a single logical change when possible
- Write a PR description explaining the motivation, the change, and any trade-offs
### PR Validation Checklist
- [ ] `cargo check --all-targets` passes with zero errors
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [ ] `cargo fmt --all --check` passes with zero diffs
- [ ] `cargo doc --no-deps --all-features` with `RUSTDOCFLAGS="-D warnings"` runs clean
- [ ] `cargo nextest run --profile ci` runs the standard suite to success
- [ ] `cargo llvm-cov nextest --profile heavy --features slow-tests --summary-only` keeps coverage at or above the 80 percent minimum
- [ ] `cargo audit` reports zero vulnerabilities
- [ ] `cargo deny check advisories licenses bans sources` passes with zero violations


## Testing
- Run the standard suite with `cargo nextest run --profile ci` for the fast CI-aligned runner
- Run the slow suite separately with `cargo nextest run --profile heavy --features slow-tests`
- Measure full-audit coverage with `cargo llvm-cov nextest --profile heavy --features slow-tests --summary-only`
- Keep the full-audit coverage floor at or above 80 percent
- Unit tests live inside `#[cfg(test)] mod tests` blocks within the implementation file
- Integration tests live under `tests/` and SHOULD use `assert_cmd` plus `wiremock` for HTTP mocks
- A hidden flag `--skip-memory-guard` exists exclusively for tests that do not perform real allocation
- Treat `init`, `remember`, `recall`, and `hybrid-search` as heavy-memory commands during manual validation
- Start heavy-command validation with `--max-concurrency 1` and scale only after measuring RSS and swap behavior
- JAMAIS issue real HTTP requests or touch real filesystem paths outside a `TempDir` in tests
- Run `cargo test --lib lock::tests retry::circuit_breaker_tests` after touching `lock.rs` or `retry.rs` to exercise the new v1.0.68 singleton and circuit-breaker helpers
- Run `cargo test --test terminal_compile_windows` after touching `src/terminal.rs` to confirm the public surface stays callable; the dedicated CI job `windows-build-check` runs the full cross-platform type check
- Test assertions involving timestamps MUST be timezone-agnostic — parse ISO via `chrono::DateTime::parse_from_rfc3339` and compare `timestamp()` against `DateTime::UNIX_EPOCH` instead of hardcoded `1970-01-01T00:00:00` strings; this rule was added after a `SQLITE_GRAPHRAG_DISPLAY_TZ` leak in v1.0.66/v1.0.67 made three pre-existing tests flaky

### v1.0.76 Test Matrix (3 features)
- The CI matrix runs `clippy` and `test` jobs across `default` and `llm-only` features (the `embedding-legacy` leg was removed in v1.0.79 together with the feature)
- The `default` and `llm-only` jobs install a stub `mock-llm` CLI on `PATH` so embedding round-trip tests can run without real OAuth credentials

- New code that touches `src/extract/llm_embedding.rs` MUST be exercised via the mock LLM contract in `tests/fixtures/mock-llm/`
- New code that depends on the daemon MUST NOT depend on daemon autostart; the daemon is deprecated and will be removed in v1.1.0
- New code that introduces a new migration version MUST round-trip through `migrate --rehash` and `migrate --to-llm-only` integration tests to validate the SipHasher13 checksum rewrite path


## Documentation
- Every public API MUST have `///` doc comments with at least one testable example when reasonable
- Run `cargo doc --no-deps --all-features` with `RUSTDOCFLAGS="-D warnings"` locally before pushing
- Documentation formatting rules are defined in `docs_rules/rules_rust_documentacao.md`
- Bilingual README, CONTRIBUTING, SECURITY, and CODE_OF_CONDUCT MUST stay synchronized across EN and pt-BR
- When adding or modifying CLI commands, update documentation in BOTH English and Portuguese files (e.g., `README.md` and `README.pt-BR.md`, `docs/HOW_TO_USE.md` and `docs/HOW_TO_USE.pt-BR.md`)
- Update the CHANGELOG under the Unreleased section for every user-visible change


## How to Report Bugs
- Open an issue using the Bug Report template on GitHub
- Include a minimal reproduction case, ideally under 20 lines of invocation or code
- Include the output of `cargo --version` and `rustc --version`
- Include your OS, architecture, SQLite version, and sqlite-graphrag version
- Include the exact command you ran, the observed output, and the expected output


## How to Request Features
- Open an issue using the Feature Request template on GitHub
- Describe the concrete use case and who benefits; avoid abstract wish-list framing
- Describe at least one alternative you considered and why it did not fit
- Reference any upstream PRD section or related issue when applicable


## Release Process
- Maintainers bump `version` in `Cargo.toml` following Semantic Versioning 2.0.0
- Maintainers update the CHANGELOG moving Unreleased entries under the new version with ISO date
- Maintainers tag the release commit as `vX.Y.Z` using `git tag -a vX.Y.Z -m "Release vX.Y.Z"`
- Pushing the tag triggers `.github/workflows/release.yml` which builds release artifacts and GitHub release assets
- Final publication to crates.io is done manually with `cargo publish --locked`

## Recent Releases
### v1.0.76 - 2026-06-07 — LLM-Only One-Shot, OAuth-Only Embedding
- **BREAKING ARCHITECTURAL CHANGE**: the default build no longer bundles any local model. All embedding generation, NER, and vector search delegate to `claude -p` or `codex exec` headless (OAuth, no MCP, no hooks). The CLI is one-shot. Binary drops from 39 MB to ~6 MB.
- **Removed crates**: `fastembed 5.13.4`, `ort 2.0.0-rc.12`, `ndarray 0.16`, `tokenizers 0.22`, `huggingface-hub 0.4`, `sqlite-vec 0.1.9`
- **Removed features**: `daemon` (as a performance optimization, kept for source compatibility until v1.1.0), `--enable-ner` GLiNER ONNX path (moved to `ner-legacy` feature)
- **Added**: `ExtractionBackend` trait with `LlmBackend` / `EmbeddingBackend` / `NoneBackend` / `CompositeBackend`; `VersionAdapter` trait with `CodexAdapter` / `ClaudeAdapter` / `OpencodeAdapter`; `migrate --rehash` and `migrate --to-llm-only --drop-vec-tables`; BLOB-backed `memory_embeddings` / `entity_embeddings` / `chunk_embeddings` tables; pure-Rust cosine in `src/similarity.rs`; OAuth-only LLM credential flow with `AppError::Validation` abort on `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` in env
- **Migration V013** drops the `vec_memories` / `vec_entities` / `vec_chunks` virtual tables; old embeddings are recomputed lazily on next write
- **CI matrix**: `default` and `llm-only` since v1.0.79 (`embedding-legacy` removed); mock LLM CLI wired into 26 test files; 107/115 previously-slow tests fixed
- **7 new ADRs**: `adr-0019-llm-only-one-shot`, `adr-0020-pure-rust-cosine`, `adr-0021-deprecate-daemon`, `adr-0022-blob-embeddings`, `adr-0023-remove-tokenizers`, `adr-0024-fts5-coarse-cosine-refine`, `adr-0025-oauth-only-embedding`; all with PT-BR translations
- **2 new JSON schemas**: `migrate-rehash.schema.json`, `migrate-to-llm-only.schema.json`
- **3 new docs**: `docs/HOW_TO_USE.md`, `docs/MIGRATION.md`, `docs/AGENTS.md` (and PT-BR) for the v1.0.76 LLM-Only architecture
- **1 new doc**: `docs/HEADLESS_INVOCATION.md` (and PT-BR) covering Claude/Codex/OpenCode OAuth-safe headless invocation
- 745 lib tests pass, 0 fail, 3 ignored; `cargo clippy --all-targets --all-features -- -D warnings` zero warnings
- See `gaps.md` for the full resolution history and `CHANGELOG.md` for the v1.0.76 entry

### v1.0.68 - 2026-06-03 — Process Lifecycle Governance and Windows Compile Fix
- **G28-A** MCP server isolation via `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` (subprocess receives `CLAUDE_CONFIG_DIR=<empty dir>`; `--strict-mcp-config` and `--mcp-config '{}'` are ignored upstream per anthropics/claude-code#10787)
- **G28-B** `lock::acquire_job_singleton(JobType, namespace, wait_seconds)` plus `AppError::JobSingletonLocked { job_type, namespace }` (exit 75) integrated into `enrich`, `ingest --mode claude-code`, and `ingest --mode codex` to prevent process proliferation against the same database
- **G28-D** `retry::CircuitBreaker` helper with `AttemptOutcome::{Success, Transient, HardFailure}`; rate-limited and timeout errors are explicitly excluded from the failure count; `enrich` emits a `tracing::warn!` when `--llm-parallelism > 4`
- **G29** `src/terminal.rs` rewritten with `!handle.is_null() && handle != INVALID_HANDLE_VALUE` so `cargo install sqlite-graphrag` succeeds on Windows; `windows-sys` pinned to `=0.59.0` exact; new CI job `windows-build-check` runs `cargo check --target x86_64-pc-windows-msvc --lib --all-features` on every push
- **Test Fixes** three pre-existing timezone-leak failures in `src/commands/{history,list,read}.rs` fixed via `chrono::DateTime::parse_from_rfc3339` + `DateTime::UNIX_EPOCH` comparison
- **Documentation** new ADRs `adr-008-process-lifecycle-singleton`, `adr-009-windows-sys-handle-pinning`, `adr-010-mcp-isolation-claude-config-dir`; `SKILL.md` EN+PT, `AGENTS.md` EN+PT, `llms.txt`, `llms.pt-BR.txt`, `llms-full.txt`, `INTEGRATIONS.md` EN+PT, `MIGRATION.md` EN+PT, `TESTING.md` EN+PT, `HOW_TO_USE.md` EN+PT, `CROSS_PLATFORM.md` EN+PT, `COOKBOOK.md` EN+PT updated with the v1.0.68 section; `docs/schemas/error-envelope.schema.json` updated to document the second `code: 75` template
- **CI** new `windows-build-check` job; `language-check` job retained from prior release
- 692 lib tests + 2 integration tests pass; 0 warnings under `clippy -- -D warnings` and `cargo doc --no-deps --all-features` with `RUSTDOCFLAGS="-D warnings"`
- See `gaps.md` for the full resolution history and `CHANGELOG.md` for the v1.0.68 entry

## Mandatory Pre-Push Checklist (since v1.0.68)
- [ ] `cargo fmt --all --check` is clean
- [ ] `cargo check --all-targets` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` reports zero warnings
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features` reports zero warnings
- [ ] `cargo test --lib` reports 692 passed, 0 failed
- [ ] `cargo test --test terminal_compile_windows` reports 2 passed
- [ ] PR title is in English and follows Conventional Commits (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`, `ci:`, `build:`, `perf:`)
- [ ] No `Co-authored-by: ...` trailer for any AI agent (Claude, Codex, GPT, Copilot, Cursor, Gemini, Anthropic, OpenAI)
- [ ] CHANGELOG entries added under `[Unreleased]` in BOTH `CHANGELOG.md` and `CHANGELOG.pt-BR.md`
- [ ] If touching `windows-sys` or any FFI crate, run `cargo check --target x86_64-pc-windows-msvc --lib --all-features` locally
- [ ] If touching `lock.rs` or `retry.rs`, run `cargo test --lib lock::tests retry::circuit_breaker_tests`


## Recognition
- Contributors are credited in the CHANGELOG next to the version that shipped their change
- Contributors are also listed in each GitHub Release note when the contribution was user-visible
- JAMAIS add `Co-authored-by` trailers for AI agents in any commit or PR description


## Questions
- Open a GitHub Discussion for design questions or broader topics not tied to a specific issue
- Use Security Advisories for anything that resembles a security issue; see SECURITY.md
