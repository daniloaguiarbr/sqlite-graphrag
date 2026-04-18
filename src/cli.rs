use crate::commands::*;
use clap::{Parser, Subcommand};

/// Retorna o número máximo de invocações simultâneas permitidas pela heurística de CPU.
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
#[command(name = "neurographrag")]
#[command(version)]
#[command(about = "Local GraphRAG memory for LLMs in a single SQLite file")]
#[command(arg_required_else_help = true)]
pub struct Cli {
    /// Número máximo de invocações CLI simultâneas permitidas (default: 4).
    ///
    /// Limita o semáforo de contagem de slots de concorrência. O valor é restrito
    /// ao intervalo [1, 2×nCPUs]. Valores acima do teto são rejeitados com exit 2.
    #[arg(long, global = true, value_name = "N")]
    pub max_concurrency: Option<usize>,

    /// Aguardar até SECONDS por um slot livre antes de desistir (exit 75).
    ///
    /// Útil em pipelines de agentes que fazem retry: a instância faz polling a
    /// cada 500 ms até o timeout ou um slot abrir. Default: 300s (5 minutos).
    #[arg(long, global = true, value_name = "SECONDS")]
    pub wait_lock: Option<u64>,

    /// Pular a verificação de memória disponível antes de carregar o modelo.
    ///
    /// Uso exclusivo em testes automatizados onde a alocação real não ocorre.
    #[arg(long, global = true, hide = true, default_value_t = false)]
    pub skip_memory_guard: bool,

    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    /// Valida flags de concorrência e retorna erro descritivo se inválidas.
    pub fn validate_flags(&self) -> Result<(), String> {
        if let Some(n) = self.max_concurrency {
            if n == 0 {
                return Err("--max-concurrency deve ser >= 1".to_string());
            }
            let teto = max_concurrency_ceiling();
            if n > teto {
                return Err(format!(
                    "--max-concurrency {n} excede o teto de {teto} (2×nCPUs) neste sistema"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize database and download embedding model
    Init(init::InitArgs),
    /// Save a memory with optional entity graph
    Remember(remember::RememberArgs),
    /// Search memories semantically
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
        }
    }
}
