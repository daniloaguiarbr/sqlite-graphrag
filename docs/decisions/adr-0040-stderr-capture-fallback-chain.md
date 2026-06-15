# ADR-0040: Captura de Stderr/Stdout Tails + Cadeia de Fallback para Backends LLM

- **Status**: Aceito
- **Data**: 2026-06-15
- **Versão**: v1.0.82 (resolve GAP-005)
- **Autores**: tech-lead

## Contexto

Quando o subprocesso LLM (`codex exec` ou `claude -p`) crashava com exit não-zero, a
mensagem de erro era substituída pelo literal `stderr=` ou `output:` em
`src/commands/{claude_runner,codex_spawn}.rs:497-501,545-548`. O usuário não sabia distinguir
entre OOM (exit 137), binary not found (exit 127), abort interno (exit 134) ou
genérico (exit 1). Retry cego no mesmo backend quebrado desperdiçava quota OAuth.

O schema `AppError::Embedding(String)` carregava só a string formatada, sem campos
estruturados para automação.

## Decisão

Refatorar `AppError::Embedding` em nova variante `AppError::LlmBackend { error: LlmBackendError }`
com 4 sub-variantes tipadas:

```rust
pub enum LlmBackendError {
    NonZeroExit {
        exit_code: Option<i32>,
        signal: Option<i32>,
        stdout_tail: String,    // 1KB max, UTF-8 safe
        stderr_tail: String,    // 1KB max, UTF-8 safe
        binary: String,
        hint: String,            // EXIT_CODE_HINTS lookup
    },
    SpawnFailed { binary: String, source: String },
    Timeout { secs: u64, binary: String },
    NoBackendsAvailable,
}
```

- `truncate_tail` preserva boundary UTF-8 (hand-rolled since `is_char_boundary` not on `[u8]`)
- Tabela estática `EXIT_CODE_HINTS` mapeia 9 exit codes conhecidos (1, 2, 101, 126, 127, 134,
  137, 139, 143) a hints acionáveis (ex: 137 → "OOM killer; reduzir --llm-parallelism")
- Função `embed_with_fallback(backends, skip_on_failure)` itera chain, loga warn em cada
  falha, retorna `Ok(vec![])` se `--skip-embedding-on-failure` set

Persistência em fila `pending_embeddings` (V015) para reprocessamento posterior via
`enrich --operation re-embed --pending-only` ou `embedding retry`.

## Consequências

### Positivas
- Diagnóstico estruturado: exit code, signal, hint acionável
- Cadeia de fallback: codex→claude→none automática em falha
- Fila persistente: corpo salvo mesmo quando embedding falha
- Truncamento preserva UTF-8 (não corta em byte de continuação)
- Subcomandos `embedding` + `pending-embeddings` para inspeção

### Negativas
- Overhead de 1KB por subprocesso capturado (mínimo)
- Tabela `EXIT_CODE_HINTS` exige manutenção quando novos exit codes relevantes
  aparecem
- Cadeia com N backends adiciona latência cumulativa em caso de falha (mitigado por
  timeout curto e fallback imediato em `BinaryNotFound`)

## Alternativas Consideradas

1. **Manter literal `stderr=` (status quo)**: zero diagnóstico — descartado
2. **Persistir stderr completo sem truncar**: pode estourar limite SQLite de 1 GB para
   subprocessos verbosos — descartado
3. **Só retry cego no mesmo backend**: desperdiça quota OAuth em falha persistente —
   descartado

## Referências

- `gaps.md:1111-1503` — GAP-005 completo
- `src/llm/exit_code_hints.rs` (tabela `EXIT_CODE_HINTS` + `LlmBackendError`)
- `src/extract/llm_embedding.rs:447-595` (`invoke_claude` / `invoke_codex` populam tails)
- `src/embedder.rs:embed_with_fallback` (cadeia)
- `migrations/V015__pending_embeddings.sql` (fila persistente)
- `src/commands/embedding.rs` + `src/commands/pending_embeddings.rs` (subcomandos)
