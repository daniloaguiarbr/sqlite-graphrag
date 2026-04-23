# CLAUDE.md: Instructions for Claude Code Working on sqlite-graphrag


## Rule Zero: Inviolable Law
- This document is SUPREME LAW for Claude Code sessions on sqlite-graphrag.
- You MUST re-read this document BEFORE every action.
- You MUST cite the applicable rule BEFORE acting on it.
- Any violation results in IMMEDIATE CRITICAL FAILURE.
- Any violation requires complete rework of the output.
- This document loads in EVERY session via the Claude Code harness.
- The Portuguese mirror lives at `docs/CLAUDE.pt-BR.md` with identical rules.


## Session Loading Contract
- YOU MUST read this document at session start before any user prompt.
- YOU MUST re-read this document after any compact or context reset.
- YOU MUST treat any conflict with casual chat preferences as won by this file.
- YOU ARE FORBIDDEN from caching stale knowledge of this file across sessions.


## Model Policy
- YOU MUST route every cognitive teammate to the `sonnet` model class.
- YOU MUST route every read-only exploration teammate to the `haiku` model class.
- YOU ARE FORBIDDEN from using `haiku` for writing, deciding, or executing work.
- YOU ARE FORBIDDEN from using `sonnet` for tasks an `haiku` can cover.
- YOU MUST document the model choice inside every `Task` spawn payload.
- Violation of the model policy is an IMMEDIATE CRITICAL FAILURE.


## Agent Teams Requirement
- YOU MUST solve EVERY task through Agent Teams without exception.
- YOU MUST call `TeamCreate` with a descriptive kebab-case `team_name`.
- YOU MUST call `TaskCreate` with self-contained description and `activeForm`.
- YOU MUST call `Task` with `team_name`, `subagent_type`, `name`, and `model` set.
- YOU MUST spawn every teammate of a phase in ONE batch to maximize parallelism.
- YOU ARE FORBIDDEN from spawning plain subagents without a `team_name`.
- YOU ARE FORBIDDEN from executing sequentially when parallelism is viable.
- A team MUST have at least three teammates to justify orchestration overhead.


## Agent Roles Available
- `architect` defines structs, enums, traits, and module contracts.
- `implementer` writes production code following `architect` specs.
- `tester` writes unit, integration, property-based, and CLI tests.
- `reviewer` checks clippy, anti-patterns, and conformance to this file.
- `researcher` consults `context7` and the web, never writes code.
- `explorer` maps files read-only on the `haiku` model exclusively.
- `security` runs `cargo audit`, `cargo deny`, and scans secrets.
- `docs-writer` writes doc comments, README, and CHANGELOG entries.
- `diagnostician` quantifies technical debt and anti-pattern counts.
- `analyst` calculates code-to-test ratios and coupling hotspots.
- `validator` runs the ten validation gates end-to-end.
- `standardizer` updates project rules to prevent recurrence.
- `investigator` debates bug hypotheses with falsifiable tests.


## CLI Tools Hierarchy
- YOU MUST use `rg` for text content search and NEVER `grep` or `egrep`.
- YOU MUST use `fd` for locating files and NEVER `find` or `locate`.
- YOU MUST use `bat` for file display and NEVER `cat`, `less`, or `head`.
- YOU MUST use `eza` for listings and NEVER `ls` or `tree`.
- YOU MUST use `sd` for single-file substitution and NEVER `sed` or `awk`.
- YOU MUST use `ruplacer` for mass substitution and NEVER `sed -i` recursion.
- YOU MUST use `jaq` for JSON manipulation and NEVER `jq`.
- YOU MUST use `sg` for syntax-aware search and NEVER regex on code structure.
- YOU MUST use `xh` for HTTP calls and NEVER `curl` or `wget`.
- YOU MUST use `fend` for arithmetic and unit conversion and NEVER `bc` or `expr`.
- YOU MUST use `ouch` for compression and NEVER `tar`, `zip`, or `gzip`.
- YOU MUST use `procs` for process listing and NEVER `ps`.
- YOU MUST use `dysk` for filesystem info and NEVER `df`.
- YOU MUST use `dutree` for disk analysis and NEVER `du` or `tree -h`.
- YOU MUST use `tokei` for code counting and NEVER `wc -l` on source files.
- YOU MUST use `difft` for diff inspection and NEVER `diff` without wrapping.
- YOU MUST use `choose` for field selection and NEVER `cut` or `awk` columns.
- YOU MUST use `z` for directory navigation and NEVER `cd` inside sessions.


## Documentation Lookup Contract
- YOU MUST run `context7 library <name> --json` before adopting any crate.
- YOU MUST extract the `id` with `jaq -r '.[0].id'` from the library result.
- YOU MUST run `context7 docs <id> --query "<question>" --text` to read docs.
- YOU MUST treat `trustScore < 7` as a signal to corroborate via web search.
- YOU MUST fall back to `duckduckgo-search-cli -q -f json "<query>"` when needed.
- YOU ARE FORBIDDEN from inventing an API signature without context7 evidence.
- YOU ARE FORBIDDEN from skipping documentation lookup on trust from memory.


## Rust Tooling Hierarchy
- YOU MUST run `cargo check` BEFORE invoking `rust-analyzer` CLI subcommands.
- YOU MUST prefer `rust-analyzer ssr` for semantic refactors touching types.
- YOU MUST prefer `sg --rewrite` for syntactic refactors across the tree.
- YOU MUST prefer `sd` for single-file substitutions with literal text.
- YOU MUST prefer `ruplacer --go` for multi-file substitutions at scale.
- YOU MUST use `Edit` or `Write` ONLY as last resort for new files or stubs.


## PDCA Eight Phases
- Phase 1 Understanding reads the user goal and clarifies via `AskUserQuestion`.
- Phase 2 Exploration reads project rules, memory, and maps the repository.
- Phase 3 Research spawns researchers that consult `context7` in parallel.
- Phase 4 Identification pinpoints the problem with Ishikawa and Five Whys.
- Phase 5 Planning decomposes into three to ten tasks with dependencies declared.
- Phase 6 Delegation spawns every teammate at once with self-contained prompts.
- Phase 7 Verification runs the ten gates and confirms the goal is reached.
- Phase 8 Shutdown sends `shutdown_request`, waits, and calls `teammates()`.


## Debate Mode for Bugs
- YOU MUST enter debate mode whenever the user reports a bug.
- YOU MUST spawn three to five investigators with distinct hypotheses.
- YOU MUST instruct every investigator to write a failing reproducer test.
- YOU MUST let investigators challenge peers via `SendMessage` evidence trails.
- YOU MUST accept only the hypothesis backed by code evidence and failing test.
- YOU ARE FORBIDDEN from letting the lead intervene with a preferred answer.


## Forbidden Shortcuts
- YOU ARE FORBIDDEN from resolving any task outside Agent Teams.
- YOU ARE FORBIDDEN from spawning subagents without `team_name`.
- YOU ARE FORBIDDEN from serializing tasks that can run in parallel.
- YOU ARE FORBIDDEN from skipping PDCA phases or reordering them.
- YOU ARE FORBIDDEN from skipping the ten validation gates before merge.
- YOU ARE FORBIDDEN from claiming completion without an executed test run.


## Required Before Commit
- Gate 1 requires `timeout 120 cargo check --all-targets` to pass clean.
- Gate 2 requires `timeout 180 cargo clippy --all-targets --all-features -- -D warnings` to pass.
- Gate 3 requires `timeout 60 cargo fmt --all --check` to report zero differences.
- Gate 4 requires `RUSTDOCFLAGS="-D warnings" timeout 120 cargo doc --no-deps --all-features` to pass.
- Gate 5 requires `timeout 300 cargo nextest run --all-features` to report zero failures.
- Gate 6 requires `timeout 600 cargo llvm-cov --text` at eighty percent minimum.
- Gate 7 requires `timeout 120 cargo audit` to report zero open advisories.
- Gate 8 requires `timeout 120 cargo deny check advisories licenses bans sources` to pass.
- Gate 9 requires `timeout 120 cargo publish --dry-run --allow-dirty` to succeed.
- Gate 10 requires `timeout 120 cargo package --list` to exclude sensitive files.


## Timeout Protocol
- YOU MUST wrap every long-running command with `timeout` in integer seconds.
- YOU MUST use `timeout 60` for fast commands such as `cargo fmt --check`.
- YOU MUST use `timeout 120` for medium commands such as `cargo check` or `audit`.
- YOU MUST use `timeout 180` for `cargo clippy --all-targets --all-features`.
- YOU MUST use `timeout 300` for `cargo build --release` or `cargo nextest run`.
- YOU MUST use `timeout 600` for coverage runs or long integration suites.
- YOU MUST convert textual durations via `fend` when user input uses minutes.
- YOU ARE FORBIDDEN from passing human-readable suffixes such as `5m` to timeout.


## Error Handling Rules
- YOU ARE FORBIDDEN from `unwrap()` in production binaries or libraries.
- YOU ARE FORBIDDEN from `expect()` unless the branch is provably unreachable.
- YOU ARE FORBIDDEN from leaving `println!` debug output in committed code.
- YOU ARE FORBIDDEN from leaving `dbg!` macros in committed code.
- YOU ARE FORBIDDEN from leaving `todo!()` or `unimplemented!()` in main.
- YOU MUST propagate errors with the question mark operator across boundaries.
- YOU MUST return `anyhow::Result<T>` from binary entry points for context.
- YOU MUST return `thiserror::Error` enums from libraries for typed errors.


## Memory Persistence
- YOU MUST write a Serena memory at the end of every session with work done.
- YOU MUST also update `MEMORY.md` with a short entry linking the Serena note.
- YOU MUST capture commit hash, tag, coverage, and gates outcome in memory.
- YOU MUST capture open questions that the next session will need to resolve.
- YOU ARE FORBIDDEN from assuming the next session remembers this one.


## Language and Naming
- YOU MUST name variables, functions, and types in Brazilian Portuguese.
- YOU MUST name log messages, errors, and user-facing strings bilingually.
- YOU MUST keep commands, flags, and CLI surface identical across languages.
- YOU MUST localize through the `Idioma` enum and `Mensagem` exhaustive match.
- YOU ARE FORBIDDEN from generic names like `data`, `info`, `temp`, or `aux`.


## Compatible Agents
- Claude Code, Codex, Gemini CLI, Opencode, OpenClaw, Paperclip consume this CLI.
- VS Code Copilot, Google Antigravity, Windsurf, Cursor, Zed consume this CLI.
- Aider, Jules, Kilo Code, Roo Code, Cline, Continue consume this CLI.
- Factory, Augment Code, JetBrains AI Assistant, OpenRouter consume this CLI.
- Minimax, Z.ai, Ollama, Hermes Agent, LangChain, LangGraph consume this CLI.
- Every agent MUST honor the JSON contract and exit-code table from this repo.


## Correct Patterns
- Pattern 1 persists configuration as TOML with `schema_version: u32`.
- Pattern 2 masks tokens showing twelve leading plus four trailing characters.
- Pattern 3 reuses a single `reqwest::Client` across the async runtime.
- Pattern 4 writes files with `chmod 600` on Unix via `PermissionsExt`.
- Pattern 5 centralizes stdout through `src/output.rs` as the only I/O sink.


## Antipatterns
- Antipattern 1 runs `cargo install` without a matching `context7` lookup.
- Antipattern 2 commits `.env` files with keys under any circumstance.
- Antipattern 3 writes `println!("DEBUG:...")` and leaves it in the PR branch.
- Antipattern 4 ignores clippy warnings by passing `--allow` inline.
- Antipattern 5 merges a PR without the ten validation gates green.


## Final Reminder
- This document is INVIOLABLE and OVERRIDES any casual user preference.
- A violation is a CRITICAL IMMEDIATE FAILURE that demands rework.
- YOU MUST confirm `timeout 300 cargo nextest run --all-features` before every merge.
- YOU MUST persist session decisions to memory before claiming the task is done.
