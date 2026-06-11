# ADR-0022: Embeddings com Backing BLOB (v1.0.76)

- Status: Aceito (2026-06-07)
- Atualização (v1.0.79): a válvula de escape `embedding-legacy` mencionada abaixo foi removida antecipando o cronograma da v1.1.0; a janela de transição está fechada
- Decisores: Danilo Aguiar
- Escopo: migrations/V013, src/storage/connection.rs, src/storage/memories.rs, src/storage/entities.rs, src/storage/chunks.rs

## Contexto

Na v1.0.74, os vetores de embedding eram armazenados em três tabelas virtuais vec0 (`vec_memories`, `vec_entities`, `vec_chunks`) fornecidas pela extensão `sqlite-vec`. Essas tabelas:

- Exigiam uma extensão C carregada no startup via `sqlite3_auto_extension`.
- Não suportavam `INSERT OR REPLACE` (forçavam o código a fazer `DELETE` + `INSERT` para cada escrita de embedding).
- Não tinham FK CASCADE, forçando limpeza explícita na camada de storage (`vec0 lacks FK CASCADE — clean vec_entities explicitly` era um comentário no código).
- Armazenavam o embedding como um blob opaco `float[384]`, sem metadados sobre qual modelo o produziu.

## Decisão

A migration V013 (`migrations/V013__drop_vec_use_blob_embeddings.sql`) dropa as três tabelas vec e cria três tabelas comuns com backing BLOB:

- `memory_embeddings(memory_id PK, namespace, embedding BLOB, source, model, dim, created_at, updated_at)`
- `entity_embeddings(entity_id PK, namespace, embedding BLOB, source, model, dim, created_at, updated_at)`
- `chunk_embeddings(chunk_id PK, memory_id, embedding BLOB, source, model, dim, created_at)`

A coluna `embedding` é uma sequência f32 little-endian de 384 × 4 = 1536 bytes, produzida por `embedder::f32_to_bytes` e consumida por `embedder::bytes_to_f32`. A coluna `source` é uma de `"llm-claude"`, `"llm-codex"` ou `"legacy-fastembed"` (a última apenas quando a feature `embedding-legacy` está habilitada); a coluna `model` armazena o nome do modelo LLM (ex. `claude-sonnet-4-6`).

As colunas `source` e `model` permitem ao operador auditar qual LLM produziu cada embedding. Isso era impossível com vec0 porque o array de floats era opaco.

## Consequências

### Positivas

- Sem extensão externa. A CLI não exige mais `sqlite-vec` carregável em runtime; as tabelas de embedding são SQLite puro.
- FK CASCADE funciona. Deletar uma memória via `DELETE FROM memories` limpa automaticamente `memory_embeddings` via a cláusula `ON DELETE CASCADE`.
- INSERT OR REPLACE funciona. A linha única `INSERT OR REPLACE INTO memory_embeddings (...) VALUES (...)` é atômica e idempotente.
- `INSERT OR REPLACE INTO chunk_embeddings(chunk_id, memory_id, embedding, source, model, dim)` é o novo caminho canônico de escrita, com todos os metadados em uma linha.
- A coluna `source` permite ao operador inspecionar o corpus em busca de desvio de versão de LLM (ex. "todos os embeddings antes de 2026-05-01 usaram claude-sonnet-4-5, todos depois usaram claude-sonnet-4-6").

### Negativas

- A busca KNN é O(N × D) por chamada (ver ADR-0020). Para tamanhos de namespace acima de 100k memórias, o operador deve particionar por namespace ou por data e confiar no FTS5 para filtragem grossa.
- Bancos de dados v1.0.74 existentes perdem seus embeddings de tabela vec na migração. O re-embedding é lazy (o próximo `remember` / `edit` / `ingest` re-embeda a memória afetada), mas operadores com milhões de memórias pré-existentes devem planejar um re-ingest em lote ou usar a feature `embedding-legacy` durante a janela de transição.

## Verificação

- `cargo test --lib`: 711 testes passam.
- `cargo test --lib storage::memories::tests::upsert_vec_and_delete_vec_work`: verde — a nova tabela `memory_embeddings` aceita upsert e delete corretamente.
- `cargo test --lib storage::entities::tests::upsert_entity_vec_replaces`: verde — a nova tabela `entity_embeddings` se comporta da mesma forma.
- `cargo test --lib storage::chunks::tests::test_upsert_chunk_vec_and_knn_search`: verde — a nova tabela `chunk_embeddings` faz round-trip.
