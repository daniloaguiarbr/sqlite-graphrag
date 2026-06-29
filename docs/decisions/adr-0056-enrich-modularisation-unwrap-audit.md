# ADR-0056: Enrich Modularisation + unwrap/expect Audit + parse_claude_output DRY (v1.0.97)

- **Status**: Accepted
- **Date**: 2026-06-29
- **Version**: v1.0.97 (closes the tech-debt items flagged in ADR-0046)

## Context

ADR-0046 (v1.0.88) recorded two pieces of known tech debt: `src/commands/enrich.rs`
was 4116 lines (and grew to 6013 by v1.0.97) with a planned split into
`queue/extraction/postprocess`, and a separate audit flagged "423 unwrap()/expect()
outside tests" for review.

Investigation corrected both premises:

- The "423" figure counted `#[cfg(test)]` blocks. The real production count was
  ~36 sites across 6 files (`enrich` 25, `embedder` 6, `signals` 2, and
  `system_load`/`constants`/`chunking` 1 each). A subsequent `clippy` gate found
  5 more in `config_cmd.rs` that a `cfg(test)`-boundary heuristic had missed.
- The suggested `llm_runner.rs` extraction was obsolete: `claude_runner.rs` already
  hosts the shared Claude helpers and `enrich` already used them. The only real
  duplication left was `ingest_claude::parse_claude_output`, which had diverged
  semantically (it tolerates `max_turns`; `claude_runner` treats it as fatal).

## Decision

1. **Modularise** `enrich.rs` (6013 lines) into a directory module
   `src/commands/enrich/` with `mod.rs` (orchestrator + run + CLI types),
   `queue.rs`, `scan.rs`, `postprocess.rs`, and `extraction.rs`. `mod.rs` drops to
   2355 lines. The six externally-consumed symbols (`run`, `EnrichArgs`,
   `EnrichOperation`, `EnrichMode`, `EnrichStatus`, `cleanup_queue_entry`) stay
   public and are re-exported from `mod.rs`. No behaviour change; all enrich unit
   tests preserved.

2. **Audit unwrap/expect** in production code. Conversions: `OnceLock.get().expect`
   to `ok_or_else(AppError)`; `signals` thread-spawn `.expect` to
   `.inspect_err(warn).ok()` (best-effort, function returns `()`); `system_load`
   mutex `.expect` to `unwrap_or_else(into_inner)` poison-recovery;
   `wait_with_timeout` `status.unwrap()` to `let-else`; 24 `provider_binary.expect`
   in the worker/serial dispatchers to a single pre-computed `provider_bin`
   (`unwrap_or_else(|| Path::new(""))`, preserving `ReEmbed` where the binary is
   legitimately absent); `config_cmd` `serde_json::to_string(...).unwrap()` to `?`
   via the existing `AppError::Json(#[from])`.

3. **Lint gate**: `#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]`
   in `src/lib.rs`. Proven compile-time invariants (`constants::name_slug_regex`
   const regex, `chunking` overlap<size const) keep `expect` with a local
   `#[allow]` and justification (converting them would be over-engineering).

4. **DRY** `parse_claude_output`: add `claude_runner::parse_claude_output_opts(stdout,
   tolerate_max_turns: bool)`. `parse_claude_output` is the `false` wrapper (enrich);
   `ingest_claude` calls it with `true`. ~40 duplicated lines removed; the `max_turns`
   semantic divergence is preserved (and guarded by
   `test_terminal_reason_max_turns_detected`). `extract_with_claude` (own OAuth guard)
   and `open_queue_db` (divergent schema) are intentionally NOT unified.

## Consequences

### Positive

- `enrich/mod.rs` 6013 to 2355 lines; four cohesive submodules.
- Zero production `unwrap/expect` panics reachable in `enrich`; lint blocks regression.
- `parse_claude_output` single source of truth; `ingest` gains G03 max_turns + auth warn.
- `cargo build`/`clippy --lib` (0 warnings)/`cargo test` all green; enrich tests 36/36.

### Negative / Notes

- `open_queue_db` remains duplicated (divergent schema) — left for a future pass.
- The `cfg(test)`-boundary line shifts when a file is edited; always re-derive it.

## Cross-references

- ADR-0046 (the tech-debt source this closes)
- `src/commands/enrich/` (mod, queue, scan, postprocess, extraction)
- `src/commands/claude_runner.rs` (`parse_claude_output_opts`)
- `src/lib.rs` (unwrap_used/expect_used lint gate)
