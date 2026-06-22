---
name: sqlite-graphrag
description:Para memória persistente, GraphRAG, ou contexto de longo prazo em Claude Code, Codex, Cursor, Windsurf, agentes AI. Em: lembrar disso, salvar conversa, recuperar contexto, busca híbrida, grafo de entidades, memória SQLite, RAG local, embedding LLM-only, fluxo OAuth, embedding BLOB-backed, migrate to-llm-only, migrate rehash, drop vec tables, embedding-dim, llm-parallelism, batched embedding, re-embed, force-reembed, OAuth-only, aborto ANTHROPIC_API_KEY, endurecimento claude codex, Mock LLM CI, daemon removido, ADR-0041 v1.0.83, ANTHROPIC_AUTH_TOKEN, OpenRouter, AWS Bedrock, --dry-run-backend, backend_invoked, vec_degraded_reason, LlmEmbeddingBuilder, validação preflight, exit code 16, BUG-11/12/13, drift de schema, Must-Ignore, paridade --db, health --namespace, migrate --dry-run, ingest --auto-describe, codex-models --json, v1.0.86 v1.0.87 v1.0.88 v1.0.89. KW: memória RAG GraphRAG SQLite one-shot OAuth offline persistente grafo entidade.
---


## Versão Atual (v1.0.89)
- Versão atual do binário: v1.0.89 (lançada em 2026-06-19)
- Versão atual do schema: v15 (após init ou migrate em banco fresco)
- Esta skill documenta features de v1.0.86 até v1.0.89
- Versões anteriores (v1.0.85.2 e abaixo) estão fora do escopo
- Para versões mais antigas, consulte o histórico git desta skill


## Quando Esta Skill Ativa
- USE quando o usuário pede para lembrar, salvar, recordar, recuperar, buscar, ou persistir algo entre sessões
- USE para contexto de longo prazo, grafo de conhecimento, GraphRAG, RAG, ligação de entidades, gestão de memória
- USE quando sqlite, sqlite-graphrag, embedding, FTS5, hybrid-search, ou memória LLM for mencionado
- NÃO USE para dados efêmeros pontuais, I/O simples de arquivo, ou tarefas sem relação a contexto persistente


## Princípios Fundamentais
- INVOKE sempre como subprocesso via `std::process::Command`
- READ stdout para dados estruturados JSON ou NDJSON
- READ stderr para logs de tracing e mensagens humanas
- CHECK exit code ANTES de parsear stdout
- TRUST em contratos JSON como API versionada por SemVer
- BUILD é LLM-only e one-shot; binário tem 14.6 MiB stripped ELF (NÃO 6 MB como em docs antigos)
- BUILD NÃO tem daemon, NÃO tem ONNX runtime, NÃO tem cache de modelo
- OAUTH-ONLY: spawn ABORTA exit 1 se `ANTHROPIC_API_KEY` estiver setada
- OAUTH-ONLY: spawn ABORTA exit 1 se `OPENAI_API_KEY` estiver setada
- NAMESPACE por projeto via `--namespace <ns>` ou env
- NAMESPACE default é `global` quando omitido
- NUNCA expor o binário como servidor MCP ou serviço HTTP
- NUNCA escrever arquivo `.sqlite` em paralelo ao binário
- NUNCA editar o arquivo `.sqlite` a partir de outra ferramenta


## Cartão de Referência Rápida
- INIT primeira vez: `sqlite-graphrag init --namespace <ns>`
- VERIFICAR saúde: `sqlite-graphrag health --json | jaq '.integrity_ok'`
- ARMAZENAR memória: `sqlite-graphrag remember --name <kebab> --type note --description "x" --body "y"`
- INGESTIR pasta: `sqlite-graphrag ingest ./docs --recursive --pattern "*.md" --type document`
- BUSCAR semântica: `sqlite-graphrag recall "query" --k 5 --json`
- BUSCAR híbrida: `sqlite-graphrag hybrid-search "query" --k 10 --rrf-k 60 --json`
- TRAVESSIA de grafo: `sqlite-graphrag graph traverse --from <entity> --depth 2`
- PESQUISA profunda: `sqlite-graphrag deep-research "question" --k 20 --max-hops 3 --json`
- DELEÇÃO física: `sqlite-graphrag forget --name <n>` depois `purge --retention-days 30 --yes`


## Inicialização, Saúde e Config Global
- EXECUTE `sqlite-graphrag init --namespace <ns>` no primeiro uso
- EXECUTE `health --json` para verificar `integrity_ok` e `schema_ok`
- VERIFIQUE `schema_version >= 15` após `init` ou `migrate`
- EXECUTE `migrate --json` após cada upgrade do binário
- USE `migrate --to-llm-only --drop-vec-tables --json` para bancos v1.0.74 ou v1.0.75
- USE `migrate --rehash --json` para reparar drift de checksum SipHasher13 V002
- USE `migrate --dry-run --json` para PREVIEW de migrações pendentes sem aplicar
- TRATE exit code 10 como erro de banco; execute `vacuum` e `health`
- TRATE exit code 15 como ocupado; amplie `--wait-lock`
- TRATE exit code 16 como falha preflight (v1.0.87+); corrija config MCP ou defina `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1`
- ABORTE pipeline quando `integrity_ok` retornar `false`
- EXECUTE `optimize --json` para refrescar estatísticas do planner; resposta inclui `fts_rebuilt`
- USE `optimize --skip-fts --json` quando FTS5 foi reconstruído recentemente
- EXECUTE `fts rebuild --json` quando `health.fts_degraded` for true
- INSPEIONE `wal_size_mb` em `health` para fragmentação
- VERIFIQUE `journal_mode` igual a `wal` em produção
- USE `debug-schema --json` para troubleshooting de drift de schema
- PASSE `--db <PATH>` para sobrescrever localização do banco (agora aceito em `embedding status/list/abandon`, `pending list/show` desde v1.0.89, ADR-0049)
- PASSE `--namespace <NS>` em `health` desde v1.0.89 para filtrar contagens para um namespace
- DEFINA env `SQLITE_GRAPHRAG_DB_PATH` para configuração persistente
- DEFINA env `SQLITE_GRAPHRAG_NAMESPACE` para namespace persistente
- PASSE `--lang en` ou `--lang pt` para forçar idioma do stderr
- PASSE `--tz America/Sao_Paulo` para localizar timestamps
- DEFINA env `SQLITE_GRAPHRAG_DISPLAY_TZ` para timezone persistente
- DEFINA `SQLITE_GRAPHRAG_LOG_FORMAT=json` para agregadores de log
- USE `-v` para info, `-vv` para debug, `-vvv` para trace
- ATIVE `SQLITE_GRAPHRAG_LOW_MEMORY=1` em containers restritos
- DEFINA env `SQLITE_GRAPHRAG_EMBEDDING_DIM` na faixa `[8, 4096]` (default 64 MRL)
- DEFINA `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` para modo compliance (ADR-0041)
- DEFINA `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` APENAS para harnesses de teste CI
- VALORES válidos de `--type`: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- FLAGS globais: `--db`, `--namespace`, `--lang`, `--tz`, `--json`, `--low-memory`, `--max-concurrency N`, `--wait-lock SECS`, `--llm-parallelism N`, `--llm-backend claude|codex|none|auto[,fallback...]`, `--llm-model <MODEL>`, `--dry-run-backend`, `--llm-fallback-mode <claude|codex>`, `--graceful-shutdown-secs N`, `--claude-binary <PATH>`, `--codex-binary <PATH>`, `--skip-embedding-on-failure`


## Contrato de Arquitetura (OAuth/LLM/One-Shot)
- BUILD é LLM-only; build padrão NÃO tem `fastembed`, `ort`, `ndarray`, `tokenizers`, `huggingface-hub`, `sqlite-vec`, `GLiNER`
- BUILD removeu subcomando `daemon` inteiramente (ADR-0021)
- COSINE similarity é pure Rust em `src/similarity.rs`
- COSINE roda sobre `memory_embeddings`, `entity_embeddings`, `chunk_embeddings` com BLOB
- SCHEMA v15 após `init` ou `migrate` em banco fresco
- MIGRAÇÃO V013 dropa virtual tables `vec_memories`, `vec_entities`, `vec_chunks`
- MIGRAÇÃO V014 cria tabela de checkpoint `pending_memories`
- MIGRAÇÃO V015 cria fila de retry `pending_embeddings`
- OAUTH-ONLY: `ANTHROPIC_API_KEY` ABORTA spawn com `AppError::Validation` (ADR-0011)
- OAUTH-ONLY: `OPENAI_API_KEY` ABORTA spawn com `AppError::Validation` (ADR-0011)
- OAUTH-ONLY: ambas API keys EXCLUÍDAS do whitelist de env-clear
- OAUTH-ONLY: flag `--bare` REMOVIDA de todos os caminhos executáveis
- OAUTH-ONLY: 7 flags de endurecimento SEMPRE passadas para `claude -p`
- FLAGS de endurecimento para claude: `--model claude-sonnet-4-6 --strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions --output-schema`
- FLAGS de endurecimento para codex: `--model gpt-5.5 --json --output-schema --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules -c mcp_servers='{}' --ask-for-approval never`
- ADR-0041 v1.0.83: `ANTHROPIC_AUTH_TOKEN` PRESERVADA para providers Anthropic-compatíveis
- ADR-0041 v1.0.83: `ANTHROPIC_BASE_URL` PRESERVADA para endpoints customizados
- ADR-0041 v1.0.83: `OPENAI_BASE_URL` PRESERVADA para endpoints OpenAI-compatíveis
- ADR-0041 v1.0.83: `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT` PRESERVADAS
- ADR-0041 v1.0.83: providers suportados incluem OpenRouter, AWS Bedrock, gateways corporativos
- PRECEDÊNCIA de DIM de embedding: env `SQLITE_GRAPHRAG_EMBEDDING_DIM` depois `schema_meta.dim` depois default 64 MRL
- DIM de embedding adapta tamanho de lote: base 8 chunks / 25 nomes de entidade em dim 64
- MOCK LLM CLI para CI: prefixar `tests/mock-llm` ao PATH
- RECEITA de bypass de SHUTDOWN: `PATH=tests/mock-llm:$PATH SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1 setsid -w timeout 120 sqlite-graphrag …`
- NUNCA instalar com `--features embedding-legacy` ou `--features ner-legacy`
- NUNCA depender do daemon ou flag `--bare` (REMOVIDOS em v1.0.76 e v1.0.79)
- NUNCA misturar queries em `vec_memories` (REMOVIDO em v1.0.76)
- NUNCA chamar `migrate --to-llm-only` sem guarda de segurança `--drop-vec-tables`


## CRUD — Caminho de Escrita (remember, remember-batch, ingest)
- INVOKE `remember --name <kebab> --type <kind> --description <text> --body-stdin` para corpos longos
- INVOKE `remember --name <kebab> --body-file <path>` para evitar escape shell
- INVOKE `remember --name <kebab> --body <text>` para corpos curtos
- PASSE `--force-merge` para updates idempotentes e restauração de soft-deleted
- PASSE `--clear-body` para limpar corpo durante update com `--force-merge`
- PASSE `--dry-run` para validar inputs sem persistir
- PASSE `--max-rss-mb <MiB>` para abortar quando RSS exceder threshold (default 8192)
- RESPEITE limite de 512000 bytes e 512 chunks por corpo
- INVOKE `remember --graph-stdin` para anexar `{body, entities, relationships}` em único JSON
- PASSE entities como `[{name, entity_type}]` com kebab-case ASCII
- PASSE relationships como `[{source, target, relation, strength}]` onde `strength ∈ [0.0, 1.0]`
- USE `--enable-ner` para extração de entidades URL-regex (URL-regex APENAS desde v1.0.79)
- NUNCA envie `entity_type` e `type` juntos no mesmo objeto JSON
- NUNCA use `--gliner-variant` (no-op desde v1.0.79)
- INVOKE `remember-batch` para 10+ memórias via NDJSON stdin
- ESPERE evento por item: `name`, `status ∈ {created, updated, skipped, failed}`, `memory_id?`, `error?`, `elapsed_ms`
- ESPERE linha de summary: `total`, `created`, `updated`, `skipped`, `failed`, `elapsed_ms`
- INVOKE `ingest <DIR> --recursive --pattern "*.md"` para importar diretório
- PASSE `--type <kind>` para aplicar mesmo tipo a todos arquivos ingeridos
- RESPEITE cap `--max-files 10000` como validação all-or-nothing
- USE `--fail-fast` para parar na primeira falha por arquivo
- USE `--max-name-length N` para sobrescrever truncamento de nomes em 60 chars
- ESPERE linha NDJSON por arquivo: `file`, `name`, `status`, `truncated`, `original_name?`, `memory_id?`, `action?`, `error?`
- ESPERE linha de summary: `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- USE `--llm-parallelism N` em `ingest` (default 2, clamp [1, 32])
- DISTINGA `--max-concurrency N` (fan-out CLI) de `--ingest-parallelism N` (per-file extract+embed)
- USE `--auto-describe` (default true desde v1.0.89) para extrair descrição da primeira linha significativa do corpo; opt-out via `--no-auto-describe`
- INVOKE `ingest --mode claude-code` para extração curada por LLM
- INVOKE `ingest --mode codex` para extração curada por OpenAI Codex
- ESPERE eventos claude-code: contagem `entities`, contagem `rels`, `cost_usd` (Omite cost para OAuth)
- USE `--resume` para continuar do queue DB após interrupção
- USE `--retry-failed` para retentar apenas arquivos que falharam
- NUNCA use `fd | xargs remember`; use `ingest` em vez disso
- NUNCA misture `--body`, `--body-file`, `--body-stdin`, `--graph-stdin` em única invocação
- NUNCA passe corpo vazio sem entities via `--graph-stdin` (exit 1 desde v1.0.54)
- NUNCA use `--force-merge` em `ingest` (exclusivo de `remember`)
- NUNCA misture tipos diferentes de memória em mesma invocação de `ingest`


## CRUD — Leitura, Histórico, Atualização
- INVOKE `read --name <kebab>` para fetch O(1) por nome
- INVOKE `read --id <N>` para lookup direto por memory_id
- INVOKE `read --with-graph` para incluir entidades e relacionamentos vinculados
- PARSEE campos `body`, `description`, `created_at_iso`, `updated_at_iso`
- TRATE exit code 4 como memória não encontrada no namespace
- ESPERE v1.0.85 G55 mensagem bilíngue: `--lang en` emite `Memory not found`, `--lang pt` emite `Memória não encontrada`
- INVOKE `list --type <kind> --limit N` para filtrar por tipo de memória
- USE `--offset N` para paginar datasets grandes
- USE `--include-deleted` para incluir memórias soft-deleted
- ESPERE resposta de `list`: `items[]`, `total_count`, `truncated`, `body_length`, `elapsed_ms`
- INVOKE `history --name <n>` para listar versões em ordem cronológica reversa
- USE `--diff` para incluir estatísticas de diff de caracteres entre versões
- ESPERE `versions[]`: `version`, `created_at_iso`, `body_length`, `deleted?`, `changes?`
- INVOKE `edit --name <n> --body-file <path>` para atualizar corpo de arquivo
- USE `--description <text>` para atualizar apenas descrição
- USE `--type <kind>` para mudar tipo de memória sem recriar (v1.0.66)
- USE `--force-reembed` para regenerar embedding sem mudança de corpo (v1.0.79)
- USE `--llm-parallelism N` em `edit` (default 4, clamp [1, 32])
- USE `--expected-updated-at <ts>` para optimistic locking
- TRATE exit code 3 como conflito de optimistic lock; recarregue `read --json` e retente
- INVOKE `rename --from <old> --to <new>` para renomear preservando histórico
- TRATE exit 1 quando novo nome for igual ao antigo (v1.0.64)
- INVOKE `restore --name <n> --version <N>` para restaurar versão antiga
- OMITA `--version` para selecionar última versão não-restore automaticamente
- ESPERE que cada `edit` ou `restore` crie nova versão imutável
- ESPERE correção de desync do FTS5 aplicada (v1.0.56); memórias editadas ficam imediatamente localizáveis
- NUNCA pule optimistic locking em pipelines concorrentes


## CRUD — Deleção (forget, purge, unlink, prune, cleanup)
- INVOKE `forget --name <n>` para soft-delete reversível
- ESPERE que `forget` desapareça das saídas de `recall` e `list`
- TRATE exit 4 como memória ausente (desde v1.0.52)
- INVOKE `restore` para reverter soft-delete antes de qualquer purge
- INVOKE `purge --retention-days <N> --yes` para deleção física
- USE `--dry-run` primeiro para auditar contagem
- ESPERE retenção default de 90 dias para memórias soft-deleted
- INVOKE `unlink --from <a> --to <b> --relation <type>` para remoção direcionada de aresta
- OMITA `--relation` para remover todas arestas entre `--from` e `--to`
- USE `--entity <name> --all` para remover em massa todos relacionamentos de uma entidade
- TRATE exit code 4 como aresta inexistente
- INVOKE `prune-relations --relation <type> --yes` para deleção em massa de relacionamentos
- USE `--show-entities` com `--dry-run` para listar nomes de entidades afetadas
- INVOKE `cleanup-orphans --dry-run` para auditar entidades órfãs
- APLIQUE `--yes` em pipelines automatizados para `cleanup-orphans`
- INVOKE `prune-ner --entity <n>` para remover bindings NER de entidade específica
- INVOKE `prune-ner --all --yes` para remover todos bindings NER no namespace
- USE pipeline padrão: bulk `forget` depois `cleanup-orphans --yes` depois `vacuum --json`
- NUNCA delete manualmente via shell `sqlite3`; use apenas comandos do binário


## Grafo de Entidades (link, graph, memory-entities, rename, delete, merge, reclassify, normalize)
- INVOKE `link --from <a> --to <b> --relation <type>` para criar aresta
- PASSE `--create-missing` para auto-criar entidades inexistentes durante link
- PASSE `--entity-type <kind>` para entidades auto-criadas (default `concept`)
- PASSE `--weight <float>` para peso da aresta (default 0.5)
- USE `--strict-relations` para falhar em tipos de relação não-canônicos
- USE `--max-entity-degree N` para avisar quando entidade excede N conexões
- INVOKE `graph entities --json` para listar todas entidades
- ACESSE via `.entities[]` (campo é `entities` NÃO `items`)
- FILTRE via `--entity-type <kind>`
- ORDENE via `--sort-by degree|name|created_at` (default `name`)
- DEFINA direção via `--order asc|desc` (default `asc`)
- PAGINE via `--limit N --offset N`
- INVOKE `graph stats --json` para inspecionar `node_count`, `edge_count`, `avg_degree`, `max_degree`
- INVOKE `graph traverse --from <root> --depth <N>` para travessia de subgrafo
- ESPERE `hops[]`: `entity`, `relation`, `direction`, `weight`, `depth`
- TRATE exit 4 como entidade raiz inexistente
- USE `--format json|dot|mermaid` com `--output <path>` para exportar grafo
- INVOKE `memory-entities --name <memory>` para lookup forward de entidades
- INVOKE `memory-entities --entity <name>` para lookup reverso de memórias
- INVOKE `rename-entity --name <old> --new-name <new>` para renomear entidade
- TRATE exit 4 como entidade não encontrada
- TRATE exit 1 se novo nome falhar validação
- INVOKE `delete-entity --name <n> --cascade` para remover entidade e todos bindings
- PASSE `--cascade` é OBRIGATÓRIO quando entidade tem relacionamentos (senão exit 1)
- INVOKE `merge-entities --names "a,b,c" --into <target>` para mesclar entidades
- INVOKE `reclassify --name <n> --new-type <kind>` para reclassificação individual
- INVOKE `reclassify --from-type <old> --to-type <new> --batch` para reclassificação em massa
- INVOKE `reclassify-relation --from-relation <old> --to-relation <new> --batch`
- INVOKE `normalize-entities --yes` para normalizar todos nomes para kebab-case ASCII
- VALIDE nomes: mínimo 2 chars, sem newlines, sem ALL_CAPS curtos (4 chars ou menos rejeitados desde fix BUG-13 v1.0.88)
- NORMALIZE nomes via NFKD depois ASCII depois lowercase depois hífens
- RELAÇÕES canônicas: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- MAPEAMENTO não-canônico: `adds|creates → causes`, `implements → supports`, `blocks → contradicts`, `tested-by → related`, `part-of → applies-to`
- TIPOS canônicos de entidade: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- NUNCA use `mentions` como relação padrão (adiciona ruído)
- NUNCA persista estado efêmero em entidades


## Busca GraphRAG (recall, hybrid-search, related, deep-research, enrich)
- USE padrão canônico de três camadas: `hybrid-search` depois `read --name` depois `related|graph traverse`
- INVOKE `recall <query> --k N` para busca semântica pura KNN
- PASSE `--no-graph` para desabilitar expansão automática de grafo
- INTERPRETE `distance` crescente como similaridade decrescente
- INTERPRETE `score` como `1.0 - distance` clamped em `[0.0, 1.0]`
- ESPERE `source ∈ {direct, graph}` e `graph_depth` para resultados de grafo
- ESPERE resposta: `direct_matches[]`, `graph_matches[]`, `results[]`, `elapsed_ms`
- INVOKE `hybrid-search <query> --k N` para fusão FTS5 e KNN via RRF
- PASSE `--rrf-k 60` para constante RRF padrão
- PASSE `--weight-vec 1.0` e `--weight-fts 1.0` para fusão balanceada
- USE `--with-graph --max-hops 2 --min-weight 0.3` para expansão de grafo
- ESPERE resposta `hybrid-search`: `results[]`, `graph_matches[]`, `fts_degraded`, `vec_degraded_reason?`, `backend_invoked`, `elapsed_ms`
- LEIA TANTO `results[]` QUANTO `graph_matches[]` quando `--with-graph` ativo
- INVOKE `related <name> --hops N` para travessia multi-hop a partir de memória
- PASSE `--relation <type>` para filtrar travessia por relação
- ESPERE `hop_distance` explícito por hop
- INVOKE `deep-research "<query>" --k 20` para pesquisa paralela multi-hop
- PASSE `--max-sub-queries 7` para cap de decomposição de query
- PASSE `--max-hops 3 --min-weight 0.3 --max-results 50` para travessia de grafo
- PASSE `--with-bodies` para incluir corpos completos de memórias nos resultados
- ESPERE resposta: `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context?`, `stats`
- INVOKE `enrich --operation <op> --mode claude-code` para qualidade de grafo via LLM
- OPERAÇÕES: `memory-bindings`, `entity-descriptions`, `body-enrich` (Jaccard >=0.7), `re-embed --limit N --resume`
- PASSE `--llm-parallelism N` para controlar subprocessos LLM concorrentes
- PASSE `--max-cost-usd N` para cap de gasto acumulado (ignorado para usuários OAuth)
- PASSE `--resume` e `--retry-failed` para resiliência a crash
- USE `--dry-run` para preview sem spawnar LLM
- USE query AMPLA para `recall --k 5`
- USE query MISTA de tokens para `hybrid-search --k 10`
- USE query MISTA com grafo para `hybrid-search --with-graph --max-hops 2`
- USE query EXPLORATÓRIA de memória para `related --hops 2`
- USE query EXPLORATÓRIA de entidade para `graph traverse --depth 2`
- NUNCA confunda `distance` com `combined_score` em ranking
- NUNCA aumente `--hops` sem inspecionar `graph stats` antes
- NUNCA pule camada 2 quando snippet for insuficiente
- NUNCA leia apenas `.results[]` quando `--with-graph` estiver ativo


## Superfície v1.0.86+ (pending, slots, embedding, llm-backend, shutdown)
- INVOKE `pending list --filter-status queued` para inspecionar fila de checkpoint de três estágios do remember
- INVOKE `pending show <id>` para inspecionar linha única de checkpoint
- INVOKE `pending cleanup --yes` para remover linhas em estado terminal
- RESPALDADO pela tabela `pending_memories` criada pela migração V014 (ADR-0036)
- PASSE `--db <PATH>` em `pending list`/`pending show` (v1.0.89, ADR-0049)
- INVOKE `pending-embeddings list` para inspecionar fila de retry de embeddings que falharam
- INVOKE `pending-embeddings process` para reprocessar com próximo backend
- RESPALDADO pela tabela `pending_embeddings` criada pela migração V015 (ADR-0040)
- INVOKE `slots status` para inspecionar semáforo de slots host-wide
- INVOKE `slots release --slot-id <N> --yes` para colher slots órfãos
- LOCK via `fs4 = "0.9"` com `fcntl(F_SETLK)` em Unix e `LockFileEx` em Windows (ADR-0039)
- INVOKE `embedding status` para contagens agregadas por status
- INVOKE `embedding list` para inspeção por entrada
- PASSE `--db <PATH>` em `embedding status`/`embedding list`/`embedding abandon` (v1.0.89, ADR-0049)
- PASSE `--llm-backend codex,claude` para codex-primeiro com fallback claude (ADR-0038)
- PASSE `--llm-backend codex,claude,none` para fallback de embedding null
- DEFAULT de `--llm-backend` é `codex`
- PASSE `--llm-fallback-mode <claude|codex>` para trocar backend mid-job em rate-limit
- PASSE `--max-concurrency N` flag global para limitar invocações CLI pesadas concorrentes
- PASSE `--wait-lock SECS` flag global para ampliar janela de aquisição de lock
- PASSE `--llm-parallelism N` flag global para cap de fan-out de subprocessos de embedding (default 4, clamp [1, 32])
- PASSE `--ingest-parallelism N` para controlar paralelismo extract+embed por arquivo em `ingest`
- PASSE `--graceful-shutdown-secs N` para reservar orçamento de cleanup antes de SIGKILL
- PASSE `--skip-embedding-on-failure` APENAS quando `--llm-backend …,none`
- PASSE ADR-0041 `--strict-env-clear` para descartar credenciais de provider customizado em subprocesso
- PASSE `--dry-run-backend` para planejar operação de backend sem executá-la (preview idempotente)
- PARSEE campo `backend_invoked` nos envelopes de recall, hybrid-search, remember, edit, ingest, enrich, read para confirmar backend efetivo
- LEIA `vec_degraded_reason` nos envelopes de recall/hybrid-search quando caminho vec estiver degradado
- SAIBA que backend claude divide-se em embedder local via `embed_via_claude_local` (zero-token, compatível com OAuth)
- USE `LlmEmbeddingBuilder` para compor pipeline de embedding: `with_backend(Codex).or_fallback(Claude).or_skip()`
- INVOKE `codex-models --json` desde v1.0.89 para emitir envelope JSON `{"action":"codex_models","count":N,"default":"...","models":[...]}` (alias no-op)
- EXECUTE `codex login` após upgrade para refrescar refresh token OAuth (incidente 2026-06-14)
- AÇÃO do operador para OAuth stale: `codex login` depois retry


## Camada de Validação Pre-Flight v1.0.87+ (ADR-0045, GAP-META-005)
- SAIBA que `src/spawn/preflight.rs` porta todo spawn de subprocesso LLM através de 7 guards ANTES do fork
- SAIBA que exit code 16 (`EX_CONFIG`) é o código universal de falha preflight (adicionado v1.0.87)
- SAIBA que 7 guards rodam em ordem: `check_argv_size`, `check_binary_exists`, `check_mcp_config_inline`, `check_mcp_config_path`, `check_walkup_mcp_json`, `check_output_buffer`, `check_claude_config_dir`
- SAIBA que `check_argv_size` rejeita argv excedendo `ARG_MAX - 4096` bytes (margem para env vars do kernel)
- SAIBA que `check_binary_exists` aborta quando `claude` ou `codex` não está em PATH
- SAIBA que `check_mcp_config_inline` reescreve `--mcp-config '{}'` literal para tempfile com `{"mcpServers":{}}` (Claude Code 2.1.177 rejeita a forma literal)
- SAIBA que `check_mcp_config_path` valida conteúdo JSON de arquivos `--mcp-config <PATH>`
- SAIBA que `check_walkup_mcp_json` rejeita `.mcp.json` inválido na cadeia ancestral do CWD (até 16 níveis via `Path::ancestors()`)
- SAIBA que `check_output_buffer` dobra buffer do parser acima de 64 KB para lidar com saídas grandes
- SAIBA que `check_claude_config_dir` evita vazamento MCP de `~/.claude/` user-level
- DEFINA `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` APENAS em emergências; bypass reverte para `Command::spawn()` direto e herda todas as 5 classes de bug GAP-META-005
- LEIA envelope JSON `AppError::PreFlightFailed(PreFlightError)` para remediação específica por variante
- SAIBA que fix BUG-11 v1.0.88 garante que falha preflight propaga via `embed_via_backend_strict`; NUNCA espere sucesso silencioso quando preflight falha
- NUNCA prossiga após exit code 16 sem resolver a variante específica reportada


## Hotfixes v1.0.88+ (BUG-11, BUG-12, BUG-13)
- SAIBA que BUG-11 (CRÍTICO) foi CORRIGIDO: falha preflight em `extract/llm_embedding.rs:563-565` agora propaga para `remember` via `embed_via_backend_strict` em vez de persistência silenciosa com `backend_invoked: "none"` e zero chunks
- REPRODUZA o fix BUG-11: `CLAUDE_CONFIG_DIR=/tmp/bad-config-with-mcp sqlite-graphrag remember --name X --type note --description x --body y` retorna exit 11 com envelope JSON de erro
- SAIBA que BUG-12 (MÉDIO) foi CORRIGIDO: enforço OAuth-only emite exatamente 1 linha stderr (eram 2 — `eprintln!` duplicado removido em `src/output.rs`)
- VERIFIQUE o fix BUG-12: `ANTHROPIC_API_KEY=sk-test sqlite-graphrag init` emite 1 linha stderr
- SAIBA que BUG-13 (MÉDIO) foi CORRIGIDO: `link --create-missing` valida nomes de entidade ANTES de normalizar (estava bypassando validação; abreviações ALL_CAPS de 3-4 chars como `API`, `WAL`, `RUST` agora corretamente rejeitadas via CLI casando com o caminho `remember --graph-stdin`)
- VERIFIQUE o fix BUG-13: `sqlite-graphrag link --from api --to service --create-missing --relation uses` retorna exit 1 com erro de validação
- INVOKE a variante `AppError::PreFlightFailed(PreFlightError)` no tratamento de erros; exit code 16, `is_permanent() == true`


## Remediação de Deadlock de Embedding v1.0.89+ (ADR-0050)
- PASSE `--llm-model <MODEL>` como flag global para selecionar modelo de embedding para TODOS os backends (v1.0.89, ADR-0050)
- MODELO padrão para backend codex: `gpt-5.5`; para backend claude: `claude-sonnet-4-6`
- DEFINA env `SQLITE_GRAPHRAG_LLM_MODEL` como override persistente para `--llm-model`
- PASSE `--codex-binary <PATH>` para sobrescrever localização do binário codex (v1.0.89, ADR-0050)
- DEFINA env `SQLITE_GRAPHRAG_CODEX_BINARY` como override persistente para `--codex-binary`
- PASSE `--claude-binary <PATH>` para sobrescrever localização do binário claude (propagado via set_var desde v1.0.89)
- PASSE `--skip-embedding-on-failure` para retornar exit 0 quando embedding LLM falha (cabeado end-to-end desde v1.0.89, ADR-0050)
- SAIBA que 7 flags CLI mortas foram corrigidas na v1.0.89 via propagação `set_var` em `main.rs`: `--llm-model`, `--llm-fallback`, `--skip-embedding-on-failure`, `--claude-binary`, `--codex-binary`, `--llm-max-host-concurrency`, `--llm-slot-wait-secs`
- SAIBA que `deep-research` e `remember-batch` agora recebem `llm_backend` do main.rs (v1.0.89, ADR-0050)
- SAIBA que timeout adaptativo escala com tamanho do batch: `base + 15s × (batch_size - 1)` (v1.0.89, ADR-0050)
- SAIBA que erros de OAuth expirado agora incluem hint acionável: "execute codex login" ou "atualize OAuth do claude" (v1.0.89)
- SAIBA que `BoolishValueParser` aceita `1/yes/on/true` e `0/no/off/false` para env vars booleanas (v1.0.89, ADR-0050)
- SAIBA que flag `--yes` em `slots release`, `purge`, `cleanup-orphans` foi cabeada end-to-end (v1.0.89, BUG-YES-FLAG-IGNORED)


## Drift de Schema e Paridade de Flag v1.0.89+ (ADR-0048, ADR-0049)
- SAIBA que `health.schema.json` foi regenerado via macro derive `schemars` (ADR-0048); `additionalProperties: true` conforme política Must-Ignore (RFC 7493 I-JSON)
- SAIBA que 17 novos campos foram adicionados ao envelope `health` desde v1.0.88: `fts_query_ok`, `vec_memories_missing`, `vec_memories_orphaned`, `sqlite_version`, `mentions_ratio`, `mentions_warning`, `top_relation`, `top_relation_ratio`, `applies_to_ratio`, `relation_concentration_warning`, `super_hub_count`, `super_hub_warning`, `top_hub_entity`, `top_hub_degree`, `hub_warning`, `non_normalized_count`, `normalization_warning`
- REGENERE schemas via `cargo run --bin dump-schema` (ordenamento BTreeMap idempotente)
- PASSE `--namespace <NS>` em `health` para filtrar contagens para um namespace
- USE `migrate --dry-run --json` para PREVIEW de migrações pendentes sem aplicar; lista nomes+versões, valida checksums, verifica pré-condições
- USE `codex-models --json` como alias no-op retornando envelope JSON
- USE `--auto-describe` (default true) em `ingest` para extrair descrição da primeira linha significativa do corpo; opt-out via `--no-auto-describe`
- PASSE `--db <PATH>` em `embedding status`/`embedding list`/`embedding abandon`/`pending list`/`pending show` (ADR-0049)
- SAIBA que `--db <PATH>` NÃO é global; cada subcomando aceita independentemente (`clap::Arg::global = true` foi REJEITADO como invasivo)
- TRATE o tamanho do binário como 14.6 MiB stripped ELF (NÃO 6 MB como em docs antigos); veja descrição em `Cargo.toml:6`


## Contratos JSON (Top-5 Campos por Comando)
- TOP campos `recall`: `results[].name`, `snippet`, `distance`, `score`, `source`
- TOP campos `hybrid-search`: `results[].name`, `combined_score`, `vec_rank`, `fts_rank`, `source`
- TOP campos `health`: `integrity_ok`, `schema_ok`, `counts`, `wal_size_mb`, `schema_version`
- TOP campos `list`: `items[].name`, `type`, `description`, `updated_at_iso`, `deleted_at_iso?`
- TOP campos `edit`: `memory_id`, `name`, `action`, `version`, `elapsed_ms`
- TOP campos `read`: `name`, `body`, `description`, `created_at_iso`, `updated_at_iso`
- TOP campos `forget`: `action`, `forgotten`, `name`, `namespace`, `elapsed_ms`
- TOP campos `link`: `action`, `from`, `to`, `relation`, `weight`
- TOP campos `graph entities`: `entities[].id`, `name`, `entity_type`, `degree`, `description?`
- TOP campos `deep-research`: `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context`, `stats`
- EVENTOS NDJSON de `enrich`: `phase`, `name`, `status`, `entities?`, `rels?`, `cost_usd?`, `elapsed_ms?`
- TOP campos `pending list`: `id`, `name`, `status`, `created_at`, `namespace`
- TOP campos `slots status`: `max_concurrency`, `acquired`, `waiting`, `held_by_pid[]`
- TOP campos `embedding status`: `pending`, `processing`, `done`, `failed`, `skipped`
- Envelopes `remember`/`edit`/`ingest`/`enrich`/`read`: incluem `backend_invoked` e `vec_degraded_reason?`
- `health.schema.json` usa `"additionalProperties": true` conforme política Must-Ignore (RFC 7493 I-JSON) desde v1.0.89 (ADR-0048); os outros 49 schemas em `docs/schemas/` ainda usam `"additionalProperties": false` (Must-Validate) pendentes de regeneração em v1.0.90+
- SCHEMAS completos em `docs/schemas/*.schema.json` (nunca inline schema completo em skill)


## Códigos de Saída e Retry
- EXIT 0 significa sucesso; parsee stdout
- EXIT 1 significa erro de validação (peso inválido, self-link, max-files excedido, bypass ALL_CAPS em link)
- EXIT 2 significa erro de parsing de argumento Clap
- EXIT 3 significa conflito de optimistic lock; recarregue `read --json` e retente
- EXIT 4 significa entidade, memória ou versão não encontrada
- EXIT 5 significa erro de namespace
- EXIT 6 significa payload acima do limite de tamanho
- EXIT 9 significa memória duplicada (use `--force-merge` para update ou restore)
- EXIT 10 significa erro de banco; execute `vacuum` e `health`
- EXIT 11 significa falha de embedding (erro de subprocesso LLM, incluindo falha preflight desde fix BUG-11)
- EXIT 13 significa falha parcial de batch; reprocesse apenas os que falharam
- EXIT 14 significa erro de I/O (permissão, disco cheio)
- EXIT 15 significa banco ocupado; amplie `--wait-lock`
- EXIT 16 significa falha de validação preflight (v1.0.87+, ADR-0045); cheque envelope JSON para variante
- EXIT 19 significa SHUTDOWN_EXIT_CODE (ADR-0037); trabalho parcial descartado; RETRY OBRIGATÓRIO
- EXIT 19 envelope: `{error:true, code:19, signal, graceful, message}`
- EXIT 20 significa erro interno ou falha de serialização JSON
- EXIT 75 significa slots esgotados OU `JobSingletonLocked`
- EXIT 75 de `enrich`/`ingest --mode claude-code|codex`: parsee `job '(\w+)'.*namespace '(\w+)'`
- EXIT 75 circuit breaker: respeite janela de cooldown por namespace; NÃO retente imediatamente
- EXIT 77 significa pressão de RAM; aguarde memória livre
- NUNCA ignore exit code não-zero como sucesso
- NUNCA reprocesse batch inteiro após exit 13
- NUNCA aumente concorrência após exit 75 ou 77
- NUNCA confunda exit 1 (validação) com exit 9 (duplicada)
- NUNCA trate exit 16 como transitório; corrija o problema preflight subjacente


## Concorrência, RAM, Paralelismo, Slots
- RESPEITE teto rígido `2 × nCPUs` para comandos pesados
- TRATE como pesados: `init`, `remember`, `ingest`, `recall`, `hybrid-search`
- DISTINGA `--max-concurrency` (fan-out CLI) de `--ingest-parallelism` (per-file)
- DEFINA `--llm-parallelism N` default 4 em `remember`/`edit`, default 2 em `ingest`
- CLAMP `--llm-parallelism` na faixa `[1, 32]`
- USE `--llm-max-host-concurrency N` para cap de subprocessos LLM cross-process
- USE `--llm-slot-wait-secs N` para esperar slot ou `--llm-slot-no-wait` para abortar
- AMPLIE `--wait-lock SECS` quando contenção for esperada
- ATIVE `SQLITE_GRAPHRAG_LOW_MEMORY=1` para paralelismo unitário (3-4x mais lento)
- USE `--strict-env-clear` (ADR-0041) para preservar apenas `PATH` em compliance
- RECEITA de bypass de SHUTDOWN: prefixar `tests/mock-llm` ao PATH depois setar `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` depois envolver com `setsid -w timeout`
- JOB SINGLETON: `enrich`, `ingest --mode claude-code`, `ingest --mode codex` adquirem singleton por namespace
- USE `--wait-job-singleton SECS` para esperar lock ou `--force-job-singleton` para quebrar lock stale
- LIMITE ingestão paralela em CI para evitar rate limits da LLM
- NUNCA rode `enrich` em paralelo contra mesmo banco


## Manutenção (fts, backup, vacuum, optimize, migrate, export, debug-schema, vec, completions)
- INVOKE `fts rebuild --json` para reconstruir totalmente o índice full-text FTS5
- INVOKE `fts check --json` para executar verificação de integridade FTS5
- INVOKE `fts stats --json` para inspecionar saúde FTS5 (`total_rows`, `fts_functional`)
- INVOKE `optimize --fts-dry-run` para preview de rebuild FTS5
- INVOKE `optimize --fts-progress N` para imprimir progresso a cada N segundos
- PASSE `--no-fts-skip-when-functional` para forçar rebuild FTS5 mesmo quando saudável
- INVOKE `backup --output <path> --json` para backup online seguro via API SQLite
- INVOKE `sync-safe-copy --dest <path>` para snapshot atômico antes de operações críticas
- INVOKE `export --namespace <ns> --type <kind> --json` para exportar memórias como NDJSON
- INVOKE `vacuum --json` após purge grande para reclamar espaço
- INVOKE `migrate --rehash --json` para reparar drift de checksum V002
- INVOKE `migrate --to-llm-only --drop-vec-tables --json` para upgrades v1.0.74/75
- INVOKE `migrate --dry-run --json` para preview de migrações (v1.0.89)
- INVOKE `debug-schema --json` (oculto do `--help`) para inspecionar estado de schema
- INVOKE `completions <bash|zsh|fish|elvish|powershell>` para gerar completions de shell
- INVOKE `vec orphan-list --json` para listar vetores órfãos de memória
- INVOKE `vec purge-orphan --yes --dry-run` para PREVIEW de purge
- INVOKE `vec purge-orphan --yes` para purgar PERMANENTEMENTE órfãos
- INVOKE `vec stats --json` para inspecionar saúde das tabelas vec
- REGENERE schemas via `cargo run --bin dump-schema` (v1.0.89, ADR-0048)
- AGENDE semanal: `purge --retention-days 30 --yes` depois `cleanup-orphans --yes` depois `prune-relations --relation mentions --yes` depois `vacuum --json` depois `optimize --json` depois `sync-safe-copy --dest ~/backups/`
- DESDE v1.0.53 toda escrita executa `PRAGMA wal_checkpoint(TRUNCATE)` após commit
- SE corrupção ocorrer apesar do checkpoint: `sqlite3 broken.sqlite ".recover" | sqlite3 repaired.sqlite`


## Exemplos Prontos

### Exemplo 1 — Bootstrap de namespace de projeto
```bash
sqlite-graphrag init --namespace meuprojeto
sqlite-graphrag health --json | jaq '.integrity_ok'
sqlite-graphrag health --json | jaq '{schema_version, counts}'
```
- ESPERE: exit 0, `integrity_ok: true`, `schema_version >= 15`

### Exemplo 2 — Armazenar e recuperar memória
```bash
sqlite-graphrag remember --name decisao-auth --type decision \
  --description "JWT 15 min de expiração com fluxo de refresh" \
  --body-stdin <<'EOF'
Escolhemos JWT com 15 minutos de expiração porque:
- Refresh tokens são cookies HTTP-only
- 15min reduz blast radius de XSS
- Fluxo de refresh reemite tokens em atividade do usuário
EOF

sqlite-graphrag read --name decisao-auth --json | jaq '{description, body_length}'
```
- ESPERE: memória persistida, body contém texto completo, `body_length` > 100

### Exemplo 3 — Busca híbrida com expansão de grafo
```bash
sqlite-graphrag hybrid-search "autenticação JWT" --k 5 --with-graph --max-hops 2 --json \
  | jaq -r '(.results[] | .name), (.graph_matches[] | .name)' | sort -u
```
- ESPERE: top 5 resultados KNN+FTS5 fundidos mais 0-N vizinhos multi-hop

### Exemplo 4 — Ingest em massa de pasta de documentação
```bash
sqlite-graphrag ingest ./docs --recursive --type document \
  --pattern "*.md" --max-files 1000 --auto-describe --json \
  | jaq -c 'select(.status)' | jaq -s 'group_by(.status) | map({status: .[0].status, count: length})'
```
- ESPERE: progresso NDJSON; summary mostra `files_total`, `files_succeeded`, `files_failed`

### Exemplo 5 — Travessia de grafo a partir de entidade conhecida
```bash
sqlite-graphrag graph entities --json | jaq -r '.entities[].name' | head -10
sqlite-graphrag graph traverse --from jwt --depth 2 --json | jaq -r '.hops[] | "\(.entity) \(.relation)"'
```
- ESPERE: lista de entidades; travessia mostra vizinhança de 2 hops via relações canônicas

### Exemplo 6 — Pergunta de pesquisa profunda
```bash
sqlite-graphrag deep-research "Como o binário se autentica em providers OAuth?" \
  --k 20 --max-hops 3 --max-sub-queries 5 --json \
  | jaq '{stats, evidence_chains: (.evidence_chains | length)}'
```
- ESPERE: sub-queries decompostas, cadeias de evidência ligando seed ao alvo, graph_context populado

### Exemplo 7 — Extração de entidades curada por LLM
```bash
sqlite-graphrag --llm-model claude-sonnet-4-6 ingest ./corpus --mode claude-code --recursive --resume --json \
  | jaq -c 'select(.status == "done") | {file, entities, rels}'
```
- ESPERE: NDJSON por arquivo com `entities` count, `rels` count; `--resume` continua após interrupção

### Exemplo 8 — Diagnosticar falha preflight (exit 16)
```bash
CLAUDE_CONFIG_DIR=/tmp/bad-mcp sqlite-graphrag remember --name teste --type note --description x --body y 2>&1
echo "exit=$?"
sqlite-graphrag remember --name teste --type note --description x --body y 2>&1 || echo "exit=$?"
```
- ESPERE: primeira invocação retorna exit 16 com envelope `AppError::PreFlightFailed`
- ESPERE: segunda invocação sem diretório MCP ruim retorna exit 0

### Exemplo 9 — Recuperação de soft-delete
```bash
sqlite-graphrag forget --name decisao-auth
sqlite-graphrag history --name decisao-auth --json | jaq '.versions[0].deleted'
sqlite-graphrag restore --name decisao-auth
sqlite-graphrag recall "JWT" --k 3 --json | jaq '.results[].name'
```
- ESPERE: soft-delete esconde de recall; restore traz de volta; recall mostra novamente

### Exemplo 10 — Health check com filtro de namespace e tabelas vec
```bash
sqlite-graphrag health --namespace prod --json | jaq '{integrity_ok, schema_version, counts}'
sqlite-graphrag vec stats --json | jaq '.'
sqlite-graphrag embedding status --json | jaq '{pending, done, failed}'
```
- ESPERE: contagens escopadas para o namespace `prod`; saúde de tabelas vec; status da fila de embedding

### Exemplo 11 — Regenerar schemas JSON após mudanças de tipo
```bash
cargo run --bin dump-schema -- --check
git diff --stat docs/schemas/
cargo run --bin dump-schema  # se --check falhou
```
- ESPERE: `--check` sai com 0 quando schemas estão sincronizados; regeneração produz output idempotente

### Exemplo 12 — Pipeline de manutenção (semanal)
```bash
sqlite-graphrag purge --retention-days 30 --yes --dry-run
sqlite-graphrag cleanup-orphans --yes --dry-run
sqlite-graphrag prune-relations --relation mentions --yes --dry-run
sqlite-graphrag vacuum --json
sqlite-graphrag optimize --json
sqlite-graphrag sync-safe-copy --dest ~/backups/graphrag-$(date +%Y%m%d).sqlite
```
- ESPERE: cada dry-run reporta contagens; pipeline completo reclama espaço e gera snapshot seguro


### Exemplo 13 — Inspecionar whitelist de modelos Codex (v1.0.89, alias no-op, GAP-E2E-010a)
```bash
sqlite-graphrag codex-models --json | jaq '{count, default, models: .models[:3]}'
sqlite-graphrag codex-models  # modo texto para humanos
sqlite-graphrag codex-models --json | jaq '.models | length'
```
- ESPERE: envelope JSON `{"action":"codex_models","count":N,"default":"gpt-5.5","models":[...]}`
- ESPERE: modo texto emite lista legível de modelos suportados
- USE ao validar que o escopo OAuth atual inclui os nomes de modelo codex necessários

### Exemplo 14 — Health check escopado para um namespace (v1.0.89, GAP-E2E-002)
```bash
sqlite-graphrag health --namespace prod --json | jaq '{integrity_ok, schema_version, counts}'
sqlite-graphrag health --namespace dev --json | jaq '.counts'  # contagens diferentes
sqlite-graphrag health --json | jaq '.counts'  # contagens globais
```
- ESPERE: contagens filtradas para o namespace especificado; campos integrity e schema_version inalterados
- USE em ambientes multi-tenant para verificar isolamento por namespace
- REGRA DE OMISSÃO: quando `--namespace` é omitido, contagens agregam entre todos namespaces (visão global)

### Exemplo 15 — Preview de migração em dry-run (v1.0.89, GAP-E2E-009)
```bash
sqlite-graphrag migrate --dry-run --json | jaq '.would_apply[]? | {name, version}'
sqlite-graphrag migrate --to-llm-only --drop-vec-tables --dry-run --json | jaq '.'
sqlite-graphrag migrate --dry-run --json  # sempre faça PREVIEW antes de migrações destrutivas
```
- ESPERE: lista de migrações pendentes com nome+versão sem aplicá-las; banco permanece inalterado
- ESPERE: `--to-llm-only --dry-run` reporta plano de drop de tabelas vec sem executar
- USE em pipelines CI e antes de qualquer passo de migração irreversível


## Referências para Documentação Estendida

Para detalhes além do escopo de uso diário desta skill, os seguintes documentos do projeto estendem a cobertura:

- `docs/HOW_TO_USE.md` — quickstart, instalação, workflows comuns
- `docs/COOKBOOK.md` — 50+ receitas para padrões avançados (diagnóstico preflight, recovery de drift de schema, etc.)
- `docs/MIGRATION.md` — caminhos de upgrade entre versões
- `docs/CROSS_PLATFORM.md` — comportamento em Linux, macOS, Windows ARM64
- `docs/AGENTS.pt-BR.md` — documentação PT-BR estendida para agentes de IA
- `docs/schemas/*.schema.json` — contratos JSON Schema completos (versionados por SemVer)
- `docs/decisions/adr-*.md` — Architecture Decision Records (justificativas para cada escolha de design)
- `llms-full.txt` — dump completo de contexto LLM com todas as regras
- `gaps.md` — gaps abertos e fechados atualmente
- `CHANGELOG.md` — release notes versão por versão
- `Cargo.toml` — metadados do pacote e documentação de tamanho do binário (14.6 MiB)


## Resumo de Regras Ativas e Anti-padrões
- NUNCA passe `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` (OAuth-only, exit 1)
- NUNCA dependa do daemon ou use flag `--bare` (REMOVIDOS v1.0.76 e v1.0.79)
- NUNCA instale com `--features embedding-legacy` ou `--features ner-legacy` (REMOVIDOS)
- NUNCA use crates `fastembed`, `tokenizers`, `sqlite-vec`, ou `GLiNER`
- NUNCA espere KNN sqlite-vec; cosine é pure Rust em `src/similarity.rs`
- NUNCA rode `enrich` em paralelo contra mesmo banco (job singleton via `lock::acquire_job_singleton`)
- NUNCA escreva no arquivo `.sqlite` fora do binário
- NUNCA ignore exit 19 (envelope SHUTDOWN_EXIT_CODE); trabalho parcial descartado, RETRY OBRIGATÓRIO
- NUNCA ignore exit 16 (falha preflight); corrija config MCP ou `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1`
- NUNCA duplique conteúdo já existente em `CHANGELOG.md`
- NUNCA use `mentions` como relação padrão de grafo
- NUNCA passe corpo vazio via `--graph-stdin` (exit 1 desde v1.0.54)
- NUNCA use `--gliner-variant` (no-op desde v1.0.79)
- NUNCA chame `migrate --to-llm-only` sem guarda de segurança `--drop-vec-tables`
- NUNCA ignore flag `--wait-lock` quando contenção for esperada
- NUNCA assuma exit 1 igual a exit 9 (validação vs duplicada)
- NUNCA assuma que o tamanho do binário é 6 MB; o real é 14.6 MiB stripped ELF
