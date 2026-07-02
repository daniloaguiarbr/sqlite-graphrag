
## v1.0.86, v1.0.87, v1.0.88, v1.0.89 — Coverage Update

This section updates the framework to cover the documentation generated for the four most recent releases. The framework was last updated at v1.0.85.2 (line 661); this update closes a 3-release gap.

### v1.0.86 — LLM-Heavy Surface and Host-Wide Slot Semaphore
- New subcommand families documented in `INTEGRATIONS.md` lines 50-75: `pending` (V014, `pending_memories` table), `embedding` (status/list/abandon), `pending-embeddings` (V015, retry queue), `slots` (host-wide semaphore)
- New global flags documented in `AGENTS.md` lines 14-22: `--max-concurrency`, `--wait-lock`, `--llm-parallelism`, `--ingest-parallelism`, `--graceful-shutdown-secs`, `--skip-embedding-on-failure`
- New ADRs: ADR-0036 (pending-memories), ADR-0037 (shutdown-json-envelope), ADR-0038 (llm-backend), ADR-0039 (llm-host-slot-semaphore), ADR-0040 (stderr-capture-fallback-chain)
- `llms.txt` is OUT OF DATE for v1.0.86 (still declares "Current version 1.0.85.2") — should be regenerated for v1.0.90

### v1.0.87 — Pre-flight Validation Layer (ADR-0045, GAP-META-005)
- New module `src/spawn/preflight.rs` (≥200 lines, 7 guards, 15 unit tests) gates every LLM subprocess spawn BEFORE the fork
- New `AppError::PreFlightFailed(PreFlightError)` variant with `exit_code() == 16` and `is_permanent() == true`
- New exit code 16 (`EX_CONFIG`) for pre-flight failures — NOT documented in any existing exit code table
- New env var: `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` — opt-out for all 7 guards
- New ADR: ADR-0045 (preflight-validation-layer) — created in EN only as of v1.0.87; PT-BR translated in v1.0.89
- Documented in: `INTEGRATIONS.md` (added v1.0.89), `HEADLESS_INVOCATION.md` (added v1.0.89), `SECURITY.md` (added v1.0.89), `AGENTS.md` (added v1.0.89)
- `llms.txt` is OUT OF DATE for v1.0.87

### v1.0.88 — Hotfixes BUG-11/12/13 (ADR-0046, ADR-0047)
- **BUG-11 (CRITICAL)** fixed: preflight failure in `extract/llm_embedding.rs:563-565` now propagates to `remember` via `embed_via_backend_strict` instead of silent persistence with `backend_invoked: "none"`
- **BUG-12 (MEDIUM)** fixed: OAuth-only enforcement now emits 1 stderr line (was 2) — duplicate `eprintln!` removed
- **BUG-13 (MEDIUM)** fixed: `link --create-missing` now respects entity-name validation (rejected ALL_CAPS abbreviations in CLI were previously accepted)
- New ADRs: ADR-0046 (preflight-remediation), ADR-0047 (stderr-deduplication)
- New regression tests: `tests/bug11_preflight_regression.rs` (2 tests), `oauth_stderr_emits_single_line_v1088` (1 test), `tests/entity_validation_integration.rs` (8 tests)
- Documented in: `AGENTS.md` (added v1.0.89), `TESTING.md` (added v1.0.89)
- `llms.txt` is OUT OF DATE for v1.0.88

### v1.0.89 — Schema Drift, Flag Parity, Description Heuristic
- **GAP-E2E-007 (P1)** closed: `health.schema.json` regenerated via `schemars` derive macro. `additionalProperties: true` (Must-Ignore policy per RFC 7493 I-JSON). 17 new fields added.
- New bin: `cargo run --bin dump-schema` regenerates 70+ schemas
- **GAP-E2E-008 (P3)** closed: `embedding status/list/abandon`, `pending list/show` now accept `--db <PATH>` (no `clap::Arg::global = true` — see ADR-0049)
- **GAP-E2E-009 (P3)** closed: `migrate --dry-run --json` now reports pending migrations without applying
- **GAP-E2E-010 (P3)** closed: `codex-models --json` accepted as no-op; `pending list --db <PATH>` parity
- **GAP-E2E-011 (P2)** closed: `ingest --auto-describe` (default true) extracts description from first meaningful body line
- **GAP-E2E-002 (P3)** closed: `health --namespace <NS> --json` filters counts to a single namespace
- **GAP-E2E-001 (P2)** closed: Binary size 14.6 MiB documented in `Cargo.toml:6` (was 6 MB since v1.0.76)
- New ADRs: ADR-0048 (schema-as-derived-artifact), ADR-0049 (db-flag-scope-per-subcommand)
- New regression tests: 7 suites covering the 10 GAPs (see TESTING.md v1.0.89 section)
- Documented in: `AGENTS.md` (added v1.0.89), `TESTING.md` (added v1.0.89), `docs/decisions/INDEX.md` (created v1.0.89)
- `llms.txt` still needs update for v1.0.89

### v1.0.90 — OpenCode Backend Integration (ADR-0051)
- New LLM backend `opencode` added to `LlmBackendChoice` enum
- New global flags: `--opencode-binary`, `--opencode-model`, `--opencode-timeout`
- New env vars: `SQLITE_GRAPHRAG_OPENCODE_BINARY`, `SQLITE_GRAPHRAG_OPENCODE_MODEL`, `SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL`, `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT`
- New ADR: ADR-0051 (opencode-backend-integration) — EN only, PT-BR pending
- Documented in: `AGENTS.md`, `HEADLESS_INVOCATION.md`, `COOKBOOK.md`, `HOW_TO_USE.md`, `MIGRATION.md`

### v1.0.93 — OpenRouter Embedding Backend (ADR-0052, GAP-OR-INGEST)
- New `EmbeddingBackendChoice` enum (`auto|openrouter|llm`) separate from `LlmBackendChoice`
- New global flags: `--embedding-backend`, `--embedding-model`, `--openrouter-api-key`
- New ingest flag: `--enrich-after` (triggers `enrich --operation memory-bindings` after embedding)
- OpenRouter REST API embedding via `reqwest+rustls-tls` (~100-500ms vs 20-60s subprocess)
- 10 verified OpenRouter models (qwen3-embedding-4b/8b, nvidia nemotron, openai 3-small/3-large, perplexity, mistral, baai bge-m3, google gemini-embedding-001/002)
- 5 BUG-OR fixes: input_type per model, MRL detection, model validation, HTTP 200 malformed retry, dimension override
- API key handled via `secrecy::SecretString` with zeroize-on-drop, NEVER logged
- No default model — user MUST specify `--embedding-model` when using `--embedding-backend openrouter`
- New ADR: ADR-0052 (openrouter-embedding-backend) — EN + PT-BR
- New test plan section in `TEST_PLAN.md` and `TEST_PLAN.pt-BR.md` covering layers 1/2/7/8
- New env var: `OPENROUTER_API_KEY`
- New env var: `SQLITE_GRAPHRAG_EMBEDDING_BACKEND`
- Documented in: ALL 12 root .md files, ALL 24 docs/ .md files, 7 schema JSON files, `docs/schemas/README.md`, `docs/decisions/INDEX.md`

### v1.0.94 — Four-Gap Remediation (ADR-0053)
- `DEFAULT_EMBEDDING_DIM` 64 -> 384; `DEFAULT_EMBED_TIMEOUT_SECS` 120 -> 300; entity embedding honours `--embedding-backend`/`--llm-backend`; `enrich --mode` now required (clap exit 2 when omitted).
- New ADR: ADR-0053 (EN + PT-BR); ADR-0051 PT-BR translation added (closes the only post-framework bilingual gap); docs/decisions/INDEX.md updated.
- Updated: README, INTEGRATIONS, SECURITY, CONTRIBUTING (root EN+PT); docs/AGENTS, HOW_TO_USE, COOKBOOK, MIGRATION, TESTING (EN+PT); DOCUMENTATION_FRAMEWORK; SKILL (EN+PT); llms.txt, llms.pt-BR.txt, llms-full.txt.

### v1.0.95 — OpenRouter Chat Enrich (ADR-0054, GAP-OR-ENRICH)
- New `enrich --mode openrouter` routes the enrich JUDGE to the OpenRouter REST `/chat/completions` endpoint; structured extraction (memory-bindings, entity-descriptions, body-enrich) no longer requires a local claude/codex/opencode CLI.
- New flags: `--openrouter-model` (REQUIRED, no default, exit 1 when absent), `--openrouter-api-key` (env `OPENROUTER_API_KEY`), `--openrouter-timeout` (default 300), `--openrouter-base-url` (optional).
- New module `src/chat_api.rs` (`OpenRouterChatClient`) mirrors `src/embedding_api.rs`; the SCAN->JUDGE->PERSIST pipeline is unchanged — only the JUDGE transport moves.
- Structured Outputs strict mode plus `provider.require_parameters`; `reasoning.enabled:false` with a reasoning-mandatory fallback path.
- `usage.cost` is read from the response; 13/13 OpenRouter text models compatible (9 with `reasoning.enabled:false`, 4 via the reasoning-mandatory fallback).
- New ADR: ADR-0054 (openrouter-chat-enrich) — EN + PT-BR.
- Updated in this release: README/CHANGELOG/AGENTS/INTEGRATIONS/SECURITY/CONTRIBUTING (root EN+PT); docs/AGENTS, HOW_TO_USE, COOKBOOK, MIGRATION, HEADLESS_INVOCATION, CROSS_PLATFORM, TESTING, TEST_PLAN, schemas/README (EN+PT); SKILL (EN+PT); docs/decisions/INDEX.md + ADR-0054.

### v1.0.96 — Enrich Dead-Letter + REST Concurrency (ADR-0055)
- New `enrich --until-empty` drives an internal scan->drain loop until the eligible queue empties or `--max-runtime` (default 3600s) expires, replacing the external bash drain loop; resolves GAP-ENRICH-BACKLOG-CONVERGE.
- Dead-letter discipline: the `.enrich-queue.sqlite` queue gains `error_class` and `next_retry_at` columns (idempotent ALTER TABLE) plus a terminal `dead` status and the `idx_enrich_queue_eligible` index; Transient failures (rate-limit, timeout, 5xx) reschedule with exponential backoff (reusing `AttemptOutcome`/`compute_delay` from `src/retry.rs`), HardFailures (validation, parse) go terminal at once, and an item turns `dead` after `--max-attempts` (default 8, range 1..=20) Transient retries or on the first HardFailure — the live set strictly shrinks toward convergence.
- New flags: `--until-empty`, `--max-runtime <SECONDS>`, `--max-attempts <N>`, `--status` (read-only JSON counts: `unbound_backlog`, per-operation `scan_backlog`, `queue_pending/done/failed/dead/skipped`, `eligible_now`, `waiting` — calls NO LLM, acquires NO singleton; `scan_backlog` (GAP-SG-77, v1.1.0) is the real per-operation database backlog a scan would enqueue — it kills the false `pending=0` for `entity-descriptions`/`body-enrich`/`re-embed`, and `state` derives `pending-scan` from it), `--rest-concurrency <N>` (clamp 1..=16, default 8, DISTINCT from `--llm-parallelism`).
- REST fan-out (GAP-OPENROUTER-REST-CONCURRENCY): `embed_passages_parallel_with_embedding_choice` (`src/embedder.rs`) fans out OpenRouter REST calls per 32-chunk batch via a bounded `tokio::task::JoinSet` (NO new dependency); chunk order preserved by index, in-flight clamp 1..16 (Cloudflare-safe); SQLite writes stay serialized via WAL + atomic claim (single-writer intact).
- New ADR: ADR-0055 (enrich-deadletter-rest-concurrency) — EN + PT-BR; docs/decisions/INDEX.md updated.
- New schema: `docs/schemas/enrich-status.schema.json` (DERIVED per ADR-0048, regenerated via `dump_schema` — NEVER hand-edited).
- Updated in this release: README/CHANGELOG/AGENTS/INTEGRATIONS/SECURITY/CONTRIBUTING (root EN+PT); docs/AGENTS, HOW_TO_USE, COOKBOOK, MIGRATION, HEADLESS_INVOCATION, CROSS_PLATFORM, TESTING, TEST_PLAN (EN+PT); SKILL (EN+PT); llms.txt, llms.pt-BR.txt, llms-full.txt.
- Tests: 8 dead-letter unit tests in `commands::enrich::tests`, 1 ordering test in `embedder::tests` (`reassemble_ordered_restores_input_order`), live `tests/openrouter_live_concurrency.rs` (`#[ignore]`); nextest 1086 passed, 0 failed, 6 skipped.

### v1.0.97 — Post-Sealing Audit (ADR-0056/0057/0058)
- New read-only inspector `enrich --prune-dead-orphans` (GAP-SG-66, ADR-0058): deletes ONLY `status='dead' AND item_type='memory'` enrich-queue rows whose `item_key` is absent from the main DB; entity-keyed dead rows untouched; only the `.enrich-queue.sqlite` sidecar is mutated; `DeadSummary` gains a `pruned` count.
- Queue sidecar derived from `--db` (GAP-SG-64 enrich + GAP-SG-65 ingest, ADR-0057): new helper `paths::sidecar_path(db_path, filename)` resolves `.enrich-queue.sqlite`/`.ingest-queue.sqlite` next to the database instead of the CWD; no legacy file migration.
- Enrich modularisation + `unwrap`/`expect` audit + `parse_claude_output` DRY (GAP-SG-57..60, ADR-0056): `src/commands/enrich.rs` (6013 lines) split into `src/commands/enrich/` (mod + queue + scan + postprocess + extraction); production `unwrap`/`expect` converted to `?` and gated by the `src/lib.rs` lint.
- Flaky `llm_slots::tests` hardened (GAP-SG-63); global binary realigned via `cargo install --path . --locked --force` so `installed_binary_smoke` runs 26/0 without bypass (GAP-SG-62).
- Documented in: README, CHANGELOG, AGENTS, COOKBOOK, HOW_TO_USE, HEADLESS_INVOCATION, INTEGRATIONS (root EN+PT); llms.txt, llms.pt-BR.txt, llms-full.txt; SKILL (EN+PT); TESTING, MIGRATION (EN+PT); docs/decisions/INDEX.md + ADR-0056/0057/0058 (EN+PT).

### v1.0.99 — Remove Destructive Degree-Cap Pruning + Doc/Convergence Fixes (ADR-0059, GAP-SG-67/68/69)
- **GAP-SG-67** — the destructive GLOBAL degree-cap pruning is REMOVED: `graph::enforce_degree_cap` and its two call sites (`remember`, `link`) are deleted, and the `--max-entity-degree` flag is REMOVED (BREAKING: clap exit 2 if passed; the `--max-entity-degree 0` mitigation is now obsolete). Writes are 100% additive — they never prune/delete edges nor emit a warn, and the total `relationships` count never decreases on a normal write. Schema stays v15 (no migration). Trade-off: hub degree grows unbounded; future normalisation is an explicit MAINTENANCE command only.
- **GAP-SG-68** — `graph entities --sort-by degree` sorted ascending against a doc-comment that promised "descending by default"; fixed by aligning the DOC to the ascending behaviour ("Sort by degree (total number of relationships). Use --order desc for most-connected-first."). 6 `build_order_by_*` tests stay green; only `src/commands/graph_export.rs` (one line) changed.
- **GAP-SG-69** — `enrich --operation body-enrich --until-empty` did not converge (scan re-scanned bodies rejected by the preservation guard, status `skipped`); fixed with the `skipped_item_keys` helper (`queue.rs`), the BodyEnrich initial scan + rescan now exclude preservation-vetoed `skipped` keys, the `.enrich-queue.sqlite` sidecar is preserved while `skipped` rows remain (removed only when `dead==0` AND `skipped==0`), and `cleanup_queue_entry` clears the veto when the body changes. Empirical convergence 55→3; test `skipped_item_keys_excludes_only_skipped_for_operation`.
- New ADR: ADR-0059 (EN + PT-BR); docs/decisions/INDEX.md updated.
- Updated: README, CHANGELOG, AGENTS, INTEGRATIONS (root EN+PT); docs/AGENTS, MIGRATION (EN+PT); DOCUMENTATION_FRAMEWORK; llms.txt, llms.pt-BR.txt, llms-full.txt.

### v1.1.01 — Production-Database Audit Remediation (12-priority roadmap, gaps.md)
- Official release name is v1.1.01; the crate manifest carries `version = "1.1.1"` (SemVer rejects a leading zero in the patch component). Schema stays v15 (no migration). Binary ~19 MiB.
- New command: `graph recompute-degree` (P3) reconciles the `entities.degree` cache from the real `relationships` rows; new schema `docs/schemas/graph-recompute-degree.schema.json`.
- New flags: `--target` (`enrich --operation re-embed`), `--literal-from` (`reclassify-relation`), `--ids`/`--into-id` (`merge-entities`, response gains required `target_id`), `--id` (`rename-entity`), `--name-prefix` (`ingest`).
- Coverage observability (P6): `health` and `embedding status` gain `*_missing` counters (LEFT JOIN, absent embedding table reports ALL missing); `embedding-status.schema.json` and `health.schema.json` updated.
- Exit code 6 limit errors are fully typed (structured message instead of a generic payload error).

| Document | EN Coverage | PT-BR Coverage | Drift |
|---|---|---|---|
| `README.md` / `README.pt-BR.md` | v1.0.99 (GAP-SG-67/68/69) | v1.0.99 (espelhado) | Current |
| `CHANGELOG.md` / `CHANGELOG.pt-BR.md` | v1.0.99 (100%) | v1.0.99 (100%) | Current |
| `AGENTS.md` / `AGENTS.pt-BR.md` | v1.0.99 (GAP-SG-67/68/69) | v1.0.99 (espelhado) | Current |
| `INTEGRATIONS.md` / `INTEGRATIONS.pt-BR.md` | v1.0.99 (GAP-SG-67/68/69) | v1.0.99 (espelhado) | Current |
| `SECURITY.md` / `SECURITY.pt-BR.md` | v1.0.96 (no v1.0.99 exit code/env var change) | v1.0.96 (espelhado) | Current |
| `CONTRIBUTING.md` / `CONTRIBUTING.pt-BR.md` | v1.0.96 (no v1.0.99 contributor-flow change) | v1.0.96 (espelhado) | Current |
| `llms.txt` / `llms.pt-BR.txt` | v1.0.99 (GAP-SG-67/68/69) | v1.0.99 (espelhado) | Current |
| `llms-full.txt` | v1.0.99 (GAP-SG-67/68/69) | N/A | Current |
| `COOKBOOK.md` / `COOKBOOK.pt-BR.md` | v1.0.99 (GAP-SG-67 upgrade recipe) | v1.0.99 (espelhado) | Current |
| `HOW_TO_USE.md` / `HOW_TO_USE.pt-BR.md` | v1.0.99 (GAP-SG-67/68/69) | v1.0.99 (espelhado) | Current |
| `MIGRATION.md` / `MIGRATION.pt-BR.md` | v1.0.99 (--max-entity-degree removal, GAP-SG-67) | v1.0.99 (espelhado) | Current |
| `TESTING.md` / `TESTING.pt-BR.md` | v1.0.99 (GAP-SG-67/68/69 test changes) | v1.0.99 (espelhado) | Current |
| `CROSS_PLATFORM.md` / `CROSS_PLATFORM.pt-BR.md` | v1.0.97 (no v1.0.99 platform change) | v1.0.97 (espelhado) | Current |
| `HEADLESS_INVOCATION.md` / `HEADLESS_INVOCATION.pt-BR.md` | v1.0.97 (no v1.0.99 change) | v1.0.97 (espelhado) | Current |
| `TEST_PLAN.md` / `TEST_PLAN.pt-BR.md` | v1.0.99 (GAP-SG-67/68/69 test plan) | v1.0.99 (espelhado) | Current |
| `skill/sqlite-graphrag-en` / `skill/sqlite-graphrag-pt` | v1.0.97 (post-sealing audit) | v1.0.97 (espelhado) | Current |
| `docs/decisions/` (52 ADRs) | 100% (52/52) | 77% (40/52) | 12 ADRs missing PT-BR (adr-0007 through adr-0018) |
| `docs/schemas/` (70+ schemas) | 100% (backend_invoked includes openrouter) | N/A | Current |

### Framework Update — Mandatory Coverage of v1.0.86+

To prevent future drift, the following is now MANDATORY for any release that introduces new CLI surface, new ADRs, or new exit codes:

1. **README.md AND README.pt-BR.md** updated in the same release cycle
2. **CHANGELOG.md AND CHANGELOG.pt-BR.md** updated in the same release cycle
3. **AGENTS.md** updated with `## New in vX.Y.Z` section
4. **TESTING.md** updated with `## vX.Y.Z — <summary>` section listing new tests
5. **INTEGRATIONS.md** updated with new subcommand family documentation
6. **SECURITY.md** updated if exit codes change or new env vars are introduced
7. **llms.txt** updated if new subcommands or global flags are added
8. **New ADR** created in EN, PT-BR translation added within 1 release cycle
9. **docs/decisions/INDEX.md** updated to include new ADR with link

A CI gate that checks all 9 items would prevent the 3-version drift observed in v1.0.86-88.# Documentation Framework — Prompt Rules for Replication

> Regras imperativas invioláveis para replicar o framework de documentação deste projeto em qualquer outro projeto Rust CLI ou software open-source


## Visão Geral do Framework

- Este framework define 3 camadas de documentação: RAIZ, DOCS e SKILL
- CADA camada tem arquivos obrigatórios, estrutura definida e objetivo específico
- TODOS os arquivos de documentação seguem o padrão bilíngue EN/PT-BR
- A camada RAIZ comunica com humanos (desenvolvedores, contribuidores, usuários)
- A camada DOCS comunica com humanos avançados (integradores, operadores, testadores)
- A camada SKILL comunica com máquinas (agentes de IA, LLMs, pipelines de automação)


## Princípio Bilíngue Inviolável

### OBRIGATÓRIO — Espelhamento 1:1
- CADA arquivo `.md` na raiz DEVE ter seu par `.pt-BR.md` espelhado
- CADA arquivo `.md` na pasta `docs/` DEVE ter seu par `.pt-BR.md` espelhado
- CADA arquivo `.txt` de LLM DEVE ter seu par `.pt-BR.txt` espelhado
- CADA pasta em `skill/` DEVE ter variante `-en` e variante `-pt`
- NUNCA publique arquivo de documentação sem seu par bilíngue
- NUNCA misture idiomas dentro do mesmo arquivo
- NUNCA traduza automaticamente sem revisão humana

### OBRIGATÓRIO — Cross-Reference Entre Idiomas
- CADA arquivo EN DEVE conter link para versão PT-BR na primeira linha útil
- CADA arquivo PT-BR DEVE conter link para versão EN na primeira linha útil
- Formato EN: `Read this document in [Portuguese (pt-BR)](ARQUIVO.pt-BR.md).`
- Formato PT-BR: `Leia este documento em [inglês (EN)](ARQUIVO.md).`
- POSICIONE o link ANTES de qualquer conteúdo substantivo

### OBRIGATÓRIO — Convenção de Nomes
- Versão inglês: `NOME.md` (nome canônico sem sufixo)
- Versão português: `NOME.pt-BR.md` (sufixo `.pt-BR` antes da extensão)
- Versão inglês TXT: `nome.txt`
- Versão português TXT: `nome.pt-BR.txt`
- NUNCA use `NOME-en.md` ou `NOME_EN.md` para a versão inglês
- NUNCA use `NOME-pt.md` sem o `-BR` completo


## Camada 1 — Pasta Raiz (18 arquivos MD + 2 pares de templates + 3 licenças + 4 configs)

### OBRIGATÓRIO — Inventário Completo da Raiz — Documentação Bilíngue
- `README.md` + `README.pt-BR.md` — Porta de entrada do projeto
- `CHANGELOG.md` + `CHANGELOG.pt-BR.md` — Histórico de mudanças por versão
- `CONTRIBUTING.md` + `CONTRIBUTING.pt-BR.md` — Guia de contribuição
- `CODE_OF_CONDUCT.md` + `CODE_OF_CONDUCT.pt-BR.md` — Código de conduta
- `SECURITY.md` + `SECURITY.pt-BR.md` — Política de segurança e vulnerabilidades
- `INTEGRATIONS.md` + `INTEGRATIONS.pt-BR.md` — Catálogo de integrações externas
- `llms.txt` + `llms.pt-BR.txt` — Resumo compacto para agentes de IA (llms.txt standard)
- `llms-full.txt` — Versão expandida do llms.txt com documentação completa inline (EN-only)
- `gaps.md` — Relatório de acceptance testing com gaps identificados (EN-only)

### OBRIGATÓRIO — Arquivos de Licença na Raiz
- `LICENSE` — Arquivo de licença principal (symlink ou dual-license notice)
- `LICENSE-MIT` — Texto completo da licença MIT
- `LICENSE-APACHE` — Texto completo da licença Apache 2.0
- DEVE usar licença dual `MIT OR Apache-2.0` como padrão Rust community
- DEVE incluir AMBOS os textos de licença como arquivos separados
- NUNCA omita arquivos de licença — crates.io e GitHub dependem deles

### OBRIGATÓRIO — Arquivos de Configuração Documentais na Raiz
- `Cargo.toml` — Manifesto do projeto com metadados em inglês
- `Cross.toml` — Configuração de cross-compilation
- `deny.toml` — Política de supply chain e licenças
- `rust-toolchain.toml` — Pinning de toolchain Rust

### Objetivo de Cada Arquivo da Raiz

#### README.md + README.pt-BR.md
- OBJETIVO: primeira impressão do projeto para qualquer visitante
- DEVE conter badge cluster com 5 badges: crates.io, docs.rs, CI, licença, Contributor Covenant
- DEVE conter hero tagline em blockquote com 15 palavras ou menos
- DEVE conter seção "What is it?" com 6 bullets técnicos
- DEVE conter seção "Why?" com diferencial em 3-4 bullets
- DEVE conter seção "Quick Start" com 4 comandos ou menos
- DEVE conter tabelas de comandos agrupadas por família
- DEVE conter tabela de variáveis de ambiente
- DEVE conter seção "Integration Patterns" com exemplos pipeable
- DEVE conter seção "Exit Codes" com tabela numérica
- DEVE conter seção "Troubleshooting FAQ" com 3-5 problemas
- DEVE conter link para CHANGELOG, nunca changelog inline
- DEVE conter seção "Contributing" apontando para CONTRIBUTING.md
- DEVE conter seção "Security" apontando para SECURITY.md
- DEVE conter seção "License" com identificador SPDX
- NUNCA exceda 900 linhas por versão de idioma
- ESTRUTURA README segue modelo AIDA (Atenção, Interesse, Desejo, Ação)

#### CHANGELOG.md + CHANGELOG.pt-BR.md
- OBJETIVO: registro cronológico reverso de todas as mudanças por versão
- DEVE seguir formato Keep a Changelog (https://keepachangelog.com/en/1.1.0/)
- DEVE agrupar por: Added, Changed, Fixed, Removed, Security, Deprecated
- DEVE incluir data de release em formato ISO 8601
- DEVE incluir número de arquivos alterados por release
- DEVE incluir contagem de bugs corrigidos e features novas no heading
- NUNCA omita uma versão publicada do changelog
- NUNCA registre mudanças internas invisíveis ao usuário

#### CONTRIBUTING.md + CONTRIBUTING.pt-BR.md
- OBJETIVO: onboarding de novos contribuidores com fluxo completo
- DEVE conter seção "Welcome" com tom inclusivo
- DEVE conter seção "Quick Start" com passos de setup
- DEVE conter seção "Development Setup" com requisitos de toolchain
- DEVE conter seção "Branching Strategy" com convenção de branches
- DEVE conter seção "Commit Convention" com formato de mensagens
- DEVE conter seção "Pull Request Process" com checklist de validação
- DEVE conter seção "Testing" com comandos de teste
- DEVE conter seção "Documentation" com política de docs
- DEVE conter seção "How to Report Bugs" com template
- DEVE conter seção "How to Request Features" com template
- DEVE conter seção "Release Process" com fluxo de publicação
- NUNCA exceda 150 linhas por versão de idioma

#### CODE_OF_CONDUCT.md + CODE_OF_CONDUCT.pt-BR.md
- OBJETIVO: estabelecer padrões de comportamento da comunidade
- DEVE adotar Contributor Covenant 2.1 como base
- DEVE conter badge do Contributor Covenant
- DEVE conter informações de contato para reportar violações
- DEVE conter seções de escopo, enforcement e atribuição
- NUNCA modifique o texto padrão do Contributor Covenant sem justificativa

#### SECURITY.md + SECURITY.pt-BR.md
- OBJETIVO: canal de comunicação para vulnerabilidades de segurança
- DEVE conter seção "Supported Versions" com tabela de versões ativas
- DEVE conter seção "Reporting a Vulnerability" com instruções claras
- DEVE conter seção "Response SLA" com tempos de resposta
- DEVE conter seção "Fix SLA by CVSS Severity" com prazos por gravidade
- DEVE conter seção "Disclosure Policy" com política de divulgação
- DEVE conter seção "Best Practices for Users" com orientações
- NUNCA exceda 80 linhas por versão de idioma

#### INTEGRATIONS.md + INTEGRATIONS.pt-BR.md
- OBJETIVO: catálogo completo de plataformas, agentes e ferramentas compatíveis
- DEVE conter tabela sumária com todas as integrações
- DEVE conter seção dedicada por integração com: nome, tipo de agente, método de integração
- DEVE conter exemplos de configuração para cada integração
- DEVE cobrir: agentes de IA, IDEs, CI/CD, containers, package managers, shells
- DEVE agrupar integrações por categoria (agentes, IDEs, CI/CD, etc.)
- NUNCA liste integração sem exemplo funcional

#### llms.txt + llms.pt-BR.txt
- OBJETIVO: resumo compacto otimizado para descoberta por agentes de IA
- SEGUE o padrão llms.txt (https://llmstxt.org/)
- DEVE conter título H1 com nome do projeto
- DEVE conter blockquote hero com proposta de valor em uma frase
- DEVE conter parágrafo de abertura com números concretos (agentes, tamanho, latência)
- DEVE conter seção "Primary Documentation" com links para docs principais
- DEVE conter seção "Core Commands" com lista completa de subcomandos
- DEVE conter seção "Environment Variables" com todas as variáveis
- DEVE conter seção "Exit Codes" com tabela numérica
- DEVE conter seção "Stable Facts" com fatos verificáveis e estáveis
- NUNCA exceda 150 linhas
- NUNCA inclua detalhes de implementação interna
- TRATE este arquivo como cartão de visita do projeto para LLMs

#### llms-full.txt
- OBJETIVO: documentação completa inline para contexto expandido de LLMs
- DEVE conter TODA a informação do README + HOW_TO_USE + COOKBOOK condensados
- DEVE ser autocontido — um LLM DEVE conseguir operar o projeto lendo APENAS este arquivo
- DEVE incluir Quick Start, todos os comandos, variáveis de ambiente, padrões de integração
- DEVE incluir exemplos de uso para cada comando principal
- PODE exceder 500 linhas quando necessário para completude
- NUNCA exija leitura de arquivo externo para operar o projeto
- VERSÃO única em inglês (sem par PT-BR) — inglês é lingua franca de LLMs

#### gaps.md
- OBJETIVO: relatório de acceptance testing com gaps identificados por versão
- DEVE conter resultado agregado (X/Y PASS + N FINDINGs)
- DEVE conter versão do binário e estado do banco de produção
- DEVE conter cada gap com: classificação de severidade (HIGH, MEDIUM, LOW)
- CADA gap DEVE conter seções: Problem, Consequences, Root Cause, Solution, Benefits, How to Resolve
- DEVE ser atualizado a cada release com nova rodada de acceptance testing
- VERSÃO única em inglês — documento técnico interno


## Camada 2 — Pasta docs/ (14 arquivos MD + subpasta schemas/)

### OBRIGATÓRIO — Inventário Completo da Pasta docs/
- `docs/AGENTS.md` + `docs/AGENTS.pt-BR.md` — Guia completo para integração com agentes de IA
- `docs/COOKBOOK.md` + `docs/COOKBOOK.pt-BR.md` — Receitas práticas de produção
- `docs/CROSS_PLATFORM.md` + `docs/CROSS_PLATFORM.pt-BR.md` — Suporte cross-platform
- `docs/HOW_TO_USE.md` + `docs/HOW_TO_USE.pt-BR.md` — Guia de uso completo
- `docs/MIGRATION.md` + `docs/MIGRATION.pt-BR.md` — Guia de migração entre versões
- `docs/TESTING.md` + `docs/TESTING.pt-BR.md` — Guia de testes e estratégia de QA
- `docs/HEADLESS_INVOCATION.md` + `docs/HEADLESS_INVOCATION.pt-BR.md` — Referência canônica de invocação headless OAuth-safe (adicionado na v1.0.76)
- `docs/DOCUMENTATION_FRAMEWORK.md` — Este próprio framework (versão única EN, referencia regras de PT-BR indiretamente)
- `docs/schemas/README.md` — Índice e documentação dos JSON Schemas (bilíngue inline)
- `docs/schemas/*.schema.json` — Um schema JSON Draft 2020-12 por subcomando
- `docs/decisions/adr-NNNN-*.md` — Architectural Decision Records (ADRs) documentando decisões de design v1.0.x

### Mudanças na Camada 2 a Partir da v1.0.76
- Adicionados `docs/HEADLESS_INVOCATION.md` + versão PT-BR (promovidos do gaps.md)
- 2 novos schemas JSON para `migrate --rehash` e `migrate --to-llm-only`
- `docs/AGENTS.md` ganhou seção "v1.0.76 Architecture (LLM-Only)" e "OAuth Enforcement"
- `docs/TESTING.md` ganhou seção "v1.0.76 Test Infrastructure — 3-Feature CI Matrix"
- `docs/COOKBOOK.md` ganhou receita "How To Upgrade From v1.0.74 Or v1.0.75 To v1.0.76"
- `docs/MIGRATION.md` reescrito do zero para a breaking change v1.0.76
- `docs/HOW_TO_USE.md` reescrito do zero para LLM-Only One-Shot
- 7 novos ADRs (0019-0025) cobrindo a arquitetura v1.0.76, todos com versão PT-BR
- ADR 0026 documenta o drift de migração V002 (PT-BR incluso)

### Mudanças na Camada 2 a Partir da v1.0.77
- ADR-0027 documenta a correção do G40 (`applied_on = NULL` bloqueava migrações), com versão PT-BR
- `docs/schemas/migrate-rehash.schema.json` atualizado com campo `null_rows_fixed`
- `docs/schemas/migrate-to-llm-only.schema.json` atualizado com campos `null_rows_fixed` e `vec_tables_removed_via_writable_schema`
- `docs/schemas/debug-schema.schema.json` atualizado: `applied_on` agora aceita `null` (tipo `["string", "null"]`)
- `docs/AGENTS.md` ganhou seção "New in v1.0.77" cobrindo o G40 fix
- `docs/TESTING.md` ganhou seção "v1.0.77 Test Additions — G40 Fix Coverage"
- `docs/COOKBOOK.md` ganhou subseção "v1.0.77 Fix" na receita de upgrade
- `docs/MIGRATION.md` ganhou seção "MIGRATING TO v1.0.77 — G40 Fix" no topo

### Mudanças na Camada 2 a Partir da v1.0.78
- ADR-0028 documenta a correção do G41 (`run_rehash` registrava V013 sem executar SQL), com versão PT-BR
- `docs/schemas/migrate-rehash.schema.json` atualizado com campo `v013_tables_created`
- `docs/schemas/migrate-to-llm-only.schema.json` atualizado com campo `v013_tables_created`
- `docs/AGENTS.md` ganhou seção "New in v1.0.78" cobrindo o G41 fix
- `docs/AGENTS.pt-BR.md` ganhou seção "Novidades na v1.0.78" cobrindo o G41 fix
- `docs/TESTING.md` ganhou seção "v1.0.78 Test Additions — G41 Fix Coverage"
- `docs/TESTING.pt-BR.md` ganhou seção correspondente em português
- `docs/COOKBOOK.md` ganhou subseção "v1.0.78 Fix" na receita de upgrade
- `docs/COOKBOOK.pt-BR.md` ganhou subseção correspondente em português
- `docs/MIGRATION.md` ganhou seção "MIGRATING TO v1.0.78 — G41 Phantom V013 Registration Fix" no topo
- `docs/MIGRATION.pt-BR.md` ganhou seção correspondente em português
- `README.md` e `README.pt-BR.md` atualizados para "Current release: v1.0.78"
- `docs/MIGRATION.pt-BR.md` ganhou seção correspondente em português
- `README.md` e `README.pt-BR.md` atualizados para "Current release: v1.0.78"

### Mudanças na Camada 2 a Partir da v1.0.79
- ADR-0019-0026 cobertos na seção anterior (v1.0.76/v1.0.77/v1.0.78)
- Pipeline de embedding LLM fechado pelo G42 (S1 dim configurável, S2 batching, S3 bounded parallelism, S4 tempfile RAII, S5 modelo env, S6 empty CLAUDE_CONFIG_DIR, S7 codex headless actionable, S8 panic-free signal handler, S9 canonical re-embed)
- G43 dim-adoption em `open_rw` e `open_ro`; mocks de teste reescritos para 64 dims + batch schema
- G44 dim-adaptive batch size via `clamp(base×64/dim, 1, base)`
- G50 CI vermelho fechado: 6 causas (doctest, mock inline, benchmark LLM, language policy, race de dim, deny obsoleto)
- G51 mocks LLM extraem dim do prompt para testes multi-dim
- G52 `vec stats` ganhou `dims: [{table, dim, rows}]`; schema fiel ao binário
- G47 flags documentadas inexistentes: aliases visíveis para `--type` em `edit` e `--entity-type` em `reclassify`
- G48 G20 não cegava `--max-hops` igual ao default (Option<T>)
- G49 `SQLITE_GRAPHRAG_EMBEDDING_DIM` inválido emite `tracing::warn!`
- Daemon infrastructure e features legadas (`embedding-legacy`, `ner-legacy`, `full`) totalmente removidas
- `docs/AGENTS.md` e `docs/AGENTS.pt-BR.md` ganharam seções "v1.0.79" cobrindo G42-G52 e a remoção do daemon
- `docs/TESTING.md` e `docs/TESTING.pt-BR.md` ganharam seções "v1.0.79 Test Additions"
- `docs/COOKBOOK.md` e `docs/COOKBOOK.pt-BR.md` ganharam subseções "v1.0.79 Fix" nas receitas de upgrade
- `docs/MIGRATION.md` ganhou receita de re-embed com `enrich --operation re-embed --limit N --resume` (substituindo a receita quebrada `edit --description`)
- `README.md` e `README.pt-BR.md` atualizados para "Current release: v1.0.79"

### Mudanças na Camada 2 a Partir da v1.0.80
- **ADR-0032 (G53, v1.0.80) — Library API Stability Policy**: CLI é contrato estável; API da biblioteca é instável em v1.x.y. Consumidores da lib devem fixar `=1.0.80`; bump de patch é estritamente aditivo na superfície da lib. Documentado em `docs/decisions/adr-0032-g53-lib-api-policy.md` e em `docs/decisions/adr-0032-g53-lib-api-policy.pt-BR.md`
- **ADR-0033 (G53-WINDOWS-INFRA, v1.0.80) — Windows CI Resilience**: jobs `clippy` e `test` da matrix windows-2025 ganharam steps de pre-warm e verify gateados em `if: matrix.os == 'windows-2025'`. Os 2 modos históricos de falha de infra (rustup download transitório e `E0463` por stdlib ausente) agora são recuperáveis na primeira re-run. Documentado em `docs/decisions/adr-0033-g53-windows-infra-resilience.md` e versão PT-BR
- **ADR-0034 (SHUTDOWN Resilience, v1.0.80) — Panic-Free Third-Signal Exit**: `src/signals.rs` é envolvido em uma barreira de captura de panic; o terceiro Ctrl-C consecutivo sai com código 130 e ZERO I/O. Receita canônica de bypass SHUTDOWN em 3 camadas (`nohup` → `setsid` → `disown`) documentada em `docs/HEADLESS_INVOCATION.md` e `docs/COOKBOOK.md`. Documentado em `docs/decisions/adr-0034-shutdown-resilience.md` e versão PT-BR
- **ADR-0041 (Custom Provider Credential Preservation, v1.0.83) — Shared env_whitelist helper**: 6 env vars de custom-provider (`ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT`) preservadas ao spawnar subprocessos LLM, habilitando Minimax/OpenRouter/AWS Bedrock/gateways corporativos sem alterar o mandato OAuth-only que continua rejeitando `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`. Helper compartilhado `src/spawn/env_whitelist.rs` elimina duplicação dos 3 spawners. Flag opt-out `--strict-env-clear` / `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` para compliance (PCI-DSS/SOC2/HIPAA). SEM telemetria nova. Documentado em `docs/decisions/adr-0041-preserve-custom-provider-env.md` e versão PT-BR
- `docs/MIGRATION.md` e `docs/MIGRATION.pt-BR.md` ganharam seção "MIGRATING TO v1.0.80" no topo (sem migração de banco, apenas bump de versão e nota sobre pin de lib)
- `docs/CROSS_PLATFORM.md` e `docs/CROSS_PLATFORM.pt-BR.md` ganharam subseção "CI Windows Infra Resilience (G53-WINDOWS-INFRA, ADR-0033, v1.0.80)" após a seção HANDLE
- `README.md` e `README.pt-BR.md` ganharam bullet "Upgrading from v1.0.79 to v1.0.80?" e badge "Current release: v1.0.80"
- `CHANGELOG.md` e `CHANGELOG.pt-BR.md` ganharam entradas para G45 (cross-process embedding singleton), G53 (stability policy + semver-checks CI), G55 S2 (MemoryNotFound estrutural), G56 (entity-embed cache), G58 (FTS5 fallback), G53-WINDOWS-INFRA e SHUTDOWN resilience



### Objetivo e Entrega de Cada Arquivo da Pasta docs/

#### docs/AGENTS.md + docs/AGENTS.pt-BR.md
- OBJETIVO: guia exaustivo para agentes de IA consumirem o projeto como ferramenta
- DEVE conter hero tagline idêntica ao README
- DEVE conter seção "Why Agents Love This CLI" com benefícios de máquina
- DEVE conter seção "Compatible Agents and Orchestrators" com lista completa
- DEVE conter seção "Agent Integration Details" com exemplos por agente
- DEVE conter TODA a referência de CRUD (Create, Read, Update, Delete)
- DEVE conter TODA a referência de pesquisa (recall, hybrid-search, related, graph traverse, deep-research)
- DEVE conter referência de grafo (link, unlink, entities, stats, traverse)
- DEVE conter referência de manutenção (comandos `cache` e `daemon` removidos na v1.0.76; código restante do daemon deletado na v1.0.79)
- DEVE conter contrato JSON completo com campos por comando
- DEVE conter exit codes com estratégia de retry
- DEVE conter seção de concorrência e recursos
- DEVE ser AUTOCONTIDO — um agente DEVE operar o projeto lendo APENAS este arquivo
- NUNCA exija leitura de outro arquivo para completude operacional
- ENTREGA: um agente de IA CONSEGUE usar o projeto end-to-end lendo apenas AGENTS.md

#### docs/COOKBOOK.md + docs/COOKBOOK.pt-BR.md
- OBJETIVO: receitas práticas prontas para copiar e executar
- DEVE conter seção "CLI Flag Aliases" com tabela de aliases
- DEVE conter seção "Default Values Reference" com valores padrão
- CADA receita DEVE seguir formato "How To [Verbo] [Objeto] [Contexto]"
- CADA receita DEVE conter bloco de código executável copiar-colar
- CADA receita DEVE ser independente das demais
- DEVE cobrir: bootstrap, ingest, search, graph, integração com agentes, backup, export, debug
- DEVE incluir receitas de integração para cada agente suportado
- DEVE incluir receitas de operações avançadas (merge, rename, reclassify, prune)
- ENTREGA: um operador RESOLVE qualquer tarefa comum copiando uma receita

#### docs/CROSS_PLATFORM.md + docs/CROSS_PLATFORM.pt-BR.md
- OBJETIVO: documentar suporte e particularidades de cada plataforma
- DEVE conter tabela de targets suportados com status
- DEVE conter instruções de instalação por plataforma
- DEVE conter particularidades de runtime por OS (subprocesso LLM, musl, ARM64)
- DEVE conter seção de CI/CD com matrix de targets
- ENTREGA: um desenvolvedor CONFIGURA build e CI para qualquer target lendo este arquivo

#### docs/HOW_TO_USE.md + docs/HOW_TO_USE.pt-BR.md
- OBJETIVO: guia narrativo de uso do início ao domínio completo
- DEVE conter hero tagline com proposta de valor
- DEVE conter links de navegação para README e outros docs
- DEVE cobrir: instalação, inicialização, operações CRUD, busca, grafo
- DEVE seguir progressão de complexidade crescente
- DEVE incluir exemplos com saída esperada
- ENTREGA: um novo usuário SAI operando o projeto após ler este arquivo

#### docs/MIGRATION.md + docs/MIGRATION.pt-BR.md
- OBJETIVO: guia de migração entre versões ou nomes do projeto
- DEVE conter tabela "What Changes" com antes/depois
- DEVE conter instruções passo-a-passo de migração
- DEVE conter seção de rollback para caso de problemas
- DEVE conter breaking changes com impacto e solução
- ENTREGA: um usuário MIGRA entre versões sem perda de dados lendo este arquivo

#### docs/TESTING.md + docs/TESTING.pt-BR.md
- OBJETIVO: guia de estratégia de testes e como executar a suíte
- DEVE conter motivação para categorização de testes
- DEVE conter categorias de teste (unitário, integração, contrato, E2E)
- DEVE conter comandos exatos para executar cada categoria
- DEVE conter política de cobertura mínima
- DEVE conter instruções para adicionar novos testes
- DEVE conter seção "Test Matrix" com a matriz CI de features vigente (`default` e `llm-only` desde a v1.0.79; `embedding-legacy` removida)
- DEVE conter o contrato da Mock LLM CLI para rodar testes sem credenciais OAuth reais
- ENTREGA: um contribuidor ESCREVE e EXECUTA testes seguindo este guia

#### docs/HEADLESS_INVOCATION.md + docs/HEADLESS_INVOCATION.pt-BR.md
- OBJETIVO: referência canônica de invocação headless OAuth-safe de Claude Code, Codex CLI e OpenCode
- DEVE conter tabela comparativa dos interruptores de MCP e Hooks por CLI
- DEVE conter os comandos exatos de hardening flags para cada CLI
- DEVE conter seção de "Por Que NÃO Usar `--bare`" para Claude
- DEVE conter ressalvas de bugs conhecidos (issue #14490 do Claude, issue #3441 do Codex)
- DEVE ser referenciado em `docs/HOW_TO_USE.md` e `docs/AGENTS.md` para usuários finais
- ENTREGA: um operador invoca LLM headless sem herdar MCPs ou hooks lendo este arquivo

#### docs/schemas/README.md
- OBJETIVO: índice e documentação de todos os JSON Schemas do projeto
- DEVE ser bilíngue inline (seção EN seguida de seção PT-BR no mesmo arquivo)
- DEVE conter tabela mapeando subcomando para arquivo de schema
- DEVE conter seção de seleção de schema por modo de ingestão
- DEVE conter seção de schemas de input (payloads de entrada)
- DEVE conter seção de uso com exemplos de validação
- DEVE conter seção de comportamento de flags
- DEVE conter garantia de estabilidade (política SemVer de schemas)
- ENTREGA: um integrador VALIDA qualquer output do CLI contra o schema correto

#### docs/schemas/*.schema.json
- OBJETIVO: contrato formal de cada subcomando em JSON Schema Draft 2020-12
- DEVE haver exatamente UM arquivo `.schema.json` por subcomando ou evento NDJSON
- DEVE usar `"additionalProperties": false` em todos os schemas
- DEVE documentar TODOS os campos obrigatórios e opcionais
- NOME do arquivo DEVE ser kebab-case do nome do subcomando: `nome-comando.schema.json`
- SUBCOMANDOS com modos DEVEM ter schemas separados por modo: `ingest-file-event.schema.json` vs `ingest-claude-file-event.schema.json`
- DEVE incluir schema de error envelope: `error-envelope.schema.json`
- DEVE incluir schemas de input: `entities-input.schema.json`, `relationships-input.schema.json`
- ENTREGA: qualquer parser ou agente VALIDA output do CLI programaticamente


## Camada 3 — Pasta skill/ (2 pastas, 2 arquivos SKILL.md)

### OBRIGATÓRIO — Inventário Completo da Pasta skill/
- `skill/<nome-projeto>-en/SKILL.md` — Skill de instrução para agentes de IA em inglês
- `skill/<nome-projeto>-pt/SKILL.md` — Skill de instrução para agentes de IA em português

### OBRIGATÓRIO — Estrutura de Diretório da Skill
- CADA idioma em pasta separada com sufixo `-en` ou `-pt`
- DENTRO de cada pasta, exatamente UM arquivo chamado `SKILL.md`
- NOME da pasta segue padrão: `<nome-do-projeto>-<idioma>`
- NUNCA misture idiomas na mesma pasta
- NUNCA nomeie o arquivo diferente de `SKILL.md`

### OBRIGATÓRIO — Estrutura do Arquivo SKILL.md
- DEVE iniciar com YAML frontmatter delimitado por `---`
- Frontmatter DEVE conter campo `name:` com nome do projeto
- Frontmatter DEVE conter campo `description:` com texto de trigger para agentes de IA
- O campo `description` DEVE ser otimizado para matching por LLMs — incluir sinônimos, keywords, nomes de agentes, cenários de uso
- O campo `description` DEVE incluir condições de auto-invocação mesmo sem menção explícita
- Após o frontmatter, o corpo DEVE conter TODA a referência operacional do projeto
- O corpo DEVE usar estrutura de headings H2/H3 com labels imperativas (REQUIRED, FORBIDDEN, Correct Pattern)

### OBRIGATÓRIO — Conteúdo do SKILL.md
- DEVE conter seção "Fundamental Principles" com filosofia de uso
- DEVE conter seção "Initialization and Health Check" com bootstrap
- DEVE conter seção "Global Configuration" com todas as variáveis e flags
- DEVE conter TODAS as operações CRUD documentadas individualmente
- DEVE conter TODAS as operações de pesquisa (search, recall, traverse)
- DEVE conter referência de grafo (link, unlink, entities, stats)
- DEVE conter gerenciamento de entidades (delete, rename, reclassify, merge)
- DEVE conter contrato JSON completo com campos críticos por comando
- DEVE conter exit codes com estratégia de retry
- DEVE conter seção de concorrência e recursos
- DEVE conter seção de manutenção e backup
- DEVE ser AUTOCONTIDO — injetado como system prompt, o agente DEVE operar sem ler mais nada

### OBRIGATÓRIO — Linguagem Imperativa do SKILL.md
- USE headings H3 com prefixo de categoria: `### REQUIRED —`, `### FORBIDDEN —`, `### Correct Pattern —`
- USE bullets iniciando com VERBO IMPERATIVO em MAIÚSCULAS: `USAR`, `NUNCA`, `EXECUTAR`, `TRATAR`
- USE negações absolutas: `NUNCA`, `JAMAIS`, `PROIBIDO`
- USE afirmações absolutas: `SEMPRE`, `OBRIGATÓRIO`, `DEVE`
- NUNCA use linguagem sugestiva ("considere", "talvez", "recomendado")
- NUNCA use voz passiva
- CADA bullet DEVE ser uma regra independente e atômica

### Objetivo e Entrega do SKILL.md
- OBJETIVO: transformar qualquer agente de IA em operador competente do projeto
- PÚBLICO: LLMs e agentes de IA (Claude Code, Codex, Cursor, Windsurf, etc.)
- FORMATO: markdown com YAML frontmatter, otimizado para injeção em system prompts
- ENTREGA: um agente de IA que recebe SKILL.md como contexto OPERA o projeto end-to-end sem assistência humana


## Relação Entre as 3 Camadas

### OBRIGATÓRIO — Hierarquia de Completude
- Camada 1 (RAIZ): informações de alto nível, onboarding, governança do projeto
- Camada 2 (DOCS): documentação técnica profunda, receitas, guias operacionais
- Camada 3 (SKILL): instrução máquina-para-máquina, autocontida e imperativa

### OBRIGATÓRIO — Progressão de Audiência
- README.md → qualquer visitante (30 segundos para entender o projeto)
- AGENTS.md → integrador técnico (opera o projeto via agente de IA)
- COOKBOOK.md → operador avançado (resolve tarefas específicas via receitas)
- SKILL.md → agente de IA (opera o projeto autonomamente sem humano)

### OBRIGATÓRIO — Regra de Autocontenção
- README.md DEVE ser suficiente para decidir se o projeto é relevante
- AGENTS.md DEVE ser suficiente para integrar o projeto com qualquer agente
- COOKBOOK.md DEVE ser suficiente para resolver qualquer tarefa operacional
- HOW_TO_USE.md DEVE ser suficiente para um novo usuário operar o projeto
- SKILL.md DEVE ser suficiente para um agente de IA operar o projeto
- llms-full.txt DEVE ser suficiente para um LLM entender o projeto completamente
- NENHUM arquivo DEVE exigir leitura de outro para cumprir seu objetivo primário

### OBRIGATÓRIO — Sobreposição Intencional
- README.md, AGENTS.md, COOKBOOK.md, SKILL.md e llms-full.txt PODEM repetir informação
- A repetição É INTENCIONAL — cada arquivo serve audiência diferente em contexto diferente
- NUNCA substitua conteúdo por "veja arquivo X" quando a audiência-alvo pode não ter acesso ao arquivo X
- SEMPRE prefira redundância sobre referência cruzada em documentos autocontidos


## Convenções de Formatação

### OBRIGATÓRIO — Headings
- H1 (`#`) SOMENTE para título do documento (uma vez por arquivo)
- H2 (`##`) para seções principais
- H3 (`###`) para subseções com prefixo de categoria
- NUNCA use H4 ou inferior — reestruture a hierarquia
- NUNCA use heading sem conteúdo abaixo

### OBRIGATÓRIO — Hero Tagline
- CADA documento DEVE ter blockquote hero após H1
- Formato: `> proposta de valor em 15 palavras ou menos`
- POSICIONE imediatamente após badges (se houver) e antes de qualquer conteúdo

### OBRIGATÓRIO — Badges (apenas README)
- MÍNIMO 5 badges: crates.io, docs.rs, CI, licença, Contributor Covenant
- POSICIONE imediatamente após H1
- USE formato shields.io para uniformidade
- ORDEM: registry, docs, CI, licença, código de conduta

### PROIBIDO — Formatação
- NUNCA use emojis em documentação técnica
- NUNCA use negrito com asteriscos duplos para ênfase
- NUNCA use separador horizontal de três hífens (`---`) exceto em frontmatter
- NUNCA use HTML inline em markdown
- NUNCA use imagens sem alt-text descritivo

### OBRIGATÓRIO — Estilo de Escrita
- CADA bullet DEVE ter entre 8 e 15 palavras
- USE verbos no imperativo
- ELIMINE advérbios e conectores parasitas
- SUBSTITUA "pode" por "entrega", "garante", "elimina"
- SUBSTITUA "é recomendado" por DEVE ou SEMPRE
- SUBSTITUA "evite" por PROIBIDO ou JAMAIS
- USE números concretos em vez de qualificadores vagos


## Omissões Detectadas no Projeto Modelo — Gaps Estruturais

### STATUS LEGADO — Gaps identificados e corrigidos em versões anteriores
- As três omissões abaixo foram DETECTADAS e CORRIGIDAS antes do v1.0.68
- Mantidas aqui como referência histórica do que o framework exige
- Projetos novos DEVEM satisfazer as três regras desde o primeiro release
- Esta seção NÃO descreve o estado atual do projeto; o estado atual está em `gaps.md`

### STATUS LEGADO — README.md e README.pt-BR.md NÃO continham cross-reference bilíngue
- O README.md NÃO continha link para README.pt-BR.md na primeira linha útil
- O README.pt-BR.md NÃO continha link para README.md na primeira linha útil
- TODOS os outros pares bilíngues (CONTRIBUTING, SECURITY, etc.) já continham o cross-reference
- REGRA: README.md DEVE conter `Read this document in [Portuguese (pt-BR)](README.pt-BR.md).` após badges
- REGRA: README.pt-BR.md DEVE conter `Leia este documento em [inglês (EN)](README.md).` após badges
- CORREÇÃO aplicada no projeto modelo antes do v1.0.68

### STATUS LEGADO — INTEGRATIONS.md e INTEGRATIONS.pt-BR.md NÃO continham cross-reference bilíngue
- O INTEGRATIONS.md NÃO continha link para INTEGRATIONS.pt-BR.md
- O INTEGRATIONS.pt-BR.md NÃO continha link para INTEGRATIONS.md
- REGRA: INTEGRATIONS.md DEVE conter `Read this document in [Portuguese (pt-BR)](INTEGRATIONS.pt-BR.md).`
- REGRA: INTEGRATIONS.pt-BR.md DEVE conter `Leia este documento em [inglês (EN)](INTEGRATIONS.md).`
- CORREÇÃO aplicada no projeto modelo antes do v1.0.68

### STATUS LEGADO — Ausência de GitHub Issue e PR Templates
- O projeto NÃO continha `.github/ISSUE_TEMPLATE/` com templates de bug report e feature request
- O projeto NÃO continha `.github/PULL_REQUEST_TEMPLATE.md` com checklist de PR
- REGRA: TODO projeto open-source DEVE conter templates de issue e PR no GitHub
- CORREÇÃO aplicada no projeto modelo antes do v1.0.68 — ver `gaps.md` entrada de resolução v1.0.68


## Camada Auxiliar — CI/CD Workflows (.github/workflows/)

### OBRIGATÓRIO — Inventário de Workflows
- `.github/workflows/ci.yml` — Pipeline de validação em push e PR
- `.github/workflows/release.yml` — Pipeline de build e publicação em tags `v*`
- NUNCA publique release sem workflow de CI passando
- NUNCA publique sem workflow de release automatizado

### OBRIGATÓRIO — ci.yml
- DEVE executar: fmt, clippy, test, doc, audit, deny em matrix multi-OS
- DEVE incluir job `msrv` para validar MSRV declarado
- DEVE incluir job `language-check` para auditoria de idioma no código
- DEVE incluir job `commit-check` para bloquear Co-authored-by de agentes

### OBRIGATÓRIO — release.yml
- DEVE triggerar em tags `v*`
- DEVE incluir: validate, build-matrix, publish-github-release, publish-crates-io
- DEVE gerar binários para: linux-gnu, linux-musl, macos-arm64, macos-x86, windows-msvc
- DEVE gerar SHA256SUMS.txt para verificação de integridade


## Camada Auxiliar — Pastas de Suporte

### OBRIGATÓRIO — Pasta migrations/
- DEVE conter migrações SQL versionadas para projetos com banco de dados
- FORMATO de nome: `V<NNN>__<descricao_snake_case>.sql`
- NUMERAÇÃO sequencial sem gaps
- CADA migração DEVE ser idempotente ou com rollback documentado

### OBRIGATÓRIO — Pasta scripts/
- DEVE conter scripts auxiliares de desenvolvimento e auditoria
- NOMEIE scripts em inglês com kebab-case ou snake_case
- DOCUMENTE propósito de cada script no primeiro comentário

### OBRIGATÓRIO — Pasta benches/
- DEVE conter benchmarks com `criterion` para projetos com requisitos de performance
- NOMEIE benchmarks em inglês com snake_case
- INCLUA benchmark de regressão como baseline


## Padrões de Cross-Reference Entre Arquivos

### OBRIGATÓRIO — README Aponta para Docs
- README.md DEVE conter links para: CONTRIBUTING.md, SECURITY.md, CHANGELOG.md
- README.md DEVE conter seção "JSON Schemas" apontando para docs/schemas/README.md
- README.md DEVE conter seção "Contributing" apontando para CONTRIBUTING.md
- README.md DEVE conter seção "Security" apontando para SECURITY.md

### OBRIGATÓRIO — Docs Apontam para README
- CADA arquivo em docs/ DEVE conter link de volta ao README.md principal
- Formato: `Return to the main [README.md](../README.md) for command reference`
- POSICIONE após hero tagline e cross-reference de idioma

### OBRIGATÓRIO — CHANGELOG Formato de Heading por Release
- Formato: `## [X.Y.Z] - YYYY-MM-DD`
- DEVE incluir seção `[Unreleased]` no topo para mudanças em progresso
- Subseções: `### Added`, `### Changed`, `### Fixed`, `### Removed`, `### Security`, `### Deprecated`
- NUNCA altere heading de release já publicada

### OBRIGATÓRIO — llms.txt Aponta para Docs Primários
- DEVE conter seção "Primary Documentation" com links para:
  - README.md no repositório GitHub
  - docs/AGENTS.md no repositório GitHub
  - docs/COOKBOOK.md no repositório GitHub
  - docs/HOW_TO_USE.md no repositório GitHub
- DEVE usar URLs absolutas do GitHub, não caminhos relativos


## Checklist de Conformidade para Novos Projetos

### OBRIGATÓRIO — Antes do Primeiro Release
- [x] LICENSE + LICENSE-MIT + LICENSE-APACHE criados com textos completos
- [x] README.md + README.pt-BR.md criados com todas as seções obrigatórias e 5 badges
- [x] CHANGELOG.md + CHANGELOG.pt-BR.md criados com formato Keep a Changelog
- [x] CONTRIBUTING.md + CONTRIBUTING.pt-BR.md criados com fluxo completo
- [x] CODE_OF_CONDUCT.md + CODE_OF_CONDUCT.pt-BR.md criados com Contributor Covenant 2.1
- [x] SECURITY.md + SECURITY.pt-BR.md criados com SLAs definidas
- [x] INTEGRATIONS.md + INTEGRATIONS.pt-BR.md criados com catálogo inicial
- [x] llms.txt + llms.pt-BR.txt criados com resumo compacto
- [x] llms-full.txt criado com documentação inline completa
- [x] gaps.md criado com primeira rodada de acceptance testing
- [x] docs/AGENTS.md + docs/AGENTS.pt-BR.md criados com referência autocontida
- [x] docs/COOKBOOK.md + docs/COOKBOOK.pt-BR.md criados com receitas iniciais
- [x] docs/CROSS_PLATFORM.md + docs/CROSS_PLATFORM.pt-BR.md criados com targets
- [x] docs/HOW_TO_USE.md + docs/HOW_TO_USE.pt-BR.md criados com guia narrativo
- [x] docs/MIGRATION.md + docs/MIGRATION.pt-BR.md criados (mesmo que vazio para v1)
- [x] docs/TESTING.md + docs/TESTING.pt-BR.md criados com estratégia de testes
- [x] docs/schemas/README.md criado bilíngue inline com índice de schemas
- [x] docs/schemas/*.schema.json criados para cada subcomando com saída JSON
- [x] skill/<projeto>-en/SKILL.md criado com referência operacional completa
- [x] skill/<projeto>-pt/SKILL.md criado espelhando versão EN
- [x] .github/workflows/ci.yml criado com pipeline de validação multi-OS
- [x] .github/workflows/release.yml criado com pipeline de publicação em tags
- [x] .github/ISSUE_TEMPLATE/ criado com templates de bug e feature request
- [x] .github/PULL_REQUEST_TEMPLATE.md criado com checklist de validação
- [x] TODOS os cross-references entre idiomas verificados em TODOS os pares
- [x] NENHUM arquivo de documentação sem par bilíngue
- [x] NENHUM README ou INTEGRATIONS sem link para versão no outro idioma

### OBRIGATÓRIO — Quando o Checklist Está 100% Concluído
- MARQUE cada item como `[x]` no checklist acima
- A remoção de qualquer item só é permitida quando ele vira legado documentado em `gaps.md`
- Projetos que herdam o template DEVEM copiar o checklist já marcado como ponto de partida
- ADICIONE novos itens quando o framework ganhar regras; nunca remova itens marcados como concluídos

### OBRIGATÓRIO — A Cada Release
- [ ] CHANGELOG.md + CHANGELOG.pt-BR.md atualizados com mudanças da versão
- [ ] README.md + README.pt-BR.md atualizados se houver novos comandos ou variáveis
- [ ] docs/AGENTS.md + docs/AGENTS.pt-BR.md atualizados se houver mudanças de contrato JSON
- [ ] docs/COOKBOOK.md + docs/COOKBOOK.pt-BR.md atualizados se houver novas receitas
- [ ] docs/HOW_TO_USE.md + docs/HOW_TO_USE.pt-BR.md atualizados com novas flags e subcomandos
- [ ] docs/MIGRATION.md + docs/MIGRATION.pt-BR.md atualizados com breaking changes e guia de upgrade
- [ ] docs/TESTING.md + docs/TESTING.pt-BR.md atualizados com novos testes adicionados
- [ ] docs/CROSS_PLATFORM.md + docs/CROSS_PLATFORM.pt-BR.md atualizados se houver mudanças multiplataforma
- [ ] docs/schemas/*.schema.json atualizados se houver mudanças de output JSON
- [ ] docs/schemas/README.md atualizado se houver novos schemas
- [ ] docs/decisions/adr-NNNN-*.md criado para cada decisão arquitetural nova
- [ ] skill/*/SKILL.md atualizados se houver mudanças operacionais
- [ ] llms.txt + llms.pt-BR.txt atualizados se houver mudanças na proposta de valor
- [ ] llms-full.txt atualizado para refletir estado atual completo
- [ ] gaps.md atualizado com nova rodada de acceptance testing
- [ ] INTEGRATIONS.md + INTEGRATIONS.pt-BR.md atualizados se houver novas integrações
- [ ] TODAS as seções "Authentication" e "API keys" revisadas para refletir a OAuth-only enforcement (v1.0.69+)


## Contagem de Referência — Métricas do Projeto Modelo

### Referência de Tamanho por Arquivo (linhas aproximadas)
- README.md: 800-900 linhas
- CHANGELOG.md: cresce a cada release (~100 linhas por release)
- CONTRIBUTING.md: 120-150 linhas
- CODE_OF_CONDUCT.md: 80-100 linhas
- SECURITY.md: 60-80 linhas
- INTEGRATIONS.md: 400-500 linhas (cresce com integrações)
- llms.txt: 120-150 linhas
- llms-full.txt: 500-600 linhas
- gaps.md: variável por release
- docs/AGENTS.md: 1200-1300 linhas
- docs/COOKBOOK.md: 1700-1800 linhas
- docs/HOW_TO_USE.md: 700-750 linhas
- docs/CROSS_PLATFORM.md: 200-210 linhas
- docs/MIGRATION.md: 250-300 linhas
- docs/TESTING.md: 220-240 linhas
- docs/schemas/README.md: 120-130 linhas
- skill/*/SKILL.md: 800-850 linhas

### Referência de Contagem de Schemas
- UM schema `.json` por subcomando que emite JSON no stdout
- UM schema `.json` por tipo de evento NDJSON (file-event, summary, phase)
- UM schema `error-envelope.schema.json` universal
- Schemas de input para payloads de entrada (entities-input, relationships-input)
- Total típico: 40-60 schemas para um CLI com 30+ subcomandos


## v1.0.82 Documentation Framework Additions
## v1.0.85, v1.0.85.1, v1.0.85.2 — ADR-0042, ADR-0043, ADR-0044 Coverage

Three new ADRs were added in this release cycle:

- **ADR-0042 (v1.0.84)** — Claude Backend Split (GAP-002). Resolves the v1.0.83 synonym bug where `--llm-backend claude` silently fell through to codex. See `adr-0042-claude-backend-split.md` (EN) and `adr-0042-claude-backend-split.pt-BR.md` (PT-BR).
- **ADR-0043 (v1.0.85)** — Five-Gap Remediation. Introduces the 7-variant `FallbackReason` enum, the `reason_code` discriminator, the `try_embed_query_with_deterministic_fallback` retry path, the `anthropic-ratelimit-*-remaining` header capture, the dim=64 lock, and the bilingual `MemoryNotFound` message. See `adr-0043-five-gap-remediation.md` (EN) and `adr-0043-five-gap-remediation.pt-BR.md` (PT-BR).
- **ADR-0044 (v1.0.85.2)** — Hotfixes BUG-001/002/003. Documents `--dry-run-backend` standalone behavior, `embed_via_backend` returning `Result<(Vec<f32>, LlmBackendKind), AppError>`, and the `setup_mock_path()` JSON alignment in `tests/embedder.rs:37-77`. See `adr-0044-hotfixes-bug-001-002-003.md` (EN) and `adr-0044-hotfixes-bug-001-002-003.pt-BR.md` (PT-BR).

All three ADRs follow the standard template (Status, Data, Versão, Autores, Contexto, Decisão, Consequências, Alternativas, Cross-refs). The Status field is "Aceito" (PT-BR) or "Accepted" (EN).

### Schema Index Update

The 7 envelope schemas in `docs/schemas/` (`edit`, `embedding-status`, `enrich-summary`, `hybrid-search`, `ingest-summary`, `recall`, `remember`) now include `backend_invoked: enum ["claude", "codex", "none"]` per ADR-0042. The `recall` and `hybrid-search` schemas additionally include `vec_degraded_reason: Option<String>` per ADR-0043. See `docs/schemas/README.md` for the full index.
### Five New Schemas (GAP-001/002/004/005)
- `docs/schemas/pending-list.schema.json` — `sqlite-graphrag pending list` output (ADR-0036, GAP-001)
- `docs/schemas/embedding-list.schema.json` — `sqlite-graphrag embedding list` and `pending-embeddings list` outputs (ADR-0040, GAP-005)
- `docs/schemas/embedding-status.schema.json` — `sqlite-graphrag embedding status` output (ADR-0040, GAP-005)
- `docs/schemas/slots-status.schema.json` — `sqlite-graphrag slots status` output (ADR-0039, GAP-004)
- `docs/schemas/shutdown-envelope.schema.json` — JSON envelope emitted at exit code 19 (ADR-0037, GAP-002)
- All five new schemas declare `"additionalProperties": false` and use `$id` URLs with the `daniloaguiarbr` owner consistent with the legacy schemas
- All five are referenced in `docs/schemas/README.md` under the new "Schemas Adicionados na v1.0.82 (GAP-001/002/004/005)" section
### Five New ADRs (ADR-0036 through ADR-0040)
- `docs/decisions/adr-0036-pending-memories-staging.md` (and `.pt-BR.md`) — Three-stage remember checkpoint queue (GAP-001)
- `docs/decisions/adr-0037-shutdown-json-envelope.md` (and `.pt-BR.md`) — Shutdown envelope at exit 19 (GAP-002)
- `docs/decisions/adr-0038-llm-backend-user-choice.md` (and `.pt-BR.md`) — `--llm-backend` global flag (GAP-003)
- `docs/decisions/adr-0039-llm-host-slot-semaphore.md` (and `.pt-BR.md`) — fs4 cross-process slot semaphore (GAP-004)
- `docs/decisions/adr-0040-stderr-capture-fallback-chain.md` (and `.pt-BR.md`) — codex OAuth 401 mitigation (GAP-005)
`docs/decisions/adr-0041-preserve-custom-provider-env.md` (and `.pt-BR.md`) — Custom provider env whitelist helper + `--strict-env-clear` flag (GAP-058 partial)
- All five ADRs follow the canonical structure: Contexto, Decisão, Consequências, Alternativas Consideradas, Notas de Transcrição
- All five ADRs link to the relevant JSON schema in the Consequências section
- The pt-BR translations preserve the H2 section count parity with the EN originals
