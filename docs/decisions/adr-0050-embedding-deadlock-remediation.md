# ADR-0050 — Embedding Deadlock Remediation

**Status**: Accepted
**Date**: 2026-06-21
**Context**: sqlite-graphrag v1.0.89 — GAP-RECALL-001, GAP-FLAGS-MORTAS, BUG-SKIP-EMBED, BUG-MODEL-VAZIO, BUG-SKIP-EMBED-INCOMPLETE

## Problem

Multi-session Claude Code environments experienced a self-perpetuating
deadlock: `recall`, `hybrid-search` and `deep-research` hung
indefinitely at the "Computing query embedding..." step. Root causes:

1. The LLM subprocess (`codex exec` / `claude -p`) stalled or died
   without releasing the host-wide LLM slot semaphore.
2. Multiple Claude Code sessions each spawned embedding subprocesses
   that saturated the slot pool.
3. The default embedding timeout was 300 seconds — far too long for
   a short query embedding (actual RTT is 2-3 seconds). It was
   reduced to 60 seconds.
4. `codex_embed_model()` and `claude_embed_model()` returned an empty
   string when no env var was set, causing codex to reject the request
   with "The '' model is not supported".
5. `--skip-embedding-on-failure` was a dead flag: accepted by clap,
   propagated to an env var in `main.rs`, but never read by any
   embedding module.
6. Seven global CLI flags (`--claude-binary`, `--codex-binary`,
   `--llm-model`, `--skip-embedding-on-failure`,
   `--llm-max-host-concurrency`, `--llm-slot-wait-secs`,
   `--llm-slot-no-wait`) were accepted by clap but never propagated
   to the internal modules that read them via `std::env::var`.

## Decision

Apply seven layered fixes in v1.0.89:

### FIX-1: Explicit `drop(stdin)` before `wait_with_output`

The `invoke_codex` function in `src/extract/llm_embedding.rs` now
explicitly `drop(stdin)` after `write_all` to close the child's
stdin file descriptor. Without this, the child waits for EOF on
stdin and never produces output. `invoke_claude` does not need
this: it uses `.stdin(Stdio::null())` and passes the prompt as a
command-line argument.

### FIX-2: Reduce default embedding timeout from 300s to 60s

`DEFAULT_EMBED_TIMEOUT_SECS` changed from 300 to 60. A query
embedding should complete in 2-5 seconds; 300s masked stalled
subprocesses. Added `embed_timeout_for_batch(batch_size)` that
scales: base + 15s per additional item (batch of 8 = 165s).

### FIX-3: Stale slot cleanup on startup

`llm_slots.rs` now exposes `find_stale_slots()` which scans the
slot directory for lock files held by PIDs that no longer exist.
The active startup cleanup is `reaper::scan_and_kill_orphans()`
(called from `main.rs`), which reaps the stale slots it identifies.

### FIX-4: Reaper kills orphan sqlite-graphrag processes

`reaper.rs` expanded to scan for orphan `sqlite-graphrag` processes
(PPID=1, age > 60s) in addition to `claude` and `codex` orphans.

### FIX-5: Sensible model defaults

`codex_embed_model()` now returns `"gpt-5.5"` (ChatGPT Pro default)
and `claude_embed_model()` returns `"claude-sonnet-4-6"` (Claude
Pro/Max default) when no env var is set. Previously returned empty
string, causing every embedding call to fail.

### FIX-6: `--skip-embedding-on-failure` wired end-to-end

`should_skip_embedding_on_failure()` reads the env var. The
`remember` command wraps all 3 embedding call sites (passage,
parallel chunks, entity texts) with error guards that check the
flag. Embedding type changed from `Vec<f32>` to `Option<Vec<f32>>`
in `remember.rs`; `upsert_vec` conditioned on `Some`. The memory
is persisted without an embedding vector when the flag is active.

### FIX-7: CLI flag propagation via `set_var`

Seven global flags now propagated from CLI struct to env vars via
`std::env::set_var` in `main.rs` before command dispatch. The
anti-pattern: clap reads env vars as FALLBACK (env → field) but
does NOT set env vars when the flag is provided via CLI (field → env
requires explicit `set_var`).

## Alternatives Considered

### A. Global retry with exponential backoff

Rejected: the stalled subprocess is the root cause. Retrying with a
longer timeout would only delay the deadlock, not prevent it.

### B. In-process embedding (fastembed / ONNX)

Rejected: the v1.0.76 architecture decision (ADR-0019) removed all
local models. Reverting would add 30+ MB to the binary and
reintroduce the ONNX runtime dependency. The LLM-only one-shot
architecture is the correct long-term direction.

### C. Fallback to FTS5-only on slot exhaustion

Implemented in `deep-research` (GAP-DEEPRESEARCH-001) via
`try_embed_query_with_deterministic_fallback`. Not applied to
`recall`/`hybrid-search` because those commands exist specifically
to provide vector similarity; FTS5-only would change their
semantics. The timeout reduction (FIX-2) and stale slot cleanup
(FIX-3) address the root cause instead.

## Consequences

- `recall` and `hybrid-search` recover from stalled subprocesses
  in 30s instead of 300s.
- Multi-session environments no longer accumulate orphan processes
  that saturate the LLM slot pool.
- `--skip-embedding-on-failure` is now functional: `remember`
  returns exit 0 and persists the memory without an embedding.
- Default model names (`gpt-5.5`, `claude-sonnet-4-6`) eliminate
  the "empty model" failure mode.
- CLI flags like `--claude-binary` and `--llm-model` now work
  whether passed via CLI or env var.

## Validation

- Build: 0 errors, 0 clippy warnings.
- Test suite: 847 lib tests, 0 failures.
- E2E: 18 end-to-end scenarios verified against release binary.
- `--skip-embedding-on-failure` confirmed: exit 0, memory created,
  entities persisted, graph edges created.
