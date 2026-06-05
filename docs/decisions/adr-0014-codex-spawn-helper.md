# ADR-0014 — `codex_spawn` Helper Unificado (v1.0.69)

- **Status.** Accepted.
- **Date.** 2026-06-05.
- **Deciders.** Danilo Aguiar (operator), Claude Code (advisor).
- **Supersedes.** None.
- **Related gaps.** G31 (flags ausentes em `enrich`), G32 (parser JSONL ingênuo), G33 (validação de modelo ausente).

## Context

`enrich --mode codex` and `ingest --mode codex` had drifted in three independent dimensions:

1. **Spawn flags.** `ingest_codex.rs:320-329` passed seven hardening flags (`--json --output-schema --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules`). `enrich.rs:2773-2780` passed only three. The operator kept an external wrapper at `~/.local/bin/codex-clean` to inject the missing flags.
2. **JSONL parser.** `ingest_codex.rs:430-540` implemented a proper line-by-line `parse_codex_output`. `enrich.rs:2846-2850` used `serde_json::from_str` on the raw stdout, which always failed with `trailing characters at line 2 column 1`.
3. **Model validation.** Neither call-site validated `--codex-model` against the ChatGPT Pro OAuth whitelist; the rejection came from Codex itself after a wasted OAuth turn.

A wrapper script solved the immediate problem but multiplied configuration surface area and hid the real fix from the codebase.

## Decision

1. Extract the spawn pipeline into `src/commands/codex_spawn.rs` with `pub struct CodexSpawnArgs { binary, schema_path, model, timeout, sandbox_mode }` and `pub fn build_codex_command(args) -> Command`. The function ALWAYS passes the seven canonical flags plus `-c mcp_servers='{}'` (OAuth-only hardening from gaps.md:234) and `--ask-for-approval never`.
2. Extract the JSONL parser into `pub fn parse_codex_jsonl(stdout: &str) -> Result<(ExtractionResult, Usage), AppError>`. Both call-sites consume the same parser.
3. Add `validate_codex_model(model)`, `list_codex_models()`, and `suggest_codex_model(query)` against the ChatGPT Pro OAuth whitelist (`codex-auto-review`, `gpt-5.3-codex-spark`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.5`). The validation runs BEFORE the subprocess is spawned.
4. Move the schema JSON path from `std::env::temp_dir()` to `paths::AppPaths::cache_dir().join("schemas")` so it survives reboots and lives in a trusted directory.
5. Expose the model list via a new top-level subcommand `codex-models --json` so operators can introspect without spawning Codex.

## Consequences

- The external wrapper `~/.local/bin/codex-clean` becomes legacy. Operators can `rm` it after upgrading.
- Both call-sites have IDENTICAL defaults; future hardening lands in one place.
- 11 unit tests cover parser edge cases (multi-line JSONL, malformed lines, rate-limit detection), model validation (valid, invalid, empty, custom alias, fuzzy match), and command-flag presence.
- The schema path is now persistent in the cache dir; debugging is easier because the file survives between runs.

## Alternatives Considered

- Patch the divergence in place without extracting a helper. REJECTED. The duplication would re-emerge the next time one side is updated.
- Use a JSON-LD library for the JSONL parsing. REJECTED. `codex exec --json` emits newline-delimited JSON, not JSON-LD.
- Make the model list a dynamic query against the Codex CLI. REJECTED. The CLI does not expose a `list-models` command; the static whitelist mirrors the OAuth provider's accepted set.

## References

- `src/commands/codex_spawn.rs` (~700 lines, 11 tests).
- `src/commands/enrich.rs:3191-3207` (call_site uses helper).
- `src/commands/ingest_codex.rs:265-340` (call_site uses helper).
- `src/cli.rs:360` (new `codex-models` subcommand).
- `src/main.rs:319-329` (dispatch).
- gaps.md G31+G32+G33 lines 1444-1716.
