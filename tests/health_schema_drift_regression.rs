//! GAP-E2E-007 (v1.0.89): the health schema must cover all 36 keys emitted by
//! HealthResponse AND must use the Must-Ignore policy (RFC 7493 I-JSON), not Must-Validate.

use std::fs;

#[test]
fn assert_all_health_keys_in_schema() {
    let schema_str =
        fs::read_to_string("docs/schemas/health.schema.json").expect("schema file must exist");
    let schema: serde_json::Value =
        serde_json::from_str(&schema_str).expect("schema must be valid JSON");
    let properties = schema["properties"]
        .as_object()
        .expect("schema must have properties");

    // 23 chaves sempre presentes + 13 condicionais = 36 chaves totais.
    let required_keys = [
        // Sempre presentes (23)
        "status",
        "namespace",
        "integrity",
        "integrity_ok",
        "schema_ok",
        "vec_memories_ok",
        "vec_memories_missing",
        "vec_memories_orphaned",
        "vec_entities_ok",
        "vec_chunks_ok",
        "fts_ok",
        "fts_query_ok",
        "model_ok",
        "counts",
        "db_path",
        "db_size_bytes",
        "schema_version",
        "missing_entities",
        "wal_size_mb",
        "journal_mode",
        "sqlite_version",
        "checks",
        "elapsed_ms",
        // Campos condicionais (Option<T> via skip_serializing_if, 13)
        "mentions_ratio",
        "mentions_warning",
        "top_relation",
        "top_relation_ratio",
        "applies_to_ratio",
        "relation_concentration_warning",
        "non_normalized_count",
        "normalization_warning",
        "super_hub_count",
        "super_hub_warning",
        "top_hub_entity",
        "top_hub_degree",
        "hub_warning",
    ];

    for key in required_keys {
        assert!(
            properties.contains_key(key),
            "health.schema.json must contain key '{key}' (GAP-E2E-007 drift detected)"
        );
    }
    assert!(
        properties.len() >= required_keys.len(),
        "schema has only {} keys, expected at least {}",
        properties.len(),
        required_keys.len()
    );
}

#[test]
fn assert_must_ignore_policy_active() {
    let schema_str =
        fs::read_to_string("docs/schemas/health.schema.json").expect("schema file must exist");
    let schema: serde_json::Value =
        serde_json::from_str(&schema_str).expect("schema must be valid JSON");
    let additional = schema["additionalProperties"].as_bool();
    assert_eq!(
        additional,
        Some(true),
        "health.schema.json must use additionalProperties: true (Must-Ignore per RFC 7493 I-JSON) — currently: {additional:?}"
    );
}

#[test]
fn assert_draft_2020_12() {
    let schema_str =
        fs::read_to_string("docs/schemas/health.schema.json").expect("schema file must exist");
    let schema: serde_json::Value =
        serde_json::from_str(&schema_str).expect("schema must be valid JSON");
    let schema_uri = schema["$schema"].as_str().unwrap_or("");
    assert!(
        schema_uri.contains("draft/2020-12"),
        "health.schema.json must declare Draft 2020-12 per docs_rules/rules_rust_json_e_ndjson.md line 555 — currently: {schema_uri}"
    );
}

#[test]
fn assert_dump_schema_is_idempotent() {
    // Executa o binário duas vezes e verifica que o checksum do schema é idêntico.
    let output1 = std::process::Command::new("cargo")
        .args(["run", "--quiet", "--bin", "dump_schema", "--", "health"])
        .output()
        .expect("first dump_schema invocation must succeed");
    assert!(
        output1.status.success(),
        "first dump_schema failed: {}",
        String::from_utf8_lossy(&output1.stderr)
    );
    let checksum1 = b3sum_file("docs/schemas/health.schema.json");

    let output2 = std::process::Command::new("cargo")
        .args(["run", "--quiet", "--bin", "dump_schema", "--", "health"])
        .output()
        .expect("second dump_schema invocation must succeed");
    assert!(
        output2.status.success(),
        "second dump_schema failed: {}",
        String::from_utf8_lossy(&output2.stderr)
    );
    let checksum2 = b3sum_file("docs/schemas/health.schema.json");

    assert_eq!(
        checksum1, checksum2,
        "dump_schema must be idempotent (GAP-E2E-007)"
    );
}

fn b3sum_file(path: &str) -> String {
    let content = std::fs::read(path).expect("schema file must be readable");
    let mut hasher = blake3::Hasher::new();
    hasher.update(&content);
    hasher.finalize().to_hex().to_string()
}
