# neurographrag

Your AI agents forget everything. Give any LLM agent a memory that survives restarts, cloud outages, and API bills. No cloud. No Python. No embeddings API. Still GraphRAG. This 25 MB binary gives them a brain.

[![Crates.io](https://img.shields.io/crates/v/neurographrag.svg)](https://crates.io/crates/neurographrag)
[![docs.rs](https://img.shields.io/docsrs/neurographrag)](https://docs.rs/neurographrag)
[![CI](https://img.shields.io/github/actions/workflow/status/daniloaguiarbr/neurographrag/ci.yml?branch=main)](https://github.com/daniloaguiarbr/neurographrag/actions)
[![License](https://img.shields.io/crates/l/neurographrag.svg)](LICENSE)
[![Downloads](https://img.shields.io/crates/d/neurographrag.svg)](https://crates.io/crates/neurographrag)
[![MSRV](https://img.shields.io/crates/msrv/neurographrag)](https://crates.io/crates/neurographrag)
[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg)](CODE_OF_CONDUCT.md)

> Your AI agents forget everything. Give any LLM agent a memory that survives restarts, cloud outages, and API bills. No cloud. No Python. No embeddings API. Still GraphRAG. This 25 MB binary gives them a brain.

- Portuguese version available at [README.pt-BR.md](README.pt-BR.md)

```bash
cargo install --locked neurographrag
```


## What is it?
### neurographrag delivers durable memory for AI agents
- Stores memories, entities and relationships inside a single SQLite file under 25 MB
- Embeds content locally via `fastembed` with the `multilingual-e5-small` model
- Combines FTS5 full-text search with `sqlite-vec` KNN into a hybrid Reciprocal Rank Fusion ranker
- Extracts an entity graph with typed edges for multi-hop recall across memories
- Preserves every edit through an immutable version history table for full audit
- Runs on Linux, macOS and Windows natively with zero external services required


## Why neurographrag?
### Differentiators against cloud RAG stacks
- Offline-first architecture eliminates OpenAI embeddings and Pinecone recurring fees
- Single-file SQLite storage replaces Docker clusters of vector databases entirely
- Graph-native retrieval beats pure vector RAG on multi-hop questions by design
- Deterministic JSON output unlocks clean orchestration by LLM agents in pipelines
- Native cross-platform binary ships without Python, Node or Docker dependencies


## Superpowers for AI Agents
### First-class CLI contract for orchestration
- Every subcommand accepts `--json` producing deterministic stdout payloads
- Every invocation is stateless with explicit exit codes for routing decisions
- Note: CLI is stateless — each invocation reloads the embedding model (~1s); daemon mode targeting <50ms latency is planned for v3.0.0
- Every write is idempotent through `--name` kebab-case uniqueness constraints
- Stdin accepts bodies or JSON payloads for entities and relationship batches
- Stderr carries tracing output under `NEUROGRAPHRAG_LOG_LEVEL=debug` only
- Cross-platform behavior is identical across Linux, macOS and Windows hosts
### 27 AI agents and IDEs supported out of the box
| Agent | Vendor | Minimum version | Integration pattern |
| --- | --- | --- | --- |
| Claude Code | Anthropic | 1.0 | Subprocess with `--json` stdout |
| Codex | OpenAI | 1.0 | Tool call wrapping `cargo run -- recall` |
| Gemini CLI | Google | 1.0 | Function call returning JSON |
| Opencode | Opencode | 1.0 | Shell tool with `hybrid-search --json` |
| OpenClaw | Community | 0.1 | Subprocess pipe into `jaq` filters |
| Paperclip | Community | 0.1 | Direct CLI invocation per message |
| VS Code Copilot | Microsoft | 1.85 | Terminal subprocess via tasks |
| Google Antigravity | Google | 1.0 | Agent tool with structured JSON |
| Windsurf | Codeium | 1.0 | Custom command registration |
| Cursor | Anysphere | 0.42 | Terminal integration or MCP wrapper |
| Zed | Zed Industries | 0.160 | Extension wrapping subprocess |
| Aider | Paul Gauthier | 0.60 | Shell command hook per turn |
| Jules | Google Labs | 1.0 | Workspace shell integration |
| Kilo Code | Community | 1.0 | Subprocess invocation |
| Roo Code | Community | 1.0 | Custom command via CLI |
| Cline | Saoud Rizwan | 3.0 | Terminal tool registered manually |
| Continue | Continue Dev | 0.9 | Context provider via shell |
| Factory | Factory AI | 1.0 | Tool call with JSON response |
| Augment Code | Augment | 1.0 | Terminal command wrapping |
| JetBrains AI Assistant | JetBrains | 2024.3 | External tool per IDE |
| OpenRouter | OpenRouter | 1.0 | Function routing through shell |
| Minimax | Minimax | 1.0 | Subprocess invocation |
| Z.ai | Z.ai | 1.0 | Subprocess invocation |
| Ollama | Ollama | 0.1 | Subprocess invocation |
| Hermes Agent | Community | 1.0 | Subprocess invocation |
| LangChain | LangChain | 0.3 | Subprocess via tool |
| LangGraph | LangChain | 0.2 | Subprocess via node |


## Quick Start
### Install and record your first memory in four commands
```bash
cargo install --locked neurographrag
neurographrag init
neurographrag remember --name onboarding-note --type user --description "first memory" --body "hello graphrag"
neurographrag recall "graphrag" --k 5 --json
```
- The flag `--locked` reuses the `Cargo.lock` shipped with the crate to prevent MSRV breakage
- Without `--locked` Cargo may resolve a patch release that requires a newer `rustc` than 1.88


## Installation
### Multiple distribution channels
- Install from crates.io with `cargo install --locked neurographrag`
- Build from source with `git clone` followed by `cargo build --release`


## Usage
### Initialize the database
```bash
neurographrag init
neurographrag init --namespace project-foo
```
### Remember a memory with an entity graph
```bash
neurographrag remember \
  --name integration-tests-postgres \
  --type feedback \
  --description "prefer real Postgres over SQLite mocks" \
  --body "Integration tests must hit a real database."
```
### Recall memories by semantic similarity
```bash
neurographrag recall "postgres integration tests" --k 3 --json
```
### Hybrid search combining FTS5 and vector KNN
```bash
neurographrag hybrid-search "postgres migration rollback" --k 10 --json
```
### Inspect database health and stats
```bash
neurographrag health --json
neurographrag stats --json
```
### Purge soft-deleted memories after retention period
```bash
neurographrag purge --retention-days 90 --dry-run --json
neurographrag purge --retention-days 90 --yes
```


## Commands
### Core database lifecycle
| Command | Arguments | Description |
| --- | --- | --- |
| `init` | `--namespace <ns>` | Initialize database and download embedding model |
| `health` | `--json` | Show database integrity and pragma status |
| `stats` | `--json` | Count memories, entities and relationships |
| `migrate` | `--json` | Apply pending schema migrations via `refinery` |
| `vacuum` | `--json` | Checkpoint WAL and reclaim disk space |
| `optimize` | `--json` | Run `PRAGMA optimize` to refresh statistics |
| `sync-safe-copy` | `--dest <path>` (alias `--output`) | Checkpoint then copy a sync-safe snapshot |
### Memory content lifecycle
| Command | Arguments | Description |
| --- | --- | --- |
| `remember` | `--name`, `--type`, `--description`, `--body` | Save a memory with optional entity graph |
| `recall` | `<query>`, `--k`, `--type` | Search memories semantically via KNN |
| `read` | `--name <name>` | Fetch a memory by exact kebab-case name |
| `list` | `--type`, `--limit`, `--offset` | Paginate memories sorted by `updated_at` |
| `forget` | `--name <name>` | Soft-delete a memory preserving history |
| `rename` | `--old <name>`, `--new <name>` | Rename a memory while keeping versions |
| `edit` | `--name`, `--body`, `--description` | Edit body or description creating new version |
| `history` | `--name <name>` | List all versions of a memory |
| `restore` | `--name`, `--version` | Restore a memory to a previous version |
### Retrieval and graph
| Command | Arguments | Description |
| --- | --- | --- |
| `hybrid-search` | `<query>`, `--k`, `--rrf-k` | FTS5 plus vector fused via Reciprocal Rank Fusion |
| `namespace-detect` | `--cwd <path>` | Resolve namespace precedence for invocation |
### Maintenance
| Command | Arguments | Description |
| --- | --- | --- |
| `purge` | `--retention-days <n>`, `--dry-run`, `--yes` | Permanently delete soft-deleted memories |


## Environment Variables
### Runtime configuration overrides
| Variable | Description | Default | Example |
| --- | --- | --- | --- |
| `NEUROGRAPHRAG_DB_PATH` | Absolute path to the SQLite database file | XDG data dir | `/data/graph.sqlite` |
| `NEUROGRAPHRAG_CACHE_DIR` | Directory for embedding model cache | XDG cache dir | `~/.cache/neurographrag` |
| `NEUROGRAPHRAG_LANG` | CLI output language as `en` or `pt` | `en` | `pt` |
| `NEUROGRAPHRAG_LOG_LEVEL` | Tracing filter level for stderr output | `info` | `debug` |
| `NEUROGRAPHRAG_NAMESPACE` | Namespace override bypassing detection | none | `project-foo` |


## Integration Patterns
### Compose with Unix pipelines and tools
```bash
neurographrag recall "auth tests" --k 5 --json | jaq -r '.results[].name'
```
### Feed hybrid search into a summarizer endpoint
```bash
neurographrag hybrid-search "postgres migration" --k 10 --json \
  | jaq -c '.results[] | {name, combined_score}' \
  | xh POST http://localhost:8080/summarize
```
### Backup with atomic snapshot and compression
```bash
neurographrag sync-safe-copy --dest /tmp/ng.sqlite
ouch compress /tmp/ng.sqlite /tmp/ng-$(date +%Y%m%d).tar.zst
```
### Claude Code subprocess example in Node
```javascript
const { spawn } = require('child_process');
const proc = spawn('neurographrag', ['recall', query, '--k', '5', '--json']);
```
### Docker Alpine build for CI pipelines
```dockerfile
FROM rust:1.88-alpine AS builder
RUN apk add musl-dev sqlite-dev
RUN cargo install --locked neurographrag
```


## Exit Codes
### Deterministic status codes for orchestration
| Code | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Validation error or runtime failure |
| `2` | Duplicate detected or invalid CLI argument |
| `3` | Conflict during optimistic update |
| `4` | Memory or entity not found |
| `5` | Namespace could not be resolved |
| `6` | Payload exceeded configured limits |
| `10` | SQLite database error |
| `11` | Embedding generation failed |
| `12` | `sqlite-vec` extension failed to load |
| `13` | Batch partial failure (import, reindex, stdin batch) |
| `14` | Filesystem I/O error |
| `15` | Database busy after retries (moved from 13 in v2.0) |
| `20` | Internal or JSON serialization error |
| `73` | Memory guard rejected low RAM condition |
| `75` | `EX_TEMPFAIL`: all concurrency slots busy |
| `77` | Available RAM below minimum required to load the embedding model |


## Performance
### Measured on a 1000-memory database
- Cold startup under 50 milliseconds on native ARM64 Apple Silicon
- Recall with `--k 5` completes under 20 milliseconds after model load
- Hybrid search with RRF completes under 30 milliseconds on warm cache
- First `init` downloads the quantized model once and caches it locally
- Embedding model uses approximately 750 MB of RAM per process instance


## Safe Parallel Invocation
### Counting semaphore with four simultaneous slots
- Each invocation loads `multilingual-e5-small` consuming roughly 750 MB of RAM
- Up to four instances run in parallel via `MAX_CONCURRENT_CLI_INSTANCES` default
- Lock files live at `~/.cache/neurographrag/cli-slot-{1..4}.lock` using `flock`
- A fifth concurrent invocation waits up to 300 seconds then exits with code 75
- Use `--max-concurrency N` to override the slot limit for the current invocation
- Memory guard aborts with exit 77 when less than 2 GB of RAM is available
- SIGINT and SIGTERM trigger graceful shutdown via `shutdown_requested()` atomic


## Troubleshooting FAQ
### Common issues and fixes
- Database locked after crash requires `neurographrag vacuum` to checkpoint the WAL
- First `init` takes roughly one minute while `fastembed` downloads the quantized model
- Permission denied on Linux means the cache directory lacks write access for your user
- Namespace detection falls back to `default` when no `.neurographrag` marker exists
- Parallel invocations beyond four slots receive exit 75 and SHOULD retry with backoff


## Compatible Rust Crates
### Invoke neurographrag from any Rust AI framework via subprocess
- Each crate calls the binary through `std::process::Command` with `--json` flag
- No shared memory or FFI required: the contract is pure stdout JSON
- Pin the binary version in your `Cargo.toml` workspace for reproducible builds
- All 18 crates below work identically on Linux, macOS and Windows

### rig-core
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["recall", "project goals", "--k", "5", "--json"])
    .output().unwrap();
```

### swarms-rs
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["hybrid-search", "agent memory", "--k", "10", "--json"])
    .output().unwrap();
```

### autoagents
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["remember", "--name", "task-context", "--type", "project",
           "--description", "current sprint goal", "--body", "finish auth module"])
    .output().unwrap();
```

### graphbit
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["recall", "decision log", "--k", "3", "--json"])
    .output().unwrap();
```

### agentai
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["hybrid-search", "previous decisions", "--k", "5", "--json"])
    .output().unwrap();
```

### llm-agent-runtime
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["recall", "user preferences", "--k", "5", "--json"])
    .output().unwrap();
```

### anda
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["stats", "--json"])
    .output().unwrap();
```

### adk-rust
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["recall", "tool outputs", "--k", "5", "--json"])
    .output().unwrap();
```

### rs-graph-llm
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["hybrid-search", "graph relations", "--k", "10", "--json"])
    .output().unwrap();
```

### genai
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["recall", "model context", "--k", "5", "--json"])
    .output().unwrap();
```

### liter-llm
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["remember", "--name", "session-notes", "--type", "user",
           "--description", "session recap", "--body", "discussed architecture"])
    .output().unwrap();
```

### llm-cascade
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["recall", "fallback context", "--k", "3", "--json"])
    .output().unwrap();
```

### async-openai
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["recall", "system prompt history", "--k", "5", "--json"])
    .output().unwrap();
```

### async-llm
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["hybrid-search", "chat context", "--k", "5", "--json"])
    .output().unwrap();
```

### anthropic-sdk
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["recall", "tool use patterns", "--k", "5", "--json"])
    .output().unwrap();
```

### ollama-rs
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["recall", "local model outputs", "--k", "5", "--json"])
    .output().unwrap();
```

### mistral-rs
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["hybrid-search", "inference context", "--k", "10", "--json"])
    .output().unwrap();
```

### llama-cpp-rs
```rust
use std::process::Command;
let out = Command::new("neurographrag")
    .args(["recall", "llama session context", "--k", "5", "--json"])
    .output().unwrap();
```


## Contributing
### Pull requests are welcome
- Read the contribution guidelines in [CONTRIBUTING.md](CONTRIBUTING.md)
- Open issues at the GitHub repository for bugs or feature requests
- Follow the code of conduct described in [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)


## Security
### Responsible disclosure policy
- Security reports follow the policy described in [SECURITY.md](SECURITY.md)
- Contact the maintainer privately before disclosing vulnerabilities publicly


## Changelog
### Release history tracked separately
- Read the full release history in [CHANGELOG.md](CHANGELOG.md)


## Acknowledgments
### Built on top of excellent open source
- `fastembed` provides local quantized embedding models without ONNX hassle
- `sqlite-vec` adds vector indexes directly inside SQLite as an extension
- `refinery` runs schema migrations with transactional safety guarantees
- `clap` powers the CLI argument parsing with derive macros
- `rusqlite` wraps SQLite with safe Rust bindings and bundled build


## License
### Dual license MIT OR Apache-2.0
- Licensed under either of Apache License 2.0 or MIT License at your option
- See `LICENSE-APACHE` and `LICENSE-MIT` in the repository root for full text
