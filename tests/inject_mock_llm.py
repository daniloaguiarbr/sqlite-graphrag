#!/usr/bin/env python3
"""Inject mock LLM PATH into every Rust test file that spawns the binary.

Strategy (single-pass with chained-call handling to avoid recursive substitution):
  1. The regex matches `Command::cargo_bin("sqlite-graphrag")` followed
     by an OPTIONAL `.unwrap()` or `.expect(...)` chain. The whole match
     is replaced with `sgr_cmd()` because `sgr_cmd()` already calls
     `.expect(...)` internally. Without this, a leftover `.unwrap()` on
     a `Command` would not compile.
  2. Add `mod common;` and the `sgr_cmd()` helper AFTER the substitution
     so the helper's own `Command::cargo_bin("sqlite-graphrag")` body
     is not re-substituted (the helper is added in a code region that
     the regex already consumed).
  3. Use a sentinel-free design — the regex is single-pass, no
     intermediate markers needed.
"""

import re
import sys
from pathlib import Path

HELPER_TEMPLATE = """
/// Builds a fresh `Command` with the mock LLM PATH prepended.
///
/// v1.0.76 spawns `claude` or `codex` on every `remember` / `ingest` /
/// `edit`. The bundled mocks under `tests/mock-llm/` return a fixed
/// 384-dim zero vector so the binary finishes without a real OAuth
/// login. The mock directory is leaked (no TempDir cleanup) so the
/// spawned subprocess always finds the mocks.
fn sgr_cmd() -> Command {
    let mock_dir = common::mock_llm_path();
    let mut c = Command::cargo_bin("sqlite-graphrag")
        .expect("sqlite-graphrag binary not found");
    c.env("PATH", common::prepend_path(&mock_dir));
    c
}
"""

MOD_LINE = '#[path = "common/mod.rs"]\nmod common;'

# Pattern: `Command::cargo_bin("sqlite-graphrag")` optionally chained
# with `.unwrap()` or `.expect("...")`. The expect string is greedy
# enough to swallow a closing parenthesis and quote.
INVOKE_RE = re.compile(
    r'Command::cargo_bin\(\s*"sqlite-graphrag"\s*\)'
    r'(?:\s*\.\s*unwrap\(\))?'
    r'(?:\s*\.\s*expect\(\s*"[^"]*"\s*\))?'
)


def inject(path: Path) -> bool:
    text = path.read_text()
    if "Command::cargo_bin(\"sqlite-graphrag\")" not in text:
        return False

    original = text

    # PASS 1: substitute every call site with `sgr_cmd()`. We do this
    # BEFORE injecting the helper so the helper body (which itself
    # contains `Command::cargo_bin("sqlite-graphrag")`) is not seen by
    # the regex.
    text, n_subs = INVOKE_RE.subn("sgr_cmd()", text)

    # PASS 2: add `mod common;` if missing.
    if 'mod common;' not in text:
        m = re.search(r"^(?:use [^\n]+\n)+", text, re.MULTILINE)
        if m:
            insert_at = m.end()
        else:
            lines = text.split("\n")
            for i, line in enumerate(lines):
                if line.strip() and not line.startswith("#!"):
                    insert_at = sum(len(l) + 1 for l in lines[: i + 1])
                    break
            else:
                insert_at = 0
        text = text[:insert_at] + "\n" + MOD_LINE + "\n" + text[insert_at:]

    # PASS 3: add `sgr_cmd()` helper if not present.
    if "fn sgr_cmd()" not in text:
        m = re.search(r"^(use [^\n]+\n)+", text, re.MULTILINE)
        if m:
            insert_at = m.end()
        else:
            insert_at = 0
        text = text[:insert_at] + HELPER_TEMPLATE + "\n" + text[insert_at:]

    if n_subs == 0 and text == original:
        return False
    path.write_text(text)
    return True


def main() -> int:
    tests_dir = Path("tests")
    changed = 0
    for f in sorted(tests_dir.glob("*.rs")):
        if f.name == "common" or f.name == "mod.rs":
            continue
        if f.is_dir():
            continue
        if inject(f):
            print(f"  patched: {f}")
            changed += 1
    print(f"\n{changed} test file(s) patched.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
