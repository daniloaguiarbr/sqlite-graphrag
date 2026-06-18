//! Process entry point: signal handling, language/timezone init, dispatch.

// v1.0.74: gate the mimalloc global allocator behind a cfg so the
// Miri Unsafe Validation job (which passes
// `RUSTFLAGS="--cfg sqlite_graphrag_miri"`) can run the unsafe
// `f32_to_bytes` and `controlled_batch_plan` tests. mimalloc's
// `mi_malloc_aligned` is a foreign function that Miri cannot model
// (`error: unsupported operation: can't call foreign function
// 'mi_malloc_aligned' on OS 'linux'`). The default Linux allocator is
// used during Miri runs; production binaries still get mimalloc.
#[cfg(not(sqlite_graphrag_miri))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use clap::Parser;
use sqlite_graphrag::{
    cli::Cli,
    commands,
    constants::{
        CLI_LOCK_DEFAULT_WAIT_SECS, LLM_WORKER_RSS_MB, MAX_CONCURRENT_CLI_INSTANCES,
        MIN_AVAILABLE_MEMORY_MB,
    },
    lock::acquire_cli_slot,
    memory_guard::{available_memory_mb, calculate_safe_concurrency, check_available_memory},
    storage::connection::register_vec_extension,
};

fn main() -> std::process::ExitCode {
    // v1.0.80 (A1/G6): the explicit Write::flush calls below are NOT
    // redundant. `std::process::ExitCode` is a transparent wrapper around
    // a u8 returned from main; on process exit, the C runtime flushes its
    // OWN stdio buffers but does NOT know about Rust's internal
    // `BufWriter` wrapping stdout/stderr. Without the explicit flush, the
    // last partial line of JSON output (notably from
    // `output::emit_json_compact` and `emit_progress`) can be lost when
    // the process is killed by a signal or exits with an error code. This
    // is a deliberate defensive policy: flush every error-path AND the
    // success-path before returning.
    // v1.0.80 (A1/G1): the main thread is intentionally 100% synchronous.
    // The default LLM-only build (v1.0.76+) does not own a tokio runtime
    // here: every remember, ingest, and enrich spawns a headless claude
    // or codex subprocess via std::process::Command and waits on its exit.
    // The per-subprocess concurrency cap is enforced by the
    // acquire_cli_slot counting semaphore and the MAX_CONCURRENT_CLI_*
    // constants; cross-process sync happens via SQLite WAL and flock.
    // The pre-tokio design is a deliberate policy choice: no async
    // runtime context to cancel, no tokio::select! arms to skip, and no
    // JoinSet to drain on shutdown (see ADR-0034 for the SHUTDOWN global
    // and the audit-mode bypass). Touching this entry point requires
    // revisiting the per-subprocess cancellation policy, not just adding
    // a runtime.
    // Reset SIGPIPE to default so pipe consumers (head, jaq) cause clean exit 141.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    sqlite_graphrag::terminal::init_console();

    // G28: reap orphan LLM subprocesses from a previous crashed invocation
    // BEFORE doing any work. The scan is a no-op on non-Unix platforms.
    let _reaper_report = sqlite_graphrag::reaper::scan_and_kill_orphans();

    // v1.0.79: ONNX Runtime removed from default LLM-only build. The
    // fastembed/ort/onnxruntime crates are no longer in the dependency tree;
    // embeddings and NER delegate to headless claude/codex subprocesses
    // (OAuth-only, no MCP, no hooks). Only RAYON_NUM_THREADS below remains
    // relevant for parallel similarity and batch ops in the LLM-only path.

    // Limit the Rayon pool to 2 threads — more is waste for sequential embeddings.
    if std::env::var_os("RAYON_NUM_THREADS").is_none() {
        // SAFETY: this  runs during single-threaded program startup,
        // before any rayon pool is built and before any worker thread exists.
        // Rayon reads  exactly once during
        //  and never re-reads it, so mutating the env
        // here is the only correct point to cap the pool. The cap of 2 is
        // calibrated against : each worker holds a
        // single batch-embedding call, so > 2 is waste and risks RSS
        // oversubscription on 4-8 GiB hosts. The 2024 edition makes
        // ; this comment is the explicit documentation of the
        // single-threaded invariant.
        unsafe {
            std::env::set_var("RAYON_NUM_THREADS", "2");
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

    sqlite_graphrag::telemetry::init_tracing(&log_level, &log_format);

    register_vec_extension();

    // v1.0.80 (A1/G7): the deadlock-detection thread below is intentionally
    // process-scoped (it has no shutdown signal). It is a watchdog: it polls
    // every 10 seconds and reports any deadlocks it finds via tracing, then
    // sleeps again. When the process exits (via std::process::ExitCode
    // return or a signal), the kernel tears down all threads; there is no
    // leak because the thread is never joined or detached in the Rust
    // sense. The 10-second poll interval is a balance: short enough to
    // catch deadlocks in interactive tests, long enough to not pollute
    // tracing output during normal operation. The thread body is
    // panic-resistant: a panic inside the loop kills only this thread, and
    // since the main thread never joins it, the panic is silently dropped
    // (Rust's default panic-on-thread-death is bypassed for detached
    // threads). We accept this because the alternative — a panicking
    // deadlock check — would itself be a deadlock.

    #[cfg(feature = "deadlock-detection")]
    {
        std::thread::spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_secs(10));
            let deadlocks = parking_lot::deadlock::check_deadlock();
            if !deadlocks.is_empty() {
                tracing::error!(target: "deadlock_detection", count = deadlocks.len(), "deadlocks detected");
                for (i, threads) in deadlocks.iter().enumerate() {
                    for t in threads {
                        tracing::error!(
                            target: "deadlock_detection",
                            index = i,
                            thread_id = ?t.thread_id(),
                            backtrace = ?t.backtrace(),
                            "deadlock thread info"
                        );
                    }
                }
            }
        });
    }

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

    // Initialize global language BEFORE any bilingual emit_progress.
    // This call is a no-op if the pre-parse above already initialized the OnceLock.
    sqlite_graphrag::i18n::init(cli.lang);

    // G42/S1 (v1.0.79): the global --embedding-dim flag materialises as the
    // env var so every downstream resolution point (constants::embedding_dim,
    // schema_meta sync) sees a single, consistent override channel.
    if let Some(dim) = cli.embedding_dim {
        // SAFETY: set before any tokio runtime or worker thread spawns;
        // single-threaded context guaranteed by program startup order.
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_EMBEDDING_DIM", dim.to_string());
        }
    }

    // Initialize display timezone (flag --tz > env SQLITE_GRAPHRAG_DISPLAY_TZ > UTC).
    if let Err(e) = sqlite_graphrag::tz::init(cli.tz) {
        sqlite_graphrag::output::emit_error(&e.localized_message());
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let _ = std::io::Write::flush(&mut std::io::stderr());
        return std::process::ExitCode::from(e.exit_code() as u8);
    }

    // Validate flags before any heavy initialization.
    if let Err(msg) = cli.validate_flags() {
        sqlite_graphrag::output::emit_error(&msg);
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let _ = std::io::Write::flush(&mut std::io::stderr());
        return std::process::ExitCode::from(2);
    }

    let embedding_heavy = cli
        .command
        .as_ref()
        .is_some_and(|c| c.is_embedding_heavy());
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
                    return std::process::ExitCode::from(e.exit_code() as u8);
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
        let available_mb = match measured_available_mb {
            Some(mb) => mb,
            None => {
                sqlite_graphrag::output::emit_error_i18n(
                    "embedding-heavy command must measure available RAM",
                    &sqlite_graphrag::i18n::validation::runtime_pt::embedding_heavy_must_measure_ram(),
                );
                let _ = std::io::Write::flush(&mut std::io::stdout());
                let _ = std::io::Write::flush(&mut std::io::stderr());
                return std::process::ExitCode::from(20);
            }
        };
        // v1.0.79: every build is LLM-only; the per-worker budget is the
        // claude/codex subprocess RSS, not the old 1100 MB ONNX model load.
        let safe_concurrency = calculate_safe_concurrency(
            available_mb,
            cpu_count,
            LLM_WORKER_RSS_MB,
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
    let _slot_guard = if cli.command.as_ref().is_some_and(|c| c.uses_cli_slot()) {
        Some(match acquire_cli_slot(max_concurrency, Some(wait_secs)) {
            Ok(pair) => pair,
            Err(e) => {
                sqlite_graphrag::output::emit_error(&e.localized_message());
                let _ = std::io::Write::flush(&mut std::io::stdout());
                let _ = std::io::Write::flush(&mut std::io::stderr());
                return std::process::ExitCode::from(e.exit_code() as u8);
            }
        })
    } else {
        None
    };

    sqlite_graphrag::signals::register_shutdown_handler();

    // v1.0.84 (ADR-0042 / GAP-002): early-exit branch for `--dry-run-backend`.
    // Resolves the LLM backend that WOULD be invoked for embedding,
    // prints a compact JSON envelope, and exits 0 without spawning any
    // subprocess. Sits BEFORE the subcommand match so it works even when
    // no positional command is provided (sanity-check flag).
    if cli.dry_run_backend {
        match commands::dry_run_backend::emit_dry_run_backend(&cli) {
            Ok(()) => {
                let _ = std::io::Write::flush(&mut std::io::stdout());
                let _ = std::io::Write::flush(&mut std::io::stderr());
                return std::process::ExitCode::SUCCESS;
            }
            Err(e) => {
                sqlite_graphrag::output::emit_error_json(e.exit_code(), &e.localized_message());
                sqlite_graphrag::output::emit_error(&e.localized_message());
                let _ = std::io::Write::flush(&mut std::io::stdout());
                let _ = std::io::Write::flush(&mut std::io::stderr());
                return std::process::ExitCode::from(e.exit_code() as u8);
            }
        }
    }

    let result = match cli.command {
        Some(cmd) => match cmd {
            sqlite_graphrag::cli::Commands::Init(args) => commands::init::run(args),
            sqlite_graphrag::cli::Commands::Remember(args) => {
                commands::remember::run(args, cli.llm_backend)
            }
            sqlite_graphrag::cli::Commands::RememberBatch(args) => {
                commands::remember_batch::run(args)
            }
            sqlite_graphrag::cli::Commands::Ingest(args) => {
                commands::ingest::run(args, cli.llm_backend)
            }
            sqlite_graphrag::cli::Commands::Recall(args) => {
                commands::recall::run(args, cli.llm_backend)
            }
            // v1.0.82 (GAP-003): pass LlmBackendChoice (Copy) so the dispatch
            // match arm can move `args` while still borrowing `cli`.
            sqlite_graphrag::cli::Commands::Edit(args) => {
                commands::edit::run(args, cli.llm_backend)
            }
            sqlite_graphrag::cli::Commands::History(args) => commands::history::run(args),
            sqlite_graphrag::cli::Commands::Restore(args) => commands::restore::run(args),
            sqlite_graphrag::cli::Commands::HybridSearch(args) => {
                commands::hybrid_search::run(args, cli.llm_backend)
            }
            sqlite_graphrag::cli::Commands::Read(args) => commands::read::run(args),
            sqlite_graphrag::cli::Commands::List(args) => commands::list::run(args),
            sqlite_graphrag::cli::Commands::Forget(args) => commands::forget::run(args),
            sqlite_graphrag::cli::Commands::Purge(args) => commands::purge::run(args),
            sqlite_graphrag::cli::Commands::Rename(args) => commands::rename::run(args),
            sqlite_graphrag::cli::Commands::Health(args) => commands::health::run(args),
            sqlite_graphrag::cli::Commands::Migrate(args) => commands::migrate::run(args),
            sqlite_graphrag::cli::Commands::NamespaceDetect(args) => {
                commands::namespace_detect::run(args)
            }
            sqlite_graphrag::cli::Commands::Optimize(args) => commands::optimize::run(args),
            sqlite_graphrag::cli::Commands::Stats(args) => commands::stats::run(args),
            sqlite_graphrag::cli::Commands::SyncSafeCopy(args) => {
                commands::sync_safe_copy::run(args)
            }
            sqlite_graphrag::cli::Commands::Backup(args) => commands::backup::run(args),
            sqlite_graphrag::cli::Commands::Vacuum(args) => commands::vacuum::run(args),
            sqlite_graphrag::cli::Commands::Link(args) => commands::link::run(args),
            sqlite_graphrag::cli::Commands::Unlink(args) => commands::unlink::run(args),
            sqlite_graphrag::cli::Commands::DeepResearch(args) => {
                commands::deep_research::run(args)
            }
            sqlite_graphrag::cli::Commands::Related(args) => commands::related::run(args),
            sqlite_graphrag::cli::Commands::Graph(args) => commands::graph_export::run(args),
            sqlite_graphrag::cli::Commands::Export(args) => commands::export::run(args),
            sqlite_graphrag::cli::Commands::Fts(args) => commands::fts::run(args),
            sqlite_graphrag::cli::Commands::Vec(args) => commands::vec::run(args),
            sqlite_graphrag::cli::Commands::CodexModels => {
                let models = commands::codex_spawn::list_codex_models();
                let payload = serde_json::json!({
                    "action": "codex_models",
                    "count": models.len(),
                    "models": models,
                    "default": "gpt-5.5",
                });
                sqlite_graphrag::output::emit_json_compact(&payload).and(Ok(()))
            }
            sqlite_graphrag::cli::Commands::PruneRelations(args) => {
                commands::prune_relations::run(args)
            }
            sqlite_graphrag::cli::Commands::PruneNer(args) => commands::prune_ner::run(args),
            sqlite_graphrag::cli::Commands::CleanupOrphans(args) => {
                commands::cleanup_orphans::run(args)
            }
            sqlite_graphrag::cli::Commands::MemoryEntities(args) => {
                commands::memory_entities::run(args)
            }
            sqlite_graphrag::cli::Commands::Cache(args) => commands::cache::run(args),
            sqlite_graphrag::cli::Commands::DeleteEntity(args) => {
                commands::delete_entity::run(args)
            }
            sqlite_graphrag::cli::Commands::Reclassify(args) => commands::reclassify::run(args),
            sqlite_graphrag::cli::Commands::RenameEntity(args) => {
                commands::rename_entity::run(args)
            }
            sqlite_graphrag::cli::Commands::MergeEntities(args) => {
                commands::merge_entities::run(args)
            }
            sqlite_graphrag::cli::Commands::Enrich(args) => {
                commands::enrich::run(&args, cli.llm_backend)
            }
            sqlite_graphrag::cli::Commands::ReclassifyRelation(args) => {
                commands::reclassify_relation::run(args)
            }
            sqlite_graphrag::cli::Commands::NormalizeEntities(args) => {
                commands::normalize_entities::run(args)
            }
            sqlite_graphrag::cli::Commands::Completions(args) => commands::completions::run(args),
            sqlite_graphrag::cli::Commands::DebugSchema(args) => commands::debug_schema::run(args),
            sqlite_graphrag::cli::Commands::Slots(args) => commands::slots::run(args),
            sqlite_graphrag::cli::Commands::Pending(args) => commands::pending::run(args),
            sqlite_graphrag::cli::Commands::Embedding(args) => {
                commands::embedding::run(args, cli.llm_backend)
            }
            sqlite_graphrag::cli::Commands::PendingEmbeddings(args) => {
                commands::pending_embeddings::run(args)
            }
        },
        None => Ok(()),
    };

    if let Err(e) = result {
        sqlite_graphrag::output::emit_error_json(e.exit_code(), &e.localized_message());
        sqlite_graphrag::output::emit_error(&e.localized_message());
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let _ = std::io::Write::flush(&mut std::io::stderr());
        return std::process::ExitCode::from(e.exit_code() as u8);
    }

    let _ = std::io::Write::flush(&mut std::io::stdout());
    let _ = std::io::Write::flush(&mut std::io::stderr());

    if sqlite_graphrag::shutdown_requested() {
        // GAP-002 (v1.0.82): deterministic code 19 for shutdown, regardless
        // of which Unix signal triggered it. The JSON envelope has already
        // been emitted to stdout by the signal handler itself; this branch
        // just propagates the code to the shell.
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let _ = std::io::Write::flush(&mut std::io::stderr());
        return std::process::ExitCode::from(sqlite_graphrag::constants::SHUTDOWN_EXIT_CODE as u8);
    }

    std::process::ExitCode::SUCCESS
}
