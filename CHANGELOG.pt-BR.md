Leia este documento em [inglês (EN)](CHANGELOG.md).


# Changelog

Todas as mudanças notáveis deste projeto serão documentadas neste arquivo.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/),
e este projeto adere ao [Semantic Versioning](https://semver.org/lang/pt-BR/spec/v2.0.0.html).

## [Sem Versão]

## [1.0.37] - 2026-04-30

### Corrigido
- **B1+B2 (BLOQUEANTE, docs)**: Sincronizado `CHANGELOG.pt-BR.md` com a entrada v1.0.36 (estava ausente no espelho PT) e adicionados dois callouts faltantes em `README.pt-BR.md:108-109` espelhando `README.md` ("**Execute `init` primeiro**" e "**`graphrag.sqlite` é criado no diretório de trabalho atual por padrão**"). Auditoria no corpus flowaiper revelou que usuários PT-BR não descobriam o comportamento implícito do cwd.
- **H7+M9 (HIGH, comportamento)**: `list --include-deleted --json` agora emite `deleted_at` (Unix epoch) e `deleted_at_iso` (RFC 3339) para memórias soft-deleted. Memórias ativas continuam omitindo ambos os campos via `#[serde(skip_serializing_if = "Option::is_none")]` para backward compatibility. `MemoryRow` em `src/storage/memories.rs` ganhou campo `deleted_at: Option<i64>`; todos os quatro SELECTs SQL atualizados para incluir a coluna. `docs/schemas/list.schema.json` atualizado para documentar ambos os campos opcionais. Anteriormente agentes LLM chamando `list --include-deleted` não conseguiam distinguir linhas ativas de soft-deleted sem uma segunda query SQL.
- **H8 (HIGH, comportamento)**: `src/i18n.rs::Language::from_env_or_locale` agora respeita a precedência POSIX `LC_ALL > LC_MESSAGES > LANG`. O loop anterior iterava todas as três variáveis e retornava PT no primeiro prefixo "pt", violando semântica POSIX onde `LC_ALL` sobrescreve `LANG` independente do valor (`LC_ALL=en_US LANG=pt_BR` retornava PT em vez de EN). A correção para iteração na primeira variável setada, reconhece ambos os prefixos "pt" e "en", e cai no padrão English somente quando nenhuma variável de locale está setada. Três novos testes de regressão cobrem a regra de precedência.

### Adicionado
- **H9 (hardening CI)**: Novo job `cargo-audit` em `.github/workflows/ci.yml` executa `cargo audit --deny warnings`. Complementa `cargo deny check`, que anteriormente não sinalizava `RUSTSEC-2025-0119` (number_prefix unmaintained, transitiva via fastembed/hf-hub/indicatif) nem `RUSTSEC-2024-0436` (paste unmaintained, transitiva via tokenizers/text-splitter). Qualquer novo advisory agora bloqueia o merge até reconhecimento ou pin.
- **B6 (multiplataforma)**: Adicionado target `x86_64-unknown-linux-musl` à matriz de `.github/workflows/release.yml` (usa o step existente `Install musl tools` condicionado a `matrix.musl == true`). Habilita deploys em Alpine Linux e containers distroless sem forçar usuários a compilar do código.
- **B3 (docs)**: Criado `docs_rules/rules_rust.md` como índice canônico da Regra Zero referenciada pelo `CLAUDE.md` do projeto. Lista todos os oito arquivos de regras específicas em `docs_rules/` com resumos de uma linha e princípios invioláveis.
- **B4 (docs)**: Renomeado `docs_rules/rules_rusts_paralelismo_e_multiprocessamento.md` para `rules_rust_paralelismo_e_multiprocessamento.md` (correção de typo: `s` extra). O arquivo é gitignored e excluído do tarball publicado, então o rename não é visível para consumidores do crates.io.

### Melhorado
- **H1 (HIGH, extração)**: Expandido `ALL_CAPS_STOPWORDS` em `src/extraction.rs:58-173` com 23 palavras técnicas/genéricas PT-BR adicionais encontradas vazando para `entities` durante auditoria de 50 arquivos do corpus flowaiper: `ACID`, `AINDA`, `APENAS`, `CEO`, `CRIE`, `DDL`, `DEFINIR`, `DEPARTMENT`, `DESC`, `DSL`, `DTO`, `EPERM` (errno POSIX), `ESCREVA`, `ESRCH` (errno POSIX), `ESTADO`, `FATO`, `FIFO` (estrutura de dados), `FLUXO`, `FONTES`, `FUNCIONA`, `MESMO`, `METADADOS`, `PONTEIROS`. Lista cresceu de 108 para 131 entradas; anteriormente essas palavras eram capturadas por `regex_all_caps()` como entidades espúrias `concept`, poluindo o grafo com não-entidades (~27% das 402 entidades em corpus de 50 docs eram ruído). Filtro de stopwords está em ordem alfabética para leitura/revisão e usa scan linear via `.contains()`.

### Notas
- Findings descobertos durante o ciclo de auditoria v1.0.36 sobre o corpus real `flowaiper/docs_flowaiper` (495 arquivos markdown PT-BR). Fases A/B/C/D completaram (D=200/200), fase E (495/495) estava rodando no momento destas correções.
- Backlog v1.0.38+ remanescente: dedupe case-insensitive de entidades (CLAUDE/Claude, GEMINI/Gemini, GITHUB/GitHub vazando como entidades separadas), alinhamento hífen vs underscore em relations (CLI aceita `depends-on`, schema CHECK usa `depends_on`), ADR sobre daemon vs `rules_rust_cli_stdin_stdout` ("PROIBIDO daemons persistentes"), e targets multiplataforma remanescentes (`x86_64-apple-darwin`, `wasm32-wasip2`, universal2 macOS).
- Todos os oito gates de validação CLAUDE.md passam: fmt, clippy `-D warnings`, test (431/434, 3 ignorados), doc com `RUSTDOCFLAGS="-D warnings"`, audit com ignores documentados para dois advisories transitivos unmaintained pendentes upstream, deny check, publish dry-run, package list (138 arquivos, zero sensíveis).

## [1.0.36] - 2026-04-30

### Corrigido (Política linguística)
- **C1 (CRITICAL)**: Sincronizado enum `--type` em `skill/sqlite-graphrag-en/SKILL.md:46` e `-pt/SKILL.md:46` de 4 valores listados para o conjunto completo de 9 (`user, feedback, project, reference, decision, incident, skill, document, note`). Agentes usando SKILL.md como contrato perdiam silenciosamente cinco tipos de memória desde v1.0.30. Fonte de verdade: `src/cli.rs:364-374` (enum `MemoryType`) e `src/commands/remember.rs:26` long-help.
- **H1+H2+H3 (HIGH)**: Traduzidas três strings em português sem acentos em macros `tracing::warn!` que escaparam do gate de auditoria `rg '[áéíóúâêôãõç]' src/` documentado em v1.0.33: `src/extraction.rs:1204` (`"NER falhou..."` → `"NER failed..."`), `src/extraction.rs:964` (`"batch NER falhou (chunk de N janelas)..."` → `"batch NER failed (chunk of N windows)..."`), `src/commands/remember.rs:345` (`"auto-extraction falhou..."` → `"auto-extraction failed..."`). Bônus: também traduzidos `src/storage/urls.rs:37` (`"falha ao persistir url..."` → `"failed to persist url..."`) e o erro de produção em `src/commands/remember.rs:367` (`"limite de N namespaces ativos excedido..."` → `"active namespace limit of N reached..."`).
- **M1 (MEDIUM)**: Adicionado gate complementar de CI no job `language-check` de `.github/workflows/ci.yml` que escaneia macros `tracing::*!`, `#[error(...)]`, doc comments e `panic!`/`assert!`/`expect`/`bail!`/`ensure!` para palavras em português sem marcas diacríticas (`falhou`, `janelas`, `usando apenas`, `nao foi`, `ja existe`, `obrigatorio`, `memoria`, etc.). String literals simples não são escaneadas intencionalmente porque carregam fixtures legítimas em PT para extração multilíngue.
- **M3 (MEDIUM)**: Renomeados 33 nomes de funções de teste em português para inglês em `tests/integration.rs`, `tests/exit_codes_integration.rs`, `tests/concurrency_limit_integration.rs`, `tests/recall_integration.rs`, `tests/prd_compliance.rs`, `tests/loom_lock_slots.rs`, `tests/vacuum_integration.rs`, `src/commands/optimize.rs`, `list.rs`, `health.rs`, `debug_schema.rs`, `unlink.rs`. Exemplos: `test_link_idempotente_retorna_already_exists` → `test_link_idempotent_returns_already_exists`; `prd_optimize_executa_e_retorna_status_ok` → `prd_optimize_runs_and_returns_status_ok`; `optimize_response_serializa_campos_obrigatorios` → `optimize_response_serializes_required_fields`. Mais ~80 helpers `.expect("X falhou")` traduzidos para `.expect("X failed")`, doc comments e mensagens de assert limpas em `src/graph.rs`, `src/memory_guard.rs`, `src/cli.rs`, `src/storage/entities.rs` e diversos arquivos `tests/*.rs`. STRINGS de fixture que exercitam ingestão PT-BR (ex.: inputs NER multilíngue) permanecem intencionalmente em PT-BR.

### Corrigido (Lógica do código)
- **H5 (HIGH)**: Estendido `regex_section_marker()` em `src/extraction.rs:210-218` para incluir `Camada` ao lado de `Etapa`, `Fase`, `Passo`, `Seção`, `Capítulo`. Auditoria sobre corpus PT-BR de 50 arquivos mostrou `Camada 1` a `Camada 5` vazando para `entities` com degree 3 cada, poluindo o grafo. O filtro agora remove esses tokens tanto no estágio de regex prefilter quanto no post-merge BERT NER.
- **M7 (MEDIUM)**: Expandido `ALL_CAPS_STOPWORDS` em `src/extraction.rs:60-165` com `ADICIONADA`, `ADICIONADAS`, `ADICIONADO`, `ADICIONADOS`, `CLARO`, `CONFIRMARAM`, `CONFIRMEI`, `CONFIRMOU` (mesclados alfabeticamente na lista). A auditoria anterior encontrou essas formas verbais e adjetivas PT-BR sendo capturadas como entidades `concept` por `regex_all_caps()` em `apply_regex_prefilter`.
- **L2 (LOW)**: Backoff de spawn do daemon em `src/daemon.rs:record_spawn_failure` agora aplica half jitter (`base/2 + rand([0, base/2))`) em vez de exponencial puro. Evita retry herd se múltiplas instâncias da CLI detectarem falha do daemon simultaneamente. Usa `SystemTime::now().subsec_nanos()` como fonte de entropia sem dependências — suficiente para coordenação de spawn de baixa frequência.
- **L5+L6 (LOW)**: `src/i18n.rs::Language::from_env_or_locale` agora trata `SQLITE_GRAPHRAG_LANG=""` vazio como não-definido (sem `tracing::warn!` emitido), seguindo convenção POSIX. `src/i18n.rs::init` faz short-circuit quando o OnceLock já está populado, evitando que o resolvedor de env rode uma segunda vez e emita o warning duas vezes.

### Melhorado
- **M2 (MEDIUM)**: Adicionada seção "JSON Schemas" em `README.md`, `README.pt-BR.md`, `docs/AGENT_PROTOCOL.md` e `docs/AGENT_PROTOCOL.pt-BR.md` com link para os 30 arquivos canônicos de JSON Schema em `docs/schemas/`. Esses contratos existiam desde v1.0.33 mas eram indescobríveis a partir da documentação pública.
- **M4 (MEDIUM)**: `src/i18n.rs::tr` não vaza mais uma alocação por chamada. A assinatura agora exige inputs `&'static str` (que todos os chamadores no repositório já passam — são string literals) e retorna um deles diretamente. O padrão anterior `Box::leak(en.to_string().into_boxed_str())` acumulava alocações em pipelines de longa duração.
- **L3 (LOW)**: Adicionado callout MSRV (Rust 1.88) nas seções Installation de `README.md` e `README.pt-BR.md`. Anteriormente documentado apenas como nota de rodapé nas notas Mac Intel.

### Notas
- **M6 reclassificado como artefato de documentação/teste**: foi reportado que `related --json` retornava `graph_depth: null`, mas o campo se chama `hop_distance` (`src/commands/related.rs:77` e chave serializada). A query da auditoria usou `.graph_depth` que não existia. O campo sempre esteve corretamente populado. Sem mudança de código necessária.
- **L1 (sys_locale) foi diferido**: o parsing manual de `LC_ALL`/`LANG` em `src/i18n.rs:34-57` funciona corretamente nos targets usados no CI. Adicionar `sys_locale` introduziria uma dependência para benefício marginal (APIs CFLocale do macOS e GetUserDefaultLocaleName do Windows) sem reproducer confirmado.
- **L4 (BERT NER misclassifications) está fora de escopo**: `Tokio=location`, `Borda=person`, `Campos=location` e `AdapterRun=organization` são limitações de `Davlan/bert-base-multilingual-cased-ner-hrl`. Filtrar exigiria modelo diferente ou whitelist curada; ambos diferidos até causarem impacto concreto ao usuário.
- Todos os 427 testes de lib passam com os novos nomes de teste e assertions traduzidas. `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo doc`, `cargo audit` e `cargo deny check advisories licenses bans sources` estão limpos.
- O novo gate `language-check` no CI agora bloqueia qualquer PR que reintroduza PT em superfícies tracing/error/doc/assert.

## [1.0.35] - 2026-04-30

### Corrigido
- **WAL-AUTO-INIT (HIGH)**: O caminho de auto-init (`remember`, `ingest`, `recall`, `list`, ... — todo comando que passa por `ensure_db_ready()`) agora ativa `journal_mode=wal` consistentemente. Antes da v1.0.35 apenas o comando `init` explícito alterava o journal mode para WAL; bancos criados sob demanda por outros comandos permaneciam em `journal_mode=delete`, quebrando a semântica de checkpoint do `sync-safe-copy`, as garantias de concorrência documentadas e o conselho de troubleshooting que referenciava WAL. A correção move `PRAGMA journal_mode = WAL` para `apply_connection_pragmas` (chamado por todo `open_rw`) e adiciona uma re-asserção defensiva (`ensure_wal_mode`) após migrações para neutralizar o reuso interno de handles do refinery. Cobertura de regressão: `tests/wal_auto_init_regression.rs`.
- **JSON-SCHEMA-VERSION (MEDIUM-HIGH)**: `init --json`, `stats --json` e `migrate --json` agora emitem `schema_version` como **número** JSON em vez de string, alinhando com `health --json` (que já usava número). Corrige inconsistência de parsing para clientes que consumiam ambos os formatos. **Quebra** clientes que comparavam explicitamente como string; clientes usando comparação numérica não são afetados.
- **DAEMON-SOCKET-FALLBACK (LOW)**: Caminho de fallback do socket Unix em `to_local_socket_name()` agora respeita `XDG_RUNTIME_DIR` e em seguida `SQLITE_GRAPHRAG_HOME` antes de cair para `/tmp`. Reduz risco de colisão em hosts multi-tenant. O caminho só é usado quando sockets de namespace abstrato falham ao bindar (raro).

### Adicionado
- **CLI-LIMIT-ALIAS (UX)**: `recall` e `hybrid-search` agora aceitam `--limit` como alias de `-k/--k`. Alinhamento com `list`/`related` que já usavam `--limit`. Não-quebrante, aditivo.
- **CLI-RENAME-FROM-TO (UX)**: `rename` agora aceita `--from`/`--to` como aliases de `--name`/`--new-name`. Não-quebrante, aditivo.
- **JSON-RELATED-INPUT-ECHO (UX)**: A resposta `related --json` agora inclui campos `name` e `max_hops` ecoando o input para transparência. Não-quebrante, aditivo.

### Modificado
- **GRAPH-NODE-KIND-DEPRECATED**: `graph --format json` ainda emite ambos os campos `kind` e `type` por nó, mas `kind` está agora formalmente documentado como **deprecated** (mantido para backward-compat pré-v1.0.35). Novos consumidores DEVEM ler `type`. O campo duplicado será removido em uma futura release maior.

### Documentação
- **PRAGMA-USER-VERSION-49**: Adicionado doc comment em `src/constants.rs` explicando por que `SCHEMA_USER_VERSION = 49` (assinatura do projeto para ferramentas externas) versus `CURRENT_SCHEMA_VERSION = 9` (contagem de migrações aplicacional). São intencionalmente diferentes e servem propósitos distintos.
- **README**: Tabela do ciclo de vida de conteúdo de memória expandida com flags `--body-file`/`--body-stdin`/`--entities-file`/`--relationships-file`/`--graph-stdin` para `remember`, novos aliases para `recall`/`rename`, e callout sobre validação de nomes em ASCII kebab-case. Linhas explícitas para `ingest` e `cache clear-models` adicionadas.
- **JSON Schemas**: `docs/schemas/stats.schema.json`, `docs/schemas/migrate.schema.json` e `docs/schemas/debug-schema.schema.json` atualizados refletindo `schema_version` como integer e clarificando a relação `user_version` (49) vs `schema_version` (9) como intencionalmente independentes.

### Notas
- Achados da auditoria #4 (flags estruturadas de truncamento na saída JSON) e #6 (progress/ETA no resumo de ingest) ficam diferidos para v1.0.36 — requerem design de schema além de uma release de patch. Truncamento atualmente é exposto apenas via `tracing::warn!`; consumidores de pipeline devem monitorar stderr.
- Todos os 427 testes de lib passam. Teste de regressão `wal_auto_init_regression.rs` adicionado (usa `assert_cmd` + `tempfile`, mesmo padrão dos testes de integração existentes).
- Entradas detalhadas para v1.0.32, v1.0.33 e v1.0.34 abaixo trazem resumo executivo; o detalhamento completo permanece em `CHANGELOG.md` (EN) que é a fonte canônica.

## [1.0.34] - 2026-04-30

### Adicionado
- **JS7 (LOW)**: `vacuum --json` agora inclui o campo `reclaimed_bytes: u64` (calculado como `size_before_bytes.saturating_sub(size_after_bytes)`).

### Documentação
- **PRD-sync (LOW)**: `docs_rules/prd.md` (excluído do crate publicado) atualizado para refletir os enums atuais de MemoryType (9) e EntityType (13) após V008 e V009.

### Notas
- Auditoria de `unwrap`/`expect`: ZERO unwraps em produção; 12 expects em produção todos com invariantes documentados (compile-time, BERT NER, OnceLock, regex literais).
- Auditoria de blocos `unsafe`: todos com comentários SAFETY (~14 blocos em main.rs/embedder.rs/connection.rs/optimize.rs/paths.rs).
- Bump de patch: `reclaimed_bytes` é puramente aditivo; sem API removida; sem mudança comportamental.

## [1.0.33] - 2026-04-30

### Corrigido (Política Linguística)
- **C3-residual (HIGH)**: Traduzida string em português remanescente em `src/daemon.rs:183` (Drop impl). Gate `rg '[áéíóúâêôãõç]' src/ -g '!i18n.rs'` agora retorna ZERO matches.
- **PT-V007 (HIGH)**: Traduzido cabeçalho SQL de 5 linhas em português em `migrations/V007__memory_urls.sql` para inglês (arquivo é parte do crate publicado).
- **AS-PT (MEDIUM)**: Traduzidas 20 mensagens de `assert!` em português para inglês em `src/commands/hybrid_search.rs` (19) e `src/commands/list.rs` (1) + 9 mensagens em `src/storage/memories.rs`.

### Corrigido (Documentação)
- **D3 (MEDIUM)**: Sincronizado doc-comment de `--type` em `recall.rs`, `list.rs`, `hybrid_search.rs` para listar todos os 13 tipos de entidade do grafo (project/tool/person/file/concept/incident/decision/memory/dashboard/issue_tracker/organization/location/date) — antes listava apenas 10.

### Notas
- Validado contra ingest real de 50 arquivos `.md` (~6.6 MB): 50/50 indexados em 56.9s com `--skip-extraction`; 5/5 com extração BERT completa em 57.3s. Auto-create de `graphrag.sqlite` com modo 0600 confirmado.
- Campos JSON duplicados em `stats --json` e `list --json` preservados intencionalmente para backward compat.
- Assimetria de tipo `schema_version` entre `stats --json` (String) e `health --json` (u32) documentada como issue conhecida — corrigida posteriormente em v1.0.35.

## [1.0.32] - 2026-04-30

### Corrigido (Crítico — achados de auditoria de v1.0.31)
- **C1 (CRITICAL)**: Auto-init unificado em todos os handlers CRUD via novo helper `ensure_db_ready` em `src/storage/connection.rs`. Antes apenas `remember` auto-criava o DB; agora todos os subcomandos CRUD criam o banco no primeiro uso.
- **C2 (CRITICAL)**: Documentado o detach deliberado do daemon órfão em `src/daemon.rs:487` com comentário SAFETY explicando ownership de ciclo de vida via spawn lock + ready file + idle-timeout shutdown.
- **C3 (CRITICAL)**: Novo teste de integração `tests/readme_examples_executable.rs` parseia todos os blocos bash de `README.md` e `README.pt-BR.md` em compile time e executa cada invocação contra um binário real.

### Corrigido (Alto)
- **A1 (HIGH)**: Traduzidas 8 strings PT runtime para EN em `src/lock.rs`, `src/daemon.rs`. Adicionadas mensagens i18n bilíngues para validação de query/body vazios.
- **A2 (HIGH)**: Refatorado `src/commands/ingest.rs` de fork-spawn por arquivo para pipeline in-process. **40× mais rápido**: 50 arquivos em 21s (vs ~14 min antes).
- **A3 (HIGH)**: Substituído `.expect("OnceLock populated by set() above")` em `src/embedder.rs:56` por `.ok_or_else(...)?` propagando erro real.
- **A4 (HIGH)**: Adicionado `#[command(after_long_help = "EXAMPLES: ...")]` com 2-4 invocações realistas a 21 subcomandos previamente sem.
- **A5 (HIGH)**: Auto-migração transparente. `ensure_db_ready` compara `PRAGMA user_version` com `SCHEMA_USER_VERSION` e roda migrações pendentes automaticamente quando DB antigo é aberto por binário novo.
- **A6 (HIGH)**: Renomeados 23 identificadores PT para EN em testes e fontes; comentários PT residuais traduzidos.

### Corrigido (Médio)
- **M1 (MEDIUM)**: `recall -k` e `hybrid-search -k` agora usam `value_parser = parse_k_range` validando intervalo `1..=4096` em parse time.
- **M2 (MEDIUM)**: UX de `purge` clarificada com alias `--max-age-days` e mensagem helpful quando `purged_count == 0`.
- **M3 (MEDIUM)**: Adicionado `#[arg(help = "...")]` a 9 argumentos posicionais previamente sem help.
- **M4 (MEDIUM)**: Verificado que `daemon --stop` já existe; design de detach orfão documentado.

### Corrigido (Baixo)
- **B_1-B_4 (LOW)**: Estrutura README, badge CI, exemplos bash para 16 subcomandos, e campos `name_was_normalized`/`original_name` em saída JSON de `remember`.

### Adicionado
- `tests/readme_examples_executable.rs` (442 linhas, 10 testes).
- `parse_k_range` value parser em `src/parsers/mod.rs`.
- `validation::empty_query()` / `validation::empty_body()` em `src/i18n.rs`.
- `ensure_db_ready(&AppPaths)` em `src/storage/connection.rs`.

### Notas
- Pipeline de validação: `cargo fmt --check` ✓, `cargo clippy -- -D warnings` ✓, `cargo test --lib` 427/427 ✓, `cargo doc --no-deps` ✓, `cargo audit` ✓, `cargo deny` ✓.
- Gate de linguagem: `rg '[áéíóúâêôãõç]' src/ -g '!i18n.rs'` ZERO matches.
- Performance: 50 arquivos ingest em 21s (≈40× mais rápido que v1.0.31).

## [1.0.31] - 2026-04-30

### Corrigido
- **A2 (P1-CRÍTICO)**: subcomando `ingest` agora emite NDJSON correto (um objeto JSON por linha). Antes emitia JSON multilinha indentado, quebrando consumidores line-by-line. 5 chamadas em `src/commands/ingest.rs` trocadas de `output::emit_json` para `output::emit_json_compact`.
- **A3 (P1-MÉDIO)**: `stats --json` agora reporta `schema_version` correto (ex.: "9") lendo de `refinery_schema_history`. Antes retornava "unknown" porque a tabela `schema_meta` (vazia) era consultada.
- **A4 (P1-MÉDIO)**: comando `forget` agora popula `action` e `deleted_at` no JSON de saída. Três estados explícitos: `soft_deleted`, `already_deleted`, `not_found`. Race-safe via re-SELECT após soft-delete.
- **A1 (P0-CRÍTICO)**: pipeline de extração não trava mais em documentos > 50 KB. Adicionado cap `EXTRACTION_MAX_TOKENS=5000` (override via env `SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS`). Body que excede o cap é truncado para NER mas o body completo continua passando pelo regex. Impacto empírico: documento de 68 KB caiu de >5 minutos para ~37 segundos (88% de redução), mantendo `extraction_method=bert+regex-batch`.
- **A9 (P2-MÉDIO)**: fan-out de relacionamentos reduzido — entidades co-ocorrendo na mesma sentença/parágrafo agora geram edges; antes gerava C(N,2) "mentions" entre todas entidades da memória.
- **A10 (P2-MÉDIO)**: truncamento de nome em 60 chars agora emite `tracing::warn` e trata colisões com sufixo numérico (-1, -2, ...) dentro da mesma run.

### Adicionado
- **A6**: nova suite `tests/ingest_integration.rs` cobrindo contrato NDJSON, fail-fast, max-files, truncamento de nome, --skip-extraction, variantes de --pattern, walk recursivo (10 testes).
- **A7**: testes E2E para V009 em `tests/schema_migration_integration.rs`: `v009_document_type_lifecycle_e2e`, `v009_note_type_lifecycle_e2e`, `v009_invalid_type_rejected`.
- **A11**: stoplist PT-BR de palavras em caixa alta para filtro NER (ADAPTER, PROJETO, PASSIVA, SOMENTE, LEITURA, etc.). Melhora qualidade da extração para corpora em português.

### Melhorado
- **A5 (P1-MÉDIO)**: 210 funções de teste em `src/*` renomeadas de português para inglês em 35 arquivos (cobre também helpers como `nova_memoria` → `new_memory`, `cria_node` → `make_node`, `resposta_vazia` → `empty_response`). Codebase agora 100% em conformidade com a política linguística do projeto (identificadores exclusivos em inglês).
- **A8 (P1-MÉDIO)**: refinamento de `.unwrap()`/`.expect()` em código de produção. A contagem original da auditoria de 167 estava inflada — a maioria dos matches estava em blocos `#[cfg(test)] mod tests` (aceitáveis pelo CLAUDE.md). O inventário real em produção era de 13 ocorrências. Melhorias: 1 `.expect()` em `src/embedder.rs` recebeu mensagem de invariante mais precisa; 10 `Regex::new(LITERAL).unwrap()` em initializers de `OnceLock` em `src/extraction.rs` substituídos por `.expect("compile-time validated <kind> regex literal")`; 2 `.max_by(...).unwrap()` sobre logits do BERT NER substituídos por `.expect("BERT NER logits invariant: no NaN in classifier output")`; 1 `.expect()` em `src/chunking.rs` traduzido de PT para EN.
- **A12+A13**: ~38 comentários PT traduzidos em `tests/signal_handling_integration.rs`, `tests/lock_integration.rs` e `deny.toml`. 2 entries `[advisories.ignore]` obsoletas removidas (RUSTSEC-2024-0436, RUSTSEC-2025-0119) — `cargo deny check` agora reporta zero warnings de advisory-not-detected.
- **A14**: ~150 comentários PT adicionais traduzidos em `tests/prd_compliance.rs`, `tests/integration.rs`, `tests/concurrency_hardened.rs`, `tests/security_hardening.rs` e outros arquivos de teste.

### Metodologia da Auditoria
- 13 gaps identificados empiricamente via auditoria em plan-mode contra binário v1.0.30 instalado, usando corpus real (20 documentos markdown PT-BR).
- Todos os fixes validados via PDCA + orquestração com Agent Teams: 13 tasks, 9 teammates spawnados em paralelo, cada um com Regra Zero do CLAUDE.md e validação por task.
- Validação aprovada: cargo fmt, cargo clippy --all-targets -- -D warnings, cargo audit, cargo deny check, cargo doc -D warnings, cargo nextest run.

## [1.0.30] - 2026-04-29

### Adicionado (Novo Subcomando — Ingestão em Massa)
- `sqlite-graphrag ingest <DIR> --type <TYPE>` para indexar em massa todo arquivo de uma pasta como memória separada. Flags: `--pattern` (default `*.md`), `--recursive`, `--skip-extraction`, `--fail-fast`, `--max-files` (cap default 10000), `--namespace`, `--db`. Saída NDJSON: um objeto por arquivo (`{file, name, status, memory_id, action}`) seguido de summary final (`{summary, files_total, files_succeeded, files_failed, files_skipped, elapsed_ms}`). Nome derivado do basename em kebab-case. Cada arquivo é processado por subprocesso `remember --body-file`, preservando slots de concorrência, locks e semântica de erro do `remember` standalone. Resolve gap UX de longa data onde usuário precisava `for f in *.md; do remember ...; done`.

### Alterado (Help mais Claro — `link` / `unlink`)
- `link --help` e `unlink --help` agora deixam EXPLÍCITO que `--from` e `--to` aceitam nomes de ENTIDADES (nós do grafo extraídos por BERT NER, ou criados implicitamente por `link` anterior), NÃO nomes de memória. Inclui blocos `EXAMPLES:` e `NOTES:` em `after_long_help`. O help anterior "Source entity" era facilmente mal interpretado como "nome de memória"; o erro `Erro: entidade '<name>' não existe` confundia o usuário. Doc comments agora apontam `graph --format json | jaq '.nodes[].name'` como forma canônica de listar entidades elegíveis.

### Alterado (Dependências — upgrade rusqlite/refinery)
- `rusqlite` 0.32 → 0.37 e `refinery` 0.8 → 0.9. Cargo.lock agora resolve `rusqlite v0.37.0`, `refinery v0.9.1`, `refinery-core v0.9.1`, `refinery-macros v0.9.1`, `libsqlite3-sys v0.35.0`. Zero mudanças de código fonte — ambos crates mantiveram APIs públicas estáveis. Tentativa de chegar a rusqlite 0.39 foi bloqueada por `refinery-core 0.9.0` com cap `rusqlite = ">=0.23, <=0.37"`; revisitar quando refinery elevar esse teto.

### Corrigido (Crítico — Inconsistência de Schema/Contrato CLI)
- `migrations/V009__expand_memory_types.sql` — nova migration que recria a tabela `memories` (e suas filhas com FK: `memory_versions`, `memory_chunks`, `memory_entities`, `memory_relationships`, `memory_urls`) para expandir o CHECK do campo `type` de 7 para 9 valores, adicionando `'document'` e `'note'`. Sem essa migration, `--type document` e `--type note` (adicionados ao enum da CLI em v1.0.29) eram sempre rejeitados em runtime com `exit 10` — `CHECK constraint failed: type IN ('user','feedback','project','reference','decision','incident','skill')`. A camada Clap aceitava nove valores enquanto o banco impunha apenas sete, quebrando todos os exemplos do README que usavam `--type document`.
- `tests/schema_migration_integration.rs` atualizado para assert exatamente 9 migrations aplicadas (antes esperava 6) e `schema_version = "9"`.

### Corrigido (Crítico — Violações de Política Linguística Não Detectadas em v1.0.28)
A auditoria v1.0.28 usou regex de linha única e reportou zero violações; macros multi-linha e identificadores sem acentos escaparam. Corrigido nesta versão:

- `src/extraction.rs:749, 1025` — 2 `tracing::warn!` em PT traduzidos para EN.
- `src/extraction.rs` — 8 chamadas `.context(...)`, `.with_context(...)` e `anyhow::anyhow!` em PT traduzidas (forward pass, removendo dimensão batch, criando tensor de ids/máscara/diretório do modelo, carregando tokenizer NER, encoding NER).
- `src/daemon.rs` — 2 strings `tracing::*!` traduzidas (lock file de spawn, daemon encerrado graciosamente).
- `src/commands/restore.rs` — 1 `tracing::info!` traduzido (`restore --version omitido`).

### Corrigido (Identificadores de Teste — Política Inglês-Apenas)
~80 identificadores de teste (nomes de função, helpers, módulos `mod`, type aliases) renomeados de PT para EN. Phase 1 só pegou subset com diacríticos; identificadores sem acento (`*_aceita_`, `*_rejeita`, `*_funciona`, `*_retorna`, etc.) foram missed. Arquivos tocados:

- `src/cli.rs`, `src/paths.rs`, `src/errors.rs`, `src/commands/{init, migrate, sync_safe_copy, cleanup_orphans, list, vacuum}.rs`, `src/extraction.rs`, `src/output.rs`, `src/memory_guard.rs`, `src/storage/{urls, memories, entities}.rs`.
- `tests/security_hardening.rs` (16 fns), `tests/integration.rs` (~28 fns), `tests/prd_compliance.rs` (~15 fns), `tests/concurrency_*.rs`, `tests/i18n_bilingual_integration.rs`, `tests/signal_handling_integration.rs`, `tests/v2_breaking_integration.rs`, `tests/lock_integration.rs`, `tests/property_based.rs`, `tests/loom_lock_slots.rs`, `tests/regression_positional_args.rs`, `tests/recall_integration.rs`, `tests/daemon_integration.rs`, `tests/schema_migration_integration.rs`.

### Notas
- `errors::to_string_pt()` e `main::emit_progress_i18n(en, pt)` mantêm strings PT legítimas — são o branch i18n acionado quando `--lang pt` (ou locale detectado) está ativo. Não são violações.
- Comportamento default `./graphrag.sqlite` em CWD (`paths.rs:35-41`) confirmado empiricamente contra o corpus de auditoria v1.0.29 (29 de 30 documentos Markdown flowaiper indexados end-to-end; recall p50 ~50ms, hybrid-search p50 ~52ms; uma falha do stress test foi timeout externo de 60s, não defeito da ferramenta).

## [1.0.25] - 2026-04-28

### Adicionado
- Flag `recall --all-namespaces` busca em todos os namespaces numa única consulta (P0-1).
- BERT NER agora emite tipos `organization` (B-ORG), `location` (B-LOC) e `date` (B-DATE)
  alinhados com a migration V008. Releases anteriores mapeavam ORG→`project`,
  LOC→`concept` e descartavam DATE completamente (P0-2 + alinhamento V008).
- Migration de schema V008: CHECK constraint de `entities.type` expandida para incluir
  `organization`, `location`, `date`. Migration aditiva; linhas existentes são preservadas.
- BRAND_NAME_REGEX captura nomes de organizações em CamelCase como "OpenAI", "PostgreSQL",
  "ChatGPT" que o BERT NER frequentemente classifica incorretamente (P0-2).
- Filtro de falsos positivos para verbos monossilábicos em PT-BR ("Lê", "Vê", "Cá", etc.)
  para saídas do BERT com confiança abaixo de 0.85 (P0-2).
- SECTION_MARKER_REGEX filtra fragmentos de texto como "Etapa 3", "Fase 1", "Passo 2",
  "Seção 4", "Capítulo 1" da extração de entidades (P0-4).
- 12 novas ALL_CAPS_STOPWORDS: `API`, `CAPÍTULO`, `CLI`, `ETAPA`, `FASE`, `HTTP`, `HTTPS`,
  `JWT`, `LLM`, `PASSO`, `REST`, `UI`, `URL` (P0-4).
- README documenta subcomandos `graph traverse|stats|entities` com tabela de flags (P1-A).

### Alterado
- `recall.graph_matches[].distance` agora reflete o hop count via proxy
  `1.0 - 1.0 / (hop + 1)`. Releases anteriores usavam placeholder `0.0`. Distância
  cosseno real reservada para v1.0.26 (P1-M).
- Lógica longest-wins de `merge_and_deduplicate` reescrita com chave composta
  `entity_type + name_lc` e containment bidirecional de substring. Resolve duplicação
  "Sonne"/"Sonnet" e truncamento "Open"/"Paper" (P0-3).
- Versão do `Cargo.toml` bumped de `1.0.24` para `1.0.25`.

### Corrigido
- `is_valid_entity_type` agora aceita os novos tipos da V008 `organization`, `location`, `date` (P0-A) — sem esta correção, `remember` rejeitaria qualquer entidade emitida pelo mapeamento IOB alinhado à V008 com exit 1.
- Regex `augment_versioned_model_names` não captura mais marcadores de seção em português como "Etapa 3" ou "Fase 1" (P0-B) — filtro de defesa em profundidade aplicado após augmentation e dentro de `iob_to_entities.flush()`.
- `remember --name` com mais de 80 bytes agora retorna exit 6 (LimitExceeded) em vez de
  exit 1 (Validation). Restaura o contrato de exit codes usado por agentes orquestradores (P1-J).

### Notas
- `recall.graph_matches[].distance` é aproximada; distância cosseno semântica reservada para v1.0.26.
- Caps de entidades (30) e relacionamentos (50) permanecem silenciosos na v1.0.25;
  flags `--limit-entities` / `--limit-relations` planejadas para v1.0.26.

## [1.0.24] - 2026-04-27

### Adicionado
- Inferência em lote do BERT NER via `predict_batch` reduz latência por documento em fluxos multi-doc (Phase 3 perf).
- Retry de SQLITE_BUSY e SQLITE_LOCKED com backoff exponencial em `with_busy_retry`; evita exit 10 espúrio em contenção de WAL (Phase 3).
- Aquecimento `spawn_blocking` para carga do modelo BERT no daemon; previne bloqueio do executor async durante inicialização (Phase 3).
- Migração de schema V007: tabela `memory_urls` com índices; URLs extraídas pelo BERT NER agora são persistidas separadamente em vez de vazar para o grafo de entidades (Phase 2).
- Módulo CRUD `src/storage/urls.rs` com `upsert_urls`, `get_urls_for_memory` e `delete_urls_for_memory` (Phase 2).
- Campo `RememberResponse.urls_persisted: usize` reportando quantas entradas de URL foram inseridas em `memory_urls` (Phase 2).
- Campo `RememberResponse.relationships_truncated: bool` indicando se o payload de relacionamentos foi truncado pelo limite de `max_relationships_per_memory` (Phase 4).
- `namespace_initial` persistido em `schema_meta` no `init`; `purge` resolve namespace contextualmente via `SQLITE_GRAPHRAG_NAMESPACE` (Phase 4 P1-A/P1-C).
- Argumentos posicionais e por flag em `read`, `forget`, `history`, `edit`, `rename`; por exemplo, `sqlite-graphrag read minha-nota` é equivalente a `sqlite-graphrag read --name minha-nota` (Phase 4 P1-B).
- Lista de stopwords expandida com 17 novas entradas: `ACEITE`, `ACK`, `ACL`, `BORDA`, `CHECKLIST`, `COMPLETED`, `CONFIRME`, `DEVEMOS`, `DONE`, `FIXED`, `NEGUE`, `PENDING`, `PLAN`, `PODEMOS`, `RECUSE`, `TOKEN`, `VAMOS` (Phase 2 P0-3).
- Normalização Unicode NFKC em `merge_and_deduplicate` evita entidades quase duplicadas causadas por formas Unicode compostas vs decompostas (Phase 2 P1-E).
- Testes de regressão para `graph` traverse com exit 4 quando o banco está ausente (Phase 1 P0-7).
- Testes de regressão para equivalência de argumento posicional com flag em `read`, `forget`, `history`, `edit`, `rename` (Phase 4 P1-B).

### Modificado
- `ReadResponse.metadata` agora é `serde_json::Value` em vez de `String`; agentes recebem um objeto estruturado diretamente sem segunda chamada a `JSON.parse` (Phase 5 P2-A).
- `LinkResponse` simplificado: campos redundantes `source` e `target` removidos; `LinkArgs` não aceita mais os aliases de flag `--source`/`--target` (Phase 4 P1-O).
- `purge` não assume mais namespace `"global"` como padrão; resolve via `SQLITE_GRAPHRAG_NAMESPACE` ou `--namespace` explícito (Phase 4 P1-C).
- O comportamento de `recall --precise` está agora documentado e usa internamente `effective_k = 100000` para KNN exaustivo (Phase 1 P0-6).
- `init --model` agora usa o enum tipado `EmbeddingModelChoice` validado em tempo de parse (Phase 1 P0-8).
- Medição de RAM em `main.rs` usa propagação de `Result` em vez de `expect` (Phase 1 P1-G).
- Carga do modelo no aquecimento do daemon movida para `spawn_blocking` para não bloquear o executor Tokio (Phase 3 P1-I).
- Regex de `augment_versioned_model_names` estendida para reconhecer padrões como `GPT-4o`, `Claude 4 Sonnet`, `Llama 3 Pro`, `Mixtral 8x7B` (Phase 5 P2-D).
- `extend_with_numeric_suffix` agora aceita sufixos alfanuméricos (ex: `v2`, `3b`, `7B`) além dos puramente numéricos (Phase 5 P2-E).
- Serialização de entidades do grafo usa `Vec::new()` em vez de `Option<Vec>`; o campo `entities` é sempre um array, nunca `null` (Phase 5 P2-C).
- Docstrings do argumento `--type` esclarecidas para distinguir `type` de memória de `entity_type` (Phase 5 P2-J).
- Versão do `Cargo.toml` bumped de `1.0.23` para `1.0.24`.

### Corrigido
- `remember` rejeita nomes que normalizam para string vazia após canonicalização kebab-case; retorna exit 1 com mensagem de validação clara (Phase 4 P0-4).
- URLs não vazam mais para o grafo de entidades; todos os tokens com forma de URL do BERT NER agora são roteados para `memory_urls` via V007 (Phase 2 P0-2).
- Serialização de `HybridSearchResponse.weights` confirmada correta; o campo era um flag fantasma sem efeito comportamental (Phase 4 P1-N).

### Segurança
- Comentários `// SAFETY:` adicionados a todos os blocos `unsafe { std::env::set_var(...) }` em `main.rs` (Phase 1 P1-H).
- `deny.toml`: `unmaintained` definido como `"workspace"` para restringir verificações de crates não mantidas apenas aos membros do workspace; reduz falsos positivos de CI em crates transitivas (Phase 5 P2-K).
- Valor inválido em `SQLITE_GRAPHRAG_LANG` agora emite log `tracing::warn!` em vez de retornar silenciosamente ao inglês (Phase 1 P1-M).

### Interno
- 412+ testes passando em todas as fases.
- Release bundle: Fases 1, 2, 3, 4 e 5 em um único commit.

## [1.0.23] - 2026-04-27

### Corrigido
- Mesclagem de subword do BERT NER agora prefere o candidato mais longo quando múltiplas fontes extraem nomes sobrepostos. Antes "OpenAI" extraído por regex podia perder para "Open" vazado de subword BERT porque ambos deduplicavam para a chave lowercase `open`. A nova lógica em `merge_and_deduplicate` retém estritamente a entrada mais longa, favorecendo a marca mais específica visível no corpus (P1 fix em `src/extraction.rs`).
- Nomes de modelos versionados com separador de espaço ("Claude 4", "Llama 3", "Python 3") agora são extraídos como entidades `concept` pelo novo passe `augment_versioned_model_names`. O BERT NER frequentemente classifica esses tokens como substantivos comuns e os pula, então o sufixo de versão sumia. Variantes com hífen como "GPT-5" continuam tratadas pelo pipeline NER+sufixo existente (P1 fix em `src/extraction.rs`).
- `recall` agora expõe `graph_depth: Option<u32>` em cada `RecallItem`. Matches diretos por vetor recebem `None` (use `distance`); resultados de traversal recebem `Some(0)` como sentinela para "alcançável via grafo, profundidade ainda não rastreada com precisão". O placeholder legado `distance: 0.0` permanece por compatibilidade mas deve ser tratado como depreciado para linhas de grafo (P1 fix em `src/commands/recall.rs` e `src/output.rs`).
- `remember` agora reporta `chunks_persisted: usize` ao lado de `chunks_created: usize` para que clientes saibam exatamente quantas linhas foram inseridas em `memory_chunks`. Bodies de chunk único reportam `chunks_persisted: 0` (a própria linha de memória atua como chunk) enquanto multi-chunk reportam `chunks_persisted == chunks_created`. Resolve o achado da auditoria v1.0.22 onde corpos curtos mostravam `chunks_created: 1` com zero linhas persistidas (P1 fix em `src/output.rs` e `src/commands/remember.rs`).

### Adicionado
- `recall --max-graph-results <N>` limita `graph_matches` a no máximo N entradas. Padrão é unbounded para preservar a forma vista em v1.0.22, mas permite capar vizinhanças densas de grafo explicitamente. A docstring de `-k` agora declara claramente que ela controla apenas `direct_matches` (P1 fix de UX em `src/commands/recall.rs`).
- README EN agora lista os aliases `pt-BR` e `portuguese` para `SQLITE_GRAPHRAG_LANG`. Antes apenas o README PT-BR os mencionava, deixando leitores ingleses sem ciência (P1 fix de sincronia de docs).
- README EN+PT agora documentam os cinco targets de binários pré-compilados explicitamente e destacam que Mac Intel (`x86_64-apple-darwin`) requer build local porque o GitHub aposentou o runner macos-13 em dezembro de 2025 e a Apple descontinuou suporte ao x86_64. Migração recomendada é para Apple Silicon (P1 fix de clareza de distribuição).
- `docs/COOKBOOK.md` e `docs/COOKBOOK.pt-BR.md` taglines agora declaram a contagem correta de 23 receitas (alegavam incorretamente 15 desde as adições da v1.0.22). Contado por `rg -c '^## How To'` em ambos arquivos (P1 fix de precisão de docs).

### Modificado
- `Cargo.toml` versão bumpada de `1.0.22` para `1.0.23`.
- JSON do `RememberResponse` ganha o campo `chunks_persisted` (sempre presente); JSON do `RecallItem` ganha `graph_depth` (omitido quando `None` via `skip_serializing_if`). Ambas adições são forward-compatible para qualquer cliente que use parsers JSON tolerantes.

## [1.0.22] - 2026-04-27

### Corrigido
- Workflow `forget` + `restore` não fica mais sem saída. `history --name <X>` agora retorna versões de memórias soft-deleted (antes filtrava `deleted_at IS NULL`); resposta inclui novo campo booleano `deleted`. `restore --version` agora é opcional: quando omitido, a última versão não-`restore` é usada automaticamente. Juntos, esses fixes fazem o round-trip `forget` → `restore` funcionar sem exigir leitura de SQL (correção P0 em `src/commands/history.rs` e `src/commands/restore.rs`).
- `list`, `forget`, `edit`, `read`, `rename`, `history`, `hybrid-search` agora verificam ausência de `graphrag.sqlite` antecipadamente e retornam `AppError::NotFound` (exit 4) com a mensagem amigável "Execute 'sqlite-graphrag init' primeiro", alinhando com `stats`/`recall`/`health`. Antes, `list` vazava o erro bruto do rusqlite e retornava exit 10 (correção de inconsistência P1).
- `remember` agora rejeita `body` vazio ou só com whitespace (sem grafo externo) via `AppError::Validation` (exit 1). Evita persistir memórias com embeddings vazios que quebravam a semântica de recall (correção P1 em `src/commands/remember.rs`).
- Pós-processamento BERT NER estendido para filtrar stopwords adicionais ALL CAPS PT-BR/EN observadas no stress de 495 documentos FlowAiper (verbos, adjetivos, substantivos comuns) e nomes de métodos HTTP (`GET`, `POST`, `DELETE`, etc.). Saídas NER de token único agora também são filtradas, não apenas matches do prefilter regex (correção P1 em `src/extraction.rs`).
- Prefilter de URL do BERT NER agora remove pontuação markdown final (backticks, parênteses, colchetes, pontos, ponto-e-vírgulas) antes de persistir URLs como entidades. Antes, `https://example.com/`` era armazenado literalmente (correção P1 em `src/extraction.rs`).
- Entidades BERT NER com sufixos numéricos hifenizados ou separados por espaço (ex: `GPT-5`, `Claude 4`, `Python 3.10`) agora são estendidas no pós-processamento em vez de truncadas. Lookup de sufixo é conservador: só estende quando ≤6 caracteres e puramente numéricos (correção P1 em `src/extraction.rs::extend_with_numeric_suffix`).
- Enumeração `entity_type` em README EN e pt-BR corrigida de "9 valores" para "10 valores" com `issue_tracker` listado (correção P1 docs).

### Adicionado
- Variável de ambiente `SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY` para configurar o cap de relacionamentos-por-memória (padrão 50, intervalo [1, 10000]). A auditoria identificou que documentos com grafos ricos atingem o cap silenciosamente; usuários com corpora técnico agora podem ajustar (correção P1 via `src/constants.rs::max_relationships_per_memory()`).
- Campo `HistoryResponse.deleted: bool` expondo se a memória está atualmente soft-deletada, permitindo aos clientes detectar estado esquecido sem inspecionar `memory_versions` diretamente.
- 18 flags de CLI antes não documentadas agora possuem docstrings `///` visíveis em `--help`: `init --model`, `init --force`, `remember --name/--description/--body/--body-stdin/--metadata/--session-id`, `read --name`, `forget --name`, `edit --name/--body/--body-file/--body-stdin/--description`, `history --name`, `daemon --idle-shutdown-secs/--ping/--stop` (correção UX P1).

### Modificado
- `Cargo.toml` versão bumped de `1.0.21` para `1.0.22`.
- Const `MAX_RELS=50` em `src/extraction.rs` consolidada em `crate::constants::max_relationships_per_memory()` removendo a definição duplicada.
- Tipo do arg `restore --version` mudou de `i64` para `Option<i64>` (compatível com versão anterior: passar versão explícita continua funcionando).

## [1.0.21] - 2026-04-26

### Corrigido
- BERT NER `iob_to_entities` não vaza mais fragmentos WordPiece como `##AI` ou `##hropic` como entidades separadas. Quando BERT emite label `B-*` em um token iniciado por `##` (estado confuso do modelo), o subword é anexado à entidade ativa se houver, ou descartado caso contrário (correção P0 em `src/extraction.rs:381-394`). Validação empírica: auditoria de 138 documentos FlowAiper produziu ZERO fragmentos `##` na tabela de entidades.
- `recall` rejeita queries vazias com `AppError::Validation` e mensagem clara em vez de vazar erro bruto do rusqlite `Invalid column type Null at index: 1, name: distance` (correção P1 em `src/commands/recall.rs`).
- `restore` agora re-embeda o corpo da memória restaurada e faz upsert em `vec_memories` para que recall vetorial funcione em memórias restauradas. v1.0.20 deixava `vec_memories` desatualizado após `forget` + `restore` (correção P1 em `src/commands/restore.rs`).
- `stats` reporta `chunks_total` com precisão consultando `memory_chunks` e tratando apenas erros "no such table" como estado legado do DB; outros erros do SQLite agora são logados via `tracing::warn!` para visibilidade (correção P1 em `src/commands/stats.rs`).
- Seis panics em caminhos de produção convertidos para `unreachable!()` idiomático dentro de blocos `#[cfg(test)]` (correção P1 em `graph_export.rs`, `memory_guard.rs`, `optimize.rs`, `tz.rs`, `namespace_detect.rs`).
- Tabelas de exit codes do README EN e pt-BR agora listam `73` (guarda de memória rejeitou condição de pouca RAM), alinhando com `llms.txt` e semântica do source (correção P1 docs).

### Adicionado
- Campo `RememberResponse.extraction_method: Option<String>` expondo se a extração automática usou `bert+regex` ou caiu em `regex-only`. Campo é omitido do JSON quando `--skip-extraction` está ativo (telemetria P1 em `src/output.rs` e `src/commands/remember.rs`).
- Campo `ExtractionResult.extraction_method` populado por `extract_graph_auto` e `RegexExtractor`, expondo o caminho real de extração (correção P1 em `src/extraction.rs`).
- 2 testes novos cobrindo o fix do merge IOB: `iob_strip_subword_b_prefix` e `iob_subword_orphan_descarta`.

### Modificado
- `Cargo.toml` versão atualizada de `1.0.20` para `1.0.21`.

## [1.0.20] - 2026-04-26

### Corrigido
- Carregamento do modelo BERT NER agora baixa `tokenizer.json` do subdiretório `onnx/` do repositório `Davlan/bert-base-multilingual-cased-ner-hrl` no HuggingFace, onde o arquivo está de fato publicado. A v1.0.19 tentava baixar da raiz do repositório e recebia 404 em toda ingestão, caindo silenciosamente em graceful degradation só com regex (correção P0 primária em `src/extraction.rs::ensure_model_files`).
- Pesos da classifier head do BERT NER agora são carregados do arquivo safetensors via `VarBuilder::pp("classifier").get(...)` tanto para `weight` quanto para `bias`. A v1.0.19 inicializava com `Tensor::zeros`, o que produziria argmax constante em todos os tokens e tornaria toda predição degenerada mesmo após o fix do tokenizer. Este segundo P0 estava mascarado pelo primeiro e foi descoberto durante o planejamento emergencial (correção P0 secundária em `src/extraction.rs::BertNerModel::load`).
- Prefilter regex de identificadores ALL_CAPS agora filtra palavras-regra do português (`NUNCA`, `SEMPRE`, `PROIBIDO`, `OBRIGATÓRIO`, `DEVE`, `JAMAIS`, etc.) e equivalentes em inglês (`NEVER`, `ALWAYS`, `MUST`, `TODO`, `FIXME`, etc.), preservando identificadores com underscore como `MAX_RETRY` e acrônimos como `OPENAI`. Na v1.0.19 contra corpus técnico em PT-BR, 70% das top entidades eram ruído de palavras-regra (correção P1).
- Tipo de entidade para email mudou de `person` para `concept` porque regex sozinho não distingue indivíduos de endereços de role ou lista (correção P2).
- `merge_and_deduplicate` agora emite `tracing::warn!` quando a contagem de entidades é truncada em `MAX_ENTS=30`, expondo o cap antes silencioso (correção P2).
- `build_relationships` agora emite `tracing::warn!` quando o cap de relacionamentos `MAX_RELS=50` é atingido, complementando o aviso de entidades (correção P2).
- `remember` agora trata bodies só com whitespace (`\n\t  `) como vazios para skip de auto-extração, já que `.is_empty()` sozinho deixava whitespace puro passar (correção P3 em `src/commands/remember.rs`).
- Normalização kebab-case de `remember` e `rename` agora aplica `trim_matches('-')` para remover hífens em bordas, corrigindo rejeição de inputs como `my-name-` truncados por limites de comprimento de filename (correção P3 em `src/commands/remember.rs` e `src/commands/rename.rs`).

### Adicionado
- 4 testes unitários novos em `src/extraction.rs` cobrindo o stopword filter (`regex_all_caps_filtra_palavra_regra_pt`), aceitação de identificador com underscore (`regex_all_caps_aceita_constante_com_underscore`), aceitação de acrônimo de domínio (`regex_all_caps_aceita_acronimo_dominio`) e a reclassificação email→concept (`regex_email_captura_endereco`).

### Modificado
- `Cargo.toml` versão bumped de `1.0.19` para `1.0.20`.

## [1.0.19] - 2026-04-26

### Adicionado
- Chunking hierárquico-recursivo de markdown via `text-splitter = "0.30.1"` (`src/chunking.rs::split_into_chunks_hierarchical`) preserva fronteiras H1/H2 e separadores suaves de parágrafo para documentos que começam com marcadores markdown.
- Extração híbrida automática de entidades (`src/extraction.rs::extract_graph_auto`) combinando pré-filtro regex (emails, URLs, UUIDs, identificadores ALL_CAPS) com passagem CPU `candle` BERT NER (`Davlan/bert-base-multilingual-cased-ner-hrl`, ~676 MB safetensors, AFL-3.0). NER opera em janela deslizante com `MAX_SEQ_LEN=512` e `STRIDE=256`, limitado a `MAX_ENTS=30`/`MAX_RELS=50`. O modelo é baixado lazy na primeira execução e degrada graciosamente para apenas regex em caso de falha (via `tracing::warn!`).
- `remember` agora invoca `extract_graph_auto` automaticamente quando `--skip-extraction` está ausente, nenhum `--entities-file`/`--relationships-file`/`--graph-stdin` é fornecido e o body é não-vazio, materializando entidades e relacionamentos `mentions` antes da persistência.
- 15 testes unitários em `src/extraction.rs` cobrindo pré-filtro regex (email/URL/UUID/ALL_CAPS), decodificação IOB (mapeamento PER/ORG/LOC, descarte de DATE, ORG-com-sufixo-`sdk` → `tool`), enforcement de `MAX_RELS`, dedup por nome em lowercase e fallback gracioso quando o modelo NER está ausente.
- 6 novos testes de chunking em `src/chunking.rs` validando fronteiras `# H1` e `## H2`, documentos markdown de 60 KB com overlap 50, fallback de texto puro e separadores suaves de parágrafo `\n\n`.

### Mudado
- `Cargo.toml` adiciona `text-splitter = "0.30.1"` (features `markdown`, `tokenizers`) e `candle-core`/`candle-nn`/`candle-transformers = "0.10.2"` (default-features off) além de `huggingface-hub` (`hf-hub` renomeado) para downloads de modelo.
- `Cargo.toml` faz bump de `sqlite-vec` de `0.1.6` para `0.1.9` (correção de DELETE e melhorias em constraints KNN) e remove seis dependências órfãs (`notify`, `slug`, `toml`, `uuid`, `zerocopy`, `tracing-appender`).
- `Cargo.toml` reduz `tokio` de `features = ["full"]` para o conjunto mínimo `["rt-multi-thread", "sync", "time", "io-util", "macros"]`.
- Footprint de threads do daemon reduzido de ~65 para ≤4 threads sustentadas via `RAYON_NUM_THREADS=2`, `ORT_INTRA_OP_NUM_THREADS=1` e `ORT_INTER_OP_NUM_THREADS=1` definidos em `src/main.rs` antes da inicialização de qualquer runtime.
- A flag `--skip-extraction` agora exibe help string documentando que desabilita a extração automática de entidades/relacionamentos; o campo previamente dormente é reutilizado como toggle visível ao usuário.

### Corrigido
- `recall` agora reporta `DB inexistente` de forma consistente com os demais subcomandos via helper compartilhado `erros::banco_nao_encontrado` (P1-A).
- `recall --min-distance` foi renomeado para `--max-distance` mantendo `min-distance` como alias legado para compatibilidade (P2-K).
- `related ''` rejeita strings vazias com erro de validação claro em vez de produzir zero resultados silenciosamente (P2-L).
- 15+ strings voltadas ao usuário em `embedder.rs`, `daemon.rs`, `paths.rs`, `tokenizer.rs` e `commands/remember.rs` agora exibem traduções em português junto aos originais em inglês (P2-I).
- `--name` é auto-normalizado para kebab-case com `tracing::warn!` quando snake_case ou CapsName são detectados (P2-H).
- Flags ocultas `--body-file`, `--entities-file`, `--relationships-file`, `--graph-stdin`, `--metadata-file` agora expõem `#[arg(help = ...)]` para aparecer no `--help` (P2-G).
- `stats.memories`, `list.items` e `health.counts.memories` foram unificados sob a chave `memories_total` em todos os outputs JSON (P3-E).
- `HybridSearchItem.rrf_score: Option<f64>` agora é populado com o score real de reciprocal-rank-fusion em vez de retornar sempre `null` (P3-F).
- Rejeição de `--tz` agora sugere fusos horários IANA válidos na mensagem de erro (P3-A).

## [1.0.18] - 2026-04-26

### Adicionado
- Novo helper `parent_or_err` em `src/paths.rs` e quatro testes unitários protegem contra paths malformados vindos de `--db /` ou de `SQLITE_GRAPHRAG_DB_PATH` vazio.
- Novo `DaemonSpawnGuard` em `src/daemon.rs` remove o arquivo `daemon-spawn.lock` em encerramento gracioso e emite uma linha estruturada `tracing::info!` ao encerrar o daemon.
- Variável de ambiente `ORT_DISABLE_CPU_MEM_ARENA=1` agora é setada por padrão em `main.rs` antes do fastembed inicializar, complementando a mitigação existente de `with_arena_allocator(false)` contra crescimento descontrolado de RSS em payloads de shapes variáveis.
- README e `README.pt-BR.md` agora expõem quatro variáveis de ambiente `SQLITE_GRAPHRAG_*` adicionais na tabela de configuração em runtime: `DISPLAY_TZ`, `DAEMON_FORCE_AUTOSTART`, `DAEMON_DISABLE_AUTOSTART`, `DAEMON_CHILD`.
- README e `README.pt-BR.md` agora apresentam o cluster de quatro badges exigido pelas regras do projeto: crates.io, docs.rs, license, Contributor Covenant.

### Alterado
- `path.parent().unwrap()` removido de `src/paths.rs`, `src/daemon.rs::try_acquire_spawn_lock` e `src/daemon.rs::save_spawn_state`; os três call sites agora propagam erros de validação via `parent_or_err`.
- Tagline do README reescrita de um parágrafo de 36 palavras para um blockquote de 12 palavras em conformidade com a regra de documentação sobre tamanho de tagline; o parágrafo duplicado acima do blockquote foi removido.
- Snippets de instalação do README não fazem mais hard-code de `--version 1.0.17` em oito locais entre `README.md` e `README.pt-BR.md`; agora recomendam `cargo install sqlite-graphrag --locked` e linkam para `CHANGELOG.md` para o histórico de versões.

### Corrigido
- O CI agora fixa `cargo-nextest` em `0.9.114`, a release mais nova compatível com o MSRV Rust 1.88.
- Os testes Loom agora usam o gate local `sqlite_graphrag_loom` para evitar compilar dependências Tokio sob o `cfg(loom)` upstream.
- O JSON de relacionamentos de grafo agora aceita aliases `from`/`to` e relações com hífen, normalizando antes da gravação.
- Clippy no macOS e testes de concorrência no Windows agora tratam errno e contenção de lock de arquivo específicos da plataforma corretamente.
- A documentação de grafo e `related` agora reflete a superfície real da CLI e não afirma mais extração automática de entidades em ingestão body-only.

## [1.0.17] - 2026-04-26

### Alterado
- `remember` agora aceita payloads de body até `512000` bytes e até `512` chunks, com embeddings multi-chunk seriais para manter a memória limitada em corpora reais de documentação
- `remember --graph-stdin` agora aceita um objeto estrito de grafo com `body` opcional, `entities` e `relationships`, permitindo que um único payload stdin grave texto e grafo explícito

### Corrigido
- A migração de schema `V006__memory_body_limit` eleva o `CHECK` SQLite de `memories.body` para bancos existentes, mantendo o limite Rust e a constraint do banco alinhados
- `scripts/audit-remember-safely.sh` agora envolve cleanup do daemon, init, health e chamadas auditadas de `remember` com `/usr/bin/timeout -k 30 "${AUDIT_TIMEOUT_SECS:-1800}"`
- A documentação de testes agora recomenda comandos longos com timeout para reduzir risco de travamentos locais em runs slow, loom, heavy e audit

## [1.0.16] - 2026-04-26

### Corrigido
- `remember` agora cria e migra o banco padrão `./graphrag.sqlite` antes da escrita, evitando arquivos SQLite vazios e falhas `no such table` em diretórios novos
- `remember --graph-stdin --skip-extraction` agora persiste payloads explícitos de grafo em vez de descartar entidades e relacionamentos silenciosamente
- Falhas em payloads de grafo agora são validadas antes da escrita e memória, chunks, entidades e relacionamentos são persistidos de forma atômica, então input inválido não deixa memórias parciais
- O parser de input de grafo agora rejeita campos desconhecidos e valida `entity_type`, `relation` e `strength` antes de tocar no SQLite
- Docs para agentes, arquivos de contexto para LLMs, schemas e saída de `--help` agora refletem o contrato estrito de JSON via stdin/stdout
- `scripts/test-loom.sh` agora envolve execuções longas de loom com timeout configurável

## [1.0.15] - 2026-04-26

### Corrigido
- `remember --graph-stdin` agora rejeita JSON inválido em vez de persistir payloads malformados como corpos de memória
- `remember` e `edit` agora rejeitam fontes ambíguas de corpo, como `--body` explícito junto com `--body-stdin`
- O CRUD de grafo via `--graph-stdin` agora preserva valores declarados de `entity_type` quando relacionamentos referenciam entidades existentes no input
- `graph --json` agora domina formatos textuais como `--format dot`, `--format mermaid` e saída textual de stats
- `daemon` agora aceita as flags compartilhadas `--db` e `--json`, mantendo a mesma superfície determinística de flags para invocações por agentes

## [1.0.14] - 2026-04-25

### Corrigido
- A matriz oficial de release agora exclui `x86_64-apple-darwin` e `x86_64-unknown-linux-musl`, que a cadeia atual de dependências com `ort` não sustenta por binários ONNX Runtime pré-compilados nesta configuração do projeto
- O workflow de release não tenta mais montar um binário universal macOS a partir de um artefato Intel não suportado
- As docs de release e de compatibilidade agora descrevem apenas os targets que o projeto consegue publicar com consistência sem build custom do ONNX Runtime

## [1.0.13] - 2026-04-25

### Corrigido
- `x86_64-apple-darwin` agora compila em runner Intel macOS explícito, em vez de falhar num host Apple Silicon sem caminho compatível para os binários ORT pré-compilados desse target
- `x86_64-unknown-linux-musl` agora compila via `cross`, fornecendo o toolchain C++ musl exigido por `esaxx-rs`
- O contrato de runtime do ONNX dinâmico em ARM64 GNU e o requisito de runner Windows ARM64 agora ficam preservados na release candidata que validará a matriz completa

## [1.0.12] - 2026-04-25

### Corrigido
- `aarch64-unknown-linux-gnu` agora compila via estratégia target-specific de ONNX Runtime com `load-dynamic`, em vez de falhar na linkedição dos arquivos ORT pré-compilados
- O contrato de runtime de `libonnxruntime.so` no ARM64 GNU agora está documentado explicitamente nas docs de release e nas docs voltadas a agentes
- O workflow de release agora usa o runner oficial GitHub-hosted Windows ARM64 para `aarch64-pc-windows-msvc`, em vez de um runner x64 incompatível

## [1.0.11] - 2026-04-25

### Corrigido
- A cobertura de smoke da binária instalada agora inclui o contrato público de fallback para `./graphrag.sqlite` no diretório da invocação, fechando um ponto cego de auditoria de release
- Os testes de contrato agora exigem os wrappers atuais de `list` (`items`) e `related` (`results`) em vez de aceitar silenciosamente arrays root legados
- `graph traverse` e `graph stats` agora expõem apenas os formatos que realmente suportam, evitando help enganoso e invocações documentadas inválidas
- O texto de help de subcomandos menos centrais agora está consistentemente inglês-first em toda a superfície pública auditada da CLI
- `COOKBOOK`, `AGENTS`, `INTEGRATIONS`, a orientação de schemas e os exemplos de grafo/health agora estão alinhados aos payloads reais e às formas válidas de comando da binária

## [1.0.10] - 2026-04-24

### Alterado
- O `--help` da CLI agora é consistentemente inglês por padrão no output estático do clap, enquanto `--lang` continua controlando apenas mensagens humanas de runtime
- A documentação de release agora deixa explícitos o upgrade com `cargo install ... --force` e a verificação da versão ativa com `sqlite-graphrag --version`
- A documentação de testes agora separa a cobertura padrão do nextest das suítes críticas de contrato em `slow-tests`

### Adicionado
- Novo job de CI `slow-contracts` executa `doc_contract_integration` e `prd_compliance` com `--features slow-tests`
- `installed_binary_smoke` agora exige por padrão paridade de versão entre a binária instalada e o workspace atual, com escape hatch explícito para auditorias legadas deliberadas

## [1.0.9] - 2026-04-24

### Corrigido
- `--skip-memory-guard` agora desabilita auto-start do daemon por padrão, evitando que subprocessos de teste e auditoria deixem daemons residentes sem opt-in explícito
- O daemon agora se encerra quando seu diretório de controle desaparece, evitando que execuções baseadas em `TempDir` deixem processos órfãos
- `installed_binary_smoke` agora desabilita explicitamente o auto-start do daemon para a binária instalada
- `audit-remember-safely.sh` agora isola `SQLITE_GRAPHRAG_CACHE_DIR` e executa `daemon --stop` no encerramento, evitando vazamento de processos após auditorias

### Adicionado
- Novo teste de regressão do daemon provando que `--skip-memory-guard` não auto-sobe o daemon sem opt-in explícito
- Novo teste de regressão do daemon provando que o processo se encerra quando o diretório temporário de cache/controle desaparece

## [1.0.8] - 2026-04-24

### Adicionado
- Auto-start automático do daemon no primeiro comando pesado de embedding quando o socket do daemon está indisponível
- Serialização de spawn via lock file dedicado do daemon para evitar tempestade de processos
- Estado persistido de backoff de spawn do daemon para suprimir tentativas repetidas após falhas
- Novos testes do daemon cobrindo auto-start e restart automático após shutdown

### Alterado
- Comandos pesados agora tentam usar o daemon, sobem o processo sob demanda e fazem fallback local apenas quando backoff ou falha de spawn exigem isso
- `sqlite-graphrag daemon` continua disponível para gestão explícita em foreground, mas deixou de ser obrigatório no caminho comum

### Corrigido
- O maior gap remanescente do daemon na `v1.0.7` foi fechado: o daemon deixou de ser puramente opt-in

## [1.0.7] - 2026-04-24

### Corrigido
- A documentação de integrações não afirma mais que o projeto roda "sem daemons" agora que `sqlite-graphrag daemon` existe
- A documentação voltada a agentes agora descreve o reuso do daemon persistente nos comandos pesados em vez de um modelo puramente stateless
- HOW_TO_USE agora documenta `sqlite-graphrag daemon`, `--ping`, `--stop` e o caminho de fallback automático nos comandos pesados
- TESTING agora documenta a suíte de integração do daemon e o fluxo básico de recuperação do daemon

## [1.0.6] - 2026-04-24

### Adicionado
- Novo subcomando `daemon` para manter o modelo de embeddings carregado em um processo IPC persistente
- Novo protocolo JSON por socket local para `ping`, `shutdown`, `embed_passage`, `embed_query` e embeddings controlados de múltiplas passagens
- Nova suíte de testes de integração do daemon provando que `init`, `remember`, `recall` e `hybrid-search` incrementam o contador de embeddings do daemon quando ele está disponível
- Novo helper `scripts/audit-remember-safely.sh` para auditar binárias instaladas ou locais sob limites de memória via cgroup

### Alterado
- `init`, `remember`, `recall` e `hybrid-search` agora tentam usar o daemon persistente primeiro e fazem fallback para o caminho local atual quando o daemon não está disponível
- `remember` agora usa o tokenizer real de `multilingual-e5-small` antes do embedding, substituindo a aproximação anterior por caracteres no caminho quente
- O embedding multi-chunk em `remember` agora usa micro-batching controlado por orçamento de tokens preenchidos em vez de serialização cega de todos os chunks
- O help de `remember --type` agora deixa explícito que o campo se refere a `memories.type`, não ao `entity_type` do grafo

### Corrigido
- O script de auditoria segura do `remember` agora usa diretório temporário único por execução e valida o banco com `health` após `init`
- Entradas sintéticas densas em bytes, mas abaixo do guard de tamanho, deixaram de fragmentar artificialmente em falhas de 7 chunks na build local melhorada

## [1.0.5] - 2026-04-24

### Corrigido
- `chunking::Chunk` deixou de armazenar corpos owned dos chunks, então o `remember` multi-chunk evita duplicar o corpo inteiro em memória dentro de cada chunk
- A persistência dos chunks agora insere slices de texto diretamente a partir do body armazenado, em vez de alocar outra coleção intermediária owned
- A documentação pública agora descreve corretamente `1.0.4` como release publicada atual e `1.0.5` como próxima linha local
- `remember` agora emite instrumentação de memória por etapa e rejeita documentos que excedem o limite operacional explícito atual de multi-chunk antes de iniciar trabalho ONNX
- O limite operacional explícito de multi-chunk foi reduzido de 8 para 6 após a auditoria segura em cgroup mostrar OOM ainda presente em entradas moderadas com 7 chunks sob `MemoryMax=4G`
- `remember` agora também rejeita corpos multi-chunk densos acima de `4500` bytes antes de iniciar trabalho ONNX, com base na janela de OOM observada na auditoria segura em cgroup
- O embedder agora força `max_length = 512` explicitamente e desabilita a CPU memory arena do ONNX Runtime para reduzir retenção de memória entre inferências repetidas com shapes variáveis

### Causa Raiz
- O desenho anterior ainda duplicava o body por meio de `Vec<Chunk>` carregando `String` owned para cada chunk
- Essa duplicação ampliava a pressão do alocador exatamente no caminho multi-chunk já tensionado pela inferência ONNX
- A ausência de um guard operacional explícito também permitia que entradas Markdown moderadas alcançassem o caminho pesado de embedding multi-chunk sem parada de segurança antecipada
- A auditoria segura subsequente mostrou que até alguns documentos com 7 chunks permaneciam inseguros dentro de um cgroup de `4G`, justificando um teto temporário mais estrito
- A auditoria segura subsequente também mostrou que alguns documentos densos entre `4540` e `4792` bytes ainda disparavam OOM abaixo do teto por chunks, justificando um guard temporário adicional por tamanho do body
- A documentação oficial do ONNX Runtime confirma que `enable_cpu_mem_arena = true` é o padrão, que desligá-lo reduz consumo de memória e que o custo pode ser maior latência
- A API da crate `ort` também documenta que `memory_pattern` deve ser desabilitado quando o tamanho da entrada varia, o que combina com o caminho de `remember` sob inferência repetida e shapes efetivos variáveis
- A inspeção do `fastembed 5.13.2` mostrou que o caminho CPU não desabilita a CPU memory arena do ONNX Runtime por padrão e só desabilita `memory_pattern` automaticamente no caminho DirectML
- A inspeção dos metadados do tokenizer de `multilingual-e5-small` confirmou que o teto real do modelo é `512`, então forçar `max_length = 512` alinha o projeto ao modelo em vez de depender de um default genérico da biblioteca
- A retenção da arena CPU passa, portanto, a ser tratada como causa fortemente sustentada e tecnicamente coerente, mas ainda não como causa única completamente provada em todos os casos patológicos

## [1.0.4] - 2026-04-23

### Corrigido
- `remember` agora gera embeddings de corpos chunkados em modo serial e reutiliza os mesmos embeddings por chunk para agregação e persistência em `vec_chunks`, evitando o caminho de batch que travava com documentos Markdown reais
- `remember` agora evita uma `Vec<String>` extra com cópias dos textos dos chunks e também evita reconstruir uma `Vec<storage::chunks::Chunk>` intermediária antes da persistência dos chunks
- `remember` agora resolve checagens baratas de duplicação antes de qualquer trabalho de embedding e não clona mais o corpo completo desnecessariamente para `NewMemory`
- `namespace-detect` agora aceita `--db` como no-op para que o contrato público do comando fique alinhado com o restante da superfície da CLI
- A documentação pública e o texto do workflow de release agora refletem corretamente a linha publicada `1.0.3` e o contrato de grafo explícito
- O chunking agora usa uma heurística mais conservadora de chars por token e garante progresso seguro em UTF-8, reduzindo o risco de chunks patológicos em entradas Markdown reais

### Causa Raiz
- Markdown real com estrutura rica em parágrafos podia levar a progressão não monotônica dos chunks com a lógica antiga de overlap
- O caminho antigo de `remember` também duplicava pressão de memória ao clonar textos de chunks para uma `Vec<String>` dedicada e ao reconstruir structs de chunk com novas `String` owned antes da persistência
- O caminho antigo de `remember` também gastava trabalho de ONNX antes de resolver condições baratas de duplicação e ainda clonava o corpo completo para `NewMemory` antes do insert ou update
- A combinação aumentava a pressão do alocador e tornava o caminho pesado de embedding mais vulnerável a crescimento patológico de memória em entradas problemáticas

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


## [Legacy NeuroGraphRAG]
<!-- Bloco anterior ao rename para sqlite-graphrag, preservado para rastreabilidade -->

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
