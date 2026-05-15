# Guia de Migração — neurographrag para sqlite-graphrag

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
