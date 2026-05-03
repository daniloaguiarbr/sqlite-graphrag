//! CLI argument structs and command surface (clap-based).
//!
//! Defines `Cli` and all subcommand enums; contains no business logic.

use crate::commands::*;
use crate::i18n::{current, Language};
use clap::{Parser, Subcommand};

/// Common daemon-control options shared across embedding-heavy subcommands.
#[derive(clap::Args, Debug, Clone)]
pub struct DaemonOpts {
    /// Allow the CLI to spawn a background daemon if none is running.
    ///
    /// Default `true`. Pass `--autostart-daemon=false` to disable.
    /// Env var `SQLITE_GRAPHRAG_DAEMON_DISABLE_AUTOSTART=1` is honoured only when this flag is unset.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub autostart_daemon: bool,
}

/// Returns the maximum simultaneous invocations allowed by the CPU heuristic.
fn max_concurrency_ceiling() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get() * 2)
        .unwrap_or(8)
}

#[derive(Copy, Clone, Debug, clap::ValueEnum)]
pub enum RelationKind {
    AppliesTo,
    Uses,
    DependsOn,
    Causes,
    Fixes,
    Contradicts,
    Supports,
    Follows,
    Related,
    Mentions,
    Replaces,
    TrackedIn,
}

impl RelationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AppliesTo => "applies_to",
            Self::Uses => "uses",
            Self::DependsOn => "depends_on",
            Self::Causes => "causes",
            Self::Fixes => "fixes",
            Self::Contradicts => "contradicts",
            Self::Supports => "supports",
            Self::Follows => "follows",
            Self::Related => "related",
            Self::Mentions => "mentions",
            Self::Replaces => "replaces",
            Self::TrackedIn => "tracked_in",
        }
    }
}

#[derive(Copy, Clone, Debug, clap::ValueEnum)]
pub enum GraphExportFormat {
    Json,
    Dot,
    Mermaid,
}

#[derive(Parser)]
#[command(name = "sqlite-graphrag")]
#[command(version)]
#[command(about = "Local GraphRAG memory for LLMs in a single SQLite file")]
#[command(arg_required_else_help = true)]
pub struct Cli {
    /// Maximum number of simultaneous CLI invocations allowed (default: 4).
    ///
    /// Caps the counting semaphore used for CLI concurrency slots. The value must
    /// stay within [1, 2×nCPUs]. Values above the ceiling are rejected with exit 2.
    #[arg(long, global = true, value_name = "N")]
    pub max_concurrency: Option<usize>,

    /// Wait up to SECONDS for a free concurrency slot before giving up (exit 75).
    ///
    /// Useful in retrying agent pipelines: the process polls every 500 ms until a
    /// slot opens or the timeout expires. Default: 300s (5 minutes).
    #[arg(long, global = true, value_name = "SECONDS")]
    pub wait_lock: Option<u64>,

    /// Skip the available-memory check before loading the model.
    ///
    /// Exclusive use in automated tests where real allocation does not occur.
    #[arg(long, global = true, hide = true, default_value_t = false)]
    pub skip_memory_guard: bool,

    /// Language for human-facing stderr messages. Accepts `en` or `pt`.
    ///
    /// Without the flag, detection falls back to `SQLITE_GRAPHRAG_LANG` and then
    /// `LC_ALL`/`LANG`. JSON stdout stays deterministic and identical across
    /// languages; only human-facing strings are affected.
    #[arg(long, global = true, value_enum, value_name = "LANG")]
    pub lang: Option<crate::i18n::Language>,

    /// Time zone for `*_iso` fields in JSON output (for example `America/Sao_Paulo`).
    ///
    /// Accepts any IANA time zone name. Without the flag, it falls back to
    /// `SQLITE_GRAPHRAG_DISPLAY_TZ`; if unset, UTC is used. Integer epoch fields
    /// are not affected.
    #[arg(long, global = true, value_name = "IANA")]
    pub tz: Option<chrono_tz::Tz>,

    /// Increase logging verbosity (-v=info, -vv=debug, -vvv=trace).
    ///
    /// Overrides `SQLITE_GRAPHRAG_LOG_LEVEL` env var when present. Logs are emitted
    /// to stderr; JSON stdout is unaffected.
    #[arg(short = 'v', long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Commands,
}

#[cfg(test)]
mod json_only_format_tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn restore_accepts_only_format_json() {
        assert!(Cli::try_parse_from([
            "sqlite-graphrag",
            "restore",
            "--name",
            "mem",
            "--version",
            "1",
            "--format",
            "json",
        ])
        .is_ok());

        assert!(Cli::try_parse_from([
            "sqlite-graphrag",
            "restore",
            "--name",
            "mem",
            "--version",
            "1",
            "--format",
            "text",
        ])
        .is_err());
    }

    #[test]
    fn hybrid_search_accepts_only_format_json() {
        assert!(Cli::try_parse_from([
            "sqlite-graphrag",
            "hybrid-search",
            "query",
            "--format",
            "json",
        ])
        .is_ok());

        assert!(Cli::try_parse_from([
            "sqlite-graphrag",
            "hybrid-search",
            "query",
            "--format",
            "markdown",
        ])
        .is_err());
    }

    #[test]
    fn remember_recall_rename_vacuum_json_only() {
        assert!(Cli::try_parse_from([
            "sqlite-graphrag",
            "remember",
            "--name",
            "mem",
            "--type",
            "project",
            "--description",
            "desc",
            "--format",
            "json",
        ])
        .is_ok());
        assert!(Cli::try_parse_from([
            "sqlite-graphrag",
            "remember",
            "--name",
            "mem",
            "--type",
            "project",
            "--description",
            "desc",
            "--format",
            "text",
        ])
        .is_err());

        assert!(
            Cli::try_parse_from(["sqlite-graphrag", "recall", "query", "--format", "json",])
                .is_ok()
        );
        assert!(
            Cli::try_parse_from(["sqlite-graphrag", "recall", "query", "--format", "text",])
                .is_err()
        );

        assert!(Cli::try_parse_from([
            "sqlite-graphrag",
            "rename",
            "--name",
            "old",
            "--new-name",
            "new",
            "--format",
            "json",
        ])
        .is_ok());
        assert!(Cli::try_parse_from([
            "sqlite-graphrag",
            "rename",
            "--name",
            "old",
            "--new-name",
            "new",
            "--format",
            "markdown",
        ])
        .is_err());

        assert!(Cli::try_parse_from(["sqlite-graphrag", "vacuum", "--format", "json",]).is_ok());
        assert!(Cli::try_parse_from(["sqlite-graphrag", "vacuum", "--format", "text",]).is_err());
    }
}

impl Cli {
    /// Validates concurrency flags and returns a localised descriptive error if invalid.
    ///
    /// Requires that `crate::i18n::init()` has already been called (happens before this
    /// function in the `main` flow). In English it emits EN messages; in Portuguese it emits PT.
    pub fn validate_flags(&self) -> Result<(), String> {
        if let Some(n) = self.max_concurrency {
            if n == 0 {
                return Err(match current() {
                    Language::English => "--max-concurrency must be >= 1".to_string(),
                    Language::Portuguese => "--max-concurrency deve ser >= 1".to_string(),
                });
            }
            let teto = max_concurrency_ceiling();
            if n > teto {
                return Err(match current() {
                    Language::English => format!(
                        "--max-concurrency {n} exceeds the ceiling of {teto} (2×nCPUs) on this system"
                    ),
                    Language::Portuguese => format!(
                        "--max-concurrency {n} excede o teto de {teto} (2×nCPUs) neste sistema"
                    ),
                });
            }
        }
        Ok(())
    }
}

impl Commands {
    /// Returns true for subcommands that load the ONNX model locally.
    pub fn is_embedding_heavy(&self) -> bool {
        matches!(
            self,
            Self::Init(_) | Self::Remember(_) | Self::Recall(_) | Self::HybridSearch(_)
        )
    }

    pub fn uses_cli_slot(&self) -> bool {
        !matches!(self, Self::Daemon(_))
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize database and download embedding model
    #[command(after_long_help = "EXAMPLES:\n  \
        # Initialize in current directory (default behavior)\n  \
        sqlite-graphrag init\n\n  \
        # Initialize at a specific path\n  \
        sqlite-graphrag init --db /path/to/graphrag.sqlite\n\n  \
        # Initialize using SQLITE_GRAPHRAG_HOME env var\n  \
        SQLITE_GRAPHRAG_HOME=/data sqlite-graphrag init\n\n\
        NOTES:\n  \
        - `init` is OPTIONAL: any subsequent CRUD command auto-initializes graphrag.sqlite if missing.\n  \
        - As a side effect, `init` warms a smoke-test embedding which auto-spawns the persistent daemon (~600s idle timeout).")]
    Init(init::InitArgs),
    /// Run or control the persistent embedding daemon
    Daemon(daemon::DaemonArgs),
    /// Save a memory with optional entity graph
    #[command(after_long_help = "EXAMPLES:\n  \
        # Inline body\n  \
        sqlite-graphrag remember --name onboarding --type user --description \"intro\" --body \"hello\"\n\n  \
        # Body from file\n  \
        sqlite-graphrag remember --name doc1 --type document --description \"...\" --body-file ./README.md\n\n  \
        # Body from stdin (pipe)\n  \
        cat README.md | sqlite-graphrag remember --name doc1 --type document --description \"...\" --body-stdin\n\n  \
        # Skip BERT entity extraction (faster)\n  \
        sqlite-graphrag remember --name quick --type note --description \"...\" --body \"...\" --skip-extraction")]
    Remember(remember::RememberArgs),
    /// Bulk-ingest every file under a directory as separate memories (NDJSON output)
    Ingest(ingest::IngestArgs),
    /// Search memories semantically
    #[command(after_long_help = "EXAMPLES:\n  \
        # Top 10 semantic matches (default)\n  \
        sqlite-graphrag recall \"agent memory\"\n\n  \
        # Top 3 only\n  \
        sqlite-graphrag recall \"agent memory\" -k 3\n\n  \
        # Search across all namespaces\n  \
        sqlite-graphrag recall \"agent memory\" --all-namespaces\n\n  \
        # Disable graph traversal (vector-only)\n  \
        sqlite-graphrag recall \"agent memory\" --no-graph")]
    Recall(recall::RecallArgs),
    /// Read a memory by exact name
    Read(read::ReadArgs),
    /// List memories with filters
    List(list::ListArgs),
    /// Soft-delete a memory
    Forget(forget::ForgetArgs),
    /// Permanently delete soft-deleted memories
    Purge(purge::PurgeArgs),
    /// Rename a memory preserving history
    Rename(rename::RenameArgs),
    /// Edit a memory's body or description
    Edit(edit::EditArgs),
    /// List all versions of a memory
    History(history::HistoryArgs),
    /// Restore a memory to a previous version
    Restore(restore::RestoreArgs),
    /// Search using hybrid vector + full-text search
    #[command(after_long_help = "EXAMPLES:\n  \
        # Hybrid search combining KNN + FTS5 BM25 with RRF\n  \
        sqlite-graphrag hybrid-search \"agent memory architecture\"\n\n  \
        # Custom weights for vector vs full-text components\n  \
        sqlite-graphrag hybrid-search \"agent\" --weight-vec 0.7 --weight-fts 0.3")]
    HybridSearch(hybrid_search::HybridSearchArgs),
    /// Show database health
    Health(health::HealthArgs),
    /// Apply pending schema migrations
    Migrate(migrate::MigrateArgs),
    /// Resolve namespace precedence for the current invocation
    NamespaceDetect(namespace_detect::NamespaceDetectArgs),
    /// Run PRAGMA optimize on the database
    Optimize(optimize::OptimizeArgs),
    /// Show database statistics
    Stats(stats::StatsArgs),
    /// Create a checkpointed copy safe for file sync
    SyncSafeCopy(sync_safe_copy::SyncSafeCopyArgs),
    /// Run VACUUM after checkpointing the WAL
    Vacuum(vacuum::VacuumArgs),
    /// Create an explicit relationship between two entities
    Link(link::LinkArgs),
    /// Remove a specific relationship between two entities
    Unlink(unlink::UnlinkArgs),
    /// List memories connected via the entity graph
    Related(related::RelatedArgs),
    /// Export a graph snapshot in json, dot or mermaid
    Graph(graph_export::GraphArgs),
    /// Remove entities that have no memories and no relationships
    CleanupOrphans(cleanup_orphans::CleanupOrphansArgs),
    /// Manage cached resources (embedding models, etc.)
    Cache(cache::CacheArgs),
    #[command(name = "__debug_schema", hide = true)]
    DebugSchema(debug_schema::DebugSchemaArgs),
}

#[derive(Copy, Clone, Debug, clap::ValueEnum)]
pub enum MemoryType {
    User,
    Feedback,
    Project,
    Reference,
    Decision,
    Incident,
    Skill,
    Document,
    Note,
}

#[cfg(test)]
mod heavy_concurrency_tests {
    use super::*;

    #[test]
    fn command_heavy_detects_init_and_embeddings() {
        let init = Cli::try_parse_from(["sqlite-graphrag", "init"]).expect("parse init");
        assert!(init.command.is_embedding_heavy());

        let remember = Cli::try_parse_from([
            "sqlite-graphrag",
            "remember",
            "--name",
            "test-memory",
            "--type",
            "project",
            "--description",
            "desc",
        ])
        .expect("parse remember");
        assert!(remember.command.is_embedding_heavy());

        let recall =
            Cli::try_parse_from(["sqlite-graphrag", "recall", "query"]).expect("parse recall");
        assert!(recall.command.is_embedding_heavy());

        let hybrid = Cli::try_parse_from(["sqlite-graphrag", "hybrid-search", "query"])
            .expect("parse hybrid");
        assert!(hybrid.command.is_embedding_heavy());
    }

    #[test]
    fn command_light_does_not_mark_stats() {
        let stats = Cli::try_parse_from(["sqlite-graphrag", "stats"]).expect("parse stats");
        assert!(!stats.command.is_embedding_heavy());
    }
}

impl MemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Feedback => "feedback",
            Self::Project => "project",
            Self::Reference => "reference",
            Self::Decision => "decision",
            Self::Incident => "incident",
            Self::Skill => "skill",
            Self::Document => "document",
            Self::Note => "note",
        }
    }
}
