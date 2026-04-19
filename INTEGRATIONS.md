# Integrations


> 21 agents and 20+ platforms in a single CLI contract

- Read the Portuguese version at [INTEGRATIONS.pt-BR.md](INTEGRATIONS.pt-BR.md)


## Summary Table
### Catalog — Every Supported Integration
| Name | Type | Minimum Version | Example | Official Docs |
| --- | --- | --- | --- | --- |
| Claude Code | AI Agent | 1.0+ | `neurographrag recall "query" --json` | https://docs.anthropic.com/claude-code |
| Codex CLI | AI Agent | 0.5+ | `neurographrag remember --name X --type user --body "..."` | https://github.com/openai/codex |
| Gemini CLI | AI Agent | any recent | `neurographrag hybrid-search "query" --k 5 --json` | https://github.com/google-gemini/gemini-cli |
| Opencode | AI Agent | any recent | `neurographrag recall "auth flow" --json` | https://github.com/opencode-ai/opencode |
| OpenClaw | AI Agent | any recent | `neurographrag list --type user --json` | community project |
| Paperclip | AI Agent | any recent | `neurographrag read --name note --json` | community project |
| VS Code Copilot | AI Agent | 1.90+ | tasks.json | https://code.visualstudio.com/docs/copilot |
| Google Antigravity | AI Agent | any recent | `neurographrag hybrid-search "prompt" --json` | Google Antigravity docs |
| Windsurf | AI Agent | any recent | `neurographrag recall "refactor plan" --json` | https://windsurf.com/docs |
| Cursor | AI Agent | 0.40+ | `neurographrag remember --name cursor-ctx --type agent --body "..."` | https://cursor.com/docs |
| Zed | AI Agent | any recent | `neurographrag recall "open tabs" --json` | https://zed.dev/docs |
| Aider | AI Agent | 0.60+ | `neurographrag recall "refactor" --k 5 --json` | https://aider.chat |
| Jules | AI Agent | preview | `neurographrag stats --json` | https://jules.google |
| Kilo Code | AI Agent | any recent | `neurographrag recall "tasks" --json` | community project |
| Roo Code | AI Agent | any recent | `neurographrag hybrid-search "repo ctx" --json` | community project |
| Cline | AI Agent | VS Code ext | `neurographrag list --limit 20 --json` | https://cline.bot |
| Continue | AI Agent | VS Code or JetBrains | `neurographrag recall "docstring" --json` | https://docs.continue.dev |
| Factory | AI Agent | any recent | `neurographrag recall "pr context" --json` | https://factory.ai |
| Augment Code | AI Agent | any recent | `neurographrag hybrid-search "review" --json` | https://docs.augmentcode.com |
| JetBrains AI Assistant | AI Agent | 2024.2+ | `neurographrag recall "stacktrace" --json` | https://www.jetbrains.com/ai |
| OpenRouter | AI Router | any | `neurographrag recall "rule" --json` | https://openrouter.ai/docs |
| POSIX Shells | Shell | any | `neurographrag recall "$query" --json` | https://www.gnu.org/software/bash |
| GitHub Actions | CI/CD | any | workflow YAML | https://docs.github.com/actions |
| GitLab CI | CI/CD | any | `.gitlab-ci.yml` | https://docs.gitlab.com/ee/ci |
| CircleCI | CI/CD | any | `.circleci/config.yml` | https://circleci.com/docs |
| Jenkins | CI/CD | 2.400+ | Jenkinsfile | https://www.jenkins.io/doc |
| Docker and Podman Alpine | Container | any | Dockerfile | https://docs.docker.com |
| Kubernetes | Orchestrator | 1.25+ | Job or CronJob | https://kubernetes.io/docs |
| Homebrew | Package Manager | macOS and Linux | `brew install neurographrag` (planned) | https://brew.sh |
| Scoop and Chocolatey | Package Manager | Windows | `scoop install neurographrag` (planned) | https://scoop.sh and https://chocolatey.org |
| Nix and Flakes | Package Manager | any | `nix run .#neurographrag` | https://nixos.org |


## Claude Code
### Anthropic Agent — Subprocess Integration
- Purpose is to persist context across Claude Code sessions without external memory services
- Use `neurographrag recall "$USER_PROMPT" --k 5 --json` in a pre-task hook to inject context
- Minimum version requires Claude Code 1.0 or later for stable `.claude/hooks/` directory support
- Official docs live at https://docs.anthropic.com/claude-code describing hook lifecycle events
- Golden tip is to capture exit code `75` as retry-later and keep the agent alive gracefully


## Codex CLI
### OpenAI Agent — AGENTS.md Driven Subprocess
- Purpose is to expose the memory contract through the native `AGENTS.md` convention
- Use `neurographrag recall "<query>" --k 5 --json` documented inside `AGENTS.md` at repo root
- Minimum version requires Codex CLI 0.5 or later for deterministic AGENTS.md parsing rules
- Official docs live at https://github.com/openai/codex covering AGENTS.md discovery order
- Golden tip is to include a working invocation example under each listed command for Codex


## Gemini CLI
### Google Agent — Subprocess With JSON Contract
- Purpose is to inject memory into Gemini 2.5 Pro prompts during long coding sessions
- Use `neurographrag hybrid-search "query" --k 5 --json` for recall with mixed keyword intent
- Minimum version supports any recent Gemini CLI release with subprocess invocation enabled
- Official docs live at https://github.com/google-gemini/gemini-cli for tool integration patterns
- Golden tip is to set `NEUROGRAPHRAG_LANG=pt` when prompting Gemini in Portuguese contexts


## Opencode
### Community Agent — Subprocess Integration
- Purpose is to persist multi-turn context in the open source Opencode orchestration loop
- Use `neurographrag recall "$query" --json` as part of the Opencode pre-generation pipeline
- Minimum version supports any recent Opencode release exposing a plugin subprocess hook
- Official project lives at https://github.com/opencode-ai/opencode with community issue tracker
- Golden tip is to set the namespace to the repo slug to avoid cross-project memory leakage


## OpenClaw
### Community Agent — Subprocess Driver
- Purpose is to inject persistent memory into OpenClaw agent loops without plugin rebuild
- Use `neurographrag list --type user --json` to fetch seed context at the start of a run
- Minimum version supports any recent OpenClaw release able to shell out to CLI binaries
- Official docs live inside the OpenClaw GitHub README explaining subprocess integration rules
- Golden tip is to configure `NEUROGRAPHRAG_DB_PATH` once per session to avoid path surprises


## Paperclip
### Community Agent — Subprocess Client
- Purpose is to persist cross-session memory in the Paperclip autonomous developer agent
- Use `neurographrag read --name onboarding-note --json` to seed the session with prior notes
- Minimum version supports any recent Paperclip release that can spawn child subprocess calls
- Official docs live in the Paperclip community repository describing subprocess hook contracts
- Golden tip is to run `health --json` at startup and abort when integrity reports any damage


## VS Code Copilot
### Microsoft Agent — tasks.json Integration
- Purpose is to surface relevant memory from a selection inside VS Code Copilot chat panels
- Use the example tasks.json entry that calls `neurographrag recall "$selection" --json`
- Minimum version requires VS Code 1.90 or later for the latest tasks.json variable substitutions
- Official docs live at https://code.visualstudio.com/docs/copilot covering chat tool registration
- Golden tip is to bind the task to `Cmd+Shift+M` for single-keystroke memory recall invocation


## Google Antigravity
### Google Agent — Runner Integration
- Purpose is to run neurographrag as a first-class runner inside Antigravity pipelines at scale
- Use `neurographrag hybrid-search "$PROMPT" --json --k 10` as the retrieval step in a runner
- Minimum version supports any recent Antigravity release that accepts arbitrary runner binaries
- Official docs live on the Google Antigravity product page describing runner configuration format
- Golden tip is to run `sync-safe-copy` before each pipeline to guard the shared memory artifact


## Windsurf
### Codeium Agent — Terminal Integration
- Purpose is to expose memory recall to Windsurf assistant panels via terminal task invocation
- Use `neurographrag recall "$EDITOR_CONTEXT" --json` mapped to a Windsurf Run task binding
- Minimum version supports any recent Windsurf release with terminal task execution enabled
- Official docs live at https://windsurf.com/docs describing the terminal task binding syntax
- Golden tip is to persist results to `/tmp/ng.json` so Windsurf prompt templates can read them


## Cursor
### Cursor Agent — Terminal Integration
- Purpose is to pair Cursor AI with a local memory backend that survives editor restarts
- Use `neurographrag remember --name cursor-ctx --type agent --body "$SELECTION"` from a key binding
- Minimum version requires Cursor 0.40 or later for stable AI rules and terminal env override
- Official docs live at https://cursor.com/docs covering AI rules and terminal integration patterns
- Golden tip is to set `NEUROGRAPHRAG_NAMESPACE=${workspaceFolderBasename}` per project workspace


## Zed
### Zed Industries Agent — Assistant Panel Integration
- Purpose is to wire memory recall into the Zed assistant panel without custom extensions
- Use `neurographrag recall "open tabs" --json --k 5` as a terminal command available to Zed
- Minimum version supports any recent Zed release with the assistant panel and terminal tasks
- Official docs live at https://zed.dev/docs describing assistant panel and terminal integration
- Golden tip is to define a Zed task profile sharing memory across multiple open workspaces


## Aider
### Open Source Agent — Shell Integration
- Purpose is to augment Aider pair programming with durable memory across git repositories
- Use `neurographrag recall "refactor target" --k 5 --json` invoked before each Aider prompt
- Minimum version requires Aider 0.60 or later for stable subprocess and hook invocation
- Official docs live at https://aider.chat describing configuration and custom shell commands
- Golden tip is to scope memory by repository via `NEUROGRAPHRAG_NAMESPACE=$(basename $(pwd))`


## Jules
### Google Labs Agent — CI Automation
- Purpose is to run memory maintenance inside Jules preview automation pipelines automatically
- Use `neurographrag stats --json` as a CI step to monitor memory growth week over week
- Minimum version is the current Jules preview release available via Google Labs early access
- Official docs live at https://jules.google explaining CI job configuration and authentication
- Golden tip is to fail the pipeline when `stats.memories` exceeds agreed thresholds for a project


## Kilo Code
### Community Agent — Subprocess Integration
- Purpose is to expose a persistent memory layer to the Kilo Code autonomous engineering agent
- Use `neurographrag recall "recent tasks" --json` at the start of every Kilo Code agent run
- Minimum version supports any recent Kilo Code release capable of spawning child processes
- Official docs live in the Kilo Code community repository describing the subprocess contract
- Golden tip is to log exit code `75` as retryable rather than fatal when orchestrator is busy


## Roo Code
### Community Agent — Subprocess Integration
- Purpose is to inject memory into Roo Code agent prompts for deeper repository understanding
- Use `neurographrag hybrid-search "repo context" --json` for recall across mixed query types
- Minimum version supports any recent Roo Code release with hook capabilities for subprocess
- Official docs live in the Roo Code community repository explaining hook lifecycle conventions
- Golden tip is to chain `related <name> --hops 2` after recall for multi-hop graph expansion


## Cline
### Community VS Code Extension — Terminal Integration
- Purpose is to give Cline persistent memory across VS Code sessions without cloud services
- Use `neurographrag list --limit 20 --json` as a seed step at Cline conversation startup
- Minimum version supports the current Cline VS Code extension release in the marketplace
- Official docs live at https://cline.bot covering terminal tool registration and usage patterns
- Golden tip is to bind the command to a Cline tool with descriptive name and usage explanation


## Continue
### Open Source Agent — IDE Terminal Integration
- Purpose is to surface neurographrag memory inside Continue chat panels in VS Code or JetBrains
- Use `neurographrag recall "docstring" --json` from a Continue custom command registration
- Minimum version supports any recent Continue extension release in VS Code or JetBrains stores
- Official docs live at https://docs.continue.dev describing custom commands and tool integration
- Golden tip is to document each command in the Continue config so the embedded LLM picks it up


## Factory
### Factory Agent — API Or Subprocess
- Purpose is to integrate neurographrag with Factory autonomous development droids in production
- Use `neurographrag recall "pr context" --json` during the Factory droid plan preparation phase
- Minimum version supports any recent Factory release with subprocess or API tool integration
- Official docs live at https://factory.ai explaining droid tool configuration and plan execution
- Golden tip is to set a long `--wait-lock` for Factory droids running under heavy concurrency


## Augment Code
### Augment Agent — IDE Integration
- Purpose is to feed Augment Code review agents with persistent cross-repository memory state
- Use `neurographrag hybrid-search "code review" --json` inside Augment IDE review preparation
- Minimum version supports any recent Augment Code release with terminal and subprocess hooks
- Official docs live at https://docs.augmentcode.com describing tool registration and agents
- Golden tip is to enable `--lang en` explicitly for consistent review language across teams


## JetBrains AI Assistant
### JetBrains Agent — IDE Integration
- Purpose is to add neurographrag memory to JetBrains AI Assistant across IntelliJ PyCharm WebStorm
- Use `neurographrag recall "$SELECTION" --json` registered as a JetBrains external tool runner
- Minimum version requires JetBrains AI Assistant 2024.2 or later for modern tool registration
- Official docs live at https://www.jetbrains.com/ai explaining tool and external runner registration
- Golden tip is to bind the tool to a keyboard shortcut to invoke recall with one hand on keyboard


## OpenRouter
### Multi-LLM Router — Any Version Supported
- Purpose is to share a common memory backend across every OpenRouter-hosted LLM in a pipeline
- Use `neurographrag recall "routing rule" --json` as a preamble step before any routed request
- Minimum version supports any OpenRouter API release since memory remains local and independent
- Official docs live at https://openrouter.ai/docs explaining routing rules and API integration
- Golden tip is to reuse the same namespace across all routed models for consistent context


## POSIX Shells
### Bash Zsh Fish PowerShell Nushell — Any Version
- Purpose is to compose neurographrag with classic Unix and Windows shell pipelines seamlessly
- Use `neurographrag recall "$query" --json | jaq '.hits[].name'` in any POSIX-compatible shell
- Minimum version supports any recent Bash Zsh Fish PowerShell 7 or Nushell 0.90 and later
- Official docs live at https://www.gnu.org/software/bash and respective shell project homepages
- Golden tip is to quote variables explicitly to avoid word splitting in queries with spaces


## GitHub Actions
### CI/CD — Any Recent Runner Image
- Purpose is to run memory maintenance and backups inside scheduled GitHub Actions workflows
- Use a scheduled cron workflow that runs `neurographrag purge --days 30 --yes` and `vacuum`
- Minimum version works on any `ubuntu-latest`, `macos-latest` or `windows-latest` GitHub runner
- Official docs live at https://docs.github.com/actions describing scheduled workflows syntax
- Golden tip is to upload the sync-safe-copy output as a build artifact for rollback capability


## GitLab CI
### CI/CD — Any Recent Runner
- Purpose is to run neurographrag maintenance inside GitLab CI scheduled pipelines routinely
- Use a scheduled `.gitlab-ci.yml` stage invoking `cargo install --locked neurographrag` first
- Minimum version supports any recent GitLab runner image with Rust toolchain available for install
- Official docs live at https://docs.gitlab.com/ee/ci describing scheduled pipelines configuration
- Golden tip is to cache the cargo install directory between runs for faster job startup times


## CircleCI
### CI/CD — Any Recent Executor
- Purpose is to run neurographrag maintenance and backups inside CircleCI scheduled workflows
- Use a scheduled workflow with `cargo install --locked neurographrag` followed by the job steps
- Minimum version supports any recent CircleCI Linux or macOS executor with Rust toolchain
- Official docs live at https://circleci.com/docs describing scheduled pipelines and workflows
- Golden tip is to persist the DB to workspace storage so downstream jobs can audit the snapshot


## Jenkins
### CI/CD — Jenkins 2.400+
- Purpose is to integrate neurographrag backups into self-hosted Jenkins pipelines for regulated environments
- Use a Jenkinsfile stage running `cargo install --locked neurographrag` and the operational commands
- Minimum version requires Jenkins 2.400 or later for stable pipeline and agent management features
- Official docs live at https://www.jenkins.io/doc covering declarative pipeline syntax in depth
- Golden tip is to archive the sync-safe-copy output as a build artifact for long-term retention


## Docker and Podman Alpine
### Container — Any Recent Version
- Purpose is to package neurographrag in minimal Alpine images for reproducible production deployments
- Use a multi-stage Dockerfile with a Rust builder stage and an Alpine runtime copying the binary
- Minimum version supports any Docker or Podman release compatible with multi-stage build syntax
- Official docs live at https://docs.docker.com covering multi-stage build and image minimization
- Golden tip is to mount the SQLite file as a named volume to persist memory across container restarts


## Kubernetes Jobs And CronJobs
### Kubernetes — 1.25+
- Purpose is to run neurographrag maintenance as Kubernetes CronJobs inside managed production clusters
- Use a CronJob manifest referencing the Alpine image and invoking purge plus vacuum on schedule
- Minimum version requires Kubernetes 1.25 or later for stable CronJob and concurrency policy support
- Official docs live at https://kubernetes.io/docs describing Job CronJob and PersistentVolumeClaim
- Golden tip is to mount the DB from a PVC with access mode `ReadWriteOnce` for data safety


## Homebrew
### Package Manager — macOS And Linux
- Purpose is to install neurographrag on macOS and Linux with the familiar Homebrew package manager
- Use `brew install neurographrag` once the official formula lands on the Homebrew core taps
- Minimum version supports any Homebrew 4.0 or later release on macOS or Linuxbrew distributions
- Official docs live at https://brew.sh explaining formula discovery and installation conventions
- Golden tip is to pin the release via `brew install neurographrag@1.2.1` once versioned taps exist


## Scoop And Chocolatey
### Package Manager — Windows
- Purpose is to install neurographrag on Windows with Scoop or Chocolatey familiar to Windows developers
- Use `scoop install neurographrag` or `choco install neurographrag` once official manifests land
- Minimum version supports any Scoop 0.3 or Chocolatey 2.0 release with modern manifest features
- Official docs live at https://scoop.sh and https://chocolatey.org explaining manifest conventions
- Golden tip is to set `NEUROGRAPHRAG_HOME` to a path under `%USERPROFILE%` for per-user isolation


## Nix And Flakes
### Package Manager — Any Nix Version
- Purpose is to install neurographrag in reproducible Nix environments including NixOS and dev shells
- Use `nix run github:daniloaguiarbr/neurographrag#neurographrag` to execute without installation
- Minimum version requires Nix 2.4 or later with Flakes feature enabled in user configuration
- Official docs live at https://nixos.org describing Flakes enablement and usage from command line
- Golden tip is to pin the flake input hash so the binary stays reproducible across every rebuild
