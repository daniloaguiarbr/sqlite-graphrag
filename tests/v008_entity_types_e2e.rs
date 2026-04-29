//! E2E regression test for V008 entity types (`organization`, `location`, `date`).
//!
//! Confirms the V008 schema migration's CHECK constraint accepts the three
//! new BERT NER entity types end-to-end through the public CLI surface.
//!
//! This test addresses a long-standing gap (deferred since v1.0.26) where
//! the only assertions for these types lived in `src/extraction.rs` unit
//! tests and never exercised the CLI binary against a real schema.

use assert_cmd::Command;
use serde_json::Value;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

fn cmd(temp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").expect("binary present in target/");
    c.env_clear()
        .env("HOME", temp.path())
        .env("SQLITE_GRAPHRAG_HOME", temp.path())
        .env(
            "SQLITE_GRAPHRAG_CACHE_DIR",
            temp.path().join("cache").to_string_lossy().to_string(),
        )
        .env("SQLITE_GRAPHRAG_LANG", "en")
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "warn")
        .current_dir(temp.path());
    c
}

/// Ensures the V008 entity types persist successfully via `remember --entities-file`
/// without triggering the CHECK constraint introduced in `migrations/V008__expand_entity_types.sql`.
#[test]
#[serial]
fn v008_entity_types_organization_location_date_round_trip() {
    let temp = TempDir::new().expect("tempdir");

    // Skip the test gracefully if the embedding model is unavailable in the
    // CI cache; the suite would otherwise download ~460 MB on every run.
    let model_cache = std::env::var("SQLITE_GRAPHRAG_CACHE_DIR").ok().or_else(|| {
        directories::ProjectDirs::from("", "", "sqlite-graphrag")
            .map(|p| p.cache_dir().to_string_lossy().to_string())
    });
    let has_model = model_cache
        .as_ref()
        .map(|p| std::path::Path::new(p).join("models").exists())
        .unwrap_or(false);
    if !has_model && std::env::var("SQLITE_GRAPHRAG_FORCE_DOWNLOAD").is_err() {
        eprintln!(
            "skipping v008_entity_types_e2e: embedding model not cached. \
             Set SQLITE_GRAPHRAG_FORCE_DOWNLOAD=1 to download (~460 MB)."
        );
        return;
    }

    // Step 1: init the database in the tempdir CWD.
    let init = cmd(&temp).arg("init").output().expect("init runs");
    assert!(init.status.success(), "init failed: {init:?}");

    // Step 2: create an entities-file payload that exercises all three V008 types.
    let entities_path = temp.path().join("entities.json");
    fs::write(
        &entities_path,
        r#"[
            {"name": "OpenAI", "entity_type": "organization"},
            {"name": "São Paulo", "entity_type": "location"},
            {"name": "2026-04-29", "entity_type": "date"}
        ]"#,
    )
    .expect("write entities-file");

    // Step 3: persist a memory associated with these entities.
    let remember = cmd(&temp)
        .args([
            "remember",
            "--name",
            "v008-regression",
            "--type",
            "reference",
            "--description",
            "V008 entity types regression",
            "--body",
            "OpenAI and São Paulo on 2026-04-29.",
            "--entities-file",
        ])
        .arg(&entities_path)
        .arg("--skip-extraction")
        .output()
        .expect("remember runs");
    assert!(
        remember.status.success(),
        "remember failed: stderr={}",
        String::from_utf8_lossy(&remember.stderr)
    );

    // Step 4: export the graph and confirm all three entity types are present.
    let graph = cmd(&temp)
        .args(["graph", "--format", "json"])
        .output()
        .expect("graph runs");
    assert!(graph.status.success(), "graph export failed: {graph:?}");

    let payload: Value =
        serde_json::from_slice(&graph.stdout).expect("graph output is valid JSON");
    let nodes = payload
        .get("nodes")
        .and_then(|n| n.as_array())
        .expect("graph response has nodes array");

    let mut found_org = false;
    let mut found_loc = false;
    let mut found_date = false;
    for node in nodes {
        let kind = node
            .get("type")
            .or_else(|| node.get("kind"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match kind {
            "organization" => found_org = true,
            "location" => found_loc = true,
            "date" => found_date = true,
            _ => {}
        }
    }
    assert!(found_org, "expected entity type `organization` in graph");
    assert!(found_loc, "expected entity type `location` in graph");
    assert!(found_date, "expected entity type `date` in graph");
}
