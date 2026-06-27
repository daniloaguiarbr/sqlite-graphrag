# ADR-0054 — OpenRouter chat transport for `enrich`

**Status**: Accepted
**Date**: 2026-06-27
**Context**: sqlite-graphrag v1.0.95 — GAP-OR-ENRICH

## Problem

`enrich` runs a SCAN→JUDGE→PERSIST pipeline where the JUDGE is an LLM
that returns structured JSON. Through v1.0.94 the JUDGE had only three
transports — `claude-code`, `codex`, `opencode` — and each one resolves
to a `Command::new` spawn of a locally installed, locally authenticated
CLI. There was no REST transport for the JUDGE at all.

Meanwhile, embeddings had already moved to a REST client
(`src/embedding_api.rs`, ADR-0052/0053): `remember`/`recall`/`ingest`
embed against OpenRouter `/embeddings` with no local subprocess. This
left an asymmetry documented in `gaps.md` — embedding has a REST path,
enrichment does not. Concretely:

1. **CLI dependency** — every `enrich` run required one of three CLIs
   installed and OAuth-authenticated on the host, blocking headless or
   container environments that have an API key but no CLI.

2. **No model choice** — the JUDGE model was whatever the spawned CLI
   defaulted to; users could not pick a specific text model.

3. **Fragile output** — the JUDGE answer was parsed out of the
   subprocess stdout, and per-item cold-start added latency.

The only OpenRouter HTTP client (`OPENROUTER_EMBEDDINGS_URL` in
`src/embedding_api.rs`) targets `/embeddings` exclusively; no client
existed for `/chat/completions`.

## Decision

Add a fourth `enrich` transport, `--mode openrouter`, that routes the
JUDGE to the OpenRouter `/chat/completions` REST endpoint. The
SCAN→JUDGE→PERSIST logic is untouched; only the JUDGE transport changes.

### New module `src/chat_api.rs`

`OpenRouterChatClient` mirrors `src/embedding_api.rs`: it holds a
`reqwest::Client`, a `secrecy::SecretBox<String>` API key (zeroize on
drop, never logged), and the bound model name. `complete(system_prompt,
input_text, schema_str, max_tokens)` runs one structured-output
completion and returns `(serde_json::Value, cost_usd, is_oauth)`.

- `response_format` is `json_schema` with `strict: true`. The reused
  per-operation schemas (`BINDINGS_SCHEMA`, `ENTITY_DESCRIPTION_SCHEMA`,
  etc.) are runner-agnostic and carry the contract.
- `provider.require_parameters: true` routes only providers that honour
  the schema, so a provider that silently drops `response_format` is
  excluded rather than returning unconstrained text.
- `reasoning.enabled: false` disables reasoning for extraction to cut
  paid tokens and latency. Because reasoning-mandatory support varies by
  model, a graceful fallback wraps this: `complete()` tries `enabled:
  false` first, and if the provider rejects it (HTTP 400 mentioning
  `reasoning`, detected by helper `reasoning_disable_rejected`) it retries
  ONCE omitting the `reasoning` field so the model uses its mandatory
  default. 9 of the 13 tested models accept `enabled: false`; 4 require
  the fallback. The deprecated `usage: {include:true}` parameter is NOT
  sent — the `usage` object (with `cost`) already comes back automatically.
- Two parses happen: the HTTP body into `ChatResponse`, then the
  `choices[0].message.content` string into the final JSON value. Empty
  content or a non-JSON body under a strict schema is reported as an
  explicit "model incompatible with structured outputs" error naming the
  model.
- `cost_usd` is read from `usage.cost` (or `0.0` when absent) and summed
  into the run total. `is_oauth` is always `false` because OpenRouter
  uses an API key, not OAuth.
- Retry/backoff is identical to the embeddings client: immediate abort
  on 401/400/404, `retry-after` on 429, exponential backoff + jitter on
  5xx and 200-with-parse-failure. Headers are minimal — only
  `Authorization: Bearer`, no `HTTP-Referer`/`X-Title`.

### `enrich` wiring (`src/commands/enrich.rs`)

`EnrichMode` gains an `OpenRouter` variant (Display `"openrouter"`).
New flags: `--openrouter-model` (REQUIRED for this mode),
`--openrouter-api-key` (env `OPENROUTER_API_KEY`), `--openrouter-timeout`,
`--openrouter-base-url`. `validate_mode_flags` rejects cross-mode flags
(claude/codex/opencode flags under `--mode openrouter`). The preflight
probe validates only the API key for this mode — no subprocess spawn.
Each of the JUDGE dispatch arms gains an `OpenRouter => call_openrouter(...)`
branch; `call_openrouter` is a sync wrapper that drives
`client.complete(...)` through `shared_runtime()?.block_on(...)` and
returns the same `(Value, f64, bool)` tuple as the other three runners.

### Reused infrastructure

`resolve_api_key("openrouter", cli)` (env > config > CLI precedence),
`shared_runtime()` (sync→async bridge), and the `OnceLock` singleton
pattern from `OPENROUTER_CLIENT` are reused verbatim. A new
`OPENROUTER_CHAT_CLIENT` singleton in `src/embedder.rs` mirrors
`get_openrouter_embedder`. No new dependency — `reqwest`, `secrecy`,
`tokio`, `serde` are already present.

## Alternatives Considered

### A. Add a generic HTTP JUDGE mode (any OpenAI-compatible endpoint)

Rejected (YAGNI). The gap is specifically OpenRouter parity with the
embeddings path. A generic endpoint flag would duplicate auth/retry
surface without a current second backend to justify it; OpenRouter
already fronts every model the user listed.

### B. Default `--openrouter-model` to a known-good text model

Rejected per explicit user constraint. `--openrouter-model` is REQUIRED;
absence returns an `AppError::Validation` (exit 1) before any network
call. A default would silently pick a model whose structured-outputs
support and cost the user did not choose.

### C. Parse `usage` via a second usage-accounting request

Rejected. `usage: {include:true}` is deprecated and the `usage` object
with `cost` already arrives on the chat response; a second call would add
latency and cost for data already in hand.

### D. Enable reasoning and exclude it from the output

Rejected for extraction. `reasoning.enabled: false` is cheaper and
faster; `{exclude:true}` would still bill reasoning tokens. Models with
`reasoning.mandatory: true` reject the disable; rather than erroring,
`complete()` retries once omitting `reasoning` (see Decision) so the model
uses its mandatory default — turning the 9/13 that accept `enabled: false`
into 13/13 compatible.

## Consequences

- Positive: `enrich` runs with no local CLI installed or authenticated —
  an API key alone suffices, unblocking headless and container use.
- Positive: the user picks the exact JUDGE text model via
  `--openrouter-model`, with a 13-model compatibility matrix exercised
  E2E.
- Positive: `strict` Structured Outputs yields reliable JSON without
  fragile stdout parsing, and `usage.cost` gives real per-item cost in
  one request.
- Negative: tokens are paid against the user's `OPENROUTER_API_KEY`,
  unlike the OAuth-free local CLI modes — the trade-off is convenience
  and headless reach versus per-token billing.
- Negative: `json_schema` support varies by provider; a model without
  structured outputs fails with an explicit OpenRouter error.
  `reasoning.mandatory` is NOT a failure — the fallback in the Decision
  absorbs it. The real 13-model test (13/13 pass: 9 with `enabled: false`,
  4 via the fallback) is the only reliable proof of which models are
  production-safe.
- The existing three modes are unchanged; `--mode` remains required
  (ADR-0053), now with `openrouter` as a fourth valid value.

## Validation

- Build: `cargo build --release` 0 errors; `cargo clippy --all-targets
  --all-features -- -D warnings` 0 warnings; `cargo fmt --all --check`
  0 diffs; `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` 0 warnings.
- Unit: `wiremock::MockServer` tests for request assembly
  (`response_format`, `provider.require_parameters`, `reasoning`),
  response parse + second JSON parse, `usage.cost` read, retry (429
  `retry-after`, 5xx backoff, 401 permanent), 400/404 no-retry, empty
  content as incompatible; `validate_mode_flags` cross-flag rejection;
  `--openrouter-model` required (exit 1).
- Real API: `tests/openrouter_chat_real.rs` (`#[ignore]`) iterates the 13
  listed text models against the strict `BINDINGS_SCHEMA`. Matrix: 13/13
  pass — 9 accept `reasoning.enabled: false`, 4
  (`minimax/minimax-m2.7[:nitro]`, `openai/gpt-oss-120b[:nitro]`) require
  the reasoning-mandatory fallback.

## Cross-references

- `gaps.md` — GAP-OR-ENRICH marked RESOLVIDO em v1.0.95
- ADR-0053 (v1.0.94 four-gap remediation) — made `enrich --mode` required
- ADR-0052 (OpenRouter embedding backend) — the embeddings REST client
  this module mirrors
- `src/chat_api.rs` (`OpenRouterChatClient`), `src/commands/enrich.rs`
  (`EnrichMode::OpenRouter`, `call_openrouter`, flag validation),
  `src/embedder.rs` (`OPENROUTER_CHAT_CLIENT` singleton,
  `resolve_api_key`, `shared_runtime`), `src/embedding_api.rs` (mirrored
  retry/backoff)
