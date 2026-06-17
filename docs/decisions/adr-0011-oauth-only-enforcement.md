# ADR-0011 — OAuth-Only Enforcement (v1.0.69)

- **Status.** Accepted.
- **Date.** 2026-06-05.
- **Deciders.** Danilo Aguiar (operator), Claude Code (advisor).
- **Supersedes.** None.
- **Related gaps.** G28-A (proliferação de MCP), gaps.md lines 41-49 (Regras Invioláveis de Invocação Headless).

## Context

The original `enrich` and `ingest --mode {claude-code,codex}` flow passed a half-dozen hardening flags but allowed two PROHIBITED paths:

1. The `claude_runner::build_claude_command` function had a `if ANTHROPIC_API_KEY.is_ok() { cmd.arg("--bare") }` branch. Per gaps.md:49, `--bare` is FORBIDDEN because it disables OAuth and demands `ANTHROPIC_API_KEY` (the very thing the project prohibits).
2. The `codex_spawn::build_codex_command` whitelist explicitly included `OPENAI_API_KEY`, passing any API key from the environment straight to the child. Per gaps.md:48, `OPENAI_API_KEY` is FORBIDDEN in the spawn environment of any `codex exec`.

The two code paths had drifted; `ingest_claude.rs` and `claude_runner.rs` maintained duplicate `ENV_WHITELIST` arrays, and `ingest_claude.rs:325` had the same forbidden `if ANTHROPIC_API_KEY { --bare }` branch.

A re-read of gaps.md lines 41-49 (the four PROIBIÇÕES ABSOLUTAS on Claude/Codex headless invocation) and the operator's explicit "é proibido usar claude code headless com api" prompt revealed the inconsistency. Three call-sites had to be aligned and the API-key path had to become a hard error.

## Decision

1. The OAuth-only guard is mandatory in EVERY spawn helper. The guard returns an `AppError::Validation` and a `/usr/bin/false` command carrying a `--oauth-only-violation-*` marker when the prohibited environment variable is present.
2. `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` are INTENTIONALLY ABSENT from the `env_clear` whitelists. Defence-in-depth: even if a future refactor moves the guard, the variable never reaches the child.
3. The `--bare` flag is REMOVED from all executable code. It appears only in documentation explaining why it is forbidden.
4. Every spawn helper always passes the canonical hardening flag set documented in gaps.md:201-208 (claude) and 233-238 (codex).
5. Four new tests (`#[serial_test::serial(env)]`) validate the canonical flag set and the abort behaviour.

## Consequences

- Operators using API keys (a small minority) must migrate to OAuth. The error message is actionable and points at the OAuth login flow.
- The four tests run in the serial `env` group to avoid races on the global environment. Total test runtime increase: 0.04s.
- The marker `--oauth-only-violation-{anthropic,openai}-api-key-set` makes spawn failures self-documenting in CI logs.
- The `ENV_WHITELIST` arrays are now in two places (claude + codex). A future refactor should extract `whitelist_env_clear` into a shared helper. Filed as a follow-up.

## Related Decisions

- ADR-0041 — Custom Provider Credential Preservation (v1.0.83).
  This ADR-0011 follow-up was filed in 2026-06-17 and RESOLVES
  the helper-extraction follow-up via `src/spawn/env_whitelist.rs`.
  The shared `apply_env_whitelist(cmd, strict)` helper unifies
  the three duplicated spawners (`claude_runner`, `codex_spawn`,
  `ingest_claude`) and extends the whitelist to preserve the
  custom-provider vars (`ANTHROPIC_AUTH_TOKEN`,
  `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`,
  `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`,
  `OTEL_EXPORTER_OTLP_ENDPOINT`) while keeping the OAuth-only
  guard from this ADR-0011 intact. The two ADRs compose: this
  one rejects `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`; ADR-0041
  preserves the OAuth tokens and base-URL overrides used by
  Anthropic-compatible providers (Minimax, OpenRouter, AWS
  Bedrock, corporate gateways).
- ADR-0025 — OAuth-Only Embedding (v1.0.76). Reaffirms this
  ADR-0011 at the `extract/llm_embedding.rs` layer.

## Alternatives Considered

- Keep the API-key path with a warning. REJECTED. gaps.md:47,48,49 are PROIBIÇÕES ABSOLUTAS. Warnings do not satisfy absolute prohibitions.
- Read the OAuth token from the environment via `OAUTH_TOKEN`. REJECTED. Claude Code reads the OAuth token from `~/.claude/.credentials.json` (or OS keychain); Codex reads from `~/.codex/auth.json`. The OAuth flow does not pass tokens through the environment.

## References

- `src/commands/claude_runner.rs:222-303` (canonical command and OAuth-only guard).
- `src/commands/codex_spawn.rs:205-279` (canonical command, `-c mcp_servers='{}'` flag, OAuth-only guard).
- `src/commands/ingest_claude.rs:255-340` (extract helper aligned with `claude_runner`).
- `src/commands/claude_runner.rs:574-666` (four `#[serial_test::serial(env)]` tests).
- `src/commands/codex_spawn.rs:684-758` (four `#[serial_test::serial(env)]` tests).
- gaps.md lines 41-49 (Regras Invioláveis).
- gaps.md lines 201-208 (claude canonical command).
- gaps.md lines 233-238 (codex canonical command).
