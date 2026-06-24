---
name: sqlite-graphrag
description: Esta skill DEVE ativar para operações da CLI sqlite-graphrag incluindo memória persistente, GraphRAG, grafo de entidades, busca híbrida, recall, remember, ingest, enrich, deep-research, embedding LLM, seleção de backend (codex claude opencode), enforcement OAuth-only, validação preflight, FTS5, similaridade cosine BLOB-backed, isolamento de CWD, gestão de namespace, migração, manutenção. Ativa em palavras-chave memória RAG GraphRAG SQLite one-shot OAuth offline persistente grafo entidade embedding codex claude opencode remember recall hybrid-search ingest enrich forget purge link
---


## Quando Esta Skill Ativa
- ATIVE quando o usuário pede para lembrar, salvar, recordar, recuperar, buscar ou persistir algo entre sessões
- ATIVE para contexto de longo prazo, grafo de conhecimento, GraphRAG, RAG, ligação de entidades, gestão de memória
- ATIVE quando sqlite, sqlite-graphrag, embedding, FTS5, hybrid-search ou memória LLM for mencionado
- NUNCA ATIVE para dados efêmeros pontuais, I/O simples de arquivo ou tarefas sem relação a contexto persistente


## Regras de Instrução para LLMs (Prompt Rules)
- QUANDO "lembre disso", "salve isso" → EXECUTE `remember --force-merge` com `--graph-stdin` incluindo entidades curadas e relações canônicas
- QUANDO "o que sabe sobre X", "recall X" → EXECUTE `hybrid-search "X" --k 10 --json` PRIMEIRO, EXPANDA top com `read --name <nome> --json`
- QUANDO "como X se relaciona com Y" → EXECUTE `graph traverse --from X --depth 2 --json` ou `related X --hops 2 --json`
- QUANDO "pesquise profundamente sobre X" → EXECUTE `deep-research "X" --k 20 --max-hops 3 --json`
- ANTES de criar memória → EXECUTE `hybrid-search "<nome>" --k 5 --json` para VERIFICAR duplicatas; se encontrar, USE `--force-merge`
- APÓS criar/atualizar memória → VERIFIQUE com `read --name <nome> --json | jaq '{name, description, body_length}'`
- APÓS CADA turno com achados novos → AVALIE persistência via `remember --force-merge`; se nada novo, DECLARE "Nenhum achado novo"
- QUANDO exit code não-zero → LEIA envelope JSON via `jaq '{code, message, error_class}'`, REPORTE remediação
- QUANDO exit 9 (duplicada) → RETENTE com `--force-merge`
- QUANDO exit 19 (SHUTDOWN) → RETRY OBRIGATÓRIO; trabalho parcial descartado
- QUANDO exit 75 (singleton) → AGUARDE e retente; NUNCA aumente concorrência
- QUANDO exit 16 (preflight) → CORRIJA config MCP; NUNCA bypass com `SKIP_PREFLIGHT`
- SEMPRE parseie saída JSON com `jaq` (NUNCA `jq`)
- SEMPRE passe flag `--json` em toda invocação de `sqlite-graphrag`
- SEMPRE use `--llm-backend codex --llm-model gpt-5.4-mini` OU `--llm-backend claude --llm-model claude-sonnet-4-6` OU `--llm-backend opencode --llm-model opencode/big-pickle` para comandos de embedding
- SEMPRE use APENAS relações canônicas: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- SEMPRE mapeie não-canônicas: `adds|creates → causes`, `implements → supports`, `blocks → contradicts`, `tested-by → related`, `part-of → applies-to`
- SEMPRE normalize nomes de entidade para kebab-case ASCII lowercase ANTES de passar à CLI
- NUNCA use MCP Serena ou `.md` para persistência; NUNCA escreva MEMORY.md
- NUNCA inicie daemon (REMOVIDO); NUNCA passe `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY`
- PREFIRA `remember --force-merge` sobre `edit`; PREFIRA `--graph-stdin` sobre `--enable-ner`
- LIMITE entidades a conceitos de domínio; REJEITE palavras genéricas, pronomes, UUIDs, timestamps


## Arquitetura e Princípios
- INVOKE sempre como subprocesso; READ stdout para JSON/NDJSON; READ stderr para logs; CHECK exit code ANTES de parsear
- SAIBA que BUILD é LLM-only one-shot; binário NÃO tem daemon, NÃO tem ONNX runtime, NÃO tem cache de modelo
- SAIBA que similaridade COSINE é pure Rust sobre BLOB-backed `memory_embeddings`, `entity_embeddings`, `chunk_embeddings`
- SAIBA que SCHEMA é v15 após `init` ou `migrate` em banco fresco
- ENFORCE OAUTH-ONLY: spawn ABORTA exit 1 se `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estiver definida
- SAIBA que `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL` são PRESERVADAS para providers customizados (OpenRouter, Bedrock)
- SAIBA que flags de endurecimento são SEMPRE passadas para subprocessos `claude -p` e `codex exec`
- SAIBA que CWD do subprocesso é ISOLADO via `apply_cwd_isolation`; `CLAUDE_CONFIG_DIR` definido para dir de isolamento; órfãos limpos via `cleanup_isolation_dirs`
- SAIBA que 7 guards preflight rodam ANTES de cada fork LLM: `check_argv_size`, `check_binary_exists`, `check_mcp_config_inline`, `check_mcp_config_path`, `check_walkup_mcp_json`, `check_output_buffer`, `check_claude_config_dir`
- SAIBA que exit 16 (`EX_CONFIG`) é falha preflight universal; LEIA envelope para remediação por variante
- DEFINA `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` APENAS em emergências
- ISOLE NAMESPACE por projeto via `--namespace <ns>` ou env; default é `global`
- NUNCA exponha o binário como servidor MCP ou serviço HTTP
- NUNCA escreva arquivo `.sqlite` em paralelo ao binário ou de outra ferramenta
- USE MOCK LLM CLI para CI: prefixe `tests/mock-llm` ao PATH


## Seleção de Backend LLM
- PASSE `--llm-backend codex` para spawnar Codex CLI headless (backend DEFAULT)
- PASSE `--llm-backend claude` para spawnar Claude Code headless via `embed_via_claude_local` (zero-token, compatível com OAuth)
- PASSE `--llm-backend opencode` para spawnar OpenCode CLI headless (sistema de auth próprio, NÃO OAuth)
- PASSE `--llm-backend codex,claude` para codex-primeiro com fallback claude
- PASSE `--llm-backend codex,claude,opencode,none` para cadeia completa de fallback com embedding null como último recurso
- PASSE `--llm-model <MODEL>` para selecionar modelo de embedding para o backend ativo
- SAIBA modelos DEFAULT: codex=`gpt-5.5`, claude=`claude-sonnet-4-6`, opencode=`opencode/big-pickle`
- PASSE `--llm-fallback-mode <claude|codex|opencode>` para trocar backend mid-job em rate-limit
- PASSE `--skip-embedding-on-failure` APENAS quando `--llm-backend …,none` está ativo
- PASSE `--dry-run-backend` para planejar operação de backend sem executar (preview idempotente)
- PARSEE campo `backend_invoked` em todo envelope de embedding para CONFIRMAR qual backend rodou
- PASSE `--codex-binary <PATH>`, `--claude-binary <PATH>`, `--opencode-binary <PATH>` para sobrescrever localização dos binários
- PASSE `--opencode-model <MODEL>` e `--opencode-timeout <SECONDS>` para ajustes específicos do opencode
- PASSE `--mode codex|claude-code|opencode` para pipelines de extração em ingest e enrich
- SAIBA que output NDJSON do opencode tem 3 tipos de evento: `step_start`, `text`, `step_finish`
- SAIBA modelos gratuitos opencode: `opencode/big-pickle`, `opencode/deepseek-v4-flash-free`, `opencode/mimo-v2.5-free`, `opencode/nemotron-3-ultra-free`, `opencode/north-mini-code-free`
- EXECUTE `codex login` para refrescar OAuth do codex; refresque OAuth do claude quando stale
- NUNCA passe API keys com qualquer backend; spawn ABORTA exit 1


## Referência de Flags Globais
- `--db <PATH>` — sobrescrever localização do banco (NÃO é global; cada subcomando aceita independentemente)
- `--namespace <ns>` — escopar operações para um namespace
- `--lang en|pt` — forçar idioma do stderr
- `--tz <TIMEZONE>` — localizar timestamps
- `--json` — saída JSON estruturada (SEMPRE passe)
- `--low-memory` — paralelismo unitário para containers restritos
- `--max-concurrency N` — cap de invocações CLI pesadas concorrentes
- `--wait-lock SECS` — ampliar janela de aquisição de lock
- `--llm-parallelism N` — cap de fan-out de subprocessos de embedding (default 4, clamp [1, 32])
- `--llm-backend <chain>` — seleção de backend com fallback separado por vírgula
- `--llm-model <MODEL>` — modelo de embedding para backend ativo
- `--dry-run-backend` — planejar operação de backend sem executar
- `--llm-fallback-mode <backend>` — trocar backend mid-job em rate-limit
- `--llm-fallback <chain>` — cadeia de fallback comma-separated tentada quando primário falha (default `codex,claude,none`)
- `--llm-slot-no-wait` — falhar imediatamente exit 75 quando nenhum slot LLM livre (em vez de esperar)
- `--embedding-dim N` — override de dimensionalidade de embedding [8, 4096] (default 64 MRL)
- `--graceful-shutdown-secs N` — orçamento de cleanup antes de SIGKILL
- `--skip-embedding-on-failure` — exit 0 em falha de embedding (APENAS com fallback terminando em `none`)
- `--strict-env-clear` — preservar apenas `PATH` em subprocesso para compliance
- `--codex-binary`, `--claude-binary`, `--opencode-binary` — sobrescrever paths dos binários
- `--opencode-model`, `--opencode-timeout` — overrides específicos do opencode
- `-v`/`-vv`/`-vvv` — logging info/debug/trace no stderr


## CRUD Escrita (remember, remember-batch, ingest)
- INVOKE `remember --name <kebab> --type <kind> --description <text>` com `--body <text>` ou `--body-file <path>` ou `--body-stdin`
- INVOKE `remember --graph-stdin` para anexar `{body, entities, relationships}` em único JSON
- PASSE entities como `[{name, entity_type}]` em kebab-case ASCII
- PASSE relationships como `[{source, target, relation, strength}]` onde `strength em [0.0, 1.0]`
- PASSE `--force-merge` para updates idempotentes e restauração de soft-deleted
- PASSE `--clear-body` para limpar corpo durante update com `--force-merge`
- PASSE `--dry-run` para validar inputs sem persistir
- PASSE `--max-rss-mb <MiB>` para abortar quando RSS exceder threshold (default 8192)
- RESPEITE limite de 512000 bytes e 512 chunks por corpo
- VALORES válidos de `--type`: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- USE `--enable-ner` para extração de entidades URL-regex (APENAS URL-regex desde remoção do NER)
- INVOKE `remember-batch` para 10+ memórias via NDJSON stdin; ESPERE status por item e linha de sumário
- INVOKE `ingest <DIR> --recursive --pattern "*.md"` para importar diretório
- PASSE `--type <kind>` para aplicar mesmo tipo a todos arquivos ingeridos
- PASSE `--mode codex|claude-code|opencode` para extração de entidades curada por LLM
- USE `--auto-describe` (default true) para extrair descrição da primeira linha do corpo; opt out via `--no-auto-describe`
- USE `--resume` para continuar da fila após interrupção; `--retry-failed` para apenas falhados
- USE `--fail-fast` para parar na primeira falha por arquivo
- USE `--max-name-length N` para sobrescrever truncamento padrão de nomes em 60 chars
- USE `--llm-parallelism N` em `ingest` (default 2); `--ingest-parallelism N` para paralelismo per-file
- PASSE `--claude-model <MODEL>` e `--claude-timeout <secs>` (default 300) para `--mode claude-code`
- PASSE `--codex-model <MODEL>` e `--codex-timeout <secs>` (default 300) para `--mode codex`
- PASSE `--rate-limit-wait <secs>` (default 60) para espera inicial em rate-limit com `--mode claude-code`
- PASSE `--queue-db <path>` para BD de fila customizado; `--keep-queue` para preservar após conclusão
- PASSE `--low-memory` em `ingest` para modo single-threaded (3-4x mais lento, <4 GB RAM)
- PASSE `--dry-run` em `ingest` para preview de mapeamento arquivo-para-nome sem persistir
- RESPEITE cap `--max-files 10000` como validação all-or-nothing
- NUNCA misture `--body`, `--body-file`, `--body-stdin`, `--graph-stdin` em única invocação
- NUNCA passe corpo vazio sem entities via `--graph-stdin`
- NUNCA use `fd | xargs remember`; INVOKE `ingest` em vez disso
- NUNCA use `--force-merge` em `ingest` (exclusivo de `remember`)


## CRUD Leitura, Atualização, Deleção
- INVOKE `read --name <kebab> --json` para fetch O(1); `read --id <N>` por memory_id; `--with-graph` para entidades vinculadas
- INVOKE `list --type <kind> --limit N --offset N --json`; `--include-deleted` para soft-deleted
- INVOKE `history --name <n> --diff --json` para versões com diff de caracteres
- INVOKE `edit --name <n> --body-file <path>` para atualizar corpo (re-embeda automaticamente)
- USE `--description <text>` para atualizar apenas descrição (sem re-embed)
- USE `--type <kind>` para mudar tipo de memória sem recriar
- USE `--force-reembed` para regenerar embedding sem mudar corpo
- USE `--expected-updated-at <ts>` para optimistic locking; TRATE exit 3 como conflito
- INVOKE `rename --from <old> --to <new>` para renomear preservando histórico
- INVOKE `restore --name <n> --version <N>` para restaurar versão anterior
- INVOKE `forget --name <n>` para soft-delete reversível; TRATE exit 4 como ausente
- INVOKE `purge --retention-days <N> --yes` para hard delete; USE `--dry-run` primeiro
- INVOKE `unlink --from <a> --to <b> --relation <type>` para remover aresta; `--entity <name> --all` para massa
- INVOKE `prune-relations --relation <type> --yes` para deleção em massa; `--show-entities --dry-run` para preview
- INVOKE `cleanup-orphans --yes` após bulk forget; depois `vacuum --json`
- NUNCA pule optimistic locking em pipelines concorrentes
- NUNCA delete manualmente via shell `sqlite3`


## Operações de Grafo de Entidades
- INVOKE `link --from <a> --to <b> --relation <type> --create-missing --weight <float>` para criar aresta
- PASSE `--entity-type <kind>` para entidades auto-criadas (default `concept`)
- PASSE `--max-entity-degree N` para avisar quando entidade exceder N conexões
- USE `--strict-relations` para falhar em tipos de relação não-canônicos
- INVOKE `graph entities --json` para listar entidades; ACESSE via `.entities[]` (NÃO `.items[]`)
- ORDENE via `--sort-by degree|name|created_at`; PAGINE via `--limit N --offset N`
- INVOKE `graph stats --json` para inspecionar `node_count`, `edge_count`, `avg_degree`, `max_degree`
- SAIBA que grau de entidade é calculado via query COUNT precisa (`recalculate_degree`)
- INVOKE `graph traverse --from <root> --depth <N> --json` para travessia de subgrafo
- USE `--format json|dot|mermaid` com `--output <path>` para exportar grafo
- INVOKE `memory-entities --name <memory>` para lookup forward; `--entity <name>` para reverso
- INVOKE `rename-entity`, `delete-entity --cascade`, `merge-entities --names "a,b,c" --into <target>`
- INVOKE `reclassify --name <n> --new-type <kind>` ou `--from-type <old> --to-type <new> --batch`
- INVOKE `reclassify-relation --from-relation <old> --to-relation <new> --batch` para migração em massa de tipos de relação
- INVOKE `normalize-entities --yes` para normalizar todos nomes para kebab-case ASCII
- INVOKE `prune-ner --entity <n>` para remover bindings NER; `prune-ner --all --yes` para todos no namespace
- VALIDE nomes de entidade: mínimo 2 chars, sem newlines, sem ALL_CAPS curtos (4 chars ou menos REJEITADOS)
- RELAÇÕES canônicas: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- TIPOS canônicos de entidade: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- NUNCA use `mentions` como relação padrão


## Busca GraphRAG (recall, hybrid-search, related, deep-research, enrich)
- USE padrão canônico de três camadas: `hybrid-search` depois `read --name` depois `related|graph traverse`
- INVOKE `recall <query> --k N` para busca semântica pura KNN; PASSE `--no-graph` para desabilitar expansão de grafo
- INTERPRETE `distance` crescente como similaridade decrescente; `score` = `1.0 - distance` clamped [0.0, 1.0]
- INVOKE `hybrid-search <query> --k N` para fusão FTS5+KNN via RRF
- PASSE `--rrf-k 60` para fusão padrão; `--weight-vec 1.0 --weight-fts 1.0` para balanceada
- PASSE `--type <kind>` para filtrar resultados por tipo de memória
- PASSE `--fallback-fts-only` para pular embedding ao vivo e servir apenas FTS5 BM25 (modo offline)
- USE `--with-graph --max-hops 2 --min-weight 0.3` para expansão de grafo; LEIA TANTO `results[]` QUANTO `graph_matches[]`
- INVOKE `related <name> --hops N` para travessia multi-hop a partir de memória
- INVOKE `deep-research "<query>" --k 20 --max-hops 3 --max-sub-queries 7 --max-results 50` para pesquisa paralela multi-hop
- PASSE `--graph-decay <float>` (default 0.7) para decaimento de score por hop; `--graph-min-score <float>` (default 0.05) para threshold mínimo
- PASSE `--max-neighbors-per-hop N` para limitar vizinhos por entidade por hop
- PASSE `--timeout <secs>` (default 30) para timeout por sub-query
- PASSE `--with-bodies` para incluir corpos completos de memórias nos resultados
- INVOKE `enrich --operation <op>` para qualidade de grafo via LLM: `memory-bindings`, `entity-descriptions`, `body-enrich`, `re-embed --limit N --resume`
- PASSE `--llm-parallelism N` para controlar subprocessos LLM concorrentes
- PASSE `--max-cost-usd N` para limitar custo acumulado de LLM (ignorado para usuários OAuth)
- USE `--dry-run` para preview sem spawnar LLM
- PARSEE top campos: `recall` retorna `results[].{name, snippet, distance, score, source}`; `hybrid-search` retorna `results[].{name, combined_score, vec_rank, fts_rank}`
- PARSEE `deep-research` retorna `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context`, `stats`
- NUNCA confunda `distance` com `combined_score` em ranking
- NUNCA aumente `--hops` sem inspecionar `graph stats` antes


## Códigos de Saída e Estratégia de Retry
- EXIT 0: sucesso; EXIT 1: erro de validação; EXIT 2: parsing de argumento; EXIT 3: conflito de lock otimista (recarregue e retente)
- EXIT 4: não encontrado; EXIT 5: erro de namespace; EXIT 6: payload grande demais; EXIT 9: duplicada (use `--force-merge`)
- EXIT 10: erro de banco (execute `vacuum` + `health`); EXIT 11: falha de embedding (verifique backend + OAuth)
- EXIT 13: falha parcial de batch (reprocesse apenas falhados); EXIT 14: erro de I/O; EXIT 15: banco ocupado (amplie `--wait-lock`)
- EXIT 16: falha preflight (corrija config MCP, NUNCA trate como transitório)
- EXIT 19: SHUTDOWN (RETRY OBRIGATÓRIO, trabalho parcial descartado); PARSEE envelope `{error, code, signal, graceful, message}`
- EXIT 20: erro interno; EXIT 75: slots esgotados ou job singleton locked (respeite cooldown, NUNCA retente imediatamente)
- EXIT 77: pressão de RAM (aguarde memória livre)
- NUNCA ignore exit não-zero; NUNCA reprocesse batch inteiro após exit 13; NUNCA confunda exit 1 com exit 9


## Concorrência e Paralelismo
- RESPEITE teto rígido `2 x nCPUs` para comandos pesados: `init`, `remember`, `ingest`, `recall`, `hybrid-search`
- DEFINA `--llm-parallelism N` default 4 em `remember`/`edit`, default 2 em `ingest` (clamp [1, 32])
- USE `--llm-max-host-concurrency N` para cap de subprocessos LLM cross-process
- USE `--llm-slot-wait-secs N` para esperar slot ou `--llm-slot-no-wait` para abortar
- SAIBA que JOB SINGLETON: `enrich`, `ingest --mode claude-code|codex|opencode` adquirem singleton por namespace
- USE `--wait-job-singleton SECS` ou `--force-job-singleton` para quebrar lock stale
- ATIVE `SQLITE_GRAPHRAG_LOW_MEMORY=1` para paralelismo unitário (3-4x mais lento)
- NUNCA rode `enrich` em paralelo contra mesmo banco


## Pipeline de Manutenção e Subcomandos de Diagnóstico
- EXECUTE `sqlite-graphrag init --namespace <ns>` no primeiro uso
- EXECUTE `health --json` para verificar `integrity_ok`, `schema_ok`, `schema_version >= 15`
- EXECUTE `migrate --dry-run --json` para preview; depois `migrate --json` após upgrade do binário
- EXECUTE `optimize --json` para refrescar estatísticas do planner; inclui `fts_rebuilt`
- EXECUTE `fts rebuild --json` quando `health.fts_degraded` for true; `fts check --json` para integridade; `fts stats --json` para contagens
- INVOKE `backup --output <path> --json` para backup online; `sync-safe-copy --dest <path>` para snapshot atômico
- INVOKE `export --namespace <ns> --type <kind> --json` para exportar como NDJSON
- INVOKE `vacuum --json` após purge grande; INSPECIONE `wal_size_mb` em health para fragmentação
- INVOKE `vec orphan-list --json` depois `vec purge-orphan --yes` para limpar vetores órfãos; `vec stats --json` para saúde
- INVOKE `debug-schema --json` para troubleshooting de drift de schema
- INVOKE `completions <bash|zsh|fish|elvish|powershell>` para completions de shell
- INVOKE `codex-models --json` para inspecionar whitelist de modelos codex
- INVOKE `stats --json` para estatísticas do banco (contagens, tamanhos, breakdown por namespace)
- INVOKE `namespace-detect --json` para resolver precedência de namespace da invocação atual
- INVOKE `cache list --json` para listar arquivos de modelo em cache; `cache clear-models --yes` para forçar re-download
- INVOKE `pending list --filter-status queued --json` para fila de checkpoint; `pending show <id>`; `pending cleanup --yes`
- INVOKE `pending-embeddings list --json` para fila de retry; `pending-embeddings process --json` para reprocessar
- INVOKE `slots status --json` para semáforo host-wide; `slots release --slot-id <N> --yes` para órfãos
- INVOKE `embedding status --json` para contagens; `embedding list --json` para inspeção por entrada
- AGENDE semanal: `purge` depois `cleanup-orphans` depois `prune-relations --relation mentions` depois `vacuum` depois `optimize` depois `sync-safe-copy`
- SAIBA que toda escrita executa `PRAGMA wal_checkpoint(TRUNCATE)` após commit
- SE corrupção: `sqlite3 broken.sqlite ".recover" | sqlite3 repaired.sqlite`


## Referência de Variáveis de Ambiente
- `SQLITE_GRAPHRAG_DB_PATH` — path persistente do banco
- `SQLITE_GRAPHRAG_NAMESPACE` — namespace persistente
- `SQLITE_GRAPHRAG_LLM_BACKEND` — backend persistente (codex|claude|opencode|none|auto)
- `SQLITE_GRAPHRAG_LLM_MODEL` — override persistente de modelo
- `SQLITE_GRAPHRAG_CODEX_BINARY` / `SQLITE_GRAPHRAG_CODEX_EMBED_MODEL` — binário e modelo de embed codex
- `SQLITE_GRAPHRAG_CLAUDE_BINARY` — override de path do binário claude
- `SQLITE_GRAPHRAG_OPENCODE_BINARY` / `SQLITE_GRAPHRAG_OPENCODE_MODEL` / `SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL` / `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT` — overrides opencode
- `SQLITE_GRAPHRAG_EMBEDDING_DIM` — dimensão de embedding [8, 4096] (default 64 MRL)
- `SQLITE_GRAPHRAG_LOW_MEMORY` — habilitar paralelismo unitário
- `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR` — modo compliance
- `SQLITE_GRAPHRAG_DISPLAY_TZ` — timezone persistente
- `SQLITE_GRAPHRAG_LOG_FORMAT` — `json` para agregadores de log
- `SQLITE_GRAPHRAG_SKIP_PREFLIGHT` — bypass preflight (APENAS EMERGÊNCIAS)
- `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN` — APENAS para harnesses de teste CI


## Fórmulas CLI Prontas para Uso
- INIT namespace: `sqlite-graphrag init --namespace <ns>`
- VERIFICAR saúde: `sqlite-graphrag health --namespace <ns> --json | jaq '{integrity_ok, schema_version}'`
- MIGRATE preview: `sqlite-graphrag migrate --dry-run --json`
- MIGRATE aplicar: `sqlite-graphrag migrate --json`
- REMEMBER codex todas flags: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini --codex-binary <path> --llm-parallelism 4 remember --name <n> --type decision --description "desc" --body-file doc.md --force-merge --max-rss-mb 4096 --json`
- REMEMBER claude todas flags: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 --claude-binary <path> --llm-parallelism 4 remember --name <n> --type decision --description "desc" --body "conteudo" --force-merge --json`
- REMEMBER opencode todas flags: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle --opencode-binary <path> --opencode-timeout 300 --llm-parallelism 4 remember --name <n> --type note --description "desc" --body-stdin --json`
- REMEMBER graph-stdin: pipe JSON `{body, entities, relationships}` para `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini remember --name <n> --type decision --description "desc" --graph-stdin --force-merge --json`
- REMEMBER-BATCH: pipe NDJSON para `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini remember-batch --json`
- DRY-RUN backend: `sqlite-graphrag --llm-backend codex --dry-run-backend recall "query" --k 5 --json`
- RECALL codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini recall "query" --k 5 --no-graph --json`
- RECALL claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 recall "query" --k 5 --json`
- RECALL opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle recall "query" --k 5 --json`
- HYBRID-SEARCH codex todas flags: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini hybrid-search "query" --k 10 --with-graph --max-hops 2 --min-weight 0.3 --rrf-k 60 --weight-vec 1.0 --weight-fts 1.0 --type decision --json`
- HYBRID-SEARCH claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 hybrid-search "query" --k 10 --json`
- HYBRID-SEARCH opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle hybrid-search "query" --k 10 --with-graph --json`
- HYBRID-SEARCH fts-only: `sqlite-graphrag hybrid-search "query" --k 10 --fallback-fts-only --json`
- DEEP-RESEARCH todas flags: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 deep-research "pergunta" --k 20 --max-hops 3 --max-sub-queries 7 --max-results 50 --with-bodies --graph-decay 0.7 --graph-min-score 0.05 --timeout 30 --max-neighbors-per-hop 10 --json`
- RELATED: `sqlite-graphrag related <nome> --hops 2 --relation uses --json`
- INGEST codex todas flags: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini ingest ./docs --mode codex --recursive --pattern "*.md" --type document --auto-describe --resume --max-files 1000 --max-name-length 80 --llm-parallelism 2 --codex-model gpt-5.4-mini --codex-timeout 300 --fail-fast --low-memory --json`
- INGEST claude todas flags: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 ingest ./docs --mode claude-code --recursive --pattern "*.md" --type document --auto-describe --resume --claude-model claude-sonnet-4-6 --claude-timeout 600 --rate-limit-wait 60 --max-cost-usd 5 --queue-db .ingest-queue.sqlite --keep-queue --json`
- INGEST opencode todas flags: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle ingest ./docs --mode opencode --recursive --pattern "*.md" --type document --auto-describe --opencode-model opencode/big-pickle --opencode-timeout 600 --json`
- INGEST dry-run: `sqlite-graphrag ingest ./docs --dry-run --pattern "*.md" --recursive --json`
- ENRICH re-embed codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation re-embed --limit 100 --resume --llm-parallelism 4 --json`
- ENRICH memory-bindings claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --max-cost-usd 5 --json`
- ENRICH opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation entity-descriptions --mode opencode --dry-run --json`
- READ com grafo: `sqlite-graphrag read --name <n> --with-graph --json`
- READ por id: `sqlite-graphrag read --id 42 --json`
- LIST: `sqlite-graphrag list --type decision --limit 50 --offset 0 --include-deleted --json`
- HISTORY: `sqlite-graphrag history --name <n> --diff --json`
- EDIT corpo codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini edit --name <n> --body-file novo.md --expected-updated-at "2026-01-01T00:00:00Z" --json`
- EDIT descrição: `sqlite-graphrag edit --name <n> --description "nova desc" --json`
- EDIT force-reembed: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini edit --name <n> --force-reembed --json`
- RENAME: `sqlite-graphrag rename --from <antigo> --to <novo> --json`
- RESTORE: `sqlite-graphrag restore --name <n> --version 2 --json`
- FORGET: `sqlite-graphrag forget --name <n> --json`
- PURGE preview: `sqlite-graphrag purge --retention-days 30 --yes --dry-run --json`
- LINK todas flags: `sqlite-graphrag link --from <a> --to <b> --relation uses --weight 0.8 --create-missing --entity-type tool --strict-relations --max-entity-degree 50 --json`
- UNLINK: `sqlite-graphrag unlink --from <a> --to <b> --relation uses --json`
- UNLINK massa: `sqlite-graphrag unlink --entity <nome> --all --json`
- GRAPH stats: `sqlite-graphrag graph stats --json | jaq '{node_count, edge_count, avg_degree}'`
- GRAPH entities: `sqlite-graphrag graph entities --sort-by degree --order desc --limit 20 --json`
- GRAPH traverse: `sqlite-graphrag graph traverse --from <entidade> --depth 2 --json`
- GRAPH exportar: `sqlite-graphrag graph --format <json|dot|mermaid> --output <path>`
- MERGE entidades: `sqlite-graphrag merge-entities --names "a,b,c" --into alvo --json`
- NORMALIZE entidades: `sqlite-graphrag normalize-entities --yes --json`
- RECLASSIFY entidade: `sqlite-graphrag reclassify --name <n> --new-type concept --json`
- RECLASSIFY massa: `sqlite-graphrag reclassify --from-type tool --to-type concept --batch --json`
- RECLASSIFY-RELATION: `sqlite-graphrag reclassify-relation --from-relation <antiga> --to-relation <nova> --batch --json`
- PRUNE-NER: `sqlite-graphrag prune-ner --entity <n>` ou `prune-ner --all --yes`
- PRUNE-RELATIONS preview: `sqlite-graphrag prune-relations --relation mentions --yes --show-entities --dry-run`
- CLEANUP pipeline: INVOKE `forget --name <n>` depois `cleanup-orphans --yes --json` depois `vacuum --json`
- PENDING lista: `sqlite-graphrag pending list --filter-status queued --json`
- PENDING-EMBEDDINGS: `sqlite-graphrag pending-embeddings list --json` depois `pending-embeddings process --json`
- SLOTS: `sqlite-graphrag slots status --json` e `slots release --slot-id <N> --yes --json`
- EMBEDDING status: `sqlite-graphrag embedding status --json` e `embedding list --json`
- FTS: `sqlite-graphrag fts rebuild --json` e `fts check --json` e `fts stats --json`
- VEC: `sqlite-graphrag vec stats --json` e `vec orphan-list --json` depois `vec purge-orphan --yes --json`
- BACKUP: `sqlite-graphrag backup --output backup.sqlite --json`
- SYNC-SAFE-COPY: `sqlite-graphrag sync-safe-copy --dest snapshot.sqlite`
- EXPORT: `sqlite-graphrag export --namespace <ns> --type decision --json`
- OPTIMIZE: `sqlite-graphrag optimize --json`
- VACUUM: `sqlite-graphrag vacuum --json`
- DEBUG-SCHEMA: `sqlite-graphrag debug-schema --json`
- CODEX-MODELS: `sqlite-graphrag codex-models --json`
- COMPLETIONS: `sqlite-graphrag completions <bash|zsh|fish|elvish|powershell>`
- STATS: `sqlite-graphrag stats --json`
- NAMESPACE-DETECT: `sqlite-graphrag namespace-detect --json`
- CACHE listar: `sqlite-graphrag cache list --json`
- CACHE limpar: `sqlite-graphrag cache clear-models --yes`
- FALLBACK CHAIN: `sqlite-graphrag --llm-backend codex --llm-fallback codex,claude,opencode,none --skip-embedding-on-failure remember --name <n> --type note --description "desc" --body-file nota.md --json`


## Regras Ativas
- SEMPRE passe `--llm-backend` e `--llm-model` em comandos de embedding
- SEMPRE parsee `backend_invoked` para confirmar qual backend rodou
- SEMPRE execute `codex login` ou refresque OAuth do claude quando stale
- NUNCA passe API keys (OAuth-only, exit 1); NUNCA use daemon, `--bare`, `--gliner-variant` (REMOVIDOS)
- NUNCA instale com `--features embedding-legacy` ou `--features ner-legacy`
- NUNCA rode `enrich` em paralelo contra mesmo banco; NUNCA escreva `.sqlite` fora do binário
- NUNCA ignore exit 19 (RETRY OBRIGATÓRIO) ou exit 16 (corrija config MCP)
- NUNCA chame `migrate --to-llm-only` sem guarda `--drop-vec-tables`
