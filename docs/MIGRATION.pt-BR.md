# Guia de Migração — neurographrag para sqlite-graphrag

> Leia este documento em [inglês (EN)](MIGRATION.md).  Volte para o [README.md](../README.md) principal para a referência completa de comandos.

- Este guia cobre o rename do legado `neurographrag` para `sqlite-graphrag v1.0.27`
- O projeto renomeado preserva o mesmo conjunto central de funcionalidades do legado `neurographrag v2.3.0`
- O crate e o repositório públicos já existem; use o checkout local apenas para validar mudanças não lançadas

## O Que Muda
- O nome do binário muda de `neurographrag` para `sqlite-graphrag`
- O nome do package Cargo muda de `neurographrag` para `sqlite-graphrag`
- O crate path Rust muda de `neurographrag` para `sqlite_graphrag`
- As variáveis de ambiente mudam de `NEUROGRAPHRAG_*` para `SQLITE_GRAPHRAG_*`
- O arquivo local padrão do banco passa a ser `./graphrag.sqlite` no diretório da invocação
- Os diretórios XDG padrão mudam de `neurographrag` para `sqlite-graphrag`
- O schema do banco continua compatível; o maior risco está em drift de paths, não em migração estrutural

## Migração Passo a Passo
### Passo 1 — Instalar o binário renomeado
```bash
cargo install --path .
```
- Instale a release publicada com `cargo install sqlite-graphrag --version 1.0.27 --locked`

### Passo 2 — Atualizar invocações de comando
```bash
sqlite-graphrag init
sqlite-graphrag health --json
sqlite-graphrag recall "migração postgres" --k 5 --json
```
- Substitua toda chamada `neurographrag ...` em scripts, jobs de CI e aliases locais

### Passo 3 — Atualizar variáveis de ambiente
| Antiga | Nova |
| --- | --- |
| `NEUROGRAPHRAG_DB_PATH` | `SQLITE_GRAPHRAG_DB_PATH` |
| `NEUROGRAPHRAG_CACHE_DIR` | `SQLITE_GRAPHRAG_CACHE_DIR` |
| `NEUROGRAPHRAG_NAMESPACE` | `SQLITE_GRAPHRAG_NAMESPACE` |
| `NEUROGRAPHRAG_LANG` | `SQLITE_GRAPHRAG_LANG` |
| `NEUROGRAPHRAG_LOG_LEVEL` | `SQLITE_GRAPHRAG_LOG_LEVEL` |
| `NEUROGRAPHRAG_LOG_FORMAT` | `SQLITE_GRAPHRAG_LOG_FORMAT` |
| `NEUROGRAPHRAG_DISPLAY_TZ` | `SQLITE_GRAPHRAG_DISPLAY_TZ` |

### Passo 4 — Decidir como tratar o path do banco
- Para continuar usando o banco legado, aponte `SQLITE_GRAPHRAG_DB_PATH` para o path absoluto antigo explicitamente
- Para começar limpo sob os defaults renomeados, não defina nada e deixe `sqlite-graphrag` criar `./graphrag.sqlite`
- O config local `.neurographrag/config.toml` deixa de fazer parte do fluxo padrão

### Passo 5 — Verificar a configuração migrada
```bash
sqlite-graphrag health --json
sqlite-graphrag stats --json
sqlite-graphrag namespace-detect
```
- Confirme que `schema_version`, namespace resolvido e caminho do banco batem com o esperado

## Mudanças de Schema JSON

### v1.0.44 — Rename no output de `graph entities`
- O campo de array JSON foi renomeado de `.items` para `.entities`
- Consumidores devem atualizar seus filtros: `.items[]` → `.entities[]`
- Exemplo: `sqlite-graphrag graph entities --json | jaq '.entities[].name'`

### v1.0.49 — Vocabulário extensível de relações
- O argumento `--relation` agora aceita qualquer string em kebab-case ou snake_case
- 12 relações canônicas permanecem como valores bem conhecidos
- Relações não canônicas emitem `tracing::warn!` no stderr mas são aceitas

### v1.0.50 — `prune-relations`, daemon auto-restart, schema v11
- Novo subcomando `prune-relations` para remoção em massa de relacionamentos por tipo: `sqlite-graphrag prune-relations --relation mentions --yes --json`
- Auto-restart do daemon em version mismatch: CLI detecta daemon desatualizado e reinicia antes do primeiro request de embedding (uma tentativa por processo)
- Migração V011 adiciona índice `idx_relationships_ns_relation` para filtragem por tipo de relação
- Versão do schema atualizada de 10 para 11
- `warn_if_non_canonical` agora emite warnings em `unlink` e `related` (antes apenas em `link`, `remember`, `ingest`)
- Funções `errors_msg::*` sempre retornam inglês; JSON stdout é contrato de API determinístico somente em inglês
- Exportação de grafo registra edges órfãs via `tracing::warn!` em vez de ignorá-las silenciosamente

### v1.0.68 — 2 correções CRÍTICAS: build Windows (G29) e proliferação de processos (G28)
- CORREÇÃO (G29) `cargo install sqlite-graphrag` no Windows agora compila.  v1.0.66 e v1.0.67 quebravam com `error[E0308]: mismatched types` em `src/terminal.rs:29` porque `HANDLE` em `windows-sys >= 0.59` é `*mut c_void` (era `isize` em 0.48/0.52).  Se você pulou v1.0.66 e v1.0.67 por causa da falha no Windows, esta é a primeira versão que compila no Windows desde v1.0.65.
- ADICIONADO (G28-B) `AppError::JobSingletonLocked { job_type, namespace }` (exit 75, classificado como retryable).  `enrich`, `ingest --mode claude-code` e `ingest --mode codex` agora adquirem um singleton por namespace antes de qualquer trabalho, então duas invocações concorrentes no mesmo banco falham rápido em vez de empilhar.  Atualize pipelines que antes rodavam múltiplos `enrich` em paralelo para usar a queue DB e `--resume`, ou sequencie-os.
- ADICIONADO (G28-A) env var `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` (opt-in).  Quando definida para um diretório existente e vazio, o subprocesso do Claude Code é iniciado com `CLAUDE_CONFIG_DIR=<esse dir>`, suprimindo servidores MCP do escopo user e a fan-out de 8-10 processos.  Defina esta var quando você tem muitos MCPs configurados mas quer uma árvore de subprocessos enxuta.
- ADICIONADO (G28-D) struct `retry::CircuitBreaker` com `AttemptOutcome::{Success, Transient, HardFailure}`.  Erros rate-limited e timeout são explicitamente excluídos da contagem de falhas.  Opt-in para loops de retry customizados; os paths de retry internos continuam usando suas `RetryConfig` ajustadas por domínio.
- ALTERADO (G28-D, não-breaking) `enrich` emite `tracing::warn!` quando `--llm-parallelism > 4`.  O padrão de 1 não mudou; usuários existentes rodando paralelismo > 1 veem o warning no stderr mas a operação completa normalmente.
- ALTERADO (G29) `windows-sys` fixado em `=0.59.0` exato em `Cargo.toml`.  Versões de patch futuras na linha 0.59.x exigirão bump manual; isso é intencional para prevenir regressão silenciosa no tipo `HANDLE`.
- ADICIONADO job de CI `windows-build-check` que roda `cargo check --target x86_64-pc-windows-msvc --lib --all-features` em todo push e PR.
- CORREÇÃO de 3 falhas de teste pré-existentes (`src/commands/{history,list,read}.rs`) que vazavam a env var `SQLITE_GRAPHRAG_DISPLAY_TZ` entre testes paralelos; os testes agora são timezone-agnostic.
- Sem migrações de banco em v1.0.68; `sqlite-graphrag migrate --json` é no-op.

### v1.0.67 — 2 NOVOS comandos, 24 correções de gaps, remember-batch, completions, migração V012
- NOVO comando `remember-batch` para criação em lote de memórias via NDJSON no stdin
- NOVO comando `completions` para geração de completions de shell (Bash, Zsh, Fish, PowerShell, Elvish)
- `read --id <N>` para busca direta por memory_id
- `read --with-graph` inclui entidades e relacionamentos vinculados
- `enrich --llm-parallelism <N>` para workers LLM paralelos
- `health` detecta entidades super-hub (grau > 50) e reporta avisos de normalização
- `edit` pula re-embedding quando conteúdo do body é inalterado (comparação body_hash)
- `rename` purga memórias ghost (soft-deleted) que ocupam o nome destino
- Validação de flags em `hybrid-search`, `recall`, `ingest` rejeita flags silenciosamente descartadas
- Migração V012 adiciona `created_at`/`updated_at` na tabela relationships
- Execute `sqlite-graphrag migrate --json` após upgrade para aplicar V012
- Schemas JSON adicionados: `remember-batch.schema.json`, `remember-batch-summary.schema.json`
- Schemas JSON atualizados: `health.schema.json` (campos super-hub), `rename.schema.json` (ghost_purged)

### v1.0.66 — 35 correções BUG/GAP, edit --type, graph_context, aliases LLM-friendly
- 3 correções CRÍTICAS: crash reclassify-relation, flooding de evidence chain, atualização de weight do link
- Flag `edit --type` para alterar tipo de memória sem recriar
- Campo `graph_context` na resposta JSON do `deep-research`
- `graph --format json` inclui alias `entities` junto com `nodes`
- `list --json` inclui alias `memories` junto com `items`
- `graph entities --json` inclui campo `description` por entidade
- `health --json` inclui contagens `vec_memories_missing` e `vec_memories_orphaned`
- Execute `sqlite-graphrag migrate --json` após upgrade
- Migração de dados recomendada: `reclassify-relation --from-relation applies-to --to-relation applies_to --batch --yes`

### v1.0.65 — 3 NOVOS comandos, correções deep-research, normalização de entidades, pipeline enrich

- NOVO comando `reclassify-relation` para reclassificação em massa ou individual de tipos de relacionamento com tratamento de colisões UNIQUE
- NOVO comando `normalize-entities` para normalizar nomes de entidade para kebab-case minúsculo e mesclar duplicatas automaticamente
- NOVO comando `enrich` para qualidade do grafo aumentada por LLM via `--mode claude-code` ou `--mode codex`; 3 operações: memory-bindings, entity-descriptions, body-enrich
- Correção CRITICAL: `deep-research` agora computa embedding separado por sub-query — decomposição era cosmética na v1.0.64
- Correção CRITICAL: `deep-research` funde KNN + FTS5 via RRF em vez de score fixo 0.5 para resultados FTS
- Correção HIGH: cadeias de evidência do `deep-research` agora são caminhos direcionados seed-para-target em vez de dumps globais
- Nomes de entidade normalizados para kebab-case em todo path de escrita (remember, ingest, link, rename-entity)
- `health` agora reporta concentração de relações: `top_relation`, `top_relation_ratio`, `applies_to_ratio`, `relation_concentration_warning`
- Novas flags do deep-research: `--rrf-k`, `--graph-decay`, `--graph-min-score`, `--max-neighbors-per-hop`
- Flag de warning `--max-entity-degree` em `link` e `remember`
- Novos schemas JSON: `deep-research`, `reclassify-relation`, `normalize-entities`, `enrich-phase`, `enrich-item-event`, `enrich-summary`
- Nenhuma migração de schema necessária; nenhuma breaking change nos contratos JSON existentes

### v1.0.64 — Comando deep-research, correção de hooks no ingest, detecção OAuth de custo, pré-validação de body cap, rejeição de rename mesmo nome
- NOVO subcomando `deep-research` para pesquisa profunda multi-hop paralela via decomposição de query e fan-out bounded
- `ingest --mode claude-code` desabilita hooks via `--settings '{"hooks":{}}'` para usuários OAuth — previne que hooks Stop consumam turns de extração
- `ingest --mode claude-code` detecta OAuth via `apiKeySource` e omite `cost_usd` enganoso do NDJSON — `--max-cost-usd` ignorado para assinantes
- `ingest --mode claude-code` e `--mode codex` validam tamanho do body ANTES de enviar ao LLM — arquivos excedendo 512 KB ignorados com warning
- `rename` e `rename-entity` rejeitam renomeações para o mesmo nome com exit 1
- Nenhuma migração de schema necessária; nenhuma breaking change nos contratos JSON existentes

### v1.0.63 — Preservação de nome no restore, normalização de relações no ingest, re-embed no edit

- `restore` preserva o nome atual da memória após rename — não reverte mais para o nome original da versão; elimina crash UNIQUE constraint (exit 10) quando nome antigo está ocupado
- `ingest --mode claude-code` e `--mode codex` normalizam strings de relação antes de inserir no DB (`depends-on` → `depends_on`) — elimina falsos avisos `non-canonical relation` e previne inconsistência de formato no DB
- `edit` regenera embedding vetorial quando body muda — `recall` e `hybrid-search` retornam scores precisos após edit
- Seção AUTHENTICATION adicionada ao `ingest --help` documentando princípio OAuth-first
- Detecção de falha de autenticação: `tracing::warn!` acionável quando autenticação do Claude Code ou Codex CLI falha
- Sem migração de schema necessária — compatível com bancos existentes

### v1.0.62 — Correção de embedding no claude-code, NOVO modo codex

- G01 CRITICAL fix: `ingest --mode claude-code` agora persiste embeddings vetoriais — `recall` encontra memórias ingeridas via claude-code
- NOVO `--mode codex` para extração via OpenAI Codex CLI — alternativa ao `--mode claude-code`
- Novas flags: `--codex-binary`, `--codex-model`, `--codex-timeout`
- Nova variável de ambiente: `SQLITE_GRAPHRAG_CODEX_BINARY`
- G02-G10: validação de versão, variáveis de ambiente no Windows, contador de skipped, cap de 10MB, normalização de nomes, warnings de entidade, WAL queue, WAL checkpoint, schema additionalProperties
- Sem migração de schema necessária — compatível com bancos existentes

> **Autenticação:** OAuth funciona automaticamente em ambos os modos — nenhuma chave de API necessária.
> `--mode claude-code` lê OAuth de `~/.claude/.credentials.json` (Claude Pro/Max/Team).
> `--mode codex` lê autenticação de dispositivo via `codex auth login` (OpenAI).
> Chaves de API (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`) são opcionais e aceleram o startup do subprocesso.

### v1.0.61 — 15 correções de bugs no ingest --mode claude-code

#### Correções críticas
- `--bare` substituído por `--dangerously-skip-permissions` — corrige falha de autenticação OAuth para usuários Pro/Max
- `--max-turns` aumentado de 1 para 3 — Claude precisa de >1 turno para extração estruturada
- campo source da memória alterado de `"claude-code"` para `"agent"` — corrige violação de CHECK constraint

#### Novas funcionalidades
- Flag `--claude-timeout <S>` (padrão 300s) — timeout por arquivo via crate `wait-timeout`
- `--resume` agora reseta arquivos travados em `processing`; `--retry-failed` reseta arquivos `failed`
- `--dry-run` agora funciona com `--mode claude-code` — pré-visualiza mapeamento sem spawnar Claude
- Re-ingestão do mesmo diretório atualiza memórias existentes em vez de falhar com UNIQUE constraint
- Falha de cold-start `--json-schema` automaticamente retentada uma vez
- `env_clear()` + injeção seletiva para hardening de segurança do subprocesso
- `--bare` condicional quando `ANTHROPIC_API_KEY` está definido (startup mais rápido para API key)

#### Sem migração necessária
- Sem alterações de schema; substituição direta da v1.0.60

### v1.0.60 — ingest --mode claude-code, correções CI, schema reverso

#### Nova feature: ingest --mode claude-code
- `sqlite-graphrag ingest ./docs --mode claude-code --recursive --json` usa Claude Code CLI local para extração curada por LLM de entidades/relações
- Spawna `claude -p` headless por arquivo com `--json-schema` para saída estruturada garantida
- Requer Claude Code >= 2.1.0 com assinatura Pro/Max ativa — zero API keys necessárias
- Resumível via `--resume`; controle de orçamento via `--max-cost-usd <N>`; rate limit com backoff exponencial
- Queue DB (`.ingest-queue.sqlite`) rastreia progresso por arquivo; eventos NDJSON incluem `entities`, `rels`, `cost_usd`
- Modos existentes `--mode none` (padrão) e `--mode gliner` continuam funcionando sem alteração

#### Novo schema: memory-entities-reverse.schema.json
- `memory-entities --entity <name> --json` reverse lookup agora tem JSON Schema dedicado
- Forward (`--name`) usa `memory-entities.schema.json`; reverse (`--entity`) usa `memory-entities-reverse.schema.json`
- Agentes validando respostas reverse contra schema forward devem atualizar

#### Correções de testes CI
- 8 falhas de testes corrigidas em exit codes, i18n, ingest fail-fast, contagem de migrations e exemplos bash Windows
- Sem mudanças de comportamento runtime — todas correções são apenas em código de teste

### v1.0.58 — Correção FTS5 force-merge (CRÍTICO), correção UNIQUE merge-entities, rename-entity, validação

#### CRÍTICO: Corrupção do índice FTS5 via remember --force-merge corrigida
- Cada `remember --force-merge` corrompia silenciosamente o FTS5 desde v1.0.56
- **Ação**: Execute `sqlite-graphrag fts rebuild` após atualizar

#### Correção UNIQUE do merge-entities para memory_entities
- Usa `UPDATE OR IGNORE` + cleanup (padrão de relationships do v1.0.57)

#### Novo comando: rename-entity
- `rename-entity --name <antigo> --new-name <novo>` renomeia entidade preservando relacionamentos

#### Novas funcionalidades
- `memory-entities --entity <nome>` busca reversa entidade→memórias
- `reclassify --description "texto"` atualiza descrição da entidade
- Validação de nomes de entidade (rejeita newlines, <2 chars, ALL_CAPS curto)
- Campo `action` na resposta do purge

### v1.0.57 — 16 correções: merge-entities UNIQUE, memory-entities coluna, WAL checkpoint, backup atômico

- `UPDATE OR IGNORE` em relationships do merge-entities
- Coluna `entity_type` no memory-entities
- WAL checkpoint em fts rebuild/check
- Backup atômico via tempfile-rename
- 18 novos testes

### v1.0.56 — Correção FTS5 sync, 7 novos comandos, envelope JSON de erro, degradação graciosa

- Sync FTS5 agora funciona em `edit`, `rename`, `restore` — memórias editadas antes ficavam invisíveis à busca textual
- `hybrid-search` degrada graciosamente quando FTS5 está corrompido: cai para apenas vetorial com `fts_degraded: true`
- TODOS os caminhos de erro emitem JSON no stdout: `{"error": true, "code": N, "message": "..."}`
- `--force-merge` com body vazio preserva body existente (mudança: use `--clear-body` para limpar explicitamente)
- `--type` e `--description` agora opcionais com `--force-merge` (herdados da memória existente)
- Limite padrão de `list --json` alterado de 50 para todas as memórias (output texto mantém 50)
- `unlink --relation` agora opcional (remove todos entre o par); `--entity X --all` para remoção em massa
- 7 novos comandos: `fts` (rebuild/check/stats), `backup`, `delete-entity`, `reclassify`, `merge-entities`, `memory-entities`, `prune-ner`
- `graph entities` adiciona campo `degree` e `--sort-by degree|name|created_at --order asc|desc`
- `health` adiciona `fts_query_ok` (teste funcional FTS5) e `sqlite_version`
- `optimize` agora reconstrói índice FTS5 (pule com `--skip-fts`)
- `ingest` auto-prefixa basenames numéricos com `doc-` e adiciona flag `--max-name-length`

### v1.0.55 — Correções de precisão de documentação para SKILL.md, CLAUDE.md e tabela de exit codes

#### Campo do summary de export corrigido de `total` para `exported`
- SKILL.md documentava o campo do summary de export como `total`; o campo real no JSON é `exported`
- Agentes que parseiam `.total` do summary de export devem migrar para `.exported`

#### Campos de resposta do list corrigidos
- SKILL.md documentava `total`, `limit`, `offset` como campos top-level na resposta do `list`
- A resposta real contém `items[]` (e alias `memories[]` desde v1.0.66), `total_count`, `truncated` e `elapsed_ms` no nível superior
- Agentes que parseiam `.total`, `.limit` ou `.offset` do list devem remover essas referências
- Desde v1.0.66: `memories[]` é alias domain-specific de `items[]` — ambos contêm dados idênticos

#### Exit code de timezone inválido corrigido de 1 para 2
- `--tz` com valor de timezone inválido retorna exit 2 (parsing de argumentos Clap), não exit 1 (validação da aplicação)
- Clap valida `chrono_tz::Tz` via `FromStr` antes do código da aplicação executar
- Exit code 2 agora explicitamente documentado nas tabelas de exit codes do SKILL.md e CLAUDE.md

#### Campos alias legados do stats documentados
- Resposta de `stats` inclui aliases legados não documentados: `db_bytes`, `edges`, `memories_total`, `entities_total`, `relationships_total`
- Agora documentados; prefira os nomes canônicos dos campos (`db_size_bytes`, `relationships`, etc.)

### v1.0.54 — WAL checkpoint para prune-relations, validação de body vazio, consistência memory_type

#### WAL checkpoint TRUNCATE adicionado ao prune-relations
- `prune-relations` era o último comando de escrita sem `PRAGMA wal_checkpoint(TRUNCATE)` após commit
- Todos os 12 comandos de escrita agora fazem checkpoint consistentemente; nenhuma ação necessária

#### Validação de body vazio com --graph-stdin
- `remember --graph-stdin` com body vazio e sem entidades agora retorna corretamente exit 1 (Validation) em vez de criar silenciosamente uma memória inerte com zero chunks
- Agentes que dependiam de `--graph-stdin` com body vazio criando uma memória devem fornecer body não-vazio ou pelo menos uma entidade

#### Campo memory_type adicionado ao JSON de list e export
- Saída JSON de `list` e `export` agora inclui `memory_type` junto com `type`, consistente com `read`
- Agentes que parseiam `.memory_type` de `list` ou `export` não recebem mais null
- Nenhuma ação necessária: o campo `type` existente permanece inalterado

#### Vec::with_capacity aplicado em 9 cold paths
- Melhoria de performance apenas; sem mudanças de API ou comportamento

### v1.0.53 — WAL checkpoint após escritas, export --json

#### WAL checkpoint TRUNCATE em cada comando de escrita
- Todos os comandos de escrita (remember, edit, forget, ingest, link, unlink, rename, restore, cleanup-orphans, purge) agora executam `PRAGMA wal_checkpoint(TRUNCATE)` após commit
- Isso garante que o arquivo do banco esteja sempre autocontido quando ferramentas externas (Dropbox, iCloud, OneDrive, rsync) o leem
- Nenhuma ação necessária: o checkpoint é automático e adiciona ~1-5ms por escrita
- Se o checkpoint falhar por contenção (SQLITE_BUSY após timeout de 5s), o comando falha com código de erro
- Exceção: `ingest` usa checkpoint best-effort (ignora falha) para não perder o resumo NDJSON após batch grande

#### export aceita flag --json
- `export --json` agora é aceito como flag oculta no-op para uniformidade de contrato
- Anteriormente retornava exit 2 do Clap; agora retorna exit 0 com o mesmo output NDJSON
- Nenhuma ação necessária a menos que você tratasse explicitamente exit 2 do `export --json`

### v1.0.52

#### Breaking: Exit code de Duplicate alterado de 2 para 9
- `AppError::Duplicate` agora retorna exit code 9 em vez de 2
- Exit code 2 passa a ser usado exclusivamente pelo Clap para erros de parsing de argumentos
- Agentes que roteiam no exit 2 para detectar duplicatas devem atualizar para exit 9
- Constante `DUPLICATE_EXIT_CODE` adicionada em `src/constants.rs`

#### Breaking: forget não mais emite JSON quando memória não é encontrada
- `forget` com um nome de memória inexistente agora retorna apenas erro no stderr + exit 4
- Anteriormente emitia JSON `{"action":"not_found",...}` no stdout E erro no stderr
- Alinha o comportamento com `read`, `edit`, `history`, `rename` em not-found
- Agentes que parseiam JSON no stdout para o caso not-found do forget devem migrar para roteamento por exit code

### v1.0.51

- `SQLITE_GRAPHRAG_NAMESPACE` agora é respeitado por todos os comandos. Se você dependia do comportamento anterior em que `list`, `read`, `edit`, `forget`, `history`, `rename`, `restore` e `remember` sempre usavam 'global' independentemente da variável de ambiente, passe explicitamente `--namespace global` para preservar o comportamento antigo.
- Nova flag `--max-rss-mb` para `remember` e `ingest` (padrão: 8192 MiB). Nenhuma ação necessária a menos que queira reduzir o threshold.

## Notas de Compatibilidade
- Não existe alias de compatibilidade para o nome antigo do binário nesta cópia do repositório
- Contratos JSON, exit codes e semântica operacional permanecem alinhados ao comportamento legado `v2.3.0`
- A release pública atual sob o novo nome é `sqlite-graphrag v1.0.27`

## Rollback
- Reinstale ou restaure o binário legado `neurographrag` se precisar reverter imediatamente
- Restaure as env vars antigas `NEUROGRAPHRAG_*` se necessário
- Se você alterou paths, reapointe o binário legado para o arquivo de banco anterior antes de retestar

## Veja Também
- `README.md` para o caminho atual de instalação e orientações de release
- `CHANGELOG.md` para a linhagem legada e as notas da release renomeada
- `docs/HOW_TO_USE.md` para exemplos atuais de comandos
