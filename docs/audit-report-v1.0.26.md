# Audit Report v1.0.26

## Header
- Date: 2026-04-28
- Scope: Full-surface audit of sqlite-graphrag v1.0.26 covering README EN/PT accuracy, CLI contract, runtime behavior of v1.0.26 fixes (`SQLITE_GRAPHRAG_HOME`, daemon counter, GraphRAG default-on), exit-code table, supply-chain validation, and hunt for new gaps.
- Auditors: 3 parallel Explore agents (validator, gap-hunter, tester-funcional) + interactive review.
- Source binary: `cargo install sqlite-graphrag --locked --version 1.0.26` (39 MB at `/home/comandoaguiar/.cargo/bin/sqlite-graphrag`, modified 2026-04-28 05:21).
- Reference branch: `main` at commit `e4c40d6` (post `8b0f497` v1.0.26 release).

## v1.0.26 Fixes Validation (10/10 verified)
| Fix | Status | Evidence |
| --- | --- | --- |
| `SQLITE_GRAPHRAG_HOME` env var implemented | PASS | `src/paths.rs:67` reads env, integrated into precedence chain |
| README states "GraphRAG active by default" | PASS | `README.md:143` + PT mirror |
| JSON sample with `extracted_entities`/`extracted_relationships`/`urls_persisted` | PASS | `README.md:158-165` + PT mirror |
| Exit codes table with sub-causes for code 1 | PASS | `README.md:312` + PT mirror |
| Env var table lists `SQLITE_GRAPHRAG_HOME` | PASS | `README.md:264` |
| "automatic ingestion" replaced with "daemon autostart" | PASS | `README.md:47` |
| Daemon `handled_embed_requests` counter regression fixed | PASS | `src/daemon.rs:239` `Arc<AtomicU64>` shared between `run_async` and `handle_client` |
| `contract_15_link` no longer asserts stale `source`/`target` keys | PASS | `tests/doc_contract_integration.rs:672-679` asserts `["action", "from", "to", "relation", "weight", "namespace"]` |
| `docs/HOW_TO_USE.md` mentions `--skip-extraction` | PASS | `docs/HOW_TO_USE.md:66` |
| CHANGELOG.md has v1.0.26 entry | PASS | All claimed fixes present |

## NEW Findings (after v1.0.26) — 10 items
| # | Severity | Title | Evidence | Patch in v1.0.27? |
| --- | --- | --- | --- | --- |
| #1 | CRITICAL (P0) | README.md:60 + README.pt-BR.md:60 declared "exactly 10 entity types"; migration V008 expanded to 13 (`organization`, `location`, `date`) | `migrations/V008__expand_entity_types.sql:11-14` vs README:60 ("10 values") | **Patched** (README EN+PT updated to 13 values + new types listed) |
| #2 | CRITICAL (P0) | README EN+PT documented `unlink --relationship-id` flag that does NOT exist; real flags are `--from --to --relation` | `README.md:241`, `README.pt-BR.md:241` | **Patched** (corrected to actual flags) |
| #3 | CRITICAL (P0) | `tests/doc_contract_integration.rs:669` rustfmt drift (multi-line array → single-line) | failure of `cargo fmt --all --check` pre-patch | **Patched** (cargo fmt --all applied) |
| #4 | HIGH (P1) | `init --help` only documents `_DB_PATH`; missing `_HOME` and full precedence chain | `sqlite-graphrag init --help` output line `[env: SQLITE_GRAPHRAG_DB_PATH=]` | **Patched** (docstring updated with full precedence) |
| #5 | HIGH (P1) | `SQLITE_GRAPHRAG_LOG_FORMAT` implemented in `src/main.rs:63` but absent from env-var table | `README.md:267` and `README.pt-BR.md:267` listed only `_LOG_LEVEL` | **Patched** (new row inserted in EN+PT) |
| #6 | HIGH (P1) | 6 `eprintln!` calls in `src/main.rs` violated Pattern 5 (single I/O sink) | `rg eprintln src/main.rs` returned 6 hits at lines 127, 141, 153, 178, 224, 280 | **Patched** (migrated to `output::emit_error` and `output::emit_error_i18n`) |
| #7 | HIGH (P1) | `.config/nextest.toml` lacked `test-groups` for cross-binary serialization; `contract_15_link` flaky cross-binary | nextest.toml had `test-threads = 2` but zero test-group | **Patched** (test-groups `graphrag-serial` added for both `default` and `ci` profiles, validated against nextest 0.9.114 syntax via context7) |
| #8 | HIGH (P1) | `docs/MIGRATION.md` and `docs/MIGRATION.pt-BR.md` claimed "current release is v1.0.17" — 9 versions stale | linhas 3, 21, 58 each | **Patched** (replace_all `1.0.17` → `1.0.27`) |
| #9 | HIGH (P1) | `docs/HOW_TO_USE.md` and `docs/HOW_TO_USE.pt-BR.md` Recipe Two examples used `link --source/--target` flags that do NOT exist; CLI rejects with exit 2 | linhas 109-110 each | **Patched** (corrected to `--from`/`--to`) |
| #10 | MEDIUM (P2) | `src/commands/recall.rs:220-223` graph_distance proxy comment forward-dated "reserved for v1.0.26" — but this IS v1.0.26; risk of agents trusting `distance` field as real cosine | `recall.rs:223` `let graph_distance = 1.0 - 1.0 / (hop as f32 + 1.0);` | **Patched** (comment updated with WARNING and v1.0.28 deferral) |
| #11 | MEDIUM (P2) | `src/constants.rs` lacked `CURRENT_SCHEMA_VERSION: u32` despite comment "must stay in sync with migrations" | `constants.rs:5` comment + V008 latest migration | **Patched** (constant + unit test asserting equality with `V*.sql` count) |
| #12 | LOW (P3) | `src/tokenizer.rs:101-103` flagged as blocking `std::fs::read` in async path | INVESTIGATED | **NOT applicable**: `get_tokenizer`/`get_model_max_length` are called only from `src/commands/remember.rs:389-391` which is `pub fn run()` (synchronous). False positive. |

## Deferred to v1.0.28+ (P2/P3, not patched in v1.0.27)
- `purge --help` mixes EN+PT in same description — violates bilingual policy.
- ~20 flag descriptions across `src/cli.rs` and `src/commands/*.rs` are empty strings (only `[default: ...]` shown in `--help`).
- `__debug_schema` hidden subcommand leaks via clap typo suggestion; should be `#[clap(hide = true)]` or properly documented.
- V008 entity types lack CLI E2E test; current assertions all live in `src/extraction.rs` unit tests.
- Flag fragmentation: `recall -k` (default 10) vs `list --limit` (50) vs `related --limit` (10) — three different defaults, three names. Either unify or document explicitly.
- `link` subcommand lacks `--source`/`--target` aliases that `unlink` already supports (asymmetric).
- Two divergent phrasings for `--json` no-op flag across subcommands.
- 148 `.unwrap()` instances in `src/` — not bloqueante, but technical debt audit deferred.
- `recall.graph_matches[].distance` real cosine implementation (would require re-embedding, +200-500ms latency).
- `RecallResponse.results` direct/graph dedupe (current contract is silent on overlap).
- `deny.toml` ignores RUSTSEC-2024-0436 and RUSTSEC-2025-0119 produced `advisory-not-detected` warnings — may be stale post-cargo-update; cleanup pending verification of `cargo tree`.

## Functional Test Metrics (50 real docs from docs_flowaiper corpus)

- Sample size: 50 heterogeneous files (mostly `.md`) copied to `/tmp/sqlite-graphrag-audit-026/corpus/`
- Pre-warm: PASS (init 1.28s; trivial `remember` 970ms)
- **Successfully ingested: 50/50 (100%)** — zero failures
- Cold first-doc latency: 25,229 ms; warm p50: 20,448 ms (n=49); mean 20,362 ms; min 2,609 ms (small JSON); max 36,195 ms
- Total wall time: 1,059 s (~17.6 min including 4×10s inter-batch pauses)
- Final DB size: 86.78 MB (91,004,928 bytes)
- Final entities: 389 (avg **7.78 / doc**)
- Final relationships: 1,697 (avg **33.94 / doc**)
- Avg entity degree 1.92; max 20

### Recall Sample (5 queries, `-k 5` direct + graph traversal)
| Query | direct | graph | total | Top-1 distance | Latency |
| --- | --- | --- | --- | --- | --- |
| agent | 5 | 43 | 48 | 0.1450 | 62 ms |
| claude | 5 | 43 | 48 | 0.1361 | 49 ms |
| API | 5 | 43 | 48 | 0.1361 | 50 ms |
| test | 5 | 44 | 49 | 0.1271 | 49 ms |
| configuration | 5 | 43 | 48 | 0.1335 | 49 ms |

Top-1 distances 0.127–0.145 (low = strong semantic match). Recall p50 ~50 ms.

### Path Resolution Validation (INSTALLED v1.0.26 binary)
- `SQLITE_GRAPHRAG_HOME` override: **PASS** (DB landed at `$HOME_DIR/graphrag.sqlite`)
- Traversal `..` rejected: **PASS** (error message bilingual + exit non-zero)
- `--db` precedence over `_HOME`: **PASS** (DB created at `--db`; `$HOME/graphrag.sqlite` not created)

### Safety Posture
- RAM stayed 40-43% across all batches (peak 29 GiB used / 62 GiB total)
- Load1 oscillated 2.6 - 3.7 (below 4×nCPU=128 threshold)
- No abort triggered, no thermal/swap incident, no zombies
- Inter-batch 10s pauses honored; per-doc 60s hard timeout never exceeded

### Anomaly (minor, deferred to v1.0.28+)
- CLI ergonomic friction: `--db` flag position is inconsistent across subcommands (some accept it before subcommand, some after). Documented behavior, but worth standardizing.
- `recall -k 5` returns ~48 results because `results[]` aggregates `direct_matches` + `graph_matches` (5 + 43-44 graph). Already in deferred dedupe item.

## Validation Gates Outcome
| Gate | Command | Pre-patches | Post-patches |
| --- | --- | --- | --- |
| 1 | `cargo check --all-targets` | PASS | PASS (0.87s, cached) |
| 2 | `cargo clippy --all-targets --all-features -- -D warnings` | PASS | PASS (zero warnings) |
| 3 | `cargo fmt --all --check` | **FAIL** (1 diff in `tests/doc_contract_integration.rs:669`) | PASS (after `cargo fmt --all`) |
| 4 | `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features` | PASS | PASS (zero warnings) |
| 5 | `cargo nextest run --profile default` | 449/449 PASS, 5 skipped | PASS (now also serializes `contract_15_link` cross-binary; new test `schema_version_matches_migrations_count` PASS) |
| 6 | `cargo llvm-cov` (heavy) | NOT MEASURED | NOT MEASURED |
| 7 | `cargo audit` | PASS (0 vulns; 2 allowed advisories) | PASS |
| 8 | `cargo deny check advisories licenses bans sources` | PASS (2 `advisory-not-detected` warnings) | PASS (warnings unchanged, deferred cleanup to v1.0.28) |
| 9 | `cargo publish --dry-run --allow-dirty` | PASS | PASS (`Finished dev profile in 1m 36s`, "aborting upload due to dry run") |
| 10 | `cargo package --list` | PASS (sanitized) | PASS (179 files, no `CLAUDE.md`/`.serena`/`.claude`/`docs_rules`/`.profraw`/`.env`) |

## Conclusion
- Documentation drift was the largest residual debt — 6 of 11 effective patches are documentation fixes (entity_type 10→13, unlink flag, MIGRATION version, HOW_TO_USE link recipes, README LOG_FORMAT, init --help precedence).
- 4 code refactors land in v1.0.27 — eprintln!→output.rs, recall comment, schema version constant, fmt drift fix.
- 1 test-only config addition (nextest test-groups for cross-binary flake elimination).
- 1 false-positive identified (tokenizer spawn_blocking) — investigated and confirmed not applicable.
- Runtime behavior of v1.0.26 fixes confirmed correct on Linux x86_64.
- Daemon counter regression fix from v1.0.26 verified with `Arc<AtomicU64>` shared.
- 11 effective patches landed; 11 deferred to v1.0.28+ for orthogonal scope reasons.

## Roadmap For Next Releases
- **v1.0.28** (planned):
  - Implement real cosine distance for `recall.graph_matches[].distance` (replace hop-count proxy).
  - Dedupe `RecallResponse.results` between `direct_matches` and `graph_matches` (or document overlap explicitly).
  - Cleanup `deny.toml` advisory ignores after `cargo tree` verification of transitive deps.
  - Localize `purge --help` (currently EN+PT mix violating bilingual policy).
  - Hide `__debug_schema` properly via `#[clap(hide = true)]`.
  - Fill ~20 empty flag descriptions in `src/cli.rs` and `src/commands/*.rs`.
  - Add CLI E2E test for V008 entity types round-trip.
  - Audit `.unwrap()` instances in production paths (148 total).
- **v1.0.29** (planned):
  - Unify `-k`/`--limit` flag fragmentation (or document explicit semantic difference).
  - Add `--source`/`--target` aliases to `link` for symmetry with `unlink`.
  - Standardize `--json` no-op phrasing across all subcommands.
- **v1.1.0** (planned):
  - Introduce `serve` HTTP mode for in-process agent reuse beyond the current daemon contract.
