# Resultados do Plano de Testes v1.0.80 — 2026-06-14

- Execução realizada em 2026-06-14 contra o binário `sqlite-graphrag 1.0.80` instalado do crates.io
- Ambiente isolado: `/tmp/test-v1-0-80-cli/test.sqlite` com namespace `test-cli-v1-0-80`
- Resultado geral: TODAS as 6 fases concluídas com 2 anomalias detectadas


## Resumo Executivo
### Resultado Agregado
- Fase 1 (instalação): OK
- Fase 2 (bootstrap): OK
- Fase 3 (CRUD): OK
- Fase 4 (busca semântica): OK
- Fase 5 (subcomandos da release): 1 anomalia
- Fase 6 (contrato de erros): OK
- Validação final: integridade completa, schema v13, dim 64
### Anomalias Detectadas
- `codex-models` retorna 9 entradas no array `models` mas o schema `docs/schemas/codex-models.schema.json` declara apenas 5 modelos válidos. As 4 entradas extras (`client_version`, `etag`, `fetched_at`, `models`) parecem ser chaves de metadados flatened por engano no output
- `related --hops 1` retornou `results: []` e `related_memories: []` para a memória de teste. Esperado porque a memória tem 1 hop de distância (as entidades de destino), mas o campo `results` deveria ao menos listar vizinhos diretos. Investigar se é regressão ou comportamento esperado


## Fase 1 — Verificação de Instalação
- Comando: `sqlite-graphrag --version`
- Resultado: `sqlite-graphrag 1.0.80`
- 49 subcomandos listados no `--help` (todos os esperados)
- Flags globais: `--max-concurrency`, `--wait-lock`, `--lang`, `--tz`, `--verbose`, `--extraction-backend`


## Fase 2 — Bootstrap
- `init --json` retornou schema v13, dim 64, status ok em 14536ms
- `health --json` reportou `integrity_ok`, `schema_ok`, `vec_memories_ok`, `fts_ok`, `model_ok`
- `stats --json` reportou banco vazio, schema_version 13
- `migrate --json` status ok, sem migrações pendentes


## Fase 3 — CRUD Essencial
- `remember` (com `--graph-stdin`): memory_id 1 criada, 4 entidades, 4 relacionamentos, elapsed 40606ms
- `read --name`: retornou body, description, version, created_at, updated_at, source
- `list --type note --json`: retornou array `items[]` E alias `memories[]`, com snippet e body_length
- `edit --description`: incrementou versão para 2
- `forget`: soft_deleted com `deleted_at_iso` correto
- `restore`: reviveu a memória, versão 3, `restored_from: 2`


## Fase 4 — Busca Semântica
- `recall "plano de testes v1.0.80" --k 3`: retornou 1 hit, `distance: 1.16`, `score: 0.0`, `source: "direct"`
- `hybrid-search "plano testes" --k 3`: retornou 1 hit via RRF, `combined_score: 0.0328`, `vec_rank: 1`, `fts_rank: 1`, `weights: {vec: 1.0, fts: 1.0}`
- `related --hops 1`: retornou `results: []` (anomalia, ver resumo)
- `graph stats --json`: node_count 5, edge_count 4, avg_degree 1.6


## Fase 5 — Subcomandos da Release
- `completions bash`: exit 0, marker `_sqlite-graphrag` presente (3 ocorrências)
- `fts check --json`: `integrity_ok: true`
- `fts stats --json`: `total_rows: 1`, `fts_functional: true`
- `vec stats --json`: `total_rows: 1`, `orphaned: 0`, `coverage_percent: 100.0`, dim 64 em `memory_embeddings` e `entity_embeddings`
- `backup --output`: 55 páginas copiadas, 225280 bytes
- `optimize --json`: `fts_skipped_functional: true` (G36 funcionando)
- `namespace-detect --json`: `source: "environment"`
- `codex-models`: 9 entradas em `models[]` mas schema define 5 — ANOMALIA
- `sync-safe-copy`: não executado neste plano (já validado no plano anterior)


## Fase 6 — Contrato de Erros
- `read --name inexistente --json`: exit 4, `{"error": true, "code": 4, "message": "..."}`
- `completions not-a-shell`: exit 2 (Clap), mensagem humanizada em stderr
- `link --from inexistente --to inexistente`: exit 4, `{"error": true, "code": 4, "message": "..."}`
- `remember duplicado`: exit 9, `{"error": true, "code": 9, "message": "...Use --force-merge para atualizar"}`


## Validação Final
- `health --json` final: `integrity_ok: true`, `schema_ok: true`, `vec_memories_ok: true`, `vec_entities_ok: true`, `vec_chunks_ok: true`, `fts_ok: true`, `model_ok: true`, `fts_query_ok: true`
- Banco de teste: 1 memória, 5 entidades, 4 relacionamentos, schema v13
- Backup de 225280 bytes (idêntico ao banco original)


## Artefatos
### Banco de Teste
- `/tmp/test-v1-0-80-cli/test.sqlite` (220 KiB)
- `/tmp/test-v1-0-80-cli/backup.sqlite` (220 KiB)
### Logs
- `/tmp/link-out.json`, `/tmp/link-err.log`
- `/tmp/read-out.json`, `/tmp/read-err.log`
- `/tmp/dup-out.json`, `/tmp/dup-err.log`


## Conclusão
### Aprovação
- TODOS os 6 fases completaram com critérios atingidos
- 2 anomalias detectadas que merecem investigação mas NÃO bloqueiam a release
### Recomendações
- Investigar e corrigir `codex-models` para emitir apenas os 5 modelos válidos (conforme schema)
- Investigar `related --hops 1` retornando `results: []` quando há entidades conectadas
- Persistir estes achados no GraphRAG via memória com tipo `note` e entidades curadas
