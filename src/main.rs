use std::sync::atomic::Ordering;

use clap::Parser;
use sqlite_graphrag::{
    cli::Cli,
    commands,
    constants::{
        CLI_LOCK_DEFAULT_WAIT_SECS, MAX_CONCURRENT_CLI_INSTANCES, MIN_AVAILABLE_MEMORY_MB,
    },
    lock::acquire_cli_slot,
    memory_guard::check_available_memory,
    storage::connection::register_vec_extension,
    SHUTDOWN,
};

fn main() {
    // Limitar thread pool do ONNX Runtime a 1 thread intra-op e 1 inter-op por instância,
    // evitando que invocações paralelas spawnem dezenas de threads cada.
    // Deve ser definido ANTES de fastembed inicializar a sessão ONNX.
    if std::env::var_os("ORT_NUM_THREADS").is_none() {
        // SAFETY: single-threaded neste ponto — nenhuma outra thread existe ainda.
        unsafe {
            std::env::set_var("ORT_NUM_THREADS", "1");
            std::env::set_var("ORT_INTRA_OP_NUM_THREADS", "1");
            std::env::set_var("OMP_NUM_THREADS", "1");
        }
    }

    let log_level =
        std::env::var("SQLITE_GRAPHRAG_LOG_LEVEL").unwrap_or_else(|_| "warn".to_string());
    let log_format =
        std::env::var("SQLITE_GRAPHRAG_LOG_FORMAT").unwrap_or_else(|_| "pretty".to_string());

    if log_format == "json" {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(tracing_subscriber::EnvFilter::new(&log_level))
            .with_writer(std::io::stderr)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(&log_level))
            .with_writer(std::io::stderr)
            .init();
    }

    register_vec_extension();

    // Pre-parse --lang antes de Cli::parse() para que o idioma seja definido mesmo
    // quando o clap encerra cedo via process::exit (--help, erros de parse, etc.).
    // A chamada subsequente a init(cli.lang) será silenciosamente ignorada pelo OnceLock.
    {
        let args: Vec<String> = std::env::args().collect();
        let mut lang_override: Option<sqlite_graphrag::i18n::Language> = None;
        let mut i = 1usize;
        while i < args.len() {
            if args[i] == "--lang" {
                if let Some(val) = args.get(i + 1) {
                    lang_override = sqlite_graphrag::i18n::Language::from_str_opt(val);
                }
                i += 2;
            } else if let Some(val) = args[i].strip_prefix("--lang=") {
                lang_override = sqlite_graphrag::i18n::Language::from_str_opt(val);
                i += 1;
            } else {
                i += 1;
            }
        }
        sqlite_graphrag::i18n::init(lang_override);
    }

    let cli = Cli::parse();

    // Inicializar idioma global ANTES de qualquer emit_progress bilíngue.
    // Esta chamada é no-op se o pre-parse acima já inicializou o OnceLock.
    sqlite_graphrag::i18n::init(cli.lang);

    // Inicializar fuso de exibição (flag --tz > env SQLITE_GRAPHRAG_DISPLAY_TZ > UTC).
    if let Err(e) = sqlite_graphrag::tz::init(cli.tz) {
        eprintln!(
            "{}: {}",
            sqlite_graphrag::i18n::prefixo_erro(),
            e.localized_message()
        );
        std::process::exit(e.exit_code());
    }

    // Validar flags antes de qualquer inicialização pesada.
    if let Err(msg) = cli.validate_flags() {
        let prefix = match sqlite_graphrag::i18n::current() {
            sqlite_graphrag::i18n::Language::English => "error",
            sqlite_graphrag::i18n::Language::Portugues => "erro",
        };
        eprintln!("{prefix}: {msg}");
        std::process::exit(2);
    }

    // Verificar disponibilidade de memória antes de carregar o modelo ONNX.
    if !cli.skip_memory_guard {
        if let Err(e) = check_available_memory(MIN_AVAILABLE_MEMORY_MB) {
            eprintln!(
                "{}: {}",
                sqlite_graphrag::i18n::prefixo_erro(),
                e.localized_message()
            );
            std::process::exit(e.exit_code());
        }
    }

    // Resolver parâmetros de concorrência com fallback para as constantes canônicas.
    let max_concurrency = cli.max_concurrency.unwrap_or(MAX_CONCURRENT_CLI_INSTANCES);
    let wait_secs = cli.wait_lock.unwrap_or(CLI_LOCK_DEFAULT_WAIT_SECS);

    // Adquirir slot no semáforo de contagem. O handle é mantido vivo até o fim de main
    // para que o flock seja liberado automaticamente ao fechar o descritor.
    let (_lock_handle, _slot) = match acquire_cli_slot(max_concurrency, Some(wait_secs)) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!(
                "{}: {}",
                sqlite_graphrag::i18n::prefixo_erro(),
                e.localized_message()
            );
            std::process::exit(e.exit_code());
        }
    };

    // Registrar handler para SIGINT / SIGTERM / SIGHUP (via feature "termination").
    // O handler sinaliza SHUTDOWN e loga o evento; a limpeza do slot ocorre pelo Drop do File.
    if let Err(e) = ctrlc::set_handler(move || {
        SHUTDOWN.store(true, Ordering::SeqCst);
        tracing::warn!("recebido sinal de shutdown; aguardando comando encerrar gracefully");
    }) {
        tracing::warn!("não foi possível registrar handler de sinal: {e}");
    }

    let result = match cli.command {
        sqlite_graphrag::cli::Commands::Init(args) => commands::init::run(args),
        sqlite_graphrag::cli::Commands::Remember(args) => commands::remember::run(args),
        sqlite_graphrag::cli::Commands::Recall(args) => commands::recall::run(args),
        sqlite_graphrag::cli::Commands::Read(args) => commands::read::run(args),
        sqlite_graphrag::cli::Commands::List(args) => commands::list::run(args),
        sqlite_graphrag::cli::Commands::Forget(args) => commands::forget::run(args),
        sqlite_graphrag::cli::Commands::Purge(args) => commands::purge::run(args),
        sqlite_graphrag::cli::Commands::Rename(args) => commands::rename::run(args),
        sqlite_graphrag::cli::Commands::Edit(args) => commands::edit::run(args),
        sqlite_graphrag::cli::Commands::History(args) => commands::history::run(args),
        sqlite_graphrag::cli::Commands::Restore(args) => commands::restore::run(args),
        sqlite_graphrag::cli::Commands::HybridSearch(args) => commands::hybrid_search::run(args),
        sqlite_graphrag::cli::Commands::Health(args) => commands::health::run(args),
        sqlite_graphrag::cli::Commands::Migrate(args) => commands::migrate::run(args),
        sqlite_graphrag::cli::Commands::NamespaceDetect(args) => {
            commands::namespace_detect::run(args)
        }
        sqlite_graphrag::cli::Commands::Optimize(args) => commands::optimize::run(args),
        sqlite_graphrag::cli::Commands::Stats(args) => commands::stats::run(args),
        sqlite_graphrag::cli::Commands::SyncSafeCopy(args) => commands::sync_safe_copy::run(args),
        sqlite_graphrag::cli::Commands::Vacuum(args) => commands::vacuum::run(args),
        sqlite_graphrag::cli::Commands::Link(args) => commands::link::run(args),
        sqlite_graphrag::cli::Commands::Unlink(args) => commands::unlink::run(args),
        sqlite_graphrag::cli::Commands::Related(args) => commands::related::run(args),
        sqlite_graphrag::cli::Commands::Graph(args) => commands::graph_export::run(args),
        sqlite_graphrag::cli::Commands::CleanupOrphans(args) => {
            commands::cleanup_orphans::run(args)
        }
        sqlite_graphrag::cli::Commands::DebugSchema(args) => commands::debug_schema::run(args),
    };

    if let Err(e) = result {
        tracing::error!(error = %e);
        eprintln!(
            "{}: {}",
            sqlite_graphrag::i18n::prefixo_erro(),
            e.localized_message()
        );
        std::process::exit(e.exit_code());
    }
}
