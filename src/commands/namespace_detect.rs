//! Handler for the `namespace-detect` CLI subcommand.

use crate::errors::AppError;
use crate::namespace;
use crate::output;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Resolve namespace using current environment and cwd\n  \
    sqlite-graphrag namespace-detect\n\n  \
    # Override with an explicit namespace flag\n  \
    sqlite-graphrag namespace-detect --namespace my-project\n\n  \
    # Resolve via SQLITE_GRAPHRAG_NAMESPACE env var\n  \
    SQLITE_GRAPHRAG_NAMESPACE=ci-runner sqlite-graphrag namespace-detect")]
pub struct NamespaceDetectArgs {
    #[arg(long)]
    pub namespace: Option<String>,
    /// Explicit database path. Accepted as a no-op to preserve the global contract.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Explicit JSON flag. Accepted as a no-op because output is already JSON by default.
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Serialize)]
struct NamespaceDetectResponse {
    namespace: String,
    source: namespace::NamespaceSource,
    cwd: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: NamespaceDetectArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let _ = args.db;
    let _ = args.json; // --json is a no-op because output is already JSON by default
    let resolution = namespace::detect_namespace(args.namespace.as_deref())?;
    output::emit_json(&NamespaceDetectResponse {
        namespace: resolution.namespace,
        source: resolution.source,
        cwd: resolution.cwd,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::NamespaceSource;
    use clap::Parser;
    use serial_test::serial;

    #[test]
    #[serial]
    fn namespace_detect_default_returns_global_via_detect() {
        // Garante que sem flag e sem env, detect_namespace retorna "global"
        std::env::remove_var("SQLITE_GRAPHRAG_NAMESPACE");
        let resolution = namespace::detect_namespace(None).unwrap();
        assert_eq!(resolution.namespace, "global");
        assert_eq!(resolution.source, NamespaceSource::Default);
    }

    #[test]
    #[serial]
    fn namespace_detect_explicit_flag_overrides_env() {
        std::env::set_var("SQLITE_GRAPHRAG_NAMESPACE", "env-namespace");
        let resolution = namespace::detect_namespace(Some("flag-namespace")).unwrap();
        assert_eq!(resolution.namespace, "flag-namespace");
        assert_eq!(resolution.source, NamespaceSource::ExplicitFlag);
        std::env::remove_var("SQLITE_GRAPHRAG_NAMESPACE");
    }

    #[test]
    #[serial]
    fn namespace_detect_env_var_used_when_no_flag() {
        std::env::remove_var("SQLITE_GRAPHRAG_NAMESPACE");
        std::env::set_var("SQLITE_GRAPHRAG_NAMESPACE", "namespace-de-env");
        let resolution = namespace::detect_namespace(None).unwrap();
        assert_eq!(resolution.namespace, "namespace-de-env");
        assert_eq!(resolution.source, NamespaceSource::Environment);
        std::env::remove_var("SQLITE_GRAPHRAG_NAMESPACE");
    }

    #[test]
    fn namespace_detect_response_serializes_all_fields() {
        let resp = NamespaceDetectResponse {
            namespace: "meu-projeto".to_string(),
            source: NamespaceSource::ExplicitFlag,
            cwd: "/home/usuario/projeto".to_string(),
            elapsed_ms: 3,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["namespace"], "meu-projeto");
        assert_eq!(json["source"], "explicit_flag");
        assert!(json["cwd"].is_string());
        assert_eq!(json["elapsed_ms"], 3);
    }

    #[test]
    fn namespace_source_serializes_in_snake_case() {
        let cases = vec![
            (NamespaceSource::ExplicitFlag, "explicit_flag"),
            (NamespaceSource::Environment, "environment"),
            (NamespaceSource::Default, "default"),
        ];
        for (source, expected) in cases {
            let json = serde_json::to_value(source).unwrap();
            assert_eq!(
                json, expected,
                "NamespaceSource::{source:?} must serialize as \"{expected}\""
            );
        }
    }

    #[test]
    fn namespace_detect_accepts_db_as_noop() {
        let cli = crate::cli::Cli::try_parse_from([
            "sqlite-graphrag",
            "namespace-detect",
            "--db",
            "/tmp/graphrag.sqlite",
        ])
        .expect("namespace-detect must accept --db as a no-op");

        match cli.command {
            crate::cli::Commands::NamespaceDetect(args) => {
                assert_eq!(args.db.as_deref(), Some("/tmp/graphrag.sqlite"));
            }
            _ => unreachable!("unexpected command parsed"),
        }
    }
}
