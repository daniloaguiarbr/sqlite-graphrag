Read this document in [Portuguese (pt-BR)](SECURITY.pt-BR.md).


# Security Policy


## Supported Versions
- The table below lists which sqlite-graphrag versions currently receive security patches
- Users on deprecated lines are STRONGLY encouraged to upgrade to a supported release
- Upgrading early reduces exposure window and aligns with the coordinated disclosure policy

| Version | Status      | Security Patches         |
| ------- | ----------- | ------------------------ |
| 1.0.x   | Supported   | Yes, receives fixes      |
| 0.x     | Unsupported | No patches provided      |


## Reporting a Vulnerability
- OBRIGATÓRIO report security issues through GitHub Security Advisories in the public `sqlite-graphrag` repository as the preferred private channel
- Use email at daniloaguiarbr@gmail.com only as fallback when GitHub private reporting is unavailable
- JAMAIS open a public GitHub issue, pull request, or discussion for security-related reports
- Include a minimal reproduction, affected versions, and expected versus actual behavior
- Include your environment details such as OS, architecture, and rustc version
- Include CVSS 3.1 severity estimate when possible to accelerate triage


## Response SLA
- Triage of every advisory is committed to start within 72 business hours of submission
- Initial acknowledgment email will be sent within that same 72-hour window
- You will receive a case identifier and an assigned maintainer contact
- Progress updates are shared at minimum every 7 days until resolution or public disclosure


## Fix SLA by CVSS Severity
- Critical severity (CVSS 9.0 to 10.0) receives a patch within 7 calendar days of validated triage
- High severity (CVSS 7.0 to 8.9) receives a patch within 14 calendar days of validated triage
- Medium severity (CVSS 4.0 to 6.9) receives a patch within 30 calendar days of validated triage
- Low severity (CVSS 0.1 to 3.9) receives a patch within 90 calendar days of validated triage
- Released fixes follow immediately with a CHANGELOG entry and a GitHub Security Advisory when the affected line is still supported


## Disclosure Policy
- We follow coordinated disclosure with a standard 90-day embargo window from initial report
- The embargo can be shortened when a fix is released earlier than 90 days
- The embargo can be extended when a fix demands more time and the reporter agrees
- Public disclosure includes a CVE identifier when the impact warrants one
- Public disclosure includes the GitHub Security Advisory with affected versions and patched version
- Credit is attributed to the reporter unless anonymity is explicitly requested


## Security Update Policy
- Patches for supported versions ship as a new patch release on crates.io and GitHub Releases
- Every release is validated with the full 10-command quality gate described in CONTRIBUTING
- CI runs `cargo audit` and `cargo deny check advisories licenses bans sources` on every push
- Supply chain is enforced via pinned `constant_time_eq = "=0.4.2"` to protect MSRV 1.88
- Transitive dependency MSRV drift is monitored proactively per PRD policy

## v1.0.76 OAuth-Only LLM Credential Enforcement
- The default build is LLM-only and one-shot. Every embedding call spawns a headless `claude code` or `codex` subprocess.
- The spawn ABORTS with `AppError::Validation` and exit code 1 when `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is detected in the environment.
- The OAuth flow (Claude Pro/Max or ChatGPT Pro subscription) is the ONLY accepted credential mechanism.
- Both API-key env vars are INTENTIONALLY ABSENT from the env-clear whitelist in `claude_runner.rs`, `codex_spawn.rs`, and `ingest_claude.rs`. Defence in depth: even if a future refactor moves the OAuth-only guard, the variable never reaches the child.
- The `--bare` flag (which would also demand an API key) is REMOVED from every executable path since v1.0.69.
- Four `#[serial_test::serial(env)]` tests validate the canonical flag set and the abort behaviour.
- See `docs/decisions/adr-0011-oauth-only-enforcement.md` for the full rationale and `docs/decisions/adr-0025-oauth-only-embedding.md` for the v1.0.76 embedding-specific application.
- Migration: any operator currently relying on `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` must migrate to OAuth before upgrading.

## v1.0.83 Custom Provider Credential Preservation (ADR-0041)
- The default build now PRESERVES seven custom-provider env vars when spawning `claude -p` or `codex exec` subprocesses, enabling Anthropic-compatible providers (MiniMax/api.minimax.io, OpenRouter, AWS Bedrock, corporate gateways)
- The preserved vars are `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CODEX_ACCESS_TOKEN`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, and `OTEL_EXPORTER_OTLP_ENDPOINT`
- These vars are SEMANTICALLY DISTINCT from the OAuth-only-rejected `ANTHROPIC_API_KEY` and `OPENAI_API_KEY`; the OAuth-only guard in `claude_runner.rs`, `codex_spawn.rs`, and `ingest_claude.rs` continues to reject the API keys with exit 1 (defence in depth preserved)
- The whitelist now lives in a single shared helper `src/spawn/env_whitelist.rs` exposing `apply_env_whitelist(cmd, strict)` and `is_strict_env_clear()`; the three spawners delegate instead of duplicating the inline array
- For compliance environments that forbid credential forwarding via env vars (PCI-DSS, SOC2, HIPAA), operators can set `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` or pass `--strict-env-clear`; strict mode preserves only `PATH` and drops every other env var
- Five `#[serial_test::serial(env)]` regression tests live in `tests/claude_runner_env.rs` covering custom-provider propagation, OAuth-only abort preservation, codex base-URL inheritance, strict-mode credential dropping, and a no-leak audit that scans subprocess stderr for the literal token value with `RUST_LOG=trace`
- No telemetry is emitted; the fix is silent unless the OAuth-only guard fires (which surfaces an orientative marker arg pointing to `ANTHROPIC_AUTH_TOKEN` or `~/.codex/auth.json` as legitimate resolutions)
- Threat model: credential values for custom providers flow from the orchestrator process to the LLM subprocess over the process boundary. The audit-of-no-leak test prevents future regressions where a `tracing` macro might print the raw token to stderr. Operators on shared hosts should prefer `--strict-env-clear` to avoid forwarding secrets
- See `docs/decisions/adr-0041-preserve-custom-provider-env.md` (EN) and `.pt-BR.md` for the full architectural decision and the alternatives considered

## v1.0.87+ Pre-flight Validation Layer (ADR-0045)
- Every LLM subprocess spawn passes through src/spawn/preflight.rs (15 unit tests, 7 guards) BEFORE the fork.  Failures return AppError::PreFlightFailed (exit code 16, EX_CONFIG).
- 7 guards: check_argv_size, check_binary_exists, check_mcp_config_inline (replaces literal --mcp-config '{}' with tempfile, fixes BUG-2), check_mcp_config_path, check_walkup_mcp_json (validates .mcp.json walk-up, fixes BUG-5), check_output_buffer (fixes BUG-4), check_claude_config_dir (avoids MCP bleed-through).
- Bypass: SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1 disables all 7 guards.  Last-resort opt-out; bypassing reverts to direct Command::spawn() and inherits all 5 BUG classes.
- v1.0.88 hotfixes: BUG-11 (preflight failure in extract/llm_embedding.rs did not propagate to remember; fixed with embed_via_backend_strict), BUG-12 (OAuth-only emitted 2 identical stderr lines; fixed with single-line stderr), BUG-13 (link --create-missing bypassed entity-name validation; fixed by validating BEFORE normalizing in entity_validation_integration.rs, 8 tests, 4-char boundary).
- See docs/decisions/adr-0045-preflight-validation-layer.md and adr-0046-preflight-remediation.md for the full architectural decision.

## v1.0.89 Embedding Pipeline Remediation and Safety Fixes (ADR-0050)
- BUG-YES-FLAG-IGNORED: three destructive commands (slots release, purge, cleanup-orphans) declared --yes but executed deletions without it. All now abort with AppError::Validation when --yes is absent, matching the 5 other destructive commands that already enforced this
- BUG-BOOLISH-ENV: four boolean CLI flags (--skip-embedding-on-failure, --strict-env-clear, --dry-run-backend, --llm-slot-no-wait) rejected standard Unix env values (1, yes, on) with exit 2. Fixed via BoolishValueParser. Scripts setting SQLITE_GRAPHRAG_SKIP_EMBEDDING_ON_FAILURE=1 now work correctly
- BUG-STRICT-ENV-PROPAGATION: --strict-env-clear CLI flag was silently ignored because main.rs did not propagate it to the env var. Fixed: now propagated via set_var before command dispatch
- GAP-FLAGS-MORTAS: 7 global LLM flags were accepted by clap but silently ignored because internal modules read env vars directly. Fixed: main.rs now bridges CLI flags to env vars via set_var
- GAP-RECALL-001: embedding deadlock from stale LLM subprocess slots resolved via explicit drop(stdin), reduced timeout (300s to 30s), stale slot reaper, and sqlite-graphrag orphan process cleanup
- See docs/decisions/adr-0050-embedding-deadlock-remediation.md for the full architectural decision

## v1.0.93 OpenRouter API Key Handling (ADR-0052)
- v1.0.93 introduces `--embedding-backend openrouter` which uses a real API key (NOT OAuth) for direct REST API calls to OpenRouter
- The API key is provided via `--openrouter-api-key` flag or `OPENROUTER_API_KEY` env var
- The key is wrapped in `secrecy::SecretString` and zeroized on drop — NEVER held as plain String in memory after initialization
- The key is NEVER logged to stderr even at `RUST_LOG=trace` level
- The key is NEVER persisted in `graphrag.sqlite` or any cache file
- The key is NEVER forwarded to LLM subprocesses (claude, codex, opencode) — it flows only to `reqwest` HTTPS calls to `api.openrouter.ai`
- This is SEMANTICALLY DISTINCT from the OAuth-only enforcement on LLM backends: `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` still ABORT with exit 1
- The `OPENROUTER_API_KEY` env var is NOT in the env-clear whitelist — it stays in the parent process only
- Operators on shared hosts SHOULD prefer `--openrouter-api-key` flag over env var to minimize exposure window
- See `docs/decisions/adr-0052-openrouter-embedding-backend.md` for the full architectural decision

## Hall of Fame
- We publicly acknowledge researchers who report vulnerabilities responsibly
- This section is open to contributions: your name will be added after coordinated disclosure
- If you prefer anonymity, we honor that preference without exception


## Best Practices for Users
- SEMPRE install published releases with `cargo install sqlite-graphrag --locked`
- Use `cargo install --path .` only when testing an unreleased local checkout intentionally
- SEMPRE rotate your `crates.io` API tokens on a regular schedule
- SEMPRE keep your rustc toolchain updated to the latest stable release compatible with MSRV 1.88
- SEMPRE review CHANGELOG entries before upgrading across major versions
- JAMAIS commit secrets or tokens to the repository or to derived forks
- JAMAIS disable the memory guard in production via undocumented flags
- JAMAIS raise heavy-command concurrency blindly on memory-constrained hosts; prefer serial execution during audits
- JAMAIS bypass `cargo audit` warnings without opening a tracked security advisory
- JAMAIS set `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` in the environment; the spawn will abort with exit 1
- JAMAIS rely on `ANTHROPIC_AUTH_TOKEN` forwarding when the host is shared with untrusted processes; prefer `--strict-env-clear` so credentials stay in the parent process only


## v1.0.94 Headless Mode Hardening (ADR-0053)
- v1.0.94 makes `enrich --mode` REQUIRED (removed the `claude-code` default); omitting it is rejected by clap with exit 2.
- This prevents an accidental `claude -p` spawn that would inherit the caller project `.mcp.json` and execute untrusted MCP servers in a headless context.
- No new exit code and no new environment variable are introduced; the change is a safer default surface only.
- Valid modes are `claude-code`, `codex`, `opencode`; pick the one matching your `--llm-backend`.
