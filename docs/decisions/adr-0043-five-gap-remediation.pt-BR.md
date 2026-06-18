# ADR-0043: Remediação de Cinco Gaps — FallbackReason Tipado, Fallback OAuth Determinístico, Headers Rate-Limit, Read NotFound Bilíngue, dim 64

- **Status**: Aceito
- **Data**: 2026-06-17
- **Versão**: v1.0.85 (resolve GAP-003, G58, G45-CR5, G55 docs, G56 docs)
- **Autores**: tech-lead

## Contexto

GAP-002 (v1.0.84, ADR-0042) separou o entry point Claude para que `--llm-backend claude` invoque Claude de fato. Cinco gaps correlatos permaneceram abertos e são agora consolidados em um único release v1.0.85.

### GAP-003 — Timeout do slot semaphore

`acquire_llm_slot_for_embedding` em `src/embedder.rs:289-317` bloqueia até 30s quando 8+ subprocessos LLM estão ativos. O `AppError::Embedding` resultante é mapeado para `FallbackReason::EmbeddingFailed(msg)` com discriminador `"embedding_failed"`, indistinguível de exaustão de quota ou bug estrutural.

Trace de produção em `/tmp/claude-1000/.../tasks/b6ppfly55.output` (2026-06-17):

```
WARN hybrid_search: live embedding failed; falling back to FTS5
  fallback_reason=embedding failed: lock busy: failed to acquire LLM slot within 300s (max=8 concurrent)
```

### G58 — Fallback não-determinístico sob fadiga OAuth

`recall` e `hybrid-search` em `src/commands/{recall,hybrid_search}.rs` caem em FTS5-puro em qualquer erro de embedding. O operador não distingue se o fallback decorre de exaustão de quota (recuperável via troca de backend) ou bug estrutural.

### G45-CR5 — Headers `anthropic-ratelimit-*` descartados

`LlmEmbedding::invoke_claude` em `src/extract/llm_embedding.rs:530-588` descarta 12-14 headers `anthropic-ratelimit-*` retornados pelo subprocesso `claude -p`. O operador nunca vê o countdown do rate limit e só descobre a interrupção quando o subprocesso retorna exit 11.

### G55 — `read NotFound` perdia o identificador

`AppError::NotFound(String)` descartava o nome ou id da entidade ausente. Documentado em v1.0.80 como `AppError::MemoryNotFound { name, namespace }` e `AppError::MemoryNotFoundById { id }` com Display bilíngue via `pt::memory_not_found` e `pt::memory_not_found_by_id`.
### Nota — GAP-003 ID Sobrecarregado

O ID de gap `GAP-003` é usado por dois gaps distintos entre releases:

- **GAP-003 (v1.0.82)** — `docs/decisions/adr-0038-llm-backend-user-choice.pt-BR.md` documenta a escolha de backend LLM pelo usuário
- **GAP-003 (v1.0.85)** — este ADR documenta o refinamento do timeout do slot semaphore (o discriminador `FallbackReason::SlotExhausted`)

Ao citar `GAP-003` em cross-references, anexe o sufixo de versão (ex.: `GAP-003@1.0.85`) para desambiguar. ADRs futuros devem adotar esta convenção.


### G56 — `dim 384` queimava quota OAuth

Embedding com `dim=384` em codex (`gpt-5.5`) consumia ~6× mais tokens de saída que `dim=64`. Default reduzido para 64 (MRL, arXiv 2205.13147) em v1.0.79.

## Decisão

### 1. `FallbackReason` estende de 3 para 7 variantes

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum FallbackReason {
    EmbeddingFailed(String),
    SlotExhausted,                                   // GAP-003
    OAuthQuota { backend: &'static str },             // G58, G45-CR5
    BackendMismatch { requested: &'static str, resolved: &'static str },
    DimZero,                                         // discriminador de bug estrutural
    Cancelled,
    Timeout { operation: String, duration_secs: u64 },
}
```

`reason_code()` retorna string estável por variante: `"embedding_failed" | "slot_exhausted" | "oauth_quota" | "backend_mismatch" | "dim_zero" | "cancelled" | "timeout"`.

### 2. `classify_embedding_error` (função pura, sem I/O)

Localizada em `src/embedder.rs:436-477`. Mapeia `AppError` para `FallbackReason` via match lexical de substring — sem retries, sem telemetria, determinística e segura para `#[serial_test::serial(env)]`.

### 3. `try_embed_query_with_deterministic_fallback` (G58)

Em `src/embedder.rs:478-505`. Em `OAuthQuota`, re-tenta uma vez com o backend alternativo (codex ↔ claude). Em `SlotExhausted`, dorme 750 ms e re-tenta uma vez. Em qualquer outra razão, retorna imediatamente.

### 4. `acquire_llm_slot_for_embedding` (GAP-003)

Quando `crate::llm_slots::acquire_llm_slot` retorna `AppError::LockBusy` com `wait_secs > 0`, o erro é reescrito como `AppError::Embedding("slot exhausted: ...")`. `classify_embedding_error` então mapeia a substring para `FallbackReason::SlotExhausted`.

### 5. `LlmEmbedding::invoke_claude` (G45-CR5)

Após `cmd.output()`, itera `output.headers`. Para cada header `anthropic-ratelimit-*-remaining`, verifica se o valor é `0`. Quando sim, retorna `AppError::Embedding("OAuth usage quota exhausted: {name}=0")` ANTES de checar o exit status do subprocesso — isto permite `classify_embedding_error` mapeá-lo para `OAuthQuota { backend: "claude" }`.

### 6. Gates de validação

- `cargo check --workspace --all-targets` exit 0
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` exit 0
- `cargo nextest run --profile ci` 830+ testes
- `cargo llvm-cov nextest --profile ci --summary-only` ≥ 80%
- `cargo test --test embedder -- --ignored` env hermético
- `--dry-run-backend` 4 backends retornam JSON

## Consequências

### Positivas

- Operador distingue exaustão de quota de exaustão de slot de bug estrutural via discriminador `vec_degraded_reason`
- Sessões sob fadiga OAuth em codex trocam transparentemente para claude antes de cair em FTS5
- Headers de rate-limit viram sinal de primeira classe — exaustão de quota é detectada proativamente, não após exit não-zero
- Exaustão de slot tem teto de 750 ms (era 30 s) antes de degradar para FTS5
- Retrocompatível: `FallbackReason::EmbeddingFailed(msg)` continua funcionando para mensagens não reconhecidas
- Mensagens bilíngues `read NotFound` preservadas desde v1.0.80
- Default `dim 64` preservado desde v1.0.79

### Negativas

- Cinco call sites de `try_embed_query_with_choice` atualizados em hybrid_search.rs e recall.rs — corrigidos atomicamente
- `classify_embedding_error` depende de match de substring das mensagens de erro — precisa atualizar quando a redação mudar
- `try_embed_query_with_deterministic_fallback` adiciona até 750 ms de latência no caminho `SlotExhausted`
- Cresce a contagem de testes (5 novos testes de regressão) — overhead de manutenção

## Alternativas Consideradas

1. **Abordagem apenas com telemetria (rejeitada)**: adicionar campos de métricas aos envelopes sem mudar `FallbackReason`. Rejeitada — não resolve o problema subjacente de indistinguibilidade.
2. **Circuit breaker com janela rolante (rejeitada para v1.0.85)**: `AtomicU64` com contador global. Rejeitado — adiciona complexidade sem benefício proporcional no escopo de v1.0.85.
3. **Pular headers OAuth inteiramente (rejeitado)**: manter comportamento atual. Rejeitado — exaustão de quota acontece sem aviso.
4. **Refatoração big-bang única (rejeitada)**: mesclar todos os 5 gaps em um PR massivo. Rejeitado — viola a regra de escopo cirúrgico.

## Referências

- `src/embedder.rs:289-317` — `acquire_llm_slot_for_embedding`
- `src/embedder.rs:425-477` — `try_embed_query_with_fallback` + `classify_embedding_error`
- `src/embedder.rs:478-505` — `try_embed_query_with_deterministic_fallback`
- `src/commands/hybrid_search.rs:218-241` — atualização de call site
- `src/commands/recall.rs:172-184` — atualização de call site
- `src/extract/llm_embedding.rs:530-619` — `invoke_claude` com headers rate-limit
- `src/errors.rs:64-73` — `AppError::MemoryNotFound` / `MemoryNotFoundById`
- `src/errors.rs:355-365` — Display bilíngue
- `src/constants.rs:22` — `DEFAULT_EMBEDDING_DIM = 64`

## Decisões Relacionadas

- **ADR-0042 (v1.0.84)**: separação do entry point Claude — GAP-003 herda desta arquitetura
- **ADR-0041 (v1.0.83)**: preservação de env customizado de provider — habilita o caminho G45-CR5 para gateways Anthropic-compatíveis
- **ADR-0038 (v1.0.76)**: codex como backend default — alvo da troca em G58
- **ADR-0034 (v1.0.80)**: resiliência SHUTDOWN — cross-ref GAP-003 (exaustão de slot pode mascarar SHUTDOWN)
