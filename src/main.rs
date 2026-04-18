use std::sync::atomic::Ordering;

use clap::Parser;
use neurographrag::{
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

    let log_level = std::env::var("NEUROGRAPHRAG_LOG_LEVEL").unwrap_or_else(|_| "warn".to_string());
    let log_format =
        std::env::var("NEUROGRAPHRAG_LOG_FORMAT").unwrap_or_else(|_| "pretty".to_string());

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

    let cli = Cli::parse();

    // Validar flags antes de qualquer inicialização pesada.
    if let Err(msg) = cli.validate_flags() {
        eprintln!("erro: {msg}");
        std::process::exit(2);
    }

    // Verificar disponibilidade de memória antes de carregar o modelo ONNX.
    if !cli.skip_memory_guard {
        if let Err(e) = check_available_memory(MIN_AVAILABLE_MEMORY_MB) {
            eprintln!("Error: {e}");
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
            eprintln!("Error: {e}");
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
        neurographrag::cli::Commands::Init(args) => commands::init::run(args),
        neurographrag::cli::Commands::Remember(args) => commands::remember::run(args),
        neurographrag::cli::Commands::Recall(args) => commands::recall::run(args),
        neurographrag::cli::Commands::Read(args) => commands::read::run(args),
        neurographrag::cli::Commands::List(args) => commands::list::run(args),
        neurographrag::cli::Commands::Forget(args) => commands::forget::run(args),
        neurographrag::cli::Commands::Purge(args) => commands::purge::run(args),
        neurographrag::cli::Commands::Rename(args) => commands::rename::run(args),
        neurographrag::cli::Commands::Edit(args) => commands::edit::run(args),
        neurographrag::cli::Commands::History(args) => commands::history::run(args),
        neurographrag::cli::Commands::Restore(args) => commands::restore::run(args),
        neurographrag::cli::Commands::HybridSearch(args) => commands::hybrid_search::run(args),
        neurographrag::cli::Commands::Health(args) => commands::health::run(args),
        neurographrag::cli::Commands::Migrate(args) => commands::migrate::run(args),
        neurographrag::cli::Commands::NamespaceDetect(args) => {
            commands::namespace_detect::run(args)
        }
        neurographrag::cli::Commands::Optimize(args) => commands::optimize::run(args),
        neurographrag::cli::Commands::Stats(args) => commands::stats::run(args),
        neurographrag::cli::Commands::SyncSafeCopy(args) => commands::sync_safe_copy::run(args),
        neurographrag::cli::Commands::Vacuum(args) => commands::vacuum::run(args),
        neurographrag::cli::Commands::Link(args) => commands::link::run(args),
        neurographrag::cli::Commands::Unlink(args) => commands::unlink::run(args),
        neurographrag::cli::Commands::Related(args) => commands::related::run(args),
        neurographrag::cli::Commands::Graph(args) => commands::graph_export::run(args),
        neurographrag::cli::Commands::CleanupOrphans(args) => commands::cleanup_orphans::run(args),
    };

    if let Err(e) = result {
        tracing::error!(error = %e);
        eprintln!("Error: {e}");
        std::process::exit(e.exit_code());
    }
}
