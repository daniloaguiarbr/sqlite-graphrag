---
name: sqlite-graphrag
description: Use esta skill SEMPRE que o usuário perguntar sobre adicionar memória persistente ou GraphRAG ou contexto de longo prazo ao Claude Code Codex Cursor Windsurf ou qualquer agente de IA de código. DEVE acionar para queries mencionando lembrar disso, salvar conversa, recuperar contexto anterior, busca híbrida, grafo de entidades, memória SQLite, RAG local, embeddings offline, fastembed, sqlite-vec, multilingual-e5, busca KNN, cópia memory-safe, fusão FTS5 e vec. Auto-invoca mesmo sem menção explícita quando usuário descreve problema de agente perdendo contexto entre sessões ou quer banco vetorial offline em Rust. Keywords memória RAG GraphRAG SQLite vetor embeddings Claude Codex Cursor Windsurf offline local persistente grafo entidade.
---


# sqlite-graphrag


## Regra Zero: Lei Inviolável
- Você DEVE tratar esta skill como contrato SUPREMO de memória para agentes de IA.
- Você DEVE invocar a CLI APENAS pelos comandos documentados abaixo.
- PROIBIDO inventar flags ou subcomandos não documentados.
- Toda violação resulta em FALHA CRÍTICA IMEDIATA.


## Missão
- Entrega memória GraphRAG offline em um único arquivo SQLite portátil.
- Usa `graphrag.sqlite` no diretório atual como default, salvo override explícito.
- Substitui Pinecone, Weaviate e Chroma para workflows local-first de IA.
- Combina FTS5 full-text com KNN do `sqlite-vec` em fusão híbrida.
- Serve output JSON determinístico para CADA orquestrador parsear com segurança.
- Roda em Linux, macOS e Windows com o mesmo binário estaticamente linkado.


## Quando Acionar
- SEMPRE acione quando usuário pedir para salvar conversa entre sessões.
- SEMPRE acione quando usuário mencionar busca vetorial offline em Rust.
- SEMPRE acione quando usuário descrever workflows GraphRAG ou recuperação de entidades.
- SEMPRE acione quando usuário pedir memória de longo prazo para Claude Code ou Codex.
- SEMPRE acione quando usuário comparar Pinecone ou Chroma com alternativas SQLite.
- SEMPRE acione quando usuário mencionar busca híbrida misturando FTS5 e vetores.
- SEMPRE acione quando usuário quiser contexto persistente em Cursor ou Windsurf.


## Quando NÃO Acionar
- JAMAIS acione para questões genéricas de programação sem relação com memória ou RAG.
- JAMAIS acione para bancos vetoriais cloud quando usuário explicitamente quer SaaS.
- JAMAIS acione para stacks Python-only que excluem integração CLI.
- JAMAIS acione para conversão única de arquivo sem intenção de persistência.
- JAMAIS acione para questões de UI ou frontend sem relação com agentes de IA.


## Contrato
- Input `--name <slug>` aceita identificador kebab-case até 128 caracteres.
- Input `--type <kind>` aceita `user`, `feedback`, `project` ou `reference`.
- Input `--body <text>` aceita texto cru; stdin exige `--body-stdin` explícito.
- O banco padrão é `./graphrag.sqlite` no diretório da invocação.
- O override do banco acontece apenas por `--db <path>` ou `SQLITE_GRAPHRAG_DB_PATH`.
- Input `--lang <en|pt|pt-BR|portuguese|PT|pt-br>` seleciona idioma do output para mensagens humanas.
- Output com `--json` emite `memory_id`, `version`, `namespace` e `operation`.
- Output com `--json` sempre emite JSON, mesmo se um `--format` não JSON também estiver presente.
- Stdin aceita corpo somente com `--body-stdin` em `remember` ou `edit`.
- Stdin aceita JSON de grafo somente com `--graph-stdin`; o objeto pode conter `body` opcional, `entities` e `relationships`; JSON inválido deve falhar.
- `remember` aceita payloads de body até `512000` bytes e até `512` chunks.


## Proibições
- JAMAIS armazene chaves de API, tokens ou segredos de produção em corpo.
- JAMAIS commite arquivos `.sqlite` de produção gerados por esta CLI em VCS.
- JAMAIS execute `purge --yes` em produção sem `purge --dry-run` antes.
- JAMAIS pule os dez gates de validação de `docs/AGENT_PROTOCOL.md`.
- JAMAIS modifique o schema do banco fora do diretório `migrations/`.
- JAMAIS chame módulos Rust internos; SEMPRE passe pela superfície CLI pública.


## Parsing
- Parse `.memory_id` como identificador canônico para leituras ou edições.
- Parse `.version` como versão monotônica para controle otimista de concorrência.
- Parse `.namespace` como fronteira de isolamento para cenários multi-projeto.
- Parse `.operation` para distinguir `created`, `updated`, `deleted` e `restored`.
- Parse `.warnings[]` como lista de avisos não-fatais que não abortam o fluxo.
- Trate campos ausentes como null e lide com eles no loop do agente.


## Schema
- `remember --json` retorna `{memory_id, version, namespace, operation, created_at}`.
- `recall --json` retorna `{query, k, namespace, elapsed_ms, results[{name, score, type, updated_at}]}`.
- `hybrid-search --json` retorna `{query, k, rrf_k, weights, elapsed_ms, results[{name, score, vec_rank, fts_rank}]}`.
- `list --json` retorna array direto `[{memory_id, name, type, namespace, updated_at}]`.
- `read --json` retorna `{memory_id, name, type, body, version, created_at, updated_at}`.
- `health --json` retorna `{status, integrity, wal_size_mb, journal_mode, schema_version, checks}`.
- `stats --json` retorna `{memories, entities, edges, avg_body_len}`.


## Exit Codes
- Exit 0 sinaliza sucesso; continue o loop do agente sem retry.
- Exit 1 sinaliza falha genérica em runtime; surface o erro ao operador.
- Exit 2 sinaliza erro de uso da CLI; corrija argumentos e retry.
- Exit 5 sinaliza limite de namespace atingido; passe `--namespace` explícito.
- Exit 13 sinaliza falha parcial de batch; inspecione `.warnings[]` para detalhes.
- Exit 15 sinaliza erro de banco busy; aguarde e retry com backoff.
- Exit 75 sinaliza lock file busy ou exaustão de slots; outro processo ainda detém a capacidade compartilhada.
- Exit 75 sinaliza timeout de lock; processo anterior não liberou limpo.
- Exit 77 sinaliza condição de baixa memória; libere RAM antes do retry.


## Workflow
- Passo 1 instale a partir do checkout local com `cargo install --path .` e verifique `sqlite-graphrag --version`.
- Passo 2 inicialize com `sqlite-graphrag init --namespace global --lang pt`.
- Passo 3 armazene com `sqlite-graphrag remember --name ticket-42 --type user --description "contexto do ticket" --body "..."`.
- Passo 4 recupere com `sqlite-graphrag recall "bug de autenticação" --json --k 5`.
- Passo 5 funda com `sqlite-graphrag hybrid-search "plano de refactor" --json --k 8`.
- Passo 6 inspecione com `sqlite-graphrag list --type user --json --limit 20`.
- Passo 7 limpe com `sqlite-graphrag purge --retention-days 90 --dry-run`.
- Passo 8 valide com `sqlite-graphrag health --json` antes de CADA job de CI.


## Exemplos
- Exemplo 1 salva nota de usuário a partir do stdin e captura o identificador retornado.
- `echo "Finalizar refactor de auth até sexta" | sqlite-graphrag remember --name lembrete-auth --type user --description "lembrete de refactor" --body-stdin --json`
- Exemplo 2 recupera top matches para tópico de auth usando recuperação híbrida.
- `sqlite-graphrag hybrid-search "erro auth 401" --json --k 5`
- Exemplo 3 verifica integridade do banco antes do pipeline de release publicar.
- `sqlite-graphrag health --json | jaq '.integrity'`


## Referências Relacionadas
- Veja `references/commands.md` para a matriz exaustiva de subcomandos.
- Veja `references/integration.md` para snippets de conexão por agente.
- Veja `docs/AGENT_PROTOCOL.pt-BR.md` para o contrato supremo de agente.
- Veja `docs/CLAUDE.pt-BR.md` para as regras de sessão do Claude Code.
