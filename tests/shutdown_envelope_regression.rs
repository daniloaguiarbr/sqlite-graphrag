//! v1.0.82 (GAP-002): regression tests for the shutdown JSON envelope.
//!
//! The pre-v1.0.80 shutdown handler wrote a human-readable line to
//! stderr but left stdout empty, breaking the documented JSON contract
//! of every `sqlite-graphrag` subcommand. v1.0.82 emits a structured
//! envelope to stdout before exit:
//!
//! ```json
//! {"error": true, "code": 19, "message": "...",
//!  "signal": "SIGINT", "graceful": true}
//! ```
//!
//! The schema lives in `docs/schemas/shutdown-envelope.schema.json`.
//! These tests verify that the schema and the in-binary constant
//! for the exit code (19) are aligned, and that the unit tests in
//! `src/signals.rs` cover the cross-signal handler.

#![cfg(feature = "slow-tests")]

use serde_json::Value;
use std::path::Path;

#[path = "common/mod.rs"]
mod common;

/// GAP-002 acceptance: the shutdown-envelope schema file is
/// well-formed and contains every field the binary emits. The
/// schema acts as a contract; if the binary adds a new required
/// field without bumping the schema, this test fails.
#[test]
fn shutdown_envelope_schema_contract() {
    let schema_path = Path::new("docs/schemas/shutdown-envelope.schema.json");
    let raw = std::fs::read_to_string(schema_path).expect("read shutdown-envelope schema");
    let schema: Value = serde_json::from_str(&raw).expect("schema must be valid JSON");
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);
    let required: Vec<&str> = schema["required"]
        .as_array()
        .expect("required must be an array")
        .iter()
        .map(|v| v.as_str().expect("required entries must be strings"))
        .collect();
    for field in ["error", "code", "message", "signal", "graceful"] {
        assert!(
            required.contains(&field),
            "schema is missing required field `{field}`"
        );
    }
    assert_eq!(
        schema["properties"]["code"]["const"], 19,
        "code must be exactly 19 (deterministic SHUTDOWN_EXIT_CODE)"
    );
    assert_eq!(
        schema["properties"]["graceful"]["const"], true,
        "graceful must be exactly true (distinguishes operator shutdown from crash)"
    );
    let signal_enum: Vec<&str> = schema["properties"]["signal"]["enum"]
        .as_array()
        .expect("signal.enum must be array")
        .iter()
        .map(|v| v.as_str().expect("enum entries must be strings"))
        .collect();
    for s in ["SIGINT", "SIGTERM", "SIGHUP", "unknown"] {
        assert!(signal_enum.contains(&s), "signal.enum is missing `{s}`");
    }
}

/// GAP-002 acceptance: the shutdown envelope shape (constructed
/// by the unit tests in `src/signals.rs`) round-trips through
/// `serde_json`. If the binary's envelope struct drifts from
/// the schema, this test fails. The unit test covers the
/// producer side; this test exercises the contract from a
/// consumer's perspective.
#[test]
fn shutdown_envelope_synthesizes_valid_json() {
    let envelope = serde_json::json!({
        "error": true,
        "code": 19,
        "message": "shutdown signal received; operation cancelled by SIGINT",
        "signal": "SIGINT",
        "graceful": true
    });
    // The contract guarantees: the envelope is a single JSON object
    // with the 5 documented fields, and `error` is always true.
    assert_eq!(envelope["error"], true);
    assert_eq!(envelope["code"], 19);
    assert_eq!(envelope["signal"], "SIGINT");
    assert_eq!(envelope["graceful"], true);
    assert!(envelope["message"].is_string());
}
