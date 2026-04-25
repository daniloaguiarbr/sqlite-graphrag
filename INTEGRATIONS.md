# Integrations


> 21 agents and 20+ platforms in a single CLI contract

- Read the Portuguese version at [INTEGRATIONS.pt-BR.md](INTEGRATIONS.pt-BR.md)
- Every recipe below is ready to copy and costs nothing to run


## Summary Table
### Catalog — Every Supported Integration
| Name | Type | Minimum Version | Example | Official Docs |
| --- | --- | --- | --- | --- |
| Claude Code | AI Agent | 1.0+ | `sqlite-graphrag recall "query" --json` | https://docs.anthropic.com/claude-code |
| Codex CLI | AI Agent | 0.5+ | `sqlite-graphrag remember --name X --type user --body "..."` | https://github.com/openai/codex |
| Gemini CLI | AI Agent | any recent | `sqlite-graphrag hybrid-search "query" --k 5 --json` | https://github.com/google-gemini/gemini-cli |
| Opencode | AI Agent | any recent | `sqlite-graphrag recall "auth flow" --json` | https://github.com/opencode-ai/opencode |
| OpenClaw | AI Agent | any recent | `sqlite-graphrag list --type user --json` | community project |
| Paperclip | AI Agent | any recent | `sqlite-graphrag read --name note --json` | community project |
| VS Code Copilot | AI Agent | 1.90+ | tasks.json | https://code.visualstudio.com/docs/copilot |
| Google Antigravity | AI Agent | any recent | `sqlite-graphrag hybrid-search "prompt" --json` | Google Antigravity docs |
| Windsurf | AI Agent | any recent | `sqlite-graphrag recall "refactor plan" --json` | https://windsurf.com/docs |
| Cursor | AI Agent | 0.40+ | `sqlite-graphrag remember --name cursor-ctx --type project --body "..."` | https://cursor.com/docs |
| Zed | AI Agent | any recent | `sqlite-graphrag recall "open tabs" --json` | https://zed.dev/docs |
| Aider | AI Agent | 0.60+ | `sqlite-graphrag recall "refactor" --k 5 --json` | https://aider.chat |
| Jules | AI Agent | preview | `sqlite-graphrag stats --json` | https://jules.google |
| Kilo Code | AI Agent | any recent | `sqlite-graphrag recall "tasks" --json` | community project |
| Roo Code | AI Agent | any recent | `sqlite-graphrag hybrid-search "repo ctx" --json` | community project |
| Cline | AI Agent | VS Code ext | `sqlite-graphrag list --limit 20 --json` | https://cline.bot |
| Continue | AI Agent | VS Code or JetBrains | `sqlite-graphrag recall "docstring" --json` | https://docs.continue.dev |
| Factory | AI Agent | any recent | `sqlite-graphrag recall "pr context" --json` | https://factory.ai |
| Augment Code | AI Agent | any recent | `sqlite-graphrag hybrid-search "review" --json` | https://docs.augmentcode.com |
| JetBrains AI Assistant | AI Agent | 2024.2+ | `sqlite-graphrag recall "stacktrace" --json` | https://www.jetbrains.com/ai |
| OpenRouter | AI Router | any | `sqlite-graphrag recall "rule" --json` | https://openrouter.ai/docs |
| POSIX Shells | Shell | any | `sqlite-graphrag recall "$query" --json` | https://www.gnu.org/software/bash |
| Nushell | Shell | 0.90+ | `^sqlite-graphrag recall "query" --k 5 --json \| from json \| get results` | https://www.nushell.sh/book |
| GitHub Actions | CI/CD | any | workflow YAML | https://docs.github.com/actions |
| GitLab CI | CI/CD | any | `.gitlab-ci.yml` | https://docs.gitlab.com/ee/ci |
| CircleCI | CI/CD | any | `.circleci/config.yml` | https://circleci.com/docs |
| Jenkins | CI/CD | 2.400+ | Jenkinsfile | https://www.jenkins.io/doc |
| Docker and Podman Alpine | Container | any | Dockerfile | https://docs.docker.com |
| Kubernetes | Orchestrator | 1.25+ | Job or CronJob | https://kubernetes.io/docs |
| Homebrew | Package Manager | macOS and Linux | `brew install sqlite-graphrag` (planned) | https://brew.sh |
| Scoop and Chocolatey | Package Manager | Windows | `scoop install sqlite-graphrag` (planned) | https://scoop.sh and https://chocolatey.org |
| Nix and Flakes | Package Manager | any | `nix run .#sqlite-graphrag` | https://nixos.org |


## Claude Code
### Anthropic Agent — Subprocess Integration
- Recipe ready to copy into `.claude/hooks/`, zero cloud cost, memory stays on your machine
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to persist context across Claude Code sessions without external memory services
- Use `sqlite-graphrag recall "$USER_PROMPT" --k 5 --json` in a pre-task hook to inject context
- Minimum version requires Claude Code 1.0 or later for stable `.claude/hooks/` directory support
- Official docs live at https://docs.anthropic.com/claude-code describing hook lifecycle events
- Golden tip is to capture exit code `75` as retry-later and keep the agent alive gracefully


## Codex CLI
### OpenAI Agent — AGENTS.md Driven Subprocess
- Recipe ready to paste into `AGENTS.md` at repo root, zero cloud cost to activate
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to expose the memory contract through the native `AGENTS.md` convention
- Use `sqlite-graphrag recall "<query>" --k 5 --json` documented inside `AGENTS.md` at repo root
- Minimum version requires Codex CLI 0.5 or later for deterministic AGENTS.md parsing rules
- Official docs live at https://github.com/openai/codex covering AGENTS.md discovery order
- Golden tip is to include a working invocation example under each listed command for Codex


## Gemini CLI
### Google Agent — Subprocess With JSON Contract
- Recipe ready to copy into your Gemini CLI config, zero cloud cost, runs fully local
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to inject memory into Gemini 2.5 Pro prompts during long coding sessions
- Use `sqlite-graphrag hybrid-search "query" --k 5 --json` for recall with mixed keyword intent
- Minimum version supports any recent Gemini CLI release with subprocess invocation enabled
- Official docs live at https://github.com/google-gemini/gemini-cli for tool integration patterns
- Golden tip is to set `SQLITE_GRAPHRAG_LANG=pt` when prompting Gemini in Portuguese contexts


## Opencode
### Community Agent — Subprocess Integration
- Recipe ready to copy into the Opencode plugin hook, zero cloud cost, runs as subprocess
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to persist multi-turn context in the open source Opencode orchestration loop
- Use `sqlite-graphrag recall "$query" --json` as part of the Opencode pre-generation pipeline
- Minimum version supports any recent Opencode release exposing a plugin subprocess hook
- Official project lives at https://github.com/opencode-ai/opencode with community issue tracker
- Golden tip is to set the namespace to the repo slug to avoid cross-project memory leakage


## OpenClaw
### Community Agent — Subprocess Driver
- Recipe ready to drop into OpenClaw startup, zero cloud cost, memory is fully local
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to inject persistent memory into OpenClaw agent loops without plugin rebuild
- Use `sqlite-graphrag list --type user --json` to fetch seed context at the start of a run
- Minimum version supports any recent OpenClaw release able to shell out to CLI binaries
- Official docs live inside the OpenClaw GitHub README explaining subprocess integration rules
- Golden tip is to run the binary inside the target project folder and keep the default `graphrag.sqlite`


## Paperclip
### Community Agent — Subprocess Client
- Recipe ready to paste into Paperclip hook config, zero cloud cost, all memory stays local
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to persist cross-session memory in the Paperclip autonomous developer agent
- Use `sqlite-graphrag read --name onboarding-note --json` to seed the session with prior notes
- Minimum version supports any recent Paperclip release that can spawn child subprocess calls
- Official docs live in the Paperclip community repository describing subprocess hook contracts
- Golden tip is to run `health --json` at startup and abort when integrity reports any damage


## VS Code Copilot
### Microsoft Agent — tasks.json Integration
- Recipe ready to paste into tasks.json, zero cloud cost, recall fires from inside the editor
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to surface relevant memory from a selection inside VS Code Copilot chat panels
- Use the example tasks.json entry that calls `sqlite-graphrag recall "$selection" --json`
- Minimum version requires VS Code 1.90 or later for the latest tasks.json variable substitutions
- Official docs live at https://code.visualstudio.com/docs/copilot covering chat tool registration
- Golden tip is to bind the task to `Cmd+Shift+M` for single-keystroke memory recall invocation


## Google Antigravity
### Google Agent — Runner Integration
- Recipe ready to register as an Antigravity runner, zero cloud cost, binary is self-contained
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to run sqlite-graphrag as a first-class runner inside Antigravity pipelines at scale
- Use `sqlite-graphrag hybrid-search "$PROMPT" --json --k 10` as the retrieval step in a runner
- Minimum version supports any recent Antigravity release that accepts arbitrary runner binaries
- Official docs live on the Google Antigravity product page describing runner configuration format
- Golden tip is to run `sync-safe-copy` before each pipeline to guard the shared memory artifact


## Windsurf
### Codeium Agent — Terminal Integration
- Recipe ready to paste into a Windsurf Run task binding, zero cloud cost to activate recall
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to expose memory recall to Windsurf assistant panels via terminal task invocation
- Use `sqlite-graphrag recall "$EDITOR_CONTEXT" --json` mapped to a Windsurf Run task binding
- Minimum version supports any recent Windsurf release with terminal task execution enabled
- Official docs live at https://windsurf.com/docs describing the terminal task binding syntax
- Golden tip is to persist results to `/tmp/ng.json` so Windsurf prompt templates can read them


## Cursor
### Cursor Agent — Terminal Integration
- Recipe ready to drop into `.cursorrules` or a terminal binding, zero cloud cost, memory is local
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to pair Cursor AI with a local memory backend that survives editor restarts
- Use `sqlite-graphrag remember --name cursor-ctx --type project --body "$SELECTION"` from a key binding
- Minimum version requires Cursor 0.40 or later for stable AI rules and terminal env override
- Official docs live at https://cursor.com/docs covering AI rules and terminal integration patterns
- Golden tip is to set `SQLITE_GRAPHRAG_NAMESPACE=${workspaceFolderBasename}` per project workspace


## Zed
### Zed Industries Agent — Assistant Panel Integration
- Recipe ready to add as a Zed task profile, zero cloud cost, runs from the built-in terminal
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to wire memory recall into the Zed assistant panel without custom extensions
- Use `sqlite-graphrag recall "open tabs" --json --k 5` as a terminal command available to Zed
- Minimum version supports any recent Zed release with the assistant panel and terminal tasks
- Official docs live at https://zed.dev/docs describing assistant panel and terminal integration
- Golden tip is to define a Zed task profile sharing memory across multiple open workspaces


## Aider
### Open Source Agent — Shell Integration
- Recipe ready to paste into your shell alias before `aider`, zero cloud cost, zero config server
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to augment Aider pair programming with durable memory across git repositories
- Use `sqlite-graphrag recall "refactor target" --k 5 --json` invoked before each Aider prompt
- Minimum version requires Aider 0.60 or later for stable subprocess and hook invocation
- Official docs live at https://aider.chat describing configuration and custom shell commands
- Golden tip is to scope memory by repository via `SQLITE_GRAPHRAG_NAMESPACE=$(basename $(pwd))`


## Jules
### Google Labs Agent — CI Automation
- Recipe ready to add as a Jules CI step, zero cloud cost, binary installs in seconds via cargo
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to run memory maintenance inside Jules preview automation pipelines automatically
- Use `sqlite-graphrag stats --json` as a CI step to monitor memory growth week over week
- Minimum version is the current Jules preview release available via Google Labs early access
- Official docs live at https://jules.google explaining CI job configuration and authentication
- Golden tip is to fail the pipeline when `stats.memories` exceeds agreed thresholds for a project


## Kilo Code
### Community Agent — Subprocess Integration
- Recipe ready to paste into Kilo Code startup hook, zero cloud cost, memory is a local file
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to expose a persistent memory layer to the Kilo Code autonomous engineering agent
- Use `sqlite-graphrag recall "recent tasks" --json` at the start of every Kilo Code agent run
- Minimum version supports any recent Kilo Code release capable of spawning child processes
- Official docs live in the Kilo Code community repository describing the subprocess contract
- Golden tip is to log exit code `75` as retryable rather than fatal when orchestrator is busy


## Roo Code
### Community Agent — Subprocess Integration
- Recipe ready to wire into Roo Code hook lifecycle, zero cloud cost, all data is local SQLite
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to inject memory into Roo Code agent prompts for deeper repository understanding
- Use `sqlite-graphrag hybrid-search "repo context" --json` for recall across mixed query types
- Minimum version supports any recent Roo Code release with hook capabilities for subprocess
- Official docs live in the Roo Code community repository explaining hook lifecycle conventions
- Golden tip is to chain `related <name> --hops 2` after recall for multi-hop graph expansion


## Cline
### Community VS Code Extension — Terminal Integration
- Recipe ready to register as a Cline terminal tool, zero cloud cost, memory persists locally
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to give Cline persistent memory across VS Code sessions without cloud services
- Use `sqlite-graphrag list --limit 20 --json` as a seed step at Cline conversation startup
- Minimum version supports the current Cline VS Code extension release in the marketplace
- Official docs live at https://cline.bot covering terminal tool registration and usage patterns
- Golden tip is to bind the command to a Cline tool with descriptive name and usage explanation


## Continue
### Open Source Agent — IDE Terminal Integration
- Recipe ready to paste into Continue custom commands config, zero cloud cost, no server needed
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to surface sqlite-graphrag memory inside Continue chat panels in VS Code or JetBrains
- Use `sqlite-graphrag recall "docstring" --json` from a Continue custom command registration
- Minimum version supports any recent Continue extension release in VS Code or JetBrains stores
- Official docs live at https://docs.continue.dev describing custom commands and tool integration
- Golden tip is to document each command in the Continue config so the embedded LLM picks it up


## Factory
### Factory Agent — API Or Subprocess
- Recipe ready to add to the Factory droid tool config, zero cloud cost, binary is self-contained
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to integrate sqlite-graphrag with Factory autonomous development droids in production
- Use `sqlite-graphrag recall "pr context" --json` during the Factory droid plan preparation phase
- Minimum version supports any recent Factory release with subprocess or API tool integration
- Official docs live at https://factory.ai explaining droid tool configuration and plan execution
- Golden tip is to set a long `--wait-lock` for Factory droids running under heavy concurrency


## Augment Code
### Augment Agent — IDE Integration
- Recipe ready to wire into Augment IDE tool registration, zero cloud cost, runs as subprocess
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to feed Augment Code review agents with persistent cross-repository memory state
- Use `sqlite-graphrag hybrid-search "code review" --json` inside Augment IDE review preparation
- Minimum version supports any recent Augment Code release with terminal and subprocess hooks
- Official docs live at https://docs.augmentcode.com describing tool registration and agents
- Golden tip is to enable `--lang en` explicitly for consistent review language across teams


## JetBrains AI Assistant
### JetBrains Agent — IDE Integration
- Recipe ready to register as a JetBrains external tool, zero cloud cost, recall takes milliseconds
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to add sqlite-graphrag memory to JetBrains AI Assistant across IntelliJ PyCharm WebStorm
- Use `sqlite-graphrag recall "$SELECTION" --json` registered as a JetBrains external tool runner
- Minimum version requires JetBrains AI Assistant 2024.2 or later for modern tool registration
- Official docs live at https://www.jetbrains.com/ai explaining tool and external runner registration
- Golden tip is to bind the tool to a keyboard shortcut to invoke recall with one hand on keyboard


## OpenRouter
### Multi-LLM Router — Any Version Supported
- Recipe ready to add as a preamble to any OpenRouter pipeline, zero cloud cost, memory stays local
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to share a common memory backend across every OpenRouter-hosted LLM in a pipeline
- Use `sqlite-graphrag recall "routing rule" --json` as a preamble step before any routed request
- Minimum version supports any OpenRouter API release since memory remains local and independent
- Official docs live at https://openrouter.ai/docs explaining routing rules and API integration
- Golden tip is to reuse the same namespace across all routed models for consistent context


## POSIX Shells
### Bash Zsh Fish PowerShell — Any Version
- Recipe ready to paste into any shell alias or script, zero cloud cost, pipes work out of the box
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default and can optionally reuse `sqlite-graphrag daemon` for heavy embedding commands
- Purpose is to compose sqlite-graphrag with classic Unix and Windows shell pipelines seamlessly
- Use `sqlite-graphrag recall "$query" --json | jaq '.hits[].name'` in any POSIX-compatible shell
- Minimum version supports any recent Bash Zsh Fish or PowerShell 7 release
- Official docs live at https://www.gnu.org/software/bash and respective shell project homepages
- Golden tip is to quote variables explicitly to avoid word splitting in queries with spaces


## Nushell
### Nushell — Structured Data Pipeline Integration
- Recipe ready to paste into a Nushell script, zero cloud cost, output becomes native Nu table
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess via `^` sigil in Nu
- Purpose is to compose sqlite-graphrag output with Nushell structured data pipelines natively
- Use `^sqlite-graphrag recall "query" --k 5 --json | from json | get results` to query memory
- Minimum version supports Nushell 0.90 or later for stable external command and `from json` pipeline
- Official docs live at https://www.nushell.sh/book describing external commands and JSON parsing
- Golden tip is to pipe results into `select name score` to display a ranked memory table in Nu


## GitHub Actions
### CI/CD — Any Recent Runner Image
- Recipe ready to copy into `.github/workflows/`, zero cloud cost, runs on any GitHub runner image
- While MCPs require a dedicated server, sqlite-graphrag installs in seconds via cargo on any runner
- Purpose is to run memory maintenance and backups inside scheduled GitHub Actions workflows
- Use a scheduled cron workflow that runs `sqlite-graphrag purge --days 30 --yes` and `vacuum`
- Minimum version works on any `ubuntu-latest`, `macos-latest` or `windows-latest` GitHub runner
- Official docs live at https://docs.github.com/actions describing scheduled workflows syntax
- Golden tip is to upload the sync-safe-copy output as a build artifact for rollback capability


## GitLab CI
### CI/CD — Any Recent Runner
- Recipe ready to copy into `.gitlab-ci.yml`, zero cloud cost, runs on any GitLab runner image
- While MCPs require a dedicated server, sqlite-graphrag installs in seconds via cargo on any runner
- Purpose is to run sqlite-graphrag maintenance inside GitLab CI scheduled pipelines routinely
- Use a scheduled `.gitlab-ci.yml` stage invoking `cargo install --path .` first
- Minimum version supports any recent GitLab runner image with Rust toolchain available for install
- Official docs live at https://docs.gitlab.com/ee/ci describing scheduled pipelines configuration
- Golden tip is to cache the cargo install directory between runs for faster job startup times


## CircleCI
### CI/CD — Any Recent Executor
- Recipe ready to copy into CircleCI config, zero cloud cost, binary installs via cargo in seconds
- While MCPs require a dedicated server, sqlite-graphrag installs in seconds via cargo on any executor
- Purpose is to run sqlite-graphrag maintenance and backups inside CircleCI scheduled workflows
- Use a scheduled workflow with `cargo install --path .` followed by the job steps
- Minimum version supports any recent CircleCI Linux or macOS executor with Rust toolchain
- Official docs live at https://circleci.com/docs describing scheduled pipelines and workflows
- Golden tip is to persist the DB to workspace storage so downstream jobs can audit the snapshot


## Jenkins
### CI/CD — Jenkins 2.400+
- Recipe ready to paste into a Jenkinsfile stage, zero cloud cost, works in air-gapped environments
- While MCPs require a dedicated server, sqlite-graphrag installs via cargo and can stay subprocess-only or enable `sqlite-graphrag daemon` for lower latency
- Purpose is to integrate sqlite-graphrag backups into self-hosted Jenkins pipelines for regulated environments
- Use a Jenkinsfile stage running `cargo install --path .` and the operational commands
- Minimum version requires Jenkins 2.400 or later for stable pipeline and agent management features
- Official docs live at https://www.jenkins.io/doc covering declarative pipeline syntax in depth
- Golden tip is to archive the sync-safe-copy output as a build artifact for long-term retention


## Docker and Podman Alpine
### Container — Any Recent Version
- Recipe ready to copy into a Dockerfile, zero cloud cost, final image fits under 25 MB Alpine
- While MCPs require a dedicated server, sqlite-graphrag is a single static binary with no runtime deps
- Purpose is to package sqlite-graphrag in minimal Alpine images for reproducible production deployments
- Use a multi-stage Dockerfile with a Rust builder stage and an Alpine runtime copying the binary
- Minimum version supports any Docker or Podman release compatible with multi-stage build syntax
- Official docs live at https://docs.docker.com covering multi-stage build and image minimization
- Golden tip is to mount the SQLite file as a named volume to persist memory across container restarts


## Kubernetes Jobs And CronJobs
### Kubernetes — 1.25+
- Recipe ready to copy into a CronJob manifest, zero cloud cost, runs inside your existing cluster
- While MCPs require a dedicated server, sqlite-graphrag runs as a one-shot Job with no sidecar needed
- Purpose is to run sqlite-graphrag maintenance as Kubernetes CronJobs inside managed production clusters
- Use a CronJob manifest referencing the Alpine image and invoking purge plus vacuum on schedule
- Minimum version requires Kubernetes 1.25 or later for stable CronJob and concurrency policy support
- Official docs live at https://kubernetes.io/docs describing Job CronJob and PersistentVolumeClaim
- Golden tip is to mount the DB from a PVC with access mode `ReadWriteOnce` for data safety


## Homebrew
### Package Manager — macOS And Linux
- Recipe ready to run once the formula lands, zero cloud cost, installs the same binary as cargo
- While MCPs require a dedicated server, sqlite-graphrag is a single binary with no runtime dependency
- Purpose is to install sqlite-graphrag on macOS and Linux with the familiar Homebrew package manager
- Use `brew install sqlite-graphrag` once the official formula lands on the Homebrew core taps
- Minimum version supports any Homebrew 4.0 or later release on macOS or Linuxbrew distributions
- Official docs live at https://brew.sh explaining formula discovery and installation conventions
- Golden tip is to pin the release via `brew install sqlite-graphrag@1.2.1` once versioned taps exist


## Scoop And Chocolatey
### Package Manager — Windows
- Recipe ready to run once the manifest lands, zero cloud cost, installs the same binary as cargo
- While MCPs require a dedicated server, sqlite-graphrag is a single exe with no runtime dependency
- Purpose is to install sqlite-graphrag on Windows with Scoop or Chocolatey familiar to Windows developers
- Use `scoop install sqlite-graphrag` or `choco install sqlite-graphrag` once official manifests land
- Minimum version supports any Scoop 0.3 or Chocolatey 2.0 release with modern manifest features
- Official docs live at https://scoop.sh and https://chocolatey.org explaining manifest conventions
- Golden tip is to run the binary inside the target project folder so it creates `graphrag.sqlite` there


## Nix And Flakes
### Package Manager — Any Nix Version
- Recipe ready to add as a flake input, zero cloud cost, binary hash is pinned for reproducibility
- While MCPs require a dedicated server, sqlite-graphrag runs as a pure binary in any Nix dev shell
- Purpose is to install sqlite-graphrag in reproducible Nix environments including NixOS and dev shells
- Use `nix run github:daniloaguiarbr/sqlite-graphrag#sqlite-graphrag` to execute without installation
- Minimum version requires Nix 2.4 or later with Flakes feature enabled in user configuration
- Official docs live at https://nixos.org describing Flakes enablement and usage from command line
- Golden tip is to pin the flake input hash so the binary stays reproducible across every rebuild
