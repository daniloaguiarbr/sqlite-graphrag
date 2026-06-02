//! Validates that the telemetry module is accessible and its public API
//! compiles correctly.  Full event-capture tests require a local subscriber
//! (incompatible with the global subscriber set by `init_tracing`), so we
//! validate the function signature and ensure no panics during type checking.

#[test]
fn telemetry_init_fn_is_accessible() {
    let _fn_ptr = sqlite_graphrag::telemetry::init_tracing as fn(&str, &str);
}
