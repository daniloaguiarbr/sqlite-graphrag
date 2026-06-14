# ADR-0030 — A1 Audit: Structured Panic Hook Replaces Default stderr Dump (v1.0.80)

## Status

Accepted (v1.0.80, 2026-06-14).

## Context

The v1.0.80 audit suite (A1 audit cycle, scope: telemetry
and observability) identified that the default Rust panic
hook prints the panic payload and location to stderr, which
combined with a `tracing::error!` event produces a
double-trace (one structured event in JSON or pretty, one
unstructured dump to stderr). For log aggregators that
parse JSON output, the unstructured dump is unparseable
and obscures the structured event. For pretty-mode
human readers, the double-trace is visually noisy.

## Decision

The panic hook installed in `src/telemetry.rs:47-72`
(via `std::panic::set_hook` during tracing init) emits a
single structured `tracing::error!` event with the panic
payload and location, and DELIBERATELY does NOT call the
previous hook. The default Rust panic hook is therefore
replaced for the lifetime of the process. Test runs still
fail on panic because Rust aborts the process regardless of
which hook is installed, so existing `#[should_panic]`
tests and `cargo test` invariants are unaffected.

The hook handles two payload types (`&str` and `String`),
falls back to a `<non-string panic>` marker for other
payloads, and resolves `info.location()` to a
`file:line:column` string. The location is rendered as
`unknown` when not available (e.g., panics in optimized
builds where the location is elided).

## Consequences

Positive:

- Log aggregators that parse JSON output see exactly one
  structured `tracing::error!` event per panic, with the
  same payload and location fields as the previous
  unstructured dump.
- Pretty-mode human readers see one formatted line per
  panic instead of a doubled trace.
- The hook is installed during tracing init, so any panic
  that occurs BEFORE tracing init still uses the default
  hook (acceptable: these panics occur in the tiny
  startup window before observability is enabled).

Negative:

- The default Rust panic hook is replaced for the
  lifetime of the process; tools that rely on the
  default hook's stderr output format (e.g., `rustc`'s
  `--error-format=human`) see the structured event instead
  of the human-formatted dump. This is acceptable
  because the project uses its own log format and the
  CLI is the canonical interface.
- `cargo test` panics produce one structured event per
  panic in test output; CI pipelines that grep for the
  default hook's `thread 'foo' panicked at` pattern
  must update their regexes to the structured event
  format.

## References

- `src/telemetry.rs:47-72` (panic hook implementation)
- G28 (CLI process lifecycle governance)
- A1 audit cycle (v1.0.80, scope: telemetry)

