# Gaps — Lacunas Arquiteturais Conhecidas da CLI sqlite-graphrag

## Índice de Gaps

| Gap | Versão | Status | ADR |
|---|---|---|---|
| GAP-002 | v1.0.84 | Solucionado | ADR-0042 |
| GAP-003 | v1.0.85 | Solucionado | ADR-0043 |
| GAP-004 | v1.0.85.1 | Solucionado | ADR-0043 (hotfix) |
| G58 | v1.0.85 | Solucionado | ADR-0043 |
| G45-CR5 | v1.0.85 | Solucionado | ADR-0043 |
| G55 | v1.0.80 (cross-ref v1.0.85) | Solucionado | ADR-0043 docs |
| G56 | v1.0.79 (cross-ref v1.0.85) | Solucionado | ADR-0022 / ADR-0043 docs |
| BUG-001 | v1.0.85.2 | Solucionado | ADR-0044 |
| BUG-002 | v1.0.85.2 | Solucionado | ADR-0044 |
| BUG-003 | v1.0.85.2 | Solucionado | ADR-0044 |

> NOTA: GAP-004, BUG-001, BUG-002 e BUG-003 foram descobertos em
> auditoria pós-release e resolvidos em v1.0.85.1/v1.0.85.2 (ADR-0044).
> O plano `idempotent-napping-sloth.md` lista apenas 5 gaps para v1.0.85
> (GAP-003, G58, G45-CR5, G55, G56); entradas pós-release ficam fora do
> escopo do plano original.

## GAP-002 — Flag `--llm-backend claude` Ignorada em Run Time: codex Invocado Mesmo Quando claude é Escolhido Explicitamente (v1.0.83, descoberto em produção em 2026-06-17) (Solucionado em v1.0.84 / ADR-0042)

### O Problema

- A CLI aceita o flag `--llm-backend claude` sem erro de parsing
- A flag é propagada até a função `embed_via_backend` em `src/embedder.rs:427`
- Dentro da função o ramo `LlmBackendKind::Claude` é tratado como sinônimo de `Codex`
- O comentário em `src/embedder.rs:441-443` documenta explicitamente o atalho
- O atalho diz "a future v1.0.83 will split the entry points" mas o split nunca aconteceu
- codex OAuth atinge usage limit em 2026-06-17 e o binário continua invocando codex
- O operador passa `--llm-backend claude` esperando bypass mas o bypass não existe
- A sessão inteira termina sem persistência porque nenhum backend de fato executa claude

### Consequências do Problema

- Operador fica sem memória persistida do incidente em curso
- Hook `Stop` não tem canal funcional para gravar achados proativos
- `pending list` confirma fila vazia porque corpo nunca chega ao checkpoint
- `recall` KNN puro retorna erro de dimensão zero quando strict-env-clear ativa
- `hybrid-search` falha com envelope `cannot index 2026 with results` por causa da query quebrada
- Codex OAuth atinge usage limit diária e a janela só libera em 2026-06-18 00:06
- Toda sessão do dia herda a falha porque o flag não cumpre a promessa de isolamento
- Workaround externo via `claude -p` headless direto vira dependência operacional
- Workaround precisa manter arquivo Markdown do disco como backup secundário
- Operador descobre o bug só no momento da falha porque o `--help` promete o isolamento

### Causa Raiz do Problema

- Função `embed_via_backend` em `src/embedder.rs:434-443` mapeia ambas as variantes para `embed_passage_local`
- O `embed_passage_local` em `src/embedder.rs:177-181` chama `get_embedder` que usa `LlmEmbedding::detect_available`
- O `detect_available` em `src/extract/llm_embedding.rs` segue ADR-0038 com codex como prioridade absoluta
- A função `embed_passage_with_choice` em `src/embedder.rs:205-218` traduz `LlmBackendChoice::Claude` em `vec![LlmBackendKind::Claude]`
- O chain `[claude]` cai no loop de `embed_with_fallback` que chama `embed_via_backend` por entrada
- O ramo `Claude` do match em `embed_via_backend` delega para o mesmo `embed_passage_local` do ramo `Codex`
- A delegação ignora o chain inteiro porque é um mapeamento 1-para-1 com codex
- A factory `AutoFactory` em `src/extract/llm_backend.rs:333-365` ainda prioriza codex quando ambos estão no PATH
- O guard OAuth-only em `src/spawn/env_whitelist.rs` rejeita `ANTHROPIC_API_KEY` mas preserva `ANTHROPIC_AUTH_TOKEN`
- O flag `--strict-env-clear` em `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` zera o env mesmo para provider customizado
- ADR-0041 preserva credenciais de provider customizado mas não cria caminho para `claude` puro

### A Solução

- S1 — Split real da função `embed_via_backend` em `src/embedder.rs:427-446` em dois entry points distintos
- S1a — Criar `embed_via_claude_local` que invoca `claude -p` headless sem passar por `LlmEmbedding::detect_available`
- S1b — Manter `embed_via_codex_local` que preserva o caminho atual via `embed_passage_local`
- S1c — Trocar o ramo `LlmBackendKind::Claude` para chamar o novo entry point dedicado
- S2 — Adicionar campo `backend_invoked: "codex" | "claude" | "none"` no envelope JSON de `embedding status`
- S3 — Emitir `tracing::warn!` quando o backend resolvido diverge do backend pedido pelo usuário
- S4 — Adicionar teste de regressão `embed_via_backend_claude_does_not_invoke_codex` em `tests/embedder.rs`
- S5 — Atualizar `src/extract/llm_backend.rs:435-440` removendo o comentário "treated as synonym for codex"
- S6 — Adicionar flag `--dry-run-backend` que retorna o binary path que seria invocado sem executar
- S7 — Documentar em ADR-0042 o split arquitetural com cross-reference para ADR-0038 e ADR-0041
- S8 — Adicionar campo `vec_degraded_reason: "backend_mismatch" | "oauth_quota" | "dim_zero" | null` em `hybrid-search`

### Benefícios da Solução

- Operador ganha controle real sobre qual backend é invocado em qualquer momento
- codex OAuth usage limit deixa de bloquear sessões que pedem claude explicitamente
- Diagnóstico de embedding failure fica transparente via `backend_invoked` no envelope
- Workaround externo via `claude -p` headless deixa de ser necessário em ambiente com flag funcional
- Teste de regressão impede que o atalho "synonym for codex" volte em release futura
- ADR-0042 documenta a decisão arquitetural e previne regressão por refator oportunista
- `vec_degraded_reason` permite distinguir falha por quota de falha por mismatch estrutural
- CLI continua 100% LLM-only no caminho primário e o split é puramente interno
- Backward compatible: callers que ignoram o campo `backend_invoked` seguem funcionando
- Tempo de sessão sob fadiga OAuth cai de infinito para ~45s via claude fallback real

### Como Solucionar Passo a Passo

- Passo 1 — Ler `src/embedder.rs:427-446` e confirmar que `LlmBackendKind::Claude` delega para codex
- Passo 2 — Criar branch `fix/gap-002-llm-backend-claude-split` a partir de `main` v1.0.83
- Passo 3 — Adicionar `embed_via_claude_local` em `src/extract/llm_embedding.rs` invocando `claude -p` headless
- Passo 4 — Atualizar `embed_via_backend` em `src/embedder.rs:435-440` para chamar o novo entry point
- Passo 5 — Adicionar campo `backend_invoked` ao struct `EmbeddingStatusOutput` em `src/embedder.rs`
- Passo 6 — Atualizar `emit_embedding_status` em `src/commands/embedding_status.rs` para incluir o novo campo
- Passo 7 — Adicionar teste em `tests/embedder.rs` que valida o split via mock do subprocesso `claude`
- Passo 8 — Atualizar `--help` de `--llm-backend` para documentar que `claude` agora é backend dedicado
- Passo 9 — Criar ADR-0042 em `docs/decisions/0042-claude-backend-split.md` cross-referenciando ADR-0038 e ADR-0041
- Passo 10 — Rodar `cargo test --workspace` e validar que os 542 testes existentes permanecem verdes
- Passo 11 — Rodar `cargo clippy --workspace --all-targets -- -D warnings` e garantir zero warnings
- Passo 12 — Criar entry no CHANGELOG.md v1.0.84 com referência a GAP-002 e ADR-0042
- Passo 13 — Validar manualmente com `--llm-backend claude --dry-run-backend` em sessão real

### Relação Causa x Efeito

- codex OAuth usage limit causa falha do subprocesso codex headless
- codex falhando causa embedding com vetor de zero dimensões
- embedding zerado causa erro de `validate_dim` em `src/embedder.rs:149`
- validate_dim falhando causa `AppError::Embedding` com exit code 11
- exit code 11 causa aborto da operação antes de chegar ao SQLite
- aborto antes do SQLite causa fila `pending_memories` sempre vazia
- fila vazia causa ausência de replay automático em sessões futuras
- `--llm-backend claude` deveria pular codex mas delega para codex via embed_via_backend
- delegação errada causa falsa sensação de controle para o operador
- falsa sensação de controle causa decisão de continuar usando o flag em vez de workaround externo
- workaround externo `claude -p` direto causa duplicação de canal de persistência
- duplicação de canal causa inconsistência entre memória do disco e memória do grafo
- inconsistência entre discos causa perda de post mortem estruturado quando sessão cai
- perda de post mortem causa reincidência do mesmo padrão de falha na sessão seguinte

### Causa Raiz Arquitetural

- ADR-0038 estabelece codex como default implícito desde v1.0.76
- ADR-0041 preserva credenciais de provider customizado mas não separa o entry point do claude
- O split `embed_via_backend` para `Codex` e `Claude` foi marcado como follow-up de v1.0.83 em comentário
- O follow-up nunca foi implementado porque a v1.0.83 fechou ADR-0041 antes de tocar `embedder.rs`
- O factory `LlmBackendFactory` em `src/extract/llm_backend.rs:212-228` retorna sentinel `()` para embedder
- O sentinel força o embedder real a usar o caminho legado `LlmEmbedding::detect_available`
- O caminho legado é codex-first porque ADR-0038 nunca foi revertido
- A flag `--llm-backend claude` na CLI gera `LlmBackendChoice::Claude` mas o chain é `[claude]`
- O chain `[claude]` cai no loop `embed_with_fallback` que delega para `embed_via_backend`
- O delegate `embed_via_backend` trata `Claude` como sinônimo via `embed_passage_local`
- O sinônimo faz a flag cumprir promessa de UX sem cumprir promessa de execução

### Workaround Definitivo

- Rodar `claude -p` headless direto no terminal para textos críticos que não podem esperar
- Persistir o output de `claude -p` em arquivo Markdown do disco como backup secundário
- Refrescar OAuth do codex via `codex login` antes de sessões longas com codex-first
- Configurar fallback estrutural com `--llm-backend claude,none` para degradação controlada
- Usar `--dry-run-backend` quando disponível para auditar qual binary seria invocado
- Documentar cada falha de embedding como `incident` no Markdown para próximo turno
- Rodar `sqlite-graphrag pending list` ao final de cada sessão para auditar fila vazia
- Rodar `sqlite-graphrag health --json` ao início de cada sessão para validar schema e embedding
- Manter backup do banco via `sqlite-graphrag backup --output ~/backups/graphrag-$(date).sqlite`
- Sincronizar achados do disco para o grafo via `ingest --mode claude-code` na próxima sessão funcional

### Comparação com Sessões Anteriores

- Incident de 2026-06-14 sobre codex refresh token reused via `incident-codex-oauth-refresh-token-reused-2026-06-14`
- Incident de 2026-06-14 sobre SHUTDOWN global persistente via `incident-a1-bloqueada-shutdown-2026-06-14`
- Gap G58 de 2026-06-13 sobre recall e hybrid-search sem fallback determinístico sob fadiga OAuth
- Gap G55 de 2026-06-11 sobre read NotFound perder identificador em mensagem bilíngue
- Gap G45-CR5 de 2026-06-13 sobre 14 headers anthropic-ratelimit descartados pelo subprocesso claude
- Gap G56 de 2026-06-15 sobre custo O de embedding em tokens de saída com dim 384
- ADR-0034 de 2026-06-14 sobre SHUTDOWN resilience via try_reset_shutdown e IGNORE_SHUTDOWN
- ADR-0041 de 2026-06-17 sobre preservação de ANTHROPIC_AUTH_TOKEN em providers customizados
- Padrão recorrente: dependência externa OAuth como ponto único de falha sem fallback funcional
- GAP-002 adiciona ao padrão: flag CLI que promete isolamento mas executa bypass para o mesmo backend

### Referências

- `src/embedder.rs:434-443` — função `embed_via_backend` com delegação sinônima Claude→Codex
- `src/embedder.rs:177-181` — função `embed_passage_local` que invoca `get_embedder`
- `src/embedder.rs:128-135` — função `get_embedder` com `LlmEmbedding::detect_available`
- `src/embedder.rs:205-218` — função `embed_passage_with_choice` que traduz flag CLI em chain
- `src/embedder.rs:368-409` — função `embed_with_fallback` que itera o chain até exaustão
- `src/extract/llm_backend.rs:188-198` — enum `LlmBackendKindFactory` com variante `Claude`
- `src/extract/llm_backend.rs:258-279` — factory `ClaudeFactory` que retorna sentinel `()`
- `src/extract/llm_backend.rs:333-365` — factory `AutoFactory` que prioriza codex no PATH
- `src/extract/llm_backend.rs:374-399` — `detect_available_backend` que prefere codex
- `src/spawn/env_whitelist.rs:14-19` — whitelist de env preservados por ADR-0041
- `src/cli.rs:150` — campo `llm_backend: LlmBackendChoice` na struct CLI
- `src/main.rs:313-379` — propagação do flag para os 6 comandos que produzem embedding
- ADR-0038 — codex como backend default desde v1.0.76
- ADR-0041 — preservação de credenciais de provider customizado em v1.0.83
- ADR-0034 — SHUTDOWN resilience em v1.0.80
- gap-g58-recall-sem-fallback-deterministic-2026-06-13 — fallback FTS5 em recall e hybrid-search
- incident-codex-oauth-refresh-token-reused-2026-06-14 — incidente análogo de OAuth bloqueado
- incident-a1-bloqueada-shutdown-2026-06-14 — incidente análogo de shutdown global
- SQLite FTS5 trigram tokenizer — fallback via `tokenize='trigram'` para queries com typo
- Anthropic rate limits — 12 headers `anthropic-ratelimit-*` documentados em platform.claude.com

## Status

**Solucionado em v1.0.84 (ADR-0042)** em 2026-06-17. Split real do
entry point Claude via `embed_via_claude_local` + `LlmEmbeddingBuilder`.
Envelope JSON `backend_invoked` adicionado em 7 comandos. Flag
`--dry-run-backend` para auditoria.

---

## GAP-003 — Slot Semaphore Timeout (300s) Causa Degradação Silenciosa de hybrid-search para FTS5-puro (v1.0.84, descoberto em produção em 2026-06-17) (Solucionado em v1.0.85 / ADR-0043)

### O Problema

- `acquire_llm_slot_for_embedding` em `src/embedder.rs:289-317` bloqueia por até 30s quando 8+ subprocessos LLM estão ativos
- Timeout retorna `AppError::Embedding` genérico sem discriminar "slot exhausted"
- `try_embed_query_with_fallback` em `src/embedder.rs:425-434` traduz isso em `FallbackReason::EmbeddingFailed(_)` com `reason_code: "embedding_failed"`
- Operador recebe `vec_degraded: true` mas sem `vec_degraded_reason` específico — não distingue quota OAuth de contenção interna
- Causa raiz: `embed_via_claude_local` (v1.0.84) e `embed_via_codex_local` compartilham o mesmo `acquire_llm_slot_for_embedding` com timeout único

### Consequências

- Sessões com 8+ recalls simultâneos perdem precisão semântica silenciosamente
- `pending_embeddings` enche sem diagnóstico de causa raiz
- Operador não tem visibilidade antes do timeout

### A Solução

- S1 — Adicionar variante `FallbackReason::SlotExhausted` com `reason_code: "slot_exhausted"`
- S2 — Refatorar `acquire_llm_slot_for_embedding` para emitir mensagem `"slot exhausted: ..."` quando `AppError::LockBusy` retorna
- S3 — Função `classify_embedding_error` em `src/embedder.rs:436-477` mapeia via substring match para `FallbackReason::SlotExhausted`
- S4 — Adicionar função `try_embed_query_with_deterministic_fallback` que aguarda 750ms e re-tenta uma vez em `SlotExhausted` antes de aceitar degradação

### Status

**Solucionado em v1.0.85 (ADR-0043)** em 2026-06-17. `FallbackReason` agora discrimina 7 causas via `reason_code`. Função `try_embed_query_with_deterministic_fallback` faz retry em OAuthQuota e backoff em SlotExhausted antes de cair em FTS5-puro.

---

## G58 — recall e hybrid-search sem Fallback Determinístico sob Fadiga OAuth (2026-06-13) (Solucionado em v1.0.85 / ADR-0043)

### O Problema

- `recall` e `hybrid-search` em `src/commands/{recall,hybrid_search}.rs:172-240` caem em FTS5 silenciosamente quando embedding live falha por quota OAuth
- Operador não consegue distinguir "FTS5 porque quota" de "FTS5 porque código quebrou"
- Causa raiz: classificação genérica `FallbackReason::EmbeddingFailed(msg)` sem retry alternativo codex ↔ claude

### A Solução

- S1 — Adicionar variante `FallbackReason::OAuthQuota { backend: &'static str }`
- S2 — `classify_embedding_error` detecta substring `"OAuth"` ou `"quota"` e extrai o backend
- S3 — `try_embed_query_with_deterministic_fallback` em `src/embedder.rs:478-505` faz retry com backend alternativo (codex ↔ claude) antes de cair em FTS5

### Status

**Solucionado em v1.0.85 (ADR-0043)** em 2026-06-17. `try_embed_query_with_deterministic_fallback` é o caminho canônico em `hybrid-search` e `recall`.

---

## G45-CR5 — 12-14 Headers `anthropic-ratelimit-*` Descartados pelo Subprocesso `claude -p` (2026-06-13) (Solucionado em v1.0.85 / ADR-0043)

### O Problema

- `LlmEmbedding::invoke_claude` em `src/extract/llm_embedding.rs:530-588` descarta 12-14 headers `anthropic-ratelimit-*` retornados pelo subprocesso `claude -p`
- Headers parseáveis: `requests-remaining`, `tokens-remaining`, `input-tokens-remaining`, `output-tokens-remaining`, `requests-reset`, `tokens-reset`, `status`, `policy`
- Operador não detecta rate limit proativamente; quota estoura antes do operador poder reagir

### A Solução

- S1 — Loop sobre `output.headers` em `invoke_claude` filtrando por prefixo `anthropic-ratelimit-`
- S2 — Detecta `requests-remaining=0`, `tokens-remaining=0`, `input-tokens-remaining=0`, `output-tokens-remaining=0`
- S3 — Retorna `AppError::Embedding("OAuth usage quota exhausted: {name}=0")` que `classify_embedding_error` mapeia para `FallbackReason::OAuthQuota { backend: "claude" }`
- S4 — `try_embed_query_with_deterministic_fallback` faz fallback para codex imediatamente

### Status

**Solucionado em v1.0.85 (ADR-0043)** em 2026-06-17. Headers `anthropic-ratelimit-*-remaining=0` agora ABORTAM o embed e disparam fallback codex.

---

## G55 — `read NotFound` Perdia Identificador em Mensagem Bilíngue (2026-06-11) (Solucionado em v1.0.80 / ADR-0043 docs)

### O Problema

- `AppError::NotFound(String)` carregava só mensagem genérica sem `name` ou `id`
- Operador não sabia qual memória/entidade falhou

### A Solução

- S1 — Adicionar variantes estruturais `AppError::MemoryNotFound { name, namespace }` e `AppError::MemoryNotFoundById { id }` em `src/errors.rs:64-73`
- S2 — Display bilíngue em `src/errors.rs:355-365` via helpers `pt::memory_not_found` e `pt::memory_not_found_by_id`
- S3 — Todos os call sites em `src/commands/read.rs` agora emitem variantes estruturais

### Status

**Solucionado em v1.0.80** e documentado como cross-ref em ADR-0043. v1.0.85 apenas confirma o status.

---

## G56 — Custo O de Embedding em Tokens de Saída com dim 384 (2026-06-15) (Solucionado em v1.0.79 / ADR-0043 docs)

### O Problema

- `dim=384` em codex (`gpt-5.5`) consome ~6x mais tokens de saída que `dim=64`
- Quota OAuth esgota rapidamente sob carga
- 80% das sessões degradam prematuramente

### A Solução

- S1 — Reduzir `DEFAULT_EMBEDDING_DIM` de 384 para 64 (MRL, arXiv 2205.13147) em `src/constants.rs:22`
- S2 — Função `default_embedding_dim()` lê env `SQLITE_GRAPHRAG_EMBEDDING_DIM` (8..=4096) com fallback 64
- S3 — Bancos pré-existentes mantêm `dim` registrada via `schema_meta.dim` (zero migração forçada)

### Status

**Solucionado em v1.0.79 (ADR-0022)** e confirmado em v1.0.85. Documentação consolidada em ADR-0043.

## GAP-004 — `recall` e `hybrid-search` com `--llm-backend none` em v1.0.85 NÃO Caem Graciosamente em FTS5-puro (v1.0.85, descoberto em E2E de release em 2026-06-17) (Solucionado em v1.0.85.1)

### O Problema

- Operador passa `--llm-backend none` em `recall` ou `hybrid-search` esperando FTS5-puro
- `LlmBackendChoice::None` em `src/cli.rs:51` produz chain `vec![LlmBackendKind::None]`
- `embed_via_backend` em `src/embedder.rs:617-637` no braço `LlmBackendKind::None` retorna `Ok(Vec::new())` (linha 623)
- `try_embed_query_with_choice` propaga `Ok((vec![], LlmBackendKind::None))` para o caller
- `recall.rs:198` testa `if let Some(emb) = embedding.as_ref()` — `Some(&vec![])` é `Some`
- `memories::knn_search` em `src/storage/memories.rs` recebe vetor vazio e aborta com exit 11 e mensagem `"knn_search embedding has 0 dims, expected 64"`
- Operador recebe erro em vez de degradação graciosa que o contrato de G58 / GAP-003 promete
- O envelope `vec_degraded` nunca é emitido, quebrando o `failsafe` que o v1.0.80 introduziu

### Consequências do Problema

- `recall --llm-backend none` sempre retorna exit 11 em vez de exit 0 com FTS5-puro
- Operador não pode forçar o caminho sem embedding em recall/hybrid-search
- Pipeline que dependem de FTS5-puro em `recall` precisam conhecer um workaround
- A v1.0.85 (ADR-0043) promete `vec_degraded: true` + `source: "fts_fallback"` mas a promessa é quebrada
- Inconsistência: write paths (`remember` com `--llm-backend none`) gravam com `pending_embeddings` enquanto read paths falham
- Pipeline de auditoria que precisa de `recall` determinístico fica sem opção de bypass

### Causa Raiz do Problema

- `embed_via_backend` em `src/embedder.rs:623` delega para `Ok(Vec::new())` no braço `None` (intencional, sinaliza "sem embedding")
- `try_embed_query_with_choice` em `src/embedder.rs:268-277` (pré-fix) não distingue "vetor vazio de sucesso" de "vetor válido de sucesso"
- O caller (`try_embed_query_with_deterministic_fallback` em `src/embedder.rs:441-466`) trata `Ok((vec![], _))` como embedding válido e propaga
- `recall.rs:198` faz `embedding.as_ref()` em vez de verificar `embedding.as_ref().map(|v| !v.is_empty()).unwrap_or(false)`
- O `if let Some(emb) = ...` aceita `Some(&vec![])` como vetor válido para `knn_search`
- Nenhum dos contratos verificados em G58 (recall/hybrid-search fallback) ou GAP-003 (slot exhaustion) cobre o cenário "backend resolveu para `None`"

### A Solução

- S1 — Adicionar guarda em `try_embed_query_with_choice` que intercepta `Ok((v, _))` quando `v.is_empty()` e converte para `Err(FallbackReason::DimZero)`
- S2 — `FallbackReason::DimZero` (introduzido em v1.0.85 / ADR-0043) já existe no enum, zero trabalho de modelagem
- S3 — `try_embed_query_with_deterministic_fallback` propaga `Err(DimZero)` direto (não tem caso de retry para vetor vazio)
- S4 — `recall.rs:181-186` já trata `Err(reason)` corretamente, setando `embedding = None` e emitindo `vec_degraded: true`
- S5 — Adicionar teste de regressão `try_embed_query_with_none_returns_dim_zero_fallback` em `tests/embedder.rs:629+` com `#[serial(env)]`
- S6 — O discriminador `reason_code: "dim_zero"` aparece no envelope `vec_degraded_reason` para o operador identificar causa raiz

### Benefícios da Solução

- `recall --llm-backend none` agora retorna exit 0 com FTS5-puro em vez de exit 11 com erro
- `hybrid-search --llm-backend none` segue o mesmo contrato
- Operador tem opção explícita de bypass de embedding para auditoria
- O discriminador `vec_degraded_reason: "dim_zero"` permite distinguir "usuário pediu none" de "OAuth exhausted" no envelope
- Promessa de G58 / GAP-003 (graceful degradation) é honrada em 100% dos backends
- Teste de regressão impede que `embed_via_backend` propague vetor vazio novamente

### Como Solucionar Passo a Passo

- Passo 1 — Localizar `try_embed_query_with_choice` em `src/embedder.rs:268-277`
- Passo 2 — Inserir braço intermediário `Ok((v, _backend)) if v.is_empty() => Err(FallbackReason::DimZero),` antes do braço genérico `Ok((v, backend))`
- Passo 3 — Validar que `FallbackReason::DimZero` existe no enum (linha 351 de `src/embedder.rs`)
- Passo 4 — Compilar com `cargo build --release --bin sqlite-graphrag` e validar zero warnings
- Passo 5 — Adicionar teste de regressão em `tests/embedder.rs` (52 linhas, ancorado no padrão `try_embed_query_with_*_returns_dim_zero_fallback`)
- Passo 6 — Rodar `cargo nextest run --profile ci` e validar 945+ testes verdes
- Passo 7 — Reproduzir o cenário pré-fix com mock hermético (3 memórias em tempdir, PATH com mocks dim=64) e confirmar exit 11 → exit 0
- Passo 8 — Documentar em `gaps.md` com cross-ref para ADR-0043 e G58

### Relação Causa x Efeito

- `LlmBackendKind::None` no chain causa `embed_via_backend` retornar vetor vazio
- vetor vazio causa `try_embed_query_with_choice` propagar `Ok((vec![], _))`
- `Ok((vec![], _)` causa `recall.rs:198` aceitar `Some(&vec![])` como embedding válido
- embedding vazio causa `knn_search` abortar com exit 11
- exit 11 causa falha total de `recall --llm-backend none` em vez de degradação controlada
- falha total causa operador perder o bypass explícito de embedding
- bypass perdido causa workaround externo via `--fallback-fts-only` flag (caminho paralelo, não atômico)
- workaround paralelo causa duplicação de mecanismo de FTS5-puro
- duplicação causa inconsistência entre `vec_degraded` em recall vs flag explícita
- inconsistência causa fricção de UX em pipelines de auditoria

### Causa Raiz Arquitetural

- ADR-0043 (v1.0.85) introduz `FallbackReason` com 7 variantes mas não cobre o caso "backend resolveu para `None`"
- O contrato de `try_embed_query_with_choice` promete mapeamento para `FallbackReason` mas trata `Ok(vec![])` como caso de sucesso
- O `if let Some(emb)` em `recall.rs:198` é um padrão idiomático em Rust mas semanticamente permite vetor vazio
- A regra "vetor vazio é embedding válido" faz sentido para write paths (significa "pendente de embedding")
- A mesma regra NÃO faz sentido para read paths (significa "knn_search impossível")
- O split write vs read não foi modelado quando ADR-0043 desenhou o contrato de `try_embed_query_with_choice`

### Workaround Definitivo

- Usar `--fallback-fts-only` flag em `recall` e `hybrid-search` (caminho paralelo) até a v1.0.85.1
- Manter o workaround documentado em `docs/AGENTS.md` para pipelines dependentes de FTS5-puro
- Validar via `sqlite-graphrag recall --fallback-fts-only "query"` que o envelope retorna exit 0

### Comparação com Sessões Anteriores

- G58 de 2026-06-13 sobre fallback determinístico em OAuth — GAP-004 é a extensão para `LlmBackendKind::None`
- GAP-003 de 2026-06-17 sobre slot exhaustion — GAP-004 é a contraparte para o braço `None` do chain
- ADR-0043 de 2026-06-17 introduz `FallbackReason::DimZero` mas não cobre o cenário "backend resolveu para None"
- v1.0.80 introduziu `vec_degraded: true` para recall/hybrid-search — GAP-004 descobre que o contrato é quebrado para `--llm-backend none`

### Referências

- `src/embedder.rs:268-277` — função `try_embed_query_with_choice` (pré-fix: braço único `Ok((v, backend)) => Ok(...)`)
- `src/embedder.rs:617-637` — função `embed_via_backend` com `LlmBackendKind::None => Ok(Vec::new())`
- `src/embedder.rs:351` — variante `FallbackReason::DimZero` introduzida em v1.0.85
- `src/embedder.rs:441-466` — `try_embed_query_with_deterministic_fallback` que propaga `Err(DimZero)`
- `src/cli.rs:51` — `LlmBackendChoice::None => vec![LlmBackendKind::None]`
- `src/commands/recall.rs:198` — `if let Some(emb) = embedding.as_ref()` (degradação correta do `Err`, mas `Ok(vec![])` é tratado como sucesso)
- `tests/embedder.rs:629+` — teste de regressão `try_embed_query_with_none_returns_dim_zero_fallback`
- ADR-0043 — extensão de `FallbackReason` para 7 variantes
- gap-g58-recall-sem-fallback-deterministic-2026-06-13 — contrato de FTS5-puro em recall/hybrid-search

### Status

**Solucionado em v1.0.85.1** em 2026-06-17. Braço intermediário `Ok((v, _backend)) if v.is_empty() => Err(FallbackReason::DimZero),` adicionado a `try_embed_query_with_choice` em `src/embedder.rs`. `recall --llm-backend none` agora retorna exit 0 com `vec_degraded: true` + `source: "fts_fallback"` + `vec_degraded_reason: "dim_zero"`. Teste de regressão em `tests/embedder.rs` impede reintrodução. Zero regressões: 945 testes verdes via `cargo nextest -P ci`.

## BUG-001 — `--dry-run-backend` Exige Subcommand (exit 2) (v1.0.85, descoberto em auditoria local em 2026-06-17) (Solucionado em v1.0.85.2 / ADR-0044)

### O Problema

- Operador executa `./sqlite-graphrag --dry-run-backend` esperando auditoria standalone do backend LLM
- A CLI aborta com `error: 'sqlite-graphrag' requires a subcommand but one was not provided` (exit 2)
- Causa: `Cli::command: Commands` em `src/cli.rs:248` é obrigatório via `#[command(subcommand)]`
- O early-exit em `src/main.rs:313` (`if cli.dry_run_backend { ... }`) nunca é alcançado porque o `clap::Parser::parse` aborta ANTES de chegar ao `main()`
- Workaround documentado: `./sqlite-graphrag --dry-run-backend list` (passa subcommand fictício)
- O design original da GAP-002 S6 previa `--dry-run-backend` standalone como sanity-check de CI

### Consequências

- Pipeline CI que dependem de audit standalone do backend não conseguem invocar a flag
- Operador precisa conhecer o subcommand fictício para usar a feature
- Fricção de UX em auditoria automatizada de OAuth-only environments

### A Solução

- S1 — Tornar `pub command: Option<Commands>` em `src/cli.rs:248` (clap suporta nativamente)
- S2 — Atualizar 4 call sites em `src/main.rs` (`is_embedding_heavy`, `uses_cli_slot`, match arm) para usar `.as_ref().map_or(false, |c| ...)` ou `match cli.command { Some(cmd) => match cmd { ... } None => Ok(()) }`
- S3 — Atualizar 5 call sites de `is_embedding_heavy` em `src/cli.rs` test code
- S4 — Atualizar 10 call sites em `tests/regression_positional_args.rs` para usar `if let Some(Commands::X(args))`
- S5 — Atualizar 1 call site em `src/commands/graph_export.rs` test
- S6 — Atualizar 3 call sites em `src/commands/ingest.rs` test
- S7 — Atualizar 1 call site em `src/commands/namespace_detect.rs` test
- S8 — Validar que `./sqlite-graphrag --dry-run-backend` retorna exit 0 com JSON envelope

### Benefícios

- Sanity-check de backend LLM agora funciona sem subcommand fictício
- Pipeline CI podem auditar OAuth-only env sem conhecimento de subcommands
- UX consistente com `--version`, `--help` (também standalone)
- Zero regressões: 946 testes verdes via `cargo nextest -P ci`

### Status

**Solucionado em v1.0.85.2** em 2026-06-17. `pub command: Option<Commands>` em `src/cli.rs:248` com 4 call sites em `src/main.rs` e 12 call sites em test code. Validação: `./target/release/sqlite-graphrag --dry-run-backend` retorna exit 0 com `{"action":"dry_run_backend","backend":"claude","binary":"...","model":"claude-sonnet-4-6","flavour":"claude","chain":"codex,claude,none","strict_env_clear":false}`. Cross-ref: ADR-0042 (GAP-002 S6) e ADR-0044 (este fix).

## BUG-002 — Teste `embed_via_backend_codex_does_not_invoke_claude` Falha (Mock Malformado) (v1.0.85, descoberto em auditoria local em 2026-06-17) (Solucionado em v1.0.85.2 / ADR-0044)

### O Problema

- `cargo test --test embedder embed_via_backend_codex_does_not_invoke_claude -- --ignored` falha
- Mensagem: `--llm-backend=codex must invoke the codex script, but the dump file was not created`
- Causa: `setup_mock_path()` em `tests/embedder.rs:47-66` emite o mesmo JSON `{"embedding":[64 zeros]}` para AMBOS os scripts `claude` e `codex`
- `parse_llm_json` em `src/extract/llm_embedding.rs:858-887` tem 2 strategies: Strategy 1 (JSON puro para claude) e Strategy 2 (JSONL para codex)
- Para o codex, o JSON puro é parseado por Strategy 1 antes de Strategy 2 ter chance — o vetor vazio é retornado sem erro
- O call site `try_embed_query_with_choice` intercepta vetor vazio e retorna `Err(FallbackReason::DimZero)` antes do dump file ser criado
- Resultado: o test mirror do GAP-002 para codex não tem cobertura real

### Consequências

- GAP-002 tem cobertura real só para o path claude (`embed_via_backend_claude_does_not_invoke_codex`)
- Path codex do `embed_via_backend` continua sem regressão
- Refator futuro pode quebrar o path codex silenciosamente

### A Solução

- S1 — Refatorar `setup_mock_path()` em `tests/embedder.rs:37-77` para emitir JSON puro para `claude` (Strategy 1) e JSONL estruturado para `codex` (Strategy 2)
- S2 — O JSONL codex segue o envelope `{"type":"item.completed","item":{"type":"agent_message","text":"<inner_json>"}}` com `inner_json = {"embedding":[64 zeros]}`
- S3 — Validar que ambos os tests passam via `cargo test --test embedder embed_via_backend_ -- --ignored`

### Benefícios

- GAP-002 agora tem regressão completa para ambos os paths (claude E codex)
- 1 test que estava falhando agora passa em 0.05s
- Padrão de mock JSON vs JSONL documentado para futuros tests

### Status

**Solucionado em v1.0.85.2** em 2026-06-17. `setup_mock_path()` em `tests/embedder.rs:37-77` emite JSON puro para claude e JSONL estruturado para codex. Validação: `cargo test --test embedder embed_via_backend_codex_does_not_invoke_claude -- --ignored --nocapture` retorna `test result: ok. 1 passed; 0 failed`. Cross-ref: ADR-0042 (GAP-002 S4) e ADR-0044 (este fix).

## BUG-003 — `backend_invoked` Reflete Backend Tentado, Não Backend Executado (v1.0.85, descoberto em auditoria local em 2026-06-17) (Solucionado em v1.0.85.2 / ADR-0044)

### O Problema

- Operador passa `--llm-backend codex` com `codex` ausente do PATH e `claude` presente
- `embed_via_backend(Codex)` chama `embed_passage_local` que chama `get_embedder` que chama `LlmEmbedding::detect_available`
- `detect_available` prefere `codex`; como está ausente, retorna um `LlmEmbedding` com `EmbeddingFlavour::Claude`
- O subprocesso `claude -p` é executado, NÃO o codex
- Mas `embed_with_fallback` retorna `Ok((v, *backend))` com `*backend = LlmBackendKind::Codex` (chain position)
- O envelope JSON carrega `"backend_invoked": "codex"` — MENTIRA
- Operador vê `"backend_invoked": "codex"` e pensa que codex executou, mas codex nem estava no PATH

### Consequências

- Envelope fica mentiroso: 7 comandos expõem `backend_invoked` (remember, edit, ingest, enrich, recall, hybrid-search, embedding status)
- Operador não consegue distinguir "codex executou" de "claude substituiu codex"
- Diagnóstico de embedding failure fica opaco
- ADR-0042 (GAP-002 fix) fez split do entry point Claude, mas o chain ainda propaga o chain position

### A Solução

- S1 — Adicionar `pub fn flavour(&self) -> EmbeddingFlavour` em `LlmEmbedding` (em `src/extract/llm_embedding.rs:366`)
- S2 — Adicionar `pub fn embed_passage_local_resolved(models_dir, text) -> Result<(Vec<f32>, LlmBackendKind), AppError>` em `src/embedder.rs` que retorna o `LlmBackendKind` baseado no `LlmEmbedding::flavour()` real do embedder construído
- S3 — Adicionar `pub fn embed_via_claude_local_resolved(...) -> Result<(Vec<f32>, LlmBackendKind), AppError>` que sempre retorna `LlmBackendKind::Claude` (já é dedicado)
- S4 — Refatorar `embed_via_backend` para retornar `Result<(Vec<f32>, LlmBackendKind), AppError>` usando as 2 funções acima
- S5 — Refatorar `embed_with_fallback` para propagar o `resolved_kind` retornado por `embed_via_backend` (não mais `*backend` da chain position)
- S6 — Adicionar `pub fn embed_via_backend_legacy(...)` que descarta o backend para call sites que não precisam do sinal
- S7 — Atualizar test em `src/embedder.rs:1592` para verificar `assert_eq!(kind, LlmBackendKind::None)`

### Benefícios

- Envelope `backend_invoked` agora reflete o backend que REALMENTE executou
- Operador vê a verdade: se pediu codex e codex não estava, vê `claude` no envelope
- Diagnóstico de embedding failure fica transparente
- G58 + GAP-003 + GAP-004 + BUG-003 formam o conjunto completo de observabilidade de degradação
- Zero regressões: 946 testes verdes via `cargo nextest -P ci`

### Workaround Definitivo

- Usar `--dry-run-backend` para auditar o backend ANTES de sessões longas
- Inspecionar `envelope.backend_invoked` no output JSON para confirmar o backend real

### Status

**Solucionado em v1.0.85.2** em 2026-06-17. `embed_via_backend` agora retorna `Result<(Vec<f32>, LlmBackendKind), AppError>` com o `LlmBackendKind` baseado no `LlmEmbedding::flavour()` real. `embed_with_fallback` propaga o `resolved_kind` no tuple. 7 envelopes (`backend_invoked` em remember, edit, ingest, enrich, recall, hybrid-search, embedding status) agora reportam o backend que de fato executou. Validação: 946 testes verdes. Cross-ref: ADR-0042 (GAP-002 S1) e ADR-0044 (este fix).
