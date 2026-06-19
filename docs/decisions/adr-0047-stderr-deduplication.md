# ADR-0047: Stderr Deduplication — OAuth Single-Line + slots.rs Tracing Gate (v1.0.88)

- **Status**: Accepted
- **Data**: 2026-06-19
- **Versão**: v1.0.88 (closes GAP-15 + BUG-12 followup)
- **Autores**: Danilo Aguiar <daniloaguiarbr@gmail.com>

## Context

Two distinct stderr-amplification bugs surfaced after v1.0.87 shipped:

### BUG-12 — Double-line stderr per OAuth-only enforcement

`src/output.rs:141` (`output::emit_error`) routed the error envelope through BOTH `tracing::error!` (which renders to stderr via the tracing-subscriber) AND a direct `eprintln!` of the same message. The result was 2 lines of stderr per violation — the tracing layer added a formatted `[ERROR]` prefix line, and the `eprintln!` added the raw message line. Operators running `sqlite-graphrag ... 2>err.log | tee out.json` saw doubled rows. The integration test `oauth_stderr_emits_single_line_v1088` was added in v1.0.88 specifically to fail under the doubled output.

### GAP-15 — slots.rs println! scope expansion

`src/commands/slots.rs` used `println!` directly to emit slot-acquire / slot-release diagnostics. While `println!` writes to stdout (the structured NDJSON stream), the commands in this file run as part of the global CLI dispatcher — `println!` here bypasses both:

- the JSON envelope that other commands emit on stdout
- the tracing-subscriber gate that filters by `RUST_LOG` / `--log-level`

The result was that `sqlite-graphrag slots status` produced free-form text on stdout instead of the structured JSON envelope the rest of the CLI returns for the same query.

## Decision

Two minimal, surgical fixes in v1.0.88:

### Fix 1 — BUG-12: drop the redundant `eprintln!`

In `src/output.rs:141`, the `eprintln!({msg})` call after `tracing::error!({msg})` is removed. The `tracing::error!` call alone is sufficient — the tracing-subscriber renders the formatted line to stderr exactly once, and `RUST_LOG` / `--log-level` continue to govern whether the line appears at all.

```rust
// Before (v1.0.87):
pub fn emit_error(code: u8, msg: &str) {
    tracing::error!(target: "output", code, msg);
    eprintln!("{}", msg);  // <-- removed in v1.0.88
}

// After (v1.0.88):
pub fn emit_error(code: u8, msg: &str) {
    tracing::error!(target: "output", code, msg);
}
```

### Fix 2 — GAP-15: replace `println!` in slots.rs with `crate::output::emit_info`

In `src/commands/slots.rs`, all 5 occurrences of `println!` are replaced by `crate::output::emit_info(msg)` (or `tracing::info!` where the message is purely diagnostic and not intended for the stdout envelope). The split is:

- `println!("slot acquired: ...")` → `crate::output::emit_info("slot acquired: ...")` — routes via tracing-subscriber to stderr
- `println!("slot released: ...")` → `crate::output::emit_info("slot released: ...")` — same
- `println!("acquire timed out ...")` → `crate::output::emit_info("acquire timed out ...")` — same
- `println!` in the JSON-output branch of `slots status` → removed entirely (the JSON envelope is the only stdout output)
- `println!` in the slots-release confirmation → `tracing::info!` (purely diagnostic, no envelope needed)

This guarantees:

- `sqlite-graphrag slots status` now emits the same JSON envelope shape as every other command (consistent with ADR-0040's stderr capture fallback chain)
- Operators that pipe `... 2>err.log` see slots diagnostics once on stderr (per Fix 1's pattern)
- `RUST_LOG=warn` quiets slot diagnostics; `RUST_LOG=debug` keeps them visible

## Consequences

### Positive

- stderr emits EXACTLY 1 line per OAuth-only enforcement violation (validated by `oauth_stderr_emits_single_line_v1088` integration test)
- stderr emits EXACTLY 1 line per slot-acquire / slot-release event (validated by `slots_no_println_integration` integration test, `slot_status_emit_info_not_println`)
- `sqlite-graphrag slots status --json` returns a parseable JSON envelope end-to-end (validated by `slots_status_returns_parseable_json`)
- `RUST_LOG` / `--log-level` consistently govern slot diagnostics
- 1 fewer syscall per OAuth violation (`eprintln!` performs a `write(2)` syscall that is now eliminated)

### Negative

- Operators who relied on the doubled stderr lines (parsing both lines of a violation) must update their log parsers to expect 1 line. Mitigation: the surviving line contains both the structured `code` and the human-readable `msg`, so no information is lost.
- Scripts that grepped `slots.rs` output for `acquired` or `released` substrings continue to work because the diagnostic text is preserved verbatim in `emit_info`.

## Alternatives Considered

1. Use `tracing::error!` only, do not touch `slots.rs` — REJECTED: leaves GAP-15 unaddressed; slots.rs would continue producing free-form stdout.
2. Route `slots.rs` output to a separate file (`--slots-log`) — REJECTED: adds a new flag and a new concept without addressing the duplication.
3. Use `eprintln!` in slots.rs (matching the pre-fix output.rs pattern) — REJECTED: doubles stderr noise the opposite direction.
4. Replace both `println!` AND `eprintln!` globally with `emit_info` / `emit_error` only — DEFERRED: out of scope for v1.0.88; tracked as tech debt for v1.0.89 audit pass.

## Cross-references

- ADR-0011 (OAuth-only enforcement — the violation that BUG-12 doubled on stderr)
- ADR-0040 (stderr capture fallback chain — orthogonal but adjacent: this ADR ensures 1 line per error, ADR-0040 ensures the line survives `2>` redirection)
- ADR-0037 (shutdown JSON envelope — defines the JSON envelope shape that slots.rs now also emits)
- ADR-0045 (preflight validation layer — preflight failures also flow through `output::emit_error`, so Fix 1 also deduplicates preflight error lines)
- ADR-0046 (preflight remediation — BUG-12 fix is included in that ADR's consequences; this ADR is the canonical reference for the stderr deduplication decision)
- `src/output.rs:141` (`emit_error` after Fix 1)
- `src/commands/slots.rs` (5 `println!` sites replaced)
- `tests/oauth_stderr_emits_single_line_v1088.rs` (regression test for BUG-12)
- `tests/slots_no_println_integration.rs` (regression test for GAP-15)

## Non-goals (YAGNI)

- NO introduction of a structured stderr format (current `tracing::error!` formatter is sufficient)
- NO removal of `eprintln!` from other call sites outside `output::emit_error` (each is reviewed independently)
- NO change to the `RUST_LOG` semantics
- NO new flag for stderr verbosity

## Next steps

- v1.0.89: audit pass over remaining `eprintln!` / `println!` sites for the same pattern
- v1.0.89: ADR for structured stderr format (JSON-per-line) if downstream log parsers demand it
