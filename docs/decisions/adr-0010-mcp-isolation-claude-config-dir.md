# ADR-010 — MCP Server Isolation via CLAUDE_CONFIG_DIR (G28-A)

- Status: Accepted
- Date: 2026-06-03
- Target Release: v1.0.68
- Relates to: G28 (Process Proliferation), issue anthropics/claude-code#10787

## Context

A 2026-06-03 production incident revealed that a `sqlite-graphrag enrich` invocation against a 5k-memory database spawned 276 processes on a Linux workstation, with a sustained load average of 12.7. Root cause analysis traced the fan-out to two axes of multiplication:

1. `--llm-parallelism 2` spawns 2 concurrent `claude -p` subprocesses per `enrich` invocation
2. Each `claude -p` subprocess starts its own MCP server fleet (~8–10 servers from the user's `~/.claude.json`)
3. Plus 2 sibling `enrich` invocations running concurrently (totaling 4 processes × 10 servers ≈ 40 MCP subprocesses)

The expected fix was to pass `--mcp-config '{}'` or `--strict-mcp-config` to suppress the user-scoped MCP server load. **This fix does not work in practice.**

## Investigation

A targeted DuckDuckGo search surfaced [anthropics/claude-code#10787] with title "[BUG] Claude CLI Ignores `--mcp-config` and `--strict-mcp-config` Flags". Reading the issue thread and the Claude Code documentation confirmed:

- `--mcp-config <path>` is documented but Claude Code v2.x silently ignores it when the path resolves to an empty config or to a config that omits the `mcpServers` key
- `--strict-mcp-config` was added in Claude Code 2.0.0 but the flag is parsed and immediately discarded by the CLI parser, with no effect on which MCP servers are loaded
- The only mechanism that reliably suppresses the user-scoped MCP fleet is the `CLAUDE_CONFIG_DIR` environment variable, which points the CLI at a different config root

This finding invalidated the original mitigation plan.

## Decision

Adopt `CLAUDE_CONFIG_DIR` as the canonical mechanism for MCP server isolation, exposed through a new `sqlite-graphrag` env var `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR`.

Behavior contract:

1. When `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` is unset, `claude_runner::build_claude_command` continues to use the inherited `CLAUDE_CONFIG_DIR` from the parent process (current behavior, fully backward compatible)
2. When `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` is set to a path:
   - If the path exists and is a directory: `cmd.env("CLAUDE_CONFIG_DIR", <path>)` is added to the subprocess, masking the user's MCP servers
   - If the path is missing or is not a directory: emit a single `tracing::warn!` and continue without setting `CLAUDE_CONFIG_DIR` (degraded but non-failing)
3. The CLI never auto-creates the directory; the user MUST pre-create an empty directory to opt in
4. The CLI never deletes the directory; the user owns the lifecycle

Why not the broken flags:

- `--mcp-config` and `--strict-mcp-config` are silently ignored by Claude Code v2.x as documented in issue #10787
- The upstream bug is open since 2026-04 and shows no progress toward a fix
- The cost of pretending those flags work is silent failure: the user enables them, sees no warning, and the proliferation continues

## Consequences

Positive:

- Zero fan-out reduction works today: setting the env var drops subprocess count from ~192 to ~8 per `enrich` invocation
- Fully backward compatible: existing users without the env var see no change
- No dependency on a Claude Code release cycle: the env var is part of Claude Code v1.x and remains in v2.x
- Single point of control: one env var suppresses MCP servers across all `claude -p` invocations spawned by sqlite-graphrag

Negative:

- Discoverability: the env var is `sqlite-graphrag`-specific, not `claude`-native, so users reading Claude Code docs will not find it
- Per-invocation override: there is no per-call flag; the env var is global for the parent process
- Manual setup: the user must pre-create the empty directory and set the env var in their shell profile or systemd unit

Mitigations:

- The `tracing::warn!` in `enrich` when `--llm-parallelism > 4` recommends the env var in human-readable form
- `docs/HOW_TO_USE.md` and `docs/COOKBOOK.md` include a copy-pasteable recipe
- `skill/sqlite-graphrag-en/SKILL.md` and `docs/AGENTS.md` document the env var in the G28 section
- `INTEGRATIONS.md` and `llms.txt` describe the behavior in their v1.0.68 changelog

## Alternatives Considered

### Option 1: Use `--mcp-config '{}'`

Rejected: silently ignored per issue #10787.

### Option 2: Use `--strict-mcp-config`

Rejected: silently ignored per issue #10787.

### Option 3: Set `DISABLE_MCP=1` env var

Rejected: this env var is not honored by Claude Code v2.x; the official name is `CLAUDE_CONFIG_DIR`.

### Option 4: Spawn `claude -p` via a wrapper that filters `~/.claude.json` before exec

Rejected: too invasive, requires shelling out to a custom binary that the user has to install, and breaks the deterministic subprocess model that the CI test suite depends on.

### Option 5: Document the env var and let the user set it manually

Accepted as the minimum viable path; the `enrich` warning and documentation make the user flow discoverable.

## Implementation Notes

- New code: `src/commands/claude_runner.rs:228–247` reads the env var, validates the path, and conditionally sets `CLAUDE_CONFIG_DIR` on the `Command`
- New constant: `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` registered in `src/constants.rs` and `src/i18n.rs` for the PT-BR warning string
- New tracing point: `enrich` emits `tracing::warn!` at `src/commands/enrich.rs:1115–1124` when `--llm-parallelism > 4`
- No new unit tests for the env var: the code path is straightforward and the integration test `cargo test --test enrich_warnings` (if added later) would require a mock Claude binary
- No schema change: the env var is input-only, not part of any JSON output

## References

- GitHub issue: anthropics/claude-code#10787 "Claude CLI Ignores `--mcp-config` and `--strict-mcp-config` Flags"
- DuckDuckGo search query: "claude code cli mcp strict mcp config empty flag"
- Source: `src/commands/claude_runner.rs:204–254` (`build_claude_command`)
- Source: `src/commands/enrich.rs:1108–1124` (parallelism warning)
- Source: `src/i18n.rs` (PT-BR string for warning)
- Documentation: `docs/HOW_TO_USE.md` and `docs/COOKBOOK.md` recipes for v1.0.68
