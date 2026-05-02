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
    // Limit the ONNX Runtime thread pool to 1 intra-op and 1 inter-op thread per instance,
    // preventing parallel invocations from spawning dozens of threads each.
    // Must be set BEFORE fastembed initializes the ONNX session.
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

    // Limit the Rayon pool to 2 threads — the tokio daemon uses worker_threads=2 and Rayon
    // shares the same process; more than 2 threads is waste for sequential embeddings.
    if std::env::var_os("RAYON_NUM_THREADS").is_none() {
        // SAFETY: called before tokio runtime starts; single-threaded context
        // guaranteed by program startup order. set_var becomes unsafe in Rust 2024
        // edition; this comment documents the invariant explicitly.
        unsafe {
            std::env::set_var("RAYON_NUM_THREADS", "2");
        }
    }

    // Disables the ONNX Runtime CPU memory arena to avoid aggressive
    // retention of chunks allocated during variable-shape inferences.
    // Combined with `with_arena_allocator(false)` on the execution provider, this closes
    // the door on the explosive RSS growth observed in real corpora.
    // References:
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

    // Pre-parse --lang before Cli::parse() so the language is set even
    // when clap exits early via process::exit (--help, parse errors, etc.).
    // The subsequent call to init(cli.lang) will be silently ignored by the OnceLock.
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

    // `--skip-memory-guard` is a test escape hatch. Without this protection, suites that
    // use exclusive `TempDir`s end up auto-spawning multiple daemons with the ONNX model
    // loaded, inflating the host RSS during test execution. Auto-spawn can be
    // explicitly re-enabled via `SQLITE_GRAPHRAG_DAEMON_FORCE_AUTOSTART=1`.
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

    // Initialize global language BEFORE any bilingual emit_progress.
    // This call is a no-op if the pre-parse above already initialized the OnceLock.
    sqlite_graphrag::i18n::init(cli.lang);

    // Initialize display timezone (flag --tz > env SQLITE_GRAPHRAG_DISPLAY_TZ > UTC).
    if let Err(e) = sqlite_graphrag::tz::init(cli.tz) {
        sqlite_graphrag::output::emit_error(&e.localized_message());
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let _ = std::io::Write::flush(&mut std::io::stderr());
        std::process::exit(e.exit_code());
    }

    // Validate flags before any heavy initialization.
    if let Err(msg) = cli.validate_flags() {
        sqlite_graphrag::output::emit_error(&msg);
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let _ = std::io::Write::flush(&mut std::io::stderr());
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
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                    let _ = std::io::Write::flush(&mut std::io::stderr());
                    std::process::exit(e.exit_code());
                }
            }
        };

        Some(available_mb)
    } else {
        None
    };

    // Resolve concurrency parameters with fallback to canonical constants.
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
                &sqlite_graphrag::i18n::validation::runtime_pt::embedding_heavy_must_measure_ram(),
            );
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let _ = std::io::Write::flush(&mut std::io::stderr());
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
            &sqlite_graphrag::i18n::validation::runtime_pt::heavy_command_detected(
                available_mb,
                safe_concurrency,
            ),
        );

        if effective_concurrency < requested_concurrency {
            sqlite_graphrag::output::emit_progress_i18n(
                &format!(
                    "Reducing requested concurrency from {requested_concurrency} to {effective_concurrency} to avoid memory oversubscription"
                ),
                &sqlite_graphrag::i18n::validation::runtime_pt::reducing_concurrency(
                    requested_concurrency,
                    effective_concurrency,
                ),
            );
        }

        effective_concurrency
    } else {
        requested_concurrency.min(MAX_CONCURRENT_CLI_INSTANCES)
    };
    let wait_secs = cli.wait_lock.unwrap_or(CLI_LOCK_DEFAULT_WAIT_SECS);

    // Acquire a slot in the counting semaphore. The handle is kept alive until end of main
    // so the flock is released automatically when the file descriptor is closed.
    let _slot_guard = if cli.command.uses_cli_slot() {
        Some(match acquire_cli_slot(max_concurrency, Some(wait_secs)) {
            Ok(pair) => pair,
            Err(e) => {
                sqlite_graphrag::output::emit_error(&e.localized_message());
                let _ = std::io::Write::flush(&mut std::io::stdout());
                let _ = std::io::Write::flush(&mut std::io::stderr());
                std::process::exit(e.exit_code());
            }
        })
    } else {
        None
    };

    // Register handler for SIGINT / SIGTERM / SIGHUP (via the "termination" feature).
    // The handler signals SHUTDOWN and logs the event; slot cleanup occurs via Drop on the File.
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
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let _ = std::io::Write::flush(&mut std::io::stderr());
        std::process::exit(e.exit_code());
    }
}
