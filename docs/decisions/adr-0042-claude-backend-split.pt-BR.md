# ADR-0042: Split Real do Entry Point Claude no Embedder

- **Status**: Aceito
- **Data**: 2026-06-17
- **Versão**: v1.0.84 (resolve GAP-002)
- **Autores**: tech-lead

## Contexto

A flag CLI `--llm-backend claude` é aceita pelo parser (`src/cli.rs:149-150`) e propagada como `cli.llm_backend: LlmBackendChoice` para seis comandos em `src/main.rs:310-379`. O método `LlmBackendChoice::to_chain()` em `src/cli.rs:36-59` traduz `Claude` em `vec![LlmBackendKind::Claude, LlmBackendKind::None]`. A cadeia é iterada por `embed_with_fallback` em `src/embedder.rs:368-409`, que chama `embed_via_backend` em `src/embedder.rs:427-446`. **Causa raiz**: o braço `LlmBackendKind::Claude` no match (linhas 435-444) delega para `embed_passage_local` (linhas 177-181), que invoca `get_embedder` (linhas 128-135), que usa `LlmEmbedding::detect_available` (linhas 184-207) — esta função executa um PATH-probe **preferindo `codex` PRIMEIRO** (linha 187) e só cai em `claude` quando `codex` está ausente. O comentário no código (linhas 441-443) documenta explicitamente "future v1.0.83 will split the entry points", mas o split nunca foi implementado.

### Matriz da causa raiz

| Fator | Local | Intervalo de linhas |
|---|---|---|
| Braço `LlmBackendKind::Claude` delega para `embed_passage_local` | `src/embedder.rs` | 435-444 |
| `embed_passage_local` re-executa o PATH-probe | `src/embedder.rs` | 177-181 |
| `detect_available` prefere `codex` sobre `claude` | `src/extract/llm_embedding.rs` | 184-207 |
| `with_claude` existe mas nunca é invocado pela cadeia | `src/extract/llm_embedding.rs` | 221-231 |
| Comentário obsoleto "future v1.0.83 will split" | `src/embedder.rs` | 441-443 |

### Impacto em produção (2026-06-17)

Quando o operador passa `--llm-backend claude` esperando um bypass determinístico de `codex`:

1. `embed_with_fallback` alcança o braço `LlmBackendKind::Claude`
2. O braço delega para `embed_passage_local` em vez de um caminho exclusivo Claude
3. `get_embedder` invoca `LlmEmbedding::detect_available`
4. O PATH-probe prefere `codex` porque aparece primeiro no PATH
5. Quota OAuth do `codex` esgotada → exit 11 (`AppError::Embedding`)
6. `remember`/`edit` abortam antes da persistência no SQLite completar
7. Linha parcial de memória permanece em `memories` sem vetor em `memory_embeddings`
8. Entrada órfã cresce em `pending_embeddings` a cada retentativa
9. `recall` e `hybrid-search` perdem precisão semântica

### Cross-references

- `gap-g58-recall-sem-fallback-deterministic-2026-06-13` — recall e hybrid-search sob fadiga OAuth
- `incident-codex-oauth-refresh-token-reused-2026-06-14` — cadeia de 401 do refresh-token
- ADR-0038 — codex como backend padrão desde a v1.0.76
- ADR-0041 — preservação do env de provider customizado (ADR-0042 é o fix simétrico para o entry point Claude)

## Decisão

Dividir o entry point Claude no embedder de forma que `--llm-backend claude` invoque `claude` e nunca `codex`. O split tem quatro peças concretas.

### 1. Novo builder `LlmEmbeddingBuilder` em `src/extract/llm_embedding.rs`

Expor construtores `with_claude_builder()` e `with_codex_builder()` que retornam um builder com setters `override_binary(PathBuf)` e `override_model(String)`. Os construtores existentes `with_claude` e `with_codex` se tornam wrappers finos que chamam `.build()` no builder, eliminando duplicação.

### 2. Novos `get_claude_embedder` e `embed_via_claude_local` em `src/embedder.rs`

`get_claude_embedder` armazena em cache um `OnceLock<Mutex<LlmEmbedding>>` construído somente via `LlmEmbedding::with_claude_builder()`. A cache nunca toca em `detect_available`, então codex não pode entrar no caminho de resolução.

`embed_via_claude_local` é a função pública chamada pelo novo braço do match. Ela adquire o slot LLM, chama `get_claude_embedder` e executa `embed_passage`. Honra os overrides `claude_binary` e `claude_model` vindos das flags CLI.

### 3. Troca do braço do match em `embed_via_backend` em `src/embedder.rs:435-444`

Substituir a delegação para `embed_passage_local` por uma chamada direta a `embed_via_claude_local`. Remover o comentário obsoleto "synonym for codex". O braço agora emite um evento `tracing::debug!` com `backend = "claude"` para que operadores possam confirmar o fix em logs de produção.

### 4. Observabilidade via `backend_invoked` em sete envelopes

Adicionar o campo `backend_invoked: enum [claude, codex, none]` em sete envelopes de resposta: `embedding status`, `remember`, `edit`, `ingest` (summary), `recall`, `hybrid-search`, `enrich` (summary). O campo é omitido quando a operação não invocou nenhum backend.

Para `recall` e `hybrid-search`, adicionar também `vec_degraded_reason: enum [embedding_failed, cancelled, timeout]` para que consumidores consigam desambiguar por que o embedding ao vivo caiu para FTS5.

### 5. Nova flag global `--dry-run-backend`

Resolve e imprime o backend que seria invocado (caminho do binário, modelo, flavour, modo de env-clear) sem spawnar o subprocesso. Retorna exit 0. Honra a env var `SQLITE_GRAPHRAG_DRY_RUN_BACKEND=1`.

### Helpers de serialização

- `LlmBackendKind::as_str(self) -> &'static str` retorna `"claude"`, `"codex"` ou `"none"`
- `FallbackReason::reason_code(&self) -> &'static str` retorna `"embedding_failed"`, `"cancelled"` ou `"timeout"`

## Consequências

### Positivas

- `--llm-backend claude` cumpre a promessa de UX: o binário `claude` é invocado, nunca `codex`
- Exaustão de quota OAuth do codex deixa de bloquear sessões que optam explicitamente por Claude
- Campo `backend_invoked` dá observabilidade por chamada a operadores e pipelines de CI
- `--dry-run-backend` habilita auditoria pré-voo antes de ingestões longas
- `vec_degraded_reason` substitui o `vec_error` livre por um enum, permitindo alertas estruturados
- 5 novos testes de regressão em `tests/embedder.rs` travam o contrato

### Negativas

- `embed_passage_with_choice` muda a assinatura de `Vec<f32>` para `(Vec<f32>, LlmBackendKind)` — patch-aditivo conforme política de API de biblioteca, seis call sites atualizados atomicamente
- Sete schemas JSON atualizados; consumidores devem tolerar os novos campos opcionais
- ADR-0042 introduz dependência conceitual com ADR-0034 (SHUTDOWN), ADR-0037 (rename de locale), ADR-0038 (padrão de backend) e ADR-0041 (env customizado)

### SEM telemetria nova

O fix é silencioso. Nenhum `tracing::info!` é adicionado para uso de provider customizado. O único novo evento de log é o `tracing::debug!` dentro do novo braço Claude, gateado pelo nível de log padrão. Esta é uma decisão deliberada para minimizar a superfície de observabilidade em caminhos sensíveis.

## Alternativas Consideradas

1. **Manter o atalho "synonym for codex"** — rejeitado, o próprio comentário documenta isto como um fix futuro e o bypass é exatamente o defeito do GAP-002
2. **Reverter `detect_available` para preferir `claude`** — rejeitado, quebra a ordem de resolução `Auto` codificada por ADR-0038
3. **Adicionar uma flag separada `--force-claude`** — rejeitado, cria duas formas de pedir a mesma coisa e confunde operadores
4. **Documentar workaround via `claude -p` headless externo** — rejeitado, transfere o ônus operacional para usuários e quebra o contrato determinístico da CLI

## Referências

- `src/embedder.rs:435-444` — braço `LlmBackendKind::Claude`, agora chama `embed_via_claude_local`
- `src/embedder.rs:177-181` — `embed_passage_local` (não mais alcançado pelo backend Claude)
- `src/embedder.rs:128-135` — `get_embedder` (não mais alcançado pelo backend Claude)
- `src/extract/llm_embedding.rs:184-207` — `detect_available` com PATH-probe codex-first
- `src/extract/llm_embedding.rs:221-231` — `with_claude` refatorado via `LlmEmbeddingBuilder`
- `src/spawn/env_whitelist.rs` — `apply_env_whitelist_for_claude` compartilhado por `invoke_claude` e `embed_via_claude_local`
- ADR-0038 — codex como backend padrão desde a v1.0.76
- ADR-0041 — preservação de `ANTHROPIC_AUTH_TOKEN` e env vars de provider customizado
- `gap-g58-recall-sem-fallback-deterministic-2026-06-13` — fadiga OAuth no caminho de leitura
- `incident-codex-oauth-refresh-token-reused-2026-06-14` — cadeia de 401 do refresh-token

## Decisões Relacionadas

- **ADR-0034 (v1.0.80)** — resiliência de SHUTDOWN; GAP-002 herda a superfície do incidente A1
- **ADR-0037 (v1.0.81)** — rename de locale; ortogonal ao split de backend
- **ADR-0038 (v1.0.76)** — codex como backend padrão; ADR-0042 é o split simétrico do entry point Claude
- **ADR-0041 (v1.0.83)** — preservação de env de provider customizado; habilita Claude OAuth em endpoints não-Anthropic dos quais sessões de GAP-002 dependem
