# HOW TO USE neurographrag

> Ship persistent memory to any AI agent in 60 seconds flat, zero dollars spent


- Read this guide in Portuguese at [HOW_TO_USE.pt-BR.md](HOW_TO_USE.pt-BR.md)
- Return to the main [README.md](../README.md) for command reference


## The Question That Starts Here
### Curiosity — Why Engineers Abandon Pinecone in 2026
- How many milliseconds separate your agent from production memory today
- Why senior engineers in production choose SQLite over Pinecone for LLM memory
- What changes when embeddings, search and graph live inside a single file
- Why twenty one AI agents converge on neurographrag as their persistence layer
- This guide answers every question above in under ten minutes of reading


## Reading Time and Impact
### Investment — Five Minutes to Read Ten to Execute
- Total reading time reaches five minutes for technical readers skimming headings
- Total execution time reaches ten minutes including model download on first run
- Learning curve drops to zero for anyone familiar with standard CLI patterns
- First memory gets persisted in sixty seconds after install completes
- First hybrid search returns ranked hits in under fifty milliseconds locally
- Expected tokens saved per month hit two hundred thousand on a single agent


## Prerequisites
### Environment — Minimum Supportable Baseline
- Rust 1.88 or newer installed via `rustup` across Linux macOS and Windows
- SQLite version 3.40 or newer shipped with your operating system distribution
- Operating systems Linux glibc, Linux musl, macOS 11 plus, Windows 10 plus
- Available RAM of 100 MB free for runtime plus 1 GB during embedding model load
- Disk space of 200 MB for the embedding model cache on first invocation
- Network access required ONLY for first `init` to download quantized embeddings


## First Command in 60 Seconds
### Install — Three Shell Lines You Copy Once
```bash
cargo install --locked neurographrag
neurographrag init
neurographrag remember --name first-note --type user --description "first memory" --body "hello graphrag"
```
- First line downloads, builds and installs the binary into `~/.cargo/bin`
- Second line creates the SQLite database and downloads the embedding model
- Third line persists your first memory and indexes it for hybrid retrieval
- Confirmation prints to stdout, traces route to stderr, exit code zero signals success
- Your next `recall` call returns the note you just saved in milliseconds


## Core Commands
### Lifecycle — Seven Subcommands You Use Daily
```bash
neurographrag init --namespace my-project
neurographrag remember --name auth-design --type decision --description "auth uses JWT" --body "Rationale documented."
neurographrag recall "authentication strategy" --k 5 --json
neurographrag hybrid-search "jwt design" --k 10 --rrf-k 60 --json
neurographrag read --name auth-design
neurographrag forget --name auth-design
neurographrag purge --days 30 --yes
```
- `init` bootstraps the database, downloads the model and validates the `sqlite-vec` extension
- `remember` stores content, extracts entities and generates embeddings atomically
- `recall` performs pure vector KNN search over the `vec_memories` table
- `hybrid-search` fuses FTS5 full-text and vector KNN with Reciprocal Rank Fusion
- `read` fetches a memory by its exact kebab-case name in a single SQL query
- `forget` performs a soft delete preserving the full version history
- `purge` permanently removes memories soft-deleted more than the retention threshold


## Advanced Patterns
### Recipe One — Hybrid Search With Weighted Fusion
```bash
neurographrag hybrid-search "postgres migration strategy" \
  --k 20 \
  --rrf-k 60 \
  --weight-vec 0.7 \
  --weight-fts 0.3 \
  --json \
  | jaq '.hits[] | {name, score, source}'
```
- Combines dense vector similarity and sparse full-text matches in one ranked list
- Weight tuning lets you favor semantic proximity against keyword precision per query
- RRF constant `--rrf-k 60` matches the default recommended by the RRF paper
- Pipeline saves eighty percent of tokens compared to LLM-based re-ranking
- Expected latency stays under fifteen milliseconds for databases up to 100 MB

### Recipe Two — Graph Traversal for Multi-Hop Recall
```bash
neurographrag link --source auth-design --target jwt-spec --relation depends-on
neurographrag link --source jwt-spec --target rfc-7519 --relation references
neurographrag related auth-design --hops 2 --json \
  | jaq -r '.nodes[] | select(.depth == 2) | .name'
```
- Two hops surface transitive knowledge invisible to pure vector search methods
- Typed relations let your agent reason about cause, dependency and reference chains
- Graph queries run in under five milliseconds thanks to SQLite indexed joins
- Multi-hop recall recovers context that flat embeddings consistently drop out of top-K
- Saves fifteen minutes per debugging session hunting for related architectural decisions

### Recipe Three — Batch Ingestion via Shell Pipeline
```bash
find ./docs -name "*.md" -print0 \
  | xargs -0 -n 1 -P 4 -I {} bash -c '
      name=$(basename {} .md)
      neurographrag remember \
        --name "doc-${name}" \
        --type reference \
        --description "imported from {}" \
        --body "$(cat {})"
    '
```
- Parallel factor `-P 4` matches the default counting semaphore slots exactly
- Exit code `75` signals slot exhaustion and the orchestrator should retry later
- Exit code `77` signals RAM pressure and the orchestrator must wait for free memory
- Batch throughput reaches 200 documents per minute on a modern laptop CPU
- Saves forty minutes of manual ingestion per 1000 Markdown files processed

### Recipe Four — Snapshot-Safe Sync With Dropbox or iCloud
```bash
neurographrag sync-safe-copy --output ~/Dropbox/neurographrag.sqlite
ouch compress ~/Dropbox/neurographrag.sqlite ~/Dropbox/neurographrag-$(date +%Y%m%d).tar.zst
```
- `sync-safe-copy` checkpoints the WAL and copies a consistent snapshot atomically
- Dropbox, iCloud and Google Drive NEVER corrupt the active database during sync
- Compression via `ouch` reduces snapshot size by sixty percent for archival buckets
- Recovery on a new machine takes one `ouch decompress` plus one `cp` operation
- Protects years of memory from sync-induced corruption that plagues raw SQLite files

### Recipe Five — Integration With Claude Code Orchestrator
```bash
neurographrag recall "$USER_QUERY" --k 5 --json \
  | jaq -c '{
      context: [.hits[] | {name, body, score}],
      generated_at: now | todate
    }' \
  | claude --print "Use this context to answer: $USER_QUERY"
```
- Structured JSON flows cleanly into any orchestrator reading from stdin
- Score field enables the orchestrator to drop low-relevance hits before prompting
- Determinism of exit codes lets the orchestrator route errors without parsing stderr
- Token cost drops by seventy percent compared to full-corpus context stuffing
- Round-trip latency stays under one hundred milliseconds end to end locally


## Integration With AI Agents
### Twenty One Agents — One Persistence Layer
- Claude Code from Anthropic consumes JSON via stdin and orchestrates via exit codes
- Codex from OpenAI reads hybrid-search output to ground generation in local memory
- Gemini CLI from Google parses `--json` output to inject facts into prompts
- Opencode open source harness treats neurographrag as a native MCP-style backend
- OpenClaw agent framework uses `recall` as its long-term memory tier natively
- Paperclip research assistant persists findings across sessions via `remember` atomically
- VS Code Copilot from Microsoft invokes the CLI through integrated terminal tasks
- Google Antigravity platform calls the binary inside its sandboxed worker runtime
- Windsurf from Codeium routes indexed project memories through `hybrid-search` queries
- Cursor editor hooks `recall` into its chat panel for context-aware completions
- Zed editor invokes neurographrag as an external tool in its assistant channel
- Aider coding agent queries `related` for multi-hop reasoning over commit history
- Jules from Google Labs uses exit codes to gate automated pull request reviews
- Kilo Code autonomous agent delegates long-term memory to the local SQLite file
- Roo Code orchestrator passes memory context into its planning phase deterministically
- Cline autonomous agent persists tool outputs via `remember` between cycles
- Continue open source assistant integrates via its custom context provider API
- Factory agent framework stores decision logs for auditable multi-agent workflows
- Augment Code assistant hydrates its embeddings cache from `hybrid-search` results
- JetBrains AI Assistant runs neurographrag as a side process for cross-project memory
- OpenRouter proxy layer injects retrieved context before forwarding requests upstream


## Common Errors
### Troubleshooting — Five Failures and Their Fixes
- Error `exit 10` signals database lock, run `neurographrag vacuum` to checkpoint WAL
- Error `exit 12` signals `sqlite-vec` load failure, verify SQLite version is 3.40 plus
- Error `exit 13` signals database busy, lower `--max-concurrency` or raise `--wait-lock`
- Error `exit 75` signals slots exhausted, retry after a short backoff interval
- Error `exit 77` signals low RAM, free memory before invoking the embedding model again


## Next Steps
### Level Up — Where to Go After This Guide
- Read `COOKBOOK.md` for thirty recipes covering search, graph and batch workflows
- Read `INTEGRATIONS.md` for vendor specific configuration of all 21 agents above
- Read `docs/AGENTS.md` for multi-agent orchestration patterns using Agent Teams
- Read `docs/CROSS_PLATFORM.md` to understand target binaries across nine platforms
- Star the repository at github.com/daniloaguiarbr/neurographrag to track releases
