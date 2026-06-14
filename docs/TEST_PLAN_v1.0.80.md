# Plano de Testes v1.0.80 — Validação Pós-Publicação

- Criado em 2026-06-14 logo após publicação da v1.0.80 em GitHub e crates.io
- Foco: Layer 7 do `docs/TEST_PLAN.md` aplicada especificamente à release v1.0.80
- Alvo: binário instalado de crates.io (`cargo install sqlite-graphrag --version 1.0.80`)
- Ambiente: banco isolado em `/tmp/test-v1-0-80-cli/` com namespace `test-cli-v1-0-80`


## Objetivo
### Propósito
- Validar comportamento end-to-end da CLI v1.0.80 instalada do crates.io
- Confirmar que o binário publicado responde ao contrato JSON documentado
- Detectar regressões introduzidas entre v1.0.79 e v1.0.80 antes de qualquer adoção em produção
- Servir como smoke test reproduzível por usuários e CI
### Fora do Escopo
- Auditoria de código-fonte (já feita durante a release via A1/A2)
- Comparação de performance entre versões
- Teste de carga ou stress
- Cobertura de mutação


## Pré-Requisitos
### Ambiente
- Binário `sqlite-graphrag` v1.0.80 instalado em `~/.cargo/bin/`
- PATH com `~/.cargo/bin` antes de `/usr/bin` (evita sombreamento por `timeout`)
- `atomwrite` 0.1.18+ disponível para escrita atômica do relatório
- `jaq` disponível para parsing JSON
- `rg` (ripgrep) disponível para busca em logs
### Isolamento
- Diretório de teste dedicado: `/tmp/test-v1-0-80-cli/`
- Banco isolado: `/tmp/test-v1-0-80-cli/test.sqlite`
- Namespace dedicado: `test-cli-v1-0-80`
- Variável `SQLITE_GRAPHRAG_DB_PATH` em todas as invocações


## Fases do Plano
### Fase 1 — Verificação de Instalação
- Confirmar `sqlite-graphrag --version` retorna `1.0.80`
- Listar subcomandos via `--help` e verificar todos os 49 subcomandos da release
- Inspecionar flags globais e validar `--max-concurrency`, `--lang`, `--tz`, `--verbose`
- Critério: versão reportada e help listam os mesmos subcomandos do `src/commands/`
### Fase 2 — Bootstrap
- Rodar `init` em banco isolado com namespace dedicado
- Validar `health` reporta `integrity_ok`, `schema_ok`, `vec_memories_ok`, `fts_ok`
- Validar `stats` reporta `schema_version: 13`
- Validar `migrate --json` não tem migrações pendentes
- Critério: banco inicializado e íntegro em menos de 60s
### Fase 3 — CRUD Essencial
- `remember` cria memória com `--name`, `--type`, `--description`, `--body`
- `read --name` retorna a memória com `body`, `description`, `created_at_iso`
- `list --json` lista memórias e filtra por `--type`
- `edit` atualiza `body` e regenera embedding
- `forget` faz soft-delete
- `restore` revive memória soft-deletada
- Critério: cada operação emite JSON com os campos documentados
### Fase 4 — Busca Semântica
- `recall` retorna resultados KNN com `score` e `distance`
- `hybrid-search` funde FTS5 + vetorial via RRF
- `related` faz travessia multi-hop a partir de memória
- `graph entities` e `graph stats` inspecionam o grafo
- Critério: cada busca retorna array `results` não-vazio quando há memórias relevantes
### Fase 5 — Subcomandos da Release v1.0.80
- `completions bash` (A1 audit, v1.0.67) — exit 0 e markers `_sqlite-graphrag`
- `fts check` e `fts stats` (FTS5)
- `vec stats` (G39)
- `backup` cria backup consistente via SQLite Online Backup API
- `sync-safe-copy` cria checkpoint seguro
- `optimize` executa PRAGMA optimize
- `namespace-detect` resolve namespace precedence
- Critério: cada subcomando emite JSON com campos documentados no schema
### Fase 6 — Contrato de Erros
- Tentar `--claude-timeout -1` ou similar para trigger exit 1 (Validation)
- Tentar shell inválido em `completions` para trigger exit 2 (Clap)
- Tentar `read --name inexistente` para trigger exit 4 (Not Found)
- Critério: cada erro emite envelope JSON `{"error": true, "code": N, "message": "..."}` quando `--json`


## Validação Final
### Critérios de Aprovação
- Todos os 6 fases acima com ZERO falhas
- Logs de tracing em stderr nunca poluem stdout JSON
- Exit codes correspondem ao contrato documentado (0, 1, 2, 4, 9, 75)
- `health` final reporta `integrity_ok: true` após todas as operações
### Saídas a Persistir
- Relatório de execução em `docs/TEST_PLAN_v1.0.80_results.md`
- Memória GraphRAG `skill-test-cli-v1-0-80-2026-06-14` com achados consolidados
- Entidades e relações curadas via `--graph-stdin` ligando release, binário, suíte
### Cleanup
- `rm -rf /tmp/test-v1-0-80-cli/` ao final
- Não persistir banco de teste no repositório
