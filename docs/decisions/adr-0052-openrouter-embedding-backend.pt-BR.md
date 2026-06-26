# ADR-0052: Backend de Embedding OpenRouter

- Status: ACEITO
- Data: 2026-06-25
- Substitui: nenhum
- Relacionado: ADR-0010 (build LLM-only), ADR-0039 (semáforo de slots), ADR-0041 (env de custom provider)

## Contexto

Antes da v1.0.93, cada chamada de embedding spawnava um subprocesso headless — `codex exec`, `claude -p` ou `opencode` — para calcular o vetor de embedding. O cold-start de subprocesso em um token OAuth recém-emitido ultrapassa rotineiramente 15 segundos por chamada. Para comandos que embeddingam muitos chunks (por exemplo, `ingest`, `enrich --operation re-embed`), o overhead acumulado de subprocesso dominava o tempo de wall-clock.

O OpenRouter expõe uma API REST em `https://openrouter.ai/api/v1/embeddings` que aceita um corpo de requisição compatível com OpenAI e retorna um vetor float. Um único round-trip HTTP ao OpenRouter conclui em cerca de 200ms, eliminando completamente a penalidade de subprocesso.

O desafio era adicionar esse caminho REST sem confundir com o enum `LlmBackendChoice` existente (que governa backends de geração de texto, não backends de embedding) e sem quebrar o enforcement OAuth-only que impede que `ANTHROPIC_API_KEY` e `OPENAI_API_KEY` vazem para ambientes de subprocesso.

## Decisão

Introduzir um enum `EmbeddingBackendChoice` separado em `src/embed/backend.rs` com variantes:

- `Codex` — caminho de subprocesso codex existente (padrão)
- `Claude` — caminho de subprocesso claude existente
- `OpenCode` — caminho de subprocesso opencode existente
- `OpenRouter` — novo caminho de API REST (este ADR)
- `None` — embedding nulo (caminho de skip-on-failure)

A variante OpenRouter é implementada via `reqwest` com `rustls-tls` (sem dependência de TLS nativo). O cliente HTTP envia um POST para `https://openrouter.ai/api/v1/embeddings` com o nome do modelo e o texto de entrada, recebe um array float e trunca para 64 dimensões usando truncamento MRL (Matryoshka Representation Learning) — o mesmo alvo de 64 dimensões usado por todos os outros backends de embedding.

Três novas flags CLI são adicionadas a cada subcomando que aceita `--embedding-backend`:

- `--embedding-backend openrouter` — seleciona o caminho REST
- `--embedding-model <MODEL>` — seleciona o modelo de embedding (OBRIGATÓRIO; sem modelo padrão — o usuário DEVE especificar)
- `--openrouter-api-key <KEY>` — chave de API para o OpenRouter (NÃO encaminhada para subprocessos; armazenada apenas no cliente reqwest)

A flag `--openrouter-api-key` NUNCA é escrita em logs, NUNCA é ecoada em mensagens de erro, e NUNCA é passada via variáveis de ambiente. É consumida exclusivamente pelo cliente reqwest in-process e descartada após a conclusão da chamada de embedding.

As flags `--llm-backend` existentes e o enum `LlmBackendChoice` permanecem INALTERADOS. O OpenRouter como backend de embedding não afeta a geração de texto.

## Justificativa do Truncamento MRL

Os modelos OpenRouter retornam vetores de comprimento variável (por exemplo, `text-embedding-3-small` retorna 1536 dimensões por padrão). O sqlite-graphrag armazena todos os embeddings em uma coluna `BLOB` de largura fixa dimensionada para 64 valores float32 (256 bytes). O truncamento MRL para as primeiras 64 dimensões preserva os componentes de maior informação do embedding enquanto mantém o schema inalterado. O mesmo truncamento é aplicado pelos caminhos de subprocesso codex e claude.

## Consequências

### Positivas
- A latência de embedding cai de ~15s (cold-start de subprocesso) para ~200ms (round-trip HTTP) para modelos OpenRouter.
- Sem novas dependências nativas — `reqwest` + `rustls-tls` já está presente na árvore de dependências; adicioná-lo aqui não adiciona novas unidades de compilação.
- A separação de `EmbeddingBackendChoice` garante que operadores que usam OpenRouter para embeddings ainda possam usar codex ou claude para geração de texto sem interferência.
- A chave de API do OpenRouter nunca é exposta a subprocessos, preservando o modelo de segurança de subprocesso OAuth-only.

### Negativas
- O embedding OpenRouter requer uma chave de API paga (`OPENROUTER_API_KEY`). Os backends de subprocesso (codex, claude, opencode) são OAuth-only e não requerem credenciais pagas.
- Os testes E2E em `tests/openrouter_embedding.rs` requerem que `OPENROUTER_API_KEY` esteja definida. Esses testes são excluídos do profile `ci` do nextest e devem ser executados manualmente ou em um job de CI separado com o segredo injetado.
- O truncamento MRL para 64 dimensões significa que a fidelidade semântica é menor do que o embedding em resolução total. Esse tradeoff é aceito porque todos os embeddings existentes no banco de dados usam o mesmo truncamento de 64 dimensões, e a similaridade coseno entre backends só é válida quando todos os vetores compartilham a mesma dimensão e estratégia de truncamento.

## Alternativas Consideradas

### Embedding via subprocesso com credenciais OpenRouter
Passar `OPENROUTER_API_KEY` como variável de ambiente para um processo filho foi rejeitado porque viola o modelo de segurança de subprocesso OAuth-only. Qualquer subprocesso poderia ler e exfiltrar a chave.

### Estender `LlmBackendChoice` com variante `OpenRouter`
Rejeitado porque `LlmBackendChoice` governa backends de geração de texto. Mesclar a seleção de embedding no mesmo enum tornaria impossível configurar independentemente o backend de geração de texto e o backend de embedding — uma necessidade comum de operadores (por exemplo, usar codex para geração, OpenRouter para embedding rápido).

### Armazenar vetores em resolução total e redimensionar o schema
Rejeitado porque requer uma migração de schema que invalida todos os embeddings existentes e altera a largura da coluna `BLOB`. O schema fixo de 64 dimensões é um contrato estável; alterá-lo quebraria a compatibilidade retroativa para todos os bancos de dados criados antes da v1.0.93.

## Cobertura de Testes

- `tests/openrouter_embedding.rs` — testes de API ao vivo; excluídos do profile `ci` do nextest; requerem `OPENROUTER_API_KEY`
- Testes unitários de `src/embed/backend.rs` — verificam parsing, display e cadeia de fallback de `EmbeddingBackendChoice`
- Testes unitários de `src/embed/openrouter.rs` — verificam truncamento MRL, tratamento de erros e mascaramento da chave de API em logs
- Scripts Mock LLM em `tests/mock-llm/` NÃO são estendidos para OpenRouter (o caminho REST não é um subprocesso)

## Correções Pós-Release (v1.0.93 — GAP-OR-PROPAGATION)

Após a implementação inicial propagar `EmbeddingBackendChoice` para 8 comandos, foram descobertos 5 caminhos de embedding adicionais que ainda chamavam a função antiga `embed_passage_with_choice()`, ignorando silenciosamente `--embedding-backend openrouter`:

1. `enrich.rs` — `reembed_memory_vector()` chamava a função antiga; corrigido para usar `embed_passage_with_embedding_choice()`
2. `init.rs` — sonda de dimensão chamava `embed_passage_with_choice(..., None)`; corrigido para propagar ambos os backends
3. `rename_entity.rs` — re-embedding de entidade chamava a função antiga; corrigido
4. `ingest_claude.rs` — 4 call sites com backend de embedding `None`; todos corrigidos para propagar `embedding_backend`
5. `remember.rs` — embedding paralelo de chunks chamava `embed_passages_parallel_local()`; corrigido para usar `embed_passages_parallel_with_embedding_choice()`

Total de caminhos de embedding após a correção: 13 (8 originais + 5 corrigidos no GAP-OR-PROPAGATION).

### BUG-OR-EXIT-CODE

Três pontos de validação de configuração OpenRouter em `main.rs` emitiam exit code 1 em vez de 78 (`EX_CONFIG`). Corrigido para usar `ExitCode::from(78_u8)` e `emit_error_json(78, msg)`, consistente com a convenção BSD sysexits usada em todo o projeto.

### Ranking de Recall Score E2E (dim=64 MRL)

Todos os 10 modelos validados end-to-end com `--embedding-dim 64`:

| Modelo | Recall Score |
|---|---|
| google/gemini-embedding-001 | 0.892 |
| google/gemini-embedding-2 | 0.868 |
| mistralai/mistral-embed-2312 | 0.832 |
| qwen/qwen3-embedding-8b | 0.814 |
| qwen/qwen3-embedding-4b | 0.754 |
| openai/text-embedding-3-small | 0.668 |
| nvidia/llama-nemotron-embed-vl-1b-v2:free | 0.662 |
| baai/bge-m3 | 0.537 |
| openai/text-embedding-3-large | 0.449 |
| perplexity/pplx-embed-v1-0.6b | 0.415 |

Principais conclusões:
- TODOS os 10 modelos aceitam `dimensions: 64` nativamente via MRL — sem necessidade de truncamento do lado Rust
- OpenAI large (0.449) tem desempenho PIOR que small (0.668) em dim=64 — embeddings de alta dimensão (3072) perdem mais informação ao serem truncados para 64 dimensões
- Google Gemini 001 e Mistral são as melhores escolhas para busca semântica nessa dimensionalidade reduzida
