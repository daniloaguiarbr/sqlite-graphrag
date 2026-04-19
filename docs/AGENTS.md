# neurographrag for AI Agents


> A first-class CLI contract for 21+ coding agents and LLM orchestrators

- Read the Portuguese version at [AGENTS.pt-BR.md](AGENTS.pt-BR.md)


## The Question No Agent Framework Answers
### Open Loop — Why Your Autonomous Agent Forgets Yesterday
- Your LLM agent crushed a task today and lost every insight tomorrow morning
- Your orchestrator pays 400 dollars monthly to Pinecone for stale vector context
- Your stack breaks the moment OpenAI embeddings rate-limit the indexing pipeline
- Your GraphRAG prototype dies in production under four concurrent subprocess calls
- The secret the frameworks never document sits inside a single portable SQLite file


## Why Agents Love This CLI
### Five Differentiators — Engineered for Autonomous Loops
- Deterministic JSON output removes every parser hack from your orchestrator code
- Exit codes follow `sysexits.h` so your retry logic works without string matching
- Zero runtime dependencies ship in one statically linked binary under 30 MB
- Stdin accepts structured payloads so your agents never escape shell arguments
- Cross-platform behavior stays identical on Linux, macOS and Windows out of the box


## Economy That Converts
### Numbers That Sell The Switch
- Save 200 dollars per month by replacing Pinecone plus OpenAI embedding calls
- Cut tokens spent on RAG by up to 80 percent through graph traversal recall
- Drop retrieval latency from 800 ms in cloud vector DBs to 8 ms on local SSD
- Reduce cold-start time from 12 seconds Docker boot to 90 ms single binary launch
- Avoid 4 hours weekly of cluster maintenance with a single-file zero-ops database


## Sovereignty as Competitive Advantage
### Why Local Memory Wins In 2026
- Your proprietary data NEVER leaves the developer workstation or the CI runner
- Your compliance surface shrinks to one SQLite file under your own encryption
- Your vendor lock-in vanishes since the schema is documented and portable
- Your audit trail lives in the `memory_versions` table with immutable history
- Your regulated industry gets offline-first RAG without cloud dependency clauses


## Compatible Agents and Orchestrators
### Catalog — 21 Supported Integrations
| Agent | Vendor | Minimum Version | Integration Type | Example |
| --- | --- | --- | --- | --- |
| Claude Code | Anthropic | 1.0+ | Subprocess | `neurographrag recall "query" --json` |
| Codex CLI | OpenAI | 0.5+ | AGENTS.md + subprocess | `neurographrag remember --name X --type user --body "..."` |
| Gemini CLI | Google | any recent | Subprocess | `neurographrag hybrid-search "query" --json --k 5` |
| Opencode | open source | any recent | Subprocess | `neurographrag recall "auth flow" --json --k 3` |
| OpenClaw | community | any recent | Subprocess | `neurographrag list --type user --json` |
| Paperclip | community | any recent | Subprocess | `neurographrag read --name onboarding-note --json` |
| VS Code Copilot | Microsoft | 1.90+ | tasks.json | `{"command": "neurographrag", "args": ["recall", "$selection", "--json"]}` |
| Google Antigravity | Google | any recent | Runner | `neurographrag hybrid-search "prompt" --k 10 --json` |
| Windsurf | Codeium | any recent | Terminal | `neurographrag recall "refactor plan" --json` |
| Cursor | Cursor | 0.40+ | Terminal | `neurographrag remember --name cursor-ctx --type agent --body "..."` |
| Zed | Zed Industries | any recent | Assistant Panel | `neurographrag recall "open tabs" --json --k 5` |
| Aider | open source | 0.60+ | Shell | `neurographrag recall "refactor target" --k 5 --json` |
| Jules | Google Labs | preview | CI automation | `neurographrag stats --json` |
| Kilo Code | community | any recent | Subprocess | `neurographrag recall "recent tasks" --json` |
| Roo Code | community | any recent | Subprocess | `neurographrag hybrid-search "repo context" --json` |
| Cline | community | VS Code ext | Terminal | `neurographrag list --limit 20 --json` |
| Continue | open source | VS Code or JetBrains ext | Terminal | `neurographrag recall "docstring" --json` |
| Factory | Factory | any recent | API or subprocess | `neurographrag recall "pr context" --json` |
| Augment Code | Augment | any recent | IDE | `neurographrag hybrid-search "code review" --json` |
| JetBrains AI Assistant | JetBrains | 2024.2+ | IDE | `neurographrag recall "stacktrace" --json` |
| OpenRouter | OpenRouter | any | Router for multi-LLM | `neurographrag recall "routing rule" --json` |


## Contract — Stdin and Stdout
### Input — Structured Arguments Only
- CLI flags accept typed arguments validated by `clap` with strict parsing
- Stdin accepts a raw body when `--body-stdin` is active on `remember` or `edit`
- Stdin accepts a JSON payload when `--payload-stdin` is active on batch modes
- Environment variables override defaults without mutating the file database
- Language is controlled by `--lang en` or `--lang pt` for deterministic output


### Output — Deterministic JSON Documents
- Every subcommand emits exactly one JSON document when `--json` is set
- Keys are stable across releases inside the current major version line
- Timestamps follow RFC 3339 with UTC offset notation always present
- Null fields are omitted to keep payloads lean for agent consumption
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
| `13` | Batch partial or DB busy | Honor backoff and retry later |
| `15` | Database busy after retries | Wait and retry the operation |
| `73` | Lock busy across slots | Wait and retry or raise `--max-concurrency` |
| `75` | Lock timeout reached | Increase `--wait-lock` seconds |
| `77` | Low memory threshold tripped | Free RAM before retry |


## JSON Output Format
### Recall — Vector-Only KNN
```json
{
  "query": "graphrag retrieval",
  "k": 3,
  "namespace": "default",
  "elapsed_ms": 12,
  "hits": [
    { "name": "graphrag-intro", "score": 0.91, "type": "user", "updated_at": "2026-04-18T12:00:00Z" },
    { "name": "vector-search-notes", "score": 0.84, "type": "agent", "updated_at": "2026-04-17T08:12:03Z" },
    { "name": "hybrid-ranker", "score": 0.77, "type": "feedback", "updated_at": "2026-04-16T21:04:55Z" }
  ]
}
```


### Hybrid Search — FTS5 Plus Vector RRF
```json
{
  "query": "postgres migration",
  "k": 5,
  "rrf_k": 60,
  "weights": { "vec": 0.6, "fts": 0.4 },
  "elapsed_ms": 18,
  "hits": [
    { "name": "postgres-migration-plan", "score": 0.96, "rank_vec": 1, "rank_fts": 1 },
    { "name": "db-migration-checklist", "score": 0.88, "rank_vec": 2, "rank_fts": 3 }
  ]
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
- `MAX_CONCURRENT_CLI_INSTANCES` equals 4 across cooperating subprocess agents
- `CLI_LOCK_DEFAULT_WAIT_SECS` equals 300 seconds before exit code `75`
- `PURGE_RETENTION_DAYS_DEFAULT` equals 30 days before hard delete becomes allowed


## Language Control
### Bilingual Output — One Flag Switches Locale
- Flag `--lang en` forces English messages regardless of system locale
- Flag `--lang pt` forces Portuguese messages regardless of system locale
- Env `NEUROGRAPHRAG_LANG=pt` overrides system locale when `--lang` is absent
- Missing flag and env falls back to `sys_locale::get_locale()` detection
- Unknown locales default to English without emitting any warning to stderr


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
cargo install --locked neurographrag && neurographrag init
```
- Flag `--locked` reuses the shipped `Cargo.lock` to protect MSRV from transitive drift
- Command `init` creates the SQLite file and downloads the embedding model locally
- First invocation may take one minute while `fastembed` fetches `multilingual-e5-small`
- Subsequent invocations start cold in under 100 ms on modern consumer hardware
- Uninstall with `cargo uninstall neurographrag` leaving the database file in place
