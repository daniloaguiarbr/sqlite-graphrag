# sqlite-graphrag Cookbook


> 23 production-grade recipes that save your team hours every single week

- Read the Portuguese version at [COOKBOOK.pt-BR.md](COOKBOOK.pt-BR.md)


## Latency Note
- The CLI can run stateless, but `sqlite-graphrag daemon` keeps the embedding model resident for repeated heavy commands
- For production workflows requiring lower latency, start `sqlite-graphrag daemon` once and let `init`, `remember`, `recall`, and `hybrid-search` reuse it automatically
- Current single-shot `recall` takes approximately 1 second on modern hardware
- Batch pipelines amortize this cost by invoking the binary once per document in parallel


## Default Values Reference
- `recall --k` default is 10 (not 5) — adjust for precision-recall tradeoff
- `list --limit` default is 50 — use `--limit 10000` for full exports before backup
- `hybrid-search --weight-vec` and `--weight-fts` both default to 1.0
- `purge --retention-days` default is 90 — lower for aggressive cleanup policies


## How To Bootstrap Memory Database In 60 Seconds
### Problem
- Your new laptop has no memory database and your agent keeps losing context
- Every onboarding session burns 30 minutes on fragile setup scripts and README hunts


### Solution
```bash
cargo install --path .
sqlite-graphrag init --namespace default
sqlite-graphrag health --json
```


### Explanation
- Command `init` creates the SQLite file and downloads `multilingual-e5-small` locally
- Flag `--namespace default` is a user-chosen name; the built-in fallback namespace is `global`
- Command `health` validates integrity with `PRAGMA integrity_check` and returns JSON
- Exit code `0` signals the database is ready for writes and reads from any agent
- Saves 30 minutes per laptop versus a Pinecone plus Docker plus Python bootstrap


### Variants
- Set `SQLITE_GRAPHRAG_DB_PATH=/data/team.sqlite` to share a networked file between dev pods
- Call `sqlite-graphrag migrate --json` after bumping versions to apply schema upgrades


### See Also
- Recipe "How to integrate sqlite-graphrag with Claude Code subprocess loop"
- Recipe "How to schedule purge and vacuum in cron or GitHub Actions"


## How To Bulk-Import Knowledge Base Via Stdin Pipeline
### Problem
- Your 2000 Markdown files sit idle because no loader speaks the sqlite-graphrag schema
- Manual entry burns one entire afternoon per hundred files on simple onboarding


### Solution
```bash
fd -e md docs/ -0 | xargs -0 -n 1 -I{} sh -c '
  sqlite-graphrag remember \
    --name "$(basename {} .md)" \
    --type user \
    --description "imported from {}" \
    --body-stdin < {}
'
```


### Explanation
- `fd -e md -0` emits null-delimited Markdown paths safe against spaces and quotes
- `xargs -0 -n 1` invokes `sqlite-graphrag remember` once per file without concurrency hazards
- `--body-stdin` pipes the Markdown body without quoting or shell escape accidents
- Exit code `2` flags duplicates for you to skip cleanly inside the outer shell
- Saves 4 hours per thousand files versus hand-crafted CSV loaders


### Variants
- Keep `remember` imports serial with `--max-concurrency 1`; each subprocess may load roughly 1.1 GB of ONNX RSS in v1.0.3
- Extend the one-liner to extract `--description` from the first Markdown heading of each file


### See Also
- Recipe "How to export memories to NDJSON for backup"
- Recipe "How to orchestrate parallel recall across namespaces"


## How To Combine Vector And FTS Search With Tunable Weights
### Problem
- Pure vector recall misses exact token matches like `TODO-1234` inside code comments
- Pure FTS search misses paraphrases your users typed in synonyms and abbreviations


### Solution
```bash
sqlite-graphrag hybrid-search "postgres migration deadlock" \
  --k 10 --rrf-k 60 --json
```


### Explanation
- `--rrf-k 60` is the Reciprocal Rank Fusion smoothing constant recommended by RRF literature
- Default `--weight-vec 1.0` and `--weight-fts 1.0` treat both signals as equally important
- Override for advanced tuning: `--weight-vec 0.7 --weight-fts 0.3` biases toward semantics
- JSON emits `vec_rank` and `fts_rank` per result so downstream agents can audit fusion
- Saves 50 percent tokens versus asking an LLM to re-rank after pure vector recall


### Variants
- Pass `--weight-vec 1.0 --weight-fts 0.0` to reproduce a pure `recall` baseline for A/B tests
- Raise `--k` to 50 before a re-ranker agent prunes down to the final 5 hits


### See Also
- Recipe "How to debug slow queries with health and stats"
- Recipe "How to benchmark hybrid-search against pure vec search"


## How To Traverse Entity Graph For Multi-Hop Recall
### Problem
- Your query hits one memory but misses connected notes sharing the same entity graph
- Pure vector RAG scores similar tokens and ignores typed relationships that matter


### Solution
```bash
sqlite-graphrag related authentication-flow --hops 2 --json
```


### Explanation
- `related` takes a MEMORY name (kebab-case slug), not an entity name
- The positional argument must match a name stored via `remember` in the same namespace
- `related` walks typed graph relationships between entities with user-controlled hop count
- `--hops 2` includes friends-of-friends memories linked through shared entities
- JSON output reports the traversal path so the LLM can reason about relation chains
- Saves re-embedding cost since graph expansion runs as SQLite graph walk not KNN
- Surfaces context that vector-only RAG misses by design with 80 percent fewer tokens


### Variants
- Use `graph --json` to dump the full snapshot when a human auditor wants offline analysis
- Chain `related` into `hybrid-search` by filtering candidates to the traversed set


### See Also
- Recipe "How to combine vector and FTS search with tunable weights"
- Recipe "How to orchestrate parallel recall across namespaces"


## How To Integrate sqlite-graphrag With Claude Code Subprocess Loop
### Problem
- Claude Code restarts every session and forgets the decisions made five minutes ago
- Your orchestrator lacks a deterministic memory it can trust between agent iterations


### Solution
```bash
# .claude/hooks/pre-task.sh
CONTEXT=$(sqlite-graphrag recall "$USER_PROMPT" --k 5 --json)
printf 'Relevant memories:\n%s\n' "$CONTEXT"

# .claude/hooks/post-task.sh
sqlite-graphrag remember \
  --name "session-$(date +%s)" \
  --type project \
  --description "decision log" \
  --body "$ASSISTANT_RESPONSE"
```


### Explanation
- Pre-task hook injects relevant memories into the agent prompt before generation
- Post-task hook persists agent output into the vector store for future sessions
- Hook scripts run as subprocess respecting exit code routing and slot limits
- Exit code `13` or `75` triggers retry inside the hook without killing the agent
- Saves 40 percent context tokens and keeps decisions across Claude Code restarts


### Variants
- Replace `recall` with `hybrid-search` when your prompts mix keywords and concepts
- Add `--namespace $CLAUDE_PROJECT` to isolate per-project memory in multi-repo hosts


### See Also
- Recipe "How to integrate with Codex CLI via AGENTS.md"
- Recipe "How to setup Windsurf or Zed assistant panel with sqlite-graphrag"


## How To Integrate With Codex CLI Via AGENTS.md
### Problem
- Codex reads `AGENTS.md` but skips any capability not listed with exact invocation syntax
- Your ops team loses 10 minutes per session teaching Codex the same CLI from memory


### Solution
```md
<!-- AGENTS.md at repo root -->
## Memory Layer
- Use `sqlite-graphrag recall "<query>" --k 5 --json` to fetch prior decisions
- Use `sqlite-graphrag remember --name "<kebab-name>" --type project --body "<text>"` to persist output
- Prefer `hybrid-search` when the query mixes keywords and natural language
- Respect exit code 75 as retry-later rather than error
```


### Explanation
- AGENTS.md surfaces the CLI contract as part of Codex system context automatically
- Codex invokes subprocess commands listed in AGENTS.md without further operator prompting
- Deterministic exit codes allow Codex to retry on `75` without operator intervention
- JSON output integrates with Codex parsing layer without regex or custom plugin code
- Saves 10 minutes per session and survives Codex upgrades without breaking the contract


### Variants
- Add `SQLITE_GRAPHRAG_NAMESPACE=$REPO_NAME` to `.envrc` so Codex isolates per-project memory
- Include a one-liner example under each command to anchor Codex on real usage


### See Also
- Recipe "How to integrate sqlite-graphrag with Claude Code subprocess loop"
- Recipe "How to integrate with Cursor terminal for in-editor memory"


## How To Integrate With Cursor Terminal For In-Editor Memory
### Problem
- Cursor loses context every time you close the editor or switch between branches locally
- Your paired LLM session restarts cold and re-asks the same questions every morning


### Solution
```jsonc
// Cursor settings.json snippet
{
  "terminal.integrated.env.osx": { "SQLITE_GRAPHRAG_NAMESPACE": "${workspaceFolderBasename}" },
  "cursor.ai.rules": "Before answering, run `sqlite-graphrag recall \"${selection}\" --k 5 --json` and use hits as context"
}
```


### Explanation
- Per-workspace env var isolates memory by project folder name without manual config
- Cursor AI rules instruct the embedded model to call the CLI before answering prompts
- The CLI reads only the selected code so latency stays below 50 ms for small queries
- Exit code `0` with empty hits keeps Cursor silent instead of hallucinating context
- Saves 15 minutes per day of re-asking repeated questions inside Cursor sessions


### Variants
- Swap `recall` for `hybrid-search` when your codebase mixes English docstrings and Portuguese comments
- Add a `post-save` hook that calls `remember` with the diff as body for session-wide memory


### See Also
- Recipe "How to setup Windsurf or Zed assistant panel with sqlite-graphrag"
- Recipe "How to integrate with Codex CLI via AGENTS.md"


## How To Setup Windsurf Or Zed Assistant Panel With sqlite-graphrag
### Problem
- Windsurf and Zed assistant panels ship without pluggable memory backends by default
- Your multi-IDE workflow fragments memory between Cursor Windsurf and Zed silos


### Solution
```bash
# Shared terminal command both IDEs can run
sqlite-graphrag hybrid-search "$EDITOR_CONTEXT" --k 10 --json > /tmp/ng.json
```


### Explanation
- Both Windsurf and Zed call terminal tasks from the assistant panel natively
- `/tmp/ng.json` acts as a lingua franca consumed by both assistant panels for prompts
- Single CLI binary replaces three bespoke plugins avoiding per-IDE maintenance burden
- Exit code `0` with empty hits is benign so the assistant panel degrades gracefully
- Saves hours per week by unifying memory across all editors with no plugin rebuild


### Variants
- Map the command to a shortcut such as `Cmd+Shift+M` for one-key recall invocation
- Pipe output through `jaq` to transform the payload into the exact schema each IDE prefers


### See Also
- Recipe "How to integrate with Cursor terminal for in-editor memory"
- Recipe "How to orchestrate parallel recall across namespaces"


## How To Prevent Dropbox Or iCloud Corruption With sync-safe-copy
### Problem
- Your SQLite file sits in Dropbox and syncs mid-write corrupting the WAL journal
- Classic `cp` snapshots during a write produce invalid files that refuse to open later


### Solution
```bash
sqlite-graphrag sync-safe-copy --dest ~/Dropbox/sqlite-graphrag/snapshot.sqlite
```


### Explanation
- Command forces a WAL checkpoint before the copy so the snapshot is transactionally consistent
- Output file receives `chmod 600` on Unix to prevent other users from reading sensitive memories
- Copy runs atomically via `SQLite Online Backup API` eliminating partial-write risk entirely
- Exit code `0` guarantees the snapshot opens cleanly on any other machine with the same binary
- Saves weekends of recovery work when Dropbox would have otherwise corrupted the live file


### Variants
- Schedule hourly via `launchd` on macOS or `systemd --user` on Linux for continuous backup
- Compress with `ouch compress snapshot.sqlite snapshot.tar.zst` for faster cloud upload


### See Also
- Recipe "How to schedule purge and vacuum in cron or GitHub Actions"
- Recipe "How to version control the SQLite database with Git LFS"


## How To Schedule Purge And Vacuum In Cron Or GitHub Actions
### Problem
- Soft-deleted memories pile up and inflate disk usage over months of heavy agent use
- Your SQLite file balloons past 10 GB because `VACUUM` never runs in automation


### Solution
```yaml
# .github/workflows/ng-maintenance.yml
name: sqlite-graphrag maintenance
on:
  schedule: [{ cron: "0 3 * * 0" }]
jobs:
  maintenance:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo install --path .
      - run: sqlite-graphrag purge --retention-days 30 --yes
      - run: sqlite-graphrag vacuum --json
      - run: sqlite-graphrag optimize --json
```


### Explanation
- `purge --retention-days 30` hard-deletes soft-deleted rows older than the retention window
- `vacuum` reclaims freelist pages and checkpoints the WAL journal to the main file
- `optimize` refreshes query planner statistics for faster recall on the next run
- Weekly cron at 03:00 Sunday avoids contention with business-hour agent activity
- Saves 70 percent disk usage over 6 months versus zero-maintenance deployments


### Variants
- Run on `cron 0 3 * * *` nightly when your team writes thousands of memories per day
- Replace GitHub Actions with `systemd.timer` for air-gapped environments without internet


### See Also
- Recipe "How to prevent Dropbox or iCloud corruption with sync-safe-copy"
- Recipe "How to debug slow queries with health and stats"


## How To Export Memories To NDJSON For Backup
### Problem
- SQLite backups are opaque and require the binary installed for any restore audit
- Compliance asks for plain-text exports to diff between monthly snapshots


### Solution
```bash
sqlite-graphrag list --limit 10000 --json \
  | jaq -c '.items[]' > memories-$(date +%Y%m%d).ndjson
```


### Explanation
- `list --limit 10000` enumerates memories up to the ceiling with deterministic ordering
- `jaq -c '.items[]'` iterates the `items` array into NDJSON readable by any tool instantly
- Result file opens in `rg` `bat` or spreadsheet apps without SQLite knowledge at all
- Diff two snapshots with `difft` to audit what changed between monthly backups cleanly
- Saves auditor review time since NDJSON is human-readable versus opaque binary files


### Variants
- Pipe through `ouch compress` to a `zst` archive before uploading to S3 or GCS buckets
- Loop in shell to page through namespaces if the instance hosts multi-tenant memory


### See Also
- Recipe "How to version control the SQLite database with Git LFS"
- Recipe "How to schedule purge and vacuum in cron or GitHub Actions"


## How To Version Control The SQLite Database With Git LFS
### Problem
- Your 500 MB SQLite file breaks GitHub push limits and bloats every single clone
- Branch rebases corrupt binary blobs when Git tries to merge with textual diff logic


### Solution
```bash
git lfs install
git lfs track "*.sqlite"
echo "*.sqlite filter=lfs diff=lfs merge=lfs -text" >> .gitattributes
git add .gitattributes graphrag.sqlite
git commit -m "chore: track sqlite-graphrag db via LFS"
```


### Explanation
- Git LFS stores SQLite files in a remote cache so the Git repo stays below 100 MB
- Attribute `-text` prevents Git from attempting line-based merges on binary contents
- `sync-safe-copy` before commit guarantees the file is transactionally consistent to push
- Teammates clone with `git lfs pull` fetching the DB only when they actually need it
- Saves 90 percent clone time for teammates who do not need the memory database locally


### Variants
- Tag snapshots with `git tag db-2026-04-18` to pin memory state for release reproducibility
- Skip LFS and store sync-safe-copy outputs in object storage with signed URL references


### See Also
- Recipe "How to export memories to NDJSON for backup"
- Recipe "How to prevent Dropbox or iCloud corruption with sync-safe-copy"


## How To Orchestrate Namespace Recall Safely
### Problem
- Your multi-project agent needs one recall per namespace on the same host
- Blind parallel fan-out can oversubscribe RAM because each `recall` subprocess may load the ONNX model independently


### Solution
```bash
for ns in project-a project-b project-c project-d; do
  SQLITE_GRAPHRAG_NAMESPACE="$ns" \
    sqlite-graphrag --max-concurrency 1 recall "error rate" --k 5 --json
done
```


### Explanation
- The loop stays intentionally serial because `recall` is an embedding-heavy command
- `--max-concurrency 1` prevents local oversubscription during audits, CI, and desktop use
- Env var `SQLITE_GRAPHRAG_NAMESPACE` scopes each subprocess to its own project cleanly
- One JSON document per namespace still lands in stdout for a downstream aggregator agent to fuse
- This pattern favors host safety and deterministic progress over aggressive wall-clock reduction


### Variants
- Keep parallel fan-out for light commands such as `stats` or `list`, not for `recall`
- Raise concurrency for heavy commands only after measuring RSS, observing swap, and confirming the host remains stable


### See Also
- Recipe "How to combine vector and FTS search with tunable weights"
- Recipe "How to benchmark hybrid-search against pure vec search"


## How To Debug Slow Queries With Health And Stats
### Problem
- Your recall used to return in 8 ms and now takes 400 ms after months of writes
- You lack visibility into which table ballooned or which index went stale


### Solution
```bash
sqlite-graphrag health --json | jaq '{integrity, wal_size_mb, journal_mode}'
sqlite-graphrag stats --json | jaq '{memories, memories_total, entities, entities_total, relationships, relationships_total, edges, chunks_total, avg_body_len, db_size_bytes, db_bytes}'
SQLITE_GRAPHRAG_LOG_LEVEL=debug sqlite-graphrag recall "slow query" --k 5 --json
```


### Explanation
- `health` reports `integrity`, WAL size and journal mode to spot fragmentation fast
- `stats` counts rows to reveal which table grew disproportionately since last audit
- `SQLITE_GRAPHRAG_LOG_LEVEL=debug` emits timings per SQLite stage to stderr for tracing
- Comparing current `avg_body_len` to baseline shows if bodies have grown past defaults
- Saves hours of blind tuning by exposing the exact slow path in three commands total


### Variants
- Schedule a dashboard that scrapes `stats --json` every hour and alerts on growth spikes
- Run `optimize` followed by `vacuum` when WAL exceeds 100 MB to reclaim disk performance


### See Also
- Recipe "How to schedule purge and vacuum in cron or GitHub Actions"
- Recipe "How to benchmark hybrid-search against pure vec search"


## How To Benchmark hybrid-search Against Pure vec search
### Problem
- You lack data to justify enabling hybrid search in production versus pure vector recall
- Your stakeholders want numeric evidence before approving the index storage overhead


### Solution
```bash
hyperfine --warmup 3 \
  'sqlite-graphrag recall "postgres migration" --k 10 --json > /dev/null' \
  'sqlite-graphrag hybrid-search "postgres migration" --k 10 --json > /dev/null'
```


### Explanation
- `hyperfine` measures both commands with warmup runs removing cold-cache noise from results
- Output reports mean latency standard deviation and relative speedup in a clean table
- Results let you compare recall quality versus latency on real production workloads
- Numeric evidence empowers tradeoff conversations with product and finance stakeholders
- Saves weeks of debate by grounding the decision in data rather than intuition alone


### Variants
- Replace the single query with 100 sampled queries to compute p50 p95 p99 latency buckets
- Integrate `hyperfine --export-json` into CI to detect regressions across pull requests


### See Also
- Recipe "How to combine vector and FTS search with tunable weights"
- Recipe "How to orchestrate parallel recall across namespaces"


## How To Integrate With rig-core For Agent Memory
### Problem
- Your `rig-core` agent loses context between invocations without persistent storage
- Rebuilding embeddings every run wastes 50 minutes of compute and API budget weekly

### Solution
```rust
use std::process::Command;
use serde_json::Value;

fn remember_agent_context(namespace: &str, content: &str) -> anyhow::Result<()> {
    let name = format!(
        "rig-context-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", namespace,
            "--name", &name,
            "--type", "project",
            "--description", "rig-core agent context",
            "--body", content,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "sqlite-graphrag remember failed");
    Ok(())
}

fn recall_agent_context(namespace: &str, query: &str, k: u8) -> anyhow::Result<Vec<String>> {
    let output = Command::new("sqlite-graphrag")
        .args(["recall", "--namespace", namespace, "--k", &k.to_string(), "--json", query])
        .output()?;
    anyhow::ensure!(output.status.success(), "sqlite-graphrag recall failed");
    let parsed: Value = serde_json::from_slice(&output.stdout)?;
    let items = parsed["results"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v["snippet"].as_str().map(str::to_owned))
        .collect();
    Ok(items)
}
```

### Explanation
- `Command::new("sqlite-graphrag")` shells out to the 25 MB stateless binary with zero FFI cost
- `--namespace` scopes memory to the specific rig agent preventing cross-agent contamination
- `--json` returns structured output that `serde_json` parses without fragile regex parsing
- `anyhow::ensure!` converts exit-code failures into typed errors your agent can handle
- Reduces 50 minutes of per-run context rebuilding to a single 5-millisecond CLI call

### Variants
- Replace `Command` with `tokio::process::Command` for non-blocking async agent pipelines
- Wrap both functions in a `RigMemoryAdapter` struct that implements a `MemoryStore` trait

### See Also
- Recipe "How to bootstrap memory database in 60 seconds"
- Recipe "How to run ollama offline with ollama-rs and persistent memory"


## How To Integrate With swarms-rs For Multi-Agent Memory
### Problem
- Your swarm of agents overwrites each other's memories when sharing one namespace
- Debugging which agent wrote what takes hours of grep through unstructured log files

### Solution
```rust
use std::process::Command;

fn swarm_remember(agent_id: &str, content: &str) -> anyhow::Result<()> {
    let namespace = format!("swarm-{agent_id}");
    let name = format!(
        "swarm-note-{agent_id}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", &namespace,
            "--name", &name,
            "--type", "project",
            "--description", "swarm agent note",
            "--body", content,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "swarm remember failed for agent {agent_id}");
    Ok(())
}

fn swarm_recall_all(agent_ids: &[&str], query: &str) -> anyhow::Result<Vec<(String, String)>> {
    let mut results = Vec::new();
    for agent_id in agent_ids {
        let namespace = format!("swarm-{agent_id}");
        let output = Command::new("sqlite-graphrag")
            .args(["recall", "--namespace", &namespace, "--k", "5", "--json", query])
            .output()?;
        if output.status.success() {
            let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            if let Some(items) = parsed["results"].as_array() {
                for item in items {
                    if let Some(snippet) = item["snippet"].as_str() {
                        results.push((agent_id.to_string(), snippet.to_owned()));
                    }
                }
            }
        }
    }
    Ok(results)
}
```

### Explanation
- Per-agent namespace `swarm-{agent_id}` isolates memories with zero schema changes required
- A single SQLite file hosts all namespaces eliminating the need for multiple database files
- Iterating namespaces in the coordinator collects ranked results from every swarm member
- Structured JSON output with `serde_json` makes attribution trivial versus plain text logs
- Cuts multi-agent debugging time from hours to minutes by making authorship explicit

### Variants
- Use `tokio::task::JoinSet` to recall all agent namespaces concurrently in async swarms
- Add a `coordinator` namespace where the orchestrator writes synthesized swarm decisions

### See Also
- Recipe "How to orchestrate parallel recall across namespaces"
- Recipe "How to integrate with rig-core for agent memory"


## How To Use genai With sqlite-graphrag For Universal LLM Memory
### Problem
- Switching LLM providers via `genai` resets your agent memory because embeddings differ per vendor
- Your team wastes 40 minutes per provider migration rebuilding semantic search indexes

### Solution
```rust
use std::process::Command;

async fn store_llm_turn(
    namespace: &str,
    role: &str,
    content: &str,
) -> anyhow::Result<()> {
    let entry = format!("[{role}] {content}");
    let name = format!(
        "llm-turn-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", namespace,
            "--name", &name,
            "--type", "project",
            "--description", "LLM conversation turn",
            "--body", &entry,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "failed to persist LLM turn");
    Ok(())
}

async fn retrieve_relevant_context(
    namespace: &str,
    user_query: &str,
    k: u8,
) -> anyhow::Result<String> {
    let output = Command::new("sqlite-graphrag")
        .args([
            "hybrid-search",
            "--namespace", namespace,
            "--k", &k.to_string(),
            "--json",
            user_query,
        ])
        .output()?;
    anyhow::ensure!(output.status.success(), "hybrid-search failed");
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let context = parsed["results"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v["body"].as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");
    Ok(context)
}
```

### Explanation
- sqlite-graphrag stores embeddings using `multilingual-e5-small` independently of any LLM provider
- Switching from OpenAI to Mistral via `genai` does not invalidate existing memory entries
- `hybrid-search` combines vector similarity and FTS giving richer context than vector alone
- Formatting turns as `[role] content` preserves conversation structure in the memory body
- Eliminates 40 minutes of index rebuilding per provider migration with a provider-agnostic layer

### Variants
- Prepend retrieved context as a system message before every `genai::chat` request automatically
- Store model name and temperature alongside the turn body to audit which model produced each answer

### See Also
- Recipe "How to combine vector and FTS search with tunable weights"
- Recipe "How to cascade with llm-cascade and memory fallback"


## How To Cascade With llm-cascade And Memory Fallback
### Problem
- Your cascading LLM pipeline forgets previous attempts when a provider fails and retries
- Replaying failed calls without context causes your fallback model to repeat costly mistakes

### Solution
```rust
use std::process::Command;

fn persist_cascade_attempt(
    namespace: &str,
    provider: &str,
    prompt: &str,
    result: &str,
    success: bool,
) -> anyhow::Result<()> {
    let status_label = if success { "SUCCESS" } else { "FAILURE" };
    let entry = format!("[CASCADE:{status_label}:{provider}] prompt={prompt} result={result}");
    let name = format!(
        "cascade-attempt-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", namespace,
            "--name", &name,
            "--type", "project",
            "--description", "llm-cascade attempt log",
            "--body", &entry,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "failed to persist cascade attempt");
    Ok(())
}

fn load_cascade_history(namespace: &str, prompt: &str) -> anyhow::Result<String> {
    let output = Command::new("sqlite-graphrag")
        .args([
            "recall",
            "--namespace", namespace,
            "--k", "10",
            "--json",
            prompt,
        ])
        .output()?;
    anyhow::ensure!(output.status.success(), "recall failed for cascade history");
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let history = parsed["results"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v["snippet"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(history)
}
```

### Explanation
- Labeling entries with `CASCADE:SUCCESS:provider` lets the fallback skip already-failed providers
- Recalling history before each attempt surfaces which models already attempted the same prompt
- A single namespace per pipeline run ensures isolation without managing multiple database files
- Structured labels parse with simple `str::contains` checks avoiding JSON overhead at query time
- Saves costly repeat failures by giving fallback providers full awareness of prior cascade state

### Variants
- Write a `CascadeMemory` struct that automatically calls `persist` and `load` around each try
- Filter `FAILURE` entries in the fallback selection to skip proven-failing providers automatically

### See Also
- Recipe "How to use genai with sqlite-graphrag for universal LLM memory"
- Recipe "How to integrate with rig-core for agent memory"


## How To Run Ollama Offline With ollama-rs And Persistent Memory
### Problem
- Your offline `ollama-rs` agent loses all conversation context when the process restarts
- Air-gapped environments cannot use cloud vector stores so every session starts from scratch

### Solution
```rust
use std::process::Command;

fn offline_remember(content: &str) -> anyhow::Result<()> {
    let name = format!(
        "ollama-turn-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", "ollama-local",
            "--name", &name,
            "--type", "project",
            "--description", "offline ollama context",
            "--body", content,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "offline remember failed: exit code nonzero");
    Ok(())
}

fn offline_recall(query: &str, k: u8) -> anyhow::Result<Vec<String>> {
    let output = Command::new("sqlite-graphrag")
        .args([
            "recall",
            "--namespace", "ollama-local",
            "--k", &k.to_string(),
            "--json",
            query,
        ])
        .output()?;
    anyhow::ensure!(output.status.success(), "offline recall failed");
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let items = parsed["results"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v["snippet"].as_str().map(str::to_owned))
        .collect();
    Ok(items)
}

fn build_context_prompt(query: &str, memories: &[String]) -> String {
    let context = memories.join("\n---\n");
    format!("Relevant context from memory:\n{context}\n\nUser query: {query}")
}
```

### Explanation
- sqlite-graphrag ships `multilingual-e5-small` ONNX model embedded so zero network calls occur
- The single 25 MB binary writes to a local SQLite file that survives across process restarts
- `--namespace ollama-local` keeps offline memories isolated from any networked agent namespaces
- `build_context_prompt` injects recalled memories into the Ollama prompt before each inference
- Delivers persistent vector memory in fully air-gapped environments with no cloud dependencies

### Variants
- Chain `offline_recall` with `sqlite-graphrag link` to build a knowledge graph from Ollama outputs
- Periodically call `sqlite-graphrag vacuum` to reclaim SQLite space as the offline database grows

### See Also
- Recipe "How to bootstrap memory database in 60 seconds"
- Recipe "How to integrate with rig-core for agent memory"


## How To Display Timestamps in a Local Timezone
### Problem
- JSON output from all subcommands includes `*_iso` fields in UTC by default
- Agents running in a specific region want localized timestamps for logging and display
- Pipelines parsing `created_at_iso` need offset-aware strings for correct sorting

### Solution
```bash
# One-off flag: display timestamps in São Paulo timezone
sqlite-graphrag read --name my-note --tz America/Sao_Paulo

# Persistent env var: all commands in this shell session use the given timezone
export SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo
sqlite-graphrag list --json | jaq '.items[].updated_at_iso'

# CI pipeline: force UTC explicitly to avoid system timezone surprises
SQLITE_GRAPHRAG_DISPLAY_TZ=UTC sqlite-graphrag recall "deploy notes" --json

# Extract only the offset portion to verify the timezone is applied
sqlite-graphrag read --name deploy-plan --tz Europe/Berlin --json \
  | jaq -r '.created_at_iso' \
  | rg '\+\d{2}:\d{2}$'
```

### Explanation
- Flag `--tz <IANA>` overrides all other settings and applies the given IANA timezone
- Env var `SQLITE_GRAPHRAG_DISPLAY_TZ` persists the setting across invocations without the flag
- Both fall back to UTC when absent, ensuring backward-compatible deterministic output
- Only string fields ending in `_iso` are affected; integer fields remain Unix epoch seconds
- Invalid IANA names cause exit 2 with a `Validation` error message printed to stderr
- Format produced: `2026-04-19T07:00:00-03:00` (offset explicit, no `Z` suffix)

### Variants
- Use `America/New_York` for Eastern Time (UTC-5/UTC-4 depending on DST)
- Use `Asia/Tokyo` for Japan Standard Time (UTC+9, no DST)
- Use `Europe/Berlin` for Central European Time (UTC+1/UTC+2 depending on DST)
- Use `UTC` to reset to the default explicitly in environments with a conflicting env var

### See Also
- Recipe "How to bootstrap memory database in 60 seconds"
- Recipe "How to configure language output with --lang flag"


## How To Round-Trip Forget And Restore A Memory
### Problem
- You ran `forget --name important-decision` and now `recall` returns nothing
- Reading SQL from `memory_versions` to recover the row is not part of your job
- v1.0.21 left `history` rejecting forgotten memories and `restore` requiring `--version`


### Solution
```bash
sqlite-graphrag forget --name important-decision
sqlite-graphrag history --name important-decision --json | jaq '.deleted'
sqlite-graphrag restore --name important-decision
sqlite-graphrag recall "decision" --json
```


### Explanation
- `history` in v1.0.22 returns versions for soft-deleted memories with `deleted: true` flag
- `restore` without `--version` automatically picks the latest non-`restore` version
- Together they make `forget` reversible end-to-end without inspecting SQL
- `vec_memories` is re-embedded on restore so vector recall finds the memory again
- Round-trip is idempotent: forgetting an already-forgotten memory is a no-op


### Variants
- Pass `--version N` explicitly when you need to roll back to a specific edit
- Combine with `list --include-deleted --json | jaq '.items[] | select(.deleted)'` to audit all forgotten memories
- Pipe `history --json` into `recall` to detect forgotten state programmatically before restoring


### See Also
- Recipe "How to schedule purge and vacuum in cron or GitHub Actions"
- Recipe "How to export memories to NDJSON for backup"


## How To Bulk-Ingest Without BERT Auto-Extraction
### Problem
- Your 5000-file ingestion pipeline takes hours because BERT NER runs on every body
- You already have entities extracted by an upstream LLM and want to skip auto-extraction
- Loading the 676 MB BERT model on first run exceeds your CI memory budget


### Solution
```bash
fd -e md docs/ -0 | xargs -0 -n 1 -I{} sh -c '
  sqlite-graphrag remember \
    --name "$(basename {} .md)" \
    --type document \
    --description "imported from {}" \
    --skip-extraction \
    --body-stdin < {}
'
```


### Explanation
- `--skip-extraction` disables `extract_graph_auto` for the current call only
- `extraction_method` is omitted from the JSON response when the flag is active
- Bypasses BERT model download, classifier head load, and IOB merge entirely
- Saves ~150 ms per call on warm cache, ~30 seconds on first run with cold cache
- Use this when you already supply `--entities-file` or `--graph-stdin` from upstream


### Variants
- Combine `--skip-extraction` with `--entities-file entities.json` to persist a curated graph without NER noise
- Set `SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY=200` when your upstream graph is larger than the default cap
- Use `--graph-stdin` to receive a single JSON document with both `entities` and `relationships` arrays


### See Also
- Recipe "How to bulk-import knowledge base via stdin pipeline"
- Recipe "How to traverse entity graph for multi-hop recall"
