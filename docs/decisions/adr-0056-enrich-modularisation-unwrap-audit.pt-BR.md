# ADR-0056: Modularização do Enrich + Auditoria de unwrap/expect + DRY do parse_claude_output (v1.0.97)

- **Status**: Aceito
- **Data**: 2026-06-29
- **Versão**: v1.0.97 (fecha os itens de dívida técnica sinalizados no ADR-0046)

## Contexto

O ADR-0046 (v1.0.88) registrou duas dívidas técnicas conhecidas: `src/commands/enrich.rs`
tinha 4116 linhas (e cresceu para 6013 na v1.0.97) com uma divisão planejada em
`queue/extraction/postprocess`, e uma auditoria separada sinalizou "423 unwrap()/expect()
fora de testes" para revisão.

A investigação corrigiu ambas as premissas:

- O número "423" contava blocos `#[cfg(test)]`. A contagem real em produção era
  ~36 sítios em 6 arquivos (`enrich` 25, `embedder` 6, `signals` 2, e
  `system_load`/`constants`/`chunking` 1 cada). Um gate posterior do `clippy` achou
  mais 5 em `config_cmd.rs` que uma heurística de fronteira `cfg(test)` tinha perdido.
- A extração sugerida de `llm_runner.rs` estava obsoleta: `claude_runner.rs` já
  hospeda os helpers Claude compartilhados e o `enrich` já os usava. A única duplicação
  real restante era `ingest_claude::parse_claude_output`, que havia divergido
  semanticamente (ela tolera `max_turns`; o `claude_runner` o trata como fatal).

## Decisão

1. **Modularizar** `enrich.rs` (6013 linhas) em um módulo-diretório
   `src/commands/enrich/` com `mod.rs` (orquestrador + run + tipos da CLI),
   `queue.rs`, `scan.rs`, `postprocess.rs` e `extraction.rs`. O `mod.rs` cai para
   2355 linhas. Os seis símbolos consumidos externamente (`run`, `EnrichArgs`,
   `EnrichOperation`, `EnrichMode`, `EnrichStatus`, `cleanup_queue_entry`) permanecem
   públicos e são re-exportados de `mod.rs`. Sem mudança de comportamento; todos os
   testes unitários do enrich preservados.

2. **Auditar unwrap/expect** no código de produção. Conversões: `OnceLock.get().expect`
   para `ok_or_else(AppError)`; o `.expect` de thread-spawn em `signals` para
   `.inspect_err(warn).ok()` (best-effort, a função retorna `()`); o `.expect` de mutex
   em `system_load` para recuperação de poison via `unwrap_or_else(into_inner)`; o
   `status.unwrap()` de `wait_with_timeout` para `let-else`; os 24 `provider_binary.expect`
   nos dispatchers worker/serial para um único `provider_bin` pré-computado
   (`unwrap_or_else(|| Path::new(""))`, preservando `ReEmbed` onde o binário está
   legitimamente ausente); o `serde_json::to_string(...).unwrap()` de `config_cmd` para `?`
   via o `AppError::Json(#[from])` existente.

3. **Lint gate**: `#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]`
   em `src/lib.rs`. Invariantes provados em tempo de compilação (a const regex
   `constants::name_slug_regex`, o `chunking` overlap<size const) mantêm `expect` com um
   `#[allow]` local e justificativa (convertê-los seria over-engineering).

4. **DRY** do `parse_claude_output`: adicionar `claude_runner::parse_claude_output_opts(stdout,
   tolerate_max_turns: bool)`. `parse_claude_output` é o wrapper `false` (enrich);
   `ingest_claude` o chama com `true`. ~40 linhas duplicadas removidas; a divergência
   semântica de `max_turns` é preservada (e protegida por
   `test_terminal_reason_max_turns_detected`). `extract_with_claude` (guard OAuth próprio)
   e `open_queue_db` (schema divergente) NÃO são unificados intencionalmente.

## Consequências

### Positivas

- `enrich/mod.rs` de 6013 para 2355 linhas; quatro submódulos coesos.
- Zero panics de `unwrap/expect` de produção alcançáveis no `enrich`; o lint bloqueia regressão.
- `parse_claude_output` com fonte única de verdade; o `ingest` ganha G03 max_turns + warn de auth.
- `cargo build`/`clippy --lib` (0 warnings)/`cargo test` todos verdes; testes do enrich 36/36.

### Negativas / Notas

- `open_queue_db` permanece duplicado (schema divergente) — deixado para uma passada futura.
- A linha de fronteira `cfg(test)` se desloca quando um arquivo é editado; sempre re-derive.

## Referências cruzadas

- ADR-0046 (a fonte de dívida técnica que isto fecha)
- `src/commands/enrich/` (mod, queue, scan, postprocess, extraction)
- `src/commands/claude_runner.rs` (`parse_claude_output_opts`)
- `src/lib.rs` (lint gate unwrap_used/expect_used)
