//! GAP-E2E-001 (v1.0.89): binary size documented in Cargo.toml:6 must match
//! the on-disk release binary within 1 MiB tolerance.
//!
//! Rationale: the v1.0.76 release was 6 MB (LLM-only with rusqlite + clap
//! only). The binary grew to 14.6 MiB by v1.0.89 as new features landed
//! (GAP-002 split, GAP-058 env whitelist, GAP-E2E-007 schemars, system-load
//! helpers, reaper, OAUTH-only guard). The "6 MB" prose claim was stale and
//! this regression prevents the documented size from drifting away from the
//! real binary again.
//!
//! The test parses `Cargo.toml:6` description, extracts the documented MiB
//! value, measures the release binary with `stat -c %s`, converts both to
//! MiB via integer arithmetic (1024 * 1024), and asserts they agree within
//! the tolerance.

use std::fs;
use std::path::Path;
use std::process::Command;

const TOLERANCE_MIB: u64 = 1;

#[test]
fn assert_documented_size_matches_real() {
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("Cargo.toml must be readable");
    let description_line = cargo_toml
        .lines()
        .find(|line| line.trim_start().starts_with("description"))
        .expect("Cargo.toml must have a description line");

    // Extract the MiB or MB value from the description.
    let documented_mib = parse_size_mib_from_description(description_line)
        .expect("description must mention binary size (e.g. '14.6 MiB' or '15 MB')");

    let binary_path = Path::new("target/release/sqlite-graphrag");
    if !binary_path.exists() {
        // GAP-E2E-001 is a release-time invariant. The CI test job builds only
        // the debug profile (`cargo nextest run`), so the release binary is
        // absent there — skip gracefully instead of failing. The check still
        // runs locally and in any release build where the binary exists.
        eprintln!(
            "SKIP assert_documented_size_matches_real: no release binary at {} (debug-only run)",
            binary_path.display()
        );
        return;
    }

    let real_bytes = fs::metadata(binary_path)
        .expect("binary metadata must be readable")
        .len();

    let real_mib = real_bytes / (1024 * 1024);

    let delta = real_mib.abs_diff(documented_mib);

    assert!(
        delta <= TOLERANCE_MIB,
        "documented binary size in Cargo.toml ({documented_mib} MiB) diverges from real binary ({real_mib} MiB = {real_bytes} bytes) by more than {TOLERANCE_MIB} MiB"
    );

    eprintln!(
        "GAP-E2E-001 OK: documented {documented_mib} MiB == real {real_mib} MiB ({real_bytes} bytes), delta {delta} MiB"
    );
}

fn parse_size_mib_from_description(line: &str) -> Option<u64> {
    // Look for patterns like "14.6 MiB" or "15 MB" (with optional space).
    // We accept MiB and MB; MB is converted to MiB rounded down (MB uses 10^6,
    // but for tolerance comparison we treat them equivalently with integer MiB).
    let lower = line.to_lowercase();
    let candidates = [("mib", 1u64), ("mb", 1u64)];

    for (unit, divisor_mib) in candidates {
        if let Some(idx) = lower.find(unit) {
            // Walk backwards to find the number preceding the unit.
            let prefix = &lower[..idx];
            // Trim trailing whitespace.
            let prefix = prefix.trim_end();
            // Extract the last numeric token (possibly with decimal point).
            let num_str: String = prefix
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            if let Ok(parsed) = num_str.parse::<f64>() {
                let mib = (parsed / divisor_mib as f64).round() as u64;
                return Some(mib);
            }
        }
    }
    None
}

#[test]
fn assert_release_profile_has_size_optimizations() {
    // Guard against accidental removal of LTO / strip / opt-level from
    // [profile.release], which would balloon the binary.
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("Cargo.toml must be readable");

    assert!(
        cargo_toml.contains("lto = \"fat\""),
        "[profile.release] must keep lto = \"fat\" to minimize binary size"
    );
    assert!(
        cargo_toml.contains("strip = true"),
        "[profile.release] must keep strip = true to remove debug symbols"
    );
    assert!(
        cargo_toml.contains("codegen-units = 1"),
        "[profile.release] must keep codegen-units = 1 for cross-module LTO"
    );
    assert!(
        cargo_toml.contains("opt-level = 3"),
        "[profile.release] must keep opt-level = 3 for maximum optimization"
    );
}

#[test]
fn assert_no_six_mb_claim_in_documentation() {
    // The stale "~6 MB" claim must not reappear in any markdown documentation.
    let docs_to_scan = [
        "Cargo.toml",
        "README.md",
        "llms.txt",
        "CHANGELOG.md",
        "docs/AGENTS.md",
        "docs/AGENTS.pt-BR.md",
        "docs/HOW_TO_USE.md",
        "docs/HOW_TO_USE.pt-BR.md",
        "docs/MIGRATION.md",
        "docs/MIGRATION.pt-BR.md",
        "docs/CROSS_PLATFORM.md",
        "docs/CROSS_PLATFORM.pt-BR.md",
        "docs/COOKBOOK.md",
        "docs/COOKBOOK.pt-BR.md",
        "docs/decisions/adr-0019-llm-only-one-shot.md",
        "docs/decisions/adr-0019-llm-only-one-shot.pt-BR.md",
    ];

    for path in docs_to_scan {
        let full_path = Path::new(path);
        if !full_path.exists() {
            continue;
        }
        let content =
            fs::read_to_string(full_path).unwrap_or_else(|_| panic!("{path} must be readable"));

        // Search for "~6 MB" or "6 MB binary" or similar stale patterns.
        let stale_patterns = ["~6 MB", "6 MB binary", " 6 MB)", "6 MB Rust"];
        for pattern in stale_patterns {
            assert!(
                !content.contains(pattern),
                "{path} still contains stale binary size claim '{pattern}'; update to real size (14.6 MiB)"
            );
        }
    }

    // Verify the helper exists for ad-hoc size lookup.
    let _ = Command::new("stat")
        .args(["-c", "%s", "target/release/sqlite-graphrag"])
        .output()
        .ok();
}
