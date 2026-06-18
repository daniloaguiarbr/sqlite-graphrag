# Test Plan


- Read the Portuguese version at [TEST_PLAN.pt-BR.md](TEST_PLAN.pt-BR.md)
- Companion guide: [TESTING.md](TESTING.md) documents infrastructure details per layer
- Created during the 2026-06-11 post-publication audit of v1.0.79 (gaps G46-G54)


## Objectives and Scope
### Why This Plan Exists
- G43 proved that suites outside the CI default path hide breakage for entire release cycles
- G50 proved that doctests run ONLY in CI, so a broken rustdoc example shipped in 10 releases
- The published crates.io artifact was never exercised directly before this plan existed
- This plan makes every layer explicit: what runs, when, with which command, and what passing means
### Scope
- Covers the sqlite-graphrag crate: lib unit tests, CLI integration, contracts, concurrency, benchmarks, post-publication audit
- Excludes manual exploratory testing and downstream consumer projects


## Test Layer Matrix
### Layer 1 — Unit Tests (per commit)
- Command: `/usr/bin/timeout 300 cargo nextest run --profile default`
- Scope: pure functions, parsing, validation, error variants inside `src/`
- Pass criterion: ZERO failures
- Note: tests reading the global embedding dim MUST be `#[serial_test::serial(env)]` (G50 cause E)
### Layer 2 — Integration Tests (per commit)
- Command: same nextest invocation; files live in `tests/`
- Prerequisite: `export PATH="$PWD/tests/mock-llm:$PATH"` (dim-aware mocks since G51)
- Pass criterion: ZERO failures
### Layer 3 — Doctests (per commit, MANDATORY locally)
- Command: `/usr/bin/timeout 300 cargo test --doc`
- nextest DOES NOT execute doctests; skipping this layer locally is how G50 cause A shipped broken for 10 releases
- Pass criterion: ZERO failures
### Layer 4 — Slow Contract Suites (per release)
- Command: `/usr/bin/timeout 1800 cargo nextest run --profile heavy --features slow-tests`
- Command: `/usr/bin/timeout 1200 cargo test --features slow-tests --test doc_contract_integration -- --nocapture`
- Command: `/usr/bin/timeout 1200 cargo test --features slow-tests --test prd_compliance -- --nocapture`
- Pass criterion: ZERO failures across ~1220 tests
### Layer 5 — Loom Concurrency (explicit opt-in only)
- Command: `/usr/bin/timeout 3900 bash scripts/test-loom.sh`
- THERMAL RISK: never run outside the dedicated script (2026-04-19 incident)
- Pass criterion: all gated models complete within preemption bounds
### Layer 6 — Benchmarks (per release, informative)
- Command: `/usr/bin/timeout 1800 cargo bench --bench regression_baseline -- --quick`
- Prerequisite: mock LLM on PATH (G50 cause C)
- Pass criterion: no regression above 10 percent versus stored baseline
### Layer 7 — Post-Publication Black-Box (per release, MANDATORY)
- Target: the binary installed from crates.io (`cargo install sqlite-graphrag`), never `target/`
- Setup: temp database via `SQLITE_GRAPHRAG_DB_PATH`, isolated namespace, dim-aware mocks on PATH
- Matrix: bootstrap (init/health/migrate/stats), CRUD lifecycle, search commands, graph commands, maintenance (fts/optimize/backup/vec/export), exit-code contracts (1, 2, 3, 4, 9), JSON contracts versus `docs/schemas/`
- Robustness: OAuth-only abort with `ANTHROPIC_API_KEY` set, SIGPIPE exit 141 on large output, invalid `--tz` exit 2, invalid `SQLITE_GRAPHRAG_EMBEDDING_DIM` warns (G49)
- Dimensionality: fresh database adopts 64; pre-seeded 384 database is adopted (G43) and batches shrink (G44)
- Tarball: download the `.crate`, verify no forbidden files (scripts/legacy, agent configs) and correct READMEs
- Pass criterion: every command matches its expected exit code and schema; this layer would have caught G46-G49 before users did
### Layer 8 — Real-LLM Smoke (per release, OAuth cost)
- Commands: one small create with curated graph, one `recall` round-trip, one `edit --force-reembed`
- Budget: 3 LLM calls, under 5 minutes total; expected create latency under 90 seconds (G42 criterion)
- Record the top-hit score for the retrieval-quality baseline (G54)
- Rate limits are recorded as evidence, never retried in a loop


## Release Gates (run in order, stop on first failure)
### The 8 Mandatory Gates
- Gate 1: `cargo fmt --all --check`
- Gate 2: `/usr/bin/timeout 600 cargo clippy --all-targets --all-features -- -D warnings`
- Gate 3: layers 1-4 green, INCLUDING `cargo test --doc`
- Gate 4: `RUSTDOCFLAGS="-D warnings" /usr/bin/timeout 300 cargo doc --no-deps --all-features`
- Gate 5: `/usr/bin/timeout 120 cargo audit`
- Gate 6: `/usr/bin/timeout 180 cargo deny check advisories licenses bans sources`
- Gate 7: `/usr/bin/timeout 120 cargo publish --dry-run --allow-dirty` plus `cargo package --list` review
- Gate 8: GitHub Actions CI workflow GREEN on the release commit — publishing with a red CI is the root failure documented in G50
### Informative Gates (record, decide, do not skip silently)
- `cargo +stable semver-checks --baseline-version <previous>` — requires rustc >= 1.91; 9 major breaks shipped silently in v1.0.79 (G53)
- `cargo llvm-cov --lib --summary-only` — coverage target 80 percent for new code


## Triggers
### Per Commit
- Layers 1-3 plus Gates 1-2
### Per Release (before `cargo publish`)
- Layers 1-6 plus all 8 gates plus informative gates
### Post-Publication (after crates.io accepts the version)
- Layers 7-8 against the installed registry binary
- File new gaps in `gaps.md` using the G-number format for anything found


## Risks and Constraints
- Loom outside the script can thermally freeze high-core machines (hard reset on 2026-04-19)
- Real-LLM smoke depends on active OAuth; one call costs 10-90 seconds
- Background jobs longer than ~80 minutes can be killed by agent harnesses (G42/C1); keep test jobs short
- `cargo-nextest` and `cargo-llvm-cov` are NOT assumed installed; install via prebuilt binaries before Layer 1


## Latest Plans — v1.0.84 and v1.0.85

See [TEST_PLAN_v1.0.84.md](TEST_PLAN_v1.0.84.md) for the Claude Backend Split test plan (ADR-0042). See [TEST_PLAN_v1.0.85.md](TEST_PLAN_v1.0.85.md) for the Five-Gap Remediation test plan (ADR-0043). Both new plans are snapshots of the released test design.

## Historical Plan — v1.0.80 Plan Deltas — G45, G53, G55 S2, G56, G58, ADR-0033, ADR-0034

The v1.0.80 release (patch bump, no schema migration) added the
following test deltas to the per-layer matrix above. Library
consumers are STRONGLY advised to pin to `=1.0.80` because the
lib API is unstable in v1.x.y (ADR-0032).

### Layer 1 (unit) additions

- `acquire_embedding_singleton` (G45): 5 tests covering same-db
  lock contention, distinct-db independence, `--wait-embed-singleton`
  polling, `force` flag, and PID-based stale-lock detection.
- `AppError::MemoryNotFound` and `AppError::MemoryNotFoundById`
  (G55 S2): 6 tests asserting the identifier is part of the
  variant, exit code is 4, and the pt-BR localized message
  carries name and namespace explicitly.
- `embed_entity_texts_cached` (G56): 4 tests asserting cache
  hit on second call with same model+text, miss on different
  text, `EmbedCacheStats` accounting, and behaviour when the
  underlying embedder returns an error.
- `recall --fallback-fts-only` and `hybrid-search --fallback-fts-only`
  (G58): 3 tests covering the FTS5-only path, plus 1 `#[ignore]`
  test that exercises the `EmbeddingFailed` path (requires `PATH`
  without `codex` or `claude`).

### Layer 2 (integration) additions

- `tests/completions.rs`: 7 end-to-end tests for the `completions`
  subcommand (bash, zsh, fish, powershell, elvish, invalid shell
  exit code, non-empty output validation per shell).
- `tests/shutdown_bypass.rs`: 3 integration tests covering the
  3-layer SHUTDOWN bypass recipe (`PATH=tests/mock-llm:...` plus
  `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` plus `setsid -w timeout`).
- `tests/embedder_singleton.rs`: 2 integration tests covering
  the cross-process embedding singleton against a temp database
  (concurrent `remember` invocations on the same `(namespace, db)`
  pair serialize; distinct pairs proceed in parallel).

### Layer 3 (doctest) additions

- 4 new doctest examples for `acquire_embedding_singleton`,
  `embed_entity_texts_cached`, `MemoryNotFound` construction, and
  the 3-layer SHUTDOWN bypass recipe (verified via
  `cargo test --doc` on every commit).

### Layer 4 (slow contract) additions

- `tests/doc_contract_integration.rs`: 2 new contract tests
  validating that the `vec_degraded`, `vec_error` and `warning`
  envelope fields appear in `recall` and `hybrid-search` JSON
  responses when the LLM subprocess fails (G58).
- `tests/prd_compliance.rs`: 1 new PRD-compliance test validating
  that the 6 new public library symbols documented in
  CHANGELOG.md (G45 and G56) are all `pub` and have the documented
  signatures.

### Layer 7 (post-publication) additions

- The post-publication black-box matrix now includes 3 new
  exit-code contracts: `EmbeddingSingletonLocked` (exit 75,
  retryable), `MemoryNotFound` with identifier in the message
  (exit 4), and `vec_degraded: true` in `recall` (exit 0 with
  warning).

### Layer 8 (real-LLM smoke) deltas

- The top-hit score from the real-LLM `recall` round-trip is
  recorded as the new G54 retrieval-quality baseline (existing
  field in the smoke protocol; v1.0.80 just makes the recording
  mandatory).

### Gates — new additions

- Gate 2 (clippy) gains `--all-features` (was `--all-targets`
  only) and remains the blocking bar.
- Gate 8 (CI GREEN) now requires the new `semver-checks` job
  (informational mode in v1.0.80, will become blocking in
  v1.0.81). The duplicate `--manifest-path` bug from the
  v1.0.79-initial commit is fixed.
- The windows-2025 matrix jobs gained pre-warm and verify steps
  gated on `if: matrix.os == 'windows-2025'` (ADR-0033, G53-WINDOWS-INFRA).
  Local cross-compile validation: `cargo check --target
  x86_64-pc-windows-msvc --lib --all-features` reproduces and
  `E0463` is fixed by `rustup target add x86_64-pc-windows-msvc
  --toolchain 1.88`; the build then reaches the `cc-rs: failed to
  find tool "lib.exe"` frontier, which is the expected host-Linux
  cross-compile limit.

### Triggers update

- Per commit: Layers 1-3 plus Gates 1-2 (unchanged).
- Per release (before `cargo publish`): Layers 1-6 plus all 8 gates
  plus informative gates. The new `semver-checks` informative
  gate is now part of this trigger.
- Post-publication: Layers 7-8 against the installed registry
  binary (unchanged). The Layer 7 matrix now includes the 3 new
  v1.0.80 exit-code contracts above.

## Traceability
- Every failure found by this plan becomes a numbered gap in `gaps.md` with status, root cause, and cause-effect chain
- Gaps fixed must reference the regression test that protects the fix
- Audit of 2026-06-11: this plan's first execution produced G46-G54 and their fixes
