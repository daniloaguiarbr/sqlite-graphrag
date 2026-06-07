# ADR-0020: Cosseno em Rust Puro (v1.0.76)

- Status: Aceito (2026-06-07)
- Decisores: Danilo Aguiar
- Escopo: src/similarity.rs, src/storage/memories.rs, src/storage/entities.rs, src/storage/chunks.rs

## Contexto

`sqlite-vec` expunha uma tabela virtual vec0 com colunas `MATCH` e `distance` que retornavam resultados KNN pré-ordenados. A extensão era C, carregada em runtime via `sqlite3_auto_extension` e produzia valores de distância no intervalo `[0.0, 2.0]` (distância cosseno: `1.0 - similaridade`).

A v1.0.76 remove `sqlite-vec`. O substituto é similaridade por cosseno em processo dentro de `src/similarity.rs::cosine_similarity`. A função retorna `[-1.0, 1.0]`; `similarity_to_distance` inverte para `[0.0, 2.0]` para que o resto do código (que lê colunas `distance` em resultados KNN) continue funcionando sem mudanças.

## Decisão

A busca KNN em `storage::memories::knn_search` e `storage::entities::knn_search` agora é:

1. Um scan completo da tabela relevante com backing BLOB.
2. Um produto escalar puro-Rust + normas L2 para cada linha.
3. Ordenação por distância ascendente; truncamento para `k`.

Para a dimensão padrão de embedding de 384 e tamanhos de namespace abaixo de 10k memórias, isso é O(N × 384) por chamada, o que roda em milissegundos de um dígito em hardware moderno. As características de performance são aceitáveis para o caso de uso de memória do GraphRAG (recall + hybrid-search em corpora pessoais, não busca vetorial em escala de milhões).

## Consequências

### Positivas

- Zero dependências externas. A busca vetorial não exige mais a extensão C `sqlite-vec` carregável em runtime.
- Performance previsível. Sem mais comportamento estranho do alocador interno vec0, sem mais `SQLITE_BUSY` em escritas em tabelas vec, sem mais KNN com semântica ORDER BY estranha.
- Cosseno é trivial de testar em Rust puro — ver `src/similarity.rs::tests` com 7 testes unitários cobrindo casos de borda (vetor zero, comprimentos incompatíveis, idênticos, ortogonais, opostos, inversão de `similarity_to_distance`, ordenação `top_k`).

### Negativas

- O(N × D) por chamada KNN. Para tamanhos de namespace acima de 100k memórias, isso se torna o gargalo. Operadores com namespaces muito grandes devem confiar no FTS5 (`hybrid-search`) para filtragem grossa antes de chegar ao caminho KNN; ver ADR-0024 para a estratégia de particionamento.
- Sem mais `vec_top_k`, `vec_quantize`, `vec_quantize_binary`, ou quaisquer outros built-ins vec0. Se o operador precisar de KNN aproximado estilo HNSW, deve recompilar com `--features embedding-legacy` e usar o KNN vec0 anterior.

## Verificação

- `tests/extract_backend`, `tests/spawn_version_adapter`, `tests/concurrency_adaptive`: 31 testes unitários verdes.
- `cargo test --lib`: 711 testes verdes.
- `cargo test --test storage::memories::tests::upsert_vec_and_delete_vec_work`: verde após a troca de schema para `memory_embeddings`.
- `cargo test --test storage::chunks::tests::test_upsert_chunk_vec_and_knn_search`: verde após a troca de schema para `chunk_embeddings`.
