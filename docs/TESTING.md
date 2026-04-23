# Testing Guide


- Read the Portuguese version at [TESTING.pt-BR.md](TESTING.pt-BR.md)


## Why Categorized Tests
### The Thermal Livelock Incident — 2026-04-19
- On 2026-04-19 at 11:37:40, the developer's Intel i9-14900KF reached Tjmax 100°C
- VRM temperature hit 99°C and the system required a hard reset after 3 minutes 11 seconds
- Root cause: `tests/loom_lock_slots.rs` ran without a `#[cfg(loom)]` gate
- The loom scheduler is CPU-intensive by design — it explores all thread permutations
- Running loom models without isolation causes thermal runaway on high-core-count CPUs
- This was the third incident in seven days caused by the same unguarded test file
- EVERY loom test MUST be gated with `#[cfg(loom)]` and serialized with `#[serial(loom_model)]`
- NEVER run loom tests inside the default `cargo nextest run` invocation


## Test Categories
### Unit Tests — Inline with Source
- Location: `#[cfg(test)] mod tests` blocks inside each `src/` module
- Run with: `cargo nextest run --profile default`
- Scope: pure functions, error variants, masking, parsing, validation
- Isolation: no I/O, no filesystem, no HTTP calls
- Gate: always compiled, always run in the default profile
### Integration Tests — Separate Files
- Location: `tests/` directory
- Run with: `cargo nextest run --profile default`
- Scope: CLI subcommands, JSON schema contracts, PRD compliance, storage CRUD
- Isolation: `TempDir` per test, `env_clear()`, wiremock for HTTP
- Gate: always compiled, always run in the default profile
### Loom Concurrency Tests — Explicit Opt-in Only
- Location: `tests/loom_lock_slots.rs`
- Run with: `scripts/test-loom.sh` or the CI `loom` job
- Scope: lock-slot semaphore permutation testing
- Isolation: MUST NOT run in parallel with any other test — one model at a time
- Gate: `#[cfg(loom)]` required on EVERY test function and import block
- Thermal risk: unguarded loom tests triggered system freeze on 2026-04-19
### Stress Tests — Opt-in via Feature Flag
- Location: `tests/` files guarded by `#[cfg(feature = "slow-tests")]`
- Run with: `cargo nextest run --profile heavy --features slow-tests`
- Scope: high-concurrency load, large dataset insertion, extended retry loops
- Gate: excluded from default and ci profiles
### Benchmarks — Criterion
- Location: `benches/`
- Run with: `cargo bench` or `cargo criterion`
- Scope: latency baselines for remember, recall, hybrid-search, stats, graph
- Gate: never included in `cargo nextest run`


## How to Run
### Default — Local Development
- Run all unit and integration tests: `cargo nextest run --profile default`
- Run with output on failure: `cargo nextest run --profile default --no-capture`
- Run a specific test by name: `cargo nextest run --profile default test_name_fragment`
- Run a specific file: `cargo nextest run --profile default -E 'test(schema_contract)'`
### CI — Constrained Parallelism
- Run all tests as CI would: `cargo nextest run --profile ci`
- The `ci` profile sets `test-threads = 4` and `RUST_TEST_THREADS=4`
- The `ci` profile enables retries on flaky tests
### Heavy — Stress and Slow Tests
- Run stress and slow tests: `cargo nextest run --profile heavy --features slow-tests`
- The `heavy` profile sets `test-threads = 1` for maximum isolation
- NEVER run the `heavy` profile on a thermally throttled machine


## Loom Concurrency Tests
### How Loom Works
- Loom runs each test many times permuting thread interleavings
- It uses state reduction to avoid combinatorial explosion
- Each model must complete under a bounded preemption count
- CPU usage is extremely high — one core saturates completely per model
- NEVER run loom tests alongside other tests on the same process
### Running Loom Tests Locally
- Use the canonical script: `bash scripts/test-loom.sh`
- The script sets `RUSTFLAGS="--cfg loom"` and `RUST_TEST_THREADS=1`
- The script sets `LOOM_MAX_PREEMPTIONS=2` for faster local iteration
- Run in release mode only: `--release` is mandatory for acceptable speed
- Monitor CPU temperature before and during the run
### Running Individual Loom Tests
- Build first: `RUSTFLAGS="--cfg loom" cargo build --release --tests`
- Run single model: `RUSTFLAGS="--cfg loom" RUST_TEST_THREADS=1 cargo nextest run --release -E 'test(lock_slot)'`
- Set lower preemption bound for local iteration: `LOOM_MAX_PREEMPTIONS=2`
- Set higher bound for CI thoroughness: `LOOM_MAX_PREEMPTIONS=3`
### Checkpoint and Resume
- Set `LOOM_CHECKPOINT_FILE=/tmp/loom-checkpoint.json` to resume interrupted runs
- The checkpoint file records explored permutations so far
- Delete the checkpoint file to start fresh exploration


## Environment Variables
### Loom Variables — Set Before Running `scripts/test-loom.sh`
- `RUSTFLAGS="--cfg loom"` — enables loom feature gate, REQUIRED for all loom tests
- `LOOM_MAX_PREEMPTIONS=2` — limits preemption depth per model (local: 2, CI: 2)
- `LOOM_MAX_BRANCHES=500` — limits branching factor per execution (CI default: 500)
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
- `test-threads`: number of logical CPUs
- `RUST_TEST_THREADS`: not set, inherits system default
- Retries: 0
- Timeout per test: 60 seconds
- Excludes: loom tests, slow-tests feature
### Profile — ci
- Activates: `cargo nextest run --profile ci`
- `test-threads`: 4
- `RUST_TEST_THREADS`: 4 (explicit, prevents thermal overload on shared runners)
- Retries: 2 for flaky tests
- Timeout per test: 120 seconds
- Excludes: loom tests, slow-tests feature
### Profile — heavy
- Activates: `cargo nextest run --profile heavy --features slow-tests`
- `test-threads`: 1
- `RUST_TEST_THREADS`: 1
- Retries: 0
- Timeout per test: 600 seconds
- Includes: slow-tests feature gated tests
- Excludes: loom tests (always separate)
### Loom CI Job — Separate Workflow Step
- Activates: `ci.yml` job named `loom`
- Environment: `RUSTFLAGS="--cfg loom"`, `RUST_TEST_THREADS=1`, `LOOM_MAX_PREEMPTIONS=2`, `LOOM_MAX_BRANCHES=500`
- Runs: `cargo nextest run --release -E 'test(loom)'`
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
- Fix: set `LOOM_MAX_PREEMPTIONS=2` for local iteration
- Trade-off: lower values miss rare interleavings, CI uses 2 for speed
### Flaky Tests in CI
- Symptom: test passes locally but fails intermittently in CI
- Cause: missing `#[serial]` on tests sharing global state or env vars
- Fix: add `#[serial]` from the `serial_test` crate to affected tests
- Diagnostic: run `cargo nextest run --profile ci --retries 0` to see all failures


## References
- loom crate documentation: `https://docs.rs/loom/latest/loom/`
- loom GitHub repository: `https://github.com/tokio-rs/loom`
- cargo-nextest documentation: `https://nexte.st/`
- cargo-nextest configuration reference: `https://nexte.st/docs/configuration/`
- serial_test crate: `https://docs.rs/serial_test/latest/serial_test/`
