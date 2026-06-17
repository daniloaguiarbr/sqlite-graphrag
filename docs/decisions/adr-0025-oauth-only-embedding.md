# ADR-0025: OAuth-Only LLM Credential Flow (v1.0.76 — inherited from v1.0.69)

- Status: Accepted (reaffirmed 2026-06-07)
- Update (v1.0.79): the `embedding-legacy` escape hatch mentioned below was removed ahead of the v1.1.0 schedule; the transition window is closed
- Deciders: Danilo Aguiar
- Scope: src/extract/llm_embedding.rs, src/commands/claude_runner.rs, src/commands/codex_spawn.rs

## Context

ADR-0011 (v1.0.69) mandated OAuth-only credential flow for the
claude / codex CLIs. v1.0.76 is the first release where OAuth is
the ONLY supported flow for the embedding client as well, because
the embedding client now spawns claude / codex directly (no fastembed
fallback).

The hardening flags from v1.0.69 are preserved:

For `claude code` (7 flags):

```
--strict-mcp-config
--mcp-config '{}'
--settings '{"hooks":{}}'
--dangerously-skip-permissions
--output-schema '{"type":"object","properties":{"embedding":{...}},"required":["embedding"],"additionalProperties":false}'
--model claude-sonnet-4-6
-p <prompt>
```

For `codex` (7 flags + the ChatGPT Pro OAuth whitelist):

```
--json
--output-schema '{"type":"object",...}'
--ephemeral
--skip-git-repo-check
--sandbox read-only
--ignore-user-config
--ignore-rules
-c mcp_servers='{}'
--ask-for-approval never
--model gpt-5.4
```

OAuth is enforced by the `oauth_only_enforce()` check in
`src/extract/llm_embedding.rs`, which ABORTS with
`AppError::Validation` if `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`
is in the environment. The two API-key env vars are also excluded
from the env-clear whitelist in `claude_runner::build_claude_command`
and `codex_spawn::build_codex_command`, so even a parent process
that exports them cannot bypass the check.

## Consequences

### Positive

- The CLI cannot be tricked into sending embeddings or extracted
  content to a third-party endpoint by an attacker who controls
  the `ANTHROPIC_API_KEY` env var. OAuth credentials are tied to
  the user's own subscription (Claude Pro/Max or ChatGPT Pro).
- The OAuth flow provides per-request billing visibility (the
  user's account page shows every LLM round-trip), so operators
  can audit their own LLM spend.
- The hardening flags make the LLM subprocess deterministic
  (no MCP servers, no hooks, no user config) so embedding
  responses are reproducible across hosts.

### Negative

- Operators who want to use a different LLM provider (Azure
  OpenAI, Bedrock, local Ollama) cannot do so without modifying
  `src/extract/llm_embedding.rs` to add a new spawn path. This
  is intentional; the v1.0.76 build is committed to claude and
  codex only.
- Operators with no internet access on the host running
  sqlite-graphrag cannot use the LLM backend. The
  `embedding-legacy` feature restores the local fastembed
  pipeline for offline use during the transition window.

## Verification

- `cargo test --lib extract::llm_embedding::tests::oauth_only_enforce_blocks_api_keys`:
  green — the env var check fires when either key is set.
- `cargo test --lib extract::llm_embedding::tests::flavour_as_str_is_stable`:
  green — the EmbeddingFlavour enum serializes correctly.
- `claude_runner.rs::tests::*` and `codex_spawn.rs::tests::*`:
  the canonical 7-flag set is preserved across all spawn paths.

## Related Decisions

- ADR-0011 — OAuth-only mandate (the policy this ADR reaffirms at the
  embedding layer).
- ADR-0041 — Custom Provider Credential Preservation (v1.0.83). The
  OAuth-only guard in `extract/llm_embedding.rs:237-253` still
  rejects `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`; ADR-0041 extends the
  env-clear whitelist so legitimate custom-provider vars
  (`ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`)
  reach the subprocess. The two ADRs compose: ADR-0011/0025
  reject the paid API keys; ADR-0041 preserves the OAuth tokens
  and base-URL overrides for Anthropic-compatible providers like
  Minimax, OpenRouter, AWS Bedrock, and corporate gateways.
