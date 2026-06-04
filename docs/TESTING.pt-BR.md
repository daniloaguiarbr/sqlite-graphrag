# Guia de Testes


- Leia a versão em inglês em [TESTING.md](TESTING.md)


## Por Que Categorizar os Testes
### O Incidente de Livelock Térmico — 2026-04-19
- Em 2026-04-19 às 11:37:40, o Intel i9-14900KF do desenvolvedor atingiu Tjmax 100°C
- A temperatura do VRM chegou a 99°C e o sistema exigiu reset forçado após 3 minutos e 11 segundos
- Causa raiz: `tests/loom_lock_slots.rs` executava sem gate `#[cfg(sqlite_graphrag_loom)]`
- O agendador do loom é intensivo por design — ele explora todas as permutações de threads
- Executar modelos loom sem isolamento causa runaway térmico em CPUs de alto núcleo
- Foi o terceiro incidente em sete dias causado pelo mesmo arquivo de testes sem proteção
- TODOS os testes loom DEVEM ter gate `#[cfg(sqlite_graphrag_loom)]` e ser serializados com `#[serial(loom_model)]`
- NUNCA execute testes loom dentro da invocação padrão `cargo nextest run`


## Categorias de Testes
### Testes Unitários — Inline com o Código-Fonte
- Localização: blocos `#[cfg(test)] mod tests` dentro de cada módulo em `src/`
- Executar com: `/usr/bin/timeout 300 cargo nextest run --profile default`
- Escopo: funções puras, variantes de erro, mascaramento, parsing, validação
- Isolamento: sem I/O, sem filesystem, sem chamadas HTTP
- Gate: sempre compilado, sempre executado no profile default
### Testes de Integração — Arquivos Separados
- Localização: diretório `tests/`
- Executar com: `/usr/bin/timeout 300 cargo nextest run --profile default`
- Escopo: subcomandos CLI, contratos de schema JSON, conformidade PRD, CRUD de storage
- Isolamento: `TempDir` por teste, `env_clear()`, wiremock para HTTP
- Gate: sempre compilado, sempre executado no profile default
### Testes de Concorrência Loom — Opt-in Explícito
- Localização: `tests/loom_lock_slots.rs`
- Executar com: `/usr/bin/timeout 3900 bash scripts/test-loom.sh` ou o job CI `loom`
- Escopo: teste de permutação do semáforo de lock slots
- Isolamento: NUNCA executar em paralelo com outros testes — um modelo por vez
- Gate: `#[cfg(sqlite_graphrag_loom)]` obrigatório em CADA função de teste e bloco de imports
- Risco térmico: testes loom sem proteção causaram travamento do sistema em 2026-04-19
### Testes End-to-End Lentos e Stress — Opt-in via Feature Flag
- Localização: arquivos em `tests/` protegidos por `#[cfg(feature = "slow-tests")]`
- Executar com: `/usr/bin/timeout 1800 cargo nextest run --profile heavy --features slow-tests`
- Escopo: suítes end-to-end longas, contratos, reuse de daemon, paridade i18n, roteamento de exit code, alta concorrência e loops de retry estendidos
- Gate: excluído dos profiles nextest `default` e `ci`
- Suítes críticas de release: `/usr/bin/timeout 1200 cargo test --features slow-tests --test doc_contract_integration -- --nocapture`
- Suítes críticas de release: `/usr/bin/timeout 1200 cargo test --features slow-tests --test prd_compliance -- --nocapture`
- O CI executa essas duas suítes em um job dedicado `slow-contracts` em `ubuntu-latest`
### Benchmarks — Criterion
- Localização: `benches/`
- Executar com: `/usr/bin/timeout 1800 cargo bench` ou `/usr/bin/timeout 1800 cargo criterion`
- Escopo: baselines de latência para remember, recall, hybrid-search, stats, graph
- Gate: nunca incluído em `cargo nextest run`
### Testes de Ingestão Claude Code
- Testes unitários em `src/commands/ingest_claude.rs` cobrem: parsing JSON, fallback de structured_output, tratamento de erros, detecção de rate limit, validação de entity_type, conformidade do schema
- 9 testes unitários protegem invariantes de parsing de extração sem requerer o binário Claude Code
- Testes de integração requerem Claude Code >= 2.1.0 instalado localmente — executar manualmente, não no CI
- Nomes de testes seguem convenções `test_parse_claude_output_*` e `test_extraction_schema_*`
### Testes de Ingestão Codex (v1.0.62)
- 7 testes unitários protegem o parser JSONL do Codex em `src/commands/ingest_codex.rs`
- Testes cobrem: extração válida, erros turn.failed, detecção de rate limit, validação de schema, descoberta de binário
- Parser valida o padrão "último agent_message vence" para múltiplos eventos item.completed
- Testes de integração requerem Codex CLI instalado; pulam graciosamente se indisponível
### Testes de Regressão v1.0.63
- 3 testes de integração em `tests/v1063_features.rs` protegem as correções da v1.0.63
- `restore_preserves_name_after_rename`: remember → edit → rename → restore; asserta que nome permanece renomeado
- `restore_does_not_crash_when_old_name_occupied`: remember A → rename para B → remember novo A → restore B; asserta exit 0 (era exit 10 UNIQUE crash antes da correção)
- `edit_reembeds_when_body_changes`: remember → edit body → recall novo conteúdo; asserta que recall encontra a memória editada com score preciso
### Testes de Regressão v1.0.64
- 14 testes unitários em `src/commands/deep_research.rs` protegem decomposição de query, concorrência bounded, dedup, montagem de cadeias de evidência e edge cases
- Testes unitários em `src/commands/ingest_claude.rs` cobrem parsing de terminal_reason, detecção OAuth via apiKeySource e pré-validação de tamanho do body
- Testes unitários em `src/commands/rename.rs` e `src/commands/rename_entity.rs` cobrem rejeição de mesmo nome com exit 1

### Testes de Regressão v1.0.68
#### Correção do Tipo HANDLE no Windows (G29)
- `tests/terminal_compile_windows.rs` é um novo teste de integração que roda em toda plataforma: confirma que `terminal::init_console` e `should_use_ansi` continuam chamáveis de fora do crate
- No Windows, o teste adicionalmente referencia a checagem type-safe `HANDLE.is_null() + INVALID_HANDLE_VALUE`; se o contrato de tipo regredir, `cargo check --target x86_64-pc-windows-msvc` no job de CI `windows-build-check` falha antes desse teste ser alcançado
- O novo job de CI é a checagem canônica de regressão; o teste de integração é a sonda local de pré-publish
#### Singleton de Jobs (G28-B)
- Três testes unitários em `src/lock.rs::tests`: `job_singleton_path_sanitises_namespace` (verifica slug em kebab-case a partir de input arbitrário), `job_singleton_blocks_second_invocation_same_namespace` (verifica `AppError::JobSingletonLocked` no segundo acquire), `job_singleton_allows_different_namespaces` (verifica isolamento por namespace)
- Rode via `cargo test --lib lock::tests` (sem `#[serial]` porque os IDs únicos por namespace em cada teste isolam-nos de interferência de estado compartilhado)
#### Circuit Breaker (G28-D)
- Três testes unitários em `src/retry.rs::circuit_breaker_tests`: `opens_after_threshold_consecutive_hard_failures`, `ignores_transient_errors`, `success_resets_consecutive_failures`.  Validam a classificação de AttemptOutcome que distingue `AppError::RateLimited` e `AppError::Timeout` (Transient) de `AppError::Validation` e `AppError::Conflict` (HardFailure)
#### Correções de Testes Pré-Existentes de Timezone
- Três falhas de teste pré-existentes foram corrigidas em `src/commands/{history,list,read}.rs`: os testes agora parseiam a string ISO via `chrono::DateTime::parse_from_rfc3339` e comparam `timestamp()` contra `DateTime::UNIX_EPOCH` em vez de afirmar o prefixo hardcoded `1970-01-01T00:00:00`.  Isso torna as asserções timezone-agnostic então a suite fica verde independentemente da env var `SQLITE_GRAPHRAG_DISPLAY_TZ`

### Testes de Novos Comandos v1.0.67
- Testes de `remember-batch` em `src/commands/remember_batch.rs`: testes de serialização para BatchItemEvent e BatchSummary
- Comando `completions`: testado via smoke test `cargo run -- completions bash`
- Integração `read --id`: testado via round-trip `read --id <memory_id> --json`
- Detecção de super-hub no `health`: testado com banco de produção (1059 memórias, 3 super-hubs detectados)
- `edit` skip-embed: testado via comparação body_hash (edição idempotente pula embedding)
- `rename` ghost purge: testado via workflow forget → rename
- Validação de flags: testado via `hybrid-search --max-hops 2` (sem `--with-graph`) esperando exit 1

### Testes dos Novos Comandos v1.0.65
#### Testes de Deep Research
- Testes unitários em `src/commands/deep_research.rs` cobrem divisão de decompose_query, passthrough de query única, semáforo de concorrência bounded, deduplicação de resultados, montagem de cadeias de evidência (filtro depth >= 2) e validação de query vazia
- Teste de contrato `contract_36_deep_research` em `tests/doc_contract_integration.rs`: insere duas memórias, executa `deep-research "auth and deploy" --max-sub-queries 2 --k 5`, verifica chaves obrigatórias (`query`, `sub_queries`, `results`, `evidence_chains`, `stats`) e valida enum `sub_queries[].source`
- Teste de schema `schema_36_deep_research` em `tests/schema_contract_strict.rs`: valida a resposta completa contra `docs/schemas/deep-research.schema.json` (Draft 2020-12, `additionalProperties: false`)
#### Testes de reclassify-relation
- 8 testes unitários em `src/commands/reclassify_relation.rs` cobrem serialização, action dry_run, contagem de merged_duplicates, caso sem matches e guarda de mesmo valor
- Teste de contrato `contract_37_reclassify_relation`: vincula duas entidades via `mentions`, executa `reclassify-relation --from-relation mentions --to-relation related --batch --dry-run`, verifica as 7 chaves obrigatórias e `action == "dry_run"`
- Teste de schema `schema_37_reclassify_relation`: valida contra `docs/schemas/reclassify-relation.schema.json`
#### Testes de normalize-entities
- 5 testes unitários em `src/commands/normalize_entities.rs` cobrem contagem em dry-run, renomeação in-place, merge por colisão, serialização e campo action em dry-run
- Teste de contrato `contract_38_normalize_entities`: insere uma memória, executa `normalize-entities --dry-run`, verifica 5 chaves obrigatórias e `action == "dry_run"`
- Teste de schema `schema_38_normalize_entities`: valida contra `docs/schemas/normalize-entities.schema.json`
#### Testes de enrich
- Teste de contrato `contract_39_enrich`: insere uma memória, executa `enrich --operation memory-bindings --dry-run`, parseia linhas NDJSON, verifica evento de fase validate, evento de fase scan, eventos de item preview (status=`preview`) e linha de summary com todas as chaves obrigatórias
- Teste de schema `schema_39_enrich`: valida cada tipo de linha NDJSON contra o schema correspondente (`enrich-phase.schema.json`, `enrich-item-event.schema.json`, `enrich-summary.schema.json`)
- Todos os testes de enrich usam `--dry-run` para evitar spawnar o binário LLM


## Como Executar
### Default — Desenvolvimento Local
- Executar todos os testes unitários e de integração: `/usr/bin/timeout 300 cargo nextest run --profile default`
- Executar com saída em caso de falha: `/usr/bin/timeout 300 cargo nextest run --profile default --no-capture`
- Executar um teste específico pelo nome: `/usr/bin/timeout 300 cargo nextest run --profile default fragmento_do_nome`
- Executar um arquivo específico: `/usr/bin/timeout 300 cargo nextest run --profile default -E 'test(schema_contract)'`
### CI — Paralelismo Controlado
- Executar todos os testes como o CI faria: `/usr/bin/timeout 600 cargo nextest run --profile ci`
- O profile `ci` define `test-threads = 2` e `RUST_TEST_THREADS=2`
- O profile `ci` habilita retentativas em testes instáveis
- O workflow também executa `doc_contract_integration` e `prd_compliance` separadamente com `--features slow-tests`
### Heavy — Testes de Stress e Lentos
- Executar testes de stress e lentos: `/usr/bin/timeout 1800 cargo nextest run --profile heavy --features slow-tests`
- O profile `heavy` define `test-threads = 1` para isolamento máximo
- NUNCA execute o profile `heavy` em máquina com throttling térmico ativo
- Para validação de release, prefira os comandos explícitos de contrato acima antes de uma rodada heavy mais ampla


## Auditoria Segura do Remember
### Reproduza o Comportamento da Binária Instalada com Limites de cgroup
- Use `/usr/bin/timeout 3900 bash scripts/audit-remember-safely.sh <diretorio-do-corpus>` para auditar o `remember` com segurança contra um corpus real
- O script usa por padrão o `sqlite-graphrag` instalado no `PATH`
- Sobrescreva a binária com `BIN=./target/debug/sqlite-graphrag` para comparar mudanças locais com a build publicada
- O script usa `systemd-run --user --scope -p MemoryMax=4G -p MemorySwapMax=0`
- O script inicializa um banco temporário isolado e um cache de modelo isolado para o daemon
- O script tenta executar `daemon --stop` ao sair para evitar deixar um processo residente com o modelo carregado
- O script executa casos conhecidos de sucesso, limiar, falha e caso sintético


## Testes do Daemon
### Valide Explicitamente O Reuso Do Processo Persistente
- Execute `/usr/bin/timeout 900 cargo test --all-features --test daemon_integration -- --nocapture` para validar o daemon ponta a ponta
- A suíte do daemon prova `ping`, `shutdown`, auto-start, restart após stop, auto-restart por version mismatch (desde v1.0.50) e incrementos de contador em `init`, `remember`, `recall` e `hybrid-search`; v1.0.51 adicionou 6 testes unitários (capping de backoff, half-jitter, nome de socket, CAS de versão, roundtrip de state) elevando o total do daemon para 10
- Use `SQLITE_GRAPHRAG_CACHE_DIR=/tmp/test-cache` para isolar o socket do daemon e o cache do modelo por execução
- Se um teste do daemon travar, execute `sqlite-graphrag daemon --stop` com o mesmo cache dir antes de tentar de novo
- A flag oculta de testes `--skip-memory-guard` agora desabilita auto-start do daemon por padrão, exceto quando `SQLITE_GRAPHRAG_DAEMON_FORCE_AUTOSTART=1` estiver definido


## Testes de Concorrência Loom
### Como o Loom Funciona
- O loom executa cada teste múltiplas vezes permutando os entrelaçamentos de threads
- Usa redução de estados para evitar explosão combinatória
- Cada modelo deve terminar dentro de um limite de preempção definido
- O uso de CPU é extremamente alto — um núcleo satura completamente por modelo
- NUNCA execute testes loom junto com outros testes no mesmo processo
### Executar Testes Loom Localmente
- Use o script canônico: `/usr/bin/timeout 3900 bash scripts/test-loom.sh`
- O script define `RUSTFLAGS="--cfg sqlite_graphrag_loom"` e `RUST_TEST_THREADS=1`
- O script define `LOOM_MAX_PREEMPTIONS=1` para iteração local limitada
- Execute somente no modo release: `--release` é obrigatório para velocidade aceitável
- Monitore a temperatura da CPU antes e durante a execução
### Executar Testes Loom Individualmente
- Compilar primeiro: `/usr/bin/timeout 600 env RUSTFLAGS="--cfg sqlite_graphrag_loom" cargo build --release --tests`
- Executar modelo único: `/usr/bin/timeout 3600 env RUSTFLAGS="--cfg sqlite_graphrag_loom" RUST_TEST_THREADS=1 cargo nextest run --release -E 'test(lock_slot)'`
- Limite menor para iteração local: `LOOM_MAX_PREEMPTIONS=1`
- Aumente os limites manualmente apenas em depurações focadas
### Checkpoint e Retomada
- Defina `LOOM_CHECKPOINT_FILE=/tmp/loom-checkpoint.json` para retomar execuções interrompidas
- O arquivo de checkpoint registra as permutações já exploradas
- Delete o arquivo de checkpoint para iniciar uma exploração nova


## Variáveis de Ambiente
### Variáveis do Loom — Definir Antes de Executar `scripts/test-loom.sh`
- `RUSTFLAGS="--cfg sqlite_graphrag_loom"` — habilita o gate local do projeto para loom, OBRIGATÓRIO para todos os testes loom
- `LOOM_MAX_PREEMPTIONS=1` — limita a profundidade de preempção por modelo (padrão local e CI: 1)
- `LOOM_MAX_BRANCHES=100` — limita o fator de ramificação por execução (padrão local e CI: 100)
- `LOOM_LOG=1` — habilita rastreamento detalhado de execução do loom no stderr
- `LOOM_CHECKPOINT_FILE=/tmp/loom.json` — caminho para arquivo de checkpoint para retomar execuções
- `RUST_TEST_THREADS=1` — OBRIGATÓRIO, proíbe execução paralela de modelos loom
### Variáveis do Cargo e Nextest
- `RUST_TEST_THREADS=N` — controla o paralelismo do nextest em nível de processo
- `CARGO_TERM_COLOR=always` — preserva cores nos logs do CI
- `NEXTEST_PROFILE=ci` — sobrescreve o profile ativo do nextest via ambiente
### Variáveis Específicas do sqlite-graphrag
- `SQLITE_GRAPHRAG_DB_PATH=/tmp/test/graphrag.sqlite` — isola o caminho do banco por teste
- `SQLITE_GRAPHRAG_CACHE_DIR=/tmp/test-cache` — isola cache do modelo e lock files por teste
- `SQLITE_GRAPHRAG_LOG_FORMAT=json` — muda a saída de log para JSON estruturado
- `SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo` — sobrescreve o timezone dos timestamps


## Profiles do CI
### Profile — default
- Ativa: sempre, a menos que seja sobrescrito
- `test-threads`: 2
- `RUST_TEST_THREADS`: não definido, herda o padrão do sistema
- Tentativas: 0
- Slow-timeout: período 60s, termina após 2 períodos (120s kill efetivo)
- Exclui: testes loom, feature slow-tests
### Profile — ci
- Ativa: `/usr/bin/timeout 600 cargo nextest run --profile ci`
- `test-threads`: 2
- `RUST_TEST_THREADS`: 2 (explícito, previne sobrecarga térmica em runners compartilhados)
- Tentativas: 2 para testes instáveis
- Slow-timeout: período 180s, termina após 3 períodos (540s kill efetivo)
- Exclui: testes loom, feature slow-tests
- Job dedicado `slow-contracts` cobre `doc_contract_integration` e `prd_compliance` com `/usr/bin/timeout 1200 cargo test --features slow-tests`
### Profile — heavy
- Ativa: `/usr/bin/timeout 1800 cargo nextest run --profile heavy --features slow-tests`
- `test-threads`: 1
- `RUST_TEST_THREADS`: 1
- Tentativas: 0
- Slow-timeout: período 900s, termina após 2 períodos (1800s kill efetivo)
- Inclui: testes com gate da feature slow-tests
- Exclui: testes loom (sempre separados)
### Job CI Loom — Etapa Separada no Workflow
- Ativa: job chamado `loom` em `ci.yml`
- Ambiente: `RUSTFLAGS="--cfg sqlite_graphrag_loom"`, `RUST_TEST_THREADS=1`, `LOOM_MAX_PREEMPTIONS=1`, `LOOM_MAX_BRANCHES=100`
- Executa: `/usr/bin/timeout 600 cargo test --test loom_lock_slots --release -- --test-threads=1`
- NUNCA deve ser mesclado com as execuções dos profiles default ou ci


## Solução de Problemas
### Throttling Térmico Durante os Testes
- Sintoma: a suíte de testes desacelera progressivamente, CPU reporta temperatura alta
- Causa: testes loom ou de stress rodando sem limites de thread adequados
- Correção: interrompa a execução imediatamente, deixe a CPU esfriar por 5 minutos
- Prevenção: NUNCA execute `cargo test` sem os profiles do nextest configurados
- Prevenção: SEMPRE use `scripts/test-loom.sh` para testes loom
### Travamento do Sistema Durante Testes Loom
- Sintoma: máquina fica sem resposta, exige reset forçado
- Causa: modelos loom executando em paralelo (RUST_TEST_THREADS > 1) em CPU de alto TDP
- Correção: reset forçado, depois defina `RUST_TEST_THREADS=1` antes de qualquer execução loom
- Caso histórico: 2026-04-19 11:37:40 — i9-14900KF travou por 3 minutos e 11 segundos
- Prevenção: atributo `#[serial(loom_model)]` DEVE estar presente em todo teste loom
### Teste Loom Não Termina
- Sintoma: modelo loom não termina após vários minutos
- Causa: `LOOM_MAX_PREEMPTIONS` não definido, exploração sem limite padrão
- Correção: defina `LOOM_MAX_PREEMPTIONS=1` para iteração local limitada
- Trade-off: valores menores perdem entrelaçamentos raros; aumente o limite apenas em depurações focadas
### Testes Instáveis no CI
- Sintoma: teste passa localmente mas falha de forma intermitente no CI
- Causa: ausência de `#[serial]` em testes que compartilham estado global ou variáveis de ambiente
- Correção: adicione `#[serial]` da crate `serial_test` nos testes afetados
- Diagnóstico: execute `/usr/bin/timeout 600 cargo nextest run --profile ci --retries 0` para ver todas as falhas


## Referências
- Documentação da crate loom: `https://docs.rs/loom/latest/loom/`
- Repositório GitHub do loom: `https://github.com/tokio-rs/loom`
- Documentação do cargo-nextest: `https://nexte.st/`
- Referência de configuração do cargo-nextest: `https://nexte.st/docs/configuration/`
- Crate serial_test: `https://docs.rs/serial_test/latest/serial_test/`
