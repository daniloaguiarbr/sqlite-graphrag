# Machine-Readable JSON Schemas


## English
### Purpose
- Each file in this directory is a JSON Schema Draft 2020-12 document
- Output schemas describe the exact stdout contract of every `sqlite-graphrag` subcommand
- Input schemas describe the accepted JSON payloads for file-driven graph ingestion
- Agents and parsers MUST validate responses against these schemas before processing
- All schemas use `"additionalProperties": false` — unexpected keys are contract violations
### Schema Files
| Subcommand | Schema file |
|---|---|
| `init` | `init.schema.json` |
| `remember` | `remember.schema.json` |
| `recall` | `recall.schema.json` |
| `read` | `read.schema.json` |
| `list` | `list.schema.json` |
| `forget` | `forget.schema.json` |
| `purge` | `purge.schema.json` |
| `rename` | `rename.schema.json` |
| `edit` | `edit.schema.json` |
| `history` | `history.schema.json` |
| `restore` | `restore.schema.json` |
| `hybrid-search` | `hybrid-search.schema.json` |
| `deep-research` | `deep-research.schema.json` |
| `health` | `health.schema.json` |
| `migrate` | `migrate.schema.json` |
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
| `enrich` (summary) | `enrich-summary.schema.json` |
| `ingest` (per-file event) | `ingest-file-event.schema.json` |
| `ingest` (summary) | `ingest-summary.schema.json` |
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
| error envelope (all commands) | `error-envelope.schema.json` |
### Commands Without JSON Schemas
- `completions` emits shell completion scripts (Bash, Zsh, Fish, PowerShell, Elvish) as plain text — no JSON schema applies
- `daemon`, `daemon --ping`, `daemon --stop` use plain-text status messages — no JSON schema applies
### Ingest Mode Schema Selection
- `--mode none` and `--mode gliner` use `ingest-file-event.schema.json` and `ingest-summary.schema.json`
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
### Input Payload Schemas
- `entities-input.schema.json` validates the JSON array accepted by `remember --entities-file`
- `relationships-input.schema.json` validates the JSON array accepted by `remember --relationships-file`
### Usage
- Inspect a `recall` response shape quickly: `sqlite-graphrag recall "query" | jaq '.'`
- Validate with a real JSON Schema validator: `jsonschema --instance <(sqlite-graphrag stats) docs/schemas/stats.schema.json`
- The `debug-schema` subcommand is hidden and intended for diagnostic tooling only — the binary exposes it with a double-underscore prefix (`debug-schema`) while the schema file uses the kebab-case name `debug-schema.schema.json` following the directory convention
### Flag Behavior
- Schemas describe the OUTPUT JSON contract, not the CLI input shape
- Several subcommands accept multiple flag aliases that produce the same output
- `sync-safe-copy` accepts `--dest` (primary), `--to`, and `--output` — all write to the same `dest_path` field in the response
- `graph stats` accepts both `--json` and `--format json`; if `--json` is combined with `--format text`, `--json` wins and the response remains JSON
- `debug-schema` is exposed by the binary with a double-underscore prefix; the schema file uses kebab-case `debug-schema.schema.json` following the directory convention
- The `--json` flag is the universal compatibility switch for JSON stdout; on commands with alternate text formats, `--json` wins
### Stability Guarantee
- Schemas track the `main` branch and are updated with every breaking change
- Minor additions (new optional fields) do NOT bump the schema version
- Removals or renames of required fields constitute a breaking change and increment the CLI major version


## Português Brasileiro
### Objetivo
- Cada arquivo neste diretório é um documento JSON Schema Draft 2020-12
- Os schemas de saída descrevem o contrato exato de stdout de cada subcomando `sqlite-graphrag`
- Os schemas de entrada descrevem os payloads JSON aceitos pela ingestão de grafo orientada a arquivo
- Agentes e parsers DEVEM validar respostas contra estes schemas antes de processar
- Todos os schemas usam `"additionalProperties": false` — chaves inesperadas são violações de contrato
### Arquivos de Schema
- Veja a tabela na seção English acima — os nomes de arquivo são idênticos entre idiomas
### Comandos Sem JSON Schema
- `completions` emite scripts de completion de shell (Bash, Zsh, Fish, PowerShell, Elvish) como texto puro — nenhum JSON schema se aplica
- `daemon`, `daemon --ping`, `daemon --stop` usam mensagens de status em texto puro — nenhum JSON schema se aplica
### Seleção de Schema por Modo de Ingestão
- `--mode none` e `--mode gliner` usam `ingest-file-event.schema.json` e `ingest-summary.schema.json`
- `--mode claude-code` usa `ingest-claude-phase.schema.json`, `ingest-claude-file-event.schema.json` e `ingest-claude-summary.schema.json`
- Modo claude-code emite eventos de fase adicionais (validate, scan) antes dos eventos por arquivo
- Eventos por arquivo no modo claude-code incluem campos `entities`, `rels` e `cost_usd` não presentes na ingestão normal
- `--mode codex` (adicionado na v1.0.62) reutiliza o mesmo formato NDJSON do `--mode claude-code` — nenhum schema codex separado é necessário
- Modo Codex emite os mesmos shapes de PhaseEvent, FileEvent e Summary; agentes que validam saída claude-code podem reutilizar esses schemas sem alteração

### Error Envelope Changes in v1.0.68 (G28-B)
- The `error-envelope.schema.json` `message` field for `code: 75` now has two distinct templates, both routed to the same exit code
- Template A (new since v1.0.68, G28-B): `job <job_type> for namespace '<namespace>' is already running (exit 75); wait for it to finish or pass --wait-job-singleton <SECONDS>` — emitted by `enrich`, `ingest --mode claude-code`, and `ingest --mode codex` when a concurrent invocation holds the singleton
- Template B (legacy): `all <max> concurrency slots occupied after waiting <waited_secs>s (exit 75); use --max-concurrency or wait for other invocations to finish` — emitted by the counting semaphore for any other command
- Agents can disambiguate the two with a regex on `message`: matches `^job ` for Template A and `^all ` for Template B
- The schema itself remains `additionalProperties: false` because variant-specific fields are intentionally NOT serialised to JSON; structured access to `job_type` and `namespace` requires agents to parse the quoted strings inside `message`
### Schemas Adicionados na v1.0.69 (G33 + G39)
- `vec-orphan-list.schema.json` cobre `sqlite-graphrag vec orphan-list --json`; lista cada linha órfã com `vector_hash` e `kind` (`memory` | `entity` | `chunk`).
- `vec-purge-orphan.schema.json` cobre `sqlite-graphrag vec purge-orphan --yes --json`; emite contagens de purga por tabela para `vec_memories`, `vec_entities` e `vec_chunks`.
- `vec-stats.schema.json` cobre `sqlite-graphrag vec stats --json`; emite contagens de linhas mais contagens de órfãos nas três tabelas vec.
- `codex-models.schema.json` cobre `sqlite-graphrag codex-models --json`; emite a lista branca de modelos ChatGPT Pro OAuth, o modelo padrão e um campo opcional `suggestion` quando `--suggest <substring>` é usado.
- Os quatro novos schemas declaram `"additionalProperties": false` para casar com a convenção de schemas do projeto.
- Schemas existentes (`optimize.schema.json`, `enrich-*.schema.json`, `backup.schema.json`) permanecem inalterados em shape; os novos campos v1.0.69 (`fts_progress_polls`, `enrich_preservation_score`, `backup_step_sleep_ms`) vivem dentro de seus objetos existentes como campos opcionais.

### Schemas de Payload de Entrada
- `entities-input.schema.json` valida o array JSON aceito por `remember --entities-file`
- `relationships-input.schema.json` valida o array JSON aceito por `remember --relationships-file`
### Comportamento de Flags
- Os schemas descrevem o contrato de OUTPUT JSON, não o formato de entrada CLI
- Vários subcomandos aceitam múltiplos aliases de flag que produzem a mesma saída
- `sync-safe-copy` aceita `--dest` (primária), `--to` e `--output` — todos gravam no mesmo campo `dest_path` da resposta
- `graph stats` aceita `--json` e `--format json`; se `--json` for combinado com `--format text`, `--json` vence e a resposta continua JSON
- `debug-schema` é exposto pelo binário com prefixo duplo sublinhado; o arquivo de schema usa kebab-case `debug-schema.schema.json` seguindo a convenção do diretório
- A flag `--json` é o switch universal de compatibilidade para JSON no stdout; em comandos com formatos textuais alternativos, `--json` vence
### Uso
- Inspecionar rapidamente o shape da resposta do `recall`: `sqlite-graphrag recall "consulta" | jaq '.'`
- Validar com um validador JSON Schema real: `jsonschema --instance <(sqlite-graphrag stats) docs/schemas/stats.schema.json`
- O subcomando `debug-schema` é oculto e destinado apenas a ferramentas de diagnóstico — o binário o expõe com prefixo duplo sublinhado (`debug-schema`) enquanto o arquivo de schema usa o nome kebab-case `debug-schema.schema.json` seguindo a convenção do diretório
### Garantia de Estabilidade
- Os schemas acompanham a branch `main` e são atualizados a cada breaking change
- Adições menores (novos campos opcionais) NÃO incrementam a versão do schema
- Remoções ou renomeações de campos obrigatórios constituem breaking change e incrementam a versão major da CLI
