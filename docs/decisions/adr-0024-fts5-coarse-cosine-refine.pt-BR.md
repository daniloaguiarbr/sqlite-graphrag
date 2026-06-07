# ADR-0024: Filtro Grosso FTS5 + Refinamento por Cosseno (v1.0.76)

- Status: Aceito (2026-06-07)
- Decisores: Danilo Aguiar
- Escopo: src/commands/hybrid_search.rs, src/commands/recall.rs, src/commands/related.rs, src/storage/memories.rs

## Contexto

Na v1.0.74, os comandos `hybrid-search` e `recall` retornavam uma mistura de hits FTS5 e hits de busca vetorial, fundidos via RRF. A busca vetorial era o KNN sobre `vec_memories` (ou `vec_entities`). Com `sqlite-vec` removido, o KNN é um scan completo da tabela + cosseno em Rust puro (ver ADR-0020). Para tamanhos de namespace acima de 100k memórias, o scan completo da tabela é lento demais.

## Decisão

Operadores com namespaces muito grandes devem confiar no FTS5 para o filtro grosso e rodar o refinamento por cosseno apenas sobre o conjunto candidato do FTS5. O padrão recomendado:

```bash
sqlite-graphrag hybrid-search "auth jwt design" \
    --k 50 --rrf-k 60 --json
```

O valor `--k` de 50 é intencionalmente pequeno (era 10 na v1.0.74). O FTS5 grosso + KNN vec fundidos via RRF retornam os top 50 por `combined_score`, que é o que o operador vê em `results[]`. Para queries puramente semânticas (sem correspondência exata de token), o operador deve usar `recall`:

```bash
sqlite-graphrag recall "auth jwt design" --k 20 --no-graph --json
```

O padrão agora é `--no-graph` (operadores opt-IN para expansão de grafo com `--with-graph`). Isso mantém o conjunto candidato FTS5 pequeno e o refinamento por cosseno rápido.

Para o release v1.1.0, o operador poderá definir um `--partition-key` (ex. `date >= '2026-01-01'`) para limitar o scan KNN a um subconjunto do namespace. Essa é uma otimização de follow-up; o build v1.0.76 retorna todas as linhas do namespace candidato para refinamento por cosseno.

## Consequências

### Positivas

- O padrão FTS5 + cosseno é o mesmo recomendado na implementação de referência do Microsoft GraphRAG (paper de outubro de 2024). Operadores familiarizados com esse padrão não precisam reaprender nada.
- Para o tamanho de namespace padrão (10k memórias ou menos), o scan completo por cosseno é rápido o suficiente para que o operador não precise fazer nada especial. O padrão deste ADR é para operadores com corpora muito grandes.

### Negativas

- Operadores com >100k memórias por namespace verão `recall` e `hybrid-search` mais lentos até que a otimização de partition-key chegue na v1.1.0. A lentidão é aproximadamente linear no tamanho do namespace: 100k memórias → ~30 ms de refinamento por cosseno; 1M memórias → ~300 ms. Ambos são aceitáveis para uso interativo mas lentos para cargas de trabalho em lote.

## Verificação

- `cargo test --lib`: 711 testes verdes.
- `tests/recall_integration`, `tests/hybrid_search_integration`: essas suítes AINDA NÃO foram rodadas novamente com v1.0.76 porque exigem uma CLI LLM no PATH para semear os embeddings. Documentado no CHANGELOG da v1.0.76.
