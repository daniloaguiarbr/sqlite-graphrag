# sqlite-graphrag for AI Agents


> Persistent memory for 27 AI agents in a single 25 MB Rust binary

- Read the Portuguese version at [AGENTS.pt-BR.md](AGENTS.pt-BR.md)


## CLI Flag Aliases (since v1.0.35)
- `recall` and `hybrid-search` accept `--limit` as an alias of `-k`/`--k`. Existing snippets below use `--k` and remain valid.
- `rename` accepts `--from`/`--to` as aliases of `--name`/`--new-name`.
- `rename` accepts positional arguments: `rename <old> <new>` (since v1.0.44)
- `related` accepts a positional name argument: `related <name>` (since v1.0.44)
- `graph entities` JSON response uses `entities` as the top-level array key (renamed from `items` in v1.0.44)
- `schema_version` JSON fields (`init`, `stats`, `migrate`, `health`) are emitted as JSON numbers since v1.0.35.


## The Question No Agent Framework Answers
### Open Loop — Why 27 AI Agents Choose This As Their Memory Layer
- Why do 27 AI agents choose sqlite-graphrag as their persistent memory layer?
- Three technical reasons: durable local memory, zero cloud dependencies, deterministic JSON
- Each agent gains persistent memory without spending a single additional token
- Versus heavy MCPs, sqlite-graphrag delivers a deterministic stdin/stdout contract
- The secret the frameworks never document sits inside a single portable SQLite file


## Why Agents Love This CLI
### Five Differentiators — Engineered for Autonomous Loops
- Deterministic JSON output removes every parser hack from your orchestrator code
- Exit codes follow `sysexits.h` so your retry logic works without string matching
- No Python or Node runtime ships alongside the Rust CLI binary
- Stdin accepts structured payloads so your agents never escape shell arguments
- Heavy embedding commands can auto-start and reuse `sqlite-graphrag daemon` instead of paying cold-start on every loop
- Cross-platform behavior stays identical on Linux, macOS and Windows out of the box
- Default behavior always creates or opens `graphrag.sqlite` in the current working directory


## Economy That Converts
### Numbers That Sell The Switch
- Remove recurring cloud vector database dependencies from local agent workflows
- Keep retrieval local to the workstation or CI runner instead of a remote RAG stack
- Reduce the operational surface to one SQLite file and one CLI binary
- Reuse the daemon on heavy commands instead of paying full cold-start every loop
- Preserve orchestration determinism through stable JSON and stable exit codes


## Sovereignty as Competitive Advantage
### Why Local Memory Wins In 2026
- Your proprietary data NEVER leaves the developer workstation or the CI runner
- Your compliance surface shrinks to one SQLite file under your own encryption
- Your vendor lock-in vanishes since the schema is documented and portable
- Your audit trail lives in the `memory_versions` table with immutable history
- Your regulated industry gets offline-first RAG without cloud dependency clauses


## Compatible Agents and Orchestrators
### Catalog — 27 Supported Integrations
| Agent | Vendor | Minimum Version | Integration Type | Example |
| --- | --- | --- | --- | --- |
| Claude Code | Anthropic | 1.0+ | Subprocess | `sqlite-graphrag recall "query" --json` |
| Codex CLI | OpenAI | 0.5+ | AGENTS.md + subprocess | `sqlite-graphrag remember --name X --type user --description "..." --body "..."` |
| Gemini CLI | Google | any recent | Subprocess | `sqlite-graphrag hybrid-search "query" --json --k 5` |
| Opencode | open source | any recent | Subprocess | `sqlite-graphrag recall "auth flow" --json --k 3` |
| OpenClaw | community | any recent | Subprocess | `sqlite-graphrag recall "auth flow" --json --k 3` |
| Paperclip | community | any recent | Subprocess | `sqlite-graphrag read --name onboarding-note --json` |
| VS Code Copilot | Microsoft | 1.90+ | tasks.json | `{"command": "sqlite-graphrag", "args": ["recall", "$selection", "--json"]}` |
| Google Antigravity | Google | any recent | Runner | `sqlite-graphrag hybrid-search "prompt" --k 10 --json` |
| Windsurf | Codeium | any recent | Terminal | `sqlite-graphrag recall "refactor plan" --json` |
| Cursor | Cursor | 0.40+ | Terminal | `sqlite-graphrag remember --name cursor-ctx --type project --description "..." --body "..."` |
| Zed | Zed Industries | any recent | Assistant Panel | `sqlite-graphrag recall "open tabs" --json --k 5` |
| Aider | open source | 0.60+ | Shell | `sqlite-graphrag recall "refactor target" --k 5 --json` |
| Jules | Google Labs | preview | CI automation | `sqlite-graphrag stats --json` |
| Kilo Code | community | any recent | Subprocess | `sqlite-graphrag recall "recent tasks" --json` |
| Roo Code | community | any recent | Subprocess | `sqlite-graphrag hybrid-search "repo context" --json` |
| Cline | community | VS Code ext | Terminal | `sqlite-graphrag list --limit 20 --json` |
| Continue | open source | VS Code or JetBrains ext | Terminal | `sqlite-graphrag recall "docstring" --json` |
| Factory | Factory | any recent | API or subprocess | `sqlite-graphrag recall "pr context" --json` |
| Augment Code | Augment | any recent | IDE | `sqlite-graphrag hybrid-search "code review" --json` |
| JetBrains AI Assistant | JetBrains | 2024.2+ | IDE | `sqlite-graphrag recall "stacktrace" --json` |
| OpenRouter | OpenRouter | any | Router for multi-LLM | `sqlite-graphrag recall "routing rule" --json` |
| Minimax | Minimax | any recent | Subprocess | `sqlite-graphrag recall "user preferences" --json --k 5` |
| Z.ai | Z.ai | any recent | Subprocess | `sqlite-graphrag hybrid-search "task context" --json --k 10` |
| Ollama | Ollama | 0.1+ | Subprocess | `sqlite-graphrag remember --name ollama-ctx --type project --description "..." --body "..."` |
| Hermes Agent | community | any recent | Subprocess | `sqlite-graphrag recall "tool call history" --json` |
| LangChain | LangChain | 0.3+ | Subprocess via tool | `sqlite-graphrag hybrid-search "chain context" --json --k 5` |
| LangGraph | LangChain | 0.2+ | Subprocess via node | `sqlite-graphrag recall "graph state" --json --k 3` |


## Agent Integration Details
### Minimax
- Open-source multimodal agent with video, audio, and text reasoning capabilities
- Invoke sqlite-graphrag as subprocess from within a Minimax tool definition:
```bash
sqlite-graphrag recall "user session context" --json --k 5
```
- Output: JSON with `results` entries carrying `name`, `snippet`, `distance`, and `source`

### Z.ai
- Hosted agent platform with multi-step task planning and tool orchestration
- Invoke sqlite-graphrag to persist inter-session memory across planning cycles:
```bash
sqlite-graphrag remember --name "task-plan-$(date +%s)" --type project --description "Z.ai task plan" --body "$PLAN"
sqlite-graphrag recall "previous task plan" --json --k 3
```
- Output: deterministic JSON with `results`, `direct_matches`, and `graph_matches`

### Ollama
- Local LLM server running open models on consumer hardware without cloud calls
- Invoke sqlite-graphrag as a tool to give Ollama agents persistent knowledge:
```bash
sqlite-graphrag recall "conversation history" --json --k 5
sqlite-graphrag remember --name "ollama-session" --type project --description "Ollama conversation" --body "$CONTEXT"
```
- Output: deterministic recall JSON with `elapsed_ms` and stable result fields

### Hermes Agent
- Community agent framework designed for ReAct-style tool-calling loops
- Invoke sqlite-graphrag at the start of each ReAct cycle to load prior context:
```bash
sqlite-graphrag hybrid-search "tool call results" --json --k 5
```
- Output: hybrid-search JSON combining BM25 full-text and cosine vector ranking

### LangChain
- Python orchestration framework for LLM chains with tool and retriever abstractions
- Invoke sqlite-graphrag as a custom retriever tool via subprocess from LangChain Python:
```bash
sqlite-graphrag hybrid-search "chain input query" --json --k 10 --lang en
```
- Output: JSON `results` array consumable by `json.loads` in the LangChain tool wrapper

### LangGraph
- Graph-based state machine framework for multi-agent workflows built on LangChain
- Invoke sqlite-graphrag inside each graph node to persist and recall inter-node state:
```bash
sqlite-graphrag recall "graph node output" --json --k 3
sqlite-graphrag remember --name "node-result-$(date +%s)" --type project --description "LangGraph node output" --body "$OUTPUT"
```
- Output: structured JSON enabling stateful graph traversal across LangGraph runs


## Rust Crate Integrations
### Agent and LLM Crates — Call sqlite-graphrag as a Subprocess
- Every Rust crate that spawns an LLM agent can call sqlite-graphrag via `std::process::Command`
- Deterministic subprocess recall lets Rust crates reuse a stable memory contract
- Zero additional tokens: memory lives in SQLite, not inside the context window
- Each crate gains persistent memory without importing any sqlite-graphrag dependency

### rig-core
- Modular framework for building LLM pipelines, RAG systems, and autonomous agents
- Cargo.toml:
```toml
[dependencies]
rig-core = "0.35.0"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "project context", "--json"])
    .output()?;
```
- Case: persist agent tool results across rig pipeline invocations without tokens

### swarms-rs
- Multi-agent orchestration framework with native MCP support and swarm topologies
- Cargo.toml:
```toml
[dependencies]
swarms-rs = "0.2.1"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "swarm task result", "--json", "--k", "5"])
    .output()?;
```
- Case: share persistent context across swarm agents without a central vector DB

### autoagents
- Multi-agent runtime with Ractor actors, ReAct loops, and WASM sandbox isolation
- Cargo.toml:
```toml
[dependencies]
autoagents = "0.3.7"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "react-step", "--type", "project", "--description", "autoagents step", "--body", "step output"])
    .output()?;
```
- Case: checkpoint ReAct intermediate steps for replay and audit in autoagents loops

### agentai
- Thin agent layer over genai with a simple ToolBox abstraction for tool registration
- Cargo.toml:
```toml
[dependencies]
agentai = "0.1.5"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "tool call context", "--json", "--k", "3"])
    .output()?;
```
- Case: inject prior tool call history into agentai ToolBox before each agent run

### llm-agent-runtime
- Full agent runtime with episodic memory, checkpointing, and tool orchestration
- Cargo.toml:
```toml
[dependencies]
llm-agent-runtime = "1.74.0"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "episode context", "--json"])
    .output()?;
```
- Case: extend llm-agent-runtime episodic memory with durable SQLite persistence

### anda
- Agent framework for trusted execution environments and ICP blockchain integrations
- Cargo.toml:
```toml
[dependencies]
anda = "0.4.10"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["read", "--name", "anda-agent-state", "--json"])
    .output()?;
```
- Case: persist verifiable agent state outside the TEE for cross-session continuity

### adk-rust
- Modular agent development kit inspired by LangChain and Autogen patterns
- Cargo.toml:
```toml
[dependencies]
adk-rust = "0.6.0"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "agent memory query", "--json", "--k", "10"])
    .output()?;
```
- Case: replace adk-rust in-memory context store with persistent graph-native recall

### genai
- Unified API client for OpenAI, Anthropic, Gemini, xAI, and Ollama in one crate
- Cargo.toml:
```toml
[dependencies]
genai = "0.6.0-beta.17"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "llm response cache", "--json"])
    .output()?;
```
- Case: cache expensive genai LLM responses in sqlite-graphrag for cross-run reuse

### liter-llm
- Universal LLM client supporting 143 plus providers with OpenTelemetry tracing built in
- Cargo.toml:
```toml
[dependencies]
liter-llm = "1.2.1"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "litellm-trace", "--type", "project", "--description", "liter-llm trace", "--body", "trace payload"])
    .output()?;
```
- Case: store OpenTelemetry trace snapshots in sqlite-graphrag for agent replay

### llm-cascade
- LLM cascade client with automatic failover and circuit breaker across providers
- Cargo.toml:
```toml
[dependencies]
llm-cascade = "0.1.0"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "fallback provider result", "--json"])
    .output()?;
```
- Case: persist cascade decisions so the circuit breaker learns from prior failures

### async-openai
- Rust-native async client for the full OpenAI REST API with type-safe models
- Cargo.toml:
```toml
[dependencies]
async-openai = "0.34.0"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "openai assistant output", "--json", "--k", "5"])
    .output()?;
```
- Case: store assistant thread messages in sqlite-graphrag for durable cross-session recall

### anthropic-sdk
- Direct Rust client for the Anthropic API including tool use and streaming responses
- Cargo.toml:
```toml
[dependencies]
anthropic-sdk = "0.1.5"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "claude conversation context", "--json"])
    .output()?;
```
- Case: inject prior Claude conversation turns from sqlite-graphrag before each API call

### ollama-rs
- Idiomatic Rust client for the Ollama local inference server API
- Cargo.toml:
```toml
[dependencies]
ollama-rs = "0.3.4"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "ollama-output", "--type", "project", "--description", "ollama-rs output", "--body", "generated text"])
    .output()?;
```
- Case: persist ollama-rs generation outputs for retrieval in subsequent inference calls

### llama-cpp-rs
- Rust bindings for llama.cpp enabling on-device inference with quantized models
- Cargo.toml:
```toml
[dependencies]
llama-cpp-rs = "0.3.0"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "on-device inference context", "--json", "--k", "5"])
    .output()?;
```
- Case: load persistent context into llama-cpp-rs prompt before each local inference

### mistralrs
- High-performance local inference engine for Mistral models with quantization support
- Cargo.toml:
```toml
[dependencies]
mistralrs = "0.8.1"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "mistral inference context", "--json", "--k", "5"])
    .output()?;
```
- Case: inject sqlite-graphrag persistent context into mistralrs prompts before local inference

### graphbit
- Graph-based workflow engine for deterministic LLM pipeline orchestration in Rust
- Cargo.toml:
```toml
[dependencies]
graphbit = { git = "https://github.com/graphbit-rs/graphbit" }
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "workflow node state", "--json", "--k", "3"])
    .output()?;
```
- Case: persist graphbit workflow node outputs for stateful cross-run graph traversal

### rs-graph-llm
- Typed interactive graph workflows for LLM pipelines with compile-time safety
- Cargo.toml:
```toml
[dependencies]
rs-graph-llm = { git = "https://github.com/rs-graph-llm/rs-graph-llm" }
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "graph node output", "--json", "--k", "5"])
    .output()?;
```
- Case: store rs-graph-llm typed pipeline results for persistent memory across executions


## Fundamental Principles
### REQUIRED — Usage Philosophy
- TREAT sqlite-graphrag as a local persistent memory layer
- INVOKE always as a subprocess via `std::process::Command`
- READ stdout for structured data in JSON or NDJSON
- READ stderr for tracing logs and human-readable messages
- CHECK exit code before parsing stdout
- PRESERVE context between sessions via a single SQLite file
- DELEGATE long-term memory to the binary without reimplementing
### FORBIDDEN — Anti-patterns
- NEVER expose the binary as an MCP server or HTTP service
- NEVER depend on cloud vector DBs such as Pinecone or Weaviate
- NEVER write directly to SQLite in parallel with the binary
- NEVER edit the `.sqlite` file with another tool
- NEVER assume output without validating the exit code first
- NEVER confuse `distance` with `combined_score` in ranking
- NEVER mix structured stdout with human-readable logs
- NEVER use `fd | xargs remember` when `ingest` covers the case


## Initialization and Health Check
### REQUIRED — Database Bootstrap
- RUN `sqlite-graphrag init --namespace <project>` on first use
- WAIT for offline download of the `multilingual-e5-small` model
- VALIDATE with `sqlite-graphrag health --json` before operating
- TREAT exit code 10 as a database error or corrupted database
- TREAT exit code 15 as a pending lock; widen `--wait-lock`
- ABORT pipeline when `integrity_ok` returns `false`
- RUN `migrate --json` after each binary upgrade
### REQUIRED — Continuous Monitoring
- INSPECT `wal_size_mb` in `health` to detect fragmentation
- CHECK `journal_mode` equals `wal` in production
- RUN `optimize --json` to refresh planner statistics
- DETECT schema drift via `__debug_schema` for troubleshooting
### Correct Pattern — Bootstrap Sequence
- `sqlite-graphrag init --namespace my-project`
- `sqlite-graphrag health --json | jaq '.integrity_ok'`
- `sqlite-graphrag migrate --json`
- `sqlite-graphrag stats --json | jaq '.memories'`


## Global Configuration
### REQUIRED — Database Path
- USE `--db <PATH>` when the database is not in the current directory
- SET `SQLITE_GRAPHRAG_DB_PATH` for persistent configuration
- NOTE that `--db` takes precedence over the environment variable
- DEFAULT is `graphrag.sqlite` in the current invocation directory
### REQUIRED — Namespace
- SET namespace via `--namespace` or `SQLITE_GRAPHRAG_NAMESPACE`
- VALIDATE resolution with `namespace-detect --json`
- USE `global` as the default namespace when absent
- ISOLATE projects via namespace per repository
- ADOPT `swarm-<agent_id>` for multi-agent swarms
### REQUIRED — Output Language
- USE `--lang en` or `--lang pt` to force output language
- SET `SQLITE_GRAPHRAG_LANG=en` for session override
- NOTE that `--lang` affects only human-readable stderr
- STDOUT JSON remains deterministic regardless of language
### REQUIRED — Display Timezone
- APPLY `--tz America/New_York` to localized output
- USE `SQLITE_GRAPHRAG_DISPLAY_TZ=<IANA>` to persist
- AFFECTS only `*_iso` fields in the JSON
- INTEGER epoch fields remain in UTC
- ABORT when an invalid IANA name returns exit 1 (Validation)
### REQUIRED — Log Format
- ENABLE `SQLITE_GRAPHRAG_LOG_FORMAT=json` for log aggregators
- DEFAULT `pretty` is intended for humans in the terminal only
- RAISE detail via `SQLITE_GRAPHRAG_LOG_LEVEL=debug` for diagnostics
- USE `-v`, `-vv`, `-vvv` for info, debug, and trace in subcommands
### REQUIRED — Global RAM Control
- ENABLE `SQLITE_GRAPHRAG_LOW_MEMORY=1` in constrained containers
- APPLY on hosts with less than 4 GB of available RAM
- HONORS cgroup constraints automatically when set
- TRADE-OFF is 3 to 4 times more wall-clock time
- COMBINE with the `--low-memory` flag in a specific `ingest`
### REQUIRED — ONNX Runtime on ARM64 GNU
- DISTRIBUTE `libonnxruntime.so` alongside the binary
- SET `ORT_DYLIB_PATH` explicitly in CI and systemd
- AFFECTS heavy embedding commands on `aarch64-unknown-linux-gnu`
- FAILS on the first embedding operation without the runtime accessible


## CRUD — Create with remember
### REQUIRED — Writing Individual Memories
- USE a unique kebab-case name per memory
- DECLARE `--type` from `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- PREFER `--body-stdin` for long bodies
- USE `--body-file <PATH>` to avoid shell escaping in Markdown
- PASS `--force-merge` in idempotent loops
- NER is disabled by default; pass `--enable-ner` or set `SQLITE_GRAPHRAG_ENABLE_NER=1` to activate GLiNER extraction
- `--skip-extraction` is deprecated since v1.0.45 and has no effect; NER is off by default, use `--enable-ner` to activate
- Response field `extraction_method` reports the method used: `gliner-<variant>+regex` (GLiNER succeeded), `regex-only` (GLiNER unavailable or disabled), or `none:extraction-failed` (GLiNER attempted but errored)
- RESPECT the limit of 512000 bytes and 512 chunks per body
### REQUIRED — Attaching Graph in remember
- USE `--entities-file` with a typed JSON array
- USE `--relationships-file` for typed edges
- INCLUDE the `entity_type` field in each entity object
- ACCEPT `type` as a synonym; never both at once
- USE `strength` between `0.0` and `1.0` in relationships
- MAP `from`/`to` as aliases of `source`/`target`
- USE `--graph-stdin` for a single JSON with `body`, `entities`, and `relationships`
### FORBIDDEN — Write Errors
- NEVER send `entity_type` and `type` in the same JSON object
- NEVER use `strength` outside the range `[0.0, 1.0]`
- NEVER duplicate a name without explicit `--force-merge`
- NEVER mix `--body`, `--body-file`, `--body-stdin`, `--graph-stdin`
- NEVER rely on GLiNER auto-extraction in RAM-sensitive CI
- NEVER exceed the relations cap per memory without adjusting env
- NEVER use `remember` in a loop when `ingest` covers the case
### Correct Pattern — remember Examples
- `sqlite-graphrag remember --name design-auth --type decision --description "auth JWT" --body-stdin < doc.md`
- `sqlite-graphrag remember --name doc-readme --type document --description "import" --body-file README.md --force-merge`
- `sqlite-graphrag remember --name spec-x --type reference --description "spec" --body "..." --entities-file ents.json --relationships-file rels.json`
### Valid --type Values
- `user`, `feedback`, `project`, `reference`
- `decision`, `incident`, `skill`, `document`, `note`


## CRUD — Bulk Ingest with ingest
### REQUIRED — When to Use ingest
- USE `ingest <DIR>` to import entire directories as memories
- PREFER over the `fd | xargs remember` loop in any case
- EACH file matching the pattern becomes an individual memory
- MEMORY name derives from the file basename without extension in kebab-case
- NAMES longer than 60 characters are TRUNCATED automatically
- NDJSON includes `truncated: true` and `original_name` when truncated
- AGENT must use `original_name` or `name` from NDJSON to access the memory
- OUTPUT is NDJSON, one JSON line per file plus a final summary line
- CONSUME line by line in streaming via `jaq -c` or `while read`
### REQUIRED — File Pattern with --pattern
- DEFAULT is `*.md` only; change as needed
- ACCEPT `*.<ext>` for a generic extension
- ACCEPT `<prefix>*` for a basename prefix
- ACCEPT exact filename without glob characters
- FULL POSIX glob is not supported by ingest
### REQUIRED — Recursion and Limits
- ENABLE `--recursive` to descend into subdirectories
- WITHOUT `--recursive` only top-level is processed
- RESPECT `--max-files 10000` as the default safety cap
- `--max-files` REJECTS the entire operation with exit 1 if count exceeds the cap
- `--max-files` does NOT limit to the first N; it is all-or-nothing validation
- INCREASE the cap only after auditing actual volume
- USE `--fail-fast` to stop at the first per-file failure
- WITHOUT `--fail-fast` the loop continues and reports each error in the NDJSON
### REQUIRED — Bulk Memory Type
- DECLARE `--type` applied to ALL files in the invocation
- DEFAULT is `document` when omitted
- VALID values: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- INVOKE `ingest` separately per type when mixing
- GROUP files by directory according to the desired type
### REQUIRED — RAM Control
- USE `--low-memory` in containers with less than 4 GB
- SET `SQLITE_GRAPHRAG_LOW_MEMORY=1` as a persistent override
- `--low-memory` forces `--ingest-parallelism 1` internally
- TRADE-OFF is 3 to 4 times more execution time
- CHOOSE when RSS is a greater constraint than latency
### REQUIRED — Two Parallelism Axes
- `--max-concurrency <N>` controls simultaneous CLI invocations
- `--ingest-parallelism <N>` controls extract plus embed in parallel
- DEFAULT for `--max-concurrency` is 4
- DEFAULT for `--ingest-parallelism` is `min(4, max(1, cpus/2))`
- DISTINGUISH the two axes clearly before adjusting
- WIDEN `--wait-lock <SECONDS>` to wait for a slot before exit 75
### REQUIRED — Performance and Extraction
- NER is disabled by default; pass `--enable-ner` to activate GLiNER extraction
- `--skip-extraction` is deprecated since v1.0.45 and has no effect; NER is off by default, use `--enable-ner` to activate
- Response field `extraction_method` reports the method used: `gliner-<variant>+regex` (GLiNER succeeded), `regex-only` (GLiNER unavailable or disabled), or `none:extraction-failed` (GLiNER attempted but errored)
- GLiNER NER adds approximately 100-200 ms per file with model loaded on modern hardware
- GLiNER NER adds 2 to 30 seconds per file in `--low-memory` or on first load
- GLiNER NER downloads the ONNX model on first run (fp32: 1.1 GB, int8: 349 MB via `--gliner-variant`)
- USE `--enable-ner` only when automated entity enrichment is valuable
- PREFER `--graph-stdin` with LLM-curated entities for best quality (NER is off by default)
### FORBIDDEN — ingest Anti-patterns
- NEVER use `fd | xargs sqlite-graphrag remember` when `ingest` exists
- NEVER omit `--recursive` expecting automatic descent
- NEVER pass a complex unsupported glob pattern
- NEVER ignore exit 75 for exhausted slots in automated loops
- NEVER mix different types in the same invocation
- NEVER raise `--max-files` without measuring RAM and disk first
- NEVER use `--force-merge` in ingest (flag exclusive to `remember`)
### Correct Pattern — ingest Examples
- `sqlite-graphrag ingest ./docs --recursive --pattern "*.md" --json`
- `sqlite-graphrag ingest ./decisions --type decision --json`
- `sqlite-graphrag ingest ./large-corpus --low-memory --max-files 50000 --json`
- `sqlite-graphrag ingest ./skills --type skill --recursive --fail-fast --json`
- `sqlite-graphrag ingest ./notes --type note --pattern "memo-*" --recursive --json`
### Correct Pattern — NDJSON Consumption
- `sqlite-graphrag ingest ./docs --recursive --json | jaq -c 'select(.status == "indexed")'`
- `sqlite-graphrag ingest ./docs --recursive --json | tee results.ndjson`
- NDJSON contains `files_total + 1` lines: one per file plus a final summary line
- FILTER by `select(.status)` to ignore the summary line that has no `status` field
- `jaq -sc '[.[] | select(.status)] | group_by(.status) | map({status: .[0].status, count: length})' < results.ndjson`
### REQUIRED — NDJSON Schema by Line Type
- Per-file line: `file`, `name`, `status` (`"indexed"` `"skipped"` `"failed"`), `truncated`, `original_name?`, `memory_id?`, `action?`, `error?`
- Final summary line: `summary` (true), `dir`, `pattern`, `recursive`, `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- NER extraction events go to stderr, NOT stdout


## CRUD — Read with read and list
### REQUIRED — Direct Read by Name (read)
- USE `read --name <kebab-case>` for O(1) fetch by name
- PARSE fields `body`, `description`, `created_at_iso`, `updated_at_iso`
- TREAT exit code 4 as memory not found in the namespace
- APPLY `--tz` to localize timestamps in the output
### REQUIRED — Enumeration with Filters (list)
- USE `list --type <kind>` to filter by memory type
- ADJUST `--limit <N>` with default 50 according to expected volume
- PAGINATE via `--offset <N>` for large datasets
- INCLUDE soft-deleted memories via `--include-deleted`
- EXPORT full dump with `--limit 10000 --json` before backup
### Correct Pattern — Read Examples
- `sqlite-graphrag read --name design-auth --json`
- `sqlite-graphrag list --type decision --limit 100 --json`
- `sqlite-graphrag list --include-deleted --json | jaq '.items[] | select(.deleted)'`


## CRUD — Update with edit, rename, and restore
### REQUIRED — Body and Description Editing (edit)
- USE `edit --name <name> --body <text>` for short bodies
- PREFER `--body-file` or `--body-stdin` for long bodies
- CHANGE description via `--description <text>`
- EACH edit creates a new immutable version preserving history
- VALIDATE exit code 3 as an optimistic locking conflict
- JSON response: `memory_id`, `name`, `action` ("updated"), `version`, `elapsed_ms`
### REQUIRED — History-Preserving Rename (rename)
- USE `rename --name <old> --new-name <new>`
- ACCEPT `--old`/`--new` and `--from`/`--to` as aliases since v1.0.35
- PRESERVE all versions and graph connections
- TREAT exit code 4 as missing source memory
- JSON response: `memory_id`, `name` (new), `action` ("renamed"), `version`, `elapsed_ms`
### REQUIRED — Old Version Restore (restore)
- INSPECT versions via `history --name <name>` first
- USE `restore --name <name> --version <N>` for a specific version
- OMIT `--version` to select the last non-restore version automatically
- RESTORE creates a new version without overwriting prior history
- RE-EMBED occurs automatically so vector recall can find it again
### REQUIRED — Optimistic Locking
- PASS `--expected-updated-at <epoch_or_RFC3339>` in concurrent pipelines
- TREAT exit code 3 as detected concurrency
- RELOAD `read --json` to get the new `updated_at` before retrying
- APPLY locking in `edit`, `rename`, and `restore`
### Correct Pattern — Update Flows
- `sqlite-graphrag edit --name design-auth --body-file ./revised.md --expected-updated-at "2026-04-19T12:00:00Z"`
- `sqlite-graphrag rename --from old-name --to new-name`
- `sqlite-graphrag history --name design-auth --json && sqlite-graphrag restore --name design-auth --version 2`


## CRUD — Delete with forget, purge, unlink, and cleanup-orphans
### REQUIRED — Soft Delete (forget)
- USE `forget --name <name>` for reversible soft-delete
- MEMORY disappears from `recall` and `list` by default
- VERSION history remains intact in the database
- REVERSIBLE via `restore` while no purge has occurred
- JSON response: `action` (`"soft_deleted"` `"already_deleted"`), `forgotten`, `name`, `namespace`, `deleted_at?`, `deleted_at_iso?`, `elapsed_ms`
### REQUIRED — Hard Delete (purge)
- USE `purge --retention-days <N> --yes` in automation
- DEFAULT retention is 90 days for soft-deleted memories
- RUN `--dry-run` first to audit the count
- PERMANENTLY deletes rows and reclaims disk space
### REQUIRED — Edge Removal (unlink)
- USE `unlink --from <a> --to <b> --relation <type>`
- ACCEPT `--source`/`--target` as aliases of `--from`/`--to`
- TREAT exit code 4 as nonexistent edge
- ALL three arguments are mandatory without exception
### REQUIRED — Orphan Entity Cleanup (cleanup-orphans)
- RUN `cleanup-orphans --dry-run` to audit
- APPLY `--yes` in automated pipelines
- REMOVES entities with no linked memories or edges
- RUN periodically after bulk `forget` operations
### Correct Pattern — Forget and Restore Round-Trip
- `sqlite-graphrag forget --name decision-x`
- `sqlite-graphrag history --name decision-x --json | jaq '.deleted'`
- `sqlite-graphrag restore --name decision-x`
- `sqlite-graphrag recall "decision" --json`


## Immutable Version History
### REQUIRED — Inspection with history
- USE `history --name <name> --json` to list versions
- VERSIONS start at 1 and increment with each `edit` or `restore`
- CHRONOLOGICAL reverse order by default
- INCLUDES soft-deleted memories with flag `deleted: true`
### REQUIRED — Version Semantics
- EACH `edit` creates a new immutable version preserving prior ones
- EACH `restore` creates a new version with the body of an old version
- COMPLETE audit trail of who changed what and when
- RETENTION POLICY controls when to purge permanently
### Correct Pattern — Change Audit
- `sqlite-graphrag history --name design-auth --json | jaq '.versions[].created_at_iso'`


## GraphRAG Search
### REQUIRED — Four Search Commands
- USE `recall` for KNN vector search with automatic graph expansion
- USE `hybrid-search` for FTS5 and vector fusion via RRF
- USE `related` for multi-hop traversal from a known memory
- USE `graph traverse` for traversal from a typed entity
- COMBINE all four in the canonical three-layer pattern
### REQUIRED — Canonical Three-Layer Pattern
- LAYER 1 — `hybrid-search` to find seed memories by name
- LAYER 2 — `read --name` to expand the full memory body
- LAYER 3 — `related` or `graph traverse` for a multi-hop subgraph
- APPLY layers in order, stopping when context suffices
- INJECT consolidated results into the LLM prompt
### REQUIRED — Layer 1 with hybrid-search
- USE `hybrid-search <query> --k 10 --rrf-k 60 --json`
- COMBINES FTS5 textual and KNN vector via Reciprocal Rank Fusion
- ADJUST `--weight-vec` and `--weight-fts` only with numerical evidence
- DEFAULT for both weights is `1.0` with balanced fusion
- EXTRACT only `name` via `jaq -r '.results[].name'` for the next stage
### REQUIRED — hybrid-search with Graph Expansion
- ENABLE graph traversal via `--with-graph` to discover connected memories
- ADJUST depth with `--max-hops <N>` (default 2)
- FILTER weak edges with `--min-weight <F>` (default 0.0)
- GRAPH results are in `graph_matches[]`, SEPARATE from `results[]`
- `graph_matches[]` uses RecallItem schema: `name`, `distance`, `source` ("graph"), `graph_depth`
- READ BOTH `results[]` and `graph_matches[]` when `--with-graph` is active
- EXTRACT via `jaq -r '(.results[] , .graph_matches[]) | .name'`
### REQUIRED — Alternative Layer 1 with recall
- USE `recall <query> --k 5 --json` for pure semantic queries
- ACCEPT `--limit` as an alias of `--k` since v1.0.35
- RECALL expands automatically via graph by default
- DISABLE automatic graph expansion via `--no-graph`
- INTERPRET `distance` increasing as similarity decreasing
- INTERPRET `score` as `1.0 - distance`, clamped to `[0.0, 1.0]`
- FIELD `source` indicates origin: `"direct"` (KNN) or `"graph"` (traversal)
- FIELD `graph_depth` present only in results with `source: "graph"`
- RecallResponse separates `direct_matches[]`, `graph_matches[]`, and `results[]` (aggregate)
- USE when the query does not mix exact tokens with natural language
### REQUIRED — Layer 2 with read --name
- USE `read --name <name>` to get the full body of the seed memory
- EXPAND context beyond the snippet returned by layer 1
- LOOP over the top-k names to build a context bundle
- PARSE fields `body`, `description`, `created_at_iso`
### REQUIRED — Layer 3 with related
- USE `related <name> --hops <N>` for multi-hop traversal
- TWO hops reveal transitive knowledge invisible to vector search
- HOP distance delivers an explicit signal to the orchestrator
- USE when the query requires chained multi-step reasoning
### REQUIRED — Alternative Layer 3 with graph traverse
- USE `graph traverse --from <root> --depth <N>` for a focused subgraph
- DEFAULT depth is 2 when omitted
- TREAT exit code 4 as nonexistent root entity
- HOPS return `entity`, `relation`, `direction`, `weight`, `depth`
- START from a typed entity, not a memory name
### REQUIRED — Score and Distance Semantics
- `recall` returns `distance` (lower is more similar) and `score` (1.0 - distance)
- `recall` returns `source` (`"direct"` or `"graph"`) and `graph_depth` (when graph)
- `hybrid-search` returns `combined_score`; higher is better ranking
- `hybrid-search` exposes `vec_rank` and `fts_rank` to audit fusion
- `hybrid-search` with `--with-graph` adds `graph_matches[]` in a separate field
- `related` returns `hop_distance`, explicit depth in the graph
- `graph traverse` returns `depth` per visited hop
- DISCARD weak hits before spending tokens in the prompt
### REQUIRED — Command Choice by Query Type
- BROAD conceptual query, `recall` with `--k 5`
- MIXED token and natural-language query, `hybrid-search` with `--rrf-k 60`
- MIXED query with graph context, `hybrid-search --with-graph --max-hops 2`
- EXPLORATORY query starting from memory, `related --hops 2`
- EXPLORATORY query starting from entity, `graph traverse --depth 2`
- GRAPH audit query, `graph entities` or `graph stats`
### FORBIDDEN — Search Anti-patterns
- NEVER use native SQLite text search in parallel with the binary
- NEVER confuse `distance` with `combined_score` in ranking
- NEVER increase `--hops` without inspecting `graph stats` first
- NEVER inject results without filtering by relevance threshold
- NEVER parallelize heavy searches without measuring host RSS
- NEVER skip layer 2 when the snippet is insufficient
- NEVER read only `.results[]` when `--with-graph` is active (you will miss `graph_matches[]`)
### Correct Pattern — Canonical Three-Layer Pipeline
- `sqlite-graphrag hybrid-search "auth jwt design" --k 10 --json | jaq -r '.results[].name' > seeds.txt`
- `while read -r name; do sqlite-graphrag read --name "$name" --json; done < seeds.txt > bodies.ndjson`
- `sqlite-graphrag related "$(head -n1 seeds.txt)" --hops 2 --json > graph.json`
- `paste -d '\n' bodies.ndjson <(cat graph.json) | claude --print`
### Correct Pattern — Pipeline with Graph Expansion
- `sqlite-graphrag hybrid-search "auth" --k 5 --with-graph --json | jaq -r '(.results[], .graph_matches[]) | .name' | sort -u > seeds.txt`
### Correct Pattern — Fine-Tuning hybrid-search Weights
- `--weight-vec 1.0 --weight-fts 1.0` equal weight, recommended default
- `--weight-vec 1.0 --weight-fts 0.0` reproduces pure recall baseline
- `--weight-vec 0.0 --weight-fts 1.0` reproduces pure FTS5
- `--weight-vec 0.7 --weight-fts 0.3` favors semantics over tokens
- `--weight-vec 0.3 --weight-fts 0.7` favors tokens over semantics
### Measured Gains of the Three-Layer Pattern
- REDUCTION of context tokens by up to 72x vs markdown dump
- INCREASE of accuracy by up to 18% over pure vector retrieval
- INCREASE of multi-hop accuracy from 30% to 50% according to Microsoft
- APPROXIMATE latency of 1 second on modern hardware with daemon


## Graph — Construction and Inspection
### REQUIRED — Edge Creation (link)
- USE `link --from <a> --to <b> --relation <type>`
- ENTITIES must exist as typed nodes before linking, except with `--create-missing`
- USE `--create-missing` to auto-create nonexistent entities during link
- USE `--entity-type <type>` to set the type of auto-created entities (default `concept`)
- JSON response includes `created_entities: ["a", "b"]` when entities were created
- ACCEPT `--source`/`--target` as aliases of `--from`/`--to`
- SET `--weight` optional for relation weight (default 0.5)
- TREAT exit code 4 as nonexistent entity (without `--create-missing`)
### REQUIRED — Export with graph
- EXPORT snapshot via `graph --format json`
- USE `--format dot` for offline Graphviz
- USE `--format mermaid` to embed in Markdown
- WRITE directly to a file via `--output <PATH>`
- INSPECT `nodes` and `edges` in the exported JSON
### REQUIRED — Entity Enumeration (graph entities)
- USE `graph entities --json` to list all entities
- ACCESS via `jaq -r '.entities[].name'` (field is `entities`, NOT `items`)
- FILTER by `--entity-type <type>` when needed
- PAGINATE with `--limit` and `--offset`
- USE before planning traversals or batch links
### REQUIRED — Statistics (graph stats)
- USE `graph stats --json` before expensive traversals
- INSPECT `node_count`, `edge_count`, `avg_degree`, `max_degree`
- CHOOSE traversal depth based on actual density
- DETECT subgraph isolation before planning searches
### Canonical Relation Vocabulary
- `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`
- `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- Custom relation types (e.g., `implements`, `tested-by`, `blocks`) are accepted since v1.0.49; non-canonical values emit a `tracing::warn!`
### Valid Entity Types
- `project`, `tool`, `person`, `file`, `concept`, `incident`
- `decision`, `memory`, `dashboard`, `issue_tracker`
- `organization`, `location`, `date`


## Daemon and Reduced Latency
### REQUIRED — Embedding Model Reuse
- START `sqlite-graphrag daemon` in long agent sessions
- CHECK health via `daemon --ping --json`
- STOP via `daemon --stop` at session end
- LET `init`, `remember`, `ingest`, `recall`, `hybrid-search` reuse automatically
- TREAT daemon as optional for single-shot invocations
- INSPECT the embedding request counter in `--ping`
- `daemon --ping` emits a warning when the running daemon version differs from the CLI binary version; restart the daemon after upgrades with `daemon --stop` followed by `daemon`


## Cache — Model Management
### REQUIRED — Cache Maintenance
- LIST cached models via `cache list --json`
- REMOVE model cache via `cache clear-models --json`
- `clear-models` forces re-download on the next embedding operation
- USE `cache list` to diagnose disk usage by ONNX models


## JSON Contract and Pipelines
### REQUIRED — Deterministic Output
- USE `--json` in all subcommands before piping
- PREFER `--json` over `--format json` in one-liners
- FILTER fields via `jaq` instead of regex on stdout
- READ only fields actually returned by the subcommand
- TREAT JSON as a SemVer-versioned API
### REQUIRED — --json vs --format json Matrix
- `--json` is accepted by ALL subcommands
- `--format json` accepted only in a subset with `--format`
- WHEN both are present, `--json` wins in conflict
- USE `--json` by default in portable pipelines
### REQUIRED — JSON vs NDJSON Distinction
- INDIVIDUAL commands emit a single JSON envelope on stdout
- `ingest` emits NDJSON, one JSON line per file plus summary on stdout
- CONSUME NDJSON via `jaq -c` or `while read -r line`
- AGGREGATE NDJSON into an array via `jaq -s` when needed
### REQUIRED — Critical Fields by Command
- `recall` returns `results[].name`, `snippet`, `distance`, `score`, `source` (`"direct"`/`"graph"`), `graph_depth?`
- `recall` response-level: `query`, `k`, `direct_matches[]`, `graph_matches[]`, `results[]`, `elapsed_ms`
- `hybrid-search` returns `results[].name`, `combined_score`, `score`, `vec_rank`, `fts_rank`, `source`, `body`
- `hybrid-search` response-level: `query`, `k`, `rrf_k`, `weights`, `results[]`, `graph_matches[]`, `elapsed_ms`
- `hybrid-search` `graph_matches[]` uses RecallItem: `name`, `distance`, `source` ("graph"), `graph_depth`
- `related` returns `results[].name`, `hop_distance`, `relation`, `source_entity`, `target_entity`, `weight`
- `graph traverse` returns `hops[].entity`, `relation`, `direction`, `weight`, `depth`
- `read` returns `name`, `body`, `description`, `created_at_iso`, `updated_at_iso`
- `edit` returns `memory_id`, `name`, `action` ("updated"), `version`, `elapsed_ms`
- `rename` returns `memory_id`, `name` (new), `action` ("renamed"), `version`, `elapsed_ms`
- `forget` returns `action` (`"soft_deleted"`/`"already_deleted"`), `forgotten`, `name`, `namespace`, `elapsed_ms`
- `health` returns `integrity_ok`, `schema_ok`, `vec_memories_ok`, `vec_entities_ok`, `vec_chunks_ok`, `fts_ok`, `model_ok`, `counts`, `wal_size_mb`, `journal_mode`, `db_path`, `db_size_bytes`, `checks[]`
- `health.counts` contains: `memories`, `entities`, `relationships`, `vec_memories`
- `stats` returns GLOBAL data (no namespace filter): `memories`, `entities`, `relationships`, `chunks_total`, `avg_body_len`, `namespaces[]`, `db_size_bytes`, `schema_version`, `elapsed_ms`
- `ingest` per file: `file`, `name`, `status` (`"indexed"`/`"skipped"`/`"failed"`), `truncated`, `original_name?`, `memory_id?`, `action?`, `error?`
- `ingest` summary: `summary` (true), `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- `cache list` returns models with size in bytes and total disk usage


## Exit Codes and Retry Strategy
### REQUIRED — Complete Exit Code Handling
- `0` equals success; parse stdout
- `1` equals validation (invalid weight, self-link, bad timezone, max-files exceeded)
- `2` equals duplicate (memory already exists without `--force-merge`)
- `3` equals optimistic locking conflict; reload and retry
- `4` equals entity, memory, or version not found
- `5` equals namespace error (invalid name or conflict)
- `6` equals payload above the size limit
- `10` equals database error; run `vacuum` and `health`
- `11` equals embedding failure (corrupted model or missing ORT)
- `12` equals failure loading `sqlite-vec`; check SQLite ≥ 3.40
- `13` equals partial batch failure; reprocess only failed
- `14` equals I/O error (inaccessible file, permission, disk full)
- `15` equals database busy; widen `--wait-lock`
- `20` equals internal error or JSON serialization failure
- `75` equals exhausted slots in ingest or other heavy command
- `77` equals RAM pressure; wait for free memory
### FORBIDDEN — Error Anti-patterns
- NEVER ignore a non-zero exit code as success
- NEVER reprocess the entire batch after exit 13
- NEVER increase concurrency after receiving 75 or 77
- NEVER attempt `restore` without inspecting `history` first
- NEVER assume ambiguity without reading stderr first
- NEVER confuse exit 1 (validation) with exit 2 (duplicate)


## Concurrency and Resources
### REQUIRED — Load Control
- START heavy commands with `--max-concurrency 1`
- INCREASE only after measuring host RSS and swap
- RESPECT the hard ceiling of `2×nCPUs` for heavy commands
- TREAT `init`, `remember`, `ingest`, `recall`, `hybrid-search` as heavy
- WIDEN `--wait-lock <ms>` when contention is expected
- LIMIT parallel ingestion in CI without an active daemon
### REQUIRED — Two Parallelism Axes in ingest
- `--max-concurrency` governs simultaneous CLI invocations
- `--ingest-parallelism` governs extract plus embed in parallel
- ADJUST both independently according to RAM and CPU
- USE `--low-memory` to force unitary parallelism
- HONOR `SQLITE_GRAPHRAG_LOW_MEMORY=1` on constrained hosts


## Maintenance and Backup
### REQUIRED — Periodic Hygiene
- SCHEDULE `purge --retention-days 30 --yes` weekly
- RUN `vacuum` after large purges
- RUN `optimize` to refresh planner statistics
- CLEAN orphans via `cleanup-orphans --yes` after bulk forget
### REQUIRED — Safe Backup
- USE `sync-safe-copy --dest <path>` before syncing Dropbox or iCloud
- COMPRESS snapshots via `ouch compress` for remote upload
- EXPORT memories via `list --limit 10000 --json` to NDJSON
- VERSION the database with Git LFS when feasible
### REQUIRED — Schema Diagnostics
- USE `__debug_schema --json` for troubleshooting
- INSPECT `schema_version`, `objects`, `migrations`
- COMMAND is hidden from `--help`; invoke by exact name
### Correct Pattern — Weekly Cron
- `sqlite-graphrag purge --retention-days 30 --yes`
- `sqlite-graphrag cleanup-orphans --yes`
- `sqlite-graphrag vacuum --json`
- `sqlite-graphrag optimize --json`
- `sqlite-graphrag sync-safe-copy --dest ~/Dropbox/graphrag.sqlite`


## Contract: Stdin and Stdout
### Input — Structured Arguments Only
- CLI flags accept typed arguments validated by `clap` with strict parsing
- Stdin accepts a raw body when `--body-stdin` is active on `remember` or `edit`
- Stdin accepts a graph JSON object with optional `body`, `entities`, and `relationships` when `--graph-stdin` is active on `remember`; invalid JSON fails instead of becoming memory body
- Body sources such as `--body`, `--body-file`, `--body-stdin`, and `--graph-stdin` are rejected when combined ambiguously
- `remember` accepts body payloads up to `512000` bytes and up to `512` chunks; larger payloads return exit code `6`
- Environment variables override defaults without mutating the file database
- The default database path is always `./graphrag.sqlite` in the invocation directory
- Language is controlled by `--lang en` or `--lang pt` for deterministic output


### Output — Deterministic JSON Documents
- Every subcommand emits exactly one JSON document when `--json` is set
- Keys are stable across releases inside the current major version line
- Timestamps follow RFC 3339 with UTC offset notation always present
- Optional fields may be omitted or serialized as `null`; agents must handle both forms
- Arrays preserve deterministic order sorted by `score` or `updated_at` descending


## Exit Codes Table
### Contract — Map Every Status To A Routing Decision
| Code | Meaning | Recommended Action |
| --- | --- | --- |
| `0` | Success | Continue the agent loop |
| `1` | Validation or runtime failure | Log and surface to operator |
| `2` | CLI usage error or duplicate | Fix arguments then retry |
| `3` | Optimistic update conflict | Re-read `updated_at` and retry |
| `4` | Memory or entity not found | Handle missing resource gracefully |
| `5` | Namespace limit or unresolved | Pass `--namespace` explicitly |
| `6` | Payload exceeded allowed limits | Split body into smaller chunks |
| `10` | SQLite database error | Run `health` to inspect integrity |
| `11` | Embedding generation failed | Check model files and retry |
| `12` | `sqlite-vec` extension failed | Reinstall binary with bundled extension |
| `13` | Batch operation partially failed | Inspect partial results and retry failed items |
| `14` | I/O error (file, permission, disk full) | Check file access and available disk space |
| `15` | Database busy after retries | Wait and retry the operation |
| `20` | Internal error or serialization failure | Report bug with full stderr output |
| `75` | Advisory lock held or all slots full | Wait and retry, or lower pressure on heavy commands instead of raising concurrency blindly |
| `77` | Low memory threshold tripped | Free RAM before retry |


## JSON Output Format
### Recall — Vector-Only KNN
```json
{
  "query": "graphrag retrieval",
  "k": 3,
  "direct_matches": [
    { "memory_id": 1, "name": "graphrag-intro", "namespace": "global", "type": "user", "description": "intro doc", "snippet": "GraphRAG combines...", "distance": 0.09, "source": "vec" }
  ],
  "graph_matches": [],
  "results": [
    { "memory_id": 1, "name": "graphrag-intro", "namespace": "global", "type": "user", "description": "intro doc", "snippet": "GraphRAG combines...", "distance": 0.09, "source": "vec" }
  ],
  "elapsed_ms": 12
}
```


### Hybrid Search — FTS5 Plus Vector RRF
```json
{
  "query": "postgres migration",
  "k": 5,
  "rrf_k": 60,
  "weights": { "vec": 1.0, "fts": 1.0 },
  "results": [
    { "memory_id": 1, "name": "postgres-migration-plan", "namespace": "global", "type": "project", "description": "migration plan", "body": "Step 1...", "combined_score": 0.96, "score": 0.96, "source": "hybrid", "vec_rank": 1, "fts_rank": 1 },
    { "memory_id": 2, "name": "db-migration-checklist", "namespace": "global", "type": "reference", "description": "checklist", "body": "Check indexes...", "combined_score": 0.88, "score": 0.88, "source": "hybrid", "vec_rank": 2, "fts_rank": 3 }
  ],
  "graph_matches": [],
  "elapsed_ms": 18
}
```


## Idempotency and Side Effects
### Read-Only Commands — Zero Mutations Guaranteed
- `recall` reads the vector and metadata tables without touching disk state
- `read` fetches a single row by name and emits JSON without side effects
- `list` paginates memories sorted deterministically with stable cursors
- `health` runs SQLite `PRAGMA integrity_check` and reports without writing
- `stats` counts rows in read-only transactions safe for concurrent agents


### Write Commands — Optimistic Locking Protects Concurrency
- `remember` uses `ON CONFLICT(name)` so duplicate calls return exit code `2`
- `rename` requires `--expected-updated-at` to detect stale writes via exit `3`
- `edit` creates a new row in `memory_versions` preserving immutable history
- `restore` rewinds content while appending a new version instead of overwriting
- `forget` is soft-delete so re-running it is safe and idempotent by design


## Payload Limits
### Ceilings — Enforced By The Binary
- `EMBEDDING_MAX_TOKENS` equals 512 tokens measured by the model tokenizer
- `TEXT_BODY_PREVIEW_LEN` equals 200 characters in list and recall snippets
- `MAX_CONCURRENT_CLI_INSTANCES` equals the hard ceiling of 4 across cooperating subprocess agents, but heavy commands may clamp lower dynamically from available RAM
- `CLI_LOCK_DEFAULT_WAIT_SECS` equals 300 seconds before exit code `75`
- `PURGE_RETENTION_DAYS_DEFAULT` equals 90 days before hard delete becomes allowed


## Language Control
### Bilingual Output — One Flag Switches Locale
- Flag `--lang en` forces English messages regardless of system locale
- Flag `--lang pt` or `--lang pt-BR` or `--lang portuguese` or `--lang PT` forces Portuguese
- Short codes `en` and `pt` are the canonical forms; the longer aliases are accepted without error
- Env `SQLITE_GRAPHRAG_LANG=pt` overrides system locale when `--lang` is absent
- Missing flag and env falls back to `sys_locale::get_locale()` detection
- Unknown locales default to English without emitting any warning to stderr
- Env `SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo` sets the IANA timezone applied to all `*_iso` fields in JSON output
- Flag `--tz <IANA>` takes priority over `SQLITE_GRAPHRAG_DISPLAY_TZ`; both fall back to UTC when absent
- Invalid IANA names cause exit 2 with a `Validation` error message before any command runs
- Only `*_iso` string fields are affected; integer epoch fields (`created_at`, `updated_at`) remain unchanged
- Env `SQLITE_GRAPHRAG_LOG_FORMAT=json` switches tracing output to newline-delimited JSON; default is `pretty`


## ARM64 GNU Runtime Contract
### Dynamic ONNX Runtime Loading — What Agents MUST Provide
- On `aarch64-unknown-linux-gnu`, embedding commands do NOT rely on link-time ONNX Runtime linkage
- Agents MUST make `libonnxruntime.so` reachable through `ORT_DYLIB_PATH`, the executable directory, `./lib/`, or the model cache directory
- Heavy commands affected are `init`, `remember`, `recall`, and `hybrid-search`
- If the shared library is absent, the first embedding operation fails at runtime even though the binary itself starts correctly


## JSON Output Flag
### Format — `--json` Is Universal and `--format json` Is Command-Specific
- Every subcommand accepts `--json` for deterministic JSON stdout
- Only commands that expose `--format` in their help accept `--format json`
- `--json` is the short form — preferred in one-liners and agent pipelines
- If `--json` appears with a non-JSON `--format`, `--json` wins and stdout remains JSON
- `--format json` is the explicit form — command-specific, preferred where alternate output modes also exist


## Graph Input Payloads
### Contract — `remember` Graph Files
- `--entities-file` accepts a JSON array of entity objects
- Each entity object MUST include `name` and `entity_type`
- The alias field `type` is accepted as a synonym for `entity_type`
- Agents MUST NOT send both `entity_type` and `type` in the same entity object
- Valid `entity_type` values are `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, and `date`
- `--relationships-file` accepts a JSON array of relationship objects
- Each relationship object MUST include `source`/`from`, `target`/`to`, `relation`, and `strength`
- `strength` MUST be a floating-point number in the inclusive range `[0.0, 1.0]`
- Stored graph outputs expose this value as `weight`
- File payloads MAY use canonical stored relation names with underscores such as `applies_to`, `depends_on`, and `tracked_in`; dashed aliases are normalized before storage
- CLI flags for `link` and `unlink` use dashed labels such as `applies-to`, `depends-on`, and `tracked-in`
- `--graph-stdin` accepts a single object with optional `body` plus the same `entities` and `relationships` arrays
- `link --create-missing` auto-creates entities that do not exist during linking, defaulting to type `concept`; use `--entity-type` to override (added in v1.0.44)
- `hybrid-search --with-graph` enables graph traversal seeded from top RRF results; graph matches appear in the `graph_matches` array alongside the `results` array (fixed in v1.0.44 — was previously a no-op)
- `graph entities` JSON response uses top-level key `entities` (renamed from `items` in v1.0.44); update existing `jaq` scripts from `.items[]` to `.entities[]`


## Machine-Readable Schemas
### JSON Schema Draft 2020-12 Files For Every Subcommand
- Directory `docs/schemas/` ships one `.schema.json` file per subcommand
- Every schema declares `"additionalProperties": false` — unknown keys are contract violations
- Schemas use `$defs` for shared subtypes (e.g. `RecallItem`, `HealthCheck`)
- Optional fields are absent from the `required` array and typed with `["T", "null"]` where nullable
- Validate a live response with a real JSON Schema validator: `jsonschema --instance <(sqlite-graphrag stats) docs/schemas/stats.schema.json`
- File `docs/schemas/debug-schema.schema.json` covers the hidden `__debug_schema` diagnostic subcommand
- Schemas are updated on every breaking change and follow the CLI SemVer major version


## Superpowers Summary
### Five Reasons Your Orchestrator Will Stay
- Deterministic output eliminates fragile regex parsing in your agent glue code
- Exit codes route decisions without scraping stderr for human-readable messages
- Single binary deploys identically in Docker, GitHub Actions and developer laptops
- SQLite durability survives kernel panics and container kills without corruption
- Graph-native retrieval surfaces multi-hop context that flat vector search misses


## Get Started In 30 Seconds
### Install — One Command Installs The Full Stack
```bash
cargo install --path . && sqlite-graphrag init
```
- Flag `--locked` reuses the shipped `Cargo.lock` to protect MSRV from transitive drift
- Command `init` creates `graphrag.sqlite` in the current working directory and downloads the embedding model locally
- First invocation may take one minute while `fastembed` fetches `multilingual-e5-small`
- Subsequent invocations skip the first model download, but heavy commands still depend on model residency and daemon state
- Uninstall with `cargo uninstall sqlite-graphrag` leaving the database file in place
