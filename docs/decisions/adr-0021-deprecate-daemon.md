# ADR-0021: Deprecation of `daemon` Command (v1.0.76)

- Status: Accepted (2026-06-07)
- Deciders: Danilo Aguiar
- Scope: src/daemon.rs, src/commands/daemon.rs, src/main.rs, src/cli.rs

## Context

The `daemon` subcommand (`sqlite-graphrag daemon`) was introduced in
v1.0.21 to keep the fastembed model loaded in memory across CLI
invocations. The model load was ~30 s on a cold ONNX cache; spawning a
fresh CLI per `remember` would pay that cost every time.

In v1.0.76, the fastembed model is gone. The embedding is produced by
spawning a headless LLM subprocess per call. The subprocess lifetime is
~1-3 s, and there is no model to "keep loaded" — every call is a fresh
LLM round-trip.

## Decision

The `daemon` subcommand is **deprecated** but kept for source compatibility
through the v1.0.76 → v1.1.0 transition window. The CLI no longer uses it
internally:

- `embed_passage_or_local` and `embed_query_or_local` still consult the
  daemon (if one happens to be running), but the daemon's
  `EmbedPassage` handler now spawns a fresh LLM subprocess per call.
  The overhead of the daemon socket round-trip is comparable to the
  LLM spawn, so the daemon no longer provides a meaningful speedup.
- The autostart path still tries to spawn a daemon, but the daemon's
  embedding is the same LLM call the client would make directly, so
  the daemon is now an unnecessary intermediary.
- `daemon --stop`, `daemon --ping`, and `daemon` (default autostart)
  all still work, but they no longer offer any performance benefit.

The `daemon` subcommand is **REMOVED in v1.1.0**.

## Consequences

### Positive

- The CLI is now a true one-shot: no process to keep alive, no socket
  to clean up, no state to inspect.
- The 60+ second incident from 2026-06-03 (load average 276 caused by
  4 `enrich` × 2 workers × 10 MCP servers = 192 processes) is
  structurally impossible. There is no process tree to proliferate.
- New users do not need to learn about the daemon, the singleton
  lock, the version-mismatch auto-restart, or any of the other
  long-running-process complexity. The CLI is just a `cli` now.

### Negative

- Operators with custom embeddings or local-only inference (no LLM CLI
  available) lose the daemon as a fallback. The `embedding-legacy`
  feature restores the v1.0.74 behavior for the transition window.
- The `daemon` subcommand and its IPC protocol remain in the source
  tree (~600 lines) until v1.1.0. They are no-ops for new code paths.

## Verification

- `daemon --stop` still works.
- `daemon --ping` returns a healthy response when the daemon is up.
- `daemon` (default autostart) still spawns and the embedding request
  round-trips correctly when the LLM CLI is on PATH.
- `tests/v1044_features::related_entity_seed_via_link_succeeds` and
  the other daemon-roundtrip tests pass when an LLM CLI is available
  in the test environment. They fail with "no LLM CLI found on PATH"
  when neither `claude` nor `codex` is installed; this is
  documented in CHANGELOG v1.0.76.
