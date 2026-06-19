# ADR-0019: Arquitetura LLM-Only One-Shot (v1.0.76)

- Status: Aceito (2026-06-07)
- Atualização (v1.0.79): a válvula de escape `embedding-legacy` mencionada abaixo foi removida antecipando o cronograma da v1.1.0; a janela de transição está fechada
- Decisores: Danilo Aguiar
- Escopo: src/embedder.rs, src/extraction.rs, src/similarity.rs, src/storage/connection.rs, src/storage/memories.rs, src/storage/entities.rs, src/storage/chunks.rs, migrations/V002, migrations/V013, Cargo.toml

## Contexto

O build padrão da v1.0.74 empacotava cinco dependências pesadas de modelo e extensão:

- `fastembed` 5.13.4 (text-embedding + runtime ONNX)
- `ort` 2.0.0-rc.12 (runtime ONNX)
- `ndarray` 0.16 (biblioteca de tensores)
- `tokenizers` 0.22 (tokenizer Hugging Face)
- `huggingface-hub` 0.4 (download de modelos)
- `sqlite-vec` 0.1.9 (extensão de tabela virtual vec0)

Essas dependências produziam um binário release de 39 MB, exigiam o download de um modelo de 1.1 GB no primeiro uso (ou 349 MB com int8) e travavam a CLI em um único modelo de embedding (`multilingual-e5-small`). O download bloqueava jobs de CI e tornava `cargo install` pesado.

Além disso, o modo `daemon` mantinha o modelo em memória entre invocações da CLI para amortizar o custo de carga. Esse design era frágil: o daemon podia ser morto no meio de uma requisição, o modelo podia falhar ao carregar em um host específico, e a separação entre daemon e CLI adicionava um protocolo stateful que dificultava a depuração.

## Decisão

A v1.0.76 remove todas essas dependências. O build padrão é **LLM-only e one-shot**:

- Geração de embedding: `claude code` (OAuth Anthropic) ou `codex` (OAuth ChatGPT Pro da OpenAI), spawnado por chamada, encerrado quando a resposta JSON é parseada. Sem daemon. Sem runtime ONNX.
- NER: o `LlmBackend` em `src/extract/llm_backend.rs` extrai entidades e relacionamentos via tool-use JSON. O build padrão é apenas regex de URL; o NER via LLM roda sob demanda quando o operador usa `--extraction-backend llm`.
- Busca vetorial: similaridade por cosseno é computada em Rust puro sobre os embeddings BLOB armazenados em `memory_embeddings`, `entity_embeddings` e `chunk_embeddings` (ver migration V013). A extensão `sqlite-vec` se foi.

A CLI é, portanto, um orquestrador fino que:

1. Armazena memórias em SQLite + FTS5.
2. Chama o LLM headless (claude / codex) para embedding e extração.
3. Usa FTS5 para busca por correspondência exata; usa cosseno em Rust puro para similaridade.

As flags de endurecimento do LLM da v1.0.69 são herdadas sem mudanças: 7 flags para claude (`--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions --output-schema ...`), 7 para codex. OAuth é o único fluxo de credencial aceito; `ANTHROPIC_API_KEY` e `OPENAI_API_KEY` no ambiente fazem o spawn ABORTAR com `AppError::Validation`.

## Consequências

### Positivas

- O binário release cai de 39 MB para ~14.6 MiB (apenas rustc + rusqlite + clap).
- `cargo install sqlite-graphrag` não exige mais ferramentas de build C, runtime ONNX, nem biblioteca de sistema além de um compilador C.
- O custo de cold-start do primeiro `remember` é dominado pelo spawn do subprocesso LLM (~1-3 s) em vez da carga do modelo ONNX (~30 s em cache fria).
- A CLI agora é one-shot. Não há daemon para vazar memória, nem socket para deixar para trás em crash, nem estado para inspecionar com `daemon --ping`.
- Operadores com uma das CLIs LLM suportadas (`claude` ou `codex`) ganham embedding + NER funcionais sem nenhum download de modelo.

### Negativas

- Cada chamada de embedding agora incorre em spawn de subprocesso LLM (overhead de 1-3 s). Operadores que agrupam muitas chamadas `remember` devem usar o batching do lado do LLM (um prompt com N passagens) — o helper `embed_passages_controlled` já agrupa chunks para isso.
- O ambiente de teste de CI precisa ter uma CLI LLM no PATH para exercitar os caminhos de embedding + NER. CI sem LLM é documentado a falhar `v1044_features` / `signal_handling_integration` / `v2_breaking_integration` com `embedding failed: no LLM CLI found on PATH`.
- O build padrão não tem mais fallback local para usuários que não podem ou não querem instalar `claude` ou `codex`. A feature `embedding-legacy` restaura o pipeline fastembed para a janela de transição v1.0.76 → v1.1.0; ela é removida na v1.1.0.

## Migração da v1.0.74 / v1.0.75

Bancos de dados existentes perdem seus embeddings de tabela vec quando a migration V013 roda. A nova tabela `memory_embeddings` fica vazia após a migração; o próximo `remember`, `edit` ou `ingest` re-embeda a memória via LLM. Operadores com milhões de memórias pré-existentes que querem evitar o pico de re-embedding na primeira chamada podem:

1. Rodar `migrate --to-llm-only --keep-vec` (um subcommand futuro) para despejar as tabelas vec em JSON.
2. Após o upgrade do binário, rodar um `ingest --namespace *` único para re-embedar tudo via LLM.

A feature `embedding-legacy` é a válvula de escape para operadores que querem manter o pipeline de modelo da v1.0.74 durante a janela de transição:

```
cargo install sqlite-graphrag --features embedding-legacy --version 1.0.76
```

Isso é removido na v1.1.0.
