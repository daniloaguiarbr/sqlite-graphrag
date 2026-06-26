---
name: sqlite-graphrag
description: Esta skill DEVE ativar para operações da CLI sqlite-graphrag incluindo memória persistente, GraphRAG, grafo de entidades, busca híbrida, recall, remember, ingest, enrich, deep-research, embedding via openrouter, seleção de backend codex claude opencode, validação preflight, FTS5, similaridade cosine, gestão de namespace, migração e manutenção. Ativa em palavras-chave memória RAG GraphRAG SQLite entidade grafo embedding codex claude opencode openrouter remember recall hybrid-search ingest enrich forget purge link
---


## Quando Esta Skill Ativa
- ATIVE quando o usuário pede para lembrar, salvar, recordar, recuperar, buscar ou persistir algo entre sessões
- ATIVE para contexto de longo prazo, grafo de conhecimento, GraphRAG, RAG, ligação de entidades, gestão de memória
- ATIVE quando sqlite-graphrag, embedding, FTS5, hybrid-search, openrouter ou memória LLM for mencionado
- NUNCA ATIVE para dados efêmeros pontuais, I/O simples de arquivo ou tarefas sem relação a contexto persistente


## Regras de Instrução para LLMs
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
- SEMPRE use APENAS relações canônicas: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- SEMPRE mapeie não-canônicas: `adds|creates → causes`, `implements → supports`, `blocks → contradicts`, `tested-by → related`, `part-of → applies-to`
- SEMPRE normalize nomes de entidade para kebab-case ASCII lowercase ANTES de passar à CLI
- NUNCA use MCP Serena ou `.md` para persistência; NUNCA escreva MEMORY.md
- NUNCA inicie daemon; NUNCA passe `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY`
- PREFIRA `remember --force-merge` sobre `edit`; PREFIRA `--graph-stdin` sobre `--enable-ner`
- LIMITE entidades a conceitos de domínio; REJEITE palavras genéricas, pronomes, UUIDs, timestamps


## Arquitetura e Princípios
- INVOKE sempre como subprocesso; READ stdout para JSON/NDJSON; READ stderr para logs; CHECK exit code ANTES de parsear
- SAIBA que binário NÃO tem daemon, NÃO tem ONNX runtime, NÃO tem cache de modelo
- SAIBA que similaridade COSINE é pure Rust sobre BLOB-backed `memory_embeddings`, `entity_embeddings`, `chunk_embeddings`
- SAIBA que SCHEMA é v15 após `init` ou `migrate` em banco fresco
- ENFORCE OAUTH-ONLY: spawn ABORTA exit 1 se `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estiver definida
- SAIBA que `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL` são PRESERVADAS para providers customizados (OpenRouter, Bedrock)
- SAIBA que CWD do subprocesso é ISOLADO via `apply_cwd_isolation`; órfãos limpos via `cleanup_isolation_dirs`
- SAIBA que 7 guards preflight rodam ANTES de cada fork LLM: `check_argv_size`, `check_binary_exists`, `check_mcp_config_inline`, `check_mcp_config_path`, `check_walkup_mcp_json`, `check_output_buffer`, `check_claude_config_dir`
- SAIBA que exit 16 (`EX_CONFIG`) é falha preflight universal; LEIA envelope para remediação por variante
- DEFINA `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` APENAS em emergências
- ISOLE NAMESPACE por projeto via `--namespace <ns>` ou env; padrão é `global`
- NUNCA exponha o binário como servidor MCP ou serviço HTTP
- NUNCA escreva arquivo `.sqlite` em paralelo ao binário ou de outra ferramenta
- USE MOCK LLM CLI para CI: prefixe `tests/mock-llm` ao PATH


## Seleção de Backend de Embedding
- SAIBA que `--embedding-backend` é SEPARADO de `--llm-backend`; embedding e extração de entidades são independentes
- PASSE `--embedding-backend openrouter` para usar API REST do OpenRouter (~100-500ms vs 20-60s do subprocesso LLM)
- PASSE `--embedding-backend llm` para delegar embedding ao subprocesso LLM configurado via `--llm-backend`
- PASSE `--embedding-backend auto` para seleção automática baseada em configuração disponível
- PASSE `--llm-backend codex` para spawnar Codex CLI headless (backend padrão de extração)
- PASSE `--llm-backend claude` para spawnar Claude Code headless via `embed_via_claude_local` (zero-token, OAuth)
- PASSE `--llm-backend opencode` para spawnar OpenCode CLI headless (auth próprio, NÃO OAuth)
- PASSE `--llm-backend codex,claude` para codex-primeiro com fallback claude
- PASSE `--llm-backend codex,claude,opencode,none` para cadeia completa de fallback com embedding null como último recurso
- PASSE `--llm-model <MODEL>` para selecionar modelo do subprocesso LLM ativo
- SAIBA modelos padrão de subprocesso: codex=`gpt-5.5`, claude=`claude-sonnet-4-6`, opencode=`opencode/big-pickle`
- PASSE `--llm-fallback-mode <claude|codex|opencode>` para trocar backend mid-job em rate-limit
- PASSE `--skip-embedding-on-failure` APENAS quando `--llm-backend …,none` está ativo
- PASSE `--dry-run-backend` para planejar operação sem executar (preview idempotente)
- PARSEE campo `backend_invoked` em todo envelope de embedding para CONFIRMAR qual backend rodou
- PASSE `--codex-binary <PATH>`, `--claude-binary <PATH>`, `--opencode-binary <PATH>` para sobrescrever paths dos binários
- SAIBA modelos gratuitos opencode: `opencode/big-pickle`, `opencode/deepseek-v4-flash-free`, `opencode/mimo-v2.5-free`, `opencode/nemotron-3-ultra-free`, `opencode/north-mini-code-free`
- EXECUTE `codex login` para refrescar OAuth do codex; refresque OAuth do claude quando stale
- NUNCA passe API keys com qualquer subprocesso LLM; spawn ABORTA exit 1


## Configuração e Verificação de Modelos OpenRouter
- PASSE `--openrouter-api-key <KEY>` ou DEFINA variável `OPENROUTER_API_KEY` para autenticação
- SAIBA que `OPENROUTER_API_KEY` é tratada via `secrecy::SecretString` com zeroize-on-drop — JAMAIS logada
- SAIBA que `--embedding-model` é OBRIGATÓRIO quando `--embedding-backend openrouter` — NÃO existe modelo padrão
- SAIBA que exit code 78 (EX_CONFIG) é retornado para API key ausente, modelo ausente ou key inválida
- SAIBA que truncamento MRL é aplicado ao `--embedding-dim` configurado (padrão 64)
- SAIBA que `--embedding-backend openrouter` é propagado para TODOS os 13 paths de embedding
- SAIBA 10 modelos verificados com OpenRouter:
- `google/gemini-embedding-001`
- `google/gemini-embedding-2`
- `mistralai/mistral-embed-2312`
- `qwen/qwen3-embedding-8b`
- `qwen/qwen3-embedding-4b`
- `openai/text-embedding-3-small`
- `nvidia/llama-nemotron-embed-vl-1b-v2:free`
- `baai/bge-m3`
- `openai/text-embedding-3-large`
- `perplexity/pplx-embed-v1-0.6b`
- INVOKE `sqlite-graphrag codex-models --json` para inspecionar whitelist de modelos verificados com informações de compatibilidade


## Gestão de Chave de API OpenRouter
- EXECUTE `echo "sk-or-v1-..." | sqlite-graphrag config add-key --provider openrouter --from-stdin` para registrar nova chave
- EXECUTE `sqlite-graphrag config list-keys --json` para listar chaves registradas
- EXECUTE `sqlite-graphrag config remove-key <fingerprint> --json` para remover chave por fingerprint
- EXECUTE `sqlite-graphrag config doctor --json` para diagnosticar configuração e validar chaves
- EXECUTE `sqlite-graphrag config path` para exibir caminho do arquivo de configuração
- SAIBA que chaves são armazenadas no XDG config (`~/.config/sqlite-graphrag/config.toml`) com `chmod 600`
- SAIBA que precedência é: variável de ambiente > config.toml > flag CLI
- NUNCA passe API key como argumento CLI em produção — use stdin ou variável de ambiente para evitar exposição no histórico do shell


## Referência de Flags Globais
- `--db <PATH>` — sobrescrever localização do banco (NÃO é global; cada subcomando aceita independentemente)
- `--namespace <ns>` — escopar operações para um namespace
- `--lang en|pt` — forçar idioma do stderr
- `--tz <TIMEZONE>` — localizar timestamps
- `--json` — saída JSON estruturada (SEMPRE passe)
- `--low-memory` — paralelismo unitário para containers restritos
- `--max-concurrency N` — cap de invocações CLI pesadas concorrentes
- `--wait-lock SECS` — ampliar janela de aquisição de lock
- `--llm-parallelism N` — cap de fan-out de subprocessos de embedding (padrão 4, clamp [1, 32])
- `--embedding-backend auto|openrouter|llm` — seleção de backend de embedding
- `--embedding-model <MODEL>` — modelo de embedding para OpenRouter (OBRIGATÓRIO com openrouter)
- `--openrouter-api-key <KEY>` — chave de API do OpenRouter
- `--llm-backend <chain>` — seleção de backend de subprocesso LLM com fallback separado por vírgula
- `--llm-model <MODEL>` — modelo de subprocesso LLM
- `--llm-fallback <chain>` — cadeia de fallback tentada quando primário falha (padrão `codex,claude,none`)
- `--embedding-dim N` — override de dimensionalidade de embedding [8, 4096] (padrão 64 MRL)
- `--graceful-shutdown-secs N` — orçamento de cleanup antes de SIGKILL
- `--skip-embedding-on-failure` — exit 0 em falha de embedding (APENAS com fallback terminando em `none`)
- `-v`/`-vv`/`-vvv` — logging info/debug/trace no stderr


## Operações CRUD de Escrita
- INVOKE `remember --name <kebab> --type <kind> --description <text>` com `--body <text>` ou `--body-file <path>` ou `--body-stdin`
- INVOKE `remember --graph-stdin` para anexar `{body, entities, relationships}` em único JSON
- PASSE entities como `[{name, entity_type}]` em kebab-case ASCII
- PASSE relationships como `[{source, target, relation, strength}]` onde `strength em [0.0, 1.0]`
- PASSE `--force-merge` para updates idempotentes e restauração de soft-deleted
- PASSE `--dry-run` para validar inputs sem persistir
- RESPEITE limite de 512000 bytes e 512 chunks por corpo
- VALORES válidos de `--type`: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- INVOKE `remember-batch` para 10+ memórias via NDJSON stdin
- INVOKE `ingest <DIR> --recursive --pattern "*.md"` para importar diretório
- PASSE `--mode codex|claude-code|opencode` para extração de entidades curada por LLM
- USE `--resume` para continuar da fila após interrupção; `--retry-failed` para apenas falhados
- PASSE `--enrich-after` no `ingest` para disparar `enrich --operation memory-bindings` após ingestão
- PASSE `--low-memory` em `ingest` para modo single-threaded (<4 GB RAM)
- RESPEITE cap `--max-files 10000` como validação all-or-nothing
- NUNCA misture `--body`, `--body-file`, `--body-stdin`, `--graph-stdin` em única invocação
- NUNCA use `fd | xargs remember`; INVOKE `ingest` em vez disso
- NUNCA use `--force-merge` em `ingest` (exclusivo de `remember`)


## Operações CRUD de Leitura Atualização e Deleção
- INVOKE `read --name <kebab> --json` para fetch O(1); `read --id <N>` por memory_id; `--with-graph` para entidades vinculadas
- INVOKE `list --type <kind> --limit N --offset N --json`; `--include-deleted` para soft-deleted
- INVOKE `history --name <n> --diff --json` para versões com diff de caracteres
- INVOKE `edit --name <n> --body-file <path>` para atualizar corpo (re-embeda automaticamente)
- USE `--force-reembed` para regenerar embedding sem mudar corpo
- USE `--expected-updated-at <ts>` para optimistic locking; TRATE exit 3 como conflito
- INVOKE `rename --from <old> --to <new>` para renomear preservando histórico
- INVOKE `restore --name <n> --version <N>` para restaurar versão anterior
- INVOKE `forget --name <n>` para soft-delete reversível; TRATE exit 4 como ausente
- INVOKE `purge --retention-days <N> --yes` para hard delete; USE `--dry-run` primeiro
- INVOKE `unlink --from <a> --to <b> --relation <type>` para remover aresta
- INVOKE `cleanup-orphans --yes` após bulk forget; depois `vacuum --json`
- NUNCA pule optimistic locking em pipelines concorrentes
- NUNCA delete manualmente via shell `sqlite3`


## Operações de Grafo de Entidades
- INVOKE `link --from <a> --to <b> --relation <type> --create-missing --weight <float>` para criar aresta
- PASSE `--entity-type <kind>` para entidades auto-criadas (padrão `concept`)
- USE `--strict-relations` para falhar em tipos de relação não-canônicos
- INVOKE `graph entities --json` para listar entidades; ACESSE via `.entities[]` (NÃO `.items[]`)
- ORDENE via `--sort-by degree|name|created_at`; PAGINE via `--limit N --offset N` em `graph entities`
- INVOKE `graph stats --json` para inspecionar `node_count`, `edge_count`, `avg_degree`, `max_degree`
- INVOKE `graph traverse --from <root> --depth <N> --json` para travessia de subgrafo
- USE `--format json|dot|mermaid` com `--output <path>` para exportar grafo
- INVOKE `rename-entity`, `delete-entity --cascade`, `merge-entities --names "a,b,c" --into <target>`
- INVOKE `reclassify --name <n> --new-type <kind>` ou `--from-type <old> --to-type <new> --batch`
- INVOKE `reclassify-relation --from-relation <antiga> --to-relation <nova> --batch` para migração em massa de tipos de relação
- INVOKE `normalize-entities --yes` para normalizar todos nomes para kebab-case ASCII
- INVOKE `prune-ner --entity <n>` para remover bindings NER; `prune-ner --all --yes` para todos no namespace
- INVOKE `memory-entities --name <memory>` para lookup forward de entidades vinculadas; `--entity <name>` para lookup reverso
- PASSE `--max-entity-degree N` em `link` para avisar quando entidade exceder N conexões
- RELAÇÕES canônicas: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- TIPOS canônicos de entidade: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- NUNCA use `mentions` como relação padrão


## Operações de Busca GraphRAG
- USE padrão canônico de três camadas: `hybrid-search` depois `read --name` depois `related|graph traverse`
- INVOKE `recall <query> --k N` para busca semântica pura KNN; PASSE `--no-graph` para desabilitar expansão de grafo
- INTERPRETE `distance` crescente como similaridade decrescente; `score` = `1.0 - distance` clamped [0.0, 1.0]
- INVOKE `hybrid-search <query> --k N` para fusão FTS5+KNN via RRF
- PASSE `--rrf-k 60` para fusão padrão; `--weight-vec 1.0 --weight-fts 1.0` para balanceada
- PASSE `--fallback-fts-only` para pular embedding ao vivo e servir apenas FTS5 BM25 (modo offline)
- USE `--with-graph --max-hops 2 --min-weight 0.3` para expansão de grafo; LEIA `results[]` E `graph_matches[]`
- INVOKE `related <name> --hops N` para travessia multi-hop a partir de memória
- INVOKE `deep-research "<query>" --k 20 --max-hops 3 --max-sub-queries 7 --max-results 50` para pesquisa paralela multi-hop
- PASSE `--with-bodies` para incluir corpos completos de memórias nos resultados
- INVOKE `enrich --operation <op>` para qualidade de grafo via LLM: `memory-bindings`, `entity-descriptions`, `body-enrich`, `re-embed --limit N --resume`
- PARSEE top campos: `recall` retorna `results[].{name, snippet, distance, score, source}`; `hybrid-search` retorna `results[].{name, combined_score, vec_rank, fts_rank}`
- NUNCA confunda `distance` com `combined_score` em ranking
- NUNCA aumente `--hops` sem inspecionar `graph stats` antes


## Fórmulas de Embedding OpenRouter
- REMEMBER: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember --name <n> --type decision --description "desc" --body "texto" --json`
- REMEMBER-BATCH: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember-batch --json`
- INGEST: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY ingest ./docs --recursive --pattern "*.md" --enrich-after --json`
- EDIT: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY edit --name <n> --body-file novo.md --json`
- RESTORE: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY restore --name <n> --version 2 --json`
- RECALL: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY recall "query" --k 10 --json`
- HYBRID-SEARCH: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY hybrid-search "query" --k 10 --with-graph --json`
- DEEP-RESEARCH: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY deep-research "query" --k 20 --max-hops 3 --json`
- RENAME-ENTITY: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY rename-entity --from <antigo> --to <novo> --json`
- ENRICH re-embed: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend codex --llm-model gpt-5.4-mini enrich --operation re-embed --limit 100 --json`
- INIT: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY init --namespace <ns>`


## Pipelines de Enrichment — Embedding OpenRouter e Enrich via LLM
- REMEMBER depois ENRICH via codex: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember --name <n> --type decision --description "desc" --body "texto" --json && sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --json`
- REMEMBER depois ENRICH via claude: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember --name <n> --type decision --description "desc" --body "texto" --json && sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --json`
- REMEMBER depois ENRICH via opencode: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember --name <n> --type decision --description "desc" --body "texto" --json && sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --json`
- REMEMBER-BATCH depois ENRICH via codex: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember-batch --json && sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --json`
- REMEMBER-BATCH depois ENRICH via claude: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember-batch --json && sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --json`
- REMEMBER-BATCH depois ENRICH via opencode: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember-batch --json && sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --json`
- INGEST depois ENRICH via codex: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY ingest ./docs --recursive --pattern "*.md" --json && sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --json`
- INGEST depois ENRICH via claude: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY ingest ./docs --recursive --pattern "*.md" --json && sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --json`
- INGEST depois ENRICH via opencode: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY ingest ./docs --recursive --pattern "*.md" --json && sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --json`
- EDIT depois ENRICH via codex: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY edit --name <n> --body-file novo.md --json && sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --json`
- EDIT depois ENRICH via claude: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY edit --name <n> --body-file novo.md --json && sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --json`
- EDIT depois ENRICH via opencode: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY edit --name <n> --body-file novo.md --json && sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --json`
- RESTORE depois ENRICH via codex: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY restore --name <n> --version 2 --json && sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --json`
- RESTORE depois ENRICH via claude: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY restore --name <n> --version 2 --json && sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --json`
- RESTORE depois ENRICH via opencode: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY restore --name <n> --version 2 --json && sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --json`


## Fórmulas CLI — Backends de Subprocesso LLM
- REMEMBER codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini remember --name <n> --type decision --description "desc" --body-file doc.md --force-merge --json`
- REMEMBER claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 remember --name <n> --type decision --description "desc" --body "conteudo" --force-merge --json`
- REMEMBER opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle remember --name <n> --type note --description "desc" --body-stdin --json`
- REMEMBER graph-stdin codex: pipe JSON `{body, entities, relationships}` para `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini remember --name <n> --type decision --description "desc" --graph-stdin --force-merge --json`
- RECALL codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini recall "query" --k 5 --no-graph --json`
- RECALL claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 recall "query" --k 5 --json`
- RECALL opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle recall "query" --k 5 --json`
- HYBRID-SEARCH codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini hybrid-search "query" --k 10 --with-graph --max-hops 2 --min-weight 0.3 --rrf-k 60 --json`
- HYBRID-SEARCH claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 hybrid-search "query" --k 10 --json`
- HYBRID-SEARCH opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle hybrid-search "query" --k 10 --with-graph --json`
- HYBRID-SEARCH fts-only: `sqlite-graphrag hybrid-search "query" --k 10 --fallback-fts-only --json`
- DEEP-RESEARCH claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 deep-research "pergunta" --k 20 --max-hops 3 --max-sub-queries 7 --max-results 50 --with-bodies --json`
- DEEP-RESEARCH codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini deep-research "pergunta" --k 20 --max-hops 3 --json`
- DEEP-RESEARCH opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle deep-research "pergunta" --k 20 --max-hops 3 --json`
- INGEST codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini ingest ./docs --mode codex --recursive --pattern "*.md" --type document --auto-describe --resume --json`
- INGEST claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 ingest ./docs --mode claude-code --recursive --pattern "*.md" --type document --resume --json`
- INGEST opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle ingest ./docs --mode opencode --recursive --pattern "*.md" --json`
- ENRICH re-embed codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation re-embed --limit 100 --resume --json`
- ENRICH memory-bindings claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --json`
- ENRICH opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation entity-descriptions --mode opencode --dry-run --json`
- EDIT claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 edit --name <n> --body-file novo.md --json`
- EDIT opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle edit --name <n> --body-file novo.md --json`
- RESTORE codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini restore --name <n> --version 2 --json`
- RESTORE claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 restore --name <n> --version 2 --json`
- RESTORE opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle restore --name <n> --version 2 --json`
- RENAME-ENTITY codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini rename-entity --from <antigo> --to <novo> --json`
- RENAME-ENTITY claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 rename-entity --from <antigo> --to <novo> --json`
- RENAME-ENTITY opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle rename-entity --from <antigo> --to <novo> --json`
- RELATED: `sqlite-graphrag related <nome> --hops 2 --relation uses --json`
- FALLBACK CHAIN: `sqlite-graphrag --llm-backend codex --llm-fallback codex,claude,opencode,none --skip-embedding-on-failure remember --name <n> --type note --description "desc" --body-file nota.md --json`


## Códigos de Saída e Estratégia de Retry
- EXIT 0: sucesso
- EXIT 1: erro de validação
- EXIT 2: parsing de argumento
- EXIT 3: conflito de lock otimista — recarregue e retente
- EXIT 4: não encontrado
- EXIT 5: erro de namespace
- EXIT 6: payload grande demais
- EXIT 9: duplicada — use `--force-merge`
- EXIT 10: erro de banco — execute `vacuum` + `health`
- EXIT 11: falha de embedding — verifique backend + OAuth
- EXIT 13: falha parcial de batch — reprocesse apenas falhados
- EXIT 14: erro de I/O
- EXIT 15: banco ocupado — amplie `--wait-lock`
- EXIT 16: falha preflight — corrija config MCP, NUNCA trate como transitório
- EXIT 19: SHUTDOWN — RETRY OBRIGATÓRIO, trabalho parcial descartado
- EXIT 20: erro interno
- EXIT 75: slots esgotados ou job singleton locked — respeite cooldown, NUNCA retente imediatamente
- EXIT 77: pressão de RAM — aguarde memória livre
- EXIT 78: erro de configuração — API key ausente, modelo ausente ou key inválida
- NUNCA ignore exit não-zero; NUNCA reprocesse batch inteiro após exit 13; NUNCA confunda exit 1 com exit 9


## Concorrência e Paralelismo
- RESPEITE teto rígido `2 x nCPUs` para comandos pesados: `init`, `remember`, `ingest`, `recall`, `hybrid-search`
- DEFINA `--llm-parallelism N` padrão 4 em `remember`/`edit`, padrão 2 em `ingest` (clamp [1, 32])
- USE `--llm-max-host-concurrency N` para cap de subprocessos LLM cross-process
- USE `--llm-slot-wait-secs N` para esperar slot ou `--llm-slot-no-wait` para abortar
- SAIBA que JOB SINGLETON: `enrich`, `ingest --mode claude-code|codex|opencode` adquirem singleton por namespace
- USE `--wait-job-singleton SECS` ou `--force-job-singleton` para quebrar lock stale
- ATIVE `SQLITE_GRAPHRAG_LOW_MEMORY=1` para paralelismo unitário (3-4x mais lento)
- NUNCA rode `enrich` em paralelo contra mesmo banco


## Manutenção e Diagnóstico
- EXECUTE `sqlite-graphrag init --namespace <ns>` no primeiro uso
- EXECUTE `health --json` para verificar `integrity_ok`, `schema_ok`, `schema_version >= 15`
- EXECUTE `migrate --dry-run --json` para preview; depois `migrate --json` após upgrade do binário
- EXECUTE `optimize --json` para refrescar estatísticas do planner
- INVOKE `backup --output <path> --json` para backup online
- INVOKE `sync-safe-copy --dest <path>` para snapshot atômico
- INVOKE `vacuum --json` após purge grande
- INVOKE `vec orphan-list --json` depois `vec purge-orphan --yes` para limpar vetores órfãos; `vec stats --json` para saúde
- INVOKE `stats --json` para estatísticas do banco (contagens, tamanhos, breakdown por namespace)
- INVOKE `export --namespace <ns> --type <kind> --json` para exportar como NDJSON
- INVOKE `debug-schema --json` para troubleshooting de drift de schema
- INVOKE `namespace-detect --json` para resolver precedência de namespace da invocação atual
- INVOKE `cache list --json` para listar arquivos de modelo em cache; `cache clear-models --yes` para forçar re-download
- INVOKE `completions bash|zsh|fish|elvish|powershell` para completions de shell
- INVOKE `codex-models --json` para inspecionar whitelist de modelos de embedding
- INVOKE `fts rebuild --json` quando `health.fts_degraded` for true; `fts check --json` para integridade; `fts stats --json` para contagens
- INVOKE `embedding status --json` para contagens de embedding; `embedding list --json` para inspeção por entrada
- INVOKE `pending list --filter-status queued --json`; `pending show <id>`; `pending cleanup --yes`
- INVOKE `pending-embeddings list --json`; `pending-embeddings process --json` para reprocessar
- INVOKE `slots status --json`; `slots release --slot-id <N> --yes` para órfãos
- AGENDE semanal: `purge` depois `cleanup-orphans` depois `prune-relations --relation mentions` depois `vacuum` depois `optimize` depois `sync-safe-copy`
- SAIBA que toda escrita executa `PRAGMA wal_checkpoint(TRUNCATE)` após commit
- SE corrupção: `sqlite3 broken.sqlite ".recover" | sqlite3 repaired.sqlite`


## Variáveis de Ambiente
- `SQLITE_GRAPHRAG_DB_PATH` — path persistente do banco
- `SQLITE_GRAPHRAG_NAMESPACE` — namespace persistente
- `SQLITE_GRAPHRAG_LLM_BACKEND` — backend persistente (codex|claude|opencode|none|auto)
- `SQLITE_GRAPHRAG_LLM_MODEL` — override persistente de modelo de subprocesso
- `SQLITE_GRAPHRAG_EMBEDDING_BACKEND` — backend persistente de embedding (auto|openrouter|llm)
- `SQLITE_GRAPHRAG_CODEX_BINARY` / `SQLITE_GRAPHRAG_CODEX_EMBED_MODEL` — binário e modelo de embed codex
- `SQLITE_GRAPHRAG_CLAUDE_BINARY` — override de path do binário claude
- `SQLITE_GRAPHRAG_OPENCODE_BINARY` / `SQLITE_GRAPHRAG_OPENCODE_MODEL` — overrides opencode
- `OPENROUTER_API_KEY` — chave de API do OpenRouter (tratada com zeroize-on-drop)
- `SQLITE_GRAPHRAG_EMBEDDING_DIM` — dimensão de embedding [8, 4096] (padrão 64 MRL)
- `SQLITE_GRAPHRAG_LOW_MEMORY` — habilitar paralelismo unitário
- `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR` — modo compliance
- `SQLITE_GRAPHRAG_DISPLAY_TZ` — timezone persistente
- `SQLITE_GRAPHRAG_LOG_FORMAT` — `json` para agregadores de log
- `SQLITE_GRAPHRAG_SKIP_PREFLIGHT` — bypass preflight (APENAS EMERGÊNCIAS)
- `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN` — APENAS para harnesses de teste CI


## Regras Ativas
- SEMPRE passe `--json` em toda invocação de `sqlite-graphrag`
- SEMPRE parsee `backend_invoked` para confirmar qual backend rodou
- SEMPRE execute `codex login` ou refresque OAuth do claude quando stale
- SEMPRE verifique exit code ANTES de parsear stdout
- NUNCA passe `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` — spawn ABORTA exit 1
- NUNCA rode `enrich` em paralelo contra mesmo banco
- NUNCA escreva `.sqlite` fora do binário
- NUNCA ignore exit 19 (RETRY OBRIGATÓRIO) ou exit 16 (corrija config MCP)
- NUNCA use daemon, `--bare`, `--gliner-variant` (REMOVIDOS)
- NUNCA chame `migrate --to-llm-only` sem guarda `--drop-vec-tables`
