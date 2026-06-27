# Machine-Readable JSON Schemas


## English
### Purpose
- Each file in this directory is a JSON Schema Draft 2020-12 document
- Output schemas describe the exact stdout contract of every `sqlite-graphrag` subcommand
- Input schemas describe the accepted JSON payloads for file-driven graph ingestion
- Agents and parsers MUST validate responses against these schemas before processing
- Most schemas use `"additionalProperties": false` — unexpected keys are contract violations
- `health.schema.json` (v1.0.89, GAP-E2E-007, ADR-0048) uses `"additionalProperties": true` (Must-Ignore policy per RFC 7493 I-JSON and `rules_rust_json_e_ndjson.md:33`) — unknown keys are accepted to enable schema evolution
- The 17 new fields added in v1.0.89: `vec_memories_missing`, `vec_memories_orphaned`, `sqlite_version`, `mentions_ratio`, `mentions_warning`, `top_relation`, `top_relation_ratio`, `applies_to_ratio`, `relation_concentration_warning`, `super_hub_count`, `super_hub_warning`, `top_hub_entity`, `top_hub_degree`, `hub_warning`, `non_normalized_count`, `normalization_warning`, `fts_query_ok`
- New exit code 16 (`EX_CONFIG`) emitted by `AppError::PreFlightFailed` is documented in v1.0.87 (ADR-0045, GAP-META-005) — see `error-envelope.schema.json` for the structured `PreFlightError` variant details
### Schema Files
| Subcommand | Schema file |
|---|---|
| `init` | `init.schema.json` |
| `remember` (updated v1.0.84, ADR-0042) | `remember.schema.json` |
| `recall` (updated v1.0.84, ADR-0042 / v1.0.85, ADR-0043 enum 7 variants) | `recall.schema.json` |
| `read` | `read.schema.json` |
| `list` | `list.schema.json` |
| `forget` | `forget.schema.json` |
| `purge` | `purge.schema.json` |
| `rename` | `rename.schema.json` |
| `edit` (updated v1.0.84, ADR-0042) | `edit.schema.json` |
| `history` | `history.schema.json` |
| `restore` | `restore.schema.json` |
| `hybrid-search` (updated v1.0.84, ADR-0042 / v1.0.85, ADR-0043 enum 7 variants) | `hybrid-search.schema.json` |
| `deep-research` | `deep-research.schema.json` |
| `health` | `health.schema.json` |
| `migrate` | `migrate.schema.json` |
| `migrate --rehash` (v1.0.76, updated v1.0.77, v1.0.78) | `migrate-rehash.schema.json` |
| `migrate --to-llm-only` (v1.0.76, updated v1.0.77, v1.0.78) | `migrate-to-llm-only.schema.json` |
| `namespace-detect` | `namespace-detect.schema.json` |
| `optimize` | `optimize.schema.json` |
| `stats` | `stats.schema.json` |
| `sync-safe-copy` | `sync-safe-copy.schema.json` |
| `vacuum` | `vacuum.schema.json` |
| `link` | `link.schema.json` |
| `unlink` | `unlink.schema.json` |
| `related` | `related.schema.json` |
| `graph` | `graph.schema.json` |
| `graph traverse` | `graph-traverse.schema.json` |
| `graph stats` | `graph-stats.schema.json` |
| `graph entities` | `graph-entities.schema.json` |
| `cleanup-orphans` | `cleanup-orphans.schema.json` |
| `prune-relations` | `prune-relations.schema.json` |
| `reclassify-relation` | `reclassify-relation.schema.json` |
| `normalize-entities` | `normalize-entities.schema.json` |
| `enrich` (phase event) | `enrich-phase.schema.json` |
| `enrich` (per-item event) | `enrich-item-event.schema.json` |
| `enrich` (summary, updated v1.0.84, ADR-0042) | `enrich-summary.schema.json` |
| `ingest` (per-file event) | `ingest-file-event.schema.json` |
| `ingest` (summary, updated v1.0.84, ADR-0042) | `ingest-summary.schema.json` |
| `ingest --mode claude-code` (phase event) | `ingest-claude-phase.schema.json` |
| `ingest --mode claude-code` (per-file event) | `ingest-claude-file-event.schema.json` |
| `ingest --mode claude-code` (summary) | `ingest-claude-summary.schema.json` |
| `debug-schema` | `debug-schema.schema.json` |
| `fts rebuild` | `fts-rebuild.schema.json` |
| `fts check` | `fts-check.schema.json` |
| `fts stats` | `fts-stats.schema.json` |
| `backup` | `backup.schema.json` |
| `delete-entity` | `delete-entity.schema.json` |
| `reclassify` | `reclassify.schema.json` |
| `merge-entities` | `merge-entities.schema.json` |
| `rename-entity` | `rename-entity.schema.json` |
| `memory-entities` (forward: `--name`) | `memory-entities.schema.json` |
| `memory-entities` (reverse: `--entity`) | `memory-entities-reverse.schema.json` |
| `prune-ner` | `prune-ner.schema.json` |
| `remember-batch` (per-item event) | `remember-batch.schema.json` |
| `remember-batch` (summary) | `remember-batch-summary.schema.json` |
| `export` (per-memory line) | `export-memory-line.schema.json` |
| `export` (summary) | `export-summary.schema.json` |
| `vec orphan-list` (v1.0.69) | `vec-orphan-list.schema.json` |
| `vec purge-orphan` (v1.0.69) | `vec-purge-orphan.schema.json` |
| `vec stats` (v1.0.69) | `vec-stats.schema.json` |
| `codex-models` (v1.0.69) | `codex-models.schema.json` |
| `slots status` (v1.0.82, GAP-004) | `slots-status.schema.json` |
| `pending list` (v1.0.82, GAP-001) | `pending-list.schema.json` |
| `embedding status` (v1.0.82, GAP-005, updated v1.0.84, ADR-0042) | `embedding-status.schema.json` |
| `embedding list` (v1.0.82, GAP-005) | `embedding-list.schema.json` |
| shutdown envelope (v1.0.82, GAP-002) | `shutdown-envelope.schema.json` |
| error envelope (all commands) | `error-envelope.schema.json` |
### Commands Without JSON Schemas
- `completions` emits shell completion scripts (Bash, Zsh, Fish, PowerShell, Elvish) as plain text — no JSON schema applies
- `daemon` was removed in v1.0.76 (remaining code deleted in v1.0.79) — no JSON schema applies (historical)
### Ingest Mode Schema Selection
- `--mode none` and `--mode gliner` (DEPRECATED since v1.0.79: URL-regex only, emits a deprecation warning) use `ingest-file-event.schema.json` and `ingest-summary.schema.json`
- `--mode claude-code` uses `ingest-claude-phase.schema.json`, `ingest-claude-file-event.schema.json`, and `ingest-claude-summary.schema.json`
- Claude-code mode emits additional phase events (validate, scan) before per-file events
- Per-file events in claude-code mode include `entities`, `rels`, and `cost_usd` fields not present in normal ingest
- `--mode codex` (added in v1.0.62) reuses the same NDJSON schema format as `--mode claude-code` — no separate codex schemas needed
- Codex mode emits the same PhaseEvent, FileEvent, and Summary shapes; agents validating claude-code output can reuse those schemas unchanged

### Error Envelope Changes in v1.0.68 (G28-B)
- The `error-envelope.schema.json` `message` field for `code: 75` now has two distinct templates, both routed to the same exit code
- Template A (new since v1.0.68, G28-B): `job <job_type> for namespace '<namespace>' is already running (exit 75); wait for it to finish or pass --wait-job-singleton <SECONDS>` — emitted by `enrich`, `ingest --mode claude-code`, and `ingest --mode codex` when a concurrent invocation holds the singleton
- Template B (legacy): `all <max> concurrency slots occupied after waiting <waited_secs>s (exit 75); use --max-concurrency or wait for other invocations to finish` — emitted by the counting semaphore for any other command
- Agents can disambiguate the two with a regex on `message`: matches `^job ` for Template A and `^all ` for Template B
- The schema itself remains `additionalProperties: false` because variant-specific fields are intentionally NOT serialised to JSON; structured access to `job_type` and `namespace` requires agents to parse the quoted strings inside `message`

### Schema Changes in v1.0.84 (ADR-0042 / GAP-002)
- Seven response schemas gained an OPTIONAL `backend_invoked: enum [claude, codex, opencode, openrouter, none, auto]` field that reports which LLM backend the live embedding path actually invoked (opencode added in v1.0.90)
- Affected envelopes: `embedding-status`, `remember`, `edit`, `recall`, `hybrid-search`, `ingest-summary`, `enrich-summary`
- The field is omitted (not `null`) when no backend was invoked, keeping happy-path envelopes clean
- Agents SHOULD treat `backend_invoked` as the ground truth for which CLI binary ran during the call
### Update (v1.0.85 / ADR-0043)
- Two response schemas gained `vec_degraded_reason` with the seven-variant enum `embedding_failed | slot_exhausted | oauth_quota | backend_mismatch | dim_zero | cancelled | timeout` plus explicit `null` for happy-path. Callers can switch on this discriminator instead of regex against `vec_error` strings.
- Two response schemas also gained `vec_degraded_reason: enum [embedding_failed, cancelled, timeout, null]` for callers that need to distinguish OAuth quota exhaustion from cancellation from timeout
- Affected envelopes: `recall`, `hybrid-search`
- The field is omitted when live embedding succeeded, and explicitly `null` when no degradation path was triggered
- All seven updated schemas keep `"additionalProperties": false`; the new fields are additive and `null`/`omitted` are distinct contract states
- See `docs/decisions/adr-0042-claude-backend-split.md` (EN) and `.pt-BR.md` for the full rationale
### Schema Changes in v1.0.85 (ADR-0043 / five-gap remediation)
- `recall` and `hybrid-search` response schemas extended `vec_degraded_reason` enum from 3 to 7 variants: `embedding_failed | slot_exhausted | oauth_quota | backend_mismatch | dim_zero | cancelled | timeout`
- `slot_exhausted` (GAP-003) discriminates LLM subprocess semaphore contention from quota exhaustion; callers can retry with `SQLITE_GRAPHRAG_LLM_SLOT_WAIT_SECS` override
- `oauth_quota` (G58, G45-CR5) discriminates Anthropic usage limit exhaustion from structural embedding errors; triggers deterministic codex <-> claude backend swap before falling back to FTS5
- `backend_mismatch` discriminates requested vs resolved backend divergence (e.g. `--llm-backend claude` resolved to codex via PATH-probe)
- `dim_zero` discriminates an embedding that returned a zero-dimension vector (structural bug indicator distinct from quota or contention)
- The expanded enum is backwards compatible: existing callers that switch on `embedding_failed | cancelled | timeout` continue to work; new variants are additive
- Default embedding `dim` is 64 (MRL, arXiv 2205.13147) since v1.0.79; v1.0.85 confirms and locks the constant at `src/constants.rs:22 DEFAULT_EMBEDDING_DIM = 64` (G56 docs)
- `anthropic-ratelimit-*-remaining` headers are now first-class signal in `LlmEmbedding::invoke_claude` (G45-CR5); a zero value aborts the spawn with `AppError::Embedding` mapped to `FallbackReason::OAuthQuota`
- `read` `AppError::MemoryNotFound` / `MemoryNotFoundById` Display is bilingue via `pt::memory_not_found` / `pt::memory_not_found_by_id` (G55 docs, preserved from v1.0.80)
- All schemas keep `"additionalProperties": false`; the seven-variant enum is the canonical discriminator for live-embedding degradation
- See `docs/decisions/adr-0043-five-gap-remediation.md` (EN) and `.pt-BR.md` for the full rationale
### Mudancas de Schema em v1.0.85 (ADR-0043 / remediacao dos cinco gaps)
- Schemas de resposta `recall` e `hybrid-search` estenderam o enum `vec_degraded_reason` de 3 para 7 variantes: `embedding_failed | slot_exhausted | oauth_quota | backend_mismatch | dim_zero | cancelled | timeout`
- `slot_exhausted` (GAP-003) discrimina contencao do semaforo de subprocessos LLM de exaustao de cota; chamadores podem re-tentar com override `SQLITE_GRAPHRAG_LLM_SLOT_WAIT_SECS`
- `oauth_quota` (G58, G45-CR5) discrimina exaustao de cota Anthropic de erros estruturais de embedding; dispara troca deterministica codex <-> claude antes de cair em FTS5-puro
- `backend_mismatch` discrimina divergencia entre backend solicitado e resolvido (ex. `--llm-backend claude` resolvido para codex via PATH-probe)
- `dim_zero` discrimina embedding que retornou vetor de dimensao zero (indicador de bug estrutural distinto de cota ou contencao)
- O enum expandido e retrocompativel: chamadores existentes que chaveiam em `embedding_failed | cancelled | timeout` continuam funcionando; variantes novas sao aditivas
- `dim` default de embedding e 64 (MRL, arXiv 2205.13147) desde v1.0.79; v1.0.85 confirma e tranca a constante em `src/constants.rs:22 DEFAULT_EMBEDDING_DIM = 64` (G56 docs)
- Headers `anthropic-ratelimit-*-remaining` agora sao sinal de primeira classe em `LlmEmbedding::invoke_claude` (G45-CR5); valor zero aborta o spawn com `AppError::Embedding` mapeado para `FallbackReason::OAuthQuota`
- `read` `AppError::MemoryNotFound` / `MemoryNotFoundById` Display e bilingue via `pt::memory_not_found` / `pt::memory_not_found_by_id` (G55 docs, preservado desde v1.0.80)
- Todos os schemas mantem `"additionalProperties": false`; o enum de sete variantes e o discriminador canonico para degradacao de embedding live
- Veja `docs/decisions/adr-0043-five-gap-remediation.md` (EN) e `.pt-BR.md` para a justificativa completa
### Input Payload Schemas
- `entities-input.schema.json` validates the JSON array accepted by `remember --entities-file`
- `relationships-input.schema.json` validates the JSON array accepted by `remember --relationships-file`
### Usage
- Inspect a `recall` response shape quickly: `sqlite-graphrag recall "query" | jaq '.'`
- Validate with a real JSON Schema validator: `jsonschema --instance <(sqlite-graphrag stats) docs/schemas/stats.schema.json`
- The `debug-schema` subcommand is hidden and intended for diagnostic tooling only — the binary exposes it with a double-underscore prefix (`debug-schema`) while the schema file uses the kebab-case name `debug-schema.schema.json` following the directory convention


### Schema Evolution in v1.0.86 → v1.0.89 (ADR-0045, ADR-0046, ADR-0047, ADR-0048, ADR-0049)
- v1.0.86 added 6 schemas for new LLM-pipeline subcommands: `slots-status.schema.json`, `pending-list.schema.json`, `embedding-status.schema.json` (updated v1.0.84 ADR-0042), `embedding-list.schema.json`, `shutdown-envelope.schema.json` (exit 19 envelope). `pending-embeddings process` reuses `pending-list.schema.json`
- v1.0.87 added `AppError::PreFlightFailed` (exit 16 `EX_CONFIG`) documented in `error-envelope.schema.json` with 8 variants: `ArgvExceedsArgMax`, `BinaryNotFound`, `McpConfigInlineJsonRejected`, `McpConfigPathMissing`, `McpConfigPathInvalidJson`, `WalkUpMcpJsonInvalid`, `OutputBufferTooSmall`, `ClaudeConfigDirNotEmpty`
- v1.0.88 fixed: `oauth_stderr_emits_single_line_v1088` regression test validates exit-19 envelope now emits 1 stderr line (was 2). All other schemas unchanged
- v1.0.89 (GAP-E2E-007) regenerated `health.schema.json` via `schemars 0.8` derive macro. Switched from `additionalProperties: false` to `true` (Must-Ignore). 17 new fields added. New `src/bin/dump_schema.rs` regenerates the schema idempotently via `schema_for!()` + BTreeMap ordering + recursive `apply_must_ignore` policy enforcement
- v1.0.89 (GAP-E2E-008, GAP-E2E-010) added `--db <PATH>` flag parity on 5 subcommands: `embedding-status`, `embedding-list`, `pending-list`, `codex-models`. No schema changes (the flag affects input parsing, not output envelope)
- v1.0.89 (GAP-E2E-009) added `--dry-run` and `--confirm` flags to `migrate`. New `migrate-dry-run.schema.json` describes the structured dry-run report (pending_migrations[], pending_count, checksum_mismatches[], status)
- v1.0.89 (GAP-E2E-011) added `--auto-describe` (default true) to `ingest`. No schema changes; affects how `description` field is populated in `ingest-file-event.schema.json` and `ingest-summary.schema.json` envelopes

### Schema Changes in v1.0.93 (ADR-0052 / OpenRouter Embedding Backend)
- Seven response schemas updated `backend_invoked` enum to include `openrouter` as a sixth variant: `claude | codex | opencode | openrouter | none | auto`
- `openrouter` is emitted when embedding was computed via the OpenRouter REST API (`--embedding-backend openrouter`) instead of a headless LLM subprocess
- Affected envelopes: `embedding-status`, `remember`, `edit`, `recall`, `hybrid-search`, `ingest-summary`, `enrich-summary`
- No new schema files were added — the OpenRouter backend uses the same output envelope structure as existing backends
- `ingest-summary.schema.json` now reflects the `--enrich-after` flag behavior: when active, the summary includes the enrich phase results inline

### Schema Changes in v1.0.95 (ADR-0054 / OpenRouter Chat Enrich)
- `enrich` gains a fourth extraction mode `openrouter` (`--mode openrouter`) that routes the JUDGE turn to the OpenRouter `/chat/completions` REST endpoint instead of a headless `claude`/`codex`/`opencode` subprocess
- NO new schema files were added — `enrich-phase.schema.json`, `enrich-item-event.schema.json`, and `enrich-summary.schema.json` are unchanged; the SCAN→JUDGE→PERSIST envelopes keep the same shape regardless of JUDGE transport
- The optional `backend_invoked` enum already covers `openrouter` (added v1.0.93 for embedding); the same variant now also describes an enrich JUDGE served via OpenRouter chat
- Structured Outputs (`response_format` `json_schema` `strict: true`) make the JUDGE output conform to the same entity/relationship structs the subprocess backends emit — no schema divergence

### Input Payload Schemas (Reference)
- `entities-input.schema.json` validates the JSON array accepted by `remember --entities-file`
- `relationships-input.schema.json` validates the JSON array accepted by `remember --relationships-file`

### Usage
- Inspect a `recall` response shape quickly: `sqlite-graphrag recall "query" | jaq '.'`
- Validate with a real JSON Schema validator: `jsonschema --instance <(sqlite-graphrag stats) docs/schemas/stats.schema.json`
- The `debug-schema` subcommand is hidden and intended for diagnostic tooling only — the binary exposes it with a double-underscore prefix (`debug-schema`) while the schema file uses the kebab-case name `debug-schema.schema.json` following the directory convention


## Português Brasileiro
### Propósito
- Cada arquivo neste diretório é um documento JSON Schema Draft 2020-12
- Schemas de saída descrevem o contrato exato de stdout de cada subcomando `sqlite-graphrag`
- Schemas de entrada descrevem os payloads JSON aceitos para ingestão de grafo orientada a arquivo
- Agentes e parsers DEVEM validar respostas contra estes schemas antes de processar
- A maioria dos schemas usa `"additionalProperties": false` — chaves inesperadas são violações de contrato
- `health.schema.json` (v1.0.89, GAP-E2E-007, ADR-0048) usa `"additionalProperties": true` (política Must-Ignore por RFC 7493 I-JSON e `rules_rust_json_e_ndjson.md:33`) — chaves desconhecidas são aceitas para permitir evolução do schema
- Os 17 novos campos adicionados em v1.0.89: `vec_memories_missing`, `vec_memories_orphaned`, `sqlite_version`, `mentions_ratio`, `mentions_warning`, `top_relation`, `top_relation_ratio`, `applies_to_ratio`, `relation_concentration_warning`, `super_hub_count`, `super_hub_warning`, `top_hub_entity`, `top_hub_degree`, `hub_warning`, `non_normalized_count`, `normalization_warning`, `fts_query_ok`
- Novo exit code 16 (`EX_CONFIG`) emitido por `AppError::PreFlightFailed` é documentado em v1.0.87 (ADR-0045, GAP-META-005) — veja `error-envelope.schema.json` para detalhes estruturados da variante `PreFlightError`
### Arquivos de Schema
| Subcomando | Arquivo de schema |
|---|---|
| `init` | `init.schema.json` |
| `remember` (atualizado v1.0.84, ADR-0042) | `remember.schema.json` |
| `recall` (atualizado v1.0.84, ADR-0042 / v1.0.85, ADR-0043 enum 7 variantes) | `recall.schema.json` |
| `read` | `read.schema.json` |
| `list` | `list.schema.json` |
| `forget` | `forget.schema.json` |
| `purge` | `purge.schema.json` |
| `rename` | `rename.schema.json` |
| `edit` (atualizado v1.0.84, ADR-0042) | `edit.schema.json` |
| `history` | `history.schema.json` |
| `restore` | `restore.schema.json` |
| `hybrid-search` (atualizado v1.0.84, ADR-0042 / v1.0.85, ADR-0043 enum 7 variantes) | `hybrid-search.schema.json` |
| `deep-research` | `deep-research.schema.json` |
| `health` | `health.schema.json` |
| `migrate` | `migrate.schema.json` |
| `migrate --rehash` (v1.0.76, atualizado v1.0.77, v1.0.78) | `migrate-rehash.schema.json` |
| `migrate --to-llm-only` (v1.0.76, atualizado v1.0.77, v1.0.78) | `migrate-to-llm-only.schema.json` |
| `namespace-detect` | `namespace-detect.schema.json` |
| `optimize` | `optimize.schema.json` |
| `stats` | `stats.schema.json` |
| `sync-safe-copy` | `sync-safe-copy.schema.json` |
| `vacuum` | `vacuum.schema.json` |
| `link` | `link.schema.json` |
| `unlink` | `unlink.schema.json` |
| `related` | `related.schema.json` |
| `graph` | `graph.schema.json` |
| `graph traverse` | `graph-traverse.schema.json` |
| `graph stats` | `graph-stats.schema.json` |
| `graph entities` | `graph-entities.schema.json` |
| `cleanup-orphans` | `cleanup-orphans.schema.json` |
| `prune-relations` | `prune-relations.schema.json` |
| `reclassify-relation` | `reclassify-relation.schema.json` |
| `normalize-entities` | `normalize-entities.schema.json` |
| `enrich` (evento de fase) | `enrich-phase.schema.json` |
| `enrich` (evento por item) | `enrich-item-event.schema.json` |
| `enrich` (sumário, atualizado v1.0.84, ADR-0042) | `enrich-summary.schema.json` |
| `ingest` (evento por arquivo) | `ingest-file-event.schema.json` |
| `ingest` (sumário, atualizado v1.0.84, ADR-0042) | `ingest-summary.schema.json` |
| `ingest --mode claude-code` (evento de fase) | `ingest-claude-phase.schema.json` |
| `ingest --mode claude-code` (evento por arquivo) | `ingest-claude-file-event.schema.json` |
| `ingest --mode claude-code` (sumário) | `ingest-claude-summary.schema.json` |
| `debug-schema` | `debug-schema.schema.json` |
| `fts rebuild` | `fts-rebuild.schema.json` |
| `fts check` | `fts-check.schema.json` |
| `fts stats` | `fts-stats.schema.json` |
| `backup` | `backup.schema.json` |
| `delete-entity` | `delete-entity.schema.json` |
| `reclassify` | `reclassify.schema.json` |
| `merge-entities` | `merge-entities.schema.json` |
| `rename-entity` | `rename-entity.schema.json` |
| `memory-entities` (forward: `--name`) | `memory-entities.schema.json` |
| `memory-entities` (reverso: `--entity`) | `memory-entities-reverse.schema.json` |
| `prune-ner` | `prune-ner.schema.json` |
| `remember-batch` (evento por item) | `remember-batch.schema.json` |
| `remember-batch` (sumário) | `remember-batch-summary.schema.json` |
| `export` (linha por memória) | `export-memory-line.schema.json` |
| `export` (sumário) | `export-summary.schema.json` |
| `vec orphan-list` (v1.0.69) | `vec-orphan-list.schema.json` |
| `vec purge-orphan` (v1.0.69) | `vec-purge-orphan.schema.json` |
| `vec stats` (v1.0.69) | `vec-stats.schema.json` |
| `codex-models` (v1.0.69) | `codex-models.schema.json` |
| `slots status` (v1.0.82, GAP-004) | `slots-status.schema.json` |
| `pending list` (v1.0.82, GAP-001) | `pending-list.schema.json` |
| `embedding status` (v1.0.82, GAP-005, atualizado v1.0.84, ADR-0042) | `embedding-status.schema.json` |
| `embedding list` (v1.0.82, GAP-005) | `embedding-list.schema.json` |
| envelope de shutdown (v1.0.82, GAP-002) | `shutdown-envelope.schema.json` |
| envelope de erro (todos os comandos) | `error-envelope.schema.json` |
