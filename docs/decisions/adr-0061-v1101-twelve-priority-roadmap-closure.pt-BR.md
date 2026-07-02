# ADR-0061: v1.1.01 — Fechamento do Roteiro de Doze Prioridades (Limitações 1–15 do gaps.md)

- **Status**: Accepted
- **Data**: 2026-07-02
- **Versão**: v1.1.01 (nome oficial da release; o `Cargo.toml` carrega `1.1.1` porque o SemVer rejeita zero à esquerda no componente patch — o `User-Agent` HTTP é `sqlite-graphrag/1.1.1` via `CARGO_PKG_VERSION`)

## Contexto

O bloco "Melhoria do GraphRAG — Limitações da CLI sqlite-graphrag" do gaps.md
documenta quinze limitações, cada uma auditada contra o código-fonte da v1.1.0
(confirmações por arquivo e linha) e priorizadas em um roteiro de implementação
de doze prioridades. A auditoria identificou **uma causa raiz arquitetural
comum** por trás da maioria delas: a separação entre o *caminho de escrita* e o
*caminho de manutenção do grafo*. A CLI foi desenhada para escrita incremental
memória a memória, não para manutenção em massa de um corpus já existente —
então toda operação corretiva em lote dependia de comandos que não existiam
(backfill de embeddings, recompute de degree, desambiguação por ID) ou que
normalizavam a entrada de forma destrutiva (`reclassify-relation --from`).
Limitações secundárias de observabilidade agravavam o quadro: o `health`
testava a *existência* da tabela de vetores em vez da cobertura, e o
`embedding status` projetava apenas a fila de memórias pendentes. As
Prioridades 8 e 9 (fila sidecar como verdade no `enrich --status`, ausência
transitória de entidade indo ao dead-letter no primeiro miss) já haviam sido
resolvidas na v1.1.0 como GAP-SG-77 e GAP-SG-78 (ADR-0060). Esta release fecha
as dez restantes: Prioridades 1–7 e 10–12.

## Decisão

1. **Desacoplar o embedding de entidade do subprocesso LLM (P1, Limitação 3).**
   O embedding de entidade roda pela mesma API REST OpenRouter usada por
   memórias e chunks — a chain `[OpenRouter]` vale mesmo com
   `--llm-backend none` — então toda escrita nova recebe vetor de entidade e o
   gap de cobertura deixa de se regenerar. Guardas de vetor vazio são
   adicionadas a `upsert_entity_vec`, `upsert_chunk_vec` e
   `memories::upsert_vec`.

2. **Backfill de embeddings via alvos do re-embed (P2, Limitação 2).**
   `enrich --operation re-embed --target memories|entities|chunks|all`
   introduz scanners novos em `src/commands/enrich/scan.rs` cobrindo
   `entity_embeddings` e `chunk_embeddings`, com `scan_backlog` por alvo no
   `--status`, tornando a convergência do backfill observável.

3. **`graph recompute-degree` (P3, Limitação 4).** Novo subcomando
   (implementado em `src/commands/graph_export.rs`) recalcula o `degree`
   armazenado a partir das arestas reais em transação única, suporta
   `--dry-run` e reporta o envelope `{total, updated, zeroed, unchanged}`.

4. **`reclassify-relation --literal-from` (P4, Limitação 1).** A nova flag
   casa a relação armazenada verbatim, contornando a normalização do
   `value_parser` do clap na borda de argumento
   (`src/commands/reclassify_relation.rs`), de modo que arestas legadas com
   hífen (`applies-to`, `depends-on`) tornam-se migráveis.

5. **Desambiguação de entidade por ID (P5, Limitação 5).**
   `merge-entities --ids/--into-id` e `rename-entity --id` operam sobre IDs de
   entidade com escopo de namespace, eliminando a ambiguidade por nome.

6. **Observabilidade real de cobertura vetorial (P6, Limitações 7 e 8).**
   `health --json` ganha `vec_*_missing` e `vec_*_coverage_pct` com semântica
   real de órfãos e cobertura (não mera existência de tabela), e
   `embedding status --json` ganha contadores `*_missing` por tabela na sua
   seção de cobertura.

7. **Desserialização tipada de `EntityType` (P7, Limitações 6 e 9).**
   `EntityType` implementa um `Deserialize` manual cuja mensagem de erro lista
   os 13 tipos canônicos, validando cedo com mensagem acionável.

8. **Prioridades 8 e 9 — já fechadas na v1.1.0.** `scan_backlog` por operação
   no `enrich --status` (GAP-SG-77) e entidade não-materializada-ainda como
   transitória (GAP-SG-78); ver ADR-0060.

9. **Predicado do re-embed consciente de dimensão (P10, Limitação 13).** As
   funções `reembed_*_predicate` em `scan.rs` selecionam linhas cuja `dim`
   armazenada diverge da dimensão configurada ou cujo blob está vazio — nas
   três tabelas de vetor — em vez de apenas linhas sem vetor.

10. **Erros de limite de payload tipados (P11, Limitação 15).**
    `AppError::BodyTooLarge` e `AppError::TooManyChunks` substituem o
    `LimitExceeded` único e indistinto; o exit 6 é preservado, mas a mensagem
    agora nomeia o teto específico e o valor medido (512000 bytes de corpo,
    512 chunks), de modo que o operador saiba qual limite disparou.

11. **`ingest --name-prefix` (P12, Limitação 14).** O ingest aceita um prefixo
    de nome com validação de teto e orçamento reduzido para o nome derivado,
    dando controle de nomenclatura às importações em lote.

O schema permanece em v15 — sem migração. O binário da release tem ~19 MiB.

## Alternativas Consideradas

- **`UPDATE` SQL direto no `.sqlite` para manutenção.** Rejeitada: a regra do
  projeto proíbe escrever no banco fora do binário; todo caminho de manutenção
  deve ser um comando de primeira classe da CLI.
- **`1.1.01` como versão do Cargo.** Rejeitada: o SemVer proíbe zero à
  esquerda no componente patch, então o `Cargo.toml` carrega `1.1.1` enquanto
  o nome oficial da release permanece v1.1.01.
- **Um comando separado `embedding backfill` em vez de alvos no re-embed.**
  Rejeitada: estender `enrich --operation re-embed` com `--target` reutiliza a
  máquina existente de fila, `--status`, `--resume` e dead-letter em vez de
  duplicá-la.
- **Um subcomando administrativo `relation-rename-raw` em vez de
  `--literal-from`.** Rejeitada: uma flag no comando existente mantém a
  superfície menor e a semântica de filtro adjacente ao padrão normalizador.

## Consequências

### Positivas

- A causa raiz é atacada nas duas pontas: o caminho de escrita deixa de
  regenerar o gap de vetores de entidade (P1) e o caminho de manutenção
  finalmente existe para o passivo histórico (P2, P3, P4, P5).
- `health` e `embedding status` tornam-se instrumentos reais de cobertura — um
  backfill convergido é verificável por `vec_*_coverage_pct` e `*_missing` em
  vez de inferido.
- O exit 6 torna-se diagnosticável: o operador dimensiona splits pelo teto que
  de fato disparou em vez de adivinhar entre bytes e quantidade de chunks.
- Arestas de relação legadas com hífen ganham, pela primeira vez, um caminho
  de migração literal e seguro.
- O drift de dimensão (Limitação 13) agora é selecionável pelos scanners do
  re-embed, então um corpus embedado em dimensão legada pode ser
  re-vetorizado in place.

### Negativas / Notas

- O código fecha o roteiro, mas o **passivo do banco de produção permanece
  pendente de execução operacional**: backfill, recompute de degree e a
  migração das arestas com hífen ainda precisam ser rodados com os novos
  comandos.
- A divisão nome-da-release/versão-Cargo (v1.1.01 vs `1.1.1`) exige cuidado na
  comparação de versões; o `User-Agent` reporta `1.1.1`.
- P8/P9 são documentadas aqui apenas para completude do roteiro; o registro de
  design delas é o ADR-0060.

## Referências Cruzadas

- gaps.md — bloco "Melhoria do GraphRAG", Limitações 1–15 e o
  "Roteiro de Implementação Recomendado" (Prioridades 1–12).
- CHANGELOG.md — seção v1.1.01.
- ADR-0059 — v1.0.99 (remoção do degree-cap, convergência de docs).
- ADR-0060 — v1.1.0 (convergência do backlog de enrichment, GAP-SG-70..78;
  P8/P9).
- Código: `src/commands/enrich/scan.rs`, `src/commands/graph_export.rs`,
  `src/commands/reclassify_relation.rs`, `src/commands/health.rs`,
  `src/commands/embedding.rs`, `src/entity_type.rs`, `src/errors.rs`,
  `src/storage/entities.rs`.
