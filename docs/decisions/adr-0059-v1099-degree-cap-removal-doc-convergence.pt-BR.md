# ADR-0059 — v1.0.99: Remover Poda Destrutiva do Degree Cap; Alinhar Doc do sort-by-degree; Convergir body-enrich

- **Status**: Aceito
- **Data**: 2026-06-30
- **Versão**: v1.0.99 (fecha GAP-SG-67, GAP-SG-68, GAP-SG-69)

## Contexto

Uma escrita `remember` real — uma memória referenciando duas entidades super-hub
preexistentes — podou silenciosamente ~4856 arestas históricas e saiu com exit 0
emitindo apenas um WARN. Os hubs (grau 2872 e 2073) foram capados porque
`graph::enforce_degree_cap` aparava as arestas de menor peso até cada nó ficar sob
o cap, sem filtro de `memory_id`: ele varria TODA aresta que tocava o hub, não só
as arestas que a escrita atual introduziu. Combinado com `ON DELETE CASCADE`, o cap
deletou arestas que pertenciam a outras memórias. A escrita reportou sucesso; o
total de relações caiu em milhares. Este é o GAP-SG-67 — uma escrita virou
destrutiva contra estado histórico do grafo que ela nunca possuiu. O cap fora
conectado como recurso "acionável" na v1.0.97 (GAP-SG-49), mas a deleção global e
cega ao dono o tornou um risco de perda de dados em vez de um guardrail.

Dois defeitos menores surgiram na mesma auditoria:

- GAP-SG-68 — `graph entities --sort-by degree` (sem `--order`) ordenava
  ASCENDENTE, contradizendo o doc-comment de `EntitySortField::Degree` em
  `src/commands/graph_export.rs`, que prometia "descendente por padrão". O texto do
  `--help` herdava a promessa errada, então quem pedia "degree" recebia as
  entidades menos conectadas primeiro, sem aviso.

- GAP-SG-69 — `enrich --operation body-enrich --until-empty` não convergia. O scan
  re-enfileirava corpos curtos que o guard de preservação trigram-Jaccard já tinha
  rejeitado (`status='skipped'`), então cada passada re-julgava as mesmas memórias
  vetadas e o `--until-empty` nunca terminava.

## Decisão

1. **Remover a poda destrutiva do degree cap (GAP-SG-67).** Deletar
   `graph::enforce_degree_cap` e seus dois call sites em `remember` e `link`.
   Remover a flag `--max-entity-degree` de `remember` e `link` (BREAKING —
   scripts que ainda a passarem recebem erro de argumento do clap, exit 2; a
   mitigação obsoleta `--max-entity-degree 0` deixa de ser necessária). A escrita
   agora é 100% aditiva: nunca poda, deleta arestas nem emite warn de grau, e a
   contagem total de relações nunca diminui numa escrita normal. O schema
   permanece na versão 15 — sem migração.

2. **Alinhar a doc do sort-by-degree ao comportamento real (GAP-SG-68).** Em vez
   de inverter a direção da ordenação (o que mudaria um contrato SQL de longa data
   exercitado pelos testes `build_order_by_*`), reescrever o doc-comment de
   `EntitySortField::Degree` para casar com o comportamento ascendente: "Ordenar
   por grau (número total de relações). Use `--order desc` para os mais conectados
   primeiro." Uma linha de doc-comment em `src/commands/graph_export.rs`; os 6
   testes `build_order_by_*` seguem verdes.

3. **Convergir o body-enrich (GAP-SG-69).** Adicionar `skipped_item_keys`
   (`src/commands/enrich/queue.rs`), que lê os item_keys com `status='skipped'`
   para uma dada operação. O scan inicial e o rescan de `BodyEnrich`
   (`src/commands/enrich/mod.rs`) excluem memórias já vetadas como `skipped`,
   então o conjunto vivo encolhe estritamente. O `remove_file` do sidecar
   `.enrich-queue.sqlite` só roda quando `dead==0` E `skipped==0`, preservando o
   veredito de veto entre passadas. O `cleanup_queue_entry` (chamado em
   remember/edit/forget/purge) limpa o veto quando o corpo muda, então um corpo
   editado é reconsiderado automaticamente. Escopo restrito a `BodyEnrich`.

## Consequências

### Positivas

- A escrita é não-destrutiva por padrão: um `remember`/`link` referenciando um hub
  de grau alto não pode mais deletar as arestas de outra memória. O histórico do
  grafo é preservado.
- `graph entities --sort-by degree --help` não mente mais; o usuário recebe a
  ordem ascendente documentada e um ponteiro para `--order desc`.
- `enrich --operation body-enrich --until-empty` converge: empiricamente,
  items_total caiu 55→3 na segunda passada, com o veredito skipped respeitado
  entre passadas. Teste de regressão
  `skipped_item_keys_excludes_only_skipped_for_operation`.

### Negativa / Trade-off (GAP-SG-67)

- Sem o cap, o grau dos hubs cresce sem limite. Isso é aceito: uma escrita jamais
  pode deletar silenciosamente dados que não possui. Qualquer normalização futura
  de grau deve ser um comando de MANUTENÇÃO explícito (invocado pelo operador,
  ciente do dono), nunca um efeito colateral de uma escrita normal.

### Schema

- Sem migração; o schema permanece v15.

## Irmão

Reverte a conexão do GAP-SG-49 da linha de release do ADR-0056 (v1.0.97), que
tornou `enforce_degree_cap` "acionável" sem escopar a deleção às arestas da escrita
atual.
