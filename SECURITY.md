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
