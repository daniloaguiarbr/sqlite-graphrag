---
name: sqlite-graphrag
description: Use esta skill SEMPRE que o usuário perguntar sobre adicionar memória persistente, GraphRAG ou contexto de longo prazo ao Claude Code, Codex, Cursor, Windsurf ou qualquer agente de código. DEVE acionar para queries mencionando lembrar disso, salvar conversa, recuperar contexto anterior, busca híbrida, grafo de entidades, memória SQLite, RAG local, embedding LLM-only, fluxo OAuth, embedding BLOB-backed, migrate to-llm-only, migrate rehash, drop de vec tables, dimensionalidade de embedding, embedding-dim, llm-parallelism, embedding em lote, lote adaptativo, re-embed, force-reembed, ou remediação de gaps G28-G44. Auto-invoca sem menção explícita quando o usuário descreve agente perdendo contexto entre sessões ou quer memória local offline-first em Rust. DEVE também acionar em enforcement OAuth-only, aborto de ANTHROPIC_API_KEY ou OPENAI_API_KEY, flags de endurecimento para Claude e Codex, Mock LLM CLI em CI, ou remoção do daemon. Keywords memória RAG GraphRAG SQLite OAuth grafo entidade v1.0.79.
---


## Princípios Fundamentais

- Leia este documento em [inglês (EN)](../sqlite-graphrag-en/SKILL.md).
### OBRIGATÓRIO — Filosofia de Uso
- TRATAR sqlite-graphrag como camada local de memória persistente
- INVOCAR sempre como subprocesso via `std::process::Command`
- LER stdout para dados estruturados em JSON ou NDJSON
- LER stderr para logs de tracing e mensagens humanas
- VERIFICAR exit code antes de parsear stdout
- PRESERVAR contexto entre sessões via arquivo SQLite único
- DELEGAR memória de longo prazo ao binário sem reimplementar
### PROIBIDO — Anti-padrões
- NUNCA expor o binário como servidor MCP ou serviço HTTP
- NUNCA depender de vector DB cloud como Pinecone ou Weaviate
- NUNCA escrever direto no SQLite paralelo ao binário
- NUNCA editar o arquivo `.sqlite` com outra ferramenta
- NUNCA assumir saída sem validar exit code antes
- NUNCA confundir `distance` com `combined_score` no ranking
- NUNCA misturar stdout estruturado com logs humanos
- NUNCA usar `fd | xargs remember` quando `ingest` cobre o caso


## Inicialização e Verificação de Saúde
### OBRIGATÓRIO — Bootstrap do Banco
- EXECUTAR `sqlite-graphrag init --namespace <projeto>` no primeiro uso
- DESDE v1.0.76, `init` valida que uma CLI LLM (`claude` ou `codex`) é alcançável no PATH; não há download de modelo local
- VALIDAR com `sqlite-graphrag health --json` antes de operar
- TRATAR exit code 10 como erro de database ou banco corrompido
- TRATAR exit code 15 como lock pendente, ampliar `--wait-lock`
- ABORTAR pipeline quando `integrity_ok` retornar `false`
- RODAR `migrate --json` após cada upgrade do binário
### OBRIGATÓRIO — Verificação Contínua
- INSPECIONAR `wal_size_mb` no `health` para detectar fragmentação
- CONFERIR `journal_mode` igual a `wal` em produção
- RODAR `optimize --json` para refrescar estatísticas do planner; resposta inclui `fts_rebuilt` (bool) indicando se o índice FTS5 também foi reconstruído
- USAR `optimize --skip-fts --json` para pular a etapa de reconstrução do FTS5 (mais rápido, usar quando FTS5 foi reconstruído recentemente)
- DETECTAR deriva de schema via `debug-schema` em troubleshooting
### Padrão Correto — Sequência de Bootstrap
- `sqlite-graphrag init --namespace meu-projeto`
- `sqlite-graphrag health --json | jaq '.integrity_ok'`
- `sqlite-graphrag migrate --json`
- `sqlite-graphrag stats --json | jaq '.memories'`


## Configuração Global
### OBRIGATÓRIO — Caminho do Banco
- USAR `--db <PATH>` quando o banco não está no diretório atual
- DEFINIR `SQLITE_GRAPHRAG_DB_PATH` para configuração persistente
- LEMBRAR que `--db` tem precedência sobre a variável de ambiente
- PADRÃO é `graphrag.sqlite` no diretório atual de invocação
### OBRIGATÓRIO — Namespace
- DEFINIR namespace via `--namespace` ou `SQLITE_GRAPHRAG_NAMESPACE`
- VALIDAR resolução com `namespace-detect --json`
- USAR `global` como namespace padrão quando ausente
- ISOLAR projetos via namespace por repositório
- ADOTAR `swarm-<agent_id>` para enxames multi-agente
- NOTAR que `SQLITE_GRAPHRAG_NAMESPACE` agora é respeitado por todos os comandos (corrigido na v1.0.51; anteriormente 8 comandos ignoravam a variável)
### OBRIGATÓRIO — Idioma da Saída
- USAR `--lang en` ou `--lang pt` para forçar idioma
- DEFINIR `SQLITE_GRAPHRAG_LANG=pt` para override de sessão
- LEMBRAR que `--lang` afeta apenas stderr humano
- STDOUT JSON permanece determinístico independente do idioma
### OBRIGATÓRIO — Fuso Horário de Exibição
- APLICAR `--tz America/Sao_Paulo` em saídas localizadas
- USAR `SQLITE_GRAPHRAG_DISPLAY_TZ=<IANA>` para persistir
- AFETA apenas campos `*_iso` no JSON
- CAMPOS epoch inteiros permanecem em UTC
- ABORTAR quando nome IANA inválido retorna exit 2 (parsing de argumentos Clap)
### OBRIGATÓRIO — Formato de Logs
- ATIVAR `SQLITE_GRAPHRAG_LOG_FORMAT=json` para agregadores
- PADRÃO `pretty` serve apenas para humanos no terminal
- ELEVAR detalhe via `SQLITE_GRAPHRAG_LOG_LEVEL=debug` em diagnóstico
- USAR `-v`, `-vv`, `-vvv` para info, debug e trace nos subcomandos
### OBRIGATÓRIO — Controle de Memória RAM Global
- ATIVAR `SQLITE_GRAPHRAG_LOW_MEMORY=1` em containers restritos
- APLICAR em hosts com menos de 4 GB de RAM disponível
- HONRA cgroup constraints automaticamente quando definido
- TRADE-OFF é 3 a 4 vezes mais tempo de wall clock
- COMBINAR com flag `--low-memory` em `ingest` específico
### NOTA — ONNX Runtime Não Mais Necessário (v1.0.76)
- O runtime ONNX (`libonnxruntime.so`) e `ORT_DYLIB_PATH` NÃO são mais necessários no build padrão LLM-only
- Embeddings são gerados via subprocesso headless `claude -p` ou `codex exec` (OAuth)
- Nenhum download de modelo local ou runtime ONNX é necessário para o build padrão


## CRUD — Create com remember
### OBRIGATÓRIO — Escrita de Memórias Individuais
- USAR nome kebab-case único por memória
- DECLARAR `--type` entre `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`; `--type` e `--description` são OPCIONAIS quando `--force-merge` é usado (herdados da memória existente)
- PREFERIR `--body-stdin` para corpos longos
- USAR `--body-file <PATH>` para evitar escape shell em Markdown
- PASSAR `--force-merge` em loops idempotentes; também restaura memórias soft-deleted e atualiza em um passo (desde v1.0.51)
- USAR `--dry-run` para validar inputs sem persistir ou rodar embeddings
- USAR `--clear-body` para limpar explicitamente o corpo de uma memória existente ao usar `--force-merge`; sem `--clear-body`, `--force-merge` com body vazio PRESERVA o corpo existente
- NER desabilitado por padrão; passar `--enable-ner` ou definir `SQLITE_GRAPHRAG_ENABLE_NER=1` para ativar extração automática — SOMENTE URL-regex desde a v1.0.79 (o pipeline GLiNER foi removido)
- Campo `extraction_method` na resposta reporta: `url-regex` ou `none:extraction-failed` (os valores `gliner-<variant>+regex` e `regex-only` são HISTÓRICOS, ≤ v1.0.75)
- `--skip-extraction` está obsoleto desde v1.0.45 e não tem efeito; usar `--enable-ner` para ativar NER
- RESPEITAR limite de 512000 bytes e 512 chunks por body
- USAR `--max-rss-mb <MiB>` para abortar embedding se o RSS do processo ultrapassar o threshold (padrão 8192 MiB); reduzir em ambientes com memória restrita
### OBRIGATÓRIO — Anexar Grafo no remember
- USAR `--entities-file` com array JSON tipado
- USAR `--relationships-file` para arestas tipadas
- INCLUIR campo `entity_type` em cada objeto de entidade
- ACEITAR `type` como sinônimo, nunca os dois juntos
- USAR `strength` entre `0.0` e `1.0` em relationships
- MAPEAR `from`/`to` como aliases de `source`/`target`
- USAR `--graph-stdin` para JSON único com `body`, `entities` e `relationships`
### PROIBIDO — Erros de Escrita
- NUNCA enviar `entity_type` e `type` no mesmo objeto JSON
- NUNCA usar `strength` fora do intervalo `[0.0, 1.0]`
- NUNCA duplicar nome sem `--force-merge` explícito
- NUNCA misturar `--body`, `--body-file`, `--body-stdin`, `--graph-stdin`
- NUNCA depender de `--enable-ner` para extração semântica de entidades (somente URL-regex desde a v1.0.79); usar `--graph-stdin` com entidades curadas por LLM ou `ingest --mode claude-code|codex`
- NUNCA exceder o cap de relações por memória sem ajustar env
- NUNCA usar `remember` em loop quando `ingest` cobre o caso
- NUNCA passar body vazio sem entidades via `--graph-stdin`; desde v1.0.54 retorna exit 1 (Validation) em vez de criar silenciosamente uma memória inerte com zero chunks
### Padrão Correto — Exemplos de remember
- `sqlite-graphrag remember --name design-auth --type decision --description "auth JWT" --body-stdin < doc.md`
- `sqlite-graphrag remember --name doc-readme --type document --description "import" --body-file README.md --force-merge`
- `sqlite-graphrag remember --name spec-x --type reference --description "spec" --body "..." --entities-file ents.json --relationships-file rels.json`
### Valores Válidos de --type
- `user`, `feedback`, `project`, `reference`
- `decision`, `incident`, `skill`, `document`, `note`


## CRUD — Criação em Lote com remember-batch (v1.0.67)
### OBRIGATÓRIO — Criação de Memórias em Lote via NDJSON
- USAR `remember-batch` para criar múltiplas memórias em uma única invocação via NDJSON no stdin
- CADA linha de entrada é um objeto JSON com campos `name`, `type`, `description`, `body`
- SAÍDA é NDJSON: um evento por item mais uma linha de resumo
- USAR `--force-merge` para atualizar memórias existentes no lote
- USAR `--dry-run` para validar o lote sem persistir
- PREFERIR sobre loop de `remember` para 10+ memórias — reduz overhead de carregamento repetido do modelo
- Evento por item: `name`, `status` (`"created"`/`"updated"`/`"skipped"`/`"failed"`), `memory_id?`, `error?`, `elapsed_ms`
- Linha de resumo: `summary` (true), `total`, `created`, `updated`, `skipped`, `failed`, `elapsed_ms`
### Padrão Correto — Exemplos de remember-batch
- `echo '{"name":"a","type":"note","description":"x","body":"hello"}' | sqlite-graphrag remember-batch --json`
- `cat batch.ndjson | sqlite-graphrag remember-batch --force-merge --json`


## Novidades na v1.0.68
### OBRIGATÓRIO — Governança de Ciclo de Vida de Processos (G28-B)
- SABER que `enrich`, `ingest --mode claude-code` e `ingest --mode codex` adquirem um singleton por namespace via `lock::acquire_job_singleton(job_type, namespace, wait_seconds)` antes de qualquer trabalho
- TRATAR `AppError::JobSingletonLocked { job_type, namespace }` (exit 75, retryable) como sinal de que outra invocação está em andamento no mesmo banco
- NÃO paralelizar esses comandos no mesmo namespace — use a queue DB com `--resume` ou sequencie-os
- SABER que o design anterior (semáforo compartilhado com todos os comandos CLI) permitia 4 invocações paralelas de `enrich` × 2 workers × 10 servidores MCP = ~192 processos, que é a causa raiz do incidente de load average 276 em 2026-06-03
### OBRIGATÓRIO — Isolamento MCP via env var (G28-A)
- DEFINIR `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR=/caminho/para/dir/vazio` para suprimir servidores MCP do escopo user em subprocessos `claude -p`
- SABER que o diretório vazio DEVE existir mas não conter arquivos; a CLI define `CLAUDE_CONFIG_DIR=<esse dir>` no subprocesso
- SABER que o dir vazio é o ÚNICO mecanismo que o upstream do Claude Code realmente honra — [anthropics/claude-code#10787] documenta que `--strict-mcp-config` e `--mcp-config '{}'` são silenciosamente ignorados
- ESPERAR um `tracing::warn!` quando `--llm-parallelism > 4`, recomendando a combinação com o override `CLAUDE_CONFIG_DIR`
### OBRIGATÓRIO — Helper de Circuit Breaker (G28-D)
- USAR `retry::CircuitBreaker::new(threshold, cooldown)` para limitar loops de retry em falhas persistentes em código customizado
- SABER que `AttemptOutcome::Transient` (de `AppError::RateLimited` ou `AppError::Timeout`) NÃO conta para o threshold de falhas
- SABER que `AttemptOutcome::HardFailure` (de `AppError::Validation` ou `AppError::Conflict`) conta; após `threshold` hits consecutivos, `record()` retorna `true` e o caller deve abortar
- CHAMAR `cb.reset()` ao iniciar um novo job para limpar o contador de falhas consecutivas
### OBRIGATÓRIO — Type Safety do HANDLE no Windows (G29)
- SABER que v1.0.68 é o primeiro release desde v1.0.65 que compila no Windows via `cargo install`
- SABER que `windows-sys >= 0.59` define `HANDLE` como `*mut c_void` (era `isize` em 0.48/0.52); `Cargo.toml:111` fixa `=0.59.0` exato
- ESPERAR que o job de CI `windows-build-check` rode `cargo check --target x86_64-pc-windows-msvc --lib --all-features` em todo push
- SE um usuário relatar falha de compilação no Windows, redirecione para atualizar para v1.0.68 ou aplicar o patch manual documentado em `docs/CROSS_PLATFORM.pt-BR.md`
### OBRIGATÓRIO — Correções de Testes (Vazamento de Timezone)
- SABER que 3 falhas de teste pré-existentes em `src/commands/{history,list,read}.rs` foram corrigidas na v1.0.68
- SABER que os testes anteriormente vazavam a env var `SQLITE_GRAPHRAG_DISPLAY_TZ` entre threads de teste paralelos e afirmavam strings hardcoded `1970-01-01T00:00:00`
- ESPERAR que os testes agora parseiem a string ISO via `chrono::DateTime::parse_from_rfc3339` e comparem `timestamp()` contra `DateTime::UNIX_EPOCH` para asserções timezone-agnostic
- CONFIAR que `cargo test --lib` está verde em todos os fusos horários (`UTC`, `America/Sao_Paulo`, `Europe/Berlin`, etc.) desde a v1.0.68
### PROIBIDO — Anti-padrões de Ciclo de Vida de Processos (G28)
- NUNCA rodar múltiplas invocações de `enrich` no mesmo banco simultaneamente — elas saturam o host
- NUNCA passar `--strict-mcp-config` ou `--mcp-config '{}'` para a CLI do Claude Code — ela ignora ambas (issue #10787)
- NUNCA burlar o singleton via manipulação direta de arquivos `~/.local/share/sqlite-graphrag/job-singleton-*.lock`
- NUNCA assumir que `enrich` rodando por 30 minutos significa que travou — enriquecimentos longos são normais


## Novidades na v1.0.69
### OBRIGATÓRIO — OAuth-Only Enforcement (mudança COMPORTAMENTAL crítica)
- SABER que v1.0.69 é o primeiro release onde OAuth é o ÚNICO fluxo de credencial aceito
- SABER que `claude_runner::build_claude_command` SEMPRE passa 7 flags de endurecimento: `--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions --output-schema` mais 2 de `codex_spawn::build_codex_command` (G28-A, G31)
- SABER que o spawn ABORTA com `AppError::Validation` (exit 1) se `ANTHROPIC_API_KEY` estiver definida no ambiente
- SABER que o spawn ABORTA com `AppError::Validation` (exit 1) se `OPENAI_API_KEY` estiver definida no ambiente
- SABER que a flag `--bare` (que exigiria uma chave de API) foi REMOVIDA de todo caminho executável; ela aparece apenas em documentação explicando por que é proibida
- SABER que `ANTHROPIC_API_KEY` e `OPENAI_API_KEY` estão EXCLUÍDAS do whitelist de env-clear (defesa em profundidade)
- SABER que 4 testes `#[serial_test::serial(env)]` em `claude_runner.rs` e 4 em `codex_spawn.rs` validam o conjunto canônico de flags e o comportamento de aborto
- REFERENCIAR `docs/decisions/adr-0011-oauth-only-enforcement.md` para a justificativa completa
- OPERADORES que usam chaves de API DEVEM migrar para OAuth (Claude Pro/Max ou OpenAI ChatGPT Pro) antes de atualizar
### OBRIGATÓRIO — Reaper de Orfãos (G28-C)
- SABER que `src/reaper.rs::scan_and_kill_orphans()` varre `/proc` no startup ANTES de qualquer trabalho
- SABER que o reaper mata qualquer orfão `claude` ou `codex` com `PPID=1` e idade > 60 segundos
- SABER que `ORPHAN_MIN_AGE_SECS=60` e `ORPHAN_SCAN_TARGETS=["claude", "codex"]` são as constantes
- CONFIAR que a suite de 4 testes do reaper roda em <30s no Linux (`orphan_min_age_is_one_minute`, `orphan_targets_include_claude_and_codex`, `reaper_report_starts_zeroed`, `scan_completes_without_panic_on_linux`)
- O reaper é chamado do startup de `main.rs`, ANTES do CLI despachar para qualquer subcomando
### OBRIGATÓRIO — Carga do Sistema e Circuit Breaker (G28-D)
- SABER que `src/system_load.rs` expõe `load_average_one()`, `ncpus()` e `is_system_saturated(threshold)`
- SABER que `is_system_saturated` usa threshold padrão `2.0 × ncpus`
- USAR `load_average_one()` para decidir se enfileira um novo enrich ou espera — a carga é cacheada via Mutex com throttle de 1s para evitar martelar `/proc/loadavg`
- SABER que `retry::CircuitBreaker::new(threshold, cooldown)` limita loops de retry em falhas persistentes
- SABER que `AttemptOutcome::Transient` (rate limit, timeout) NÃO conta para o threshold de falhas
- SABER que `AttemptOutcome::HardFailure` (validação, conflito) conta; após `threshold` hits consecutivos, `record()` retorna `true` e o caller aborta
- CHAMAR `cb.reset()` ao iniciar um novo job para limpar o contador de falhas consecutivas
### OBRIGATÓRIO — Enum MemorySource e Validação de Source (G29)
- SABER que `src/memory_source.rs` define um enum type-safe com 5 valores: `agent`, `user`, `system`, `import`, `sync`
- SABER que `MemorySource::TryFrom(&str)` retorna `AppError::Validation` listando os valores aceitos
- SABER que `validate_source()` é o guard de runtime chamado em `storage/memories.rs::insert` e `update`
- SABER que 8 testes unitários cobrem caminhos válido/inválido/vazio/display/serialização
- REFERENCIAR `docs/decisions/adr-0012-memory-source-enum.md` para o plano de migração
### OBRIGATÓRIO — Portão de Preservação e Idempotência (G29)
- SABER que `src/preservation.rs` define `jaccard_similarity(a: &str, b: &str) -> f64` (baseado em trigrama, UTF-8 safe via `char_indices`)
- SABER que `PreservationVerdict` enum tem variantes `Preserved { score, threshold }`, `Rejected { score, threshold }` e `Unchanged { byte_len }`
- SABER que o threshold padrão de preservação é `0.7` e é aplicado em todo `enrich --operation body-enrich`
- SABER que o skip de idempotência baseado em blake3 compara os hashes do body antigo e novo ANTES da verificação Jaccard
- SABER que 10 testes unitários cobrem casos de borda do Jaccard (vazio, um char, idêntico, fronteira de threshold, Unicode)
- REFERENCIAR `docs/decisions/adr-0015-preservation-gate.md`
### OBRIGATÓRIO — Deprecação de Scripts (G29 Passo 6)
- SABER que o diretório `scripts/legacy/` contém o workaround Python deprecado `expand-curtas.py` mais um README.md explicando por que foi retirado
- SABER que `scripts/legacy/` foi adicionado ao `.gitignore` para impedir o CI de re-executá-lo
- USAR `enrich --operation body-enrich` diretamente no lugar do wrapper Python
### OBRIGATÓRIO — Singleton com Escopo por db_hash (G30)
- SABER que a assinatura de `lock::acquire_job_singleton` ganhou parâmetros `db_path: &Path` e `force: bool`
- SABER que o nome do arquivo de lock agora é `job-singleton-{tag}-{namespace_slug}-{db_hash}.lock`
- SABER que o `db_hash` é formado pelos primeiros 12 caracteres hex de `blake3(canonicalize(db_path))`
- SABER que `lock::db_path_hash` é `pub` para que callers possam computar o hash sem adquirir o lock
- USAR as novas flags `--wait-job-singleton <SECONDS>` (poll pelo lock) e `--force-job-singleton` (quebra lock stale)
- Duas invocações concorrentes de `enrich` em bancos DIFERENTES não colidem mais; o mesmo banco ainda serializa
- A mensagem de erro que referenciava uma flag inexistente `--wait-job-singleton` agora é acionável
- REFERENCIAR `docs/decisions/adr-0013-singleton-scoped-by-db-hash.md`
### OBRIGATÓRIO — Helper codex_spawn Unificado (G31+G32+G33)
- SABER que `src/commands/codex_spawn.rs` (~700 linhas, 11 testes) unifica o pipeline de spawn, parser JSONL e validação de modelo ChatGPT Pro OAuth
- SABER que TANTO `enrich --mode codex` QUANTO `ingest --mode codex` consomem o mesmo comando canônico (eram divergentes, motivaram o wrapper `~/.local/bin/codex-clean`)
- SABER que as 7 flags de endurecimento são: `--json --output-schema --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules` MAIS `-c mcp_servers='{}' --ask-for-approval never`
- SABER que `parse_codex_jsonl` itera `for line in stdout.lines()` e escolhe o último `item.completed` do tipo `agent_message`
- SABER que `validate_codex_model` verifica `--codex-model` contra a whitelist do ChatGPT Pro OAuth ANTES do subprocesso ser spawnado
- ACEITAR apenas estes 5 modelos: `codex-auto-review`, `gpt-5.3-codex-spark`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.5`
- PADRÃO de `--codex-model` é `gpt-5.5`
- REFERENCIAR `docs/decisions/adr-0014-codex-spawn-helper.md`
### OBRIGATÓRIO — Aviso Condicional de LLM Parallelism (G34)
- SABER que o aviso de `llm_parallelism > 4` agora é condicional ao modo de spawn
- Modo Claude avisa em 5 (severidade alta)
- Codex 5..16 é silencioso (Codex não spawna filhos MCP)
- Codex avisa em 17 (severidade média)
- VALIDADO em 1161 itens, 0 falhas em produção
### OBRIGATÓRIO — Preflight Check e Modo de Fallback (G35)
- USAR `--preflight-check` em `enrich` para emitir um ping de 1 turn antes de escanear N candidatos
- USAR `--fallback-mode <codex|claude-code>` para trocar de modo automaticamente em rate limit
- USAR `--rate-limit-buffer <SECONDS>` para reservar orçamento para shutdown gracioso
- PADRÃO desligado para manter `--dry-run` e fluxos de CI com custo zero
- Em rate limit do Claude o preflight ABORTA com erro claro OU troca para `--fallback-mode`
### OBRIGATÓRIO — Enriquecimento Seletivo (G37)
- USAR `--names <NOME>` (repetível) em `enrich` para selecionar um subconjunto específico de nomes de memória
- USAR `--names-file <CAMINHO>` em `enrich` para ler nomes de um arquivo (aceita comentários `#` e linhas em branco)
- COMBINAR `--names` e `--names-file` como união quando ambos estão set
- SABER que `scan_unbound_memories(conn, namespace, limit, name_filter: &[String])` usa `WHERE m.name IN (?2, ?3, ...)` para query parametrizada segura
### OBRIGATÓRIO — Flags de Endurecimento FTS5 (G36)
- USAR `optimize --fts-dry-run` para pré-visualizar o que o rebuild do FTS5 faria
- USAR `optimize --fts-progress <N>` para imprimir progresso a cada N segundos
- USAR `optimize --yes` para pular a confirmação interativa
- SABER que `optimize` agora pré-verifica com `fts check` e PULA o rebuild quando o índice passa o integrity-check
- USAR `optimize --no-fts-skip-when-functional` para forçar rebuild mesmo quando o FTS5 está saudável
- SABER que `OptimizeResponse` expõe `fts_rebuilt`, `fts_skipped_functional`, `fts_unhealthy`, `fts_rows_indexed`
- SABER que a thread de progresso do FTS5 usa `crate::storage::connection::open_ro(&db_path)` em uma thread SEPARADA (rusqlite::Connection não é Send)
- REFERENCIAR `docs/decisions/adr-0016-fts5-hardening-flags.md`
### OBRIGATÓRIO — Backup 25x Mais Rápido (G38)
- SABER que os novos defaults são `run_to_completion(1000, Duration::from_millis(5), None)` — 25x mais rápido que os antigos 100/50ms
- USAR `--backup-step-size <N>` para ajustar o número de páginas por step
- USAR `--backup-step-sleep-ms <N>` para ajustar o sleep entre steps
- USAR `--backup-no-sleep` para desabilitar o sleep entre steps inteiramente (use com cautela em SSDs)
- SABER que `BackupResponse` adiciona os campos `pages_copied` e `step_size`
- SABER que o loop é MANUAL porque `Backup::step()` retorna `StepResult` que é `#[non_exhaustive]`
### OBRIGATÓRIO — Família de Subcomandos vec (G39)
- USAR `vec orphan-list --json` para listar todos os vetores de memória órfãos (sem linha de memória correspondente)
- USAR `vec purge-orphan --yes --dry-run` para PRÉ-VISUALIZAR a purga sem remover
- USAR `vec purge-orphan --yes` para PURGAR PERMANENTEMENTE os órfãos das 3 tabelas vec (`vec_memories`, `vec_entities`, `vec_chunks`)
- USAR `vec stats --json` para inspecionar a saúde das tabelas vec (contagem de linhas por tabela, ratio de órfãos, timestamp do último vacuum)
- SABER que `forget` agora chama `delete_vec` ANTES de `soft_delete` para prevenir a criação de novos órfãos vec
- SABER que a suite de 3 testes cobre orphan-list, purge-orphan e stats (todos usam SQLite em memória para isolamento)
- REFERENCIAR `docs/decisions/adr-0017-vec-orphan-handling.md`
### OBRIGATÓRIO — 4 Novos Schemas JSON (v1.0.69)
- SABER que 4 novos schemas foram adicionados em `docs/schemas/`:
  - `vec-orphan-list.schema.json` — lista de vetores de memória órfãos
  - `vec-purge-orphan.schema.json` — resposta da purga
  - `vec-stats.schema.json` — estatísticas de saúde das tabelas vec
  - `codex-models.schema.json` — resposta da whitelist de modelos ChatGPT Pro OAuth
- TODOS seguem a convenção do projeto `"additionalProperties": false`
- INDEXADOS em `docs/schemas/README.md` (que tem sua própria entrada v1.0.69 apontando para G33 + G39)
### OBRIGATÓRIO — 8 Novos ADRs (v1.0.69)
- SABER que 8 novos Architecture Decision Records vivem em `docs/decisions/`:
  - `adr-0011-oauth-only-enforcement.md` — justificativa completa para o mandato OAuth-only
  - `adr-0012-memory-source-enum.md` — plano de migração do enum type-safe
  - `adr-0013-singleton-scoped-by-db-hash.md` — hashing BLAKE3 do caminho do banco
  - `adr-0014-codex-spawn-helper.md` — refatoração DRY do pipeline de spawn do codex
  - `adr-0015-preservation-gate.md` — preservação Jaccard + idempotência blake3
  - `adr-0016-fts5-hardening-flags.md` — flags dry-run, progress e separação de thread do FTS5
  - `adr-0017-vec-orphan-handling.md` — família de subcomandos vec + hook em forget
  - `adr-0018-v1-0-69-status.md` — status executivo do fechamento de gaps
### OBRIGATÓRIO — Crescimento da Suite de Testes
- SABER que v1.0.69 adiciona 53 testes à suite (692 → 745)
- SABER que 0 testes falham e 3 são ignorados
- SABER que 8 ADRs documentam as decisões arquiteturais por trás dos 53 novos testes
- SABER que 4 dos novos testes são `#[serial_test::serial(env)]` para validar o enforcement de env var OAuth-only
### PROIBIDO — Anti-padrões v1.0.69
- NUNCA passar `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` no ambiente — o spawn ABORTARÁ
- NUNCA usar a flag `--bare` — ela foi REMOVIDA de todo caminho executável
- NUNCA passar `gpt-4*`, `o4-mini` ou `gpt-5-codex` como `--codex-model` — são rejeitados pelo ChatGPT Pro OAuth
- NUNCA rodar `enrich` em paralelo contra o mesmo banco mesmo com o novo singleton — espere pelo singleton ou use `--wait-job-singleton`
- NUNCA chamar `reaper::scan_and_kill_orphans()` de um processo filho — apenas do processo principal no startup
- NUNCA passar `--llm-parallelism > 4` para modo Claude sem combinar com `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR`
- NUNCA chamar `optimize` sem verificar `fts stats` antes se você só quer verificar saúde (use `fts check` no lugar)


## Novidades na v1.0.79
### OBRIGATÓRIO — G42: Pipeline de Embedding LLM Rápido, Paralelo e em Lote
- SABER que a dimensionalidade default de embedding caiu de 384 para 64 (MRL, arXiv 2205.13147); precedência: env `SQLITE_GRAPHRAG_EMBEDDING_DIM` (faixa [8, 4096]) > `schema_meta.dim` do banco aberto > 64
- SABER que bancos pré-existentes mantêm a dimensionalidade registrada sem mudança em TODO comando — ZERO alteração de schema
- SABER que chamadas de embedding são EM LOTE (schema `{items:[{i,v}]}`; bases de calibração de 8 chunks / 25 nomes de entidade em dim 64, adaptadas por clamp(base×64/dim, 1, base) — G44) — 39 spawns de subprocesso colapsam em 4-5
- USAR `--llm-parallelism <N>` em `remember` (default 4), `ingest` (default 2) e `edit` (default 4), clamp [1, 32], para o fan-out bounded de embedding com `Semaphore`
- USAR `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` para selecionar o modelo de embedding do claude (simétrico à var do codex); `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` (default 300) limita cada chamada LLM com `kill_on_drop(true)`
- SABER que o caminho de embedding usa `CLAUDE_CONFIG_DIR` VAZIO por padrão (honra `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR`); as flags de isolamento MCP são silenciosamente ignoradas pelo upstream (anthropics/claude-code#10787); um `~/.claude` populado custava ~223k tokens de cache por chamada (~40-50s → ~10-15s)
- USAR `enrich --operation re-embed --limit N --resume` como o caminho one-shot canônico de re-embedding; `edit --force-reembed` regenera um embedding sem mudar o body
- SABER que vetores divergentes FALHAM com erro explícito (sem truncamento ou preenchimento silencioso, G42/C5)
- SABER que o tamanho do lote se adapta à dimensionalidade do banco (G44): bancos 384 usam automaticamente 1 chunk / 4 nomes de entidade por chamada (orçamento de floats constante) — o workaround `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS=900` deixa de ser necessário; a env permanece disponível para corpos extremos
### OBRIGATÓRIO — G43: Adoção da Dimensionalidade em Toda Conexão
- SABER que `open_rw` E `open_ro` adotam `schema_meta.dim` em toda abertura de banco — `remember` / `edit` / `recall` / `hybrid-search` operam na dimensionalidade do banco (antes do G43 usavam silenciosamente o default compilado contra bancos 384 pré-v1.0.79, gravando embeddings de dimensões misturadas invisíveis ao cosseno)
- SABER que `init` não carimba mais `dim=384` e `rename-entity` registra o tamanho real do vetor
### PROIBIDO — Anti-padrões v1.0.79
- NUNCA passar `--gliner-variant` esperando seleção de modelo — é no-op formal com `tracing::warn!`
- NUNCA usar `ingest --mode gliner` para extração semântica — DEPRECIADO, somente URL-regex
- NUNCA depender do daemon — o código restante foi DELETADO na v1.0.79; a CLI é 100% one-shot
- NUNCA instalar com `--features embedding-legacy` ou `ner-legacy` — ambas as features foram REMOVIDAS


## Novidades na v1.0.76
### OBRIGATÓRIO — Arquitetura Apenas LLM e One-Shot (QUEBRANTE)
- SABER que v1.0.76 é a primeira release onde o build padrão não embute nenhum modelo local
- SABER que toda geração de embedding, NER e busca vetorial agora delega para `claude -p` ou `codex exec` headless (OAuth, sem MCP, sem hooks)
- SABER que a CLI é one-shot — não há daemon, não há runtime ONNX, não há download de modelo
- SABER que o binário de release é ~6 MB (de 39 MB)
- SABER que os crates `fastembed`, `ort`, `ndarray`, `tokenizers`, `huggingface-hub`, `sqlite-vec` e `GLiNER` foram REMOVIDOS do build padrão
- SABER que o subcomando `daemon` foi totalmente removido na v1.0.76 (ADR-0021)
- SABER que a migração V013 dropa as virtual tables `vec_memories` / `vec_entities` / `vec_chunks` e cria as tabelas BLOB-backed `memory_embeddings` / `entity_embeddings` / `chunk_embeddings`
- SABER que a similaridade de cosseno agora é calculada em Rust puro sob demanda em `src/similarity.rs` (ADR-0020, ADR-0022)
- SABER que a feature `llm-only` é o marcador canônico para o flip de padrão da v1.1.0

### OBRIGATÓRIO — Fluxo de Embedding LLM Apenas OAuth
- SABER que v1.0.76 herda o mandato OAuth-only da v1.0.69 e o aplica ao pipeline de embedding
- SABER que o spawn LLM ABORTA com `AppError::Validation` e código de saída 1 se `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estiverem no ambiente
- SABER que ambas as variáveis de chave de API estão EXCLUÍDAS da whitelist de env-clear em `claude_runner.rs`, `codex_spawn.rs` e `ingest_claude.rs`
- SABER que a flag `--bare` (que também exigiria uma chave de API) está REMOVIDA de todo caminho executável
- SABER que o fluxo OAuth (assinatura Claude Pro/Max ou ChatGPT Pro) é o ÚNICO mecanismo de credencial aceito
- REFERENCIAR `docs/decisions/adr-0011-oauth-only-enforcement.md` e `docs/decisions/adr-0025-oauth-only-embedding.md`

### OBRIGATÓRIO — Subcomandos Migrate para Bancos v1.0.74 / v1.0.75
- USAR `migrate --rehash --json` para reescrever checksums de migração registrados via SipHasher13 para casar com o conteúdo atual do arquivo
- USAR `migrate --to-llm-only --drop-vec-tables --json` como upgrade one-shot para bancos v1.0.74 / v1.0.75 (rehash + V013 + drop vec tables)
- SABER que `--drop-vec-tables` é a guarda de segurança explícita — a CLI recusa rodar sem ela
- SABER que a migração V002 foi intencionalmente esvaziada para no-op na v1.0.76, então `--rehash` é OBRIGATÓRIO para bancos v1.0.74 atualizarem limpamente
- REFERENCIAR `docs/MIGRATION.pt-BR.md` para o caminho completo v1.0.74 → v1.0.76 → v1.1.0 e `docs/decisions/adr-0026-v002-vec-tables-migration-drift.pt-BR.md` para a causa raiz V002
- SCHEMA: `migrate-rehash.schema.json` e `migrate-to-llm-only.schema.json` (ambos em `docs/schemas/`)

### OBRIGATÓRIO — Matriz CI de 3 Features e Mock LLM CLI
- SABER que o workflow de CI roda jobs de `clippy` e `test` com uma CLI stub `mock-llm` no `PATH` para que testes de round-trip de embedding rodem sem credenciais OAuth reais
- SABER que 26 arquivos de teste foram cabeados para consumir a mock LLM CLI como substituto drop-in para `claude -p` e `codex exec`
- SABER que 107 de 115 testes previamente lentos foram corrigidos no commit `bd0a3f5` (a mock LLM desbloqueia testes que dependiam de um turno OAuth real)
- SABER que 11 novos testes unitários cobrem o subcomando migrate e 4 novos testes de integração cobrem os subcomandos de CLI end-to-end

### OBRIGATÓRIO — 7 Novos ADRs (v1.0.76)
- SABER que 7 novos Architecture Decision Records foram adicionados (todos com traduções PT-BR):
  - `adr-0019-llm-only-one-shot.md` — justificativa para remover fastembed, ort, ndarray, tokenizers, hf-hub, sqlite-vec
  - `adr-0020-pure-rust-cosine.md` — substituição do KNN sqlite-vec por cosseno em Rust puro
  - `adr-0021-deprecate-daemon.md` — o daemon não é mais otimização de performance
  - `adr-0022-blob-embeddings.md` — migração V013 dropa vec tables; tabelas BLOB-backed
  - `adr-0023-remove-tokenizers.md` — heurística de token whitespace substitui o crate tokenizers
  - `adr-0024-fts5-coarse-cosine-refine.md` — filtro grosso FTS5 + refinamento por cosseno
  - `adr-0025-oauth-only-embedding.md` — fluxo OAuth-only para o pipeline de embedding LLM
- SABER que `adr-0026-v002-vec-tables-migration-drift.md` documenta a causa raiz do mismatch V002

### OBRIGATÓRIO — 2 Novos Schemas JSON (v1.0.76)
- SABER que `migrate-rehash.schema.json` define o contrato JSON para `migrate --rehash --json` (campos: `action`, `rewritten`, `skipped`, `errors`, `namespace`, `db_path`, `elapsed_ms`)
- SABER que `migrate-to-llm-only.schema.json` define o contrato JSON para `migrate --to-llm-only --json` (campos: `action`, `rewritten`, `v013_applied`, `schema_version`, `vec_tables_were_present`, `vec_tables_dropped`, `embedding_tables_created`, `namespace`, `db_path`, `elapsed_ms`)
- Ambos os schemas seguem a convenção do projeto `"additionalProperties": false` e estão indexados em `docs/schemas/README.md`

### OBRIGATÓRIO — Nova Documentação (v1.0.76)
- SABER que `docs/HOW_TO_USE.md` e `docs/HOW_TO_USE.pt-BR.md` foram reescritos para v1.0.76 LLM-Only
- SABER que `docs/MIGRATION.md` e `docs/MIGRATION.pt-BR.md` foram criados cobrindo v1.0.74 → v1.0.76 → v1.1.0
- SABER que `docs/HEADLESS_INVOCATION.md` e `docs/HEADLESS_INVOCATION.pt-BR.md` foram criados cobrindo invocação headless OAuth-safe de Claude/Codex/OpenCode
- SABER que `docs/AGENTS.md` ganhou seções "v1.0.76 Architecture (LLM-Only)" e "OAuth Enforcement"
- SABER que `docs/TESTING.md` ganhou seção "v1.0.76 Test Infrastructure — 3-Feature CI Matrix"
- SABER que `docs/COOKBOOK.md` ganhou receita "Como Atualizar De v1.0.74 Ou v1.0.75 Para v1.0.76"

### PROIBIDO — Anti-padrões v1.0.76
- NUNCA tentar instalar `fastembed`, `tokenizers` ou `sqlite-vec` — esses crates foram removidos do build padrão
- NUNCA usar o build padrão da v1.0.76 em um host sem `claude` ou `codex` CLI no `PATH` — o pipeline de embedding exige
- NUNCA definir `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` e esperar uma chamada de embedding bem-sucedida — o spawn ABORTA
- NUNCA depender do subcomando `daemon` — o código restante foi DELETADO na v1.0.79; a CLI é 100% one-shot
- NUNCA chamar `migrate --to-llm-only` sem `--drop-vec-tables` — a CLI recusa rodar por segurança
- NUNCA adicionar código novo que dependa do cache de modelo ONNX da v1.0.74 — o build padrão é LLM-only
- NUNCA assumir que a similaridade de cosseno é computada pelo sqlite-vec — agora é Rust puro sob demanda em `src/similarity.rs`


## CRUD — Bulk Ingest com ingest
### OBRIGATÓRIO — Quando Usar ingest
- USAR `ingest <DIR>` para importar diretórios inteiros como memórias
- PREFERIR sobre loop `fd | xargs remember` em qualquer caso
- CADA arquivo correspondente ao pattern vira memória individual
- NOME da memória deriva do basename do arquivo sem extensão em kebab-case
- NOMES com mais de 60 caracteres são TRUNCADOS automaticamente
- NDJSON inclui `truncated: true` e `original_name` quando trunca
- AGENTE deve usar `original_name` ou `name` do NDJSON para acessar a memória
- SAÍDA é NDJSON, uma linha JSON por arquivo mais uma linha summary final
- CONSUMIR linha a linha em streaming via `jaq -c` ou `while read`
### OBRIGATÓRIO — Padrão de Arquivos com --pattern
- PADRÃO é `*.md` apenas, mude conforme necessário
- ACEITA `*.<ext>` para extensão genérica
- ACEITA `<prefixo>*` para prefixo de basename
- ACEITA filename exato sem caracteres glob
- GLOB completo POSIX não é suportado pelo ingest
### OBRIGATÓRIO — Recursão e Limites
- LIGAR `--recursive` para descer em subdiretórios
- SEM `--recursive` apenas top-level é processado
- RESPEITAR `--max-files 10000` como cap padrão de segurança
- `--max-files` REJEITA a operação inteira com exit 1 se contagem exceder o cap
- `--max-files` NÃO limita aos primeiros N, é validação all-or-nothing
- AUMENTAR cap apenas após auditoria de volume real
- USAR `--fail-fast` para parar na primeira falha por arquivo
- SEM `--fail-fast` o loop continua e reporta cada erro no NDJSON
### OBRIGATÓRIO — Tipo de Memória em Massa
- DECLARAR `--type` aplicado a TODOS os arquivos da invocação
- PADRÃO é `document` quando omitido
- VALORES válidos: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- INVOCAR `ingest` separadamente por tipo quando misturar
- AGRUPAR arquivos por diretório conforme o tipo desejado
### OBRIGATÓRIO — Controle de Memória RAM
- USAR `--low-memory` em containers com menos de 4 GB
- DEFINIR `SQLITE_GRAPHRAG_LOW_MEMORY=1` como override persistente
- `--low-memory` força `--ingest-parallelism 1` internamente
- TRADE-OFF é 3 a 4 vezes mais tempo de execução
- ESCOLHER quando RSS for restrição maior que latência
- USAR `--max-rss-mb <MiB>` para abortar se o RSS do processo ultrapassar o threshold durante o embedding (padrão 8192 MiB)
### OBRIGATÓRIO — Dois Eixos de Paralelismo
- `--max-concurrency <N>` controla CLI invocations simultâneas
- `--ingest-parallelism <N>` controla extract mais embed em paralelo
- PADRÃO de `--max-concurrency` é 4
- PADRÃO de `--ingest-parallelism` é `min(4, max(1, cpus/2))`
- DISTINGUIR claramente os dois eixos antes de ajustar
- AMPLIAR `--wait-lock <SECONDS>` para esperar slot antes de exit 75
### OBRIGATÓRIO — Performance e Extração
- NER desabilitado por padrão; passar `--enable-ner` para ativar extração automática — SOMENTE URL-regex desde a v1.0.79 (o pipeline GLiNER em ONNX, o download de modelo de 1,1 GB e a seleção via `--gliner-variant` foram removidos)
- `--gliner-variant` é no-op desde a v1.0.79 e emite `tracing::warn!` quando definido
- USAR `--enable-ner` apenas quando a extração de URLs como entidades for valiosa
- Campo `extraction_method` na resposta reporta: `url-regex` ou `none:extraction-failed` (valores `gliner-*` e `regex-only` são HISTÓRICOS, ≤ v1.0.75)
- Duplicatas no ingest emitem `status: "skipped"` com `action: "duplicate"` em vez de `status: "failed"`
- PREFERIR `--graph-stdin` com entidades curadas por LLM para melhor qualidade (NER está desligado por padrão; `--skip-extraction` está obsoleto desde v1.0.45)
- USAR `--dry-run` para visualizar o mapeamento arquivo-nome sem spawnar subprocesso LLM ou persistir dados
- Eventos NDJSON por arquivo incluem o campo `original_filename` preservando o basename do arquivo antes da normalização para kebab-case
### PROIBIDO — Anti-padrões de ingest
- NUNCA usar `fd | xargs sqlite-graphrag remember` quando `ingest` existe
- NUNCA omitir `--recursive` esperando descida automática
- NUNCA passar pattern com glob complexo não suportado
- NUNCA ignorar exit 75 de slot exausto em loops automatizados
- NUNCA misturar tipos diferentes na mesma invocação
- NUNCA elevar `--max-files` sem medir RAM e disco antes
- NUNCA usar `--force-merge` no ingest (flag exclusiva do `remember`)
### Padrão Correto — Exemplos de ingest
- `sqlite-graphrag ingest ./docs --recursive --pattern "*.md" --json`
- `sqlite-graphrag ingest ./decisoes --type decision --json`
- `sqlite-graphrag ingest ./large-corpus --low-memory --max-files 50000 --json`
- `sqlite-graphrag ingest ./skills --type skill --recursive --fail-fast --json`
- `sqlite-graphrag ingest ./notas --type note --pattern "memo-*" --recursive --json`
### Padrão Correto — Consumo do NDJSON
- `sqlite-graphrag ingest ./docs --recursive --json | jaq -c 'select(.status == "indexed")'`
- `sqlite-graphrag ingest ./docs --recursive --json | tee resultados.ndjson`
- NDJSON contém `files_total + 1` linhas: uma por arquivo mais uma summary final
- FILTRAR por `select(.status)` para ignorar a summary line que não tem campo `status`
- `jaq -sc '[.[] | select(.status)] | group_by(.status) | map({status: .[0].status, count: length})' < resultados.ndjson`
### OBRIGATÓRIO — Schema NDJSON por Tipo de Linha
- Linha por arquivo: `file`, `name`, `status` (`"indexed"` `"skipped"` `"failed"`), `truncated`, `original_name?`, `memory_id?`, `action?`, `error?`, `body_length?`
- Linha summary final: `summary` (true), `dir`, `pattern`, `recursive`, `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- Eventos de extração NER vão para stderr, NÃO stdout
- USAR `--max-name-length N` para sobrescrever o limite padrão de truncamento de 60 caracteres para nomes de memória
- Basenames numéricos (ex.: `123.md`) recebem o prefixo automático `doc-` para produzir nomes kebab-case válidos (ex.: `doc-123`)
### OBRIGATÓRIO — Modos de Ingestão (v1.0.62)
- `--mode none` (padrão): ingestão apenas do body, sem extração de entidades/relações
- `--mode gliner`: DEPRECIADO desde a v1.0.79 (somente URL-regex; emite `tracing::warn!`); usar `--mode claude-code` ou `--mode codex` para extração semântica
- `--mode claude-code`: extração curada por LLM via Claude Code CLI instalado localmente (`claude -p` headless)
- Modo Claude Code spawna `claude -p` por arquivo com `--json-schema` para saída estruturada garantida
- Requer Claude Code >= 2.1.0 instalado na máquina com assinatura Pro/Max ativa
- Extrai entidades do domínio e relações tipadas restritas a enums canônicos
- `--resume` continua ingestão interrompida a partir do queue DB; `--retry-failed` retenta apenas falhas
- `--max-cost-usd <N>` para quando custo acumulado exceder o orçamento
- `--claude-binary <PATH>` sobrescreve busca no PATH; `--claude-model <MODEL>` seleciona modelo
- --claude-timeout <S> define timeout por arquivo (padrão 300s); mata processos travados
- Queue DB `.ingest-queue.sqlite` rastreia progresso por arquivo; `--keep-queue` retém após conclusão
- Rate limit: backoff exponencial automático (60s → 120s → 300s → 900s)
- `--dry-run` com `--mode claude-code` emite eventos `status: "preview"` sem spawnar Claude — zero tokens consumidos
- Re-ingestão do mesmo diretório ATUALIZA memórias existentes (force-merge) em vez de falhar com UNIQUE constraint
- Falha de cold-start `--json-schema` automaticamente retentada uma vez após 2s (workaround para Claude Code Issue #23265)
- Subprocesso roda com `env_clear()` + injeção seletiva para hardening de segurança
- OAuth é o ÚNICO fluxo de credencial aceito para `claude -p` (desde v1.0.69)
- SEMPRE passa `--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions` (7 flags de endurecimento; `--bare` REMOVIDO de todo caminho executável na v1.0.69)
- ABORTA o spawn com `AppError::Validation` se `ANTHROPIC_API_KEY` estiver definida no ambiente (OAuth-only enforcement, v1.0.69)
- `ANTHROPIC_API_KEY` está excluída do whitelist de env-clear como defesa em profundidade (v1.0.69)
- 4 testes `#[serial_test::serial(env)]` validam o conjunto canônico de flags e o comportamento de aborto (v1.0.69)
- Eventos NDJSON por arquivo incluem campos `entities` (contagem), `rels` (contagem), `cost_usd`; desde v1.0.64 `cost_usd` é omitido para usuários OAuth (assinatura, não cobrado por chamada de API)
- Summary inclui `entities_total`, `rels_total`, `cost_usd` totais; `--max-cost-usd` é ignorado com warning para usuários OAuth (desde v1.0.64)
- Desde v1.0.64: arquivos excedendo limite de 512 KB são ignorados ANTES da extração LLM com `status: "skipped"` para evitar desperdício de tokens
- Schemas: `ingest-claude-phase.schema.json`, `ingest-claude-file-event.schema.json`, `ingest-claude-summary.schema.json`
- `--mode codex`: extração curada por LLM via OpenAI Codex CLI (`codex exec --json` headless por arquivo)
- Modo Codex requer Codex CLI >= 0.120.0 com API key OpenAI ativa; usa `--output-schema` para JSON estruturado
- `--codex-binary <PATH>` sobrescreve busca no PATH; `--codex-model <MODEL>` seleciona modelo; `--codex-timeout <S>` (padrão 300s)
- Variável de ambiente `SQLITE_GRAPHRAG_CODEX_BINARY` sobrescreve busca no PATH
- Pipeline completo de embedding aplicado — memórias ficam pesquisáveis via `recall` e `hybrid-search`
- Desde v1.0.63: strings de relação da extração LLM são normalizadas antes da inserção no DB (`depends-on` → `depends_on`) — consistente com o comando `remember`
- Modo Codex reutiliza o mesmo formato NDJSON do claude-code: `ingest-claude-phase.schema.json`, `ingest-claude-file-event.schema.json`, `ingest-claude-summary.schema.json`
### Padrão Correto — Exemplos de Ingestão Claude Code
- `sqlite-graphrag ingest ./docs --mode claude-code --recursive --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --resume --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --max-cost-usd 5.00 --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --claude-model claude-sonnet-4-6 --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --claude-timeout 600 --max-cost-usd 10.00 --json`
### Padrão Correto — Exemplos de Ingestão Codex
- `sqlite-graphrag ingest ./docs --mode codex --recursive --json`
- `sqlite-graphrag ingest ./docs --mode codex --codex-model o4-mini --json`
- `sqlite-graphrag ingest ./docs --mode codex --codex-timeout 600 --json`
- `sqlite-graphrag ingest ./docs --mode codex --codex-binary /usr/local/bin/codex --json`


## CRUD — Read com read e list
### OBRIGATÓRIO — Leitura Direta por Nome ou ID (read)
- USAR `read --name <kebab-case>` para fetch O(1) por nome
- USAR `read --id <N>` para lookup direto por memory_id (v1.0.67) — evita busca semântica quando o ID é conhecido de output prévio de `list` ou `recall`
- USAR `read --with-graph` para incluir entidades e relacionamentos vinculados na resposta (v1.0.67)
- PARSEAR campos `body`, `description`, `created_at_iso`, `updated_at_iso`
- TRATAR exit code 4 como memória inexistente no namespace
- APLICAR `--tz` para localizar timestamps na saída
### OBRIGATÓRIO — Enumeração com Filtros (list)
- USAR `list --type <kind>` para filtrar por tipo de memória
- AJUSTAR `--limit <N>`; padrão é TODOS os registros no modo JSON, 50 no modo texto
- PAGINAR via `--offset <N>` para datasets grandes
- INCLUIR memórias soft-deletadas via `--include-deleted`
- EXPORTAR full dump com `--limit 10000 --json` antes de backup
- RESPOSTA agora inclui `total_count` (total de registros encontrados), `truncated` (bool), e `body_length` (int) por item
### Padrão Correto — Exemplos de Leitura
- `sqlite-graphrag read --name design-auth --json`
- `sqlite-graphrag list --type decision --limit 100 --json`
- `sqlite-graphrag list --include-deleted --json | jaq '.items[] | select(.deleted)'`


## CRUD — Update com edit, rename e restore
### OBRIGATÓRIO — Edição de Corpo e Descrição (edit)
- USAR `edit --name <nome> --body <texto>` para corpos curtos
- PREFERIR `--body-file` ou `--body-stdin` para corpos longos
- ALTERAR descrição via `--description <texto>`
- ALTERAR tipo da memória via `--type <tipo>` (ex.: `note` para `decision`) sem recriar a memória (v1.0.67); pula re-embedding quando body não mudou
- CADA edit cria nova versão imutável preservando histórico
- EDIT regenera embedding vetorial quando body muda — `recall` e `hybrid-search` retornam scores precisos após edit (desde v1.0.63; edições somente de descrição não re-embedam)
- USAR `edit --force-reembed` (v1.0.79) para regenerar o embedding SEM mudar o body — o conserto cirúrgico para memória com embedding ausente ou de dimensionalidade errada
- USAR `--llm-parallelism <N>` (v1.0.79, default 4, clamp [1, 32]) para limitar o fan-out de subprocessos de embedding
- VALIDAR exit code 3 como conflito de locking otimista
- JSON response: `memory_id`, `name`, `action` ("updated"), `version`, `elapsed_ms`
- v1.0.56: bug de dessincronização do FTS5 corrigido — memórias editadas ficam imediatamente localizáveis via busca full-text
### OBRIGATÓRIO — Renomeação Preservando Histórico (rename)
- USAR `rename --name <antigo> --new-name <novo>`
- ACEITAR `--old`/`--new` e `--from`/`--to` como aliases desde v1.0.35
- PRESERVAR todas as versões e conexões do grafo
- TRATAR exit code 4 como memória de origem ausente
- Desde v1.0.64: rejeita renomeação para o mesmo nome com exit 1 (Validation) — previne inflação de versão
- JSON response: `memory_id`, `name` (novo), `action` ("renamed"), `version`, `elapsed_ms`, `ghost_purged` (bool?, v1.0.67 — true quando uma memória soft-deleted ocupando o nome alvo foi auto-purgada)
- v1.0.56: bug de dessincronização do FTS5 corrigido — memórias renomeadas ficam imediatamente localizáveis via busca full-text
### OBRIGATÓRIO — Restauração de Versão Antiga (restore)
- INSPECIONAR versões via `history --name <nome>` primeiro
- USAR `restore --name <nome> --version <N>` para versão específica
- OMITIR `--version` seleciona última versão não-restore automaticamente
- RESTORE cria nova versão sem sobrescrever histórico anterior
- RESTORE preserva o nome atual da memória — se a memória foi renomeada após a versão alvo ser criada, o nome permanece como está (corrigido em v1.0.63; antes revertia para o nome original da versão)
- RE-EMBED ocorre automaticamente para recall vetorial voltar a encontrar
- JSON response inclui `action: "restored"`, `memory_id`, `name`, `version`, `restored_from`, `elapsed_ms`
- v1.0.56: bug de dessincronização do FTS5 corrigido — memórias restauradas ficam imediatamente localizáveis via busca full-text
### OBRIGATÓRIO — Locking Otimista
- PASSAR `--expected-updated-at <epoch_ou_RFC3339>` em pipelines concorrentes
- TRATAR exit code 3 como concorrência detectada
- RECARREGAR `read --json` para obter novo `updated_at` antes de retentar
- APLICAR locking em `edit`, `rename` e `restore`
### Padrão Correto — Fluxos de Update
- `sqlite-graphrag edit --name design-auth --body-file ./revisado.md --expected-updated-at "2026-04-19T12:00:00Z"`
- `sqlite-graphrag rename --from nome-antigo --to nome-novo`
- `sqlite-graphrag history --name design-auth --json && sqlite-graphrag restore --name design-auth --version 2`


## CRUD — Delete com forget, purge, unlink e cleanup-orphans
### OBRIGATÓRIO — Remoção Lógica (forget)
- USAR `forget --name <nome>` para soft-delete reversível
- MEMÓRIA desaparece de `recall` e `list` por padrão
- HISTÓRICO de versões permanece intacto no banco
- REVERSÍVEL via `restore` enquanto não houver purge
- JSON response: `action` (`"soft_deleted"` `"already_deleted"`), `forgotten`, `name`, `namespace`, `deleted_at?`, `deleted_at_iso?`, `elapsed_ms`
- Desde v1.0.52: forget NÃO emite JSON quando a memória não é encontrada; retorna apenas erro no stderr + exit 4
### OBRIGATÓRIO — Remoção Física (purge)
- USAR `purge --retention-days <N> --yes` em automação
- PADRÃO de retenção é 90 dias para memórias soft-deletadas
- EXECUTAR `--dry-run` primeiro para auditar contagem
- APAGA permanentemente linhas e reclama espaço em disco
### OBRIGATÓRIO — Remoção de Aresta (unlink)
- USAR `unlink --from <a> --to <b> --relation <tipo>` para remoção direcionada
- `--relation` agora é OPCIONAL; omitir remove todas as arestas entre `--from` e `--to`
- USAR `--entity <nome> --all` para remover em massa TODOS os relacionamentos de uma entidade (qualquer direção)
- ACEITAR `--source`/`--target` como aliases de `--from`/`--to`
- TRATAR exit code 4 como aresta inexistente
- `--relation` aceita qualquer string em kebab-case ou snake_case; valores não canônicos emitem `tracing::warn!` desde v1.0.50
### OBRIGATÓRIO — Limpeza de Entidades Órfãs (cleanup-orphans)
- EXECUTAR `cleanup-orphans --dry-run` para auditar
- APLICAR `--yes` em pipelines automatizados
- REMOVE entidades sem memórias vinculadas nem arestas
- RODAR periodicamente após operações `forget` em massa
### OBRIGATÓRIO — Remoção em Massa de Relacionamentos (prune-relations)
- USAR `prune-relations --relation <tipo> --yes` para remoção em massa de todos os relacionamentos de um tipo
- USAR `--dry-run` para visualizar a contagem antes de confirmar
- USAR `--show-entities` com `--dry-run` para listar os nomes das entidades afetadas na resposta
- USAR `--yes` para pular confirmação interativa em pipelines automatizados
- ACEITA qualquer string em kebab-case ou snake_case como relação
- EXECUTAR `cleanup-orphans` depois para remover entidades sem relacionamentos restantes
- JSON response: `action` (`"pruned"` `"dry_run"`), `relation`, `count`, `entities_affected`, `affected_entity_names?`, `namespace`, `elapsed_ms`
### Padrão Correto — Round-Trip Forget e Restore
- `sqlite-graphrag forget --name decisao-x`
- `sqlite-graphrag history --name decisao-x --json | jaq '.deleted'`
- `sqlite-graphrag restore --name decisao-x`
- `sqlite-graphrag recall "decisão" --json`


## Gerenciamento de Entidades (v1.0.56)
### OBRIGATÓRIO — Validação e Normalização de Nome de Entidade (v1.0.58, melhorado em v1.0.65)
- TODOS os caminhos de criação de entidade (`link --create-missing`, `remember --graph-stdin`, `ingest --enable-ner`, `rename-entity --new-name`) validam nomes via `validate_entity_name()`
- REJEITA nomes com menos de 2 caracteres (exit 1)
- REJEITA nomes contendo caracteres de quebra de linha (exit 1)
- REJEITA abreviações ALL_CAPS de 4 caracteres ou menos como ruído de NER (exit 1)
- Desde v1.0.65: após validação, nomes são NORMALIZADOS para kebab-case ASCII minúsculo via `normalize_entity_name()` antes de gravar — `"Claude Code"` vira `claude-code`, `"CANONICAL_RELATIONS"` vira `canonical-relations`
### OBRIGATÓRIO — Remover Entidade (delete-entity)
- USAR `delete-entity --name <entidade> --json` para remover permanentemente um nó de entidade
- ADICIONAR `--cascade` para também remover todos os relacionamentos e bindings de memória vinculados
- SEM `--cascade` o comando falha com exit 1 se a entidade tiver relacionamentos
- JSON response: `action`, `entity_name`, `relationships_removed`, `bindings_removed`, `elapsed_ms`
- TRATAR exit code 4 como entidade não encontrada
### OBRIGATÓRIO — Reclassificar Tipo de Entidade (reclassify)
- USAR `reclassify --name <entidade> --entity-type <novo> --json` para alterar o tipo de uma entidade individual
- USAR `reclassify --from-type <antigo> --to-type <novo> --batch --json` para reclassificar em massa todas as entidades de um tipo
- JSON response: `action`, `count`, `description_updated?`, `namespace`, `elapsed_ms`
### OBRIGATÓRIO — Mesclar Entidades (merge-entities)
- USAR `merge-entities --names "a,b,c" --into <alvo> --json` para mesclar múltiplas entidades em uma
- TODOS os relacionamentos das entidades de origem são movidos para `<alvo>`
- ENTIDADES de origem são deletadas após a mesclagem
- JSON response: `action`, `sources`, `target`, `relationships_moved`, `entities_removed`, `elapsed_ms`
- TRATAR exit code 4 como qualquer entidade nomeada não encontrada
### OBRIGATÓRIO — Listar Entidades de uma Memória (memory-entities)
- USAR `memory-entities --name <memória> --json` para listar todas as entidades vinculadas a uma memória específica
- USAR `memory-entities --entity <nome-entidade> --json` para listar todas memórias vinculadas a uma entidade (busca reversa, v1.0.58)
- JSON response direta: `memory_name`, `entities: [{entity_id, name, entity_type}]`, `count`, `elapsed_ms`
- JSON response reversa: `entity_name`, `memories: [{memory_id, name, description, memory_type}]`, `count`, `elapsed_ms`
- TRATAR exit code 4 como memória ou entidade não encontrada; exit 0 com count 0 significa que existe mas sem vínculos
### OBRIGATÓRIO — Remover Bindings NER (prune-ner)
- USAR `prune-ner --entity <nome> --json` para remover bindings NER de uma entidade específica
- USAR `prune-ner --all --yes --json` para remover TODOS os bindings NER do namespace
- JSON response: `action`, `bindings_removed`, `elapsed_ms`
- Bindings NER são os vínculos criados automaticamente pela extração NER (GLiNER ≤ v1.0.75; URL-regex desde a v1.0.79); links manuais de grafo NÃO são afetados


## Histórico Imutável de Versões
### OBRIGATÓRIO — Inspeção com history
- USAR `history --name <nome> --json` para listar versões
- USAR `history --name <nome> --diff --json` para incluir estatísticas de diff de caracteres entre versões
- VERSÕES começam em 1 e incrementam a cada `edit` ou `restore`
- ORDEM cronológica reversa por padrão
- INCLUI memórias soft-deletadas com flag `deleted: true`
- COM `--diff`, cada versão inclui `changes: {added_chars, removed_chars}` com o diff em relação à versão anterior
### OBRIGATÓRIO — Semântica de Versões
- CADA `edit` cria nova versão imutável preservando anteriores
- CADA `restore` cria nova versão com corpo de versão antiga
- AUDIT TRAIL completo de quem mudou o que e quando
- RETENTION POLICY controla quando purgar definitivamente
### Padrão Correto — Auditoria de Mudanças
- `sqlite-graphrag history --name design-auth --json | jaq '.versions[].created_at_iso'`


## Pesquisa GraphRAG
### OBRIGATÓRIO — Cinco Comandos de Busca
- USAR `recall` para busca KNN vetorial com expansão automática de grafo
- USAR `hybrid-search` para fusão de FTS5 e vetorial via RRF
- USAR `related` para travessia multi-hop a partir de memória conhecida
- USAR `graph traverse` para travessia a partir de entidade tipada
- USAR `deep-research` para pesquisa profunda multi-hop paralela com decomposição de query
- COMBINAR os cinco no padrão de três camadas canônico ou usar `deep-research` como alternativa de comando único
### Deep Research (v1.0.64, melhorado em v1.0.65)
- `sqlite-graphrag deep-research "<query>" --k 20 --json` — pesquisa profunda multi-hop paralela com decomposição de query
- Divide a query em até 7 sub-queries, computa embedding SEPARADO por sub-query (correção v1.0.65 — antes compartilhava um embedding), executa em paralelo via JoinSet + Semaphore bounded
- Funde resultados KNN + FTS5 via RRF por sub-query (correção v1.0.65 — FTS tinha score fixo 0.5)
- Cadeias de evidência são caminhos direcionados seed-para-target (correção v1.0.65 — era dump flat das top-20 relações globais)
- Scores do grafo incorporam score do seed, decaimento por hop e peso da aresta (correção v1.0.65)
- Output: `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context?` (entidades + relações das memórias encontradas, v1.0.66), `stats`
- Substitui o pipeline manual de 3 camadas para pesquisa completa em uma única invocação
- `--k 20` resultados por sub-query (padrão, Recall@20 captura 95%+ dos hits relevantes)
- `--max-sub-queries 7` limita decomposição (padrão, calibrado contra benchmarks MuSiQue/StepChain)
- `--max-hops 3` profundidade de travessia do grafo (padrão, sweet spot segundo benchmark NovelHopQA)
- `--min-weight 0.3` filtra edges fracos na travessia (padrão)
- `--max-results 50` limita output deduplicado (padrão)
- `--with-bodies` inclui corpos completos das memórias nos resultados (opt-in)
- `--max-concurrency N` limita sub-queries paralelas (padrão: min(cpus, 8))
- `--timeout 30` timeout por sub-query em segundos (padrão)
- `--rrf-k 60` constante de fusão RRF (v1.0.65, igual ao hybrid-search)
- `--graph-decay 0.7` fator de decaimento do score por hop (v1.0.65)
- `--graph-min-score 0.05` threshold mínimo de score para resultados expandidos por grafo (v1.0.65)
- `--max-neighbors-per-hop N` limita fan-out do BFS por entidade por hop (v1.0.65, padrão ilimitado)
### Reclassificar Tipos de Relacionamento (v1.0.65)
- `sqlite-graphrag reclassify-relation --from-relation <antigo> --to-relation <novo> --batch --json` — renomeia tipos de relacionamento em massa
- Modo individual: `--source A --target B --from-relation antigo --to-relation novo`
- Modo batch: `--from-relation antigo --to-relation novo --batch`
- Filtros opcionais: `--filter-source-type`, `--filter-target-type`
- Trata colisões UNIQUE via `UPDATE OR IGNORE` + `DELETE`
- `--dry-run` faz preview sem modificar o banco
- JSON response: `action`, `from_relation`, `to_relation`, `count`, `merged_duplicates`, `namespace`, `elapsed_ms`
### Normalizar Nomes de Entidade (v1.0.65)
- `sqlite-graphrag normalize-entities --yes --json` — normaliza todos os nomes de entidade para kebab-case ASCII minúsculo
- Mescla colisões automaticamente: `Claude Code` + `claude-code` viram um nó com relacionamentos combinados
- `--dry-run` faz preview de quais entidades seriam renomeadas ou mescladas
- Normalização: decomposição NFKD → filtro ASCII → minúsculas → espaços/underscores para hífens → colapso de hífens consecutivos
- Nomes de entidade também são normalizados em todo path de escrita desde v1.0.65 (remember, ingest, link, rename-entity)
- JSON response: `action`, `normalized_count`, `merged_count`, `namespace`, `elapsed_ms`
### Enriquecer Qualidade do Grafo com LLM (v1.0.65)
- `sqlite-graphrag enrich --operation <op> --mode claude-code --json` — pipeline de qualidade do grafo aumentada por LLM
- Operações: `memory-bindings` (extrai entidades de memórias órfãs), `entity-descriptions` (gera descrições para entidades sem descrição), `body-enrich` (expande corpos curtos de memória) e `re-embed` (v1.0.79 — reconstrói embeddings de memória faltantes sem reescrever corpos; o caminho one-shot canônico de re-embedding com `--limit N --resume`)
- `--dry-run` faz preview sem spawnar LLM (zero tokens)
- `--max-cost-usd N` limita gasto acumulado da API (ignorado para usuários OAuth)
- `--resume` e `--retry-failed` para resiliência via queue DB
- `--llm-parallelism <N>` controla quantos subprocessos LLM rodam simultaneamente (v1.0.67, padrão 1); definir 2-4 para reduzir tempo de execução em lotes grandes de enriquecimento
- Saída é NDJSON: eventos de fase, eventos por item (status: `done`/`failed`/`skipped`/`preview`), linha de resumo
- Schemas: `enrich-phase.schema.json`, `enrich-item-event.schema.json`, `enrich-summary.schema.json`
### OBRIGATÓRIO — Padrão de Três Camadas Canônico
- CAMADA 1 — `hybrid-search` para encontrar memórias seed por nome
- CAMADA 2 — `read --name` para expandir corpo completo da memória
- CAMADA 3 — `related` ou `graph traverse` para subgrafo multi-hop
- APLICAR camadas em ordem, parando quando contexto basta
- INJETAR resultados consolidados no prompt do LLM
### OBRIGATÓRIO — Camada 1 com hybrid-search
- USAR `hybrid-search <query> --k 10 --rrf-k 60 --json`
- COMBINA FTS5 textual e KNN vetorial via Reciprocal Rank Fusion
- AJUSTAR `--weight-vec` e `--weight-fts` apenas com evidência numérica
- PADRÃO de ambos os pesos é `1.0` com fusão equilibrada
- EXTRAIR apenas `name` via `jaq -r '.results[].name'` para next stage
### OBRIGATÓRIO — hybrid-search com Expansão de Grafo
- ATIVAR travessia de grafo via `--with-graph` para descobrir memórias conectadas
- AJUSTAR profundidade com `--max-hops <N>` (padrão 2)
- FILTRAR arestas fracas com `--min-weight <F>` (padrão 0.3)
- RESULTADOS do grafo ficam em `graph_matches[]`, SEPARADOS de `results[]`
- `graph_matches[]` usa schema RecallItem: `name`, `distance`, `source` ("graph"), `graph_depth`
- LER AMBOS `results[]` e `graph_matches[]` quando `--with-graph` ativo
- EXTRAIR via `jaq -r '(.results[] , .graph_matches[]) | .name'`
### OBRIGATÓRIO — Camada 1 Alternativa com recall
- USAR `recall <query> --k 5 --json` para queries semânticas puras
- ACEITAR `--limit` como alias de `--k` desde v1.0.35
- RECALL expande automaticamente via grafo por padrão
- DESLIGAR expansão automática de grafo via `--no-graph`
- INTERPRETAR `distance` crescente como similaridade decrescente
- INTERPRETAR `score` como `1.0 - distance`, clamped a `[0.0, 1.0]`
- CAMPO `source` indica origem: `"direct"` (KNN) ou `"graph"` (travessia)
- CAMPO `graph_depth` presente apenas em resultados com `source: "graph"`
- RecallResponse separa `direct_matches[]`, `graph_matches[]` e `results[]` (agregado)
- USAR quando query não mistura tokens exatos com linguagem natural
### OBRIGATÓRIO — Camada 2 com read --name
- USAR `read --name <nome>` para obter corpo completo da memória seed
- EXPANDIR contexto além do snippet retornado pela camada 1
- LOOP sobre os top-k nomes para construir bundle de contexto
- PARSEAR campos `body`, `description`, `created_at_iso`
### OBRIGATÓRIO — Camada 3 com related
- USAR `related <nome> --hops <N>` para travessia multi-hop
- DOIS hops revelam conhecimento transitivo invisível à busca vetorial
- DISTÂNCIA de hop entrega sinal explícito ao orquestrador
- USAR quando a query exige raciocínio multi-passo encadeado
- Filtro `--relation` aceita qualquer string em kebab-case ou snake_case; valores não canônicos emitem `tracing::warn!` desde v1.0.50
### OBRIGATÓRIO — Camada 3 Alternativa com graph traverse
- USAR `graph traverse --from <raiz> --depth <N>` para subgrafo focado
- PADRÃO de profundidade é 2 quando omitido
- TRATAR exit code 4 como entidade raiz inexistente
- HOPS retornam `entity`, `relation`, `direction`, `weight`, `depth`
- PARTIR de entidade tipada, não de nome de memória
### OBRIGATÓRIO — Semântica dos Scores e Distâncias
- `recall` retorna `distance` (menor é mais similar) e `score` (1.0 - distance)
- `recall` retorna `source` (`"direct"` ou `"graph"`) e `graph_depth` (quando graph)
- `hybrid-search` retorna `combined_score`, maior é melhor ranking
- `hybrid-search` expõe `vec_rank` e `fts_rank` para auditar fusão
- `hybrid-search` com `--with-graph` adiciona `graph_matches[]` em campo separado
- `hybrid-search` resposta agora inclui `fts_degraded` (bool), `fts_error` (string?), `fts_auto_rebuilt` (bool); quando `fts_degraded` é true, apenas resultados vetoriais são retornados
- Campos por resultado do `hybrid-search` também incluem `normalized_score` (score combinado normalizado 0-1), `vec_distance` (float?), `fts_bm25` (float?)
- `related` retorna `hop_distance`, profundidade explícita no grafo
- `graph traverse` retorna `depth` por hop visitado
- DESCARTAR hits fracos antes de gastar tokens no prompt
### OBRIGATÓRIO — Escolha do Comando por Tipo de Query
- QUERY conceitual ampla, `recall` com `--k 5`
- QUERY mista de tokens e linguagem natural, `hybrid-search` com `--rrf-k 60`
- QUERY mista com contexto de grafo, `hybrid-search --with-graph --max-hops 2`
- QUERY exploratória partindo de memória, `related --hops 2`
- QUERY exploratória partindo de entidade, `graph traverse --depth 2`
- QUERY de auditoria do grafo, `graph entities` ou `graph stats`
### PROIBIDO — Anti-padrões de Pesquisa
- NUNCA usar busca textual nativa SQLite paralela ao binário
- NUNCA confundir `distance` com `combined_score` no ranking
- NUNCA aumentar `--hops` sem inspecionar `graph stats` antes
- NUNCA injetar resultados sem filtrar por threshold de relevância
- NUNCA paralelizar buscas pesadas sem medir RSS do host
- NUNCA pular camada 2 quando o snippet for insuficiente
- NUNCA ler apenas `.results[]` quando `--with-graph` ativo (perderá `graph_matches[]`)
### Padrão Correto — Pipeline Canônico de Três Camadas
- `sqlite-graphrag hybrid-search "auth jwt design" --k 10 --json | jaq -r '.results[].name' > seeds.txt`
- `while read -r nome; do sqlite-graphrag read --name "$nome" --json; done < seeds.txt > corpos.ndjson`
- `sqlite-graphrag related "$(head -n1 seeds.txt)" --hops 2 --json > grafo.json`
- `paste -d '\n' corpos.ndjson <(cat grafo.json) | claude --print`
### Padrão Correto — Pipeline com Expansão de Grafo
- `sqlite-graphrag hybrid-search "auth" --k 5 --with-graph --json | jaq -r '(.results[], .graph_matches[]) | .name' | sort -u > seeds.txt`
### Padrão Correto — Ajuste Fino de Pesos no hybrid-search
- `--weight-vec 1.0 --weight-fts 1.0` igual peso, padrão recomendado
- `--weight-vec 1.0 --weight-fts 0.0` reproduz baseline recall puro
- `--weight-vec 0.0 --weight-fts 1.0` reproduz FTS5 puro
- `--weight-vec 0.7 --weight-fts 0.3` favorece semântica sobre tokens
- `--weight-vec 0.3 --weight-fts 0.7` favorece tokens sobre semântica
### Ganhos Mensurados do Padrão de Três Camadas
- REDUÇÃO de tokens de contexto em até 72x versus dump de markdown
- AUMENTO de accuracy em até 18% sobre vector retrieval puro
- AUMENTO de multi-hop accuracy de 30% a 50% segundo Microsoft
- LATÊNCIA aproximada de 1-3 segundos em hardware moderno (subprocesso LLM one-shot)


## Grafo — Construção e Inspeção
### OBRIGATÓRIO — Criação de Arestas (link)
- USAR `link --from <a> --to <b> --relation <tipo>`
- ENTIDADES devem existir como nós tipados antes do link, exceto com `--create-missing`
- USAR `--create-missing` para auto-criar entidades inexistentes durante o link
- USAR `--entity-type <tipo>` para definir tipo das entidades auto-criadas (padrão `concept`)
- JSON response inclui `created_entities: ["a", "b"]` quando entidades foram criadas
- ACEITAR `--source`/`--target` como aliases de `--from`/`--to`
- DEFINIR `--weight` opcional para peso da relação (padrão 0.5)
- TRATAR exit code 4 como entidade inexistente (sem `--create-missing`)
- USAR `--strict-relations` para falhar com exit 1 quando um tipo de relação não canônico for usado; resposta inclui campo `warnings` listando relações não canônicas quando não estiver no modo estrito
- USAR `--max-entity-degree N` para emitir `tracing::warn!` quando criação de aresta empurraria uma entidade acima de N conexões (v1.0.65, também disponível no `remember`)
### OBRIGATÓRIO — Exportação com graph
- EXPORTAR snapshot via `graph --format json`
- USAR `--format dot` para Graphviz offline
- USAR `--format mermaid` para embutir em Markdown
- GRAVAR direto em arquivo via `--output <PATH>`
- INSPECIONAR `nodes` e `edges` no JSON exportado
- EDGES referenciando entidades inexistentes são logadas via `tracing::warn!` e ignoradas desde v1.0.50
### OBRIGATÓRIO — Enumeração de Entidades (graph entities)
- USAR `graph entities --json` para listar todas as entidades
- ACESSAR via `jaq -r '.entities[].name'` (campo é `entities`, NÃO `items`)
- FILTRAR por `--entity-type <tipo>` quando necessário
- PAGINAR com `--limit` e `--offset`
- USAR antes de planejar travessias ou links em lote
- ORDENAR via `--sort-by degree|name|created_at` (padrão `name`)
- DEFINIR direção via `--order asc|desc` (padrão `asc`)
- RESPOSTA agora inclui campo `degree` por entidade (número de relacionamentos conectados)
### OBRIGATÓRIO — Estatísticas (graph stats)
- USAR `graph stats --json` antes de travessias caras
- INSPECIONAR `node_count`, `edge_count`, `avg_degree`, `max_degree`
- ESCOLHER profundidade de travessia baseada em densidade real
- DETECTAR isolamento de subgrafos antes de planejar buscas
### Vocabulário Canônico de Relações
- `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`
- `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- Tipos customizados de relação (ex.: `implements`, `tested-by`, `blocks`) são aceitos desde v1.0.49; valores não canônicos emitem `tracing::warn!`
### Tipos Válidos de Entidade
- `project`, `tool`, `person`, `file`, `concept`, `incident`
- `decision`, `memory`, `dashboard`, `issue_tracker`
- `organization`, `location`, `date`


## Qualidade do Grafo Dirigida por LLM
### OBRIGATÓRIO — Tabela de Mapeamento de Relações
- MAPEAR relações não canônicas para equivalentes canônicos antes de persistir
- `adds` mapeia para `causes` (criação implica causalidade)
- `creates` mapeia para `causes` (mesma lógica)
- `implements` mapeia para `supports` (implementação suporta um design)
- `blocks` mapeia para `contradicts` (bloqueio contradiz progresso)
- `tested-by` mapeia para `related` (teste é uma forma de relação)
- `part-of` mapeia para `applies-to` (parte se aplica ao todo)
- PREFERIR o valor canônico sobre strings customizadas para evitar ruído de `tracing::warn!`
- RELAÇÕES customizadas são aceitas mas canônicas geram melhor recall cross-memory
### OBRIGATÓRIO — Curadoria de Entidades
- EXTRAIR apenas conceitos específicos do domínio: projetos reais, ferramentas, pessoas, decisões, arquivos
- NUNCA criar entidades de stop words, artigos, pronomes ou verbos genéricos
- NUNCA criar entidades de UUIDs, hashes, timestamps ou números de linha
- NUNCA criar entidades de caracteres únicos ou abreviações de duas letras
- ESCOLHER entity_type deliberadamente: `concept` para ideias abstratas, `tool` para software, `decision` para escolhas arquiteturais, `project` para codebases, `person` para contribuidores, `file` para caminhos de fonte
- PREFERIR menos entidades de alta qualidade sobre muitas de baixo sinal
- DEDUPLICAR: buscar `graph entities --json` antes de criar para evitar quase-duplicatas como "auth" e "authentication"
### OBRIGATÓRIO — Curadoria de Relações
- `depends-on`: A não funciona sem B (dependência forte)
- `uses`: A utiliza B mas poderia substituí-lo (dependência suave)
- `supports`: A reforça ou viabiliza B (design sustentando implementação)
- `causes`: A dispara ou produz B (cadeia causal)
- `fixes`: A resolve um problema descrito em B (correção de bug, resolução de incidente)
- `contradicts`: A conflita com ou invalida B (designs concorrentes, bloqueios)
- `applies-to`: A é relevante para ou tem escopo dentro de B (regra se aplica a módulo)
- `follows`: A vem depois de B em sequência ou prioridade (ordenação de workflow)
- `replaces`: A substitui B (migração, depreciação)
- `tracked-in`: A é monitorado ou gerenciado em B (issue em tracker, métrica em dashboard)
- `related`: A e B compartilham contexto mas nenhuma relação mais forte se aplica (usar com parcimônia, nunca como padrão)
- `mentions`: A referencia B sem implicar relacionamento (usar APENAS para citações, nunca como catch-all)
- ATRIBUIR `strength` baseado em acoplamento: 0.9 para dependências fortes, 0.7 para relações de design, 0.5 para links contextuais, 0.3 para referências fracas
### OBRIGATÓRIO — Enrichment de Descrições
- DESCRIÇÕES genéricas como "ingested from docs/README.md" desperdiçam o campo description
- ATUALIZAR via `edit --name <nome> --description "resumo semântico conciso"`
- BOA descrição responde: sobre o que é esta memória e POR QUE ela importa?
- RUIM: "ingested from auth.md" → BOM: "JWT token rotation strategy with 15-min expiry and refresh flow"
- RUIM: "user feedback" → BOM: "user prefers single bundled PR over many small ones for refactors"
- LIMITAR a uma frase, 10-20 palavras, focando no insight único
- EXECUTAR `list --type <tipo> --json | jaq '.items[] | select(.description | test("ingested|imported|added")) | .name'` para encontrar descrições genéricas
- ENRIQUECIMENTO em lote: encaminhar nomes para loop chamando `edit --description` para cada
### OBRIGATÓRIO — Workflow de Melhoria de Qualidade do Grafo
- PASSO 1 — Auditar: `graph stats --json` para medir node_count, edge_count, avg_degree
- PASSO 2 — Identificar ruído: `list --json | jaq '.items[] | select(.description | test("ingested|imported")) | .name'`
- PASSO 3 — Enriquecer descrições: `edit --name <nome> --description "resumo semântico"`
- PASSO 4 — Podar relações de baixo sinal: `prune-relations --relation mentions --dry-run --json`
- PASSO 5 — Executar poda: `prune-relations --relation mentions --yes --json`
- PASSO 6 — Limpar órfãos: `cleanup-orphans --yes --json`
- PASSO 7 — Verificar: `health --json | jaq '.integrity_ok'`
- AGENDAR este workflow após operações `ingest` em massa
### PROIBIDO — Anti-padrões de LLM no Grafo
- NUNCA usar `mentions` como relação padrão; adiciona ruído sem sinal
- NUNCA criar entidades de detalhes de implementação (nomes de variáveis, números de linha, hashes de commit)
- NUNCA definir todos os strengths como 1.0; diferenciar níveis de acoplamento
- NUNCA deixar descrições "ingested from" sem enriquecimento
- NUNCA criar edges redundantes (se A depends-on B, não adicionar também A uses B)
- NUNCA persistir estado efêmero (branch atual, progresso WIP, workarounds temporários)
- NUNCA pular deduplicação; buscar `hybrid-search` ou `graph entities` antes de criar


## Nota de Arquitetura — Sem Daemon (v1.0.76; código deletado na v1.0.79)
### NOTA — Infraestrutura do Daemon Totalmente Removida
- A infraestrutura de IPC do daemon (`sqlite-graphrag daemon`, `daemon --ping`, `daemon --stop`) foi totalmente removida na v1.0.76
- A CLI agora é 100% one-shot: cada operação de embedding spawna um subprocesso headless `claude -p` ou `codex exec` via OAuth
- Não há servidor de modelo em memória nem cache quente entre invocações
- Latência por chamada de embedding é 1-3 segundos (round-trip LLM)
- Nenhuma feature do Cargo restaura o daemon — ele foi permanentemente removido


## Nota de Arquitetura — Sem Cache de Modelo Local (v1.0.76)
### NOTA — Comandos de Cache Removidos
- Os comandos `cache list` e `cache clear-models` foram removidos na v1.0.76
- Não há cache de modelo ONNX local no build padrão LLM-only
- Saúde dos embeddings é verificada via `health --json` (checar `integrity_ok`) e `stats --json`


## Contrato JSON e Pipelines
### OBRIGATÓRIO — Saída Determinística
- USAR `--json` em todos os subcomandos antes de piping
- PREFERIR `--json` sobre `--format json` em one-liners
- FILTRAR campos via `jaq` em vez de regex sobre stdout
- LER apenas campos efetivamente retornados pelo subcomando
- TRATAR JSON como API versionada por SemVer
### OBRIGATÓRIO — Contrato JSON de Erros (v1.0.56, atualizado v1.0.68)
- TODOS os caminhos de erro agora emitem um objeto JSON no stdout: `{"error": true, "code": N, "message": "..."}`
- stderr ainda recebe o erro legível por humanos com prefixo descritivo
- CONSUMIDORES devem verificar o JSON do stdout primeiro (procurar `"error": true`), depois usar o exit code como fallback
- Aplica-se a TODOS os comandos quando `--json` é passado; sem `--json`, erros vão apenas para stderr
- Desde a v1.0.68 o envelope `code: 75` tem DOIS templates distintos — ambos mapeiam para o mesmo exit code: template A `job <job_type> for namespace '<namespace>' is already running (exit 75); wait for it to finish or pass --wait-job-singleton <SECONDS>` (emitido por `enrich`, `ingest --mode claude-code`, `ingest --mode codex` quando outra invocação segura o singleton), e template B `all <max> concurrency slots occupied after waiting <waited_secs>s (exit 75); use --max-concurrency or wait for other invocations to finish` (exaustão de semáforo legada)
### OBRIGATÓRIO — Matriz --json versus --format json
- `--json` é aceito por TODOS os subcomandos
- `--format json` aceito apenas em subset com `--format`
- QUANDO ambos presentes, `--json` vence em conflito
- USAR `--json` por padrão em pipelines portáteis
### OBRIGATÓRIO — Distinção Entre JSON e NDJSON
- COMANDOS individuais emitem JSON envelope único no stdout
- `ingest` emite NDJSON, uma linha JSON por arquivo mais summary no stdout
- CONSUMIR NDJSON via `jaq -c` ou `while read -r linha`
- AGREGAR NDJSON em array via `jaq -s` quando necessário
### OBRIGATÓRIO — Campos Críticos por Comando
- `recall` retorna `results[].name`, `snippet`, `distance`, `score`, `source` (`"direct"`/`"graph"`), `graph_depth?`
- `recall` response-level: `query`, `k`, `direct_matches[]`, `graph_matches[]`, `results[]`, `elapsed_ms`
- `hybrid-search` retorna `results[].name`, `combined_score`, `score`, `vec_rank`, `fts_rank`, `source`, `body`
- `hybrid-search` response-level: `query`, `k`, `rrf_k`, `weights`, `results[]`, `graph_matches[]`, `elapsed_ms`
- `hybrid-search` `graph_matches[]` usa RecallItem: `name`, `distance`, `source` ("graph"), `graph_depth`
- `related` retorna `results[].name`, `hop_distance`, `relation`, `source_entity`, `target_entity`, `weight`
- `graph traverse` retorna `hops[].entity`, `relation`, `direction`, `weight`, `depth`
- `read` retorna `name`, `body`, `description`, `created_at_iso`, `updated_at_iso`
- `edit` retorna `memory_id`, `name`, `action` ("updated"), `version`, `elapsed_ms`
- `rename` retorna `memory_id`, `name` (novo), `action` ("renamed"), `version`, `elapsed_ms`
- `forget` retorna `action` (`"soft_deleted"`/`"already_deleted"`), `forgotten`, `name`, `namespace`, `elapsed_ms`
- `list` response-level: `items[]`, `elapsed_ms`; cada item tem `id`, `memory_id`, `name`, `namespace`, `type`, `memory_type`, `description`, `snippet`, `updated_at`, `updated_at_iso`, `deleted_at?`, `deleted_at_iso?`
- `export` por linha: `name`, `type`, `memory_type`, `description`, `body`, `namespace`, `created_at_iso`, `updated_at_iso`, `deleted_at_iso?`; linha summary: `summary` (true), `exported`, `namespace`, `elapsed_ms`
- `health` retorna `integrity_ok`, `schema_ok`, `vec_memories_ok`, `vec_entities_ok`, `vec_chunks_ok`, `fts_ok`, `model_ok`, `counts`, `wal_size_mb`, `journal_mode`, `db_path`, `db_size_bytes`, `checks[]`
- `health.counts` contém: `memories`, `entities`, `relationships`, `vec_memories`
- `health` opcionalmente retorna `mentions_ratio` (float) e `mentions_warning` (string) quando mentions excedem 50% dos relacionamentos
- `health` agora inclui `fts_query_ok` (bool) indicando se uma query FTS5 ao vivo teve sucesso (além da integridade de schema), e `sqlite_version` (string) com a versão do SQLite em uso
- `stats` retorna dados GLOBAIS (sem filtro por namespace): `memories`, `entities`, `relationships`, `chunks_total`, `avg_body_len`, `namespaces[]`, `db_size_bytes`, `schema_version`, `elapsed_ms`; também inclui aliases legados `db_bytes`, `edges`, `memories_total`, `entities_total`, `relationships_total`
- `ingest` por arquivo: `file`, `name`, `status` (`"indexed"`/`"skipped"`/`"failed"`), `truncated`, `original_name?`, `original_filename?`, `memory_id?`, `action?`, `error?`
- `ingest` summary: `summary` (true), `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- `ingest --mode claude-code` phase: `phase` (`"validate"`/`"scan"`), `claude_path?`, `version?`, `dir?`, `files_total?`, `files_new?`, `files_existing?`
- `ingest --mode claude-code` por arquivo: `file`, `name`, `status` (`"done"`/`"failed"`/`"preview"`), `memory_id?`, `entities?`, `rels?`, `cost_usd?`, `elapsed_ms?`, `error?`, `index`, `total`
- `ingest --mode claude-code` summary: `summary` (true), `files_total`, `completed`, `failed`, `skipped`, `entities_total`, `rels_total`, `cost_usd`, `elapsed_ms`
- NOTA: `cache list` e `cache clear-models` foram removidos na v1.0.76 (sem cache de modelo local no build LLM-only)
- `prune-relations` retorna `action` (`"pruned"`/`"dry_run"`), `relation`, `count`, `entities_affected`, `affected_entity_names?`, `namespace`, `elapsed_ms`
- `fts rebuild` retorna `action` ("rebuilt"), `rows_indexed`, `elapsed_ms`
- `fts check` retorna `action` ("checked"), `integrity_ok`, `detail?`, `elapsed_ms`
- `fts stats` retorna `total_rows`, `shadow_pages?`, `fts_functional`, `elapsed_ms`
- `backup` retorna `action` ("backed_up"), `source`, `destination`, `size_bytes`, `elapsed_ms`
- `delete-entity` retorna `action` ("deleted"), `entity_name`, `namespace`, `relationships_removed`, `bindings_removed`, `elapsed_ms`
- `reclassify` retorna `action` ("reclassified"), `count`, `description_updated?` (bool, presente quando `--description` aplicado), `namespace`, `elapsed_ms`
- `merge-entities` retorna `action` ("merged"), `sources[]`, `target`, `namespace`, `relationships_moved`, `entities_removed`, `elapsed_ms`
- `memory-entities` forward retorna `memory_name`, `entities[].{entity_id, name, entity_type}`, `count`, `elapsed_ms`
- `memory-entities` reverse (`--entity`) retorna `entity_name`, `memories[].{memory_id, name, description, memory_type}`, `count`, `elapsed_ms`
- `prune-ner` retorna `action` (`"pruned"`/`"dry_run"`/`"aborted"`), `bindings_removed`, `namespace`, `entity?`, `elapsed_ms`
- `link` retorna `action` ("linked"), `from`, `to`, `relation`, `weight`, `namespace`, `elapsed_ms`, `created_entities?` (array, com `--create-missing`), `warnings?` (array, com relação não canônica)
- `unlink` retorna `action` ("deleted"), `from_name`, `to_name`, `relation`, `relationships_removed`, `namespace`, `elapsed_ms`
- `rename-entity` retorna `action` ("renamed"), `old_name`, `new_name`, `entity_id`, `namespace`, `elapsed_ms`
- `deep-research` retorna `query`, `sub_queries[]` (`id`, `text`, `source`), `results[]` (`name`, `score`, `source` enum: knn/fts/hybrid/graph, `sub_query_ids`, `snippet`, `body?`, `hop_distance?`), `evidence_chains[]` (`from`, `to`, `path[]`, `total_weight`, `depth`, `sub_query_ids`), `graph_context?` (`entities[]` com `name`, `entity_type`, `degree`; `relationships[]` com `from`, `to`, `relation`, `weight`), `stats` (`sub_queries_total`, `sub_queries_completed`, `sub_queries_failed`, `sub_queries_timed_out`, `unique_memories_found`, `evidence_chains_found`, `elapsed_ms`)
- `reclassify-relation` retorna `action` ("reclassified"/"dry_run"), `from_relation`, `to_relation`, `count`, `merged_duplicates`, `namespace`, `elapsed_ms`
- `normalize-entities` retorna `action` ("normalized"/"dry_run"), `normalized_count`, `merged_count`, `namespace`, `elapsed_ms`
- `enrich` emite NDJSON: eventos de fase (`phase`, `operation`), eventos por item (`name`, `status`, `entities?`, `rels?`, `cost_usd?`, `elapsed_ms?`), resumo (`operation`, `completed`, `failed`, `skipped`, `cost_usd`, `elapsed_ms`)
- `health` também retorna `top_relation` (string?), `top_relation_ratio` (float?), `applies_to_ratio` (float?), `relation_concentration_warning` (string?) quando qualquer relação excede 40% das arestas (v1.0.65); `vec_memories_missing` (i64) e `vec_memories_orphaned` (i64) para diagnóstico de desync vetorial (v1.0.66)
- `health` retorna campos de detecção de super-hub (v1.0.67): `super_hub_count` (i64?), `super_hub_warning` (string?), `top_hub_entity` (string?), `top_hub_degree` (i64?), `hub_warning` (string?) quando entidades excedem threshold de grau; também `non_normalized_count` (i64?) e `normalization_warning` (string?) para auditoria de normalização de nomes de entidade
- `graph --format json` retorna `nodes[]` E `entities[]` (alias, v1.0.66); `edges[]`; `elapsed_ms`
- `list --json` retorna `items[]` E `memories[]` (alias, v1.0.66); cada item inclui `body_length`
- `graph entities --json` retorna `entities[]` com `id`, `name`, `entity_type`, `namespace`, `created_at`, `degree`, `description?` (v1.0.66)
- `edit` aceita `--type` para mudar tipo de memória sem recriar (v1.0.66)
- `remember-batch` emite NDJSON por item com `name`, `status`, `memory_id?`, `error?`, `elapsed_ms` mais uma linha de resumo (v1.0.67)


## Códigos de Saída e Estratégia de Retry
### OBRIGATÓRIO — Tratamento Completo de Exit Codes
- `0` igual sucesso, parsear stdout
- `1` igual validação (peso inválido, self-link, max-files excedido)
- `2` igual erro de parsing de argumento Clap (flag inválida, timezone inválido, argumento obrigatório ausente)
- `9` igual duplicata (memória já existe sem `--force-merge`); desde v1.0.51 também retornado quando a memória é soft-deleted — use `--force-merge` para restaurar e atualizar, ou `restore` para reviver
- `3` igual conflito de locking otimista, recarregar e repetir
- `4` igual entidade, memória ou versão não encontrada
- `5` igual erro de namespace (nome inválido ou conflito)
- `6` igual payload acima do limite de tamanho
- `10` igual erro de database, executar `vacuum` e `health`
- `11` igual falha de embedding (erro no subprocesso LLM ou falha de carregamento do modelo)
- `12` igual falha ao carregar extensão vetorial (histórico; `sqlite-vec` removido na v1.0.76)
- `13` igual falha parcial em batch, reprocessar apenas falhos
- `14` igual erro de I/O (arquivo inacessível, permissão, disco cheio)
- `15` igual banco ocupado (busy), ampliar `--wait-lock`
- `20` igual erro interno ou falha de serialização JSON
- `75` igual slots exauridos no ingest ou outro pesado OU `AppError::JobSingletonLocked` de `enrich`, `ingest --mode claude-code` ou `ingest --mode codex` desde a v1.0.68; o campo `message` embute `job_type` e `namespace` para parsing via regex `job '(\w+)'.*namespace '(\w+)'`
- `77` igual pressão de RAM, aguardar memória livre
### PROIBIDO — Anti-padrões de Erro
- NUNCA ignorar exit code não-zero como sucesso
- NUNCA reprocessar lote inteiro após exit 13
- NUNCA aumentar concorrência após receber 75 ou 77
- NUNCA tentar `restore` sem inspecionar `history` antes
- NUNCA culpar ambiguidade sem ler stderr primeiro
- NUNCA confundir exit 1 (validação) com exit 9 (duplicata)


## Concorrência e Recursos
### OBRIGATÓRIO — Controle de Carga
- INICIAR comandos pesados com `--max-concurrency 1`
- AUMENTAR apenas após medir RSS e swap do host
- RESPEITAR teto rígido de `2×nCPUs` em comandos pesados
- TRATAR `init`, `remember`, `ingest`, `recall`, `hybrid-search` como pesados
- AMPLIAR `--wait-lock <ms>` quando contenção for esperada
- LIMITAR ingestão paralela em CI para evitar rate limits da LLM
### OBRIGATÓRIO — Dois Eixos de Paralelismo no ingest
- `--max-concurrency` governa invocações CLI simultâneas
- `--ingest-parallelism` governa extract mais embed paralelos
- AJUSTAR ambos independentemente conforme RAM e CPU
- USAR `--low-memory` para forçar paralelismo unitário
- HONRAR `SQLITE_GRAPHRAG_LOW_MEMORY=1` em hosts restritos


## Gerenciamento FTS5 (v1.0.56)
### OBRIGATÓRIO — Comandos FTS5
- USAR `fts rebuild --json` para reconstruir completamente o índice full-text FTS5; response: `{action, rows_indexed, elapsed_ms}`
- USAR `fts check --json` para executar a integrity-check do FTS5; response: `{action, integrity_ok, detail, elapsed_ms}`
- USAR `fts stats --json` para inspecionar a saúde do FTS5; response: `{total_rows, shadow_pages, fts_functional, elapsed_ms}`
- EXECUTAR `fts rebuild` quando `hybrid-search` retornar `fts_degraded: true` ou após suspeita de corrupção do índice
- EXECUTAR `fts check` como parte das auditorias periódicas de saúde junto com `health --json`
- TRATAR `fts_functional: false` no `fts stats` como sinal para executar `fts rebuild`


## Backup Seguro (v1.0.56)
### OBRIGATÓRIO — Comando backup
- USAR `backup --output <caminho> --json` para backup seguro e online via SQLite Online Backup API
- BACKUP é consistente mesmo com escritas em andamento
- JSON response: `{action, source, destination, size_bytes, elapsed_ms}`
- PREFERIR `backup` sobre `sync-safe-copy` para backups programáticos; ambos são seguros, mas `backup` usa a API nativa do SQLite
- TRATAR exit code 14 como erro de I/O (destino não gravável, disco cheio)


## Operações de Entidade (v1.0.56)
### OBRIGATÓRIO — delete-entity
- USAR `delete-entity --name <entidade> --cascade --json` para remover uma entidade e todos seus relacionamentos e bindings de memória
- FLAG `--cascade` é obrigatória como portão de confirmação; sem ela o comando sai com erro de validação
- JSON response: `{action, entity_name, namespace, relationships_removed, bindings_removed, elapsed_ms}`
- EXECUTAR `cleanup-orphans` depois para remover entidades recém-órfãs
- TRATAR exit code 4 como entidade não encontrada
### OBRIGATÓRIO — rename-entity (v1.0.58)
- USAR `rename-entity --name <antigo> --new-name <novo> --json` para renomear entidade preservando todos os relacionamentos e vínculos
- RE-GERA o vetor da entidade com o novo nome para precisão na busca semântica
- JSON response: `{action: "renamed", old_name, new_name, entity_id, namespace, elapsed_ms}`
- TRATAR exit code 4 como entidade não encontrada; exit 1 se novo nome já existe ou falha na validação (menor que 2 caracteres, contém quebras de linha, ou abreviação ALL_CAPS curta)
- TODOS os relacionamentos e memory_entities usam FK inteiro e não são afetados pela mudança de nome
### OBRIGATÓRIO — reclassify
- USAR `reclassify --name <entidade> --new-type <tipo> --json` para alteração individual de tipo de entidade
- USAR `reclassify --from-type <antigo> --to-type <novo> --batch --json` para reclassificação em massa
- USAR `reclassify --name <entidade> --description "texto" --json` para atualizar descrição da entidade no modo individual (v1.0.58)
- COMBINAR `--new-type` com `--description` para alterar tipo e descrição em uma operação
- JSON response: `{action, count, description_updated?, namespace, elapsed_ms}`
- TRATAR count 0 no modo batch como indicação de que --from-type pode conter erro de digitação
### OBRIGATÓRIO — merge-entities
- USAR `merge-entities --names "a,b" --into <alvo> --json` para fundir entidades de origem em um alvo
- TODOS os relacionamentos dos nós de origem são redirecionados para o alvo via UPDATE OR IGNORE
- RELACIONAMENTOS duplicados são removidos automaticamente após redirecionamento
- JSON response: `{action, sources, target, namespace, relationships_moved, entities_removed, elapsed_ms}`
- TRATAR exit code 4 como entidade alvo não encontrada
### OBRIGATÓRIO — memory-entities
- USAR `memory-entities --name <memória> --json` para listar todas entidades vinculadas a uma memória específica
- USAR `memory-entities --entity <nome-entidade> --json` para listar todas memórias vinculadas a uma entidade (busca reversa, v1.0.58)
- RESPOSTA direta: `{memory_name, entities: [{entity_id, name, entity_type}], count, elapsed_ms}`
- RESPOSTA reversa: `{entity_name, memories: [{memory_id, name, description, memory_type}], count, elapsed_ms}`
- TRATAR exit code 4 como memória/entidade não encontrada; exit 0 com count 0 significa que existe mas sem vínculos
- USAR busca reversa antes de rename-entity ou delete-entity para avaliação de impacto
### OBRIGATÓRIO — prune-ner
- USAR `prune-ner --entity <nome> --dry-run --json` para pré-visualizar remoção de bindings NER
- USAR `prune-ner --entity <nome> --yes --json` para remover bindings NER de uma única entidade
- USAR `prune-ner --all --yes --json` para remover TODOS os bindings NER no namespace
- JSON response: `{action, bindings_removed, namespace, entity, elapsed_ms}`
- EXECUTAR `cleanup-orphans` depois para remover nós de entidade sem bindings restantes


## Manutenção e Backup
### OBRIGATÓRIO — Higiene Periódica
- AGENDAR `purge --retention-days 30 --yes` semanalmente
- EXECUTAR `vacuum` após purges grandes
- RODAR `optimize` para refrescar estatísticas do planner
- LIMPAR órfãos via `cleanup-orphans --yes` após forget em massa
### OBRIGATÓRIO — Backup Seguro
- DESDE v1.0.53, todo comando de escrita executa `PRAGMA wal_checkpoint(TRUNCATE)` após commit, garantindo que o arquivo `.sqlite` esteja sempre autocontido quando ferramentas de cloud sync (Dropbox, iCloud, OneDrive) o leem
- USAR `sync-safe-copy --dest <path>` para snapshots atômicos antes de operações críticas
- COMPRIMIR snapshots via `ouch compress` para upload remoto
- EXPORTAR memórias via `sqlite-graphrag export` como NDJSON (uma linha JSON por memória + summary); suporta `--namespace`, `--type`, `--include-deleted`, `--limit`
- VERSIONAR banco com Git LFS quando viável
- SE ocorrer corrupção apesar do checkpoint, recuperar com `sqlite3 corrompido.sqlite ".recover" | sqlite3 reparado.sqlite`
### OBRIGATÓRIO — Diagnóstico de Schema
- USAR `debug-schema --json` para troubleshooting
- INSPECIONAR `schema_version`, `objects`, `migrations`
- VERSÃO atual do schema é 13 (V013 descarta vec virtual tables e cria tabelas de embedding BLOB-backed; V012 adicionou timestamps de relacionamento)
- COMANDO oculto do `--help`, invocar pelo nome exato
### Padrão Correto — Cron Semanal
- `sqlite-graphrag purge --retention-days 30 --yes`
- `sqlite-graphrag cleanup-orphans --yes`
- `sqlite-graphrag prune-relations --relation mentions --yes` (quando edges geradas por NER precisam de limpeza)
- `sqlite-graphrag vacuum --json`
- `sqlite-graphrag optimize --json`
- `sqlite-graphrag sync-safe-copy --dest ~/Dropbox/graphrag.sqlite`


## Novidades na v1.0.76 — LLM-Only One-Shot (G21 + G22 + G23 + G24 + G25)
### OBRIGATÓRIO — Mudança Arquitetural do Build Padrão
- O build padrão da v1.0.76 é LLM-Only e one-shot. Não há daemon, não há runtime ONNX, não há download do modelo `multilingual-e5-small`. A geração de embeddings e a NER delegam para um subprocesso headless `claude code` ou `codex` (OAuth, sem MCP, sem hooks). O binário de release tem aproximadamente 6 MB.
- O build padrão é LLM-only sem dependências de modelo local
- Veja ADR-0019 para a justificativa arquitetural completa, ADR-0021 para o cronograma de depreciação do daemon, ADR-0022 para as tabelas de embedding com backing BLOB, ADR-0023 para a remoção do tokenizer, ADR-0024 para o caminho de busca FTS5 como filtro grosso + refinamento por cosseno, e ADR-0025 para o fluxo de credencial exclusivamente OAuth reafirmado.
### OBRIGATÓRIO — Família do Subcomando migrate
- USAR `migrate --rehash --json` para reescrever os checksums registrados de migração via `SipHasher13(name|version|sql)` de modo que o algoritmo case com `refinery-core 0.9.1`. A mesma crate SipHasher13 e a mesma ordem de hashing são usadas. Schema de resposta: `migrate-rehash.schema.json`.
- USAR `migrate --to-llm-only --drop-vec-tables --json` como o upgrade one-shot para bancos v1.0.74 / v1.0.75. Combina a reescrita de checksum (--rehash) com a migração V013 de descarte das vec-tables e reporta o estado das vec-tables. A flag `--drop-vec-tables` é OBRIGATÓRIA como rede de segurança. Schema de resposta: `migrate-to-llm-only.schema.json`.
- Após `migrate --to-llm-only`, os embeddings são recomputados de forma preguiçosa no próximo `remember` / `edit` / `ingest`. Operadores que desejam pré-aquecer um corpus grande podem fazer loop `edit --description "<mesmo>"` sobre `list --json | jaq -r '.items[].name'`.
- A migração V002 foi intencionalmente esvaziada para um no-op na v1.0.76; essa é a causa raiz do descasamento `applied migration V2 is different than filesystem one V2` que `migrate --rehash` repara. Veja ADR-0026 para a narrativa completa do drift.
### OBRIGATÓRIO — Versão de Schema e Embeddings com Backing BLOB
- A versão atual de schema é 13. A migração V013 descarta as virtual tables `vec_memories`, `vec_entities` e `vec_chunks` e as substitui pelas tabelas regulares com backing BLOB `memory_embeddings`, `entity_embeddings` e `chunk_embeddings`. A similaridade por cosseno é computada em Rust puro sob demanda em `src/similarity.rs` (ADR-0020, ADR-0022).
- A hybrid-search ainda usa FTS5 como filtro grosso e agora refina o conjunto de candidatos com cosseno em Rust puro sobre os embeddings BLOB. O FTS5 permanece saudável porque a reconstrução é bloqueada por `optimize --fts-skip-when-functional` (G36 da v1.0.69).
- A infraestrutura do daemon foi totalmente removida na v1.0.76. O subprocesso LLM é o novo "model loader" — cada chamada spawna um processo headless.
### OBRIGATÓRIO — Apenas OAuth Reafirmado
- O mandato OAuth-only da v1.0.69 é REAFIRMADO. O spawn ABORTA com `AppError::Validation` se `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estiverem definidas no ambiente. Ambas as variáveis são excluídas da whitelist de env-clear como defesa em profundidade.
- Nova flag global `--extraction-backend llm|none` (padrão `llm`) seleciona o backend de extração. `llm` é o caminho LLM; `none` é um no-op
### PROIBIDO — Antipadrões da v1.0.76
- NUNCA instale a v1.0.76 com `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` no ambiente; o spawn aborta.
- NUNCA dependa do daemon em código novo; o daemon foi totalmente removido (código deletado na v1.0.79).
- NUNCA misture queries em `vec_memories` / `vec_entities` / `vec_chunks` (removidas na v1.0.76); use `memory_embeddings` / `entity_embeddings` / `chunk_embeddings` no lugar.
- NUNCA use `migrate --to-llm-only` sem `--drop-vec-tables`; a rede de segurança recusa a operação caso contrário.


## Completions de Shell (v1.0.67)
### OBRIGATÓRIO — Comando completions
- USAR `completions <shell>` para gerar scripts de completion de shell
- SHELLS suportados: `bash`, `zsh`, `fish`, `elvish`, `powershell`
- REDIRECIONAR saída para arquivo de configuração do shell
### Padrão Correto — Exemplos de completions
- `sqlite-graphrag completions bash > ~/.local/share/bash-completion/completions/sqlite-graphrag`
- `sqlite-graphrag completions zsh > ~/.zfunc/_sqlite-graphrag`
- `sqlite-graphrag completions fish > ~/.config/fish/completions/sqlite-graphrag.fish`
