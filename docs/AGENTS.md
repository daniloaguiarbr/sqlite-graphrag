# sqlite-graphrag for AI Agents


> Persistent memory for 27 AI agents in a single 25 MB Rust binary

- Read the Portuguese version at [AGENTS.pt-BR.md](AGENTS.pt-BR.md)


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
| OpenClaw | community | any recent | Subprocess | `sqlite-graphrag list --type user --json` |
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
| `15` | Database busy after retries | Wait and retry the operation |
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
