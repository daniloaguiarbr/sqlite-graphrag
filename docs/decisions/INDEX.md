# Architecture Decision Records (ADR) — Index

This index lists all Architecture Decision Records for the `sqlite-graphrag` project. ADRs document significant architectural choices, their context, the alternatives considered, and the consequences of each decision.

Each ADR is available in two languages: English (`adr-XXXX-slug.md`) and Brazilian Portuguese (`adr-XXXX-slug.pt-BR.md`). PT-BR translations are produced after the English version stabilizes.

## Index

| ADR | Title | Version | Status | PT-BR |
|---|---|---|---|---|
| [ADR-0007](adr-0007-retry-policy.md) | Retry Policy for Subprocess Spawns | v1.0.68 | Accepted | — |
| [ADR-0008](adr-0008-process-lifecycle-singleton.md) | Process Lifecycle Singleton (Job Lock) | v1.0.68 | Accepted | — |
| [ADR-0009](adr-0009-windows-sys-handle-pinning.md) | `windows-sys >= 0.59` Handle Type Safety | v1.0.68 | Accepted | — |
| [ADR-0010](adr-0010-mcp-isolation-claude-config-dir.md) | MCP Isolation via `CLAUDE_CONFIG_DIR` | v1.0.69 | Accepted | — |
| [ADR-0011](adr-0011-oauth-only-enforcement.md) | OAuth-Only Enforcement (mandate) | v1.0.69 | Accepted | — |
| [ADR-0012](adr-0012-memory-source-enum.md) | `MemorySource` Enum (type safety) | v1.0.69 | Accepted | — |
| [ADR-0013](adr-0013-singleton-scoped-by-db-hash.md) | Singleton Scoped by `db_hash` (BLAKE3) | v1.0.69 | Accepted | — |
| [ADR-0014](adr-0014-codex-spawn-helper.md) | Codex Spawn Helper Unification | v1.0.69 | Accepted | — |
| [ADR-0015](adr-0015-preservation-gate.md) | Preservation Gate (Jaccard trigram) | v1.0.69 | Accepted | — |
| [ADR-0016](adr-0016-fts5-hardening-flags.md) | FTS5 Hardening Flags | v1.0.69 | Accepted | — |
| [ADR-0017](adr-0017-vec-orphan-handling.md) | Vec Orphan Handling Subcommands | v1.0.69 | Accepted | — |
| [ADR-0018](adr-0018-v1-0-69-status.md) | v1.0.69 Status Executive Summary | v1.0.69 | Accepted | — |
| [ADR-0019](adr-0019-llm-only-one-shot.md) | LLM-Only One-Shot Architecture | v1.0.76 | Accepted | [PT-BR](adr-0019-llm-only-one-shot.pt-BR.md) |
| [ADR-0020](adr-0020-pure-rust-cosine.md) | Pure-Rust Cosine Similarity | v1.0.76 | Accepted | [PT-BR](adr-0020-pure-rust-cosine.pt-BR.md) |
| [ADR-0021](adr-0021-deprecate-daemon.md) | Deprecate and Remove Daemon | v1.0.76 | Accepted | [PT-BR](adr-0021-deprecate-daemon.pt-BR.md) |
| [ADR-0022](adr-0022-blob-embeddings.md) | BLOB-Backed Embedding Tables | v1.0.76 | Accepted | [PT-BR](adr-0022-blob-embeddings.pt-BR.md) |
| [ADR-0023](adr-0023-remove-tokenizers.md) | Remove `tokenizers` Crate Dependency | v1.0.76 | Accepted | [PT-BR](adr-0023-remove-tokenizers.pt-BR.md) |
| [ADR-0024](adr-0024-fts5-coarse-cosine-refine.md) | FTS5 Coarse Filter + Cosine Refine | v1.0.76 | Accepted | [PT-BR](adr-0024-fts5-coarse-cosine-refine.pt-BR.md) |
| [ADR-0025](adr-0025-oauth-only-embedding.md) | OAuth-Only Reaffirmed for Embedding | v1.0.76 | Accepted | [PT-BR](adr-0025-oauth-only-embedding.pt-BR.md) |
| [ADR-0026](adr-0026-v002-vec-tables-migration-drift.md) | V002 Vec-Tables Migration Drift | v1.0.76 | Accepted | [PT-BR](adr-0026-v002-vec-tables-migration-drift.pt-BR.md) |
| [ADR-0027](adr-0027-g40-applied-on-null-fix.md) | G40 — `applied_on` NULL Fix | v1.0.78 | Accepted | [PT-BR](adr-0027-g40-applied-on-null-fix.pt-BR.md) |
| [ADR-0028](adr-0028-g41-phantom-v013-registration.md) | G41 — Phantom V013 Registration | v1.0.78 | Accepted | [PT-BR](adr-0028-g41-phantom-v013-registration.pt-BR.md) |
| [ADR-0029](adr-0029-a1-main-lifecycle-flush-deadlock.md) | A1 — Main Lifecycle Flush Deadlock | v1.0.80 | Accepted | [PT-BR](adr-0029-a1-main-lifecycle-flush-deadlock.pt-BR.md) |
| [ADR-0030](adr-0030-a1-panic-hook-structured.md) | A1 — Structured Panic Hook | v1.0.80 | Accepted | [PT-BR](adr-0030-a1-panic-hook-structured.pt-BR.md) |
| [ADR-0031](adr-0031-a1-completions-test-coverage.md) | A1 — Completions Test Coverage | v1.0.80 | Accepted | [PT-BR](adr-0031-a1-completions-test-coverage.pt-BR.md) |
| [ADR-0032](adr-0032-g53-lib-api-policy.md) | G53 — Library API Policy | v1.0.80 | Accepted | [PT-BR](adr-0032-g53-lib-api-policy.pt-BR.md) |
| [ADR-0033](adr-0033-g53-windows-infra-resilience.md) | G53 — Windows Infra Resilience | v1.0.80 | Accepted | [PT-BR](adr-0033-g53-windows-infra-resilience.pt-BR.md) |
| [ADR-0034](adr-0034-shutdown-resilience.md) | Shutdown Resilience | v1.0.80 | Accepted | [PT-BR](adr-0034-shutdown-resilience.pt-BR.md) |
| [ADR-0035](adr-0035-a2-observability-structured.md) | A2 — Structured Observability | v1.0.80 | Accepted | [PT-BR](adr-0035-a2-observability-structured.pt-BR.md) |
| [ADR-0036](adr-0036-pending-memories-staging.md) | Pending Memories Staging (V014) | v1.0.82 | Accepted | [PT-BR](adr-0036-pending-memories-staging.pt-BR.md) |
| [ADR-0037](adr-0037-shutdown-json-envelope.md) | Shutdown JSON Envelope (exit 19) | v1.0.82 | Accepted | [PT-BR](adr-0037-shutdown-json-envelope.pt-BR.md) |
| [ADR-0038](adr-0038-llm-backend-user-choice.md) | LLM Backend User Choice (`--llm-backend`) | v1.0.82 | Accepted | [PT-BR](adr-0038-llm-backend-user-choice.pt-BR.md) |
| [ADR-0039](adr-0039-llm-host-slot-semaphore.md) | LLM Host-Wide Slot Semaphore | v1.0.82 | Accepted | [PT-BR](adr-0039-llm-host-slot-semaphore.pt-BR.md) |
| [ADR-0040](adr-0040-stderr-capture-fallback-chain.md) | Stderr Capture + Fallback Chain | v1.0.82 | Accepted | [PT-BR](adr-0040-stderr-capture-fallback-chain.pt-BR.md) |
| [ADR-0041](adr-0041-preserve-custom-provider-env.md) | Preserve Custom-Provider Env (6 vars) | v1.0.83 | Accepted | [PT-BR](adr-0041-preserve-custom-provider-env.pt-BR.md) |
| [ADR-0042](adr-0042-claude-backend-split.md) | Claude Backend Split (GAP-002) | v1.0.84 | Accepted | [PT-BR](adr-0042-claude-backend-split.pt-BR.md) |
| [ADR-0043](adr-0043-five-gap-remediation.md) | Five-Gap Remediation (G58, G45-CR5, etc.) | v1.0.85 | Accepted | [PT-BR](adr-0043-five-gap-remediation.pt-BR.md) |
| [ADR-0044](adr-0044-hotfixes-bug-001-002-003.md) | Hotfixes BUG-001/002/003 | v1.0.85.2 | Accepted | [PT-BR](adr-0044-hotfixes-bug-001-002-003.pt-BR.md) |
| [ADR-0045](adr-0045-preflight-validation-layer.md) | Pre-Flight Validation Layer (GAP-META-005) | v1.0.87 | Accepted | [PT-BR](adr-0045-preflight-validation-layer.pt-BR.md) |
| [ADR-0046](adr-0046-preflight-remediation.md) | Pre-flight Remediation (BUG-11) | v1.0.88 | Accepted | [PT-BR](adr-0046-preflight-remediation.pt-BR.md) |
| [ADR-0047](adr-0047-stderr-deduplication.md) | Stderr Deduplication (BUG-12) | v1.0.88 | Accepted | [PT-BR](adr-0047-stderr-deduplication.pt-BR.md) |
| [ADR-0048](adr-0048-schema-as-derived-artifact.md) | Schema as Derived Artifact (schemars + Must-Ignore) | v1.0.89 | Accepted | [PT-BR](adr-0048-schema-as-derived-artifact.pt-BR.md) |
| [ADR-0049](adr-0049-db-flag-scope-per-subcommand.md) | `--db` Flag Scope Per Subcommand | v1.0.89 | Accepted | [PT-BR](adr-0049-db-flag-scope-per-subcommand.pt-BR.md) |
| [ADR-0050](adr-0050-embedding-deadlock-remediation.md) | Embedding Deadlock Remediation | v1.0.89 | Accepted | [PT-BR](adr-0050-embedding-deadlock-remediation.pt-BR.md) |
| [ADR-0051](adr-0051-opencode-backend-integration.md) | OpenCode Backend Integration | v1.0.90 | Accepted | [PT-BR](adr-0051-opencode-backend-integration.pt-BR.md) |
| [ADR-0052](adr-0052-openrouter-embedding-backend.md) | OpenRouter Embedding Backend | v1.0.93 | Accepted | [PT-BR](adr-0052-openrouter-embedding-backend.pt-BR.md) |
| [ADR-0053](adr-0053-v1094-four-gap-remediation.md) | v1.0.94 Four-Gap Remediation | v1.0.94 | Accepted | [PT-BR](adr-0053-v1094-four-gap-remediation.pt-BR.md) |
| [ADR-0054](adr-0054-openrouter-chat-enrich.md) | OpenRouter Chat Enrich (GAP-OR-ENRICH) | v1.0.95 | Accepted | [PT-BR](adr-0054-openrouter-chat-enrich.pt-BR.md) |
| [ADR-0055](adr-0055-enrich-deadletter-rest-concurrency.md) | Enrich Dead-Letter + REST Concurrency (GAP-ENRICH-BACKLOG-CONVERGE, GAP-OPENROUTER-REST-CONCURRENCY) | v1.0.96 | Accepted | [PT-BR](adr-0055-enrich-deadletter-rest-concurrency.pt-BR.md) |
| [ADR-0056](adr-0056-enrich-modularisation-unwrap-audit.md) | Enrich Modularisation + unwrap/expect Audit + parse_claude_output DRY (GAP-SG-57..60) | v1.0.97 | Accepted | [PT-BR](adr-0056-enrich-modularisation-unwrap-audit.pt-BR.md) |
| [ADR-0057](adr-0057-queue-db-relative-sidecar.md) | Enrich + Ingest Queue Sidecar Derived from `--db` (GAP-SG-64, GAP-SG-65) | v1.0.97 | Accepted | [PT-BR](adr-0057-queue-db-relative-sidecar.pt-BR.md) |
| [ADR-0058](adr-0058-prune-dead-orphans.md) | `enrich --prune-dead-orphans` — Clean Orphaned Dead-Letter Rows (GAP-SG-66) | v1.0.97 | Accepted | [PT-BR](adr-0058-prune-dead-orphans.pt-BR.md) |
| [ADR-0059](adr-0059-v1099-degree-cap-removal-doc-convergence.md) | Remove Destructive Degree-Cap Pruning; Align sort-by-degree Doc; Converge body-enrich (GAP-SG-67/68/69) | v1.0.99 | Accepted | [PT-BR](adr-0059-v1099-degree-cap-removal-doc-convergence.pt-BR.md) |
| [ADR-0060](adr-0060-v110-enrichment-backlog-convergence.md) | v1.1.0 — Enrichment Backlog Convergence at the Root (GAP-SG-70..78) | v1.1.0 | Accepted | [PT-BR](adr-0060-v110-enrichment-backlog-convergence.pt-BR.md) |

## Coverage by Version

- **v1.0.68**: 4 ADRs (0007-0010)
- **v1.0.69**: 8 ADRs (0011-0018) — biggest single-release batch
- **v1.0.76**: 7 ADRs (0019-0025) — LLM-only one-shot transformation
- **v1.0.78**: 2 ADRs (0026-0027) — drift fixes
- **v1.0.80**: 7 ADRs (0028-0034) — main lifecycle + observability
- **v1.0.82**: 5 ADRs (0035-0039) — pending queues + slot semaphore
- **v1.0.83**: 1 ADR (0040) — custom-provider env
- **v1.0.84**: 1 ADR (0041) — Claude backend split
- **v1.0.85**: 2 ADRs (0042-0043) — five-gap remediation
- **v1.0.85.2**: 1 ADR (0044) — BUG-001/002/003 hotfixes
- **v1.0.87**: 1 ADR (0045) — preflight layer
- **v1.0.88**: 2 ADRs (0046-0047) — BUG-11/12/13 hotfixes
- **v1.0.89**: 3 ADRs (0048-0050) — schema + flag parity + embedding deadlock
- **v1.0.90**: 1 ADR (0051) — OpenCode backend integration
- **v1.0.93**: 1 ADR (0052) — OpenRouter embedding backend
- **v1.0.94**: 1 ADR (0053) — four-gap remediation (default dim 384, timeout 300s, enrich --mode required, entity embedding honours backends)
- **v1.0.95**: 1 ADR (0054) — OpenRouter chat enrich (`enrich --mode openrouter`)
- **v1.0.96**: 1 ADR (0055) — enrich dead-letter + REST concurrency fan-out
- **v1.0.97**: 3 ADRs (0056-0058) — enrich modularisation + unwrap audit; queue sidecar derived from `--db`; prune orphaned dead-letter
- **v1.0.99**: 1 ADR (0059) — remove destructive degree-cap pruning + flag; align sort-by-degree doc; converge body-enrich
- **v1.1.0**: 1 ADR (0060) — truncated-completion retry, adaptive max_tokens, dead-letter diagnostics, typed retry-classification, shared openrouter_http, User-Agent bump, bounded dequeue, per-operation scan_backlog, transient entity absence (GAP-SG-70..78)

## Bilíngue Status

- **EN (English)**: 54/54 ADRs (100%)
- **PT-BR (Português Brasileiro)**: 42/54 ADRs (78%)
- **PT-BR pendente**: 12 ADRs (0007-0018) — criados antes do mandato bilíngue (legado histórico)

## Conventions

- Each ADR follows the canonical structure: Status, Context, Decision, Alternatives Considered, Consequences (Positive/Negative), Cross-references
- Files are kebab-case: `adr-NNNN-short-slug.md`
- PT-BR versions use `.pt-BR.md` suffix
- Statuses: `Proposed`, `Accepted`, `Deprecated`, `Superseded by ADR-XXXX`
- Cross-references use full paths: `docs/decisions/adr-NNNN-slug.md`
- Version stamps reference the release that introduces the change, not the date written

## Adding a New ADR

1. Choose the next sequential number (next is ADR-0061)
2. Create `adr-0061-slug.md` following the canonical structure
3. Add entry to this INDEX.md (EN row)
4. After EN stabilizes (typically 1+ release), create `adr-0061-slug.pt-BR.md`
5. Add PT-BR column link
6. Update the "Coverage by Version" section
7. Reference the new ADR from `gaps.md` if it closes a documented gap
