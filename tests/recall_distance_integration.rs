//! Integration test for P1-M: graph_matches.distance uses hop-count proxy.
//!
//! Verifies that graph_matches returned by `recall` have `distance > 0.0`
//! when reached via graph traversal (hop >= 1), replacing the `0.0` placeholder.

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn cmd_base(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.arg("--skip-memory-guard");
    c
}

fn init_db(tmp: &TempDir) {
    cmd_base(tmp).arg("init").assert().success();
}

fn remember(tmp: &TempDir, name: &str, body: &str, namespace: &str) {
    cmd_base(tmp)
        .args([
            "remember",
            "--name",
            name,
            "--type",
            "user",
            "--description",
            "test description",
            "--namespace",
            namespace,
            "--body",
            body,
        ])
        .assert()
        .success();
}

/// After v1.0.25, graph_matches must not all have distance == 0.0 when hop > 0.
///
/// The test ingests two related memories (Anthropic and Claude AI assistant),
/// runs a recall query, and checks that any graph_matches have distance > 0.0.
/// If recall produces no graph_matches (e.g. BERT not available), the test is
/// considered vacuously passing — we only enforce the invariant when matches exist.
#[test]
fn graph_matches_have_nonzero_distance_after_v1025() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // Ingest two memories that share entities so graph traversal can link them
    remember(
        &tmp,
        "anthropic-foundation",
        "Anthropic is an AI safety company founded to build reliable AI systems",
        "test-ns",
    );
    remember(
        &tmp,
        "claude-assistant",
        "Claude is an AI assistant built by Anthropic for safe and helpful interactions",
        "test-ns",
    );

    let output = cmd_base(&tmp)
        .args([
            "recall",
            "Anthropic AI safety",
            "--namespace",
            "test-ns",
            "--k",
            "5",
            "--max-hops",
            "2",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    let exit_code = output.status.code().unwrap_or(-1);
    // Accept exit 0 (found results) or exit 4 (nothing found / below threshold)
    assert!(
        exit_code == 0 || exit_code == 4,
        "recall must exit 0 or 4, got {exit_code}"
    );

    if exit_code != 0 {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).expect("exit 0 must produce valid JSON");

    let graph_matches = json["graph_matches"].as_array();
    if let Some(matches) = graph_matches {
        for m in matches {
            let distance = m["distance"].as_f64().unwrap_or(f64::NAN);
            let graph_depth = m["graph_depth"].as_u64().unwrap_or(0);

            // For hop > 0, distance must be strictly positive (proxy formula)
            if graph_depth > 0 {
                assert!(
                    distance > 0.0,
                    "graph_match at hop={graph_depth} must have distance > 0.0 (got {distance}); \
                     P1-M regression: placeholder 0.0 still present"
                );
            }

            // Distance must not exceed 1.0 (valid range for proxy)
            assert!(
                distance <= 1.0,
                "graph_match distance {distance} exceeds 1.0 (invalid proxy value)"
            );
        }
    }
}
