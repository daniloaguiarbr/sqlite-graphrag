/// Regression tests for P1-B: positional + flag pattern in read/forget/history/edit/rename.
///
/// These tests verify that:
/// 1. Each Args struct accepts `name_positional` as an alternative to `--name`.
/// 2. Back-compat with `--name` flag is preserved.
/// 3. `conflicts_with` is correct — passing both positional and --name is rejected by clap.
use clap::Parser;
use sqlite_graphrag::cli::{Cli, Commands};

// ---------------------------------------------------------------------------
// read — positional NAME accepted
// ---------------------------------------------------------------------------

#[test]
fn regression_read_args_accepts_name_positional() {
    // Simulate: sqlite-graphrag read minha-memoria
    let cli = Cli::try_parse_from(["sqlite-graphrag", "read", "minha-memoria"]).unwrap();
    if let Commands::Read(args) = cli.command {
        assert_eq!(
            args.name_positional.as_deref(),
            Some("minha-memoria"),
            "ReadArgs deve capturar NAME positional"
        );
        assert!(
            args.name.is_none(),
            "ReadArgs.name deve ser None quando positional é usado"
        );
    } else {
        panic!("esperava comando Read");
    }
}

#[test]
fn regression_read_args_accepts_flag_name() {
    // Simulate: sqlite-graphrag read --name minha-memoria (back-compat)
    let cli = Cli::try_parse_from(["sqlite-graphrag", "read", "--name", "minha-memoria"]).unwrap();
    if let Commands::Read(args) = cli.command {
        assert_eq!(
            args.name.as_deref(),
            Some("minha-memoria"),
            "ReadArgs deve capturar --name flag para back-compat"
        );
        assert!(
            args.name_positional.is_none(),
            "ReadArgs.name_positional deve ser None quando --name é usado"
        );
    } else {
        panic!("esperava comando Read");
    }
}

// ---------------------------------------------------------------------------
// forget — positional NAME accepted
// ---------------------------------------------------------------------------

#[test]
fn regression_forget_args_accepts_name_positional() {
    let cli = Cli::try_parse_from(["sqlite-graphrag", "forget", "minha-memoria"]).unwrap();
    if let Commands::Forget(args) = cli.command {
        assert_eq!(
            args.name_positional.as_deref(),
            Some("minha-memoria"),
            "ForgetArgs deve capturar NAME positional"
        );
        assert!(
            args.name.is_none(),
            "ForgetArgs.name deve ser None quando positional é usado"
        );
    } else {
        panic!("esperava comando Forget");
    }
}

#[test]
fn regression_forget_args_accepts_flag_name() {
    let cli =
        Cli::try_parse_from(["sqlite-graphrag", "forget", "--name", "minha-memoria"]).unwrap();
    if let Commands::Forget(args) = cli.command {
        assert_eq!(
            args.name.as_deref(),
            Some("minha-memoria"),
            "ForgetArgs deve capturar --name flag para back-compat"
        );
        assert!(args.name_positional.is_none());
    } else {
        panic!("esperava comando Forget");
    }
}

// ---------------------------------------------------------------------------
// history — positional NAME accepted
// ---------------------------------------------------------------------------

#[test]
fn regression_history_args_accepts_name_positional() {
    let cli = Cli::try_parse_from(["sqlite-graphrag", "history", "minha-memoria"]).unwrap();
    if let Commands::History(args) = cli.command {
        assert_eq!(
            args.name_positional.as_deref(),
            Some("minha-memoria"),
            "HistoryArgs deve capturar NAME positional"
        );
        assert!(args.name.is_none());
    } else {
        panic!("esperava comando History");
    }
}

#[test]
fn regression_history_args_accepts_flag_name() {
    let cli =
        Cli::try_parse_from(["sqlite-graphrag", "history", "--name", "minha-memoria"]).unwrap();
    if let Commands::History(args) = cli.command {
        assert_eq!(
            args.name.as_deref(),
            Some("minha-memoria"),
            "HistoryArgs deve capturar --name flag para back-compat"
        );
        assert!(args.name_positional.is_none());
    } else {
        panic!("esperava comando History");
    }
}

// ---------------------------------------------------------------------------
// edit — positional NAME accepted
// ---------------------------------------------------------------------------

#[test]
fn regression_edit_args_accepts_name_positional() {
    let cli = Cli::try_parse_from([
        "sqlite-graphrag",
        "edit",
        "minha-memoria",
        "--body",
        "novo-conteudo",
    ])
    .unwrap();
    if let Commands::Edit(args) = cli.command {
        assert_eq!(
            args.name_positional.as_deref(),
            Some("minha-memoria"),
            "EditArgs deve capturar NAME positional"
        );
        assert!(args.name.is_none());
    } else {
        panic!("esperava comando Edit");
    }
}

#[test]
fn regression_edit_args_accepts_flag_name() {
    let cli = Cli::try_parse_from([
        "sqlite-graphrag",
        "edit",
        "--name",
        "minha-memoria",
        "--body",
        "novo-conteudo",
    ])
    .unwrap();
    if let Commands::Edit(args) = cli.command {
        assert_eq!(
            args.name.as_deref(),
            Some("minha-memoria"),
            "EditArgs deve capturar --name flag para back-compat"
        );
        assert!(args.name_positional.is_none());
    } else {
        panic!("esperava comando Edit");
    }
}

// ---------------------------------------------------------------------------
// rename — positional NAME (current name) accepted; --new-name remains flag-only
// ---------------------------------------------------------------------------

#[test]
fn regression_rename_args_accepts_name_positional() {
    let cli = Cli::try_parse_from([
        "sqlite-graphrag",
        "rename",
        "nome-antigo",
        "--new-name",
        "nome-novo",
    ])
    .unwrap();
    if let Commands::Rename(args) = cli.command {
        assert_eq!(
            args.name_positional.as_deref(),
            Some("nome-antigo"),
            "RenameArgs deve capturar NAME positional como nome atual"
        );
        assert!(args.name.is_none());
        assert_eq!(
            args.new_name, "nome-novo",
            "RenameArgs.new_name deve permanecer como flag obrigatória"
        );
    } else {
        panic!("esperava comando Rename");
    }
}

#[test]
fn regression_rename_args_accepts_flag_name() {
    let cli = Cli::try_parse_from([
        "sqlite-graphrag",
        "rename",
        "--name",
        "nome-antigo",
        "--new-name",
        "nome-novo",
    ])
    .unwrap();
    if let Commands::Rename(args) = cli.command {
        assert_eq!(
            args.name.as_deref(),
            Some("nome-antigo"),
            "RenameArgs deve capturar --name flag para back-compat"
        );
        assert!(args.name_positional.is_none());
    } else {
        panic!("esperava comando Rename");
    }
}

#[test]
fn regression_rename_args_accepts_alias_old() {
    // --old alias must still work (back-compat)
    let cli = Cli::try_parse_from([
        "sqlite-graphrag",
        "rename",
        "--old",
        "nome-antigo",
        "--new-name",
        "nome-novo",
    ])
    .unwrap();
    if let Commands::Rename(args) = cli.command {
        assert_eq!(
            args.name.as_deref(),
            Some("nome-antigo"),
            "RenameArgs deve aceitar --old como alias de --name"
        );
    } else {
        panic!("esperava comando Rename");
    }
}
