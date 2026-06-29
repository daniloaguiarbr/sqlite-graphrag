# ADR-0057: Sidecar de fila do enrich + ingest derivado do `--db`, não do CWD (v1.0.97)

- **Status**: Aceito
- **Data**: 2026-06-29
- **Versão**: v1.0.97 (fecha GAP-SG-64 e o GAP-SG-65 recém-descoberto)

## Contexto

A auditoria e2e da v1.0.97 expôs uma classe de bug: bancos sidecar de fila
resolvidos contra o CWD do processo em vez do diretório do banco `--db`.

- GAP-SG-64 (enrich): `const DEFAULT_QUEUE_DB: &str = ".enrich-queue.sqlite"`
  (`src/commands/enrich/mod.rs`) era aberto via `Connection::open` com literal
  relativo, então `enrich --status --db X` reportava a fila do CWD, não a de X.
- GAP-SG-65 (ingest, achado ao corrigir o 64): a flag clap
  `#[arg(long, default_value = ".ingest-queue.sqlite")]` (`IngestArgs.queue_db`,
  consumida por `ingest_claude.rs` E `ingest_codex.rs`) tinha o mesmo default
  CWD-relativo, então `--resume`/`--retry-failed` perdiam a fila quando o CWD
  mudava entre execuções.

Causa -> efeito: um caminho de fila estático/relativo (const ou `default_value`
do clap) resolve contra o CWD em vez de `AppPaths::resolve(--db).db`. Duas fontes
de verdade divergem: o scan respeita `--db` (`unbound_backlog` correto) enquanto
a fila é CWD-fixa, gerando `--status` enganoso e risco de cross-processing por
colisão de `memory_id` ao drenar uma fila enfileirada para outro banco.

Evidência empírica: com o mesmo `--db` e `--namespace`, `enrich --status`
reportou `queue_pending=111` do CWD do projeto e `0` de `/tmp/e2e-cwd-test`; a
única variável era o CWD.

Irmãos verificados e já seguros (sem ação): `slots_dir()` resolve
`XDG_RUNTIME_DIR -> SQLITE_GRAPHRAG_CACHE_DIR -> HOME/.local/share -> /tmp`;
`lock.rs` usa `cache_dir()`; o `schema_path` do codex usa `trusted_schema_path()`
(cache dir) ou tempfile. Os únicos membros vivos da classe eram as duas filas.

## Decisão

1. Adicionar `paths::sidecar_path(db_path: &Path, filename: &str) -> PathBuf` ao
   lado do `parent_or_err` existente. Deriva o sidecar no diretório-pai do banco
   e cai graciosamente no nome puro (CWD) quando `db_path` não tem pai —
   preservando o layout legado do banco default.

2. Enrich (GAP-SG-64): ampliar `open_queue_db` para `P: AsRef<Path>`
   (`rusqlite::Connection::open` já é genérico sobre `AsRef<Path>`), remover a
   const relativa `DEFAULT_QUEUE_DB` e derivar `queue_path` de `paths.db` nos
   quatro ramos de `run` (list-dead/requeue, status, dreno principal e o closure
   do worker via re-borrow `&queue_path`). O público `cleanup_queue_entry` ganha
   um parâmetro inicial `db_path: &Path`; os três chamadores (`forget`, `purge`,
   `remember`) passam o `paths.db` resolvido (`purge` propaga via
   `execute_purge`).

3. Ingest (GAP-SG-65): `IngestArgs.queue_db` vira `Option<String>` sem default
   clap; `run_claude_ingest`/`run_codex_ingest` resolvem
   `queue_path = args.queue_db.as_deref().map(PathBuf::from).unwrap_or_else(|| sidecar_path(&early_paths.db, ".ingest-queue.sqlite"))`.
   Um `--queue-db` explícito ainda sobrepõe.

4. Remover a constante morta `constants::CLI_LOCK_FILE` (zero usos).

### Por que sem migração legada

`AppPaths::resolve(None)` sem `SQLITE_GRAPHRAG_HOME` retorna
`current_dir().join("graphrag.sqlite")` (absoluto), então o sidecar derivado
COINCIDE com o legado `./.enrich-queue.sqlite` quando rodado do diretório do
projeto — o fluxo canônico mantém seu backlog sem mover arquivo. Quando `--db`
aponta para outro lugar, a fila do CWD pertence ao banco do CWD, então deixá-la
para trás é o correto; migrar mis-vincularia `memory_id`s. Mover um arquivo
SQLite com WAL foi deliberadamente evitado.

## Consequências

- `enrich --status` e o `--resume` do ingest seguem `--db`; a fila fica isolada
  por diretório de banco; cross-processing por `memory_id` entre bancos que
  compartilham um CWD é eliminado.
- Mudança de API pública: assinaturas de `cleanup_queue_entry` e
  `IngestArgs.queue_db` mudaram (interna, não publicada; schema de saída da CLI
  inalterado — `schema_contract_strict` segue 38/0).
- Novo teste de regressão `tests/enrich_queue_db_isolation.rs` planta uma fila ao
  lado de `db_a` e prova que `--status` a lê de um CWD não-relacionado (a
  divergência que a suíte anterior nunca exercia, pois os testes de integração
  rodavam onde CWD == --db).

## Irmão

Irmão de design do GAP-SG-63 (isolamento CWD/XDG do slots_dir), resolvido na
v1.0.97.
