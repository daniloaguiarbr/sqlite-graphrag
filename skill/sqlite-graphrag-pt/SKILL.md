---
name: sqlite-graphrag
description: Esta skill DEVE ativar para toda operação da CLI sqlite-graphrag cobrindo memória persistente, grafo de conhecimento GraphRAG, ligação de entidades, hybrid-search, recall, deep-research, remember, remember-batch, ingest, edit, restore, enrich, forget, purge, link, rename-entity e manutenção de grafo. Esta skill ensina a LLM a embedar via backend REST do OpenRouter com seleção explícita de modelo e preço, a rodar extração de entidades e enrichment como etapa SEPARADA através dos backends codex, claude-code, opencode ou openrouter com escolha explícita de modelo, a adicionar e verificar chaves de API OpenRouter, a honrar regras OAuth-only de subprocesso, isolamento preflight, fusão FTS5 mais cosine BLOB, relações canônicas, estratégia de retry por exit-code e isolamento de namespace. Esta skill ativa nas palavras-chave sqlite-graphrag GraphRAG memory embedding openrouter codex claude opencode remember recall hybrid-search ingest enrich deep-research forget purge link rename-entity
---


## Quando Esta Skill Ativa
- ATIVE quando o usuário pede para lembrar, salvar, recordar, recuperar, buscar ou persistir algo entre sessões
- ATIVE para contexto de longo prazo, grafo de conhecimento, GraphRAG, RAG, ligação de entidades, gestão de memória
- ATIVE quando sqlite-graphrag, embedding, FTS5, hybrid-search, OpenRouter ou memória LLM for mencionado
- NUNCA ATIVE para dados efêmeros pontuais, I/O simples de arquivo ou tarefas sem relação a contexto persistente


## Modelo Mental Central — Três Backends Independentes
- SAIBA que a CLI tem TRÊS seletores separados; NUNCA os confunda
- USE `--embedding-backend` para escolher COMO os vetores são produzidos: `openrouter` (REST, rápido) ou `llm` (subprocesso) ou `auto`
- USE `--llm-backend` para escolher QUAL subprocesso roda o embedding LLM quando `--embedding-backend llm`: `codex`, `claude`, `opencode`, `none`
- USE `--extraction-backend` (e `enrich --mode`) para escolher QUAL backend extrai entidades e relações: `codex`, `claude-code`, `opencode` (CLIs headless) ou `openrouter` (REST `/chat/completions`, sem CLI local)
- SEPARE escrita de enrichment SEMPRE: uma escrita produz embeddings; uma invocação SEPARADA de `enrich` extrai o grafo
- PASSE `--llm-backend none` em TODA escrita (`remember`, `remember-batch`, `ingest`, `edit`, `restore`) para PULAR o embedding inline de entidades e evitar o timeout do subprocesso codex
- RODE `enrich` somente DEPOIS que a escrita retornar exit 0, como comando DISTINTO, NUNCA encadeado na escrita
- PARSEIE `backend_invoked` em todo envelope de embedding para CONFIRMAR qual backend realmente rodou


## Regras de Instrução para LLMs
- QUANDO o usuário diz "lembre disso" → EXECUTE `remember --force-merge` com `--graph-stdin` carregando entidades curadas e relações canônicas, depois um `enrich` SEPARADO
- QUANDO o usuário pergunta "o que você sabe sobre X" → EXECUTE `hybrid-search "X" --k 10 --json` PRIMEIRO, depois EXPANDA os top resultados com `read --name <name> --json`
- QUANDO o usuário pergunta "como X se relaciona com Y" → EXECUTE `graph traverse --from X --depth 2 --json` ou `related X --hops 2 --json`
- QUANDO o usuário pede "pesquise profundamente sobre X" → EXECUTE `deep-research "X" --k 20 --max-hops 3 --json`
- ANTES de criar QUALQUER memória → EXECUTE `hybrid-search "<name>" --k 5 --json` para VERIFICAR duplicatas; se encontrar USE `--force-merge`
- APÓS criar ou atualizar memória → VERIFIQUE com `read --name <name> --json | jaq '{name, description, body_length}'`
- APÓS CADA turno com achados novos → AVALIE se deve persistir; se nada novo DECLARE "Nenhum achado novo para persistir"
- QUANDO o exit code for não-zero → LEIA o envelope de erro JSON do stdout via `jaq '{code, message, error_class}'`, REPORTE a remediação
- SEMPRE parseie a saída JSON com `jaq` (NUNCA `jq`)
- SEMPRE passe `--json` em toda invocação de `sqlite-graphrag`
- SEMPRE capture o stdout em uma variável PRIMEIRO, depois parseie; NUNCA pipe `sqlite-graphrag ... | jaq` direto porque NDJSON multilinha mascara falhas como nulls silenciosos
- SEMPRE use APENAS relações canônicas: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- SEMPRE mapeie relações não-canônicas ANTES de persistir: `adds|creates → causes`, `implements → supports`, `blocks → contradicts`, `tested-by → related`, `part-of → applies-to`
- SEMPRE normalize nomes de entidade para kebab-case ASCII lowercase ANTES de passar à CLI
- NUNCA use MCP Serena ou arquivos `.md` de memória para persistência; NUNCA escreva MEMORY.md
- NUNCA inicie ou referencie um daemon; NUNCA passe `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` aos backends de subprocesso
- PREFIRA `remember --force-merge` sobre `edit` para updates; PREFIRA `--graph-stdin` sobre extração inline de entidades
- LIMITE entidades a conceitos de domínio; REJEITE palavras genéricas, pronomes, UUIDs, timestamps


## Arquitetura e Princípios
- INVOQUE sempre como subprocesso; LEIA stdout para JSON/NDJSON; LEIA stderr para logs; CHEQUE o exit code ANTES de parsear
- SAIBA que o binário NÃO tem daemon, NÃO tem ONNX runtime, NÃO tem cache de modelo
- SAIBA que a similaridade cosine é pure Rust sobre `memory_embeddings`, `entity_embeddings`, `chunk_embeddings` backed por BLOB
- SAIBA que `init` ou `migrate` levam um banco fresco à versão de schema atual; LEIA o número vivo em `health --json` `schema_version`
- ENFORCE OAUTH-ONLY para os backends de subprocesso codex e claude: o spawn ABORTA exit 1 se `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estiver definida
- SAIBA que `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL` são PRESERVADAS para providers customizados
- SAIBA que o CWD do subprocesso é ISOLADO; diretórios órfãos são limpos automaticamente
- SAIBA que 7 guards preflight rodam ANTES de cada fork de subprocesso LLM; exit 16 é a falha preflight universal
- SAIBA que o subprocesso de extração headless herda o diretório de trabalho atual e qualquer `.mcp.json` presente, o que pode quebrar `claude -p`; ISOLE com um diretório de config vazio ao extrair via claude-code
- DEFINA `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` APENAS em emergências
- ISOLE NAMESPACE por projeto via `--namespace <ns>` ou env; padrão é `global`
- NUNCA exponha o binário como servidor MCP ou serviço HTTP
- NUNCA escreva o arquivo `.sqlite` em paralelo a partir de outra ferramenta


## Modelos de Embedding OpenRouter e Preços
- PASSE `--embedding-model <MODEL>` quando `--embedding-backend openrouter`; NÃO existe modelo padrão, então a omissão dispara exit 78
- SAIBA que os preços abaixo são por um milhão de tokens; ESCOLHA o modelo por custo e qualidade para a tarefa
- USE `nvidia/llama-nemotron-embed-vl-1b-v2:free` para embedding GRATUITO de custo zero (padrão RECOMENDADO)
- USE `qwen/qwen3-embedding-8b` em cerca de 0.01 USD (opção paga MAIS BARATA)
- USE `baai/bge-m3` em cerca de 0.01 USD
- USE `qwen/qwen3-embedding-4b` em cerca de 0.02 USD
- USE `openai/text-embedding-3-small` em cerca de 0.02 USD
- USE `perplexity/pplx-embed-v1-0.6b` em cerca de 0.04 USD
- USE `mistralai/mistral-embed-2312` em cerca de 0.10 USD
- USE `google/gemini-embedding-2` em cerca de 0.12 USD
- USE `openai/text-embedding-3-large` em cerca de 0.13 USD
- USE `google/gemini-embedding-001` em cerca de 0.15 USD
- MANTENHA `--embedding-dim 384` consistente entre escritas e leituras; uma dimensão divergente colide com o índice armazenado e falha o knn com exit 11
- SAIBA que o truncamento MRL é aplicado server-side ao `--embedding-dim` requisitado, então uma dimensão maior continua barata no path REST do OpenRouter
- SAIBA que NENHUM subcomando enumera modelos de embedding OpenRouter; a tabela de preços curada acima É o menu autoritativo
- VERIFIQUE a chave OpenRouter e a resolução da config com `sqlite-graphrag config doctor --json`; um modelo inválido falha rápido com exit 78
- SAIBA que `--embedding-backend openrouter` se propaga a TODOS os paths de embedding: `remember`, `remember-batch`, `ingest`, `recall`, `edit`, `restore`, `hybrid-search`, `deep-research`, `enrich`, `init`, `rename-entity`


## Gestão de Chave de API OpenRouter
- ADICIONE uma chave via stdin: `echo "sk-or-v1-..." | sqlite-graphrag config add-key --provider openrouter --from-stdin`
- LISTE chaves armazenadas: `sqlite-graphrag config list-keys --json`
- REMOVA uma chave por fingerprint: `sqlite-graphrag config remove-key <fingerprint> --json`
- RODE o doctor de diagnóstico: `sqlite-graphrag config doctor --json`
- INSPECIONE o caminho da config: `sqlite-graphrag config path`
- SAIBA que as chaves vivem na config XDG `~/.config/sqlite-graphrag/config.toml` com `chmod 600` e são zeroizadas no drop, JAMAIS logadas
- SAIBA a precedência: variável de ambiente `OPENROUTER_API_KEY` > config.toml > flag CLI `--openrouter-api-key`
- NUNCA passe a chave de API como argumento CLI em produção; PREFIRA stdin ou variável de ambiente para evitar exposição no histórico do shell


## Backends LLM Headless — Codex, Claude, OpenCode
- ESCOLHA codex com `--llm-backend codex --llm-model gpt-5.4-mini` para embedding e `--mode codex --codex-model gpt-5.4-mini` para extração; refresque OAuth com `codex login`
- ESCOLHA claude com `--llm-backend claude --llm-model claude-sonnet-4-6` para embedding e `--mode claude-code --claude-model claude-sonnet-4-6` para extração via o path OAuth zero-token
- ESCOLHA opencode com `--llm-backend opencode --llm-model opencode/big-pickle` para embedding e `--mode opencode --opencode-model opencode/big-pickle` para extração via seu próprio auth (NÃO OAuth)
- ESCOLHA openrouter SOMENTE para extração com `--mode openrouter --openrouter-model <model>` roteando o judge para o REST `/chat/completions` do OpenRouter; a chave vem de `OPENROUTER_API_KEY` e `--openrouter-model` é OBRIGATÓRIA (sem default; valor ausente sai com exit 1 antes de qualquer chamada de rede)
- SAIBA os modelos DEFAULT: codex `gpt-5.5`, claude `claude-sonnet-4-6`, opencode `opencode/big-pickle`
- SAIBA que o catálogo de modelos opencode é EXTERNO e dinâmico, com tiers gratuitos rotativos como Big Pickle, GPT-5 Nano, Nemotron Super e MiniMax Free; a CLI repassa `--opencode-model` SEM VALIDAR, então PASSE qualquer id atual do OpenCode Zen (o default verificado é `opencode/big-pickle`) e CONSULTE `opencode.ai/zen` para o catálogo vivo em vez de hardcodar ids voláteis
- SOBRESCREVA os paths dos binários com `--codex-binary`, `--claude-binary`, `--opencode-binary` quando a CLI não estiver no PATH
- AJUSTE os timeouts por backend no `ingest` com `--codex-timeout`, `--claude-timeout`, `--opencode-timeout` (segundos)
- VALIDE modelos codex com `--codex-model-validate` e auto-substitua com `--codex-model-fallback <MODEL>`
- LISTE os modelos OAuth codex com `sqlite-graphrag codex-models --json` para escolher `--codex-model` em `--mode codex`; isto lista modelos CODEX, NÃO modelos OpenRouter
- TROQUE de backend mid-job em rate limit com `--fallback-mode codex` no `enrich`, ou `--llm-fallback codex,claude,none` globalmente
- AVISE que a extração `claude-code` spawna `claude -p`, que herda o `.mcp.json` do CWD e pode falhar; PREFIRA extração codex ou isole o diretório de config
- SAIBA que `--mode openrouter` NÃO spawna nenhum subprocesso — faz uma chamada REST `/chat/completions`, logo NÃO precisa de claude, codex ou opencode CLI instalado
- PESE o trade-off: a extração `openrouter` cobra tokens na `OPENROUTER_API_KEY` (leia `usage.cost` da resposta), enquanto codex, claude-code e opencode não cobram tokens OpenRouter via seus paths OAuth ou de auth próprio zero-token


## Modelos de Texto OpenRouter para Enrich
- PASSE `--openrouter-model <MODEL>` desta tabela no `--mode openrouter`; os preços são entrada/saída em USD por um milhão de tokens
- SAIBA que estes modelos servem APENAS extração de entidades e enrichment, NUNCA embedding; a tabela de embedding acima é separada
- USE `openai/gpt-oss-120b` a 0.039/0.18 USD, contexto 131k, 36 tps (entrada MAIS BARATA, judge padrão RECOMENDADO)
- USE `openai/gpt-oss-120b:nitro` a 0.15/0.60 USD, contexto 131k, 300 tps (throughput MAIS RÁPIDO)
- USE `xiaomi/mimo-v2.5` a 0.10/0.28 USD, contexto 1M, 17 tps
- USE `deepseek/deepseek-v4-flash` a 0.09/0.18 USD, contexto 1M, 20 tps
- USE `deepseek/deepseek-v4-flash:nitro` a 0.14/0.28 USD, contexto 1M, 109 tps
- USE `minimax/minimax-m2.7` a 0.25/1.00 USD, contexto 205k, 43 tps
- USE `minimax/minimax-m3` a 0.30/1.20 USD, contexto 1M, 42 tps
- USE `minimax/minimax-m2.7:nitro` a 0.30/1.20 USD, contexto 205k, 146 tps
- USE `xiaomi/mimo-v2.5-pro` a 0.43/0.87 USD, contexto 1M, 29 tps
- USE `google/gemini-3.1-flash-lite` a 0.95/3.00 USD, contexto 1M, 100 tps
- USE `deepseek/deepseek-v4-pro` a 1.30/2.60 USD, contexto 1M, 26 tps
- USE `z-ai/glm-5.2` e `z-ai/glm-5.2:nitro` cujo preço varia por provider; CONFIRME o custo real via `usage.cost` na resposta
- SAIBA que variantes `:nitro` roteiam para o provider mais rápido a um preço maior
- VERIFIQUE que um modelo honra `json_schema` strict ANTES de produção; um modelo sem suporte a Structured Outputs falha com erro explícito do OpenRouter
- LEIA `usage.cost` da resposta do chat para contabilizar o custo real de tokens por item


## Referência de Flags Globais
- `--db <PATH>` — sobrescrever localização do banco; COLOQUE-A DEPOIS do subcomando (ex: `remember --db <PATH>`), porque o override canônico independente de posição é a variável de ambiente `SQLITE_GRAPHRAG_DB_PATH`
- `--namespace <ns>` — escopar operações para um namespace
- `--json` — saída JSON estruturada (SEMPRE passe)
- `--lang en|pt` — forçar idioma do stderr
- `--tz <TIMEZONE>` — localizar timestamps
- `--embedding-backend auto|openrouter|llm` — seletor de produção de vetor
- `--embedding-model <MODEL>` — modelo de embedding OpenRouter
- `--embedding-dim N` — dimensionalidade de embedding [8, 4096], padrão 384 MRL
- `--openrouter-api-key <KEY>` — chave de API OpenRouter
- `--llm-backend codex|claude|opencode|none|auto` — backend de embedding de subprocesso, cadeia separada por vírgula permitida
- `--llm-model <MODEL>` — modelo para o backend LLM ativo
- `--llm-fallback <chain>` — cadeia de fallback separada por vírgula quando o primário falha
- `--extraction-backend codex|claude-code|opencode|openrouter` — seletor de backend de extração de entidades (openrouter é REST, não subprocesso)
- `--openrouter-model <MODEL>` — modelo judge OBRIGATÓRIO para `--mode openrouter` (sem default; ausência sai com exit 1 antes de qualquer chamada de rede)
- `--openrouter-base-url <URL>` — override opcional do endpoint OpenRouter para o chat enrich
- `--openrouter-timeout <SECS>` — timeout da requisição do chat enrich, padrão 600
- `--llm-parallelism N` — largura do fan-out de embedding, padrão 4, clamp [1, 32]; governa TANTO o fan-out de subprocesso QUANTO o fan-out REST OpenRouter concorrente (JoinSet bounded), então `--llm-parallelism 8` rende concorrência efetiva 8 no path REST; entradas pequenas de um único batch permanecem seriais
- `--max-concurrency N` — cap de invocações pesadas concorrentes, clamp [1, 2×nCPUs]
- `--llm-max-host-concurrency N` — cap de slots de subprocesso LLM em todo o host
- `--llm-slot-wait-secs N` — esperar por um slot livre antes de abortar; `--llm-slot-no-wait` para falhar rápido
- `--wait-lock SECS` — ampliar a janela de aquisição de lock
- `--low-memory` — paralelismo unitário para containers restritos
- `--strict-env-clear` — preservar apenas PATH no subprocesso para compliance
- `--graceful-shutdown-secs N` — orçamento de cleanup antes do SIGKILL
- `--skip-embedding-on-failure` — armazenar sem vetor quando a cadeia termina em `none`
- `--codex-binary`, `--claude-binary`, `--opencode-binary` — sobrescrever paths dos binários
- `-v`/`-vv`/`-vvv` — logging info/debug/trace no stderr


## Operações CRUD de Escrita
- INVOQUE `remember --name <kebab> --type <kind> --description <text>` com `--body <text>` ou `--body-file <path>` ou `--body-stdin` ou `--graph-stdin`
- INVOQUE `remember --graph-stdin` para anexar `{body, entities, relationships}` em um único documento JSON
- INVOQUE `remember --graph-file <path>` para carregar o grafo de entidades de um arquivo; COMBINE com `--body-file <path>` para fornecer o corpo e o grafo de arquivos separados
- PASSE entities como `[{name, entity_type}]` em kebab-case ASCII; PASSE relationships como `[{source, target, relation, strength}]` onde strength está em [0.0, 1.0]
- PASSE `--strict-name` para REJEITAR um nome fora de kebab-case em vez de normalizá-lo automaticamente
- PASSE `--force-merge` para updates idempotentes e restauração de soft-deleted
- PASSE `--replace-graph` junto de `--force-merge` para ZERAR os vínculos de entidade/relacionamento existentes antes de escrever o novo grafo (substituição total, não merge)
- PASSE `--dry-run` para validar inputs sem persistir
- VALORES válidos de `--type`: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- INVOQUE `remember-batch` para 10 ou mais memórias via NDJSON stdin; PASSE `--transaction` para all-or-nothing
- INVOQUE `ingest <DIR> --recursive --pattern "*.md" --mode none` para importar um diretório como body-only, depois enriqueça SEPARADAMENTE
- SAIBA que `ingest --mode` aceita `none` (padrão body-only), `claude-code`, `codex`; opencode NÃO é um modo de ingest, então enriqueça com opencode em uma etapa SEPARADA
- USE `--resume` para continuar da fila após interrupção; `--retry-failed` apenas para itens falhados; `--auto-describe` para sintetizar descrições
- PASSE `--force-merge` no `ingest` para ATUALIZAR arquivos duplicados em vez de pulá-los; o ingest deduplica por `body_hash`, então um arquivo inalterado é pulado mesmo após renomear
- SAIBA que o `ingest` divide nativamente um corpo grande demais em múltiplos chunks, então um arquivo acima do limite por corpo é chunkado, NÃO rejeitado
- RESPEITE o limite de 512000 bytes e 512 chunks por corpo
- NUNCA misture `--body`, `--body-file`, `--body-stdin`, `--graph-stdin` em única invocação
- NUNCA use `fd | xargs remember`; INVOQUE `ingest` em vez disso
- NUNCA passe `--llm-backend codex` em qualquer escrita; o path de entidades forçaria o subprocesso codex e travaria no timeout dele; SEMPRE passe `--llm-backend none`


## CRUD Leitura Atualização Deleção
- INVOQUE `read --name <kebab> --json` para fetch O(1); PASSE `--with-graph` para incluir entidades vinculadas
- USE `read --name <n> --format raw` para imprimir o corpo puro SEM envelope JSON, ideal para pipar em outra ferramenta
- INVOQUE `list --type <kind> --limit N --offset N --json` para filtrar e paginar
- INVOQUE `history --name <n> --diff --json` para histórico de versões com estatísticas de diff de caracteres
- INVOQUE `edit --name <n> --body-file <path>` para atualizar o corpo, ou `--description <text>` e `--memory-type <kind>` para metadados
- USE `--force-reembed` para regenerar o embedding sem mudar o corpo
- USE `--expected-updated-at <ts>` para locking otimista; TRATE exit 3 como conflito, recarregue e retente
- INVOQUE `rename --name <old> --new-name <new>` para renomear uma memória preservando histórico
- INVOQUE `restore --name <n> --version <N>` para restaurar uma versão antiga
- INVOQUE `forget --name <n>` para um soft-delete reversível
- INVOQUE `purge --retention-days <N> --yes --dry-run` para preview, depois remova `--dry-run` para o hard delete
- INVOQUE `cleanup-orphans --yes` após bulk forget, depois `vacuum --json`
- NUNCA pule locking otimista em pipelines concorrentes; NUNCA delete manualmente via shell `sqlite3`


## Operações de Grafo de Entidades
- INVOQUE `link --from <a> --to <b> --relation <type> --create-missing --weight <float>` para criar uma aresta
- INVOQUE `unlink --from <a> --to <b> --relation <type>` para remover uma aresta, ou `--entity <name> --all` para dropar todas as arestas de uma entidade
- INVOQUE `unlink --memory <name> --entity <name>` para remover um único vínculo curado memória-entidade sem tocar nas arestas entidade-entidade
- INVOQUE `graph entities --json` para listar entidades via `.entities[]` (NÃO `.items[]`); ORDENE com `--sort-by name|degree|created-at` mais `--order asc|desc` (padrão `asc`; quando `--sort-by` é omitido o default é nome ascendente); USE `--order desc` para os mais-conectados-primeiro; PAGINE com `--limit N --offset N`
- INVOQUE `graph stats --json` para inspecionar `node_count`, `edge_count`, `avg_degree`, `max_degree`
- INVOQUE `graph traverse --from <root> --depth <N> --json` para travessia de subgrafo; EXPORTE com `--format json|dot|mermaid --output <path>`
- INVOQUE `rename-entity --name <old> --new-name <new>` para renomear uma entidade preservando arestas
- INVOQUE `delete-entity --name <n> --cascade` para deletar uma entidade e suas arestas
- INVOQUE `merge-entities --names "a,b,c" --into <target>` para mesclar duplicatas
- INVOQUE `reclassify --name <n> --new-type <kind>` para uma entidade, ou `--from-type <old> --to-type <new> --batch` para migração de tipo em massa
- INVOQUE `reclassify-relation --from-relation <old> --to-relation <new> --batch` para migração de tipo de relação em massa; FILTRE com `--filter-source-type` e `--filter-target-type`
- INVOQUE `prune-relations --relation mentions --dry-run` para preview de arestas de baixo valor, depois remova `--dry-run` com `--yes`
- INVOQUE `normalize-entities --yes` para normalizar todos os nomes para kebab-case ASCII
- INVOQUE `prune-ner --entity <n>` para remover bindings NER; `prune-ner --all --yes` para todo o namespace
- INVOQUE `memory-entities --name <memory>` para lookup forward, ou `--entity <name>` para lookup reverso
- SAIBA que a escrita no grafo é puramente ADITIVA: NÃO existe cap de grau, então hubs crescem sem limite e nenhuma escrita poda arestas; NORMALIZE somente via comandos de manutenção explícitos (`prune-relations`, `merge-entities`, `normalize-entities`), NUNCA durante uma escrita
- TIPOS canônicos de entidade: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- VALIDE nomes de entidade: mínimo 2 chars, sem newlines, sem ALL_CAPS curto de 4 chars ou menos
- NUNCA use `mentions` como relação padrão


## Operações de Busca GraphRAG
- USE o padrão canônico de três camadas: `hybrid-search` depois `read --name` depois `related|graph traverse`
- INVOQUE `recall <query> --k N` para KNN semântico puro; PASSE `--no-graph` para desabilitar expansão de grafo, `--precise` para scoring exato, `--max-distance <f>`, `--max-graph-results N`, `--all-namespaces`
- INVOQUE `hybrid-search <query> --k N` para fusão FTS5 mais KNN via RRF
- PASSE `--rrf-k 60` para fusão padrão; `--weight-vec 1.0 --weight-fts 1.0` para fusão balanceada
- PASSE `--fallback-fts-only` para pular embedding ao vivo e servir apenas FTS5 BM25 em modo offline
- USE `--with-graph --max-hops 2 --min-weight 0.3` para expansão de grafo; LEIA AMBOS `results[]` E `graph_matches[]`
- INVOQUE `related <name> --hops N --relation <type>` para travessia multi-hop a partir de uma memória
- INVOQUE `deep-research "<query>" --k 20 --max-hops 3 --max-sub-queries 7 --max-results 50 --with-bodies` para pesquisa paralela multi-hop
- AJUSTE deep-research com `--graph-decay <f>`, `--graph-min-score <f>`, `--max-neighbors-per-hop N`, `--max-cost-usd <f>`, `--timeout <secs>`
- PARSEIE `recall` retorna `results[].{name, snippet, distance, score, source}`
- PARSEIE `hybrid-search` retorna `results[].{name, combined_score, vec_rank, fts_rank}`
- PARSEIE `deep-research` retorna `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context`, `stats`
- NUNCA confunda `distance` com `combined_score` em ranking; NUNCA aumente `--hops` sem inspecionar `graph stats` antes


## Operações de Enrich
- INVOQUE `enrich --operation <op> --mode <backend>` onde AMBAS as flags são OBRIGATÓRIAS para qualquer operação LLM; omitir `--mode` é rejeitado com exit 2 — EXCETO os inspetores read-only `--status`, `--list-dead`, `--requeue-dead` e `--prune-dead-orphans`, que NÃO exigem `--operation` e `--mode`
- VALORES válidos de `--operation`: `memory-bindings`, `entity-descriptions`, `body-enrich`, `re-embed`, `augment-bindings`, `body-extract`
- VALORES válidos de `--mode`: `codex`, `claude-code`, `opencode`, `openrouter`
- USE `augment-bindings` para adicionar MAIS vínculos a memórias que JÁ estão vinculadas; EXIGE `--names <a,b,c>` ou `--names-file <path>` para escopar os alvos
- USE `body-extract --body-extract-graph-only` para extrair o grafo de um corpo de forma READ-ONLY, persistindo apenas entidades e relações sem reescrever o corpo
- PASSE `--codex-model`, `--claude-model`, `--opencode-model` ou `--openrouter-model` para escolher o modelo de extração compatível com o modo escolhido
- SAIBA que `--mode openrouter` exige `--openrouter-model` (sem default), lê a chave de `OPENROUTER_API_KEY`, faz uma chamada REST `/chat/completions` SEM CLI local, envia `response_format` json_schema strict com `provider.require_parameters:true`, e cobra tokens via `usage.cost`; os outros três modos são OAuth ou auth próprio zero-token
- PASSE `--limit N --resume` para `re-embed`; `--retry-failed` para reprocessar apenas itens falhados; `--dry-run` para preview
- PASSE `--min-output-chars N` para proteger o comprimento de saída do `body-enrich`; `--fallback-mode codex` para sobreviver a um rate limit do Claude
- NUNCA rode `enrich` em paralelo contra o mesmo banco; ele adquire um singleton por namespace
- PASSE `--until-empty` para loopar scan->drain INTERNAMENTE até a fila elegível esvaziar ou `--max-runtime` expirar, SUBSTITUINDO o loop bash externo de drain
- PASSE `--max-runtime <SECONDS>` para limitar o orçamento wall-clock de `--until-empty`; padrão 3600
- PASSE `--max-attempts <N>` para limitar os retries Transient antes de um item virar `dead`; padrão 8, range 1..=20
- PASSE `--status` para um relatório JSON read-only de `scan_backlog`, `unbound_backlog`, `queue_pending/done/failed/dead/skipped`, `eligible_now` e `waiting`; NÃO chama LLM e NÃO adquire singleton (e NÃO exige `--operation`/`--mode`)
- SAIBA que `scan_backlog` é o backlog REAL de banco por operação que um scan fresco enfileiraria (semântica de BANCO), DISTINTO de `unbound_backlog` (só memory-bindings) e de `queue_pending` (semântica de FILA sidecar); ele MATA o falso `pending=0` de `entity-descriptions`, `body-enrich` e `re-embed`, e o campo `state` deriva seu veredito `pending-scan` dele
- PASSE `--rest-concurrency <N>` para definir o fan-out REST de `--mode openrouter`; clamp 1..=16, padrão 8, DISTINTO de `--llm-parallelism`
- PASSE `--list-dead` para uma listagem JSON read-only de cada item terminal `dead` com seu `error_class`, `message` e os diagnósticos de truncamento `finish_reason`, `input_tokens` e `output_tokens` da resposta OpenRouter; `--requeue-dead` move esses itens de volta para `pending` para outra passada; `--ignore-backoff` desenfileira itens elegíveis de imediato, ignorando o cooldown `next_retry_at`
- PASSE `--prune-dead-orphans` para deletar APENAS as linhas da fila de enrich onde `status='dead'` e `item_type='memory'` cujo `item_key` (nome da memória) está AUSENTE do banco principal; linhas dead com chave de entidade são INTOCADAS; o banco principal é read-only — APENAS o sidecar `.enrich-queue.sqlite` é mutado; o JSON `DeadSummary` inclui o campo `pruned` com a contagem de linhas removidas; NÃO exige `--operation`/`--mode`/flags de LLM — é um inspetor SQLite puro sem aquisição de singleton; FÓRMULA: `sqlite-graphrag enrich --prune-dead-orphans --json`; USE ANTES de `--requeue-dead` para limpar linhas dead orphan de memória (memória renomeada ou purgada APÓS o enfileiramento, `error_class=permanent` 'not found') que o `--requeue-dead` sozinho apenas re-falharia
- SAIBA que a fila dead-letter TEM as colunas `error_class` e `next_retry_at` mais o status terminal `dead`: falhas Transient (rate-limit, timeout, 5xx, um retry-interno-esgotado e uma entidade ainda-não-materializada que uma passada posterior cria) reagendam com backoff exponencial limitado por `--max-attempts`, HardFailures (validação, parse) viram terminal de imediato, e o dequeue pula `dead` para o conjunto vivo encolher estritamente rumo à convergência
- SAIBA que uma completação OpenRouter truncada (`finish_reason` = `length`) NÃO é dead-lettered de imediato: o path de chat re-emite a requisição com um orçamento `max_tokens` MAIOR antes de qualquer reparo de JSON, então um item truncado por comprimento é retentado com mais espaço em vez de falhar identicamente
- SAIBA que a fila de enrich vive em um banco sidecar `.enrich-queue.sqlite` ao lado do `.sqlite` principal
- FÓRMULA STATUS: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --status --json` (sem chamada LLM, sem singleton)
- FÓRMULA UNTIL-EMPTY: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --until-empty --max-runtime 3600 --max-attempts 8 --rest-concurrency 8 --json`


## Escrita e Depois Enrich — Duas Etapas Separadas
- TRATE toda escrita como ETAPA 1 (embedar via OpenRouter, `--llm-backend none`) seguida de uma ETAPA 2 DISTINTA (`enrich`); NUNCA as encadeie com `&&`
- ESCOLHA o modelo OpenRouter da tabela de preços; ESCOLHA o backend e modelo de enrich independentemente
- REMEMBER etapa 1: `echo '{"body":"text","entities":[{"name":"jwt","entity_type":"concept"}],"relationships":[{"source":"jwt","target":"auth-svc","relation":"uses","strength":0.8}]}' | sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none remember --name <n> --type decision --description "desc" --graph-stdin --force-merge --json`
- REMEMBER etapa 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- REMEMBER etapa 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- REMEMBER etapa 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- REMEMBER etapa 2 openrouter: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --json` (chave de `OPENROUTER_API_KEY`)
- REMEMBER-BATCH etapa 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model qwen/qwen3-embedding-8b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none remember-batch --transaction --json`
- REMEMBER-BATCH etapa 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- REMEMBER-BATCH etapa 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- REMEMBER-BATCH etapa 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- REMEMBER-BATCH etapa 2 openrouter: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --json` (chave de `OPENROUTER_API_KEY`)
- INGEST etapa 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none ingest ./docs --mode none --recursive --pattern "*.md" --type document --resume --json`
- INGEST etapa 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- INGEST etapa 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- INGEST etapa 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- INGEST etapa 2 openrouter: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --json` (chave de `OPENROUTER_API_KEY`)
- EDIT etapa 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model perplexity/pplx-embed-v1-0.6b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none edit --name <n> --body-file new.md --json`
- EDIT etapa 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- EDIT etapa 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- EDIT etapa 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- EDIT etapa 2 openrouter: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --json` (chave de `OPENROUTER_API_KEY`)
- RESTORE etapa 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none restore --name <n> --version 2 --json`
- RESTORE etapa 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- RESTORE etapa 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- RESTORE etapa 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- RESTORE etapa 2 openrouter: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --json` (chave de `OPENROUTER_API_KEY`)


## Embedding e Enrich Paralelos via OpenRouter — Multiprocessamento
- ESCALE o embedding REST com `--llm-parallelism N`: ele divide os textos em chunks e os despacha em um JoinSet bounded de N requisições OpenRouter concorrentes, preservando a ordem de entrada por índice de chunk
- ESCALE o enrich REST com `--rest-concurrency N` mais `--until-empty`: N chamadas `/chat/completions` concorrentes drenam a fila enquanto a escrita SQLite permanece serial via WAL mais claim atômico, o gargalo real
- CLAMP `--llm-parallelism` em 1..32 e `--rest-concurrency` em 1..16; MANTENHA ambos na faixa segura Cloudflare 4..16 para modelos pagos; modelos `:free` têm limite de 20 req/min, então USE N baixo
- LEMBRE que várias chaves NÃO somam capacidade; o teto é a rede OpenRouter mais o singleton de namespace, NÃO o número de chaves
- REMEMBER paralelo etapa 1: `echo '{"body":"...","entities":[...],"relationships":[...]}' | sqlite-graphrag --embedding-backend openrouter --embedding-model qwen/qwen3-embedding-8b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-parallelism 8 --llm-backend none remember --name <n> --type decision --description "desc" --graph-stdin --force-merge --json`
- REMEMBER paralelo etapa 2: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --rest-concurrency 8 --until-empty --max-runtime 3600 --max-attempts 8 --json`
- REMEMBER-BATCH paralelo etapa 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model qwen/qwen3-embedding-8b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-parallelism 12 --llm-backend none remember-batch --transaction --json`
- REMEMBER-BATCH paralelo etapa 2: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model deepseek/deepseek-v4-flash:nitro --rest-concurrency 12 --until-empty --max-runtime 3600 --json`
- INGEST paralelo etapa 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-parallelism 6 --llm-backend none ingest ./docs --mode none --recursive --pattern "*.md" --type document --resume --json`
- INGEST paralelo etapa 2: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b:nitro --rest-concurrency 12 --until-empty --max-runtime 7200 --max-attempts 8 --json`
- EDIT paralelo etapa 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model qwen/qwen3-embedding-8b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-parallelism 8 --llm-backend none edit --name <n> --body-file new.md --json`
- EDIT paralelo etapa 2: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --rest-concurrency 8 --until-empty --json`
- RESTORE paralelo etapa 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model qwen/qwen3-embedding-8b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-parallelism 8 --llm-backend none restore --name <n> --version 2 --json`
- RESTORE paralelo etapa 2: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --rest-concurrency 8 --until-empty --json`
- MONITORE a convergência entre etapas com `enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --status --json`; a fila só convergiu de fato quando `scan_backlog` for 0 E `queue_pending` for 0 E `eligible_now` for 0, porque um `scan_backlog` não-zero com fila vazia significa que um scan AINDA NÃO enfileirou os candidatos de banco restantes
- INSPECIONE itens terminais com `--status`: `queue_dead` lista HardFailures que NUNCA serão reprocessados; trate-os como dívida de dados, não como erro transitório


## Fórmulas OpenRouter Somente-Leitura
- INIT: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY init --namespace <ns>`
- RECALL: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY recall "query" --k 10 --json`
- HYBRID-SEARCH: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY hybrid-search "query" --k 10 --with-graph --max-hops 2 --min-weight 0.3 --rrf-k 60 --json`
- DEEP-RESEARCH: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY deep-research "question" --k 20 --max-hops 3 --max-sub-queries 7 --max-results 50 --with-bodies --json`
- RENAME-ENTITY: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY rename-entity --name <old> --new-name <new> --json`
- ENRICH re-embed: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend codex --llm-model gpt-5.4-mini enrich --operation re-embed --limit 100 --resume --mode codex --codex-model gpt-5.4-mini --json`
- HYBRID-SEARCH offline: `sqlite-graphrag hybrid-search "query" --k 10 --fallback-fts-only --json`


## Diagnóstico e Manutenção
- INIT: `sqlite-graphrag init --namespace <ns>`; HEALTH: `sqlite-graphrag health --json | jaq '{integrity_ok, schema_version}'`
- MIGRATE: `sqlite-graphrag migrate --dry-run --json` para preview, depois `migrate --json` após um upgrade do binário
- OPTIMIZE: `sqlite-graphrag optimize --json` para refrescar estatísticas do planner; VACUUM: `sqlite-graphrag vacuum --json` após um purge grande
- FTS: `fts check --json` para integridade, `fts stats --json` para contagens, `fts rebuild --json` quando `health.fts_degraded` for true
- VEC: `vec orphan-list --json` depois `vec purge-orphan --yes`; `vec stats --json` para saúde de vetor
- EMBEDDING: `embedding --status --json` para contagens mais um objeto `coverage` reportando as contagens reais de vetor por tabela; `pending-embeddings --status --json` depois `pending-embeddings process --json` para reprocessar falhas
- SLOTS: `slots status --json` para inspecionar o semáforo do host; `slots release --slot-id <N> --yes` para órfãos
- PENDING: `pending list --filter-status queued --json`; `pending show <id>`; `pending cleanup --yes`
- EXPORT: `export --namespace <ns> --type <kind> --json` como NDJSON; STATS: `stats --json` para contagens e tamanhos, incluindo um `total_memories` no topo
- BACKUP: `backup --output backup.sqlite --json`; SNAPSHOT: `sync-safe-copy --dest <path>` sem adquirir lock
- INSPECT: `namespace-detect --json`, `debug-schema --json`, `cache list --json`, `cache clear-models --yes`
- COMPLETIONS: `completions bash|zsh|fish|elvish|powershell`
- AGENDE semanal: `purge` depois `cleanup-orphans` depois `prune-relations --relation mentions` depois `vacuum` depois `optimize` depois `sync-safe-copy`
- SE corrupção: `sqlite3 broken.sqlite ".recover" | sqlite3 repaired.sqlite`


## Códigos de Saída e Estratégia de Retry
- EXIT 0 sucesso; EXIT 1 erro de validação; EXIT 2 parsing de argumento (flag obrigatória ausente); EXIT 3 conflito de lock otimista, recarregue e retente
- EXIT 4 não encontrado; EXIT 5 erro de namespace; EXIT 6 payload grande demais; EXIT 9 duplicada, use `--force-merge`
- EXIT 10 erro de banco, execute `vacuum` mais `health`; EXIT 11 falha de embedding, verifique backend, dimensão e OAuth
- EXIT 13 falha parcial de batch, reprocesse apenas falhados; EXIT 14 erro de I/O; EXIT 15 banco ocupado (também o dequeue do enrich sob contenção de lock sustentada), amplie `--wait-lock`
- EXIT 16 falha preflight, corrija config MCP, NUNCA trate como transitório
- EXIT 19 SHUTDOWN, retry OBRIGATÓRIO, trabalho parcial descartado
- EXIT 20 erro interno; EXIT 75 slots esgotados ou singleton locked, respeite cooldown, NUNCA retente imediatamente
- EXIT 77 pressão de RAM, aguarde memória livre; EXIT 78 erro de config, chave ou modelo OpenRouter ausente
- NUNCA ignore um exit não-zero; NUNCA reprocesse um batch inteiro após exit 13; NUNCA confunda exit 1 com exit 9


## Concorrência
- RESPEITE o teto rígido `2 x nCPUs` para comandos pesados: `init`, `remember`, `ingest`, `recall`, `hybrid-search`
- DEFINA `--llm-parallelism N` padrão 4 em `remember` e `edit`, padrão 2 em `ingest`, clamp [1, 32]
- SAIBA do JOB SINGLETON: `enrich` e `ingest --mode codex|claude-code` adquirem um singleton por namespace
- USE `--wait-job-singleton SECS` ou `--force-job-singleton` para quebrar um lock stale
- HABILITE `SQLITE_GRAPHRAG_LOW_MEMORY=1` para paralelismo unitário, 3 a 4 vezes mais lento
- NUNCA rode `enrich` em paralelo contra o mesmo banco


## Variáveis de Ambiente
- `SQLITE_GRAPHRAG_DB_PATH` — override do path do banco
- `SQLITE_GRAPHRAG_NAMESPACE` — namespace persistente
- `SQLITE_GRAPHRAG_LLM_BACKEND` — backend LLM persistente
- `SQLITE_GRAPHRAG_LLM_MODEL` — modelo LLM persistente
- `SQLITE_GRAPHRAG_EMBEDDING_BACKEND` — backend de embedding persistente
- `SQLITE_GRAPHRAG_EMBEDDING_MODEL` — modelo de embedding OpenRouter persistente
- `SQLITE_GRAPHRAG_EMBEDDING_DIM` — dimensão de embedding [8, 4096], padrão 384
- `OPENROUTER_API_KEY` — chave de API OpenRouter, zeroizada no drop
- `SQLITE_GRAPHRAG_CODEX_BINARY`, `SQLITE_GRAPHRAG_CLAUDE_BINARY`, `SQLITE_GRAPHRAG_OPENCODE_BINARY` — overrides de path de binário
- `SQLITE_GRAPHRAG_OPENCODE_MODEL`, `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT` — overrides opencode
- `SQLITE_GRAPHRAG_LOW_MEMORY` — habilitar paralelismo unitário
- `SQLITE_GRAPHRAG_LOG_FORMAT` — `json` para agregadores de log
- `SQLITE_GRAPHRAG_SKIP_PREFLIGHT` — bypass preflight, APENAS EMERGÊNCIAS


## Regras Ativas
- SEMPRE passe `--json` em toda invocação
- SEMPRE passe `--embedding-backend openrouter --embedding-model <MODEL> --embedding-dim 384` em toda operação de embedding, com a chave via env ou `--openrouter-api-key`
- SEMPRE passe `--llm-backend none` nas escritas; SEMPRE rode `enrich` como etapa SEPARADA com `--mode` e o modelo correspondente
- SEMPRE parseie `backend_invoked` para confirmar qual backend rodou
- SEMPRE refresque OAuth com `codex login`, ou o OAuth do claude, quando stale
- NUNCA passe chaves de API aos backends de subprocesso codex ou claude, OAuth-only, exit 1
- NUNCA passe `--llm-backend codex` em `remember`, `remember-batch`, `ingest`, `edit`, `restore`
- NUNCA rode `enrich` em paralelo contra o mesmo banco; NUNCA escreva o `.sqlite` fora do binário
- NUNCA ignore exit 19 (retry obrigatório) ou exit 16 (corrija config MCP)
- NUNCA passe `--embedding-backend openrouter` sem `--embedding-model` e uma chave — exit 78 garantido
