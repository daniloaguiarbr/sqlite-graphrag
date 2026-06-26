# Integrações

> Leia este documento em [inglês (EN)](INTEGRATIONS.md)


> 21 agentes e 20+ plataformas em um único contrato de CLI

- Leia a versão em inglês em [INTEGRATIONS.md](INTEGRATIONS.md)
- Cada receita abaixo está pronta para copiar e custa zero para executar
- **v1.0.79: todo build é apenas LLM e one-shot.** A geração de embedding delega para um subprocesso headless `claude code` ou `codex` (OAuth). O daemon, o runtime ONNX e a feature `embedding-legacy` foram totalmente removidos; os embeddings são em lote, paralelos (`--llm-parallelism`) e com 64 dimensões por padrão (`--embedding-dim`, faixa [8, 4096]).


## Aliases de Flags CLI (desde v1.0.35)
- `recall` e `hybrid-search` aceitam `--limit` como alias de `-k`/`--k`. Os exemplos abaixo usam `--k` e continuam válidos.
- `rename` aceita `--from`/`--to` como aliases de `--name`/`--new-name` (aliases legados `--old`/`--new` continuam suportados).
- Todos os campos JSON `schema_version` (`init`, `stats`, `migrate`, `health`) são emitidos como números JSON (eram string em `init`/`stats`/`migrate` antes da v1.0.35).
- Auto-init via `remember`/`ingest`/etc. agora ativa `journal_mode = wal` corretamente (correção de regressão).

## Novas Flags (desde v1.0.45)
- A extração NER de entidades está **desativada por padrão**. Passe `--enable-ner` em `remember` ou `ingest` para ativar; defina `SQLITE_GRAPHRAG_ENABLE_NER=1` para override persistente de sessão.
- `--skip-extraction` está obsoleto e não tem efeito desde v1.0.45 (NER está desativado por padrão); a flag é mantida como no-op oculto para compatibilidade — remova-a dos scripts.
- `--graph-stdin` em `remember` lê um único objeto JSON do stdin contendo `body`, `entities` e `relationships`, sendo a forma preferida de fornecer grafos curados por um LLM.

## Novas Flags (desde v1.0.47)
- O pipeline GLiNER zero-shot NER foi REMOVIDO na v1.0.79 com a feature `ner-legacy`; `--enable-ner` agora executa apenas extração de URL por regex.
- `--gliner-variant`, `SQLITE_GRAPHRAG_GLINER_VARIANT` e `SQLITE_GRAPHRAG_GLINER_THRESHOLD` são aceitas por compatibilidade mas NÃO têm efeito desde a v1.0.79.
- Para extração de entidades/relacionamentos curada por LLM use `ingest --mode claude-code` ou `ingest --mode codex`.
- Os tipos de entidade agora incluem `organization`, `location`, `date` além de `person`, `project`, `tool`, `file`, `concept`, `decision`, `incident`, `dashboard`, `issue_tracker`, `memory`.

## Novos Comandos e Flags (desde v1.0.93)
- `--embedding-backend auto|openrouter|llm` — seleciona o backend de embedding (flag global)
- `--embedding-model MODEL` — seleciona o modelo de embedding para OpenRouter (flag global, OBRIGATÓRIO com openrouter)
- `--openrouter-api-key KEY` — chave de API para OpenRouter (flag global)
- `--enrich-after` — executa enrich após a conclusão do ingest (flag do ingest)
- **GAP-OR-PROPAGATION**: Todos os 13 paths de embedding agora honram `--embedding-backend` — incluindo `enrich`, `init`, `rename-entity`, `ingest --mode claude-code`, `remember` (chunks)
- Exit code 78 (`EX_CONFIG`) para erros de configuração OpenRouter (chave API ausente, modelo ausente, chave inválida)
- 10 modelos verificados E2E com dim=64 MRL: `google/gemini-embedding-001` (0.892), `google/gemini-embedding-2` (0.868), `mistralai/mistral-embed-2312` (0.832), `qwen/qwen3-embedding-8b` (0.814), `qwen/qwen3-embedding-4b` (0.754), `openai/text-embedding-3-small` (0.668), `nvidia/llama-nemotron-embed-vl-1b-v2:free` (0.662), `baai/bge-m3` (0.537), `openai/text-embedding-3-large` (0.449), `perplexity/pplx-embed-v1-0.6b` (0.415)

## Novos Comandos e Flags (desde v1.0.68)
### Ciclo de Vida de Processos (G28)
- `enrich`, `ingest --mode claude-code` e `ingest --mode codex` agora adquirem um singleton por namespace antes de fazer trabalho real.  Uma segunda invocação concorrente no mesmo banco falha rápido com `AppError::JobSingletonLocked { job_type, namespace }` (exit 75) em vez de empilhar árvores de subprocessos.
- Env var `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` (opt-in) — quando definida para um diretório existente e vazio, o subprocesso do Claude Code é iniciado com `CLAUDE_CONFIG_DIR=<esse dir>`, suprimindo servidores MCP do escopo user e a fan-out de 8-10 processos.  Este é o único mecanismo que o upstream do Claude Code realmente honra (veja [anthropics/claude-code#10787]).  Deliberadamente NÃO passamos `--strict-mcp-config` nem `--mcp-config '{}'` porque ambos são ignorados.
- `retry::CircuitBreaker` (API do crate Rust) — helper opt-in com `AttemptOutcome::{Success, Transient, HardFailure}`.  Erros rate-limited e timeout são explicitamente excluídos da contagem.  Use em loops de retry customizados para limitar iterações em falhas persistentes.
- `enrich` emite `tracing::warn!` (visível com `-v`) quando `--llm-parallelism > 4`, recomendando combinar com `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` para manter a fan-out administrável.
### Build Windows (G29)
- `cargo install sqlite-graphrag` no Windows agora compila.  O tipo `HANDLE` é tratado de forma type-safe via `!handle.is_null() && handle != INVALID_HANDLE_VALUE`.  `windows-sys` está fixado em `=0.59.0` exato em `Cargo.toml`.  Novo job de CI `windows-build-check` roda `cargo check --target x86_64-pc-windows-msvc --lib --all-features` em todo push e PR.

## Novos Comandos e Flags (desde v1.0.69)
### Enforcement OAuth-Only (G28-A, G31, Mudança Comportamental)
- Os spawns de `claude -p` e `codex exec` agora ABORTAM com `AppError::Validation` se `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estiverem presentes no ambiente.  OAuth (Claude Pro/Max ou ChatGPT Pro) é o ÚNICO fluxo de credencial aceito.  Veja `docs/decisions/adr-0011-oauth-only-enforcement.md` para a justificativa completa.
- A flag `--bare` (que exige uma chave de API e desabilita OAuth) foi REMOVIDA de todo caminho executável.  Ambas as variáveis de chave de API também são excluídas da whitelist de `env_clear` como defesa em profundidade.
### `enrich` — Novo Subcomando (G29 + G35 + G37)
- `enrich --operation <op> --mode <claude-code|codex> --json` roda qualidade do grafo curada por LLM.  Três operações estão totalmente implementadas: `memory-bindings` (extrai entidades de memórias órfãs), `entity-descriptions` (preenche descrições de entidade NULL ou vazias) e `body-enrich` (expande corpos curtos de memória, agora com sucesso 100% após o hotfix do G29 na CHECK constraint de `source` e a trilha de auditoria do G29 via `memory_versions`).
- `--preserve-threshold <FLOAT>` (padrão 0.7) controla o portão de preservação trigrama Jaccard de `src/preservation.rs` (10 testes).  Scores abaixo do threshold são rejeitados e emitidos como `EnrichItemResult::PreservationFailed`.
- `--preflight-check`, `--fallback-mode <claude-code|codex>` e `--rate-limit-buffer <SEGUNDOS>` (padrão 300) evitam perda de batch quando a janela OAuth de 5 horas do Claude fecha no meio do run.  A sondagem de preflight emite um ping de 1 turno; em rate limit aborta com erro claro ou troca para `--fallback-mode`.
- `--names <a,b,c>` e `--names-file <CAMINHO>` selecionam um subconjunto específico de nomes de memória.  `--names-file` aceita comentários `#` e linhas em branco.  As duas flags se combinam como união.
- O aviso de `--llm-parallelism <N>` é condicional ao modo: Claude avisa em 5 (fan-out OAuth-MCP), Codex avisa em 17 (risco de rate limit), Codex 5..16 fica silencioso (validado em 1161 itens, 0 falhas em produção).
- `--max-load-check` recusa iniciar quando o load average > `2 × ncpus`.  `--circuit-breaker-threshold <N>` (padrão 5) aborta após N resultados `HardFailure` consecutivos.
### Família de Subcomandos `vec` (G39)
- `vec orphan-list --json` lista linhas de embedding de memória órfãs com `vector_hash` (BLAKE3 do blob de embedding).
- `vec purge-orphan --yes --dry-run --json` faz preview da deleção.  `vec purge-orphan --yes --json` purga as TRÊS vec tables (`vec_memories`, `vec_entities`, `vec_chunks`) em uma única transação.
- `vec stats --json` expõe `vec_memories_rows`, `vec_entities_rows`, `vec_chunks_rows`, `orphans` e o timestamp do último vacuum.
- `forget` agora chama `memories::delete_vec` ANTES do soft-delete, prevenindo novos órfãos em estado estável.
### Subcomando `codex-models` (G33)
- `codex-models --json` lista a whitelist de modelos aceitos pelo ChatGPT Pro OAuth: `codex-auto-review`, `gpt-5.3-codex-spark`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.5`.  Retorna `models`, `count` e `default`.
- `codex-models --suggest <substring> --json` retorna a correspondência mais próxima via busca por substring com fallback Levenshtein.  `enrich --codex-model-validate` (padrão true) verifica o modelo ANTES de o subprocesso ser spawnado e aborta com uma sugestão quando inválido.  `--codex-model-fallback <MODELO>` auto-substitui em vez de abortar.
### Endurecimento de `optimize` e `backup` (G36 + G38)
- `optimize` faz pré-verificação da saúde do FTS5 via `check_fts_functional` ANTES de reconstruir.  `--fts-dry-run` sai com código 1 se a reconstrução for recomendada.  `--fts-progress <N>` (padrão 30) emite progresso a cada N segundos.  `--yes` pula o prompt de confirmação.  `--no-fts-skip-when-functional` força uma reconstrução.
- `backup` usa por padrão `run_to_completion(1000, Duration::from_millis(5), None)` — 25x mais rápido que os padrões da v1.0.68.  `--backup-step-size <PAGES>`, `--backup-step-sleep-ms <MS>`, `--backup-no-sleep` e `--backup-progress <PAGES>` (padrão 100) fornecem tunabilidade.
### Singleton Escopado por `db_hash` (G30)
- `lock::acquire_job_singleton(job_type, namespace, db_path, wait_seconds, force)`.  Duas invocações concorrentes de `enrich` em bancos DIFERENTES não colidem mais.  `db_hash` são os primeiros 12 caracteres hex de `blake3(canonicalize(db_path))`.
- `--wait-job-singleton <SEGUNDOS>` sonda pelo lock.  `--force-job-singleton` quebra um lock obsoleto.  Ambos disponíveis em `enrich` e `ingest`.
### Helper de Spawn do Codex Unificado (G31 + G32 + G33)
- `src/commands/codex_spawn.rs` (~700 linhas, 11 testes) unifica o pipeline de spawn, parser JSONL e validação de modelo ChatGPT Pro OAuth.  Ambos `enrich --mode codex` e `ingest --mode codex` consomem o mesmo comando canônico.  O wrapper externo `~/.local/bin/codex-clean` agora é obsoleto.
- 7 flags de endurecimento: `--json --output-schema --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules` mais `-c mcp_servers='{}' --ask-for-approval never`.  O schema JSON agora vive em `paths::AppPaths::cache_dir().join("schemas")` em vez de `/tmp` (diretório trusted).
### Enum `MemorySource` e Preservação (G29)
- `src/memory_source.rs` define um enum type-safe dos cinco valores da CHECK constraint: `Agent`, `User`, `System`, `Import`, `Sync`.  `TryFrom<&str>` retorna `AppError::Validation` listando os valores aceitos.  O guard runtime `validate_source` é chamado de `memories::insert` e `memories::update`.  O enum é a fundação para a migração da v1.0.70.
- Idempotência via `blake3::hash`: quando `old_hash == new_hash`, o corpo é pulado com a razão `"enriched body hash matches original (blake3:{hash}); idempotency skip"`.  Reprocessar a mesma memória é seguro.
### Circuit Breaker e System Load (G28-D)
- `retry::CircuitBreaker` é integrado no loop de workers com `breaker.record(AttemptOutcome::HardFailure)`.  O loop aborta após `--circuit-breaker-threshold` falhas consecutivas (padrão 5, defina como 0 para desabilitar).
- `src/system_load.rs` fornece `load_average_one`, `ncpus` e `is_system_saturated`.  `enrich` aborta o spawn quando `load_average_one() > 2 * ncpus` e `--max-load-check` está set (padrão true).
### Reaper de Órfãos (G28-C)
- `src/reaper.rs` varre `/proc` no startup, mata qualquer órfão `claude`/`codex` com `PPID=1` e idade maior que 60s.  Invocado do `main` ANTES de qualquer trabalho.  Suíte de 4 testes: `orphan_min_age_is_one_minute`, `orphan_targets_include_claude_and_codex`, `reaper_report_starts_zeroed`, `scan_completes_without_panic_on_linux`.

### Camada de Validação Pre-flight (v1.0.87+ — ADR-0045)
- Todo spawn de subprocesso LLM passa por `src/spawn/preflight.rs` (15 testes unitários, 7 guards) ANTES do fork.  Falhas retornam `AppError::PreFlightFailed` (código de saída 16, `EX_CONFIG`) sem spawnar o subprocesso.
- Os 7 guards em ordem: `check_argv_size` (rejeita invocações que excederiam `ARG_MAX` menos 4 KB), `check_binary_exists` (confirma que `claude`/`codex` está alcançável no `PATH`), `check_mcp_config_inline` (substitui o literal `--mcp-config {}` por um tempfile contendo `{"mcpServers":{}}` — corrige BUG-2), `check_mcp_config_path` (valida o conteúdo JSON de `--mcp-config <PATH>`), `check_walkup_mcp_json` (valida o walk-up de `.mcp.json` a partir da raiz do workspace), `check_output_buffer` (eleva o buffer do parser acima de 64 KB quando necessário — corrige BUG-4), `check_claude_config_dir` (valida que `CLAUDE_CONFIG_DIR` está vazio/ausente para evitar vazamento de MCP).
- Bypass em emergências: defina `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` para desabilitar todos os 7 guards.  Opt-out de último recurso para mitigação de incidente em produção; o bypass reverte para `Command::spawn()` direto e herda todas as 5 classes de BUG do GAP-META-005.
- Hotfixes relacionados da v1.0.88: BUG-11 (falha de preflight em `extract/llm_embedding.rs` não propagava para `remember`; corrigido com `embed_via_backend_strict` em `bug11_preflight_regression.rs`); BUG-12 (o enforcement OAuth-only emitia 2 linhas idênticas em stderr; corrigido com stderr de linha única em `oauth_stderr_emits_single_line_v1088`); BUG-13 (`link --create-missing` burlava a validação de nome de entidade; corrigido validando ANTES de normalizar em `entity_validation_integration.rs`).

### Correções no Pipeline de Embedding e Novas Flags Globais (v1.0.89 — ADR-0050)
- 7 flags globais de LLM agora são propagadas da CLI para variáveis de ambiente via set_var em main.rs: --claude-binary, --codex-binary, --llm-model, --skip-embedding-on-failure, --llm-max-host-concurrency, --llm-slot-wait-secs, --llm-slot-no-wait. Antes eram aceitas pelo clap mas silenciosamente ignoradas pelos módulos internos
- Nova flag --codex-binary (simétrica a --claude-binary) com variável de ambiente SQLITE_GRAPHRAG_CODEX_BINARY
- --skip-embedding-on-failure agora funcional: persiste memórias com embedding NULL em vez de exit 11; faça o backfill com enrich --operation re-embed
- --llm-fallback agora funcional: a cadeia CSV (codex,claude,none) é honrada pela resolução do backend Auto via parse_fallback_chain()
- deep-research e remember-batch agora honram --llm-backend (antes ignoravam o parâmetro)
- Timeout de embedding adaptativo: embed_timeout_for_batch() escala base + 15s por item de chunk adicional
- Degradação graciosa do FTS5: deep-research, recall e hybrid-search caem para FTS5-only quando o embedding LLM está indisponível
- BoolishValueParser em 4 flags booleanas: --skip-embedding-on-failure, --strict-env-clear, --dry-run-backend, --llm-slot-no-wait agora aceitam 1/0/yes/no/on/off (antes só true/false)
- Dica de expiração OAuth: invoke_claude() detecta padrões 401/Unauthorized e sugere claude login
- Modelos padrão restaurados: codex usa gpt-5.5 e claude usa claude-sonnet-4-6 quando nenhuma variável de modelo está definida

## Novos Comandos e Flags (desde v1.0.76)
### Arquitetura LLM-Only One-Shot (G21 + G22 + G23 + G24 + G25)
- O build padrão da v1.0.76 é LLM-Only e one-shot.  Sem daemon, sem runtime ONNX, sem download do modelo `multilingual-e5-small`.  A geração de embeddings e a NER delegam para um subprocesso headless `claude code` ou `codex` (OAuth, sem MCP, sem hooks).  O binário de release tem aproximadamente 6 MB.
- A feature `embedding-legacy` foi REMOVIDA na v1.0.79 (antecipando o cronograma da v1.1.0).  O pipeline legado fastembed + ort + tokenizers não existe mais; todo build é LLM-only.
- Veja ADR-0019, ADR-0020, ADR-0021, ADR-0022, ADR-0023, ADR-0024, ADR-0025, ADR-0026 para todas as decisões arquiteturais.
### Família de Subcomandos `migrate` (v1.0.76)
- `migrate --rehash --json` reescreve os checksums registrados de migração para casar com o conteúdo atual do arquivo.  Algoritmo casa com `refinery-core 0.9.1` (SipHasher13, mesma ordem de hashing).  Obrigatório para upgrades v1.0.74 → v1.0.76 onde V002 foi intencionalmente esvaziada para um no-op.  Schema de resposta: `migrate-rehash.schema.json`.
- `migrate --to-llm-only --drop-vec-tables --json` é o upgrade one-shot para bancos v1.0.74 / v1.0.75: rehash + descarte da V013 das vec tables + relatório de estado das vec tables.  A flag `--drop-vec-tables` é OBRIGATÓRIA como rede de segurança.  Schema de resposta: `migrate-to-llm-only.schema.json`.
### Tabelas de Embedding com Backing BLOB (G22)
- A migração V013 descarta as virtual tables `vec_memories`, `vec_entities` e `vec_chunks` e as substitui por tabelas regulares com backing BLOB `memory_embeddings`, `entity_embeddings` e `chunk_embeddings`.  A similaridade por cosseno é computada em Rust puro sob demanda em `src/similarity.rs` (ADR-0020, ADR-0022).
### Refinamento da Hybrid Search (G24)
- A `hybrid-search` usa FTS5 como filtro grosso e refina o conjunto de candidatos com cosseno em Rust puro sobre os embeddings BLOB.  O FTS5 permanece saudável porque a reconstrução é bloqueada por `optimize --fts-skip-when-functional` (G36 da v1.0.69).
### Seletor de Backend de Extração
- Nova flag global `--extraction-backend llm|embedding|none|both` (padrão `llm`) seleciona o backend de extração.  `llm` é o caminho LLM; `embedding` é um stub permanente desde a v1.0.79 (pipeline legado removido) que retorna erro de migração; `none` é um no-op; `both` roda os dois em paralelo e funde os resultados.
- `src/extract/` expõe o trait `ExtractionBackend` com as quatro implementações.  `src/spawn/` expõe o trait `VersionAdapter` com `CodexAdapter` (detecta `codex 0.130.0` até `0.138+` e adapta flags — `codex 0.137.0` removeu `--ask-for-approval` em favor de `-a never`), `ClaudeAdapter` (claude code 2.1.0+) e `OpencodeAdapter` (opencode headless).
### Remoção do Daemon (ADR-0021)
- O subcomando `daemon` foi DEPRECIADO na v1.0.76 e TOTALMENTE REMOVIDO na v1.0.79 (antecipando o cronograma da v1.1.0).  O subprocesso LLM é o "model loader"; a CLI é 100% one-shot com zero IPC.

## Novos Comandos e Flags (v1.0.79 — pipeline de embedding G42)
- Flag global `--embedding-dim <N>` define a dimensionalidade do embedding (padrão 64, faixa [8, 4096]); precedência: flag > env `SQLITE_GRAPHRAG_EMBEDDING_DIM` > o `dim` gravado em `schema_meta` > 64; bancos 384-dim existentes continuam funcionando sem mudança
- `--llm-parallelism <N>` agora disponível em `remember` (padrão 4), `ingest` (padrão 2) e `edit` — fan-out limitado via `Semaphore` + `JoinSet`, permits com clamp [1, 32]
- `enrich --operation re-embed --limit N --resume` é o caminho canônico de re-embed one-shot (ex.: após mudar `--embedding-dim`)
- `edit --force-reembed` regenera o embedding de uma memória sem alterar o corpo
- `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` sobrescreve o modelo de embedding do claude (simétrica à variável do codex); `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` limita cada chamada LLM de embedding (padrão 300)
- Chamadas LLM são em lote (schema `{items:[{i,v}]}` — bases de calibração de 8 chunks / 25 nomes de entidade em dim 64, adaptativas por clamp(base×64/dim, 1, base) desde o G44) e todo subprocesso usa `kill_on_drop` mais timeout explícito

## Novos Comandos e Flags (desde v1.0.67)
- `remember-batch` cria memórias em lote via NDJSON no stdin em uma única invocação; `--transaction` para atomicidade, `--force-merge` para atualizações idempotentes, `--fail-fast` para parar no primeiro erro
- `completions` gera completions de shell para Bash, Zsh, Fish, PowerShell e Elvish
- `read --id <N>` busca memória por `memory_id` inteiro diretamente (sem resolução de nome)
- `read --with-graph` inclui entidades e relacionamentos vinculados na resposta JSON
- `enrich --llm-parallelism <N>` spawna N threads paralelas de LLM (padrão 1, máximo 32)
- `health` detecta entidades super-hub (grau > 50) e reporta `super_hub_count`, `top_hub_entity`, `top_hub_degree`
- `health` reporta `non_normalized_count` e `normalization_warning` para entidades fora do padrão kebab-case
- `edit` pula re-embedding quando conteúdo do body é inalterado (comparação body_hash)
- `rename` purga memórias ghost (soft-deleted) que ocupam o nome destino antes do UPDATE
- `hybrid-search` e `recall` rejeitam `--max-hops` e `--min-weight` quando travessia de grafo está desabilitada
- Migração V012 adiciona `created_at`/`updated_at` na tabela relationships

## Novos Comandos e Flags (desde v1.0.66)
- `edit --type` altera tipo de memória sem recriar
- `deep-research` campo `graph_context` na resposta JSON com entidades e relacionamentos das memórias encontradas
- `graph --format json` inclui alias `entities` junto com `nodes` para compatibilidade com agentes LLM
- `list --json` inclui alias `memories` junto com `items` para compatibilidade com agentes LLM
- `graph entities --json` inclui campo `description` por entidade
- `health --json` inclui contagens `vec_memories_missing` e `vec_memories_orphaned`

## Novos Comandos e Flags (desde v1.0.65)
- `reclassify-relation --from-relation <antigo> --to-relation <novo> --batch` renomeia tipos de relação em massa; modo individual via `--source`/`--target`; trata colisões UNIQUE via `UPDATE OR IGNORE` + `DELETE`; `--dry-run` faz preview; filtros opcionais `--filter-source-type`/`--filter-target-type`
- `normalize-entities --yes` normaliza todos os nomes de entidade para kebab-case ASCII minúsculo; mescla colisões automaticamente; `--dry-run` faz preview
- `enrich --operation <op> --mode claude-code` qualidade do grafo aumentada por LLM; operações: `memory-bindings`, `entity-descriptions`, `body-enrich`; `--dry-run` faz preview sem LLM; `--max-cost-usd`, `--resume`, `--retry-failed`
- `deep-research` novas flags: `--rrf-k` (padrão 60), `--graph-decay` (padrão 0.7), `--graph-min-score` (padrão 0.05), `--max-neighbors-per-hop`
- `--max-entity-degree N` em `link` e `remember` emite `tracing::warn!` quando entidade excede N conexões
- `health` reporta `top_relation`, `top_relation_ratio`, `applies_to_ratio`, `relation_concentration_warning` quando qualquer relação excede 40%
- Nomes de entidade normalizados para kebab-case em todo path de escrita (remember, ingest, link, rename-entity)

## Comportamento do Daemon (HISTÓRICO — daemon removido na v1.0.79)
- Apenas da v1.0.50 até a v1.0.78: a CLI reiniciava automaticamente o daemon em caso de incompatibilidade de versão.  Desde a v1.0.79 não existe processo daemon

## Novos Comandos e Flags (desde v1.0.56)
- `fts rebuild` reconstrói o índice FTS5 de busca textual do zero
- `fts check` executa integrity-check do FTS5 sem modificar o índice
- `fts stats` exibe estatísticas do índice FTS5 (contagem, páginas shadow, status funcional)
- `backup --output <caminho>` cria cópia segura do banco via SQLite Online Backup API
- `delete-entity --name <entidade> --cascade` remove entidade e cascateia para relacionamentos e bindings NER
- `reclassify --name <entidade> --entity-type <novo>` altera tipo; `--from-type <antigo> --to-type <novo> --batch` para massa
- `merge-entities --names "a,b,c" --into <destino>` funde entidades-fonte no destino, movendo todas as edges
- `rename-entity --name <antigo> --new-name <novo>` renomeia uma entidade do grafo preservando todos os relacionamentos baseados em FK e re-gera embedding para busca semântica
- `memory-entities --name <memória>` lista entidades vinculadas a uma memória específica
- `prune-ner --entity <nome>` ou `--all --yes` remove bindings NER da tabela memory_entities
- `cleanup-orphans --dry-run --json` audita entidades com zero memórias e zero relacionamentos; `--yes` remove
- `prune-relations --relation <tipo> --dry-run --json` visualiza remoção em massa de todos relacionamentos de um tipo; `--yes` executa
- `remember --dry-run` valida input e reporta ações planejadas sem persistir
- `remember --clear-body` limpa explicitamente o body durante `--force-merge` (body vazio agora preserva existente por padrão)
- `remember --type` e `--description` agora opcionais com `--force-merge` (herdados da memória existente)
- `list` limite padrão é todas as memórias com `--json`, 50 para texto; resposta inclui `total_count`, `truncated`, `body_length`
- `history --diff` inclui resumo de mudanças por caractere entre versões consecutivas
- `hybrid-search` degradação graciosa do FTS5: campos `fts_degraded`, `fts_error`, `fts_auto_rebuilt`; auto-rebuild em corrupção
- `hybrid-search` adiciona `normalized_score` (0-1), `vec_distance`, `fts_bm25` scores brutos
- `health` adiciona `fts_query_ok` (teste funcional FTS5 MATCH), `sqlite_version`
- `optimize --skip-fts` pula rebuild do FTS5; campo `fts_rebuilt` na resposta
- `link --strict-relations` rejeita tipos de relação não-canônicos; campo `warnings` na resposta
- `unlink --relation` agora opcional (remove todos entre o par); `--entity <nome> --all` para massa
- `graph entities --sort-by degree|name|created_at --order asc|desc`; campo `degree` na resposta
- `ingest --max-name-length N` configura truncagem; `body_length` no NDJSON; auto-prefixo `doc-` para nomes numéricos
- `daemon --ping` adicionava campos `model_name`, `model_variant` (HISTÓRICO — o daemon foi removido na v1.0.79)
- TODOS os caminhos de erro agora emitem JSON no stdout: `{"error": true, "code": N, "message": "..."}`
- Sync FTS5 corrigido em `edit`, `rename`, `restore` — memórias editadas agora imediatamente localizáveis via busca textual


## Tabela Resumo
### Catálogo — Toda Integração Suportada
| Nome | Tipo | Versão Mínima | Exemplo | Docs Oficiais |
| --- | --- | --- | --- | --- |
| Claude Code | Agente IA | 1.0+ | `sqlite-graphrag recall "query" --json` | https://docs.anthropic.com/claude-code |
| Codex CLI | Agente IA | 0.5+ | `sqlite-graphrag remember --name X --type user --body "..."` | https://github.com/openai/codex |
| Gemini CLI | Agente IA | recente | `sqlite-graphrag hybrid-search "query" --k 5 --json` | https://github.com/google-gemini/gemini-cli |
| Opencode | Agente IA | recente | `sqlite-graphrag recall "auth flow" --json` | https://github.com/opencode-ai/opencode |
| OpenClaw | Agente IA | recente | `sqlite-graphrag list --type user --json` | projeto comunitário |
| Paperclip | Agente IA | recente | `sqlite-graphrag read --name note --json` | projeto comunitário |
| VS Code Copilot | Agente IA | 1.90+ | tasks.json | https://code.visualstudio.com/docs/copilot |
| Google Antigravity | Agente IA | recente | `sqlite-graphrag hybrid-search "prompt" --json` | docs do Antigravity |
| Windsurf | Agente IA | recente | `sqlite-graphrag recall "plano refactor" --json` | https://windsurf.com/docs |
| Cursor | Agente IA | 0.40+ | `sqlite-graphrag remember --name cursor-ctx --type project --body "..."` | https://cursor.com/docs |
| Zed | Agente IA | recente | `sqlite-graphrag recall "abas abertas" --json` | https://zed.dev/docs |
| Aider | Agente IA | 0.60+ | `sqlite-graphrag recall "refactor" --k 5 --json` | https://aider.chat |
| Jules | Agente IA | preview | `sqlite-graphrag stats --json` | https://jules.google |
| Kilo Code | Agente IA | recente | `sqlite-graphrag recall "tarefas" --json` | projeto comunitário |
| Roo Code | Agente IA | recente | `sqlite-graphrag hybrid-search "contexto repo" --json` | projeto comunitário |
| Cline | Agente IA | extensão VS Code | `sqlite-graphrag list --limit 20 --json` | https://cline.bot |
| Continue | Agente IA | VS Code ou JetBrains | `sqlite-graphrag recall "docstring" --json` | https://docs.continue.dev |
| Factory | Agente IA | recente | `sqlite-graphrag recall "contexto pr" --json` | https://factory.ai |
| Augment Code | Agente IA | recente | `sqlite-graphrag hybrid-search "review" --json` | https://docs.augmentcode.com |
| JetBrains AI Assistant | Agente IA | 2024.2+ | `sqlite-graphrag recall "stacktrace" --json` | https://www.jetbrains.com/ai |
| OpenRouter | Roteador IA | qualquer | `sqlite-graphrag recall "regra" --json` | https://openrouter.ai/docs |
| Shells POSIX | Shell | qualquer | `sqlite-graphrag recall "$query" --json` | https://www.gnu.org/software/bash |
| Nushell | Shell | 0.90+ | `^sqlite-graphrag recall "query" --k 5 --json \| from json \| get results` | https://www.nushell.sh/book |
| GitHub Actions | CI/CD | qualquer | workflow YAML | https://docs.github.com/actions |
| GitLab CI | CI/CD | qualquer | `.gitlab-ci.yml` | https://docs.gitlab.com/ee/ci |
| CircleCI | CI/CD | qualquer | `.circleci/config.yml` | https://circleci.com/docs |
| Jenkins | CI/CD | 2.400+ | Jenkinsfile | https://www.jenkins.io/doc |
| Docker e Podman Alpine | Container | qualquer | Dockerfile | https://docs.docker.com |
| Kubernetes | Orquestrador | 1.25+ | Job ou CronJob | https://kubernetes.io/docs |
| Scoop e Chocolatey | Gerenciador Pacote | Windows | `scoop install sqlite-graphrag` (planejado) | https://scoop.sh e https://chocolatey.org |
| Nix e Flakes | Gerenciador Pacote | qualquer | `nix run .#sqlite-graphrag` | https://nixos.org |


## Claude Code
### Agente Anthropic — Integração Subprocess
- Receita pronta para copiar em `.claude/hooks/`, zero custo, memória permanece na sua máquina
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é persistir contexto entre sessões do Claude Code sem serviços externos de memória
- Use `sqlite-graphrag recall "$USER_PROMPT" --k 5 --json` em um hook pre-task para injetar contexto
- Versão mínima exige Claude Code 1.0 ou posterior para suporte estável ao diretório `.claude/hooks/`
- Docs oficiais em https://docs.anthropic.com/claude-code descrevendo o ciclo de vida dos hooks
- Dica de ouro é capturar exit code `75` como retry-later mantendo o agente vivo graciosamente
- Desde v1.0.61, `ingest --mode claude-code` usa o binário Claude Code para extração curada por LLM de entidades/relações durante ingestão em massa
- O modo de ingestão spawna `claude -p` headless por arquivo — requer Claude Code >= 2.1.0 com assinatura Pro/Max ativa
- Usar `--claude-timeout <S>` (padrão 300s) para prevenir subprocessos travados em pipelines CI/cron


## Codex CLI
### Agente OpenAI — Subprocess Dirigido Por AGENTS.md
- Receita pronta para colar no `AGENTS.md` da raiz do repo, zero custo para ativar
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é expor o contrato de memória via convenção nativa do `AGENTS.md` da própria OpenAI
- Use `sqlite-graphrag recall "<query>" --k 5 --json` documentado dentro do `AGENTS.md` na raiz do repo
- Versão mínima exige Codex CLI 0.5 ou posterior para regras determinísticas de parsing do AGENTS.md
- Docs oficiais em https://github.com/openai/codex cobrindo a ordem de descoberta do AGENTS.md
- Dica de ouro é incluir um exemplo de invocação funcional sob cada comando listado para Codex
- Desde v1.0.62, `ingest --mode codex` usa o binário Codex CLI para extração curada por LLM de entidades/relações durante ingestão em massa
- O modo de ingestão spawna `codex exec --json` headless por arquivo — requer Codex CLI >= 0.120.0 com sessão ChatGPT OAuth ativa (codex login)
- Usar `--codex-timeout <S>` (padrão 300s) para prevenir subprocessos travados em pipelines CI/cron

> **Autenticação:** OAuth é o ÚNICO fluxo de credencial aceito. Chaves de API são PROIBIDAS.
> `--mode claude-code` lê OAuth de `~/.claude/.credentials.json` (Claude Pro/Max/Team).
> `--mode codex` lê autenticação de dispositivo via `codex login` (OpenAI ChatGPT).
> Definir `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` no ambiente ABORTA o spawn com `AppError::Validation` e código de saída 1. A flag `--bare` (que também exigiria uma chave de API) foi REMOVIDA de todo caminho executável.
> Veja `docs/decisions/adr-0011-oauth-only-enforcement.md` para a justificativa completa.

## Gemini CLI
### Agente Google — Subprocess Com Contrato JSON
- Receita pronta para copiar na config do Gemini CLI, zero custo, roda completamente local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é injetar memória em prompts do Gemini 2.5 Pro durante sessões longas de código
- Use `sqlite-graphrag hybrid-search "query" --k 5 --json` para recall com intenção mista de keyword
- Versão mínima suporta qualquer release recente do Gemini CLI com invocação subprocess habilitada
- Docs oficiais em https://github.com/google-gemini/gemini-cli sobre padrões de integração de tool
- Dica de ouro é definir `SQLITE_GRAPHRAG_LANG=pt` ao prompt-ar Gemini em contextos em português


## Opencode
### Agente Comunitário — Integração Subprocess
- Receita pronta para copiar no hook plugin do Opencode, zero custo, roda como subprocess
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é persistir contexto multi-turno no loop open source de orquestração do Opencode
- Use `sqlite-graphrag recall "$query" --json` como parte do pipeline pre-generation do Opencode
- Versão mínima suporta qualquer release recente do Opencode expondo hook subprocess via plugin
- Projeto oficial em https://github.com/opencode-ai/opencode com issue tracker comunitário
- Dica de ouro é definir o namespace pelo slug do repo para evitar vazamento entre projetos


## OpenClaw
### Agente Comunitário — Driver Subprocess
- Receita pronta para adicionar no startup do OpenClaw, zero custo, memória é totalmente local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é injetar memória persistente em loops do agente OpenClaw sem rebuild de plugin
- Use `sqlite-graphrag list --type user --json` para buscar contexto inicial no começo de uma run
- Versão mínima suporta qualquer release recente do OpenClaw capaz de shell out para binários CLI
- Docs oficiais dentro do README GitHub do OpenClaw explicando regras de integração subprocess
- Dica de ouro é executar o binário dentro da pasta alvo e manter o default `graphrag.sqlite`


## Paperclip
### Agente Comunitário — Cliente Subprocess
- Receita pronta para colar na config de hook do Paperclip, zero custo, memória fica local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é persistir memória cross-session no agente autônomo de desenvolvimento Paperclip
- Use `sqlite-graphrag read --name onboarding-note --json` para semear a sessão com notas prévias
- Versão mínima suporta qualquer release recente do Paperclip que possa spawnar subprocess filho
- Docs oficiais no repositório comunitário do Paperclip descrevendo o contrato de hook subprocess
- Dica de ouro é rodar `health --json` no startup e abortar quando integridade reporta dano algum


## VS Code Copilot
### Agente Microsoft — Integração tasks.json
- Receita pronta para colar no tasks.json, zero custo, recall dispara de dentro do editor
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é expor memória relevante de uma seleção dentro dos painéis de chat do VS Code Copilot
- Use a entrada de exemplo em tasks.json que chama `sqlite-graphrag recall "$selection" --json`
- Versão mínima exige VS Code 1.90 ou posterior para as substituições mais recentes de tasks.json
- Docs oficiais em https://code.visualstudio.com/docs/copilot cobrindo registro de tool no chat
- Dica de ouro é mapear a task em `Cmd+Shift+M` para invocação de recall com uma única tecla


## Google Antigravity
### Agente Google — Integração Runner
- Receita pronta para registrar como runner Antigravity, zero custo, binário é autocontido
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é rodar sqlite-graphrag como runner de primeira classe em pipelines Antigravity em escala
- Use `sqlite-graphrag hybrid-search "$PROMPT" --json --k 10` como passo de retrieval em um runner
- Versão mínima suporta qualquer release recente do Antigravity que aceite runners binários arbitrários
- Docs oficiais na página do produto Google Antigravity descrevendo formato de config de runner
- Dica de ouro é rodar `sync-safe-copy` antes de cada pipeline para proteger o artefato compartilhado


## Windsurf
### Agente Codeium — Integração Terminal
- Receita pronta para colar em um binding Run task do Windsurf, zero custo para ativar recall
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é expor recall de memória para painéis assistentes do Windsurf via invocação de terminal
- Use `sqlite-graphrag recall "$EDITOR_CONTEXT" --json` mapeado para um binding Run task no Windsurf
- Versão mínima suporta qualquer release recente do Windsurf com execução de task de terminal ativa
- Docs oficiais em https://windsurf.com/docs descrevendo a sintaxe de binding de task de terminal
- Dica de ouro é persistir resultados em `/tmp/ng.json` para templates de prompt Windsurf lerem


## Cursor
### Agente Cursor — Integração Terminal
- Receita pronta para adicionar em `.cursorrules` ou binding de terminal, zero custo, memória é local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é parear Cursor AI com um backend de memória local que sobrevive restarts do editor
- Use `sqlite-graphrag remember --name cursor-ctx --type project --body "$SELECTION"` por atalho
- Versão mínima exige Cursor 0.40 ou posterior para regras AI estáveis e override de env de terminal
- Docs oficiais em https://cursor.com/docs cobrindo padrões de regras AI e integração de terminal
- Dica de ouro é definir `SQLITE_GRAPHRAG_NAMESPACE=${workspaceFolderBasename}` por workspace


## Zed
### Agente Zed Industries — Integração Assistant Panel
- Receita pronta para adicionar como task profile do Zed, zero custo, roda do terminal integrado
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é cablear recall de memória no painel assistente do Zed sem extensões customizadas
- Use `sqlite-graphrag recall "abas abertas" --json --k 5` como comando de terminal disponível ao Zed
- Versão mínima suporta qualquer release recente do Zed com painel assistente e tasks de terminal
- Docs oficiais em https://zed.dev/docs descrevendo painel assistente e integração de terminal
- Dica de ouro é definir um profile de task Zed compartilhando memória entre múltiplos workspaces


## Aider
### Agente Open Source — Integração Shell
- Receita pronta para colar no alias shell antes do `aider`, zero custo, zero servidor de config
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é aumentar pair programming do Aider com memória durável entre repositórios git
- Use `sqlite-graphrag recall "refactor target" --k 5 --json` invocado antes de cada prompt Aider
- Versão mínima exige Aider 0.60 ou posterior para invocação subprocess e hook estáveis e suportadas
- Docs oficiais em https://aider.chat descrevendo configuração e comandos shell customizados
- Dica de ouro é escopar memória por repositório via `SQLITE_GRAPHRAG_NAMESPACE=$(basename $(pwd))`


## Jules
### Agente Google Labs — Automação CI
- Receita pronta para adicionar como passo CI do Jules, zero custo, binário instala em segundos
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é rodar manutenção de memória dentro dos pipelines de automação preview do Jules
- Use `sqlite-graphrag stats --json` como passo CI para monitorar crescimento de memória semanal
- Versão mínima é a release preview corrente do Jules disponível via early access do Google Labs
- Docs oficiais em https://jules.google explicando configuração de job CI e autenticação necessária
- Dica de ouro é falhar o pipeline quando `stats.memories` excede o limite combinado para um projeto


## Kilo Code
### Agente Comunitário — Integração Subprocess
- Receita pronta para colar no hook de startup do Kilo Code, zero custo, memória é arquivo local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é expor camada de memória persistente ao agente autônomo de engenharia Kilo Code
- Use `sqlite-graphrag recall "tarefas recentes" --json` no começo de toda run do agente Kilo Code
- Versão mínima suporta qualquer release recente do Kilo Code capaz de spawnar processos filhos
- Docs oficiais no repositório comunitário do Kilo Code descrevendo o contrato de subprocess
- Dica de ouro é logar exit code `75` como retryable em vez de fatal quando orquestrador está ocupado


## Roo Code
### Agente Comunitário — Integração Subprocess
- Receita pronta para cablear no ciclo de hook do Roo Code, zero custo, dados em SQLite local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é injetar memória em prompts do agente Roo Code para entendimento profundo do repo
- Use `sqlite-graphrag hybrid-search "contexto repo" --json` para recall entre tipos mistos de query
- Versão mínima suporta qualquer release recente do Roo Code com capacidade de hook subprocess
- Docs oficiais no repositório comunitário do Roo Code explicando convenções de ciclo de hook
- Dica de ouro é encadear `related <name> --hops 2` após recall para expansão multi-hop no grafo


## Cline
### Extensão Comunitária VS Code — Integração Terminal
- Receita pronta para registrar como tool de terminal do Cline, zero custo, memória persiste local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é dar ao Cline memória persistente entre sessões VS Code sem serviços em cloud
- Use `sqlite-graphrag list --limit 20 --json` como passo inicial no startup da conversa do Cline
- Versão mínima suporta a release atual da extensão VS Code do Cline no marketplace
- Docs oficiais em https://cline.bot cobrindo registro de tool de terminal e padrões de uso
- Dica de ouro é mapear o comando como tool Cline com nome descritivo e explicação de uso


## Continue
### Agente Open Source — Integração Terminal IDE
- Receita pronta para colar nos custom commands do Continue, zero custo, sem servidor necessário
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é expor memória sqlite-graphrag nos painéis de chat Continue em VS Code ou JetBrains
- Use `sqlite-graphrag recall "docstring" --json` de um registro de custom command do Continue
- Versão mínima suporta qualquer release recente da extensão Continue em VS Code ou JetBrains
- Docs oficiais em https://docs.continue.dev descrevendo comandos customizados e integração de tool
- Dica de ouro é documentar cada comando no config do Continue para o LLM embutido detectar


## Factory
### Agente Factory — API Ou Subprocess
- Receita pronta para adicionar na config de tool do droid Factory, zero custo, binário autocontido
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é integrar sqlite-graphrag com droids autônomos de desenvolvimento Factory em produção
- Use `sqlite-graphrag recall "contexto pr" --json` durante preparação do plano do droid Factory
- Versão mínima suporta qualquer release recente do Factory com integração subprocess ou API
- Docs oficiais em https://factory.ai explicando configuração de tool do droid e execução do plano
- Dica de ouro é definir `--wait-lock` longo para droids Factory rodando sob concorrência pesada


## Augment Code
### Agente Augment — Integração IDE
- Receita pronta para cablear no registro de tool da IDE Augment, zero custo, roda como subprocess
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é alimentar agentes de review Augment Code com memória persistente entre repositórios
- Use `sqlite-graphrag hybrid-search "code review" --json` na preparação de review da IDE Augment
- Versão mínima suporta qualquer release recente do Augment Code com hooks de terminal e subprocess
- Docs oficiais em https://docs.augmentcode.com descrevendo registro de tool e agentes suportados
- Dica de ouro é ativar `--lang en` explicitamente para linguagem de review consistente entre times


## JetBrains AI Assistant
### Agente JetBrains — Integração IDE
- Receita pronta para registrar como external tool do JetBrains, zero custo, recall leva milissegundos
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é adicionar memória sqlite-graphrag ao JetBrains AI Assistant em IntelliJ PyCharm WebStorm
- Use `sqlite-graphrag recall "$SELECTION" --json` registrado como runner de external tool JetBrains
- Versão mínima exige JetBrains AI Assistant 2024.2 ou posterior para registro moderno de tool
- Docs oficiais em https://www.jetbrains.com/ai explicando registro de tool e external runner
- Dica de ouro é mapear o tool a um atalho de teclado para invocar recall com uma mão no teclado


## OpenRouter
### Roteador Multi-LLM — Qualquer Versão Suportada
- Receita pronta para adicionar como preâmbulo de qualquer pipeline OpenRouter, zero custo local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é compartilhar backend comum de memória entre todo LLM hospedado via OpenRouter
- Use `sqlite-graphrag recall "regra roteamento" --json` como preâmbulo antes de request roteado
- Versão mínima suporta qualquer release da API OpenRouter já que memória fica local e independente
- Docs oficiais em https://openrouter.ai/docs explicando regras de roteamento e integração da API
- Dica de ouro é reusar o mesmo namespace entre todos os modelos roteados para contexto coeso


### Backend de Embedding OpenRouter (v1.0.93)
- Desde v1.0.93, sqlite-graphrag pode usar OpenRouter como backend dedicado de embedding via REST API
- Use `--embedding-backend openrouter --embedding-model MODEL` para embedding em ~200ms em vez de 15s via subprocesso
- 10 modelos verificados: Qwen 4B/8B, NVIDIA Nemotron (gratuito), OpenAI small/large, Perplexity, Mistral, BAAI, Google Gemini
- Defina a chave de API via variável de ambiente `OPENROUTER_API_KEY` ou flag `--openrouter-api-key`

```bash
export OPENROUTER_API_KEY="sk-or-v1-sua-chave-aqui"
sqlite-graphrag --embedding-backend openrouter \
  --embedding-model "qwen/qwen3-embedding-8b" \
  remember --name teste --type note --description "teste" --body "conteúdo" --json
```

## Minimax (desde v1.0.83 — ADR-0041)
### Provider Anthropic-Compatível — MiniMax/api.minimax.io
- Receita pronta para rotear Claude Code através de qualquer endpoint Anthropic-compatível sem violar o mandato OAuth-only
- Embora a guarda OAuth-only continue rejeitando `ANTHROPIC_API_KEY` e `OPENAI_API_KEY` com exit 1 (defesa em profundidade desde v1.0.69), a nova whitelist preserva `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CODEX_ACCESS_TOKEN`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY` e `OTEL_EXPORTER_OTLP_ENDPOINT`
- Propósito é habilitar providers Anthropic-compatíveis (MiniMax/api.minimax.io, OpenRouter, rotas customizadas do AWS Bedrock, gateways corporativos) sem forçar operadores a pagar pela rota oficial de chave de API Anthropic
- Use as variáveis de ambiente abaixo antes de invocar qualquer comando `sqlite-graphrag` que dispara embedding (`remember`, `edit`, `ingest --mode claude-code`)
- Versão mínima requer `sqlite-graphrag` 1.0.83 ou posterior; releases anteriores vão spawnar o subprocesso sem as vars do provider customizado e o provider retornará `401 Invalid authentication credentials`
- Documentação oficial em https://platform.minimax.io/document e `docs/decisions/adr-0041-preserve-custom-provider-env.md` explica a justificativa arquitetural
- Dica de ouro é verificar a alcançabilidade do provider com `curl -fsS "$ANTHROPIC_BASE_URL/v1/models" -H "Authorization: Bearer $ANTHROPIC_AUTH_TOKEN"` antes de rodar qualquer comando `sqlite-graphrag`

### Bloco de Configuração
```bash
# Configure uma vez por sessão de shell antes de invocar sqlite-graphrag
export ANTHROPIC_AUTH_TOKEN="sk-cp-seu-token-do-provider"
export ANTHROPIC_BASE_URL="https://api.minimax.io/anthropic"
# Opcional: opt-out de encaminhamento de telemetria do subprocesso
export DISABLE_TELEMETRY="1"
# Opcional: roteia OpenTelemetry para collector local em vez do padrão do provider
export OTEL_EXPORTER_OTLP_ENDPOINT="http://localhost:4317"
```

### Smoke Test
```bash
# 1. Verifica que o provider retorna modelos para o token configurado
curl -fsS "$ANTHROPIC_BASE_URL/v1/models" \
  -H "Authorization: Bearer $ANTHROPIC_AUTH_TOKEN" \
  | head -c 200 && echo

# 2. Persiste uma memória de smoke test através do provider customizado
sqlite-graphrag remember \
  --name smoke-test-minimax-v183 \
  --type note \
  --description "validacao do provider customizado via v1.0.83" \
  --body "smoke test executado em $(date -u +%FT%TZ)" \
  --graph-stdin <<'EOF'
{
  "body": "smoke test executado em $(date -u +%FT%TZ)",
  "entities": [
    {"name": "minimax", "entity_type": "tool", "description": "Provider Anthropic-compatível"}
  ],
  "relationships": []
}
EOF

# 3. Confirma que o embedding aterrissou em memory_embeddings (não NULL)
sqlite-graphrag read --name smoke-test-minimax-v183 --json | jaq '{name, memory_id, has_embedding: (.body | length > 0)}'

# 4. Roda recall para verificar que o embedding participa da busca vetorial
sqlite-graphrag recall "validacao do provider customizado" --k 3 --json | jaq '.results[] | {name, score}'
```

### Troubleshooting 401 Invalid Authentication Credentials
- **Sintoma**: `sqlite-graphrag remember` retorna exit 11 com `claude exited with exit status: 1: stderr=` (ou equivalente `codex`)
- **Causa**: as env vars `ANTHROPIC_AUTH_TOKEN` ou `ANTHROPIC_BASE_URL` NÃO chegaram ao subprocesso (sqlite-graphrag antigo, modo estrito, ou wrapping de shell que remove env)
- **Caminhos de resolução**:
  - Confirme que `sqlite-graphrag --version` reporta `1.0.83` ou posterior
  - Confirme que as env vars estão exportadas no MESMO shell onde o comando roda (não shell pai, não `.envrc` consumido só pelo direnv)
  - Rode `env | rg "ANTHROPIC_(AUTH_TOKEN|BASE_URL)"` para confirmar presença
  - Se o host impõe isolamento de env vars, remova o override de modo estrito: `unset SQLITE_GRAPHRAG_STRICT_ENV_CLEAR` ou remova `--strict-env-clear`
  - Capture o erro exato com `RUST_LOG=trace sqlite-graphrag remember ... 2> trace.log` e procure por `apply_env_whitelist`
- **Confirmação de defesa em profundidade**: a guarda OAuth-only ainda rejeita `ANTHROPIC_API_KEY` se acidentalmente setada; verifique com `export ANTHROPIC_API_KEY=sk-ant-test && sqlite-graphrag remember --name test --body x` retornando exit 1
## Shells POSIX
### Bash Zsh Fish PowerShell — Qualquer Versão
- Receita pronta para colar em alias ou script de shell, zero custo, pipes funcionam imediatamente
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é compor sqlite-graphrag com pipelines clássicos Unix e Windows shell sem atrito
- Use `sqlite-graphrag recall "$query" --json | jaq '.hits[].name'` em qualquer shell POSIX
- Versão mínima suporta qualquer Bash Zsh Fish ou PowerShell 7 recente
- Docs oficiais em https://www.gnu.org/software/bash e homepages dos respectivos projetos shell
- Dica de ouro é colocar variáveis entre aspas para evitar word splitting em queries com espaços


## Nushell
### Nushell — Integração Pipeline de Dados Estruturados
- Receita pronta para colar em script Nushell, zero custo, saída vira tabela Nu nativa
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess via sigil `^` no Nu
- Propósito é compor saída do sqlite-graphrag com pipelines de dados estruturados do Nushell nativamente
- Use `^sqlite-graphrag recall "query" --k 5 --json | from json | get results` para consultar memória
- Versão mínima suporta Nushell 0.90 ou posterior para comando externo estável e pipeline `from json`
- Docs oficiais em https://www.nushell.sh/book descrevendo comandos externos e parsing de JSON
- Dica de ouro é encadear `| select name score` para exibir tabela de memória ranqueada no Nu


## GitHub Actions
### CI/CD — Qualquer Runner Recente
- Receita pronta para copiar em `.github/workflows/`, zero custo, roda em qualquer runner GitHub
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag instala em segundos via cargo em qualquer runner
- Propósito é rodar manutenção de memória e backups em workflows agendados do GitHub Actions
- Use workflow cron que executa `sqlite-graphrag purge --days 30 --yes` e `vacuum` agendados
- Versão mínima funciona em qualquer runner `ubuntu-latest` `macos-latest` ou `windows-latest`
- Docs oficiais em https://docs.github.com/actions descrevendo sintaxe de workflows agendados
- Dica de ouro é fazer upload do sync-safe-copy como artifact do build para capacidade de rollback


## GitLab CI
### CI/CD — Runner Recente
- Receita pronta para copiar em `.gitlab-ci.yml`, zero custo, roda em qualquer runner GitLab
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag instala em segundos via cargo em qualquer runner
- Propósito é rodar manutenção sqlite-graphrag em pipelines agendados do GitLab CI rotineiramente
- Use stage `.gitlab-ci.yml` agendado que invoca `cargo install --path .` primeiro
- Versão mínima suporta runner recente do GitLab com toolchain Rust disponível para instalação
- Docs oficiais em https://docs.gitlab.com/ee/ci descrevendo configuração de pipelines agendados
- Dica de ouro é cachear o diretório cargo install entre runs para startup de job mais rápido


## CircleCI
### CI/CD — Executor Recente
- Receita pronta para copiar na config CircleCI, zero custo, binário instala via cargo em segundos
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag instala em segundos via cargo em qualquer executor
- Propósito é rodar manutenção e backups sqlite-graphrag em workflows agendados do CircleCI
- Use workflow agendado com `cargo install --path .` seguido dos passos do job
- Versão mínima suporta executor Linux ou macOS recente do CircleCI com toolchain Rust
- Docs oficiais em https://circleci.com/docs descrevendo pipelines agendados e workflows suportados
- Dica de ouro é persistir o DB no workspace para jobs downstream auditarem o snapshot gerado


## Jenkins
### CI/CD — Jenkins 2.400+
- Receita pronta para colar em stage de Jenkinsfile, zero custo, funciona em ambientes air-gapped
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag instala via cargo e roda como subprocesso one-shot sem daemon algum (o daemon foi removido na v1.0.79)
- Propósito é integrar backups sqlite-graphrag em pipelines Jenkins self-hosted para ambientes regulados
- Use stage em Jenkinsfile rodando `cargo install --path .` e comandos operacionais
- Versão mínima exige Jenkins 2.400 ou posterior para pipeline declarative e gerência de agent estáveis
- Docs oficiais em https://www.jenkins.io/doc cobrindo sintaxe de pipeline declarative a fundo
- Dica de ouro é arquivar a saída do sync-safe-copy como artifact para retenção de longo prazo


## Docker e Podman Alpine
### Container — Qualquer Versão Recente
- Receita pronta para copiar em Dockerfile, zero custo, imagem final cabe em menos de 25 MB Alpine
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag é binário estático sem dependência de runtime
- Propósito é empacotar sqlite-graphrag em imagens Alpine mínimas para deploys reproduzíveis em produção
- Use Dockerfile multi-stage com stage builder Rust e runtime Alpine copiando o binário único
- Versão mínima suporta qualquer Docker ou Podman com sintaxe multi-stage compatível ativada
- Docs oficiais em https://docs.docker.com cobrindo multi-stage build e minimização de imagem
- Dica de ouro é montar o arquivo SQLite como named volume para persistir memória entre restarts


## Kubernetes Jobs E CronJobs
### Kubernetes — 1.25+
- Receita pronta para copiar em manifesto CronJob, zero custo, roda no seu cluster existente
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como Job one-shot sem sidecar necessário
- Propósito é rodar manutenção sqlite-graphrag como Kubernetes CronJobs em clusters gerenciados
- Use manifesto CronJob referenciando a imagem Alpine e invocando purge mais vacuum agendados
- Versão mínima exige Kubernetes 1.25 ou posterior para Job CronJob e concurrency policy estáveis
- Docs oficiais em https://kubernetes.io/docs descrevendo Job CronJob e PersistentVolumeClaim
- Dica de ouro é montar o DB de um PVC com access mode `ReadWriteOnce` para segurança de dados


## Scoop E Chocolatey
### Gerenciador Pacote — Windows
- Receita pronta para executar assim que o manifesto entrar, zero custo, instala o mesmo binário do cargo
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag é único exe sem dependência de runtime
- Propósito é instalar sqlite-graphrag no Windows com Scoop ou Chocolatey familiares aos devs Windows
- Use `scoop install sqlite-graphrag` ou `choco install sqlite-graphrag` assim que manifestos oficiais saiam
- Versão mínima suporta Scoop 0.3 ou Chocolatey 2.0 com recursos modernos de manifesto ativos
- Docs oficiais em https://scoop.sh e https://chocolatey.org explicando convenções de manifesto
- Dica de ouro é executar o binário dentro da pasta do projeto para criar `graphrag.sqlite` ali


## Nix E Flakes
### Gerenciador Pacote — Qualquer Versão Nix
- Receita pronta para adicionar como flake input, zero custo, hash do binário fixado para reprodutibilidade
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como binário puro em qualquer dev shell Nix
- Propósito é instalar sqlite-graphrag em ambientes Nix reproduzíveis incluindo NixOS e dev shells
- Use `nix run github:daniloaguiarbr/sqlite-graphrag#sqlite-graphrag` para executar sem instalação prévia
- Versão mínima exige Nix 2.4 ou posterior com feature Flakes habilitada na config do usuário
- Docs oficiais em https://nixos.org descrevendo ativação de Flakes e uso via linha de comando
- Dica de ouro é fixar o hash de input do flake para o binário permanecer reproduzível em rebuilds
