//! Verifies that every `sqlite-graphrag` invocation in README.md and README.pt-BR.md
//! actually runs against the real CLI binary. Fixes finding C3 of the v1.0.31 audit:
//! several documented examples used flag names that no longer exist (e.g. `--new`
//! before the alias was added), positional-vs-flag mismatches, etc.
//!
//! HOW IT WORKS:
//! - Parses fenced ```bash blocks from both READMEs at compile time via `include_str!`.
//! - For each block, extracts lines starting with `sqlite-graphrag ` (after optional
//!   `VAR=value ` shell variable prefixes).
//! - Executes them sequentially in a single isolated `TempDir` per README.
//! - Skips entire blocks containing pipes (`|`), redirects (`>`, `<`), command
//!   substitution (`$(`, backticks), or other shell composition that cannot be run
//!   directly via `assert_cmd::Command`.
//! - Skips blocks preceded by `<!-- skip-test -->` HTML comment for legitimate
//!   cases (commands that need external state, planned but unimplemented, etc.).
//! - Treats every line as a hard requirement: any non-zero exit fails the test.
//!
//! WHEN ADDING NEW DOCUMENTED EXAMPLES:
//! - Make sure the example actually runs in isolation (or arrange prerequisites
//!   in earlier lines of the same block).
//! - If shell composition is required, mark the block with `<!-- skip-test -->`.

use assert_cmd::Command;
use serial_test::serial;
use std::path::PathBuf;
use tempfile::TempDir;

const README_EN: &str = include_str!("../README.md");
const README_PT: &str = include_str!("../README.pt-BR.md");

/// One contiguous ```bash block in a README, with metadata used to decide
/// whether and how to execute it.
#[derive(Debug)]
struct BashBlock {
    /// 1-based line number where the opening ```bash fence sits.
    source_line: usize,
    /// Lines inside the fence (without the fences themselves).
    lines: Vec<String>,
    /// True when an HTML comment `<!-- skip-test -->` precedes the opening fence.
    skip: bool,
}

/// State machine that walks the README line-by-line and extracts every ```bash
/// fenced block. Detects the `<!-- skip-test -->` marker on the line immediately
/// preceding the opening fence (blank lines between the marker and the fence
/// are tolerated).
fn extract_bash_blocks(source: &str) -> Vec<BashBlock> {
    let mut blocks = Vec::new();
    let mut in_block = false;
    let mut current_lines: Vec<String> = Vec::new();
    let mut current_start: usize = 0;
    let mut current_skip: bool = false;

    // Look-back buffer: most recent non-blank line seen *before* the opening fence.
    let mut last_non_blank: Option<String> = None;

    for (idx, raw_line) in source.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw_line.trim_end_matches('\r');

        if !in_block {
            if trimmed.trim_start().starts_with("```bash") {
                in_block = true;
                current_start = line_no;
                current_lines.clear();
                // Accept either the bare marker `<!-- skip-test -->` or the
                // explanatory form `<!-- skip-test: <reason> -->` so authors
                // can document why a block is skipped.
                current_skip = matches!(
                    last_non_blank.as_deref().map(str::trim),
                    Some(s) if s.contains("<!-- skip-test")
                );
            } else if !trimmed.trim().is_empty() {
                last_non_blank = Some(trimmed.to_string());
            }
        } else if trimmed.trim_start().starts_with("```") {
            // Closing fence.
            blocks.push(BashBlock {
                source_line: current_start,
                lines: std::mem::take(&mut current_lines),
                skip: current_skip,
            });
            in_block = false;
            current_skip = false;
            // The closing fence itself counts as "non-blank" for look-back purposes;
            // but a new <!-- skip-test --> marker would appear on a fresh line afterwards.
            last_non_blank = Some(trimmed.to_string());
        } else {
            current_lines.push(trimmed.to_string());
        }
    }

    blocks
}

/// True when a bash block's body contains shell composition that cannot be
/// faithfully reproduced through `assert_cmd::Command`. Such blocks are
/// silently skipped — they are documentation of pipelines, not pure CLI calls.
fn block_uses_shell_composition(block: &BashBlock) -> bool {
    for line in &block.lines {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        // Heuristics for shell features we cannot run via assert_cmd directly.
        if l.contains('|')
            || l.contains('>')
            || l.contains('<')
            || l.contains("$(")
            || l.contains('`')
            || l.contains("&&")
            || l.contains("||")
            || l.contains(';')
        {
            return true;
        }
    }
    false
}

/// Splits a logical command line into argv tokens. Handles `\\` line
/// continuations (already stitched by the caller), simple `"..."` and `'...'`
/// quoting, and ignores leading `VAR=value` shell variable assignments. Returns
/// only the argv that should be passed to `assert_cmd::Command`, *without* the
/// leading binary name.
///
/// Returns `None` when the line is not a `sqlite-graphrag` invocation we can run
/// (e.g. it starts with `cargo install`, `xh`, etc.).
fn parse_executable_line(line: &str) -> Option<Vec<String>> {
    let stripped = line.trim();
    if stripped.is_empty() || stripped.starts_with('#') {
        return None;
    }

    // Tokenize once; we'll discard leading VAR=value tokens after.
    let tokens = tokenize_shell(stripped)?;

    let mut iter = tokens.into_iter().peekable();

    // Skip leading `VAR=value` shell-variable assignments.
    while let Some(tok) = iter.peek() {
        if is_shell_var_assignment(tok) {
            iter.next();
        } else {
            break;
        }
    }

    // First non-assignment token must be the binary name we test.
    let first = iter.next()?;
    if first != "sqlite-graphrag" {
        return None;
    }

    Some(iter.collect())
}

fn is_shell_var_assignment(token: &str) -> bool {
    if let Some(eq) = token.find('=') {
        // Must look like NAME=value, where NAME is uppercase letters/digits/underscores.
        let name = &token[..eq];
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    } else {
        false
    }
}

/// Minimal shell tokenizer: handles unquoted whitespace splits, `"..."` and
/// `'...'` quoted strings (no escape sequence beyond the trivial `\"` inside
/// double quotes). Returns `None` on unbalanced quotes.
fn tokenize_shell(input: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            '\\' if in_double => {
                if let Some(&next) = chars.peek() {
                    current.push(next);
                    chars.next();
                } else {
                    current.push('\\');
                }
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }

    if in_single || in_double {
        return None;
    }
    if !current.is_empty() {
        out.push(current);
    }
    Some(out)
}

/// Joins lines that end with `\\` (shell line continuation) into single logical
/// lines, mirroring how a shell would see them.
fn join_continuations(lines: &[String]) -> Vec<String> {
    let mut joined: Vec<String> = Vec::new();
    let mut buffer = String::new();
    let mut continuing = false;
    for line in lines {
        let trimmed = line.trim_end();
        if let Some(without_slash) = trimmed.strip_suffix('\\') {
            buffer.push_str(without_slash);
            buffer.push(' ');
            continuing = true;
        } else if continuing {
            buffer.push_str(trimmed);
            joined.push(std::mem::take(&mut buffer));
            continuing = false;
        } else {
            joined.push(trimmed.to_string());
        }
    }
    if !buffer.is_empty() {
        joined.push(buffer);
    }
    joined
}

/// Returns the path to the compiled CLI binary under test.
fn cli_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sqlite-graphrag"))
}

/// Builds a fresh `assert_cmd::Command` pointing at the test binary, with the
/// database and cache directories pinned to a TempDir for isolation. Mirrors the
/// pattern used in `tests/cookbook_recipes.rs`.
fn cmd_in(dir: &TempDir) -> Command {
    let mut c = Command::new(cli_bin());
    c.env_clear()
        .env(
            "SQLITE_GRAPHRAG_DB_PATH",
            dir.path().join("graphrag.sqlite"),
        )
        .env("SQLITE_GRAPHRAG_CACHE_DIR", dir.path().join("cache"))
        // Disable RAM guard so CI hosts with low free memory still run the suite.
        .arg("--skip-memory-guard");
    c
}

/// Executes every `sqlite-graphrag` line in a block sequentially. Returns
/// `Err(message)` on the first failure, including stderr for diagnosis.
fn run_block_in_tempdir(block: &BashBlock, dir: &TempDir, source: &str) -> Result<usize, String> {
    let logical_lines = join_continuations(&block.lines);
    let mut executed = 0usize;

    for (offset, line) in logical_lines.iter().enumerate() {
        let argv = match parse_executable_line(line) {
            Some(argv) => argv,
            None => continue,
        };

        let output = cmd_in(dir).args(&argv).output().map_err(|e| {
            format!(
                "{source} block @ line {} (cmd #{offset}): spawn failed: {e}",
                block.source_line
            )
        })?;

        if !output.status.success() {
            return Err(format!(
                "{source} block @ line {} (cmd #{offset}) failed (exit {:?})\n  cmd: sqlite-graphrag {}\n  stdout:\n{}\n  stderr:\n{}",
                block.source_line,
                output.status.code(),
                argv.join(" "),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        executed += 1;
    }

    Ok(executed)
}

/// Runs every executable bash block in `source`, accumulating counters and
/// failing the test if any block exits non-zero.
fn run_all_blocks(readme_label: &str, content: &str, min_blocks_expected: usize) {
    let blocks = extract_bash_blocks(content);
    let dir = TempDir::new().expect("tempdir");

    // Always run `init` once so subsequent commands have a usable database.
    cmd_in(&dir).arg("init").assert().success();

    let mut tested_blocks = 0usize;
    let mut tested_lines = 0usize;
    let mut skipped_marker = 0usize;
    let mut skipped_composition = 0usize;
    let mut skipped_no_cli = 0usize;

    for block in &blocks {
        if block.skip {
            skipped_marker += 1;
            continue;
        }
        if block_uses_shell_composition(block) {
            skipped_composition += 1;
            continue;
        }

        // Pre-check: does this block contain at least one `sqlite-graphrag` line?
        let has_cli = join_continuations(&block.lines)
            .iter()
            .any(|l| parse_executable_line(l).is_some());
        if !has_cli {
            skipped_no_cli += 1;
            continue;
        }

        match run_block_in_tempdir(block, &dir, readme_label) {
            Ok(n) => {
                tested_blocks += 1;
                tested_lines += n;
            }
            Err(msg) => panic!("{msg}"),
        }
    }

    eprintln!(
        "{readme_label}: {tested_blocks} blocks / {tested_lines} commands tested, \
         {skipped_marker} skip-marker, {skipped_composition} shell-composition, \
         {skipped_no_cli} no-cli (total {} blocks)",
        blocks.len()
    );

    assert!(
        tested_blocks >= min_blocks_expected,
        "{readme_label}: expected at least {min_blocks_expected} executable blocks, ran {tested_blocks}"
    );
}

#[test]
#[serial]
fn readme_en_bash_examples_all_run() {
    run_all_blocks("README.md", README_EN, 10);
}

#[test]
#[serial]
fn readme_pt_bash_examples_all_run() {
    run_all_blocks("README.pt-BR.md", README_PT, 10);
}

// -------------------------------------------------------------------------
// Unit tests for the small parsers above. These keep the harness honest.
// -------------------------------------------------------------------------

#[cfg(test)]
mod parser_tests {
    use super::*;

    #[test]
    fn tokenize_handles_quotes_and_whitespace() {
        let toks = tokenize_shell(r#"foo --name "with space" --body 'single quoted'"#).unwrap();
        assert_eq!(
            toks,
            vec!["foo", "--name", "with space", "--body", "single quoted",]
        );
    }

    #[test]
    fn parse_strips_leading_env_assignments() {
        let argv =
            parse_executable_line("FOO=bar BAR=baz sqlite-graphrag recall hello --k 5").unwrap();
        assert_eq!(argv, vec!["recall", "hello", "--k", "5"]);
    }

    #[test]
    fn parse_returns_none_for_non_cli_line() {
        assert!(parse_executable_line("cargo install sqlite-graphrag").is_none());
        assert!(parse_executable_line("# comment").is_none());
        assert!(parse_executable_line("").is_none());
    }

    #[test]
    fn join_continuations_stitches_backslash_lines() {
        let lines = vec![
            "sqlite-graphrag remember \\".to_string(),
            "  --name x \\".to_string(),
            "  --body y".to_string(),
        ];
        let joined = join_continuations(&lines);
        assert_eq!(joined.len(), 1);
        assert!(joined[0].contains("--name x"));
        assert!(joined[0].contains("--body y"));
    }

    #[test]
    fn shell_composition_detected() {
        let b = BashBlock {
            source_line: 1,
            lines: vec!["sqlite-graphrag recall foo | jaq .".to_string()],
            skip: false,
        };
        assert!(block_uses_shell_composition(&b));
    }

    #[test]
    fn extract_bash_blocks_finds_skip_marker() {
        let src = "intro\n<!-- skip-test -->\n```bash\nfoo\n```\n";
        let blocks = extract_bash_blocks(src);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].skip);
    }

    #[test]
    fn extract_bash_blocks_finds_skip_marker_with_reason() {
        let src = "intro\n<!-- skip-test: needs network -->\n```bash\nfoo\n```\n";
        let blocks = extract_bash_blocks(src);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].skip);
    }

    #[test]
    fn extract_bash_blocks_default_no_skip() {
        let src = "intro\n```bash\nfoo\n```\n";
        let blocks = extract_bash_blocks(src);
        assert_eq!(blocks.len(), 1);
        assert!(!blocks[0].skip);
    }
}
