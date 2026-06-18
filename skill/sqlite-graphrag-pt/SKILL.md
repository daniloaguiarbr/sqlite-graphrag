---
name: sqlite-graphrag
description:Para memória persistente, GraphRAG, ou contexto de longo prazo em Claude Code, Codex, Cursor, Windsurf, agentes AI. Em: lembrar disso, salvar conversa, recuperar contexto, busca híbrida, grafo de entidades, memória SQLite, RAG local, embedding LLM-only, fluxo OAuth, embedding BLOB-backed, migrate to-llm-only, migrate rehash, drop de vec tables, embedding-dim, llm-parallelism, embedding em lote, re-embed, force-reembed, gaps G28-G58, OAuth-only, aborto ANTHROPIC_API_KEY/OPENAI_API_KEY, endurecimento Claude/Codex, Mock LLM CI, sem daemon, A1/A2, ADR-0032/0033/0034, G45-G58, 5 gaps v1.0.82 (GAP-001..005), pending-embeddings, subcomandos slots/pending/embedding, migrações V014/V015, llm-max-host-concurrency, llm-slot-wait-secs, graceful-shutdown-secs, SHUTDOWN_EXIT_CODE, codex login, ADR-0041 v1.0.83, ANTHROPIC_AUTH_TOKEN, ANTHROPIC_BASE_URL, Minimax, OpenRouter, AWS Bedrock, --dry-run-backend, backend_invoked, vec_degraded_reason, embed_via_claude_local, LlmEmbeddingBuilder, GAP-003 circuit breaker de slots, G58 fallback determinístico OAuth, G45-CR5 headers anthropic-ratelimit, G55 NotFound bilíngue, v1.0.84, v1.0.85. KW: memória RAG GraphRAG SQLite one-shot OAuth offline persistente grafo entidade v1.0.82 v1.0.83 v1.0.84 v1.0.85.
---


## Princípios Fundamentais
- INVOKE sempre como subprocesso via `std::process::Command`
- READ stdout para dados estruturados JSON ou NDJSON
- READ stderr para logs de tracing e mensagens humanas
- CHECK exit code ANTES de parsear stdout
- TRUST em contratos JSON como API versionada por SemVer
- BUILD é LLM-only e one-shot; binário tem ~6 MB
- BUILD NÃO tem daemon, NÃO tem ONNX runtime, NÃO tem cache de modelo
- OAUTH-ONLY: spawn ABORTA exit 1 se `ANTHROPIC_API_KEY` estiver setada
- OAUTH-ONLY: spawn ABORTA exit 1 se `OPENAI_API_KEY` estiver setada
- NAMESPACE por projeto via `--namespace <ns>` ou env
- NAMESPACE default é `global` quando omitido
- NUNCA expor o binário como servidor MCP ou serviço HTTP
- NUNCA escrever arquivo `.sqlite` em paralelo ao binário
- NUNCA editar o arquivo `.sqlite` a partir de outra ferramenta


## Inicialização, Saúde e Config Global
- EXECUTE `sqlite-graphrag init --namespace <ns>` no primeiro uso
- EXECUTE `health --json` para verificar `integrity_ok` e `schema_ok`
- VERIFIQUE `schema_version >= 15` após `init` ou `migrate`
- EXECUTE `migrate --json` após cada upgrade do binário
- USE `migrate --to-llm-only --drop-vec-tables --json` para bancos v1.0.74 ou v1.0.75
- USE `migrate --rehash --json` para reparar drift de checksum SipHasher13 V002
- TRATE exit code 10 como erro de banco; execute `vacuum` e `health`
- TRATE exit code 15 como ocupado; amplie `--wait-lock`
- ABORTE pipeline quando `integrity_ok` retornar `false`
- EXECUTE `optimize --json` para refrescar estatísticas do planner; resposta inclui `fts_rebuilt`
- USE `optimize --skip-fts --json` quando FTS5 foi reconstruído recentemente
- EXECUTE `fts rebuild --json` quando `health.fts_degraded` for true
- INSPEIONE `wal_size_mb` em `health` para fragmentação
- VERIFIQUE `journal_mode` igual a `wal` em produção
- USE `debug-schema --json` para troubleshooting de drift de schema
- PASSE `--db <PATH>` para sobrescrever localização do banco
- DEFINA env `SQLITE_GRAPHRAG_DB_PATH` para configuração persistente
- PASSE `--namespace <ns>` para isolar dados do projeto
- DEFINA env `SQLITE_GRAPHRAG_NAMESPACE` para namespace persistente
- PASSE `--lang en` ou `--lang pt` para forçar idioma do stderr
- PASSE `--tz America/Sao_Paulo` para localizar timestamps
- DEFINA env `SQLITE_GRAPHRAG_DISPLAY_TZ` para timezone persistente
- DEFINA `SQLITE_GRAPHRAG_LOG_FORMAT=json` para agregadores de log
- USE `-v` para info, `-vv` para debug, `-vvv` para trace
- ATIVE `SQLITE_GRAPHRAG_LOW_MEMORY=1` em containers restritos
- DEFINA env `SQLITE_GRAPHRAG_EMBEDDING_DIM` na faixa `[8, 4096]`
- DEFINA `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` para modo compliance (ADR-0041)
- DEFINA `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` APENAS para harnesses de teste CI
- VALORES válidos de `--type`: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- FLAGS globais: `--db`, `--namespace`, `--lang`, `--tz`, `--json`, `--low-memory`, `--max-concurrency N`, `--wait-lock SECS`, `--llm-parallelism N`, `--llm-backend claude|codex|none|auto[,fallback...]`, `--dry-run-backend`, `--llm-fallback-mode <claude|codex>`, `--graceful-shutdown-secs N`


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
- FLAGS de endurecimento para claude: `--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions --output-schema`
- FLAGS de endurecimento para codex: `--json --output-schema --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules -c mcp_servers='{}' --ask-for-approval never`
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
- VALIDE nomes: mínimo 2 chars, sem newlines, sem ALL_CAPS curtos
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
- ESPERE resposta `hybrid-search` (v1.0.84+): `results[]`, `graph_matches[]`, `fts_degraded`, `vec_degraded_reason?`, `backend_invoked`, `elapsed_ms`
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
- TOP campos `recall` (v1.0.84+): adicionar `backend_invoked`, `vec_degraded_reason?`
- TOP campos `hybrid-search` (v1.0.84+): adicionar `backend_invoked`, `vec_degraded_reason?`
- Envelopes `remember`/`edit`/`ingest`/`enrich`/`read` (v1.0.84+): incluem `backend_invoked`
- TODOS schemas usam `"additionalProperties": false` (API JSON versionada por SemVer)
- SCHEMAS completos em `docs/schemas/*.schema.json` (nunca inline schema completo em skill)


## Códigos de Saída e Retry
- EXIT 0 significa sucesso; parsee stdout
- EXIT 1 significa erro de validação (peso inválido, self-link, max-files excedido)
- EXIT 2 significa erro de parsing de argumento Clap
- EXIT 3 significa conflito de optimistic lock; recarregue `read --json` e retente
- EXIT 4 significa entidade, memória ou versão não encontrada
- EXIT 5 significa erro de namespace
- EXIT 6 significa payload acima do limite de tamanho
- EXIT 9 significa memória duplicada (use `--force-merge` para update ou restore)
- EXIT 10 significa erro de banco; execute `vacuum` e `health`
- EXIT 11 significa falha de embedding (erro de subprocesso LLM)
- EXIT 13 significa falha parcial de batch; reprocesse apenas os que falharam
- EXIT 14 significa erro de I/O (permissão, disco cheio)
- EXIT 15 significa banco ocupado; amplie `--wait-lock`
- EXIT 19 significa SHUTDOWN_EXIT_CODE (ADR-0037); trabalho parcial descartado; RETRY OBRIGATÓRIO
- EXIT 19 envelope: `{error:true, code:19, signal, graceful, message}`
- EXIT 20 significa erro interno ou falha de serialização JSON
- EXIT 75 significa slots esgotados OU `JobSingletonLocked`
- EXIT 75 de `enrich`/`ingest --mode claude-code|codex`: parsee `job '(\w+)'.*namespace '(\w+)'`
- EXIT 75 v1.0.85 GAP-003 circuit breaker: respeite janela de cooldown por namespace; NÃO retente imediatamente
- EXIT 77 significa pressão de RAM; aguarde memória livre
- NUNCA ignore exit code não-zero como sucesso
- NUNCA reprocesse batch inteiro após exit 13
- NUNCA aumente concorrência após exit 75 ou 77
- NUNCA confunda exit 1 (validação) com exit 9 (duplicada)


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


## Superfície v1.0.82+ (pending, slots, embedding, llm-backend, shutdown, campos v1.0.84/85)
- INVOKE `pending list --filter-status queued` para inspecionar fila de checkpoint de três estágios do remember
- INVOKE `pending show <id>` para inspecionar linha única de checkpoint
- INVOKE `pending cleanup --yes` para remover linhas em estado terminal
- RESPALDADO pela tabela `pending_memories` criada pela migração V014 (ADR-0036)
- INVOKE `pending-embeddings list` para inspecionar fila de retry de embeddings que falharam
- INVOKE `pending-embeddings process` para reprocessar com próximo backend
- RESPALDADO pela tabela `pending_embeddings` criada pela migração V015 (ADR-0040)
- INVOKE `slots status` para inspecionar semáforo de slots host-wide
- INVOKE `slots release --slot-id <N> --yes` para colher slots órfãos
- LOCK via `fs4 = "0.9"` com `fcntl(F_SETLK)` em Unix e `LockFileEx` em Windows (ADR-0039)
- INVOKE `embedding status` para contagens agregadas por status
- INVOKE `embedding list` para inspeção por entrada
- PASSE `--llm-backend codex,claude` para codex-primeiro com fallback claude (ADR-0038)
- PASSE `--llm-backend codex,claude,none` para fallback de embedding null
- DEFAULT de `--llm-backend` é `codex`
- PASSE `--llm-fallback-mode <claude|codex>` para trocar backend mid-job em rate-limit
- ESPERE v1.0.85 G58 fallback determinístico quando backend alt listado em `--llm-backend codex,claude`
- PASSE `--graceful-shutdown-secs N` para reservar orçamento de cleanup antes de SIGKILL
- PASSE `--skip-embedding-on-failure` APENAS quando `--llm-backend …,none`
- PASSE ADR-0041 `--strict-env-clear` para descartar credenciais de provider customizado em subprocesso
- EXECUTE `codex login` após upgrade para refrescar refresh token OAuth (incidente 2026-06-14)
- AÇÃO do operador para OAuth stale: `codex login` depois retry
- v1.0.84: PASSE `--dry-run-backend` para planejar operação de backend sem executá-la (preview idempotente)
- v1.0.84: PARSEE campo `backend_invoked` nos envelopes de recall, hybrid-search, remember, edit, ingest, enrich, read para confirmar backend efetivo
- v1.0.84: LEIA `vec_degraded_reason` nos envelopes de recall/hybrid-search quando caminho vec estiver degradado
- v1.0.84: SAIBA que backend claude divide-se em embedder local via `embed_via_claude_local` (zero-token, compatível com OAuth)
- v1.0.84: USE `LlmEmbeddingBuilder` para compor pipeline de embedding: `with_backend(Codex).or_fallback(Claude).or_skip()`
- v1.0.85 GAP-003: RESPEITE circuit breaker de exaustão de slots; em exit 75, faça backoff por cooldown de namespace antes de retentar
- v1.0.85 G58: ESPERE fallback determinístico de cota OAuth quando backend alt declarado na lista `--llm-backend`
- v1.0.85 G45-CR5: CAPTURE `anthropic-ratelimit-requests-remaining`, `anthropic-ratelimit-tokens-remaining`, `anthropic-ratelimit-input-tokens-reset`, `anthropic-ratelimit-output-tokens-reset` dos headers de resposta no envelope
- v1.0.85 G55: ESPERE NotFound bilíngue de `read --name <missing>` baseado em `--lang`: EN emite `Memory not found`, PT emite `Memória não encontrada`
- v1.0.85 G56: DIM de embedding default é 64 (MRL) quando `SQLITE_GRAPHRAG_EMBEDDING_DIM` não setado e `schema_meta.dim` ausente
- v1.0.85.1: SAIBA que `recall --llm-backend none` e `hybrid-search --llm-backend none` retornam exit 0 com `vec_degraded_reason: "dim_zero"` (hotfix GAP-004)
- v1.0.85.2: USE `--dry-run-backend` standalone sem subcomando (BUG-001); `setup_mock_path()` emite JSON para claude e JSONL para codex (BUG-002); o campo `backend_invoked` em 7 envelopes reflete o backend RESOLVIDO (BUG-003)


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
- INVOKE `debug-schema --json` (oculto do `--help`) para inspecionar estado de schema
- INVOKE `completions <bash|zsh|fish|elvish|powershell>` para gerar completions de shell
- INVOKE `vec orphan-list --json` para listar vetores órfãos de memória
- INVOKE `vec purge-orphan --yes --dry-run` para PREVIEW de purge
- INVOKE `vec purge-orphan --yes` para purgar PERMANENTEMENTE órfãos
- INVOKE `vec stats --json` para inspecionar saúde das tabelas vec
- AGENDE semanal: `purge --retention-days 30 --yes` depois `cleanup-orphans --yes` depois `prune-relations --relation mentions --yes` depois `vacuum --json` depois `optimize --json` depois `sync-safe-copy --dest ~/backups/`
- DESDE v1.0.53 toda escrita executa `PRAGMA wal_checkpoint(TRUNCATE)` após commit
- SE corrupção ocorrer apesar do checkpoint: `sqlite3 broken.sqlite ".recover" | sqlite3 repaired.sqlite`


## Resumo de Regras Ativas e Anti-padrões
- NUNCA passe `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` (OAuth-only, exit 1)
- NUNCA dependa do daemon ou use flag `--bare` (REMOVIDOS v1.0.76 e v1.0.79)
- NUNCA instale com `--features embedding-legacy` ou `--features ner-legacy` (REMOVIDOS)
- NUNCA use crates `fastembed`, `tokenizers`, `sqlite-vec`, ou `GLiNER`
- NUNCA espere KNN sqlite-vec; cosine é pure Rust em `src/similarity.rs`
- NUNCA rode `enrich` em paralelo contra mesmo banco (job singleton via `lock::acquire_job_singleton`)
- NUNCA escreva no arquivo `.sqlite` fora do binário
- NUNCA ignore exit 19 (envelope SHUTDOWN_EXIT_CODE); trabalho parcial descartado, RETRY OBRIGATÓRIO
- NUNCA duplique conteúdo já existente em `CHANGELOG.md`
- NUNCA use `mentions` como relação padrão de grafo
- NUNCA passe corpo vazio via `--graph-stdin` (exit 1 desde v1.0.54)
- NUNCA use `--gliner-variant` (no-op desde v1.0.79)
- NUNCA chame `migrate --to-llm-only` sem guarda de segurança `--drop-vec-tables`
- NUNCA ignore flag `--wait-lock` quando contenção for esperada
- NUNCA assuma exit 1 igual a exit 9 (validação vs duplicada)
