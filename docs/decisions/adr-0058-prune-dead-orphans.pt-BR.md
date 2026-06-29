# ADR-0058: `enrich --prune-dead-orphans` — limpar dead-letter órfão (v1.0.97)

- **Status**: Aceito
- **Data**: 2026-06-29
- **Versão**: v1.0.97 (fecha GAP-SG-66)

## Contexto

Uma auditoria dos hooks do Claude Code contra a v1.0.97 achou
`lib/graphrag-recover-dead.sh` quebrado: ele chamava `sqlite-graphrag pending
list --namespace <ns> --filter-status dead`, que a v1.0.97 REJEITA com exit 2 —
`pending list` não aceita `--namespace`, e `dead` não é valor de
`--filter-status` (`[validated, embedding_in_progress, embedding_done,
committed, abandoned, failed]`). Também mirava a tabela errada: o dead-letter
vive no sidecar da fila de enrich (`.enrich-queue.sqlite`), inspecionado por
`enrich --list-dead` (GAP-SG-23), não na tabela `pending` de embedding.

Corrigir o hook expôs o GAP-SG-66. O banco do projeto tinha 110 linhas dead,
TODAS `error_class=permanent` com `error="not found: memory 'X' not found"` —
órfãs deixadas pela fila CWD-relativa legada (ADR-0057): a memória foi renomeada
ou purgada APÓS ser enfileirada, então a linha dead aponta para um nome que não
existe mais.

Causa -> efeito: a fila indexa por `item_key`/`memory_id`; quando a memória some,
a linha dead vira órfã, e `cleanup_queue_entry` (GAP-SG-13) só dispara em
`forget`/`purge` de memória EXISTENTE. Nenhum comando descarta dead órfão:
`--requeue-dead` só as re-falha (not-found permanente volta direto a `dead`),
então `queue_dead` cresce de forma monotônica e os avisos de dead-letter dos
hooks viram ruído permanente.

## Decisão

1. Adicionar `enrich --prune-dead-orphans`: inspetor read-only (sem LLM, sem
   singleton) no grupo `required_unless_present_any`, então `--operation` e
   `--mode` ficam opcionais (como `--list-dead`/`--requeue-dead`).

2. `queue::prune_dead_orphans(queue_conn, main_conn, operation, namespace)`
   deleta só linhas `status='dead' AND item_type='memory'` cujo `item_key` (o
   nome da memória) está ausente do banco principal, reusando a query de
   existência do `enqueue_candidate`: `SELECT id FROM memories WHERE
   namespace=?1 AND name=?2 AND deleted_at IS NULL`. Linhas com chave de entidade
   (`item_type='entity'`) ficam intocadas — a chave é nome de entidade, não de
   memória. Read-only no banco principal; só o sidecar é mutado.

3. `DeadSummary` ganha o campo `pruned: i64`. NÃO é uma struct dumpada para
   schema (só `EnrichSummary`/`EnrichStatus` estão em `docs/schemas/`), então a
   adição é neutra ao schema.

4. Hooks reconectados:
   - `lib/graphrag-recover-dead.sh` (GAP-A) agora itera `GR_OPS_GATE` por
     namespace, poda órfãos via `--prune-dead-orphans`, e recupera o dead
     restante (corpo real) via `forget`+`purge`+`remember`.
   - `lib/graphrag-enrich-worker.sh` (GAP-B) o residual passa a emitir
     `total_dead` db-scoped — confiável desde o ADR-0057, que escopou a fila ao
     `--db`; o comentário anterior "queue_dead não escopado por --db" ficou
     obsoleto. Isso conserta os consumidores `auto-enrich.sh`/`memory-guardian.sh`,
     que liam `total_dead` de um produtor que nunca o escrevia.
   - `lib/graphrag-common.sh` centraliza `GR_OPS_GATE`, `gr_dead_total` e
     `gr_prune_orphans` (DRY).

### Por que podar, não re-enfileirar

`--requeue-dead` re-falha um item not-found permanente; a poda é a única limpeza
terminal-safe. Ela deleta SÓ linhas confirmadas órfãs pela MESMA checagem de
existência que o worker usa, que não tinham valor recuperável algum.

## Consequências

- `queue_dead` fica honesto; `recover-dead.sh` fecha o loop (poda órfão +
  recupera o resto); o `total_dead` do worker conserta os avisos de dead-letter
  dos hooks (GAP-B).
- Novo teste unitário `prune_dead_orphans_removes_only_orphan_memory_rows`; smoke
  real no banco do projeto podou 110 órfãos (`dead_total` 110->0, `pruned:110`).
- Schema de saída da CLI inalterado; `installed_binary_smoke` segue 26/0 após
  `cargo install --path . --locked --force`.

## Irmão

Segue o ADR-0057 — as linhas dead órfãs são o resíduo legado dele; este ADR
adiciona o caminho de limpeza que o fix de escopo da fila não pôde aplicar
retroativamente.
