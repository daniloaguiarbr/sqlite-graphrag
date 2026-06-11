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
