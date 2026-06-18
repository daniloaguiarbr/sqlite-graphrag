# ADR-0040: Captura de Stderr/Stdout Tails + Cadeia de Fallback para Backends LLM

- **Status**: Aceito
- **Data**: 2026-06-15
- **Versão**: v1.0.82 (resolve GAP-005)
- **Autores**: tech-lead

## Contexto

Quando subprocesso LLM crashava com exit não-zero, a mensagem de erro era substituída pelo
literal `stderr=` ou `output:`. Usuário não distinguia OOM (137), binary not found (127),
abort (134) ou genérico (1). Retry cego no mesmo backend desperdiçava quota OAuth.

## Decisão

Refatorar `AppError::Embedding` em `AppError::LlmBackend { error: LlmBackendError }` com 4
sub-variantes tipadas:

```rust
pub enum LlmBackendError {
    NonZeroExit { exit_code, signal, stdout_tail, stderr_tail, binary, hint },
    SpawnFailed { binary, source },
    Timeout { secs, binary },
    NoBackendsAvailable,
}
```

- `truncate_tail` preserva boundary UTF-8 (hand-rolled since `is_char_boundary` not on `[u8]`)
- Tabela estática `EXIT_CODE_HINTS` mapeia 9 exit codes (1, 2, 101, 126, 127, 134, 137, 139,
  143) a hints acionáveis (ex: 137 → "OOM killer; reduzir --llm-parallelism")
- `embed_with_fallback(backends, skip_on_failure)` itera chain, loga warn em cada falha,
  retorna `Ok(vec![])` se `--skip-embedding-on-failure` set
- Fila `pending_embeddings` (V015) para reprocessamento via `enrich --operation re-embed
  --pending-only` ou `embedding retry`

## Consequências

### Positivas
- Diagnóstico estruturado: exit code, signal, hint acionável
- Cadeia de fallback: codex→claude→none automática
- Fila persistente: corpo salvo mesmo quando embedding falha
- Truncamento preserva UTF-8
- Subcomandos `embedding` + `pending-embeddings` para inspeção

### Negativas
- Overhead de 1KB por subprocesso capturado
- Tabela `EXIT_CODE_HINTS` exige manutenção para novos exit codes
- Cadeia com N backends adiciona latência cumulativa em falha (mitigado por timeout curto)

## Referências

- `gaps.md:1111-1503`
- `src/llm/exit_code_hints.rs`
- `src/extract/llm_embedding.rs:447-595`
- `src/embedder.rs:embed_with_fallback`
- `migrations/V015__pending_embeddings.sql`
- `src/commands/embedding.rs` + `src/commands/pending_embeddings.rs`
### Refinado por ADR-0043 (v1.0.85)

ADR-0043 (`docs/decisions/adr-0043-five-gap-remediation.pt-BR.md`) estende a observabilidade introduzida aqui. O enum `LlmBackendError` do ADR-0040 é complementado pelo `FallbackReason` de 7 variantes com discriminador `reason_code`. O campo `vec_degraded_reason: Option<String>` nos envelopes `recall` e `hybrid-search` informa operadores se o embedding live falhou por quota (`"oauth_quota"`), mismatch de backend (`"backend_mismatch"`), exaustão de slot (`"slot_exhausted"`), ou uma das outras 4 razões. A captura de `anthropic-ratelimit-*-remaining=0` (G45-CR5) é o gatilho proativo que alimenta o reason code `oauth_quota`.
