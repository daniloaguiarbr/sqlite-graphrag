# neurographrag

[![Crates.io](https://img.shields.io/crates/v/neurographrag.svg)](https://crates.io/crates/neurographrag)
[![docs.rs](https://docs.rs/neurographrag/badge.svg)](https://docs.rs/neurographrag)
[![CI](https://github.com/daniloaguiarbr/neurographrag/actions/workflows/ci.yml/badge.svg)](https://github.com/daniloaguiarbr/neurographrag/actions)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Downloads](https://img.shields.io/crates/d/neurographrag.svg)](https://crates.io/crates/neurographrag)

> Local GraphRAG memory for LLMs in a single SQLite file — zero external services required

- Read the Portuguese version at [README.pt-BR.md](#português) in this same document


## What is it?
### neurographrag delivers — Durable Memory for AI Agents
- Stores memories, entities and relationships inside a single SQLite file
- Embeds content locally via `fastembed` with `multilingual-e5-small` model
- Combines FTS5 full-text search with `sqlite-vec` KNN into a hybrid ranker
- Extracts an entity graph with typed edges for multi-hop recall
- Preserves every edit through an immutable version history table
- Runs on Linux, macOS and Windows without any external service


## Why neurographrag?
### Differentiators — Against the Market
- Offline-first architecture eliminates OpenAI or Pinecone recurring fees
- Single-file storage replaces clusters of vector databases in Docker
- Graph-native retrieval outperforms pure vector RAG for multi-hop queries
- Native cross-platform binary ships without Python or Node dependencies
- Deterministic JSON output unlocks clean orchestration by LLM agents


## Quick Start
### Install — Three Commands
```bash
cargo install neurographrag
neurographrag init
neurographrag remember --name onboarding-note --type user --description "first memory" --body "hello graphrag"
neurographrag recall "graphrag" --k 5
```


## Superpowers for AI Agents
### Design — First-Class CLI Contract
- Every subcommand accepts `--json` for machine-readable output
- Every subcommand returns deterministic exit codes for orchestration
- Every subcommand reads structured arguments without hidden state
- Stdin accepts bodies or JSON payloads for entities and relationships
- Stdout emits one JSON document per invocation when `--json` is set
- Stderr carries human-readable traces under `NEUROGRAPHRAG_LOG_LEVEL=debug`
### Example — Remember Then Recall in JSON
```bash
neurographrag remember \
  --name integration-tests-postgres \
  --type feedback \
  --description "prefer real Postgres over SQLite mocks" \
  --body "Integration tests must hit a real database."

neurographrag recall "postgres integration tests" --k 3 --json | jaq '.hits[].name'
```


## Commands by Family
### Core — Database Lifecycle
| Command | Arguments | Description |
| --- | --- | --- |
| `init` | `--namespace <ns>` | Initialize database and download embedding model |
| `health` | `--json` | Show database integrity and pragma status |
| `stats` | `--json` | Count memories, entities and relationships |
| `migrate` | `--json` | Apply pending schema migrations via `refinery` |
| `vacuum` | `--json` | Checkpoint WAL and reclaim disk space |
| `optimize` | `--json` | Run `PRAGMA optimize` to refresh statistics |
| `sync-safe-copy` | `--output <path>` | Checkpoint then copy a sync-safe snapshot |
### Memory — Content Lifecycle
| Command | Arguments | Description |
| --- | --- | --- |
| `remember` | `--name`, `--type`, `--description`, `--body` | Save a memory with optional entity graph |
| `recall` | `<query>`, `--k`, `--type` | Search memories semantically via KNN |
| `read` | `--name <name>` | Fetch a memory by exact kebab-case name |
| `list` | `--type`, `--limit`, `--offset` | Paginate memories sorted by `updated_at` |
| `forget` | `--name <name>` | Soft-delete a memory preserving history |
| `rename` | `--old <name>`, `--new <name>` | Rename a memory while keeping versions |
| `edit` | `--name`, `--body`, `--description` | Edit body or description creating new version |
| `history` | `--name <name>` | List all versions of a memory |
| `restore` | `--name`, `--version` | Restore a memory to a previous version |
| `purge` | `--days <n>`, `--yes` | Permanently delete soft-deleted memories |
### Search — Hybrid Retrieval
| Command | Arguments | Description |
| --- | --- | --- |
| `recall` | `<query>`, `--k`, `--type` | Vector-only KNN over `vec_memories` |
| `hybrid-search` | `<query>`, `--k`, `--rrf-k` | FTS5 + vector fused via Reciprocal Rank Fusion |
### Namespace — Scope Resolution
| Command | Arguments | Description |
| --- | --- | --- |
| `namespace-detect` | `--cwd <path>` | Resolve namespace precedence for invocation |
### Graph — Entities and Relationships (since v1.0)
| Command | Arguments | Description |
| --- | --- | --- |
| `link` | `--source`, `--target`, `--relation` | Create or update a typed relationship |
| `unlink` | `--source`, `--target`, `--relation` | Remove a typed relationship |
| `related` | `<name>`, `--hops <n>` | Traverse graph N hops from a memory |
| `graph` | `--json` | Dump the full entity graph snapshot |
| `cleanup-orphans` | `--yes` | Remove entities with zero memory references |
### Operations — Bulk Workflows (since v1.0)
| Command | Arguments | Description |
| --- | --- | --- |
| `export` | `--output <path>` | Export memories and graph to a portable archive |
| `import` | `--input <path>` | Import memories preserving versions and namespaces |
| `reindex` | `--all`, `--chunks` | Rebuild FTS5 and vector indexes |
| `models` | `--list`, `--download <name>` | Manage local embedding models |
### Daemon — Background Mode (since v1.0)
| Command | Arguments | Description |
| --- | --- | --- |
| `daemon` | `--start`, `--stop`, `--status` | Run a long-lived process over a Unix socket |


## Environment Variables
### Configuration — Runtime Overrides
| Variable | Description | Example |
| --- | --- | --- |
| `NEUROGRAPHRAG_DB_PATH` | Absolute path to the SQLite database file | `/data/graph.sqlite` |
| `NEUROGRAPHRAG_HOME` | Root directory for data and cache | `~/.neurographrag` |
| `NEUROGRAPHRAG_LANG` | CLI output language as `en` or `pt` | `pt` |
| `NEUROGRAPHRAG_LOG_LEVEL` | Tracing filter as `error`, `warn`, `info`, `debug`, `trace` | `debug` |
| `NEUROGRAPHRAG_NAMESPACE` | Namespace override bypassing detection | `project-foo` |


## Integration Patterns
### Pipelines — Compose With Unix Tools
```bash
# Extract memory names from a recall into an LLM prompt
neurographrag recall "auth tests" --k 5 --json | jaq -r '.hits[].name'

# Feed the hybrid search output to a summarizer agent
neurographrag hybrid-search "postgres migration" --k 10 --json \
  | jaq -c '.hits[] | {name, score}' \
  | xh POST http://localhost:8080/summarize

# Backup with atomic snapshot and compression
neurographrag sync-safe-copy --output /tmp/ng.sqlite
ouch compress /tmp/ng.sqlite /tmp/ng-$(date +%Y%m%d).tar.zst
```


## Exit Codes
### Contract — Deterministic Status for Orchestration
| Code | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Validation error or runtime failure |
| `2` | Duplicate detected or invalid CLI argument |
| `3` | Conflict during optimistic update |
| `4` | Memory or entity not found |
| `5` | Namespace could not be resolved |
| `6` | Payload exceeded configured limits |
| `10` | SQLite database error |
| `11` | Embedding generation failed |
| `12` | `sqlite-vec` extension failed to load |
| `13` | Database busy after retries |
| `14` | Filesystem I/O error |
| `20` | Internal or JSON serialization error |
| `75` | `EX_TEMPFAIL` — all concurrency slots busy (`--max-concurrency` slots exhausted) |
| `77` | Available RAM below minimum required to load the embedding model |


## Safe Parallel Invocation
### Counting Semaphore — Up to 4 Simultaneous Instances
- Each invocation loads `multilingual-e5-small` consuming approximately 750 MB of RAM
- Up to 4 instances can run simultaneously by default (controlled by `MAX_CONCURRENT_CLI_INSTANCES`)
- Lock files live at `~/.cache/neurographrag/cli-slot-{1..4}.lock` using exclusive `flock` per slot
- A fifth concurrent invocation waits up to 300 seconds (`CLI_LOCK_DEFAULT_WAIT_SECS`) by default
- Use `--max-concurrency N` to override the slot limit for the current invocation
- Use `--wait-lock SECONDS` to override the wait timeout
- Memory guard aborts with exit `77` when less than 2 GB RAM is available before loading the model
- SIGINT and SIGTERM trigger a graceful shutdown signal visible via `neurographrag::shutdown_requested()`
- Example with GNU parallel respecting the slot limit:

```bash
parallel -j 4 neurographrag remember --name {1} --body {2} ::: names.txt ::: bodies.txt
```


## Integration Contract
### LLM Orchestrators — Safe Concurrency Rules
- The binary supports up to `MAX_CONCURRENT_CLI_INSTANCES` (default 4) simultaneous invocations
- Orchestrators using `xargs` SHOULD pass `-P 4` to match the default slot count
- Exit code `75` means "all slots busy — try again later" and is NOT an application error
- Exit code `77` means "insufficient RAM — free memory before retrying" and requires human action
- Use `--wait-lock N` to let the binary itself retry instead of the orchestrator retrying from outside
- Background orchestration at scale MUST use daemon mode once available in BLOCO 3


## Troubleshooting
### FAQ — Common Issues
- Database locked after crash requires `neurographrag vacuum` to checkpoint WAL cleanly
- First `init` may take one minute while `fastembed` downloads the quantized model
- Permission denied on Linux means the cache directory lacks write access for your user
- Namespace detection falls back to `default` when no `.neurographrag` marker is present
- macOS case-insensitive volumes can clash with kebab-case names differing only in case


## Contributing
### Community — Open to Pull Requests
- Issues and pull requests are welcome at the GitHub repository
- A detailed `CONTRIBUTING.md` will ship alongside the 1.0.0 release
- Security reports follow the policy described in `SECURITY.md` when published


## License
### Dual — MIT OR Apache-2.0
- Licensed under either of Apache License 2.0 or MIT License at your option
- See `LICENSE-APACHE` and `LICENSE-MIT` in the repository root for full text


## Changelog
### History — Tracked Separately
- Read the full release history at [CHANGELOG.md](CHANGELOG.md)




# Português
## O que é?
### neurographrag entrega — Memória Durável para Agentes de IA
- Armazena memórias, entidades e relacionamentos em um arquivo SQLite único
- Gera embeddings localmente via `fastembed` com modelo `multilingual-e5-small`
- Combina busca textual FTS5 com KNN do `sqlite-vec` em ranqueador híbrido
- Extrai grafo de entidades com arestas tipadas para recuperação multi-hop
- Preserva cada edição em tabela imutável de versões históricas
- Executa em Linux, macOS e Windows sem qualquer serviço externo


## Por que neurographrag?
### Diferenciais — Contra o Mercado
- Arquitetura offline-first elimina custos recorrentes de OpenAI ou Pinecone
- Armazenamento em arquivo único substitui clusters de bancos vetoriais em Docker
- Recuperação com grafo supera RAG vetorial puro em consultas multi-hop
- Binário cross-platform nativo dispensa dependências Python ou Node
- Saída JSON determinística habilita orquestração limpa por agentes de IA


## Início Rápido
### Instalação — Três Comandos
```bash
cargo install neurographrag
neurographrag init
neurographrag remember --name primeira-memoria --type user --description "primeira memória" --body "olá graphrag"
neurographrag recall "graphrag" --k 5
```


## Superpoderes para Agentes de IA
### Design — Contrato de CLI de Primeira Classe
- Todo subcomando aceita `--json` para saída legível por máquina
- Todo subcomando retorna códigos de saída determinísticos para orquestração
- Todo subcomando lê argumentos estruturados sem estado oculto
- Stdin aceita corpos ou payloads JSON para entidades e relacionamentos
- Stdout emite um documento JSON por invocação quando `--json` é definido
- Stderr carrega traces legíveis sob `NEUROGRAPHRAG_LOG_LEVEL=debug`
### Exemplo — Remember e Recall em JSON
```bash
neurographrag remember \
  --name testes-integracao-postgres \
  --type feedback \
  --description "prefira Postgres real a mocks SQLite" \
  --body "Testes de integração devem usar banco real."

neurographrag recall "testes integração postgres" --k 3 --json | jaq '.hits[].name'
```


## Comandos por Família
### Núcleo — Ciclo de Vida do Banco
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `init` | `--namespace <ns>` | Inicializa banco e baixa modelo de embedding |
| `health` | `--json` | Exibe integridade e status dos pragmas |
| `stats` | `--json` | Conta memórias, entidades e relacionamentos |
| `migrate` | `--json` | Aplica migrações pendentes via `refinery` |
| `vacuum` | `--json` | Faz checkpoint do WAL e libera espaço |
| `optimize` | `--json` | Executa `PRAGMA optimize` para atualizar estatísticas |
| `sync-safe-copy` | `--output <caminho>` | Gera cópia segura para sincronização em nuvem |
### Memória — Ciclo de Vida do Conteúdo
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `remember` | `--name`, `--type`, `--description`, `--body` | Salva memória com grafo de entidades opcional |
| `recall` | `<query>`, `--k`, `--type` | Busca memórias semanticamente via KNN |
| `read` | `--name <nome>` | Recupera memória por nome kebab-case exato |
| `list` | `--type`, `--limit`, `--offset` | Pagina memórias ordenadas por `updated_at` |
| `forget` | `--name <nome>` | Remove memória logicamente preservando histórico |
| `rename` | `--old <nome>`, `--new <nome>` | Renomeia memória mantendo versões |
| `edit` | `--name`, `--body`, `--description` | Edita corpo ou descrição gerando nova versão |
| `history` | `--name <nome>` | Lista todas as versões da memória |
| `restore` | `--name`, `--version` | Restaura memória para versão anterior |
| `purge` | `--days <n>`, `--yes` | Apaga permanentemente memórias soft-deleted |
### Busca — Recuperação Híbrida
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `recall` | `<query>`, `--k`, `--type` | KNN puro sobre `vec_memories` |
| `hybrid-search` | `<query>`, `--k`, `--rrf-k` | FTS5 combinado com vetor via Reciprocal Rank Fusion |
### Namespace — Resolução de Escopo
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `namespace-detect` | `--cwd <caminho>` | Resolve precedência de namespace para invocação |
### Grafo — Entidades e Relacionamentos (a partir da v1.0)
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `link` | `--source`, `--target`, `--relation` | Cria ou atualiza relacionamento tipado |
| `unlink` | `--source`, `--target`, `--relation` | Remove relacionamento tipado |
| `related` | `<nome>`, `--hops <n>` | Percorre grafo N hops a partir de memória |
| `graph` | `--json` | Despeja snapshot completo do grafo de entidades |
| `cleanup-orphans` | `--yes` | Remove entidades sem referências de memória |
### Operações — Fluxos em Lote (a partir da v1.0)
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `export` | `--output <caminho>` | Exporta memórias e grafo para arquivo portável |
| `import` | `--input <caminho>` | Importa memórias preservando versões e namespaces |
| `reindex` | `--all`, `--chunks` | Reconstrói índices FTS5 e vetorial |
| `models` | `--list`, `--download <nome>` | Gerencia modelos de embedding locais |
### Daemon — Modo Background (a partir da v1.0)
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `daemon` | `--start`, `--stop`, `--status` | Executa processo longo sobre socket Unix |


## Variáveis de Ambiente
### Configuração — Overrides em Runtime
| Variável | Descrição | Exemplo |
| --- | --- | --- |
| `NEUROGRAPHRAG_DB_PATH` | Caminho absoluto para o arquivo SQLite | `/data/graph.sqlite` |
| `NEUROGRAPHRAG_HOME` | Diretório raiz para dados e cache | `~/.neurographrag` |
| `NEUROGRAPHRAG_LANG` | Idioma da saída da CLI como `en` ou `pt` | `pt` |
| `NEUROGRAPHRAG_LOG_LEVEL` | Filtro de tracing `error`, `warn`, `info`, `debug`, `trace` | `debug` |
| `NEUROGRAPHRAG_NAMESPACE` | Override de namespace ignorando detecção | `projeto-foo` |


## Padrões de Integração
### Pipelines — Compondo com Ferramentas Unix
```bash
# Extrai nomes de memórias de um recall para prompt de LLM
neurographrag recall "testes auth" --k 5 --json | jaq -r '.hits[].name'

# Envia saída do hybrid-search para agente sumarizador
neurographrag hybrid-search "migração postgres" --k 10 --json \
  | jaq -c '.hits[] | {name, score}' \
  | xh POST http://localhost:8080/summarize

# Backup com snapshot atômico e compressão
neurographrag sync-safe-copy --output /tmp/ng.sqlite
ouch compress /tmp/ng.sqlite /tmp/ng-$(date +%Y%m%d).tar.zst
```


## Códigos de Saída
### Contrato — Status Determinístico para Orquestração
| Código | Significado |
| --- | --- |
| `0` | Sucesso |
| `1` | Erro de validação ou falha em runtime |
| `2` | Duplicata detectada |
| `3` | Conflito durante atualização otimista |
| `4` | Memória ou entidade não encontrada |
| `5` | Namespace não pôde ser resolvido |
| `6` | Payload excedeu limites configurados |
| `10` | Erro do banco SQLite |
| `11` | Geração de embedding falhou |
| `12` | Extensão `sqlite-vec` falhou ao carregar |
| `13` | Banco ocupado após tentativas |
| `14` | Erro de I/O do sistema de arquivos |
| `20` | Erro interno ou de serialização JSON |
| `75` | `EX_TEMPFAIL` — todos os slots de concorrência ocupados; tente novamente |
| `77` | RAM disponível abaixo do mínimo requerido; libere memória antes de prosseguir |


## Invocação Paralela Segura
### Semáforo de Contagem — Até 4 Instâncias Simultâneas
- Cada invocação carrega `multilingual-e5-small` consumindo aproximadamente 750 MB de RAM
- Até 4 instâncias podem executar simultaneamente por padrão (controlado por `MAX_CONCURRENT_CLI_INSTANCES`)
- O semáforo usa arquivos `cli-slot-{1..4}.lock` em `~/.cache/neurographrag/` com `flock` exclusivo
- Uma 5ª invocação aguarda até `--wait-lock` segundos (padrão: 300 s) antes de encerrar com código `75`
- Use `--max-concurrency N` para reduzir o limite quando a RAM disponível for restrita
- Use `--wait-lock SECONDS` para ajustar o timeout de espera por slot livre
- A guarda de memória aborta com código `77` se a RAM livre ficar abaixo de 2 GB
- Use `--skip-memory-guard` EXCLUSIVAMENTE em testes automatizados onde a alocação real não ocorre
- Sinais `SIGINT`, `SIGTERM` e `SIGHUP` são capturados e sinalizam shutdown graceful via `SHUTDOWN`


## Contrato de Integração
### Orquestradores de LLM — Regras de Concorrência Segura
- O binário suporta até 4 invocações paralelas simultâneas por padrão
- Orquestradores usando `xargs` PODEM passar `-P 4` com segurança para paralelismo máximo
- Fan-out acima de 4 recebe código `75` e DEVE fazer retry com backoff exponencial
- Código de saída `75` significa "todos os slots ocupados; tente novamente" e NÃO é erro permanente
- Código de saída `77` significa "RAM insuficiente; aguarde outras cargas encerrarem"
- Use `--wait-lock N` para que o próprio binário faça retry em vez do orquestrador externo
- Use `--max-concurrency 1` para forçar serialização estrita quando necessário


## Solução de Problemas
### FAQ — Problemas Comuns
- Banco travado após crash exige `neurographrag vacuum` para fazer checkpoint do WAL
- Primeiro `init` pode levar um minuto baixando modelo quantizado do `fastembed`
- Permissão negada no Linux indica falta de escrita no diretório de cache do usuário
- Detecção de namespace cai para `default` quando não há marcador `.neurographrag`
- Volumes case-insensitive em macOS colidem com nomes kebab-case diferindo apenas no caso


## Contribuindo
### Comunidade — Aberto a Pull Requests
- Issues e pull requests são bem-vindos no repositório do GitHub
- Um `CONTRIBUTING.md` detalhado acompanhará o release 1.0.0
- Reportes de segurança seguem política descrita em `SECURITY.md` quando publicado


## Licença
### Dual — MIT OR Apache-2.0
- Licenciado sob Apache License 2.0 ou MIT License à sua escolha
- Veja `LICENSE-APACHE` e `LICENSE-MIT` na raiz do repositório para texto completo


## Histórico de Mudanças
### Registro — Mantido em Arquivo Separado
- Leia o histórico completo de releases em [CHANGELOG.md](CHANGELOG.md)
