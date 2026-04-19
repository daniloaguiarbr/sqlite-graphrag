---
name: neurographrag
description: Use esta skill SEMPRE que o usuário perguntar sobre adicionar memória persistente ou GraphRAG ou contexto de longo prazo ao Claude Code Codex Cursor Windsurf ou qualquer agente de IA de código. DEVE acionar para queries mencionando lembrar disso, salvar conversa, recuperar contexto anterior, busca híbrida, grafo de entidades, memória SQLite, RAG local, embeddings offline, fastembed, sqlite-vec, multilingual-e5, busca KNN, cópia memory-safe, fusão FTS5 e vec. Auto-invoca mesmo sem menção explícita quando usuário descreve problema de agente perdendo contexto entre sessões ou quer banco vetorial offline em Rust. Keywords memória RAG GraphRAG SQLite vetor embeddings Claude Codex Cursor Windsurf offline local persistente grafo entidade.
---


# neurographrag


## Regra Zero — Lei Inviolável
- Você DEVE tratar esta skill como contrato SUPREMO de memória para agentes de IA.
- Você DEVE invocar a CLI APENAS pelos comandos documentados abaixo.
- PROIBIDO inventar flags ou subcomandos não documentados.
- Toda violação resulta em FALHA CRÍTICA IMEDIATA.


## Missão
- Entrega memória GraphRAG offline em um único arquivo SQLite portátil.
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
- Input `--body <text>` aceita texto cru ou lê stdin quando usa `-` como valor.
- Input `--lang <en|pt>` seleciona idioma do output para mensagens humanas.
- Output com `--json` emite `memory_id`, `version`, `namespace` e `operation`.
- Output sem `--json` emite blocos Markdown sob títulos localizados.
- Stdin aceita corpo quando usuário faz pipe de dados para `remember` ou `edit`.


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
- `recall --json` retorna `{query, results[{memory_id, score, snippet, version}]}`.
- `hybrid-search --json` retorna `{query, k, results[{memory_id, score, source}]}`.
- `list --json` retorna `{items[{memory_id, name, type, namespace, updated_at}]}`.
- `read --json` retorna `{memory_id, name, type, body, version, created_at, updated_at}`.
- `health --json` retorna `{status, integrity, schema_version, missing_entities}`.
- `stats --json` retorna `{memories_total, entities_total, chunks_total, db_bytes}`.


## Exit Codes
- Exit 0 sinaliza sucesso; continue o loop do agente sem retry.
- Exit 1 sinaliza falha genérica em runtime; surface o erro ao operador.
- Exit 2 sinaliza erro de uso da CLI; corrija argumentos e retry.
- Exit 5 sinaliza limite de namespace atingido; passe `--namespace` explícito.
- Exit 13 sinaliza falha parcial de batch; inspecione `.warnings[]` para detalhes.
- Exit 15 sinaliza erro de banco busy; aguarde e retry com backoff.
- Exit 73 sinaliza lock file busy; outro processo detém o lock de memória.
- Exit 75 sinaliza timeout de lock; processo anterior não liberou limpo.
- Exit 77 sinaliza condição de baixa memória; libere RAM antes do retry.


## Workflow
- Passo 1 instale com `cargo install neurographrag` e verifique `neurographrag --version`.
- Passo 2 inicialize com `neurographrag init --namespace default --lang pt`.
- Passo 3 armazene com `neurographrag remember --name ticket-42 --type user --body "..."`.
- Passo 4 recupere com `neurographrag recall "bug de autenticação" --json --k 5`.
- Passo 5 funda com `neurographrag hybrid-search "plano de refactor" --json --k 8`.
- Passo 6 inspecione com `neurographrag list --type user --json --limit 20`.
- Passo 7 limpe com `neurographrag purge --older-than 90d --dry-run`.
- Passo 8 valide com `neurographrag health --json` antes de CADA job de CI.


## Exemplos
- Exemplo 1 salva nota de usuário a partir do stdin e captura o identificador retornado.
- `echo "Finalizar refactor de auth até sexta" | neurographrag remember --name lembrete-auth --type user --json`
- Exemplo 2 recupera top matches para tópico de auth usando recuperação híbrida.
- `neurographrag hybrid-search "erro auth 401" --json --k 5`
- Exemplo 3 verifica integridade do banco antes do pipeline de release publicar.
- `neurographrag health --json | jaq '.integrity'`


## Referências Relacionadas
- Veja `references/commands.md` para a matriz exaustiva de subcomandos.
- Veja `references/integration.md` para snippets de conexão por agente.
- Veja `docs/AGENT_PROTOCOL.pt-BR.md` para o contrato supremo de agente.
- Veja `docs/CLAUDE.pt-BR.md` para as regras de sessão do Claude Code.
