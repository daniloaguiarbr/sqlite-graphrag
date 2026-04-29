//! Process entry point: signal handling, language/timezone init, dispatch.

use std::sync::atomic::Ordering;

use clap::Parser;
use sqlite_graphrag::{
    cli::Cli,
    commands,
    constants::{
        CLI_LOCK_DEFAULT_WAIT_SECS, EMBEDDING_LOAD_EXPECTED_RSS_MB, MAX_CONCURRENT_CLI_INSTANCES,
        MIN_AVAILABLE_MEMORY_MB,
    },
    lock::acquire_cli_slot,
    memory_guard::{available_memory_mb, calculate_safe_concurrency, check_available_memory},
    storage::connection::register_vec_extension,
    SHUTDOWN,
};

fn main() {
    // Limitar thread pool do ONNX Runtime a 1 thread intra-op e 1 inter-op por instância,
    // evitando que invocações paralelas spawnem dezenas de threads cada.
    // Deve ser definido ANTES de fastembed inicializar a sessão ONNX.
    if std::env::var_os("ORT_NUM_THREADS").is_none() {
        // SAFETY: called before tokio runtime starts; single-threaded context
        // guaranteed by program startup order. set_var becomes unsafe in Rust 2024
        // edition; this comment documents the invariant explicitly.
        unsafe {
            std::env::set_var("ORT_NUM_THREADS", "1");
            std::env::set_var("ORT_INTRA_OP_NUM_THREADS", "1");
            std::env::set_var("ORT_INTER_OP_NUM_THREADS", "1");
            std::env::set_var("OMP_NUM_THREADS", "1");
        }
    }

    // Limitar pool Rayon a 2 threads — o daemon tokio usa worker_threads=2 e o Rayon
    // compartilha o mesmo processo; threads acima de 2 são desperdício para embeddings sequenciais.
    if std::env::var_os("RAYON_NUM_THREADS").is_none() {
        // SAFETY: called before tokio runtime starts; single-threaded context
        // guaranteed by program startup order. set_var becomes unsafe in Rust 2024
        // edition; this comment documents the invariant explicitly.
        unsafe {
            std::env::set_var("RAYON_NUM_THREADS", "2");
        }
    }

    // Desabilita a CPU memory arena do ONNX Runtime para evitar retenção
    // agressiva de chunks alocados em inferências de shapes variáveis.
    // Combinada com `with_arena_allocator(false)` no execution provider, fecha
    // a porta para o crescimento explosivo de RSS observado em corpora reais.
    // Referências:
    //   - https://onnxruntime.ai/docs/performance/tune-performance/memory.html
    //   - https://github.com/qdrant/fastembed/issues/570
    if std::env::var_os("ORT_DISABLE_CPU_MEM_ARENA").is_none() {
        // SAFETY: called before tokio runtime starts; single-threaded context
        // guaranteed by program startup order. set_var becomes unsafe in Rust 2024
        // edition; this comment documents the invariant explicitly.
        unsafe {
            std::env::set_var("ORT_DISABLE_CPU_MEM_ARENA", "1");
        }
    }

    // Pre-parse --verbose / -v before tracing init so the flag overrides the env var.
    // We avoid full Cli::parse() here because it would fail on missing required args
    // when --help is requested. Counts the number of `-v` occurrences (or `--verbose`).
    let verbose_count: u8 = std::env::args()
        .skip(1)
        .map(|a| {
            if a == "--verbose" || a == "-v" {
                1u8
            } else if a.starts_with("-v") && a.chars().skip(1).all(|c| c == 'v') {
                (a.len() - 1).try_into().unwrap_or(u8::MAX)
            } else {
                0u8
            }
        })
        .sum();

    let log_level = if verbose_count > 0 {
        match verbose_count {
            1 => "info".to_string(),
            2 => "debug".to_string(),
            _ => "trace".to_string(),
        }
    } else {
        std::env::var("SQLITE_GRAPHRAG_LOG_LEVEL").unwrap_or_else(|_| "warn".to_string())
    };
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

    // `--skip-memory-guard` é um escape hatch de testes. Sem esta proteção, suites que
    // usam `TempDir` exclusivos acabam auto-subindo múltiplos daemons com o modelo ONNX
    // carregado, inflando o RSS do host durante a execução dos testes. A força pode ser
    // religada explicitamente por `SQLITE_GRAPHRAG_DAEMON_FORCE_AUTOSTART=1`.
    if cli.skip_memory_guard
        && std::env::var_os("SQLITE_GRAPHRAG_DAEMON_FORCE_AUTOSTART").is_none()
        && std::env::var_os("SQLITE_GRAPHRAG_DAEMON_CHILD").is_none()
    {
        // SAFETY: called before tokio runtime starts; single-threaded context
        // guaranteed by program startup order. set_var becomes unsafe in Rust 2024
        // edition; this comment documents the invariant explicitly.
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_DAEMON_DISABLE_AUTOSTART", "1");
        }
    }

    // Inicializar idioma global ANTES de qualquer emit_progress bilíngue.
    // Esta chamada é no-op se o pre-parse acima já inicializou o OnceLock.
    sqlite_graphrag::i18n::init(cli.lang);

    // Inicializar fuso de exibição (flag --tz > env SQLITE_GRAPHRAG_DISPLAY_TZ > UTC).
    if let Err(e) = sqlite_graphrag::tz::init(cli.tz) {
        sqlite_graphrag::output::emit_error(&e.localized_message());
        std::process::exit(e.exit_code());
    }

    // Validar flags antes de qualquer inicialização pesada.
    if let Err(msg) = cli.validate_flags() {
        sqlite_graphrag::output::emit_error(&msg);
        std::process::exit(2);
    }

    let embedding_heavy = cli.command.is_embedding_heavy();
    let measured_available_mb = if embedding_heavy {
        let available_mb = if cli.skip_memory_guard {
            available_memory_mb()
        } else {
            match check_available_memory(MIN_AVAILABLE_MEMORY_MB) {
                Ok(available_mb) => available_mb,
                Err(e) => {
                    sqlite_graphrag::output::emit_error(&e.localized_message());
                    std::process::exit(e.exit_code());
                }
            }
        };

        Some(available_mb)
    } else {
        None
    };

    // Resolver parâmetros de concorrência com fallback para as constantes canônicas.
    let requested_concurrency = cli.max_concurrency.unwrap_or(MAX_CONCURRENT_CLI_INSTANCES);
    let max_concurrency = if embedding_heavy {
        let cpu_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        // SAFETY invariant: measured_available_mb is always Some when embedding_heavy is true,
        // because the block above (lines ~137-157) sets it to Some(available_mb) in that branch.
        // Using unwrap_or_else with exit instead of ? because main() returns ().
        let available_mb = measured_available_mb.unwrap_or_else(|| {
            sqlite_graphrag::output::emit_error_i18n(
                "embedding-heavy command must measure available RAM",
                "comando intensivo em embedding precisa medir RAM disponível",
            );
            std::process::exit(20);
        });
        let safe_concurrency = calculate_safe_concurrency(
            available_mb,
            cpu_count,
            EMBEDDING_LOAD_EXPECTED_RSS_MB,
            MAX_CONCURRENT_CLI_INSTANCES,
        );
        let effective_concurrency = requested_concurrency.min(safe_concurrency);

        sqlite_graphrag::output::emit_progress_i18n(
            &format!(
                "Heavy command detected; available memory: {available_mb} MB; safe concurrency: {safe_concurrency}"
            ),
            &format!(
                "Comando pesado detectado; memória disponível: {available_mb} MB; concorrência segura: {safe_concurrency}"
            ),
        );

        if effective_concurrency < requested_concurrency {
            sqlite_graphrag::output::emit_progress_i18n(
                &format!(
                    "Reducing requested concurrency from {requested_concurrency} to {effective_concurrency} to avoid memory oversubscription"
                ),
                &format!(
                    "Reduzindo a concorrência solicitada de {requested_concurrency} para {effective_concurrency} para evitar oversubscription de memória"
                ),
            );
        }

        effective_concurrency
    } else {
        requested_concurrency.min(MAX_CONCURRENT_CLI_INSTANCES)
    };
    let wait_secs = cli.wait_lock.unwrap_or(CLI_LOCK_DEFAULT_WAIT_SECS);

    // Adquirir slot no semáforo de contagem. O handle é mantido vivo até o fim de main
    // para que o flock seja liberado automaticamente ao fechar o descritor.
    let _slot_guard = if cli.command.uses_cli_slot() {
        Some(match acquire_cli_slot(max_concurrency, Some(wait_secs)) {
            Ok(pair) => pair,
            Err(e) => {
                sqlite_graphrag::output::emit_error(&e.localized_message());
                std::process::exit(e.exit_code());
            }
        })
    } else {
        None
    };

    // Registrar handler para SIGINT / SIGTERM / SIGHUP (via feature "termination").
    // O handler sinaliza SHUTDOWN e loga o evento; a limpeza do slot ocorre pelo Drop do File.
    if let Err(e) = ctrlc::set_handler(move || {
        SHUTDOWN.store(true, Ordering::SeqCst);
        tracing::warn!(
            "shutdown signal received; waiting for current command to finish gracefully"
        );
    }) {
        tracing::warn!("failed to register signal handler: {e}");
    }

    let result = match cli.command {
        sqlite_graphrag::cli::Commands::Init(args) => commands::init::run(args),
        sqlite_graphrag::cli::Commands::Daemon(args) => commands::daemon::run(args),
        sqlite_graphrag::cli::Commands::Remember(args) => commands::remember::run(args),
        sqlite_graphrag::cli::Commands::Ingest(args) => commands::ingest::run(args),
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
        sqlite_graphrag::cli::Commands::Cache(args) => commands::cache::run(args),
        sqlite_graphrag::cli::Commands::DebugSchema(args) => commands::debug_schema::run(args),
    };

    if let Err(e) = result {
        sqlite_graphrag::output::emit_error(&e.localized_message());
        std::process::exit(e.exit_code());
    }
}
