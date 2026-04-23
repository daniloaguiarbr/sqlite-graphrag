Leia este documento em [inglês (EN)](CHANGELOG.md).


# Changelog

Todas as mudanças notáveis deste projeto serão documentadas neste arquivo.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/),
e este projeto adere ao [Semantic Versioning](https://semver.org/lang/pt-BR/spec/v2.0.0.html).

## [Sem Versão]

## [1.0.3] - 2026-04-23

### Corrigido
- Comandos pesados agora calculam concorrência segura dinamicamente a partir da memória disponível, número de CPUs e orçamento de RSS por task antes de adquirir slots da CLI
- `init`, `remember`, `recall` e `hybrid-search` agora emitem logs defensivos de progresso mostrando a carga pesada detectada e a concorrência segura calculada
- O runtime agora reduz `--max-concurrency` para o orçamento seguro de memória em comandos pesados, em vez de deixar a heurística documentada sem enforcement
- O orçamento de RSS usado pela heurística de concorrência agora é calibrado a partir de pico de RSS medido, em vez de uma estimativa histórica mais antiga

### Adicionado
- Cobertura unitária para classificação de comandos pesados e cálculo de concorrência segura

## [1.0.2] - 2026-04-23

### Adicionado
- Schemas formais de entrada para `remember --entities-file` e `remember --relationships-file`
- Contrato estável de entrada do grafo em `AGENT_PROTOCOL`, `AGENTS`, `HOW_TO_USE` e `llms-full.txt`
- Resumo curto do contrato de entrada do grafo em `llms.txt` e `llms.pt-BR.txt`

### Corrigido
- Títulos de `AGENTS` agora descrevem `--json` como universal e `--format json` como específico por comando
- A matriz de saída em `HOW_TO_USE` agora reflete a saída padrão real de `link`, `unlink` e `cleanup-orphans`
- A documentação pública não apresenta mais o projeto como pré-publicação

## [1.0.1] - 2026-04-23

### Corrigido
- `--format` foi restringido a `json` nos comandos que não implementam `text` ou `markdown`, evitando que help e parse prometam modos de saída inexistentes
- `hybrid-search` deixou de aceitar `text` ou `markdown` para falhar apenas em runtime; formatos não suportados agora são rejeitados pelo `clap` no parse dos argumentos
- Documentação e guias para agentes agora explicam que `--json` é a flag ampla de compatibilidade, enquanto `--format json` é específico por comando

### Adicionado
- A documentação de payload de `remember` agora explica que `--relationships-file` exige `strength` em `[0.0, 1.0]` e que esse campo é mapeado para `weight` nas saídas do grafo
- A documentação de payload de `remember` agora explica que `type` é aceito como alias de `entity_type`, mas os dois campos juntos são inválidos

## [1.0.0] - 2026-04-19

- Primeira release pública sob o nome `sqlite-graphrag`
- O conjunto de funcionalidades deriva do legado `neurographrag v2.3.0`

### Corrigido
- consulta SQL de graph entities agora usa o nome de coluna correto (NG-V220-01 CRITICAL)
- stats e health agora aceitam a flag --format json (NG-V220-02 HIGH)
- obrigatoriedade de --type no remember documentada em todos os exemplos (NV-005 HIGH)
- documentação de rename corrigida para --name/--new-name (NV-002)
- documentação de recall esclarece argumento posicional QUERY (NV-004)
- documentação de forget remove flag --yes inexistente (NV-001)
- documentação de list referencia campo items correto (NV-006)
- documentação de related referencia campo results correto (NV-010)
- MIGRATION.md agora documenta a transição de rename e o plano de release `v1.0.0`

### Adicionado
- flag obrigatória --relation de unlink documentada (NV-003)
- graph traverse --from espera nome de entidade documentado (NV-007)
- lista de valores restritos de entity_type documentada (NV-009)
- flag --format adicionada ao sync-safe-copy para controle de saída (NG-V220-04)

### Alterado
- __debug_schema esclarece semântica de user_version versus schema_version (NG-V220-03)
- flags globais de i18n documentadas como exclusivas do PT (GAP-I18N-02 LOW)

## [2.2.0] - 2026-04-19

### Corrigido
- G-017: alias `--to` de `sync-safe-copy` restaurado; `--destination` permanece canônico (regressão da v2.0.3)
- G-027: `PRAGMA user_version` agora definido como 49 após migrações refinery para corresponder à contagem de linhas de `refinery_schema_history`
- NG-08: subcomando `health` agora executa `PRAGMA integrity_check` antes das contagens de memórias/entidades para defesa em profundidade; saída ganha campos `journal_mode`, `wal_size_mb` e `checks[]`

### Adicionado
- NG-04: subcomando `graph entities` lista nós do grafo com filtro opcional `--type` e saída `--json`
- NG-06: flag `--format` adicionada ao `graph stats` para paridade com `graph traverse`
- NG-05: subcomando diagnóstico oculto `__debug_schema` documentado; emite campos `schema_version`, `user_version`, `objects` e `migrations`
- NG-03: todos os subcomandos agora aceitam tanto `--json` (forma curta) quanto `--format json` (forma explícita) produzindo saída idêntica

### Alterado
- NG-07: `link` e `unlink` esclarecidos para operar exclusivamente em entidades tipadas do grafo; tipos válidos documentados no `--help`

## [2.1.0] - 2026-04-19

### Corrigido
- G-001: `rename` agora emite `action: "renamed"` no JSON de saída (`src/commands/rename.rs`)
- G-002: ranks do `hybrid-search` agora começam em 1 atendendo restrição `minimum: 1` do schema
- G-003: `--expected-updated-at` agora aplica lock otimista via cláusula WHERE + verificação `changes()` (exit 3 em conflito)
- G-005: prefixo i18n `Error:` agora traduzido para `Erro:` em PT via `i18n::prefixo_erro()` em `main.rs`
- G-007: `health` retorna exit 10 quando `integrity_ok: false` via `AppError::Database` (emite JSON antes de retornar Err)
- G-013: `restore` agora encontra memórias soft-deleted (WHERE inclui `deleted_at IS NOT NULL`)
- G-018: `emit_progress()` agora usa `tracing::info!` respeitando `LOG_FORMAT=json`
- Receitas 8 e 14 do COOKBOOK corrigidas para usar `jaq '.items[]'` conforme estrutura de `list --json`
- Semântica de score no HOW_TO_USE pt-BR corrigida (`score` alto = mais relevante, não distância baixa)

### Adicionado
- G-004: Documentação dos valores válidos de `entity_type` em `--entities-file` (`project|tool|person|file|concept|incident|decision|memory|dashboard|issue_tracker`)
- G-006: `docs/MIGRATION.md` + `docs/MIGRATION.pt-BR.md` com guia de atualização v1.x para v2.x
- G-016: Subcomando `graph traverse` (flags `--from`/`--depth`) com novo schema `docs/schemas/graph-traverse.schema.json`
- G-016: Subcomando `graph stats` com novo schema `docs/schemas/graph-stats.schema.json`
- G-019/G-020: Flag global `--tz` + `tz::init()` em `main.rs` populando `FUSO_GLOBAL` para timestamps com fuso horário
- G-024: Flag `namespace-detect --db` para override de DB múltiplo
- G-025: Flags `vacuum --checkpoint` + `--format`
- G-026: Subcomando `migrate --status` com resposta `applied_migrations`
- G-027: `PRAGMA user_version = 49` definido após conclusão das migrations do refinery
- 6 novas seções H3 em HOW_TO_USE.pt-BR.md (Aliases de Flag de Idioma, Flag de Saída JSON, Descoberta de Caminho do DB, Limite de Concorrência, Nota sobre forget, Nota sobre optimize e migrate)
- Nova receita no COOKBOOK pt-BR: "Como Exibir Timestamps no Fuso Horário Local"

### Alterado
- `migrate.schema.json` agora usa `oneOf` cobrindo os modos run vs `--status` com `$defs.MigrationEntry`
- `--json` aceito como no-op em `remember`/`read`/`history`/`forget`/`purge` para consistência
- `docs/schemas/README.md` documenta convenção de nomenclatura `__debug_schema` (binário) vs kebab-case (arquivo de schema)

### Descontinuado
- `--allow-parallel` removida em v1.2.0 — consulte `docs/MIGRATION.md` para caminho de atualização


## [2.0.5] — 2026-04-19

### Corrigido
- Exit code 13 documentado como `BatchPartialFailure` e exit code 15 como `DbBusy` em AGENTS.md — separação correta conforme `src/errors.rs` desde v2.0.0
- Exit code 73 substituído por 75 (`LockBusy/AllSlotsFull`) em todas as referências de documentação
- `PURGE_RETENTION_DAYS` corrigido de 30 para 90 em AGENTS.md e HOW_TO_USE.md EN+pt-BR — alinhado à constante `PURGE_RETENTION_DAYS_DEFAULT = 90` em `src/constants.rs`

### Adicionado
- `elapsed_ms: u64` padronizado em todos os comandos que ainda não expunham o campo — uniformidade de contrato JSON
- `schema_version: u32` adicionado ao JSON stdout de `health` — facilita detecção de migração por agentes
- Subcomando oculto `__debug_schema` que imprime schema SQLite + versão de migrations para diagnóstico
- Diretório `docs/schemas/` com JSON Schema Draft 2020-12 público de cada resposta
- 12 suites de testes cobrindo: contrato JSON, exit codes P0, migração de schema, concorrência, property-based, sinais, i18n, segurança, benchmarks, smoke de instalado, receitas do cookbook e regressão v2.0.4
- 4 benchmarks criterion em `benches/cli_benchmarks.rs` validando SLAs de latência
- `proptest = { version = "1", features = ["std"] }` e `criterion = { version = "0.5", features = ["html_reports"] }` em `[dev-dependencies]`
- `[[bench]]` com `name = "cli_benchmarks"` e `harness = false` em `Cargo.toml`


## [2.0.4] — 2026-04-19

### Corrigido
- `--expected-updated-at` agora aceita tanto Unix epoch inteiro quanto string RFC 3339 via parser duplo em src/parsers/mod.rs — aplicado em edit, rename, restore, remember (GAP 1 CRITICAL)
- `entities-file` agora aceita o campo `"type"` como alias de `"entity_type"` via `#[serde(alias = "type")]` — elimina erro 422 em payloads válidos de agentes (GAP 12 HIGH)
- Mensagens internas de validação agora localizadas EN/PT via módulo `i18n::validacao` — 7 funções cobrindo comprimento do nome, nome reservado, kebab-case, comprimento de descrição, comprimento de body (GAP 13 MEDIUM)
- Flag `purge --yes` aceita silenciosamente como no-op para compatibilidade com exemplos documentados (GAP 19 MEDIUM)
- Resposta JSON de `link` agora duplica `from` como `source` e `to` como `target` — zero breaking change, adiciona aliases esperados (GAP 20 MEDIUM)
- Objetos de nó em `graph` agora duplicam `kind` como `type` via `#[serde(rename = "type")]` em graph_export.rs — zero breaking change (GAP 21 LOW)
- Registros de versão de `history` agora incluem campo `created_at_iso` RFC 3339 paralelo ao `created_at` Unix existente (GAP 24 LOW)

### Adicionado
- Schema JSON de `health` expandido conforme spec completa do PRD: +db_size_bytes, +integrity_ok, +schema_ok, +vec_memories_ok, +vec_entities_ok, +vec_chunks_ok, +fts_ok, +model_ok, +checks[] com 7 entradas (GAP 4 HIGH)
- Resposta JSON de `recall` agora inclui `elapsed_ms: u64` medido via Instant (GAP 8 HIGH)
- Resposta JSON de `hybrid-search` agora inclui `elapsed_ms: u64`, `rrf_k: u32` e `weights: {vec, fts}` (GAPs 8+10 HIGH)
- Módulo de validação i18n `src/i18n/validacao.rs` — todas as 7 mensagens de erro de validação disponíveis em EN e PT
- Parser de timestamp duplo `src/parsers/mod.rs` — aceita Unix epoch i64 e RFC 3339 via `chrono::DateTime::parse_from_rfc3339`

### Alterado
- Varredura de docs EN (T9): schemas de recall, hybrid-search, list, health, stats alinhados com saída real do binário; pesos corrigidos 0.6/0.4 → 1.0/1.0; namespace padrão documentado como `global`; alias `--json` no-op documentado; `related` documentado para receber nome da memória e não ID
- Varredura de docs PT (T10): COOKBOOK.pt-BR.md, CROSS_PLATFORM.pt-BR.md, AGENTS.pt-BR.md, README.pt-BR.md, skill/sqlite-graphrag-pt/SKILL.md, llms.pt-BR.txt alinhados espelhando as correções EN do T9
- 18 arquivos-fonte binário atualizados; 1 arquivo novo criado (src/parsers/mod.rs)
- 283 testes PASS, zero warnings de clippy, zero erros de check após alterações no binário


## [2.0.3] - 2026-04-19

### Adicionado
- `purge --days` aceito como alias de `--retention-days` para compatibilidade com docs (GAP 3)
- `recall --json` e `hybrid-search --json` aceitos como no-op (GAP 6) — saída JSON já é o padrão
- JSON de `health` agora inclui `wal_size_mb` e `journal_mode` (GAP 7)
- JSON de `stats` agora inclui `edges` (alias de `relationships`) e `avg_body_len` (GAP 8)
- Variantes de `AppError` agora localizadas via enum `Idioma` / match exaustivo de `Mensagem` (GAP 13) — `--lang en/pt` aplica-se também às mensagens de erro
- 8 novas seções em HOW_TO_USE.md para subcomandos sem documentação prévia (GAP 12): cleanup-orphans, edit, graph, history, namespace-detect, rename, restore, unlink
- Espelho bilíngue HOW_TO_USE.pt-BR.md
- Aviso de latência no COOKBOOK informando ~1s por invocação CLI vs planos do daemon (GAP P1)

### Alterado
- Toda a documentação: `--type agent` substituído por `--type project` (GAP 1) — PRD define 7 tipos válidos (user/feedback/project/reference/decision/incident/skill); `agent` nunca foi válido
- Toda a documentação: `purge --days` reescrito como `purge --retention-days` (GAP 3)
- Toda a documentação: exemplos de `remember` agora incluem `--description "..."` (GAP 2)
- README, CLAUDE, AGENT_PROTOCOL: contagem de agentes padronizada em 27 (GAP 14)
- Schemas AGENTS.md: raiz JSON de `recall` documentada como `direct_matches[]/graph_matches[]/results[]` (conforme PRD), `hybrid-search` como `results[]` com `vec_rank/fts_rank` (GAPs 4, 5)
- Padrões do COOKBOOK corrigidos: recall --k 10, list --limit 50, pesos hybrid-search 1.0/1.0, purge --retention-days 90 (GAPs 28-31)
- Nota em docs sobre `distance` (cosseno, menor=melhor) vs `score` (1-distance, maior=melhor) em JSON vs text/markdown (GAP 17)
- Nota em docs sobre namespace padrão `global` (não `default`) (GAP 16)

### Corrigido
- Binário não retorna mais exit 2 para `purge --days 30` (GAP 3)
- Binário não retorna mais exit 2 para `recall --json "q"` (GAP 6)
- Documentação de `link` agora explicita pré-requisito de entidade (GAP 9)
- Documentação da flag `--force-merge` (GAP 18)
- Documentação de `graph --format dot|mermaid` (GAP 22)
- Documentação da flag `--db <PATH>` (GAP 25)
- Documentação de `--max-concurrency` limitado a 2×nCPUs (GAP 27)

### Documentação
- `27 agentes de IA` padronizado como contagem oficial em todo o projeto
- Evidência: plano de testes de 2026-04-19 catalogou 31 gaps em `/tmp/sqlite-graphrag-testplan-v2.0.2/gaps.md`; v2.0.3 fecha todos os 31
- GAP 11 `elapsed_ms` universal em JSON adiado para v2.1.0 (requer captura de processing_time em todos os comandos)
- GAP P1 latência < 50ms requer modo daemon planejado para v3.0.0


## [2.0.2] - 2026-04-19

### Corrigido

- Flag `--lang` agora aceita os códigos curtos `en`/`pt` conforme documentado.
- Antes exigia identificadores completos `english`/`portugues`; aliases adicionados: `en/english/EN`, `pt/portugues/portuguese/pt-BR/pt-br/PT`.


## [2.0.1] - 2026-04-19

### Adicionado

- Aliases de flags para compatibilidade retroativa com a documentação bilíngue.
- `rename --old/--new` adicionados como aliases de `--name/--new-name`.
- `link/unlink --source/--target` adicionados como aliases de `--from/--to`.
- `related --hops` adicionado como alias de `--max-hops`.
- `sync-safe-copy --output` adicionado como alias de `--dest`.
- `related` agora aceita o nome da memória como argumento posicional.
- `--json` aceito como no-op em `health`, `stats`, `migrate`, `namespace-detect`.
- Flag global `--lang en|pt` com fallback via env var `SQLITE_GRAPHRAG_LANG`.
- Fallback de locale `LC_ALL`/`LANG` usado para mensagens de progresso no stderr.
- Novo módulo `i18n` com enum `Language` e helpers `init`/`current`/`tr`.
- Helpers bilíngues adicionados em `output::emit_progress_i18n`.
- Timestamps ISO 8601: `created_at_iso` adicionado em `RememberResponse`.
- `updated_at_iso` adicionado em itens de `list`.
- `created_at_iso`/`updated_at_iso` adicionados em `read`, paralelos aos inteiros epoch existentes.
- Resposta `read` agora inclui `memory_id` (alias de `id`).
- Resposta `read` agora inclui `type` (alias de `memory_type`).
- Resposta `read` agora inclui `version` para controle otimista.
- Itens `hybrid-search` agora incluem `score` (alias de `combined_score`).
- Itens `hybrid-search` agora incluem `source: "hybrid"`.
- Itens `list` agora incluem `memory_id` (alias de `id`).
- Resposta `stats` agora inclui `memories_total`, `entities_total`, `relationships_total`.
- Resposta `stats` agora inclui `chunks_total`, `db_bytes` para conformidade com contrato.
- Resposta `health` agora inclui `schema_version` no topo conforme PRD.
- Resposta `health` agora inclui `missing_entities[]` conforme PRD.
- `RememberResponse` inclui `operation` (alias de `action`), `created_at`, `created_at_iso`.
- `RecallResponse` inclui `results[]` com merge de `direct_matches` e `graph_matches`.
- Flag `init --namespace` adicionada, resolvida e ecoada em `InitResponse.namespace`.
- Flag `recall --min-distance <float>` adicionada (default 1.0, desativada por padrão).
- Quando `--min-distance` abaixo de 1.0, retorna exit 4 se todos os hits excederem o threshold.

### Corrigido

- Arquivos DB criados por `open_rw` agora recebem chmod 600 em Unix.
- Arquivos de snapshot criados por `sync-safe-copy` agora recebem chmod 600 em Unix.
- Previne vazamento de credenciais em montagens compartilhadas (Dropbox, NFS, `/tmp` multi-usuário).
- Mensagens de progresso em `remember`, `recall`, `hybrid-search`, `init` usam helper bilíngue.
- Idioma agora respeitado de forma consistente (antes misturava EN/PT na mesma sessão).

### Documentação

- COOKBOOK, AGENT_PROTOCOL, SKILL, CLAUDE.md atualizados para refletir schemas e flags reais.
- README, INTEGRATIONS e llms.txt atualizados para refletir exit codes reais.
- Validados contra o output de `--help` de cada subcomando.
- Subcomandos `graph` e `cleanup-orphans` agora documentados nos guias apropriados.
- Disclaimer honesto de latência adicionado: recall e hybrid-search levam ~1s por invocação.
- Latência de ~8ms requer daemon (planejado para v3.0.0 Tier 4).


## [2.0.0] - 2026-04-18

### Breaking

- EXIT CODE: `DbBusy` movido de 13 para 15 para liberar exit 13 para `BatchPartialFailure`.
- Scripts shell que detectavam `EX_UNAVAILABLE` (13) como DB busy agora devem checar 15.
- HYBRID-SEARCH: formato JSON da resposta remodelado; formato antigo era `{query, combined_rank[], vec_rank[], fts_rank[]}`.
- Novo formato: `{query, k, results: [{memory_id, name, namespace, type, description, body, combined_score, vec_rank?, fts_rank?}], graph_matches: []}`.
- Consumidores que parseavam `combined_rank` devem migrar para `results` conforme PRD linhas 771-787.
- PURGE: `--older-than-seconds` descontinuada em favor de `--retention-days`.
- A flag antiga permanece como alias oculto mas emite warning; será removida em v3.0.0.
- NAME SLUG: `NAME_SLUG_REGEX` mais estrita que `SLUG_REGEX` da v1.x.
- Nomes multichar devem agora começar com letra (requisito do PRD).
- Single-char `[a-z0-9]` ainda permitido; memórias existentes com dígito inicial passam inalteradas.
- `rename` para nomes estilo legado (dígito inicial, multichar) agora falhará.

### Adicionado

- `AppError::BatchPartialFailure { total, failed }` mapeando para exit 13.
- Reservado para `import`, `reindex` e batch stdin (entrando em Tier 3/4).
- Constantes em `src/constants.rs`: `PURGE_RETENTION_DAYS_DEFAULT=90`, `MAX_NAMESPACES_ACTIVE=100`.
- Constantes: `EMBEDDING_MAX_TOKENS=512`, `K_GRAPH_MATCHES_LIMIT=20`, `K_LIST_DEFAULT_LIMIT=100`.
- Constantes: `K_GRAPH_ENTITIES_DEFAULT_LIMIT=50`, `K_RELATED_DEFAULT_LIMIT=10`, `K_HISTORY_DEFAULT_LIMIT=20`.
- Constantes: `WEIGHT_VEC_DEFAULT=1.0`, `WEIGHT_FTS_DEFAULT=1.0`, `TEXT_BODY_PREVIEW_LEN=200`.
- Constantes: `ORT_NUM_THREADS_DEFAULT="1"`, `ORT_INTRA_OP_NUM_THREADS_DEFAULT="1"`, `OMP_NUM_THREADS_DEFAULT="1"`.
- Constantes: `BATCH_PARTIAL_FAILURE_EXIT_CODE=13`, `DB_BUSY_EXIT_CODE=15`.
- Flag `--dry-run` e `--retention-days` em `purge`.
- Campos `namespace` e `merged_into_memory_id: Option<i64>` em `RememberResponse`.
- Campo `k: usize` em `RecallResponse`.
- Campos `bytes_freed: i64`, `oldest_deleted_at: Option<i64>` em `PurgeResponse`.
- Campos `retention_days_used: u32`, `dry_run: bool` em `PurgeResponse`.
- Flag `--format` em `hybrid-search` (apenas JSON; text/markdown reservados para Tier 2).
- Flag `--expected-updated-at` (optimistic locking) em `rename` e `restore`.
- Guard de limite de namespaces ativos (`MAX_NAMESPACES_ACTIVE=100`) em `remember`.
- Retorna exit 5 quando o limite de namespaces ativos é excedido.

### Alterado

- `SLUG_REGEX` renomeada para `NAME_SLUG_REGEX` com valor conforme PRD.
- Novo padrão: `r"^[a-z][a-z0-9-]{0,78}[a-z0-9]$|^[a-z0-9]$"`.
- Nomes multichar devem começar com letra.

### Corrigido

- Prefixo `__` explicitamente rejeitado em `rename` (antes apenas aplicado em `remember`).
- Constantes `WEIGHT_VEC_DEFAULT`, `WEIGHT_FTS_DEFAULT` agora declaradas em `constants.rs`.
- Referências do PRD agora mapeiam símbolos reais.


## [1.2.1] - 2026-04-18

### Corrigido

- Falha de instalação em versões de `rustc` no intervalo `1.88..1.95`.
- Causada pela dependência transitiva `constant_time_eq 0.4.3` (puxada via `blake3`).
- Essa dependência elevou seu MSRV para 1.95.0 em uma patch release.
- `cargo install sqlite-graphrag` sem `--locked` agora sucede.
- Pin direto `constant_time_eq = "=0.4.2"` força versão compatível com `rust-version = "1.88"`.

### Alterado

- `Cargo.toml` agora declara pin preventivo explícito `constant_time_eq = "=0.4.2"`.
- Comentário inline documenta a razão do drift de MSRV.
- Pin será revisitado quando `rust-version` for elevado para 1.95.
- Instruções de instalação do `README.md` (EN e PT) atualizadas para `cargo install --locked sqlite-graphrag`.
- Bullet adicionado explicando a motivação para `--locked`.

### Adicionado

- Seção `docs_rules/prd.md` "Dependency MSRV Drift Protection" documenta o padrão canônico de mitigação.
- Padrão: pinagem direta de dependências transitivas problemáticas no `Cargo.toml` de nível superior.


## [1.2.0] - 2026-04-18

### Adicionado

- Semáforo de contagem cross-process com até 4 slots simultâneos via `src/lock.rs` (`acquire_cli_slot`).
- Memory guard abortando com exit 77 quando RAM livre está abaixo de 2 GB via `sysinfo` (`src/memory_guard.rs`).
- Signal handler para SIGINT, SIGTERM e SIGHUP via `ctrlc` com feature `termination`.
- Flag `--max-concurrency <N>` para controlar limite de invocações paralelas em runtime.
- Flag oculta `--skip-memory-guard` para testes automatizados onde a alocação real não ocorre.
- Constantes `MAX_CONCURRENT_CLI_INSTANCES`, `MIN_AVAILABLE_MEMORY_MB`, `CLI_LOCK_DEFAULT_WAIT_SECS` em `src/constants.rs`.
- Constantes `EMBEDDING_LOAD_EXPECTED_RSS_MB` e `LOW_MEMORY_EXIT_CODE` em `src/constants.rs`.
- Variantes `AppError::AllSlotsFull` e `AppError::LowMemory` com mensagens em português brasileiro.
- Global `SHUTDOWN: AtomicBool` e função `shutdown_requested()` em `src/lib.rs`.

### Alterado

- Default da flag `--wait-lock` aumentado para 300 segundos (5 minutos) via `CLI_LOCK_DEFAULT_WAIT_SECS`.
- Lock file migrado de `cli.lock` único para `cli-slot-{N}.lock` (semáforo de contagem N=1..4).

### Removido

- BREAKING: flag `--allow-parallel` removida; causou OOM crítico em produção (incidente 2026-04-18).

### Corrigido

- Bug crítico onde invocações CLI paralelas esgotavam a RAM do sistema.
- 58 invocações simultâneas travaram o computador por 38 minutos (incidente 2026-04-18).


## [Unreleased]

### Adicionado

- Flags globais `--allow-parallel` e `--wait-lock SECONDS` para concorrência controlada.
- Módulo `src/lock.rs` implementando lock single-instance baseado em arquivo via `fs4`.
- Nova variante `AppError::LockBusy` mapeando para exit code 75 (`EX_TEMPFAIL`).
- Variáveis de ambiente `ORT_NUM_THREADS`, `OMP_NUM_THREADS` e `ORT_INTRA_OP_NUM_THREADS` pré-definidas para 1.
- Singleton `OnceLock<Mutex<TextEmbedding>>` para reuso do modelo intra-processo.
- Testes de integração em `tests/lock_integration.rs` cobrindo aquisição e liberação de lock.
- `.cargo/config.toml` com `RUST_TEST_THREADS` conservador padrão e aliases cargo padronizados.
- `.config/nextest.toml` com profiles `default`, `ci`, `heavy` e override `threads-required` para loom e stress.
- `scripts/test-loom.sh` como invocação canônica local com `RUSTFLAGS="--cfg loom"`.
- `docs/TESTING.md` e `docs/TESTING.pt-BR.md` guia bilíngue de testes.
- Feature Cargo `slow-tests` para futuros testes pesados opt-in.

### Alterado

- Comportamento padrão agora é single-instance.
- Uma segunda invocação concorrente sai com código 75 exceto se `--allow-parallel` for passada.
- Módulo embedder refatorado de struct-com-estado para funções livres operando sobre um singleton.
- Mover `loom = "0.7"` para `[target.'cfg(loom)'.dev-dependencies]` — ignorado em cargo test padrão.
- Remover feature Cargo legada `loom-tests` substituída pelo gate oficial `#[cfg(loom)]`.
- Workflow CI `ci.yml` migrado para `cargo nextest run --profile ci` com `RUST_TEST_THREADS` explícito por job.
- Job CI loom agora exporta `LOOM_MAX_PREEMPTIONS=2`, `LOOM_MAX_BRANCHES=500`, `RUST_TEST_THREADS=1`, `--release`.

### Corrigido

- Previne OOM livelock quando a CLI é invocada em paralelismo massivo por orquestradores LLM.
- Previne livelock térmico nos testes loom ao alinhar gate `#[cfg(loom)]` com padrão upstream.
- Serializa `tests/loom_lock_slots.rs` com `#[serial(loom_model)]` para impedir execução paralela dos modelos loom.


## [0.1.0] - 2026-04-17

### Adicionado

- Fase 1: Fundação: schema SQLite com vec0 (sqlite-vec), FTS5, grafo de entidades.
- Fase 2: Subcomandos essenciais: init, remember, recall, read, list, forget, rename, edit, history.
- Fase 2 continuação: restore, health, stats, optimize, purge, vacuum, migrate, hybrid-search.
- Fase 2 continuação: namespace-detect, sync-safe-copy.

### Corrigido

- Bug de corrupção FTS5 external-content no ciclo forget+purge.
- Removido DELETE manual em forget.rs que causava a corrupção.

### Alterado

- MSRV elevado de 1.80 para 1.88 (exigido por dependências transitivas base64ct 1.8.3, ort-sys, time).

- Os links históricos abaixo continuam apontando para o repositório legado `neurographrag`
- O projeto renomeado inicia sua linha pública de versões em `sqlite-graphrag v1.0.0`

[Unreleased]: https://github.com/daniloaguiarbr/neurographrag/compare/v2.3.0...HEAD
[2.1.0]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v2.1.0
[2.0.2]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v2.0.2
[2.0.1]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v2.0.1
[2.0.0]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v2.0.0
[1.2.1]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v1.2.1
[1.2.0]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v1.2.0
[0.1.0]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v0.1.0
