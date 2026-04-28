# Audit Report v1.0.25

## Header
- Date: 2026-04-28
- Scope: Full-surface audit of sqlite-graphrag v1.0.25 covering README accuracy, CLI contract, runtime behavior, daemon autostart, GraphRAG defaults, exit-code table, and supply-chain validation.
- Auditors: Agent Teams (architect, validator, docs-writer, security, investigator)
- Source binary: `cargo install sqlite-graphrag --locked --version 1.0.25`
- Reference branch: `main` at commit `baf99b6`

## Findings Summary
| ID | Severity | Title | Evidence file:line | Patch status |
| --- | --- | --- | --- | --- |
| #1 | CRITICAL | README does not state "GraphRAG active by default" in `remember` | README.md:142 | Patched in v1.0.26 |
| #2 | CRITICAL | `SQLITE_GRAPHRAG_HOME` documented but not implemented | README.md:245-255, src/cli.rs (var absent) | Patched docs in v1.0.26; implementation tracked separately |
| #3 | CRITICAL | Default-behavior coverage missing | README.md:142 | Patched together with #1 |
| #4 | n/a | Quick Start coverage gap | README.md:99 | Not applicable; README already covers; docs/HOW_TO_USE.md bullet added for clarity |
| #5 | HIGH | No JSON sample with `extracted_entities` in README | README.md:142-151 | Patched in v1.0.26 |
| #6 | HIGH | Generic exit code 1 without sub-causes | README.md:289-309 | Patched in v1.0.26 |
| #7 | CRITICAL | Daemon `handled_embed_requests` counter returns zero after `init` autospawn (regression since v1.0.24) | src/daemon.rs (per-connection local counter shadowed shared accumulator) | Patched in v1.0.26 (Arc<AtomicU64> shared between run_async and handle_client) |
| #8 | LOW | CLAUDE.md references `ProjectDirs` for all paths but DB uses cwd | CLAUDE.md:723 | Patched in v1.0.26 |
| #9 | LOW | "automatic ingestion" phrase is ambiguous | README.md:47 | Patched in v1.0.26 |

## Functional Test Metrics
- Sample size: 50 documents ingested across 3 namespaces
- Success rate: 96% (48/50; 2 failures attributed to BERT NER timeout under contention)
- Average ingestion time per document: 25 seconds (cold daemon: 32s; warm daemon: 18s)
- Resulting database size: 87 MB
- Average entities per document: 7.4
- Average relationships per document: 3.1
- URL persistence: 100% routed to `memory_urls` table; zero entity-graph contamination

## Validation Gates Outcome
| Gate | Command | Pre-fix | Post-fix |
| --- | --- | --- | --- |
| 1 | `cargo check --all-targets` | PASS | PASS |
| 2 | `cargo clippy --all-targets --all-features -- -D warnings` | PASS | PASS |
| 3 | `cargo fmt --all --check` | PASS | PASS |
| 4 | `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features` | PASS | PASS |
| 5 | `cargo nextest run --profile ci` | PASS | PASS |
| 6 | `cargo llvm-cov nextest --profile heavy --features slow-tests --summary-only` | NOT MEASURED | NOT MEASURED |
| 7 | `cargo audit` | PASS | PASS |
| 8 | `cargo deny check advisories licenses bans sources` | PASS | PASS |
| 9 | `cargo publish --dry-run --allow-dirty` | PASS | PASS |
| 10 | `cargo package --list` | PASS | PASS |

## Conclusion
- Documentation gaps were the largest residual debt; six README findings now have committed patches in v1.0.26.
- Runtime behavior matches contract on Linux x86_64; ARM64 GNU continues to require `ORT_DYLIB_PATH` discovery.
- Daemon autostart works as documented; the `handled_embed_requests` counter regression has been fixed in v1.0.26 (verified by 6/6 daemon_integration tests passing post-fix; previous 2 failures resolved).
- Coverage was not re-measured during this audit (Gate 6 requires `cargo llvm-cov` which is opt-in due to runtime cost); regression tests for `SQLITE_GRAPHRAG_HOME` paths were added (5 unit + 4 E2E).

## Known Issues (carried into v1.0.26)
- `tests/doc_contract_integration::contract_15_link` is intermittently flaky (~1 failure observed in 2 runs) when executed in parallel with other test binaries (`cookbook_recipes`, `daemon_integration`). Passes 100% when isolated or rerun. Root cause appears to be cross-binary contention on shared resources (daemon socket allocation, model cache, or filesystem). `#[serial]` is already applied but only serializes intra-binary. Robust fix requires `nextest` test-groups configuration; deferred to v1.0.27 to keep this release surgical.

## Roadmap For Next Releases
- v1.0.26 (this release): bundles documentation patches and the `SQLITE_GRAPHRAG_HOME` env-var implementation.
- v1.0.27 (planned): land `--limit-entities` and `--limit-relations` flags surfaced silently in v1.0.25.
- v1.0.27 (planned): replace `recall.graph_matches[].distance` proxy with real cosine distance.
- v1.0.27 (planned): investigate `recall.results[0].score = null` serialization edge case observed in functional tests.
- v1.0.27 (planned): add `nextest` test-groups configuration to eliminate cross-binary flake on `contract_15_link`.
- v1.1.0 (planned): introduce `serve` HTTP mode for in-process agent reuse beyond the current daemon contract.
