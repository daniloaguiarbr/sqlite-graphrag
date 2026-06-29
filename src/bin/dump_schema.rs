//! Binary to regenerate `docs/schemas/*.schema.json` from the Rust types.
//! GAP-E2E-007 (v1.0.89): schemars 0.8 + Must-Ignore policy (RFC 7493 I-JSON).
//!
//! IDEMPOTENT: running it twice produces byte-identical output because schemars
//! normalizes the key order (SchemaObject is BTreeMap-backed) and this binary applies
//! deterministic transformations (Draft 2020-12 + additionalProperties: true).
//!
//! Usage:
//!   cargo run --bin dump_schema -- health    # regenerates health.schema.json

use schemars::schema_for;
use serde_json::Value;
use std::path::PathBuf;

/// Draft 2020-12 schema URI per docs_rules/rules_rust_json_e_ndjson.md line 555.
const DRAFT_2020_12: &str = "https://json-schema.org/draft/2020-12/schema";

/// Applies Must-Ignore (`additionalProperties: true`) recursively to all
/// objects in the schema. Rule line 537: ADOPT Must-Ignore in evolving APIs.
fn apply_must_ignore(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if map.contains_key("properties") {
                // Object com properties definido: set Must-Ignore.
                map.insert("additionalProperties".into(), Value::Bool(true));
            }
            for (_, v) in map.iter_mut() {
                apply_must_ignore(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                apply_must_ignore(v);
            }
        }
        _ => {}
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "health".to_string());

    match cmd.as_str() {
        "health" => {
            let schema = schema_for!(sqlite_graphrag::commands::health::HealthResponse);
            let mut value = serde_json::to_value(&schema)?;

            // Bump Draft para 2020-12.
            if let Value::Object(map) = &mut value {
                map.insert("$schema".into(), Value::String(DRAFT_2020_12.to_string()));
            }

            // Must-Ignore: percorre recursivamente todos os objects com `properties`.
            apply_must_ignore(&mut value);

            let json = serde_json::to_string_pretty(&value)?;
            let path = PathBuf::from("docs/schemas/health.schema.json");
            std::fs::write(&path, &json)?;
            println!("Regenerated {} ({} bytes)", path.display(), json.len());
        }
        "enrich-status" => {
            let schema = schema_for!(sqlite_graphrag::commands::enrich::EnrichStatus);
            let mut value = serde_json::to_value(&schema)?;

            // Bump Draft para 2020-12.
            if let Value::Object(map) = &mut value {
                map.insert("$schema".into(), Value::String(DRAFT_2020_12.to_string()));
            }

            // Must-Ignore: percorre recursivamente todos os objects com `properties`.
            apply_must_ignore(&mut value);

            let json = serde_json::to_string_pretty(&value)?;
            let path = PathBuf::from("docs/schemas/enrich-status.schema.json");
            std::fs::write(&path, &json)?;
            println!("Regenerated {} ({} bytes)", path.display(), json.len());
        }
        other => {
            eprintln!("Unknown schema target: {other}");
            eprintln!("Usage: dump_schema <health|enrich-status>");
            std::process::exit(2);
        }
    }
    Ok(())
}
