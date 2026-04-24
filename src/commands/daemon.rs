use crate::constants::DAEMON_IDLE_SHUTDOWN_SECS;
use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;

#[derive(clap::Args)]
pub struct DaemonArgs {
    #[arg(long, default_value_t = DAEMON_IDLE_SHUTDOWN_SECS)]
    pub idle_shutdown_secs: u64,
    #[arg(long)]
    pub ping: bool,
    #[arg(long)]
    pub stop: bool,
}

pub fn run(args: DaemonArgs) -> Result<(), AppError> {
    let paths = AppPaths::resolve(None)?;
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
