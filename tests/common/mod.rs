//! Test-only helpers shared by `tests/integration.rs`,
//! `tests/prd_compliance.rs`, and `tests/schema_migration_integration.rs`.
//!
//! The helpers in this module exist for ONE reason: the v1.0.76 binary
//! spawns `claude` or `codex` for every `remember` / `ingest` / `edit`,
//! and those CLIs require OAuth login plus a network round-trip. To run
//! the slow-tests hermetically on a CI runner we copy the two mock
//! scripts in `tests/mock-llm/` into a per-test temp directory and
//! prepend that directory to PATH so the binary finds the mocks first.
//!
//! `mock_llm_path` returns the directory; the caller wires it via
//! `Command::env("PATH", prepend_path)`.

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Copies the bundled `claude` and `codex` mock scripts into a fresh
/// temp directory and makes them executable. Returns the directory.
///
/// Tests should call this once and prepend the returned path to PATH
/// in every `Command` they build. The directory is independent of
/// the test's own `TempDir` because the mock binaries must survive
/// for the lifetime of the spawned `sqlite-graphrag` subprocess and
/// Rust drops `TempDir` instances eagerly when they go out of scope.
pub fn mock_llm_path() -> PathBuf {
    let dir = TempDir::new()
        .expect("mock_llm_path: TempDir must be creatable")
        .keep();

    for name in &["claude", "codex"] {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("mock-llm")
            .join(name);
        let dst = dir.join(name);
        fs::copy(&src, &dst)
            .unwrap_or_else(|e| panic!("mock_llm_path: copy {src:?} -> {dst:?} failed: {e}"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dst)
                .expect("mock_llm_path: stat dst")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&dst, perms).expect("mock_llm_path: chmod 755");
        }
    }

    dir
}

/// Prepends `mock_dir` to the inherited PATH and returns the new PATH
/// string. Use as `cmd.env("PATH", prepend_path(&mock_dir))`.
///
/// The function does NOT set PATH globally. It returns the composite
/// value for the caller to inject per-command, which keeps tests
/// parallel-safe.
pub fn prepend_path(mock_dir: &std::path::Path) -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    if current.is_empty() {
        return mock_dir.display().to_string();
    }
    format!("{}:{}", mock_dir.display(), current)
}
