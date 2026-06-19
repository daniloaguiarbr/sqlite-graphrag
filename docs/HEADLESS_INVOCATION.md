## Custom Provider Env Preservation in Headless Invocation (v1.0.83+)
- The headless invocation pipeline (`claude_runner`, `codex_spawn`, `ingest_claude`) now preserves six custom-provider env vars when spawning subprocesses: `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT`
- The three spawners delegate to `apply_env_whitelist(cmd, strict)` from `src/spawn/env_whitelist.rs` instead of inlining the whitelist array. This eliminates drift between the three duplicated `env_clear` + re-injection blocks
- The OAuth-only guard at `claude_runner.rs:273`, `codex_spawn.rs:259`, `ingest_claude.rs:282`, `extract/llm_embedding.rs:237-253` is unchanged; `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` still abort with `AppError::Validation` (exit 1) and the new error message references `ANTHROPIC_AUTH_TOKEN` and `~/.codex/auth.json` as legitimate resolutions
- New global flag `--strict-env-clear` / `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` enables strict mode that preserves only `PATH`. Use in compliance environments (PCI-DSS, SOC2, HIPAA) where credential forwarding via env vars is forbidden by policy
- The 7 hardening flags for `claude -p` (`--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions --output-schema` plus model and prompt) and the canonical set for `codex exec` remain unchanged. The env whitelist change is purely additive in the whitelist step between `env_clear()` and the canonical flag construction
- No new telemetry: the fix is silent. The no-leak audit test `audit_no_token_leak_in_subprocess_stderr` in `tests/claude_runner_env.rs` enforces that the literal token value NEVER appears in stdout or stderr even with `RUST_LOG=trace`
- See `docs/decisions/adr-0041-preserve-custom-provider-env.md` for the full architectural rationale
# Headless Invocation — Claude Code, Codex, OpenCode without MCP and without Hooks

> How to invoke headless LLMs in this project without inheriting MCPs or hooks from the environment, while keeping the subscription OAuth login.

- Portuguese version of this guide lives in [HEADLESS_INVOCATION.pt-BR.md](HEADLESS_INVOCATION.pt-BR.md)
- Back to [README.md](../README.md) for the command reference


## Summary

- Claude Code OAuth without MCP uses `--strict-mcp-config --mcp-config '{}'`
- Codex OAuth without MCP uses `codex exec -c mcp_servers='{}'`
- OpenCode OAuth without MCP uses `OPENCODE_CONFIG_CONTENT` with `enabled` false per server
- The most important finding: on Claude, the `--bare` flag cuts MCPs but DISABLES OAuth. `--bare` then requires an API key, which is forbidden here. That is why `--bare` is NEVER used when login is subscription-based


## OAuth-Safe Command Table

| CLI | OAuth-safe headless command | Keeps OAuth | Cuts MCP | Cuts Hooks |
| --- | --- | --- | --- | --- |
| Claude Code | `claude -p "TASK" --strict-mcp-config --mcp-config '{}' ...` | yes | yes | yes |
| Codex CLI | `codex exec -c mcp_servers='{}' ...` | yes | yes | N/A |
| OpenCode | `OPENCODE_CONFIG_CONTENT='{...enabled:false...}' opencode run ...` | yes | yes | N/A |


## Claude Code Headless OAuth without MCP and without Hooks

### What To Do

Run `claude -p` with the MCP config locked down and empty, and the hooks config zeroed out.

### Why

- `-p` enables one-shot headless mode
- `--strict-mcp-config` tells it to ignore ALL MCP config from the environment
- `--mcp-config '{}'` provides an empty server list
- `--settings '{"hooks":{}}'` disables hooks for that specific call
- The combination guarantees zero MCPs and zero hooks running, while keeping the subscription login (OAuth Pro or Max)

### v1.0.79 Update — The Real Isolation Is an Empty `CLAUDE_CONFIG_DIR`

- Issue #10787 of `anthropics/claude-code` documents that `--strict-mcp-config` and `--mcp-config` are silently IGNORED by upstream
- The only mechanism upstream honours is `CLAUDE_CONFIG_DIR` pointing to an empty directory
- Since v1.0.79 (G42/S6), the CLI embedding pipeline uses an empty `CLAUDE_CONFIG_DIR` BY DEFAULT: it honours `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR`, otherwise a managed directory `~/.local/state/sqlite-graphrag/claude-empty-config` (mode 0700, copies `.credentials.json` when present)
- A populated `~/.claude` used to cost ~223k cache-creation tokens per call (~40-50s); the empty config dir brings it down to ~10-15s
- The flags below are still passed as defence in depth, but do NOT rely on them for isolation

### Why NOT To Use `--bare`

- `--bare` also cuts MCP, hooks, skills, plugins and auto memory
- BUT `--bare` disables OAuth and the keychain (issue #39069 of `anthropics/claude-code`)
- With `--bare`, Claude requires `ANTHROPIC_API_KEY`, which is forbidden in this project
- To keep OAuth, the right path is `--strict-mcp-config`, never `--bare`

### How To Do It

```bash
claude -p "YOUR TASK HERE" \
  --strict-mcp-config \
  --mcp-config '{}' \
  --dangerously-skip-permissions \
  --settings '{"hooks":{}}' \
  --model sonnet \
  --max-turns 8 \
  --output-format json
```

### What Each Piece Does

- `--strict-mcp-config` ignores MCP from global and project settings
- `--mcp-config '{}'` provides the empty list that zeroes out servers
- `--dangerously-skip-permissions` avoids stalling on confirmation prompts (`bypassPermissions` mode)
- `--settings '{"hooks":{}}'` disables hooks for that specific call
- `--model sonnet` picks the model without depending on an environment variable
- `--max-turns 8` caps agent turns as a safety net against infinite loops
- `--output-format json` delivers output that is easy to parse with `jaq`

### How To Guarantee OAuth

- Log in once with the Pro or Max account before automating (`claude auth login`)
- Do NOT set `ANTHROPIC_API_KEY` in the call environment
- Do NOT use `--bare`
- Without the variable and without `--bare`, Claude uses the logged-in OAuth session

### Known Bug Caveat

- Issue #14490 of `anthropics/claude-code` documents that `--strict-mcp-config` does NOT override the `disabledMcpServers` list stored in `~/.claude.json`
- For a clean environment, ensure `~/.claude.json` does not contain the server in `disabledMcpServers`, or use `--bare` only in a controlled environment with `ANTHROPIC_API_KEY` (a scenario explicitly FORBIDDEN in this project)
- The robust solution is to combine `--strict-mcp-config --mcp-config '{}'` and ensure the server is not in `disabledMcpServers` in `~/.claude.json`


## Codex CLI Headless OAuth without MCP

### What To Do

Run `codex exec` zeroing out the MCP server table from the config.

### Why

- `codex exec` is the non-interactive mode built for scripts
- It writes only the final message to stdout and progress to stderr
- The `-c mcp_servers='{}'` override replaces the entire table with an empty one
- That way no MCP server from `config.toml` comes up for that call

### How To Do It

```bash
codex exec \
  -c mcp_servers='{}' \
  --sandbox workspace-write \
  --ask-for-approval never \
  "YOUR TASK HERE"
```

### More Aggressive Alternative

- Use `--ignore-user-config` to skip reading the user `config.toml` entirely
- That zeroes out MCP along with everything else in the config
- The OAuth login is stored in `auth.json`, which is a separate file
- That is why `--ignore-user-config` does NOT break the login

```bash
codex exec --ignore-user-config --sandbox workspace-write "YOUR TASK HERE"
```

### What Each Piece Does

- `-c mcp_servers='{}'` zeroes only the MCPs and preserves the model and the rest of the config
- `--ignore-user-config` is the full cut when you want a clean environment
- `--sandbox workspace-write` allows file editing without network access
- `--ask-for-approval never` runs without pausing for permission

### How To Guarantee OAuth

- Run `codex login` once for the browser flow with ChatGPT
- On a remote or browserless machine, use `codex login --device-auth`
- Do NOT set `OPENAI_API_KEY` in the call environment
- The login is stored in `~/.codex/auth.json` and `codex exec` reuses the session

### Old Bug Caveat

- Old Codex versions (0.33.0) installed via Homebrew did not read `[mcp_servers]` correctly
- Issue #3441 of the `openai/codex` repository confirms the fix landed in 0.34.0+
- Validate the version with `codex --version` before using the `-c mcp_servers='{}'` override


## OpenCode Headless without MCP

### The Honest Difference

- OpenCode does NOT have a single CLI flag to disable MCP
- Claude has `--strict-mcp-config` and Codex has `-c mcp_servers='{}'`
- OpenCode controls MCP only through the JSON config
- OpenCode configs are merged, not replaced, so each server must be disabled individually

### What To Do

- Discover the active server names with `opencode mcp list`
- Disable each one with `enabled: false` in the config

### Why

- `opencode run` is the headless mode that takes the prompt and returns the result
- Because the config is merged, deleting the key is not enough to remove the server
- Setting `enabled` false under the same name overrides and disables that MCP
- The runtime override via `OPENCODE_CONFIG_CONTENT` avoids touching project files

### How To Do It — Step 1 List Active Servers

```bash
opencode mcp list
```

### How To Do It — Step 2 Run Headless Disabling Each Server

```bash
OPENCODE_CONFIG_CONTENT='{"mcp":{"server-name-1":{"enabled":false},"server-name-2":{"enabled":false}}}' \
  opencode run --model anthropic/claude-sonnet-4-5 "YOUR TASK HERE"
```

### Permanent Alternative

- Edit `opencode.json` and mark each MCP with `enabled` false
- Worth it when you never want that server in automatic execution

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "server-name-1": { "enabled": false },
    "server-name-2": { "enabled": false }
  }
}
```

### What Each Piece Does

- `opencode mcp list` shows server names and connection status
- `OPENCODE_CONFIG_CONTENT` injects inline config with high precedence
- `enabled` false per server is what actually prevents the MCP from coming up
- `--model` picks the model in `provider/model` format

### How To Guarantee OAuth

- Run `opencode auth login` once and choose the provider
- The credential is stored in `auth.json` in the OpenCode data folder
- `opencode run` reuses that credential on subsequent calls


## OAuth Login per CLI

- Claude: session login via `claude auth login`. Do NOT use `--bare` to preserve OAuth
- Codex: `codex login` or `codex login --device-auth` (browserless)
- OpenCode: `opencode auth login`


## Headless Mode per CLI

- Claude: `claude -p`
- Codex: `codex exec`
- OpenCode: `opencode run`


## v1.0.80 Update — SHUTDOWN Resilience and the 3-Layer Bypass Recipe

v1.0.80 (ADR-0034) hardens the `src/signals.rs` handler so that the
orphaned-process scenario that the G42/C2 audit identified no longer
triggers a `SIGABRT` on `BrokenPipe`. The third consecutive Ctrl-C
exits with code 130 and **ZERO I/O**, matching the contract below.

For long embedding jobs that the agent harness (or any background
orchestrator) may kill via SIGINT, use the 3-layer bypass recipe.
All 3 layers are independent and the recipe composes additively:

```bash
# Layer 1 — PATH: route the LLM subprocess through the mock CLI in CI
export PATH="$PWD/tests/mock-llm:$PATH"

# Layer 2 — env: tell the embedder to ignore the SHUTDOWN check
export SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1

# Layer 3 — process group: detach the CLI from the harness's pgroup
setsid -w timeout 600 \
  sqlite-graphrag remember --graph-stdin < payload.json
```

- **Layer 1 (PATH)**: routes any spawned `claude -p` or `codex exec`
  through the deterministic mock-llm binary checked into
  `tests/mock-llm/`. The real LLM subprocess is bypassed; SIGINT
  cannot kill a subprocess that does not exist. This is the cheapest
  layer and is the right default for CI.
- **Layer 2 (env)**: makes the embedder's `if should_obey_shutdown()`
  short-circuit to `true`, so the `tokio::select!` cancellation arm
  is dropped and the batch runs to completion even if the
  cancellation token is already cancelled. Zero overhead in
  production because the env read is one `std::env::var` per
  `should_obey_shutdown()` call, not in a hot path.
- **Layer 3 (setsid)**: gives the CLI its own process group via
  `setsid -w`, so SIGINT from the parent harness does not propagate
  to the child. `timeout` adds a hard wall-clock cap (the Rust
  `timeout-cli` v0.1.0 binary, integer seconds only — `600` is 10
  minutes; do not pass `10m`).

The recipe is now the canonical reference for any agent harness
running long embedding jobs in background. The bypass is
explicitly opt-in: production code MUST NOT call
`try_reset_shutdown()`, and the env var MUST NOT be set in
production. Tests and audit invocations are the only valid
consumers.

If the run is interrupted between layers, the SQLite file remains
consistent (WAL, atomic commit, no partial writes), and `restore`
or `enrich --operation re-embed --resume` can pick up from the
last successful memory.

## Pre-flight Validation Layer (v1.0.87+ — ADR-0045)

From v1.0.87 onwards, every LLM subprocess spawn passes through a
mandatory pre-flight gate in `src/spawn/preflight.rs` (15 unit tests,
7 guards). The gate aborts the spawn BEFORE the fork when the
invocation would fail in runtime, returning
`AppError::PreFlightFailed` (exit code 16, `EX_CONFIG`).

### The 7 guards (in order)

1. `check_argv_size` — rejects invocations whose argv total would
   exceed `ARG_MAX` minus 4 KB safety margin
2. `check_binary_exists` — confirms `claude` or `codex` is reachable
   in `PATH` before invoking
3. `check_mcp_config_inline` — replaces literal `--mcp-config {}`
   with a tempfile holding `{"mcpServers":{}}` (fixes BUG-2)
4. `check_mcp_config_path` — validates the JSON contents of
   `--mcp-config <PATH>` if used
5. `check_walkup_mcp_json` — walks the workspace root looking for
   `.mcp.json` and validates the JSON
6. `check_output_buffer` — raises the parser buffer above 64 KB
   when expected output exceeds it (fixes BUG-4)
7. `check_claude_config_dir` — validates `CLAUDE_CONFIG_DIR` is empty
   or absent (avoids MCP bleed-through from user-level config)

### Bypassing pre-flight in emergencies

Set `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` to disable all 7 guards. This
is a **last-resort opt-out** intended for production incident
mitigation; it is not a normal mode of operation. When pre-flight
is skipped, the spawner reverts to direct `Command::spawn()` and
inherits all 5 BUG classes from GAP-META-005.

### Related regressions (v1.0.88 hotfixes)

Three BUGs were discovered and fixed in v1.0.88 after the pre-flight
gate was introduced in v1.0.87:

- **BUG-11**: preflight failure in `extract/llm_embedding.rs` did not
  propagate to `remember`, which silently persisted the memory with
  `backend_invoked: "none"` and no embedding. Fixed in v1.0.88 with
  `embed_via_backend_strict` (2 tests in `bug11_preflight_regression.rs`).
- **BUG-12**: OAuth-only enforcement emitted 2 identical stderr lines.
  Fixed in v1.0.88 with single-line stderr (test:
  `oauth_stderr_emits_single_line_v1088`).
- **BUG-13**: `link --create-missing` bypassed entity-name validation
  by normalizing the name BEFORE the validator ran. Fixed in v1.0.88
  by validating BEFORE normalizing (8 tests in
  `entity_validation_integration.rs`).

## Validated External References

### Claude Code

- `code.claude.com/docs/en/headless` — headless mode and clear exit codes
- `amux.io/guides/claude-code-headless/` — complete headless self-hosting guide (2026)
- `github.com/anthropics/claude-code/issues/39069` — `--bare` mode skips OAuth/keychain, unusable for OAuth-only
- `computingforgeeks.com/claude-code-cheat-sheet/` — cheat sheet covering `--mcp-config` and `--strict-mcp-config`
- `github.com/anthropics/claude-code/issues/14490` — `--strict-mcp-config` does not override `disabledMcpServers`

### Codex CLI

- `developers.openai.com/codex/cli/reference` — canonical CLI options reference
- `deepwiki.com/openai/codex/6.1-mcp-server-configuration` — MCP server config in `config.toml`
- `ofox.ai/blog/codex-cli-config-toml-deep-dive/` — every `config.toml` setting explained
- `github.com/openai/codex/issues/3441` — bug where `[mcp_servers]` did not work in an old Codex version

### OpenCode

- `opencode.ai/docs/mcp-servers/` — MCP control via `enabled: false` per server
- `open-code.ai/en/docs/config` — `opencode.json` reference with providers, models, MCP
- `computingforgeeks.com/opencode-cli-cheat-sheet/` — cheat sheet with headless and MCP flags


## Headless Patterns Added in v1.0.82
### Shutdown envelope capture pattern (GAP-002, ADR-0037)
```bash
# Wrap a long-running sqlite-graphrag invocation in a signal handler
# that captures the shutdown JSON envelope on stdout at exit 19.
timeout 300 sqlite-graphrag remember --name big-corpus --type document \
  --body-file ./big.md --json 2>/tmp/err.log
EXIT=$?
if [ $EXIT -eq 19 ]; then
  # parse the envelope from the last line of stdout
  jaq -e '.error and .code == 19' /tmp/err.log
  jaq -r '.signal, .graceful' /tmp/err.log
fi
```
### Fallback chain wrap pattern (GAP-003 + GAP-005, ADR-0038 + ADR-0040)
```bash
# Pre-flight: validate both backends are reachable before launching
timeout 30 codex exec --help >/dev/null 2>&1 || { echo "codex missing"; exit 1; }
timeout 30 claude --help >/dev/null 2>&1 || { echo "claude missing"; exit 1; }

# Launch with the fallback chain
sqlite-graphrag remember --name foo --type note --body "..." \
  --llm-backend codex,claude --json

# If all backends fail, inspect the pending queue
sqlite-graphrag pending-embeddings list --filter-status failed --json
```
### Slot semaphore poll pattern (GAP-004, ADR-0039)
```bash
# Wait until a slot is free before launching a heavy batch
while [ "$(sqlite-graphrag slots status --json | jaq '.acquired')" -gt 0 ]; do
  sleep 5
done
sqlite-graphrag ingest ./big-corpus --recursive --json
```
### codex OAuth 401 mitigation pattern (ADR-0040, 2026-06-14 incident)
```bash
# Refresh the OAuth token at the start of any long batch
codex login

# Configure the fallback chain to handle refresh_token_reused 401
sqlite-graphrag remember --name auth-fix --type decision \
  --body "Refresh-token rotation policy" \
  --llm-backend codex,claude --json
```
