//! Centralized tracing subscriber initialization.
//!
//! Configures the global subscriber with JSON or pretty format,
//! installs the panic hook and the log-to-tracing bridge.

use tracing_subscriber::EnvFilter;

/// Initializes the global tracing subscriber, panic hook, and log bridge.
///
/// Must be called exactly once, before any tracing events are emitted.
/// After this call, panics on any thread produce `tracing::error!` events,
/// and `log` crate events from dependencies (refinery, ureq, ort) are
/// forwarded to the tracing subscriber.
pub fn init_tracing(log_level: &str, log_format: &str) {
    // TR02: the log→tracing bridge is activated automatically by
    // tracing-subscriber's built-in `tracing-log` feature (default).
    // Calling LogTracer::init() separately would conflict with the
    // global logger that tracing-subscriber installs via .init().
    let use_ansi = crate::terminal::should_use_ansi();

    if log_format == "json" {
        tracing_subscriber::fmt()
            .json()
            .with_ansi(false)
            .with_thread_ids(true)
            .with_thread_names(true)
            .with_env_filter(EnvFilter::new(log_level))
            .with_writer(std::io::stderr)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_ansi(use_ansi)
            .with_env_filter(EnvFilter::new(log_level))
            .with_writer(std::io::stderr)
            .init();
    }

    // TR05: confirm effective filter after init
    tracing::debug!(
        target: "telemetry",
        filter = %log_level,
        format = %log_format,
        ansi = use_ansi,
        "tracing subscriber initialized"
    );

    // TR01 (v1.0.80, A1/G2): panic hook emits a structured tracing::error!
    // and DELIBERATELY DOES NOT call the previous hook. The default Rust
    // panic hook prints the same payload + location to stderr; combined
    // with the tracing event below, that produces a double-trace (one
    // structured event in JSON or pretty, one unstructured dump). We
    // prefer the structured single-trace: the tracing event carries the
    // same payload and location fields and is captured by the global
    // subscriber. Test runs still fail on panic because Rust aborts the
    // process regardless of which hook is installed.
    std::panic::set_hook(Box::new(|info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("<non-string panic>");
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        tracing::error!(
            target: "panic",
            message = %payload,
            location = location.as_deref().unwrap_or("unknown"),
            "thread panicked"
        );
    }));
}
