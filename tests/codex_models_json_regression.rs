//! GAP-E2E-010 (v1.0.89): regression tests for `codex-models --json` and
//! `pending --db`.
//!
//! Background:
//! - Before the fix, `sqlite-graphrag codex-models --json` failed with
//!   `unexpected argument '--json'` because the variant was declared as a
//!   bare unit variant (`CodexModels`). Agent pipelines that append `--json`
//!   to every invocation broke.
//! - Before the fix, `sqlite-graphrag pending list --db <PATH>` failed
//!   with `unexpected argument '--db'` because `PendingListArgs` and
//!   `PendingShowArgs` did not declare the field. Operators running two
//!   databases side-by-side could not target a specific file.
//!
//! These tests confirm both surfaces accept their flags without error and
//! that the dispatch path is wired correctly.

use clap::Parser;
use sqlite_graphrag::cli::{Cli, Commands};

// ---------------------------------------------------------------------------
// GAP-E2E-010 (a): codex-models --json is accepted as a no-op
// ---------------------------------------------------------------------------

#[test]
fn codex_models_json_flag_accepted_as_noop() {
    // Before the fix: try_parse_from returned Err("unexpected argument '--json'").
    // After the fix: parse succeeds and the json flag is captured as true.
    let cli = Cli::try_parse_from(["sqlite-graphrag", "codex-models", "--json"])
        .expect("codex-models --json deve ser aceito como no-op");
    match cli.command {
        Some(Commands::CodexModels(args)) => {
            assert!(
                args.json,
                "CodexModelsArgs.json deve ser true quando --json é passado"
            );
        }
        other => panic!("esperava CodexModels, recebi: {other:?}"),
    }
}

#[test]
fn codex_models_without_json_still_parses() {
    // Back-compat: bare `codex-models` (without --json) must still parse.
    let cli = Cli::try_parse_from(["sqlite-graphrag", "codex-models"])
        .expect("codex-models sem --json deve continuar parseando");
    match cli.command {
        Some(Commands::CodexModels(args)) => {
            assert!(
                !args.json,
                "CodexModelsArgs.json deve ser false quando --json é omitido"
            );
        }
        other => panic!("esperava CodexModels, recebi: {other:?}"),
    }
}

#[test]
fn codex_models_json_flag_does_not_appear_in_help() {
    // `hide = true` garante que --json não polui a saída de `codex-models --help`
    // para operadores humanos. Agentes descobrem o no-op por docs, não por help.
    // FIX-1 (v1.0.89): clap 4.6 requires  (not
    // ) because  needs .
    //  returns  (shared borrow), which
    // cannot be mutated in place.
    let mut cmd = <Cli as clap::CommandFactory>::command();
    let help = cmd
        .find_subcommand_mut("codex-models")
        .expect("codex-models subcommand deve existir")
        .render_help();
    assert!(
        !help.to_string().contains("--json"),
        "--json deve estar escondido (hide = true) na help do codex-models"
    );
}

// ---------------------------------------------------------------------------
// GAP-E2E-010b: pending list --db and pending show --db are accepted
// ---------------------------------------------------------------------------

#[test]
fn pending_list_db_flag_is_accepted() {
    let cli = Cli::try_parse_from([
        "sqlite-graphrag",
        "pending",
        "list",
        "--db",
        "/tmp/some-graphrag.sqlite",
    ])
    .expect("pending list --db deve ser aceito");
    match cli.command {
        Some(Commands::Pending(p)) => match p.cmd {
            sqlite_graphrag::commands::pending::PendingCmd::List(args) => {
                assert_eq!(
                    args.db.as_deref(),
                    Some("/tmp/some-graphrag.sqlite"),
                    "PendingListArgs.db deve capturar o valor passado"
                );
            }
            other => panic!("esperava PendingCmd::List, recebi: {other:?}"),
        },
        other => panic!("esperava Pending, recebi: {other:?}"),
    }
}

#[test]
fn pending_show_db_flag_is_accepted() {
    let cli = Cli::try_parse_from([
        "sqlite-graphrag",
        "pending",
        "show",
        "42",
        "--db",
        "/tmp/another.sqlite",
    ])
    .expect("pending show --db deve ser aceito");
    match cli.command {
        Some(Commands::Pending(p)) => match p.cmd {
            sqlite_graphrag::commands::pending::PendingCmd::Show(args) => {
                assert_eq!(args.pending_id, 42);
                assert_eq!(
                    args.db.as_deref(),
                    Some("/tmp/another.sqlite"),
                    "PendingShowArgs.db deve capturar o valor passado"
                );
            }
            other => panic!("esperava PendingCmd::Show, recebi: {other:?}"),
        },
        other => panic!("esperava Pending, recebi: {other:?}"),
    }
}

#[test]
fn pending_list_without_db_uses_default_path() {
    // Back-compat: omitir --db deve produzir None (AppPaths::resolve decide).
    let cli = Cli::try_parse_from(["sqlite-graphrag", "pending", "list"])
        .expect("pending list sem --db deve continuar parseando");
    match cli.command {
        Some(Commands::Pending(p)) => match p.cmd {
            sqlite_graphrag::commands::pending::PendingCmd::List(args) => {
                assert!(
                    args.db.is_none(),
                    "PendingListArgs.db deve ser None quando --db é omitido"
                );
            }
            other => panic!("esperava PendingCmd::List, recebi: {other:?}"),
        },
        other => panic!("esperava Pending, recebi: {other:?}"),
    }
}
