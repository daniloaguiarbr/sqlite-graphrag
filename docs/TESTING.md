# Testing Guide


- Read the Portuguese version at [TESTING.pt-BR.md](TESTING.pt-BR.md)


## v1.0.76 Test Infrastructure — 3-Feature CI Matrix
- The CI workflow now runs `clippy` and `test` jobs across a 3-feature matrix: `default`, `llm-only`, and `embedding-legacy`.
- The `default` and `llm-only` jobs install a stub `mock-llm` CLI on `PATH` so the embedding round-trip tests can run without a real LLM subscription.
- The `embedding-legacy` job keeps the v1.0.74 ONNX model cache path for the fastembed pipeline tests.
- 26 test files were wired to consume the mock LLM CLI as a drop-in replacement for `claude -p` and `codex exec`. This unblocks CI from requiring real OAuth credentials.
- 107 of 115 previously-slow tests were fixed in commit `bd0a3f5` (mock LLM unblocks tests that depended on a real OAuth turn).
- See the GitHub Actions workflow file `.github/workflows/ci.yml` for the matrix definition.

### Mock LLM CLI Contract
- The mock LLM is a small binary in `tests/fixtures/mock-llm/` that returns deterministic JSON for any prompt.
- For embedding requests: returns a 384-dim `f32` array (zeros with a small bias to ensure cosine distance is computable).
- For entity extraction requests: returns a fixed `{entities: [], relationships: []}` JSON object.
- Operators running tests locally must prepend the mock to `PATH`:
  ```bash
  export PATH="$PWD/target/debug:$PATH"
  cargo test --workspace
  ```

### Feature-Flag Test Selection
- `cargo test --lib` — runs against default features (mock LLM in CI, real LLM required locally).
- `cargo test --lib --no-default-features --features llm-only` — same behavior as default, explicit opt-in.
- `cargo test --lib --no-default-features --features embedding-legacy` — uses fastembed ONNX, no LLM CLI needed.
- `cargo test --workspace --features slow-tests` — runs the full contract suite including the 832-test integration matrix.


## v1.0.77 Test Additions — G40 Fix Coverage
### Test Count Delta
- v1.0.76 baseline: 719 lib tests passing
- v1.0.77 final: 723 lib tests passing (+4 unit, +2 integration)
### Unit Tests in `src/commands/migrate.rs`
- `sanitize_null_applied_on_fixes_null_rows` — verifies NULL `applied_on` rows get fixed
- `sanitize_null_applied_on_noop_when_all_filled` — verifies no-op when no NULLs exist
- `rehash_insert_includes_applied_on` — verifies INSERT now includes `applied_on`
- `remove_vec_tables_noop_when_no_vec` — verifies no-op when no vec tables exist
### Integration Tests in `tests/schema_migration_integration.rs`
- `migrate_rehash_fixes_null_applied_on` — end-to-end rehash with NULL fix
- `migrate_to_llm_only_fixes_null_applied_on` — end-to-end `--to-llm-only` with NULL fix
### Coverage Rationale
- G40 fixed a bug where `applied_on` was NULL after rehash
- The 4 unit tests cover each code path in the migrate module
- The 2 integration tests validate the CLI end-to-end flow


## Why Categorized Tests
### The Thermal Livelock Incident — 2026-04-19
- On 2026-04-19 at 11:37:40, the developer's Intel i9-14900KF reached Tjmax 100°C
- VRM temperature hit 99°C and the system required a hard reset after 3 minutes 11 seconds
- Root cause: `tests/loom_lock_slots.rs` ran without a `#[cfg(sqlite_graphrag_loom)]` gate
- The loom scheduler is CPU-intensive by design — it explores all thread permutations
- Running loom models without isolation causes thermal runaway on high-core-count CPUs
- This was the third incident in seven days caused by the same unguarded test file
- EVERY loom test MUST be gated with `#[cfg(sqlite_graphrag_loom)]` and serialized with `#[serial(loom_model)]`
- NEVER run loom tests inside the default `cargo nextest run` invocation


## Test Categories
### Unit Tests — Inline with Source
- Location: `#[cfg(test)] mod tests` blocks inside each `src/` module
- Run with: `/usr/bin/timeout 300 cargo nextest run --profile default`
- Scope: pure functions, error variants, masking, parsing, validation
- Isolation: no I/O, no filesystem, no HTTP calls
- Gate: always compiled, always run in the default profile
### Integration Tests — Separate Files
- Location: `tests/` directory
- Run with: `/usr/bin/timeout 300 cargo nextest run --profile default`
- Scope: CLI subcommands, JSON schema contracts, PRD compliance, storage CRUD
- Isolation: `TempDir` per test, `env_clear()`, wiremock for HTTP
- Gate: always compiled, always run in the default profile
### Loom Concurrency Tests — Explicit Opt-in Only
- Location: `tests/loom_lock_slots.rs`
- Run with: `/usr/bin/timeout 3900 bash scripts/test-loom.sh` or the CI `loom` job
- Scope: lock-slot semaphore permutation testing
- Isolation: MUST NOT run in parallel with any other test — one model at a time
- Gate: `#[cfg(sqlite_graphrag_loom)]` required on EVERY test function and import block
- Thermal risk: unguarded loom tests triggered system freeze on 2026-04-19
### Slow End-to-End and Stress Tests — Opt-in via Feature Flag
- Location: `tests/` files guarded by `#[cfg(feature = "slow-tests")]`
- Run with: `/usr/bin/timeout 1800 cargo nextest run --profile heavy --features slow-tests`
- Scope: long-running end-to-end smoke suites, contract suites, i18n parity, exit-code routing, high-concurrency load, and extended retry loops
- Gate: excluded from the default and `ci` nextest profiles
- Critical release suites: `/usr/bin/timeout 1200 cargo test --features slow-tests --test doc_contract_integration -- --nocapture`
- Critical release suites: `/usr/bin/timeout 1200 cargo test --features slow-tests --test prd_compliance -- --nocapture`
- CI runs those two contract suites in a dedicated `slow-contracts` job on `ubuntu-latest`
### Benchmarks — Criterion
- Location: `benches/`
- Run with: `/usr/bin/timeout 1800 cargo bench` or `/usr/bin/timeout 1800 cargo criterion`
- Scope: latency baselines for remember, recall, hybrid-search, stats, graph
- Gate: never included in `cargo nextest run`
### Claude Code Ingest Tests
- Unit tests in `src/commands/ingest_claude.rs` cover: JSON parsing, structured_output fallback, error handling, rate limit detection, entity type validation, schema conformance
- 9 unit tests protect extraction parsing invariants without requiring the Claude Code binary
- Integration tests require Claude Code >= 2.1.0 installed locally — run manually, not in CI
- Test names follow `test_parse_claude_output_*` and `test_extraction_schema_*` conventions
### Codex Ingest Tests (v1.0.62)
- 7 unit tests protect Codex JSONL parser in `src/commands/ingest_codex.rs`
- Tests cover: valid extraction, turn.failed errors, rate limit detection, schema validation, binary discovery
- Parser validates "last agent_message wins" pattern for multiple item.completed events
- Integration tests require Codex CLI installed; skip gracefully if unavailable
### v1.0.63 Regression Tests
- 3 integration tests in `tests/v1063_features.rs` protect the v1.0.63 fixes
- `restore_preserves_name_after_rename`: remember → edit → rename → restore; asserts name stays renamed
- `restore_does_not_crash_when_old_name_occupied`: remember A → rename to B → remember new A → restore B; asserts exit 0 (was exit 10 UNIQUE crash before fix)
- `edit_reembeds_when_body_changes`: remember → edit body → recall new content; asserts recall finds the edited memory with accurate score
### v1.0.64 Regression Tests
- 14 unit tests in `src/commands/deep_research.rs` protect query decomposition, bounded concurrency, dedup, evidence chain assembly, and edge cases
- Unit tests in `src/commands/ingest_claude.rs` cover terminal_reason parsing, OAuth detection via apiKeySource, and body size pre-validation
- Unit tests in `src/commands/rename.rs` and `src/commands/rename_entity.rs` cover same-name rejection with exit 1

### v1.0.68 Regression Tests
#### Windows HANDLE Type Fix (G29)
- `tests/terminal_compile_windows.rs` is a new integration test that runs on every platform: confirms `terminal::init_console` and `should_use_ansi` stay callable from outside the crate
- On Windows, the test additionally references the type-safe `HANDLE.is_null() + INVALID_HANDLE_VALUE` check; if the type contract regresses, `cargo check --target x86_64-pc-windows-msvc` in the `windows-build-check` CI job fails before this test is reached
- The new CI job is the canonical regression check; the integration test is the local pre-publish sanity probe
#### Job Singleton (G28-B)
- Three unit tests in `src/lock.rs::tests`: `job_singleton_path_sanitises_namespace` (verifies kebab-case slug from arbitrary input), `job_singleton_blocks_second_invocation_same_namespace` (verifies `AppError::JobSingletonLocked` on second acquire), `job_singleton_allows_different_namespaces` (verifies per-namespace isolation)
- Run via `cargo test --lib lock::tests` (no `#[serial]` because the per-namespace unique IDs in each test isolate them from shared-state interference)
#### Circuit Breaker (G28-D)
- Three unit tests in `src/retry.rs::circuit_breaker_tests`: `opens_after_threshold_consecutive_hard_failures`, `ignores_transient_errors`, `success_resets_consecutive_failures`.  These validate the AttemptOutcome classification that distinguishes `AppError::RateLimited` and `AppError::Timeout` (Transient) from `AppError::Validation` and `AppError::Conflict` (HardFailure)
#### Timezone Pre-Existing Fixes
- Three pre-existing test failures were fixed in `src/commands/{history,list,read}.rs`: tests now parse the ISO string via `chrono::DateTime::parse_from_rfc3339` and compare `timestamp()` against `DateTime::UNIX_EPOCH` instead of asserting the hardcoded `1970-01-01T00:00:00` prefix.  This makes the assertions timezone-agnostic so the test suite is green regardless of `SQLITE_GRAPHRAG_DISPLAY_TZ` env var setting

### v1.0.67 New Command Tests
- `remember-batch` tests in `src/commands/remember_batch.rs`: serialization tests for BatchItemEvent and BatchSummary
- `completions` command: tested via `cargo run -- completions bash` smoke test
- `read --id` integration: tested via `read --id <memory_id> --json` round-trip
- `health` super-hub detection: tested with production database (1059 memories, 3 super-hubs detected)
- `edit` skip-embed: tested via body_hash comparison (idempotent edit skips embedding)
- `rename` ghost purge: tested via forget → rename workflow
- Flag validation: tested via `hybrid-search --max-hops 2` (without `--with-graph`) expecting exit 1

### v1.0.65 New Command Tests
#### Deep Research Tests
- Unit tests in `src/commands/deep_research.rs` cover decompose_query splitting, single-query passthrough, bounded concurrency semaphore, result deduplication, evidence chain assembly (depth >= 2 filter), and empty-query validation
- Contract test `contract_36_deep_research` in `tests/doc_contract_integration.rs`: seeds two memories, runs `deep-research "auth and deploy" --max-sub-queries 2 --k 5`, asserts required keys (`query`, `sub_queries`, `results`, `evidence_chains`, `stats`) and validates `sub_queries[].source` enum
- Schema test `schema_36_deep_research` in `tests/schema_contract_strict.rs`: validates the full response against `docs/schemas/deep-research.schema.json` (Draft 2020-12, `additionalProperties: false`)
#### reclassify-relation Tests
- 8 unit tests in `src/commands/reclassify_relation.rs` cover serialization, dry-run action, merged-duplicates counting, zero-match case, and same-value rejection guard
- Contract test `contract_37_reclassify_relation`: links two entities via `mentions`, runs `reclassify-relation --from-relation mentions --to-relation related --batch --dry-run`, asserts all 7 required keys and `action == "dry_run"`
- Schema test `schema_37_reclassify_relation`: validates against `docs/schemas/reclassify-relation.schema.json`
#### normalize-entities Tests
- 5 unit tests in `src/commands/normalize_entities.rs` cover dry-run count, in-place rename, collision merge, serialization, and dry-run action field
- Contract test `contract_38_normalize_entities`: seeds a memory, runs `normalize-entities --dry-run`, asserts 5 required keys and `action == "dry_run"`
- Schema test `schema_38_normalize_entities`: validates against `docs/schemas/normalize-entities.schema.json`
#### enrich Tests
- Contract test `contract_39_enrich`: seeds a memory, runs `enrich --operation memory-bindings --dry-run`, parses NDJSON lines, asserts validate phase event, scan phase event, preview item events (status=`preview`), and summary line with all required keys
- Schema test `schema_39_enrich`: validates each NDJSON line type against its respective schema (`enrich-phase.schema.json`, `enrich-item-event.schema.json`, `enrich-summary.schema.json`)
- All enrich tests use `--dry-run` to avoid spawning the LLM binary


## How to Run
### Default — Local Development
- Run all unit and integration tests: `/usr/bin/timeout 300 cargo nextest run --profile default`
- Run with output on failure: `/usr/bin/timeout 300 cargo nextest run --profile default --no-capture`
- Run a specific test by name: `/usr/bin/timeout 300 cargo nextest run --profile default test_name_fragment`
- Run a specific file: `/usr/bin/timeout 300 cargo nextest run --profile default -E 'test(schema_contract)'`
### CI — Constrained Parallelism
- Run all tests as CI would: `/usr/bin/timeout 600 cargo nextest run --profile ci`
- The `ci` profile sets `test-threads = 2` and `RUST_TEST_THREADS=2`
- The `ci` profile enables retries on flaky tests
- The workflow also runs `doc_contract_integration` and `prd_compliance` separately with `--features slow-tests`
### Heavy — Stress and Slow Tests
- Run stress and slow tests: `/usr/bin/timeout 1800 cargo nextest run --profile heavy --features slow-tests`
- The `heavy` profile sets `test-threads = 1` for maximum isolation
- NEVER run the `heavy` profile on a thermally throttled machine
- For release validation, prefer the explicit contract commands above before broader heavy runs


## Safe Remember Audit
### Reproduce Installed-Binary Behavior Under cgroup Limits
- Use `/usr/bin/timeout 3900 bash scripts/audit-remember-safely.sh <corpus-dir>` to audit `remember` safely against a real corpus
- The script defaults to the installed `sqlite-graphrag` in `PATH`
- Override the binary with `BIN=./target/debug/sqlite-graphrag` to compare local changes against the published build
- The script uses `systemd-run --user --scope -p MemoryMax=4G -p MemorySwapMax=0`
- The script initializes an isolated temp database for each run
- The CLI is one-shot (no daemon); each embedding call spawns and discards the LLM subprocess
- The script runs known pass, threshold, fail, and synthetic cases


## Loom Concurrency Tests
### How Loom Works
- Loom runs each test many times permuting thread interleavings
- It uses state reduction to avoid combinatorial explosion
- Each model must complete under a bounded preemption count
- CPU usage is extremely high — one core saturates completely per model
- NEVER run loom tests alongside other tests on the same process
### Running Loom Tests Locally
- Use the canonical script: `/usr/bin/timeout 3900 bash scripts/test-loom.sh`
- The script sets `RUSTFLAGS="--cfg sqlite_graphrag_loom"` and `RUST_TEST_THREADS=1`
- The script sets `LOOM_MAX_PREEMPTIONS=1` for bounded local iteration
- Run in release mode only: `--release` is mandatory for acceptable speed
- Monitor CPU temperature before and during the run
### Running Individual Loom Tests
- Build first: `/usr/bin/timeout 600 env RUSTFLAGS="--cfg sqlite_graphrag_loom" cargo build --release --tests`
- Run single model: `/usr/bin/timeout 3600 env RUSTFLAGS="--cfg sqlite_graphrag_loom" RUST_TEST_THREADS=1 cargo nextest run --release -E 'test(lock_slot)'`
- Set lower preemption bound for local iteration: `LOOM_MAX_PREEMPTIONS=1`
- Increase bounds manually only for focused debugging runs
### Checkpoint and Resume
- Set `LOOM_CHECKPOINT_FILE=/tmp/loom-checkpoint.json` to resume interrupted runs
- The checkpoint file records explored permutations so far
- Delete the checkpoint file to start fresh exploration


## Environment Variables
### Loom Variables — Set Before Running `scripts/test-loom.sh`
- `RUSTFLAGS="--cfg sqlite_graphrag_loom"` — enables the project-local loom gate, REQUIRED for all loom tests
- `LOOM_MAX_PREEMPTIONS=1` — limits preemption depth per model (local and CI default: 1)
- `LOOM_MAX_BRANCHES=100` — limits branching factor per execution (local and CI default: 100)
- `LOOM_LOG=1` — enables verbose loom execution tracing to stderr
- `LOOM_CHECKPOINT_FILE=/tmp/loom.json` — path for checkpoint file to resume runs
- `RUST_TEST_THREADS=1` — REQUIRED, forbids parallel execution of loom models
### Cargo and Nextest Variables
- `RUST_TEST_THREADS=N` — controls nextest parallelism at the process level
- `CARGO_TERM_COLOR=always` — preserves color in CI logs
- `NEXTEST_PROFILE=ci` — overrides the active nextest profile from the environment
### sqlite-graphrag-Specific Variables
- `SQLITE_GRAPHRAG_DB_PATH=/tmp/test/graphrag.sqlite` — isolates the project database path per test
- `SQLITE_GRAPHRAG_CACHE_DIR=/tmp/test-cache` — isolates model cache and lock files per test
- `SQLITE_GRAPHRAG_LOG_FORMAT=json` — switches log output to structured JSON
- `SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo` — overrides timestamp timezone


## CI Profiles
### Profile — default
- Activates: always, unless overridden
- `test-threads`: 2
- `RUST_TEST_THREADS`: not set, inherits system default
- Retries: 0
- Slow-timeout: 60s period, terminate after 2 periods (120s effective kill)
- Excludes: loom tests, slow-tests feature
### Profile — ci
- Activates: `/usr/bin/timeout 600 cargo nextest run --profile ci`
- `test-threads`: 2
- `RUST_TEST_THREADS`: 2 (explicit, prevents thermal overload on shared runners)
- Retries: 2 for flaky tests
- Slow-timeout: 180s period, terminate after 3 periods (540s effective kill)
- Excludes: loom tests, slow-tests feature
- Dedicated CI job `slow-contracts` covers `doc_contract_integration` and `prd_compliance` with `/usr/bin/timeout 1200 cargo test --features slow-tests`
### Profile — heavy
- Activates: `/usr/bin/timeout 1800 cargo nextest run --profile heavy --features slow-tests`
- `test-threads`: 1
- `RUST_TEST_THREADS`: 1
- Retries: 0
- Slow-timeout: 900s period, terminate after 2 periods (1800s effective kill)
- Includes: slow-tests feature gated tests
- Excludes: loom tests (always separate)
### Loom CI Job — Separate Workflow Step
- Activates: `ci.yml` job named `loom`
- Environment: `RUSTFLAGS="--cfg sqlite_graphrag_loom"`, `RUST_TEST_THREADS=1`, `LOOM_MAX_PREEMPTIONS=1`, `LOOM_MAX_BRANCHES=100`
- Runs: `/usr/bin/timeout 600 cargo test --test loom_lock_slots --release -- --test-threads=1`
- NEVER merged with the default or ci profile runs


## Troubleshooting
### Thermal Throttling During Tests
- Symptom: test suite slows down progressively, CPU reports high temperature
- Cause: loom tests or stress tests running without proper thread limits
- Fix: stop the test run immediately, let CPU cool for 5 minutes
- Prevention: NEVER run `cargo test` without nextest profiles configured
- Prevention: ALWAYS use `scripts/test-loom.sh` for loom tests
### System Freeze During Loom Tests
- Symptom: machine becomes unresponsive, requires hard reset
- Cause: loom models running in parallel (RUST_TEST_THREADS > 1) on high-TDP CPU
- Fix: hard reset, then set `RUST_TEST_THREADS=1` before any loom run
- Historical case: 2026-04-19 11:37:40 — i9-14900KF froze for 3 minutes 11 seconds
- Prevention: `#[serial(loom_model)]` attribute MUST be present on every loom test
### Loom Test Runs Forever
- Symptom: loom model does not terminate after several minutes
- Cause: `LOOM_MAX_PREEMPTIONS` not set, defaults to unbounded exploration
- Fix: set `LOOM_MAX_PREEMPTIONS=1` for bounded local iteration
- Trade-off: lower values miss rare interleavings; raise the bound only for focused debugging
### Flaky Tests in CI
- Symptom: test passes locally but fails intermittently in CI
- Cause: missing `#[serial]` on tests sharing global state or env vars
- Fix: add `#[serial]` from the `serial_test` crate to affected tests
- Diagnostic: run `/usr/bin/timeout 600 cargo nextest run --profile ci --retries 0` to see all failures


## References

## v1.0.69 Test Inventory
### Test Count Delta
- v1.0.68 baseline: 692 tests passing.
- v1.0.69 final: 745 tests passing (+53).
- 0 failures, 3 ignored (loom tests gated by `#[cfg(sqlite_graphrag_loom)]`).
### New Tests by Module
- `src/commands/claude_runner.rs`: +4 OAuth-only conformance tests (`build_command_oauth_only_mandatory_flags`, `build_command_aborts_when_anthropic_api_key_set`, and 2 more) marked `#[serial_test::serial(env)]` to serialise env mutation.
- `src/commands/codex_spawn.rs`: +4 OAuth-only conformance tests parallel to claude, plus 11 tests for the spawn helper itself (parser edge cases, model validation, command flag presence).
- `src/commands/ingest_claude.rs`: existing tests updated to expect the canonical OAuth-only flag set.
- `src/preservation.rs`: 10 tests for `jaccard_similarity` (boundary conditions, trigrams, empty strings, Unicode) and `PreservationVerdict` (Preserved, Rejected, Unchanged variants).
- `src/memory_source.rs`: 8 tests for `as_str`, `TryFrom<&str>` (valid and invalid), `Display`, and serialisation.
- `src/reaper.rs`: 4 tests (`orphan_min_age_is_one_minute`, `orphan_targets_include_claude_and_codex`, `reaper_report_starts_zeroed`, `scan_completes_without_panic_on_linux`).
- `src/system_load.rs`: 5 tests for `load_average_one`, `ncpus`, and `is_system_saturated`.
- `src/commands/vec.rs`: 3 tests for `vec orphan-list`, `vec purge-orphan`, and `vec stats`.
- `src/commands/optimize.rs`: 1 new test for `OptimizeResponse` field set; existing 2 tests updated.
- `src/lock.rs`: 6 tests (namespace sanitisation, second-invocation blocking, per-namespace isolation, db_hash determinism, db_hash divergence, force flag).
### Serialised Tests
- All 8 OAuth-only tests are marked `#[serial_test::serial(env)]` because they mutate the global environment via `unsafe { std::env::set_var(...) }` and `unsafe { std::env::remove_var(...) }`. Running them in parallel would race.
- The `serial_test` crate (already a project dependency) provides the attribute; the tests are auto-discovered by `cargo nextest run` with serial execution semantics.
### Test Runtime
- Full suite runtime on the reference host: ~10 seconds for the 745 tests.
- The OAuth-only group adds ~0.04 seconds (env mutation is fast).
- Loom tests are NOT included in the default count; they are gated and must be run via `scripts/test-loom.sh`.
- loom crate documentation: `https://docs.rs/loom/latest/loom/`
- loom GitHub repository: `https://github.com/tokio-rs/loom`
- cargo-nextest documentation: `https://nexte.st/`
- cargo-nextest configuration reference: `https://nexte.st/docs/configuration/`
- serial_test crate: `https://docs.rs/serial_test/latest/serial_test/`
