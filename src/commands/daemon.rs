use crate::constants::DAEMON_IDLE_SHUTDOWN_SECS;
use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;

#[derive(clap::Args)]
pub struct DaemonArgs {
    /// Idle timeout in seconds before the daemon auto-shuts down to release the embedding model.
    /// Default 600s; raise for long-running batch ingestion to avoid cold-start overhead.
    #[arg(long, default_value_t = DAEMON_IDLE_SHUTDOWN_SECS)]
    pub idle_shutdown_secs: u64,
    /// Send a health-check ping to a running daemon and exit. Returns NotFound (exit 4) if no daemon.
    #[arg(long)]
    pub ping: bool,
    /// Request graceful shutdown of a running daemon. Returns NotFound (exit 4) if no daemon.
    #[arg(long)]
    pub stop: bool,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

pub fn run(args: DaemonArgs) -> Result<(), AppError> {
    let _ = args.json;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    paths.ensure_dirs()?;

    if args.ping {
        let response = crate::daemon::try_ping(&paths.models)?
            .ok_or_else(|| AppError::NotFound("daemon not running".to_string()))?;
        output::emit_json(&response)?;
        return Ok(());
    }

    if args.stop {
        let response = crate::daemon::try_shutdown(&paths.models)?
            .ok_or_else(|| AppError::NotFound("daemon not running".to_string()))?;
        output::emit_json(&response)?;
        return Ok(());
    }

    crate::daemon::run(&paths.models, args.idle_shutdown_secs)
}
