# Plano de Testes


- Leia a versão em inglês em [TEST_PLAN.md](TEST_PLAN.md)
- Guia complementar: [TESTING.pt-BR.md](TESTING.pt-BR.md) documenta a infraestrutura de cada camada
- Criado durante a auditoria pós-publicação da v1.0.79 em 2026-06-11 (gaps G46-G54)


## Objetivos e Escopo
### Por Que Este Plano Existe
- O G43 provou que suítes fora do caminho default do CI escondem quebras por ciclos inteiros de release
- O G50 provou que doctests rodam SOMENTE no CI, e um exemplo rustdoc quebrado foi publicado em 10 releases
- O artefato publicado no crates.io nunca era exercitado diretamente antes deste plano
- Este plano torna cada camada explícita: o que roda, quando, com qual comando e o que significa passar
### Escopo
- Cobre o crate sqlite-graphrag: testes unitários da lib, integração da CLI, contratos, concorrência, benchmarks e auditoria pós-publicação
- Exclui teste exploratório manual e projetos consumidores downstream


## Matriz de Camadas de Teste
### Camada 1 — Testes Unitários (por commit)
- Comando: `/usr/bin/timeout 300 cargo nextest run --profile default`
- Escopo: funções puras, parsing, validação e variantes de erro dentro de `src/`
- Critério: ZERO falhas
- Nota: testes que leem a dim global de embedding DEVEM ser `#[serial_test::serial(env)]` (G50 causa E)
### Camada 2 — Testes de Integração (por commit)
- Comando: a mesma invocação do nextest; arquivos vivem em `tests/`
- Pré-requisito: `export PATH="$PWD/tests/mock-llm:$PATH"` (mocks dim-aware desde o G51)
- Critério: ZERO falhas
### Camada 3 — Doctests (por commit, OBRIGATÓRIA localmente)
- Comando: `/usr/bin/timeout 300 cargo test --doc`
- O nextest NÃO executa doctests; pular esta camada localmente foi como a causa A do G50 ficou quebrada por 10 releases
- Critério: ZERO falhas
### Camada 4 — Suítes Lentas de Contrato (por release)
- Comando: `/usr/bin/timeout 1800 cargo nextest run --profile heavy --features slow-tests`
- Comando: `/usr/bin/timeout 1200 cargo test --features slow-tests --test doc_contract_integration -- --nocapture`
- Comando: `/usr/bin/timeout 1200 cargo test --features slow-tests --test prd_compliance -- --nocapture`
- Critério: ZERO falhas nos ~1881 testes
### Camada 5 — Concorrência Loom (somente opt-in explícito)
- Comando: `/usr/bin/timeout 3900 bash scripts/test-loom.sh`
- RISCO TÉRMICO: nunca rodar fora do script dedicado (incidente de 2026-04-19)
- Critério: todos os modelos gated completam dentro dos limites de preempção
### Camada 6 — Benchmarks (por release, informativa)
- Comando: `/usr/bin/timeout 1800 cargo bench --bench regression_baseline -- --quick`
- Pré-requisito: mock LLM no PATH (G50 causa C)
- Critério: sem regressão acima de 10 por cento contra o baseline armazenado
### Camada 7 — Black-Box Pós-Publicação (por release, OBRIGATÓRIA)
- Alvo: o binário instalado do crates.io (`cargo install sqlite-graphrag`), nunca o `target/`
- Setup: banco temporário via `SQLITE_GRAPHRAG_DB_PATH`, namespace isolado, mocks dim-aware no PATH
- Matriz: bootstrap (init/health/migrate/stats), ciclo de vida CRUD, comandos de busca, comandos de grafo, manutenção (fts/optimize/backup/vec/export), contratos de exit code (1, 2, 3, 4, 9) e contratos JSON contra `docs/schemas/`
- Robustez: abort OAuth-only com `ANTHROPIC_API_KEY` definida, SIGPIPE exit 141 em output grande, `--tz` inválido exit 2, `SQLITE_GRAPHRAG_EMBEDDING_DIM` inválida emite warning (G49)
- Dimensionalidade: banco virgem adota 64; banco 384 pré-existente é adotado (G43) e os lotes encolhem (G44)
- Tarball: baixar o `.crate`, verificar ausência de arquivos proibidos (scripts/legacy, configs de agente) e READMEs corretos
- Critério: cada comando bate o exit code e o schema esperados; esta camada teria pego G46-G49 antes dos usuários
### Camada 8 — Smoke com LLM Real (por release, custo OAuth)
- Comandos: uma criação pequena com grafo curado, um round-trip de `recall`, um `edit --force-reembed`
- Orçamento: 3 chamadas LLM, menos de 5 minutos no total; latência esperada da criação abaixo de 90 segundos (critério G42)
- Registrar o score do top hit para o baseline de qualidade de retrieval (G54)
- Rate limits são registrados como evidência, nunca retentados em loop


## Gates de Release (executar em ordem, parar na primeira falha)
### Os 8 Gates Obrigatórios
- Gate 1: `cargo fmt --all --check`
- Gate 2: `/usr/bin/timeout 600 cargo clippy --all-targets --all-features -- -D warnings`
- Gate 3: camadas 1-4 verdes, INCLUINDO `cargo test --doc`
- Gate 4: `RUSTDOCFLAGS="-D warnings" /usr/bin/timeout 300 cargo doc --no-deps --all-features`
- Gate 5: `/usr/bin/timeout 120 cargo audit`
- Gate 6: `/usr/bin/timeout 180 cargo deny check advisories licenses bans sources`
- Gate 7: `/usr/bin/timeout 120 cargo publish --dry-run --allow-dirty` mais revisão de `cargo package --list`
- Gate 8: workflow CI do GitHub Actions VERDE no commit do release — publicar com CI vermelho é a falha raiz documentada no G50
### Gates Informativos (registrar, decidir, nunca pular em silêncio)
- `cargo +stable semver-checks --baseline-version <anterior>` — exige rustc >= 1.91; 9 quebras major saíram em silêncio na v1.0.79 (G53)
- `cargo llvm-cov --lib --summary-only` — meta de cobertura de 80 por cento para código novo


## Gatilhos
### Por Commit
- Camadas 1-3 mais Gates 1-2
### Por Release (antes do `cargo publish`)
- Camadas 1-6 mais os 8 gates mais os gates informativos
### Pós-Publicação (depois do crates.io aceitar a versão)
- Camadas 7-8 contra o binário instalado do registry
- Registrar gaps novos em `gaps.md` no formato de numeração G para qualquer achado


## Riscos e Restrições
- Loom fora do script pode congelar termicamente máquinas de muitos cores (hard reset em 2026-04-19)
- O smoke com LLM real depende de OAuth ativo; uma chamada custa 10-90 segundos
- Jobs em background acima de ~80 minutos podem ser mortos por harnesses de agente (G42/C1); manter jobs de teste curtos
- `cargo-nextest` e `cargo-llvm-cov` NÃO são assumidos instalados; instalar via binários pré-compilados antes da Camada 1


## Planos Mais Recentes — v1.0.84 e v1.0.85

O plano de teste do Split do Backend Claude (ADR-0042) e o plano de teste da Remediação de Cinco Gaps (ADR-0043) estão consolidados neste documento; seus arquivos de snapshot independentes foram aposentados na v1.0.96.

## Plano de Teste v1.0.97 — Sidecar de Fila do `--db` + Poda de Órfãos Dead-Letter (ADR-0056/0057/0058, GAP-SG-57..66)

### Adições na Camada 1 (unit)
- `paths::sidecar_path` (3 testes): um `--db` absoluto deriva o sidecar ao lado dele; um nome puro (sem parent) cai no layout do CWD; um `--db` em diretório aninhado deriva o sidecar naquele diretório
- `prune_dead_orphans_removes_only_orphan_memory_rows`: somente linhas `status='dead'` com `item_type='memory'` cujo `item_key` não existe mais no banco principal são deletadas; linhas dead com chave de entidade e linhas dead de memória viva ficam intocadas
- Auditoria de `unwrap`/`expect` de produção (GAP-SG-57..60, ADR-0056) imposta por um lint gate do Clippy (`-D warnings`); a desduplicação de `parse_claude_output` mantém os parsers do enrich e do ingest_claude comportamentalmente idênticos

### Adições na Camada 2 (integração)
- `tests/enrich_queue_db_isolation.rs`: o enrich enfileira contra `tmpA/db.sqlite`, depois `enrich --status --db tmpA/db.sqlite` de um CWD diferente reporta o backlog enquanto `--db tmpB/db.sqlite` reporta zero — prova que a fila segue o `--db`, não o CWD

### Endurecimento de testes flaky
- GAP-SG-61 `concurrency_peak_never_exceeds_permits` e o cluster `llm_slots::tests` do GAP-SG-63 foram des-flakados (contabilidade determinística de permits); ambos verdes sob a suite completa

### Smoke do binário instalado (GAP-SG-62)
- `cargo install --path . --locked --force` realinhou `~/.cargo/bin/sqlite-graphrag` para 1.0.97; `installed_binary_smoke` agora roda 26/0 SEM o bypass de version-mismatch

### Totais de selagem
- `cargo test --lib` 973 passed / 0 failed; `cargo test` default 1164 / 0; `cargo test --features slow-tests` 1522 / 0 / 11 ignored; `cargo fmt --check` 0 diffs; `cargo clippy --all-targets --features slow-tests -- -D warnings` 0 warnings

## Plano de Teste v1.0.96 — Dead-Letter do Enrich + Concorrência REST OpenRouter (ADR-0055, GAP-ENRICH-BACKLOG-CONVERGE, GAP-OPENROUTER-REST-CONCURRENCY)

### Adições na Camada 1 (unit)
- Classificação de outcome (`commands::enrich::tests`, 8 testes): rate-limit / timeout / db-busy mapeiam para `AttemptOutcome::Transient`; validação / parse mapeiam para `HardFailure`
- `open_queue_db`: `ALTER TABLE` idempotente adicionando as colunas `error_class` e `next_retry_at` (re-execução é no-op)
- `record_item_failure`: um HardFailure marca o item como `dead` imediatamente; um Transient marca como `pending` com `next_retry_at` futuro via `compute_delay`; um Transient além de `--max-attempts` marca como `dead`
- Elegibilidade de dequeue: linhas com `next_retry_at` futuro são puladas e linhas `dead` são excluídas, logo o conjunto vivo é estritamente decrescente
- Ordem do fan-out de embedding (`embedder::tests::reassemble_ordered_restores_input_order`): conclusão fora de ordem do `JoinSet` é remontada pelo índice de chunk, restaurando a ordem de entrada

### Adições na Camada 2 (integração)
- Convergência do dead-letter: ingest de 6 ADRs com `--mode none`, depois `enrich --until-empty --rest-concurrency 8` drena `unbound_backlog` 6 → 0
- Segunda passada idempotente: re-executar `enrich --until-empty` faz zero trabalho (~6 ms) — nenhum item elegível resta

### Delta na Camada 8 (smoke com LLM real)
- `tests/openrouter_live_concurrency.rs` (`#[ignore]`, rodar com `cargo test --test openrouter_live_concurrency -- --ignored --nocapture`): embeda 64 chunks de `docs/*.md` em k=1 vs k=8
- Prova de ordem: cosseno diagonal 0.9999, off-diagonal max 0.899, argmax 64/64 — ordem de chunk preservada apesar da conclusão fora de ordem do JoinSet
- Total da suite: 1086 passed, 0 failed, 6 skipped via nextest

## Plano de Teste v1.0.95 — Enrich via Chat OpenRouter (ADR-0054, GAP-OR-ENRICH)

### Adições na Camada 1 (unit)
- Montagem de `ChatRequest` (`src/chat_api.rs`, `OpenRouterChatClient`): testes wiremock verificando `response_format` `json_schema` com `strict:true`, `provider.require_parameters:true` e `reasoning.enabled:false`
- Parse da resposta: extração de `choices[].message.content` seguida de um segundo parse JSON do payload de schema rígido
- Leitura de `usage.cost` do corpo da resposta
- Retry: `429` com header `retry-after`, backoff exponencial em `5xx`, `401` permanente sem retry
- Erros `400`/`404` retornados sem retry
- Conteúdo vazio / resposta de recusa tratados como modelo incompatível
- `validate_mode_flags`: rejeita flags `claude`/`codex`/`opencode` sob `--mode openrouter`
- `--openrouter-model` obrigatória: retorna exit 1 antes de qualquer chamada de rede quando ausente

### Adições na Camada 2 (integração)
- Dispatch do JUDGE para `call_openrouter` em todas as operações de enrich (`memory-bindings`, `entity-descriptions`, `body-enrich`)
- Validação de chave API via `resolve_api_key` sem spawn de subprocesso

### Delta na Camada 8 (smoke com LLM real)
- `tests/openrouter_chat_real.rs` (`#[ignore]`, rodável com `OPENROUTER_API_KEY`) iterando os 13 modelos de texto contra o schema rígido
- Matriz de compatibilidade 13/13 (9 diretos com `reasoning.enabled:false`, 4 via fallback reasoning-mandatory)

## Plano de Teste v1.0.93 — Backend de Embedding OpenRouter (ADR-0052, GAP-OR-INGEST)

### Adições na Camada 1 (unit)
- `model_default_input_type()`: 10 testes cobrindo seleção de `input_type` por modelo (fix BUG-OR-1 — NVIDIA Nemotron retorna `"passage"`, Mistral retorna `None`, demais retornam `"search_document"`)
- `model_supports_mrl()`: testes cobrindo detecção MRL para todos os 10 modelos verificados incluindo NVIDIA e BAAI (fix BUG-OR-2)
- `validate_model_id()`: testes cobrindo validação de ID de modelo contra os 10 modelos aprovados e rejeição de 5 IDs inexistentes (fixes BUG-OR-3, BUG-OR-4)
- `execute_with_retry()`: teste cobrindo retry de corpo malformado em HTTP 200 (fix BUG-OR-5 — parse error em HTTP 200 tratado como transitório)

### Adições na Camada 2 (integração)
- `tests/openrouter_embedding.rs`: testes de integração baseados em wiremock cobrindo o fluxo completo de embedding via API REST OpenRouter — construção de request, truncamento MRL, `input_type` por modelo, chunking em batch (MAX_BATCH_SIZE=32), retry de erro e tratamento de chave API via `secrecy::SecretString`
- Propagação de `EmbeddingBackendChoice`: testes verificando que `--embedding-backend openrouter` alcança todos os 8 comandos (remember, remember-batch, ingest, recall, edit, restore, hybrid-search, deep-research)
- Flag `--enrich-after`: testes verificando que `ingest --enrich-after` dispara `enrich --operation memory-bindings` sequencialmente após a fase de embedding

### Adições na Camada 7 (pós-publicação)
- Round-trip de embedding OpenRouter: `remember` com `--embedding-backend openrouter --embedding-model "qwen/qwen3-embedding-8b"` seguido de `recall` com as mesmas flags, verificando similaridade vetorial
- Exit 78 na ausência de `--embedding-model` quando `--embedding-backend openrouter` está especificado

### Delta na Camada 8 (smoke com LLM real)
- Opcional: um teste smoke de embedding OpenRouter usando `OPENROUTER_API_KEY` real (opt-in via `SQLITE_GRAPHRAG_OPENROUTER_E2E=1`)
- Orçamento: 1 chamada de API, menos de 5 segundos, latência esperada de embedding abaixo de 500ms

## Plano Histórico — Deltas do Plano v1.0.80 — G45, G53, G55 S2, G56, G58, ADR-0033, ADR-0034

A release v1.0.80 (bump patch, sem migração de schema) adicionou os
seguintes deltas de teste à matriz por camada acima.
Consumidores da biblioteca são FORTEMENTE aconselhados a fixar em
`=1.0.80` porque a API da lib é instável em v1.x.y (ADR-0032).

### Adições na Camada 1 (unit)

- `acquire_embedding_singleton` (G45): 5 testes cobrindo contenção
  de lock no mesmo banco, independência entre bancos distintos,
  polling via `--wait-embed-singleton`, flag `force` e detecção
  de lock stale via PID.
- `AppError::MemoryNotFound` e `AppError::MemoryNotFoundById`
  (G55 S2): 6 testes afirmando que o identificador é parte da
  variante, exit code é 4 e a mensagem localizada em pt-BR
  carrega nome e namespace explicitamente.
- `embed_entity_texts_cached` (G56): 4 testes afirmando hit de
  cache na segunda chamada com mesmo model+text, miss em texto
  diferente, contabilidade do `EmbedCacheStats` e comportamento
  quando o embedder subjacente retorna erro.
- `recall --fallback-fts-only` e `hybrid-search --fallback-fts-only`
  (G58): 3 testes cobrindo o caminho FTS5-only, mais 1 teste
  `#[ignore]` que exercita o caminho `EmbeddingFailed` (exige
  `PATH` sem `codex` ou `claude`).

### Adições na Camada 2 (integração)

- `tests/completions.rs`: 7 testes end-to-end para o subcomando
  `completions` (bash, zsh, fish, powershell, elvish, exit code
  de shell inválido, validação de output não-vazio por shell).
- `tests/shutdown_bypass.rs`: 3 testes de integração cobrindo a
  receita de bypass SHUTDOWN em 3 camadas (`PATH=tests/mock-llm:...`
  mais `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` mais `setsid -w timeout`).
- `tests/embedder_singleton.rs`: 2 testes de integração cobrindo
  o singleton de embedding cross-process contra um banco
  temporário (invocações concorrentes de `remember` no mesmo par
  `(namespace, db)` serializam; pares distintos prosseguem em
  paralelo).

### Adições na Camada 3 (doctest)

- 4 novos exemplos de doctest para `acquire_embedding_singleton`,
  `embed_entity_texts_cached`, construção de `MemoryNotFound` e
  a receita de bypass SHUTDOWN em 3 camadas (verificados via
  `cargo test --doc` em todo commit).

### Adições na Camada 4 (contratos lentos)

- `tests/doc_contract_integration.rs`: 2 novos testes de contrato
  validando que os campos `vec_degraded`, `vec_error` e `warning`
  do envelope aparecem nas respostas JSON de `recall` e
  `hybrid-search` quando o subprocesso LLM falha (G58).
- `tests/prd_compliance.rs`: 1 novo teste de compliance PRD
  validando que os 6 novos símbolos públicos da biblioteca
  documentados em CHANGELOG.md (G45 e G56) são todos `pub` e
  têm as assinaturas documentadas.

### Adições na Camada 7 (pós-publicação)

- A matriz black-box pós-publicação agora inclui 3 novos contratos
  de exit code: `EmbeddingSingletonLocked` (exit 75, retentável),
  `MemoryNotFound` com identificador na mensagem (exit 4) e
  `vec_degraded: true` em `recall` (exit 0 com warning).

### Delta na Camada 8 (smoke com LLM real)

- O score do top hit do round-trip de `recall` com LLM real é
  registrado como o novo baseline de qualidade de retrieval
  do G54 (campo já existente no protocolo de smoke; a v1.0.80
  apenas torna o registro obrigatório).

### Gates — adições

- Gate 2 (clippy) ganha `--all-features` (era somente
  `--all-targets`) e permanece a barra bloqueante.
- Gate 8 (CI VERDE) agora exige o novo job `semver-checks`
  (modo informativo em v1.0.80, vira bloqueante em v1.0.81).
  O bug de `--manifest-path` duplicado do commit inicial da
  v1.0.79 está corrigido.
- Os jobs da matrix windows-2025 ganharam steps de pre-warm e
  verify gateados em `if: matrix.os == 'windows-2025'`
  (ADR-0033, G53-WINDOWS-INFRA). Validação local de cross-compile:
  `cargo check --target x86_64-pc-windows-msvc --lib --all-features`
  reproduzido e o `E0463` resolvido via `rustup target add
  x86_64-pc-windows-msvc --toolchain 1.88`; o build então atinge
  a fronteira `cc-rs: failed to find tool "lib.exe"`, que é o
  limite esperado de cross-compile MSVC a partir de host Linux.

### Atualização de gatilhos

- Por commit: Camadas 1-3 mais Gates 1-2 (inalterado).
- Por release (antes do `cargo publish`): Camadas 1-6 mais os
  8 gates mais os gates informativos. O novo gate informativo
  `semver-checks` agora faz parte deste gatilho.
- Pós-publicação: Camadas 7-8 contra o binário instalado do
  registry (inalterado). A matriz da Camada 7 agora inclui os
  3 novos contratos de exit code da v1.0.80 acima.

## Rastreabilidade
- Toda falha encontrada por este plano vira um gap numerado no `gaps.md` com status, causa raiz e cadeia causa-efeito
- Gaps corrigidos devem referenciar o teste de regressão que protege a correção
- Auditoria de 2026-06-11: a primeira execução deste plano produziu o G46-G54 e suas correções
