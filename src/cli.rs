use crate::commands::*;
use crate::i18n::{current, Language};
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
#[command(name = "sqlite-graphrag")]
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

    /// Idioma das mensagens humanas (stderr). Aceita `en` ou `pt`.
    ///
    /// Sem a flag, detecta via env `SQLITE_GRAPHRAG_LANG` e depois `LC_ALL`/`LANG`.
    /// JSON de stdout é determinístico e idêntico entre idiomas — apenas
    /// strings destinadas a humanos são afetadas.
    #[arg(long, global = true, value_enum, value_name = "LANG")]
    pub lang: Option<crate::i18n::Language>,

    /// Fuso horário para campos `*_iso` no JSON de saída (ex: `America/Sao_Paulo`).
    ///
    /// Aceita qualquer nome IANA da IANA Time Zone Database. Sem a flag, usa
    /// `SQLITE_GRAPHRAG_DISPLAY_TZ`; se ausente, usa UTC. Não afeta campos epoch inteiros.
    #[arg(long, global = true, value_name = "IANA")]
    pub tz: Option<chrono_tz::Tz>,

    #[command(subcommand)]
    pub command: Commands,
}

#[cfg(test)]
mod testes_formato_json_only {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn restore_aceita_apenas_format_json() {
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
    fn hybrid_search_aceita_apenas_format_json() {
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
    /// Valida flags de concorrência e retorna erro descritivo localizado se inválidas.
    ///
    /// Requer que `crate::i18n::init()` já tenha sido chamado (ocorre antes desta função
    /// no fluxo de `main`). Em inglês emite mensagens EN; em português emite PT.
    pub fn validate_flags(&self) -> Result<(), String> {
        if let Some(n) = self.max_concurrency {
            if n == 0 {
                return Err(match current() {
                    Language::English => "--max-concurrency must be >= 1".to_string(),
                    Language::Portugues => "--max-concurrency deve ser >= 1".to_string(),
                });
            }
            let teto = max_concurrency_ceiling();
            if n > teto {
                return Err(match current() {
                    Language::English => format!(
                        "--max-concurrency {n} exceeds the ceiling of {teto} (2×nCPUs) on this system"
                    ),
                    Language::Portugues => format!(
                        "--max-concurrency {n} excede o teto de {teto} (2×nCPUs) neste sistema"
                    ),
                });
            }
        }
        Ok(())
    }
}

impl Commands {
    /// Retorna true para subcomandos que carregam o modelo ONNX localmente.
    pub fn is_embedding_heavy(&self) -> bool {
        matches!(
            self,
            Self::Init(_) | Self::Remember(_) | Self::Recall(_) | Self::HybridSearch(_)
        )
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
}

#[cfg(test)]
mod testes_concorrencia_pesada {
    use super::*;

    #[test]
    fn command_heavy_detecta_init_e_embeddings() {
        let init = Cli::try_parse_from(["sqlite-graphrag", "init"]).expect("parse init");
        assert!(init.command.is_embedding_heavy());

        let remember = Cli::try_parse_from([
            "sqlite-graphrag",
            "remember",
            "--name",
            "memoria-teste",
            "--type",
            "project",
            "--description",
            "desc",
        ])
        .expect("parse remember");
        assert!(remember.command.is_embedding_heavy());

        let recall =
            Cli::try_parse_from(["sqlite-graphrag", "recall", "consulta"]).expect("parse recall");
        assert!(recall.command.is_embedding_heavy());

        let hybrid = Cli::try_parse_from(["sqlite-graphrag", "hybrid-search", "consulta"])
            .expect("parse hybrid");
        assert!(hybrid.command.is_embedding_heavy());
    }

    #[test]
    fn command_light_nao_marca_stats() {
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
        }
    }
}
