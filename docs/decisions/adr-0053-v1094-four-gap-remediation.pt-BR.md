# ADR-0053 — Remediação de Quatro Gaps da v1.0.94

**Status**: Aceito
**Data**: 2026-06-26
**Contexto**: sqlite-graphrag v1.0.94 — GAP-OR-ENTITY-EMBED, GAP-EMBED-DIM-64, GAP-EMBED-TIMEOUT-300, GAP-HEADLESS-DEFAULT

## Problema

A v1.0.93 entregou o backend de embedding via OpenRouter REST (ADR-0052)
mas deixou quatro gaps abertos, documentados em `gaps.md`. Eles
compartilhavam um tema comum: o caminho de embedding de entidades e
vários defaults ainda estavam calibrados para a era do subprocesso codex
legado, não para o padrão OpenRouter REST.

1. **GAP-OR-ENTITY-EMBED** — O embedding de entidades em `remember`,
   `remember-batch` e `ingest` ignorava `--embedding-backend` e
   `--llm-backend`, chamando o embedder codex diretamente. Um `remember`
   com entidades novas esperava o subprocesso codex até o timeout interno
   (~119s), mesmo quando o usuário pedia OpenRouter.

2. **GAP-EMBED-DIM-64** — `DEFAULT_EMBEDDING_DIM` era 64
   (`src/constants.rs`), enquanto o corpus de produção está indexado em
   384. Pior: `src/main.rs` congelava a dimensão do cliente OpenRouter
   com um `unwrap_or(64)` cravado no startup eager — antes de o banco
   abrir — então nem a env var nem o `schema_meta.dim` do banco podiam
   corrigir. Toda operação sem `--embedding-dim 384` explícito gerava
   vetores de 64 que colidiam com o índice de 384 e abortavam o KNN com
   exit 11.

3. **GAP-EMBED-TIMEOUT-300** — `DEFAULT_EMBED_TIMEOUT_SECS` era 120s
   (`src/extract/llm_embedding.rs`), o único subprocesso LLM deixado para
   trás quando `ingest`, `enrich` e `opencode` adotaram 300s.

4. **GAP-HEADLESS-DEFAULT** — `enrich --mode` tinha default
   `claude-code` (`src/commands/enrich.rs`). Omitir `--mode` spawnava
   silenciosamente `claude -p`, que herda o `.mcp.json` do projeto do
   chamador e falha em contextos headless.

## Decisão

Aplicar quatro correções cirúrgicas na v1.0.94.

### FIX-1: Embedding de entidades honra os backends selecionados

`embed_entity_texts_cached` em `src/embedder.rs` agora recebe
`embedding_backend: EmbeddingBackendChoice` e
`llm_backend: LlmBackendChoice`. Os cache misses são roteados por
`embed_passages_parallel_with_embedding_choice` (OpenRouter REST quando
a chain resolvida começa com OpenRouter, LLM local caso contrário) em
vez do `embed_texts_parallel` exclusivo do codex. Um curto-circuito de
chain `none` retorna vetores vazios SEM spawnar subprocesso. A chave de
cache de entidade ficou backend-aware (`openrouter:{dim}`) para que
vetores codex e OpenRouter nunca colidam. Chamadores atualizados:
`remember.rs`, `remember_batch.rs`, `ingest.rs`. `remember` com
entidades novas cai de ~119s para ~0,9s sob OpenRouter.

### FIX-2: Dimensão de embedding padrão elevada de 64 para 384

`DEFAULT_EMBEDDING_DIM` mudou para 384 em `src/constants.rs`, e
`src/main.rs` agora chama `constants::embedding_dim()` (env > ATIVA >
default) em vez do `unwrap_or(64)` cravado. Bancos novos via `init`
gravam `dim=384` no `schema_meta`, casando com o corpus de produção.
Bancos legados em 64 são preservados via precedência `schema_meta.dim`
— sem re-embed forçado. O default 64 foi escolha deliberada do
G42/v1.0.79 para cortar custo de token autoregressivo no caminho de
embedding via codex; é irrelevante agora que o OpenRouter REST é o
padrão operacional, onde o truncamento MRL ocorre no servidor a custo
zero de token.

### FIX-3: Timeout do subprocesso de embedding elevado de 120s para 300s

`DEFAULT_EMBED_TIMEOUT_SECS` mudou para 300 em
`src/extract/llm_embedding.rs`, alinhando o subprocesso de embedding com
`ingest`/`enrich`/`opencode`. O override por env
`SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` e o clamp `[10, 3600]` permanecem.

### FIX-4: `enrich --mode` agora é obrigatório

Removido `default_value = "claude-code"` do argumento `mode` em
`src/commands/enrich.rs`; o campo continua `EnrichMode` (não `Option`),
então o clap torna `--mode` obrigatório. Omitir é rejeitado com exit 2,
evitando spawn acidental de `claude -p`. Valores válidos: `claude-code`,
`codex`, `opencode`.

## Alternativas Consideradas

### A. Adicionar uma flag nova `--entity-embedding-backend` (GAP-OR-ENTITY-EMBED)

Rejeitada (YAGNI). As flags `--embedding-backend`/`--llm-backend`
existentes já expressam a intenção; o caminho de entidades só precisa
honrá-las reusando `embed_passages_parallel_with_embedding_choice`.

### B. Reordenar o init eager do OpenRouter para após abrir o banco (GAP-EMBED-DIM-64)

Adiada. Elevar o default para 384 resolve o caso comum a risco zero.
Reordenar o startup para bancos legados não-384 é melhoria futura;
usuários com esses bancos ainda passam `--embedding-dim`.

### C. Tornar `enrich --mode` um `Option` com erro manual quando ausente

Rejeitada. O comportamento de argumento obrigatório do clap (sem
`default_value` num campo não-`Option`) já produz exit 2 com mensagem
clara, espelhando o argumento `operation` obrigatório existente — sem
caminho de erro customizado.

## Consequências

- `remember`/`remember-batch`/`ingest` embedam entidades pelo backend
  selecionado; escritas com entidades novas terminam em tempo
  sub-segundo sob OpenRouter.
- `recall`, `hybrid-search`, `deep-research`, `remember` e `ingest`
  funcionam sem `--embedding-dim 384` explícito em bancos novos de 384;
  o mismatch de dimensão exit 11 sai do fluxo padrão.
- O subprocesso de embedding não aborta mais cedo sob cold start ou
  lotes grandes.
- `enrich` não pode mais spawnar `claude -p` silenciosamente; o modo é
  uma escolha explícita e auditável.
- Mudança quebrante para scripts: toda invocação de `enrich` agora DEVE
  passar `--mode`. Pareamento canônico com `--llm-backend`: `codex` ->
  `codex`, `claude` -> `claude-code`, `opencode` -> `opencode`.

## Validação

- Build: `cargo build --release` 0 erros; `cargo clippy -- -D warnings`
  0 warnings; `cargo fmt --check` 0 diferenças.
- Suíte de testes: `cargo test` exit 0; testes de regressão renomeados
  (`init_default_dim_is_384`, `embed_timeout_default_is_300`) e um teste
  de contrato afirmando que `enrich` sem `--mode` é rejeitado (clap exit
  2).
- E2E: `init` grava `dim=384`; `remember` + entidade nova via OpenRouter
  = 913ms com `backend_invoked=openrouter`; `enrich` rejeita `--mode`
  ausente.

## Cross-references

- `gaps.md` — os quatro gaps marcados como RESOLVIDO em v1.0.94
- ADR-0052 (backend de embedding OpenRouter) — o predecessor da v1.0.93
- ADR-0050 (remediação de deadlock de embedding) — trabalho anterior de
  timeout/flags
- `src/constants.rs` (DEFAULT_EMBEDDING_DIM), `src/main.rs` (init eager),
  `src/extract/llm_embedding.rs` (DEFAULT_EMBED_TIMEOUT_SECS),
  `src/commands/enrich.rs` (argumento mode), `src/embedder.rs`
  (`embed_entity_texts_cached`)
