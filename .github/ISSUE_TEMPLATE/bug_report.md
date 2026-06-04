---
name: Bug report
about: Report a reproducible bug or unexpected behavior
title: "[BUG] "
labels: ["bug", "triage"]
assignees: []
---

## Summary

<!-- One-paragraph description of the bug -->

## Reproduction

### Environment
- sqlite-graphrag version: `sqlite-graphrag --version` output
- Operating system: (e.g. Fedora 41, Ubuntu 24.04, macOS 15, Windows 11)
- Architecture: (e.g. x86_64, aarch64)
- MSRV toolchain: `rustc --version` (when built from source)
- Database mode: (default `graphrag.sqlite` in CWD, or custom via `--db`)

### Steps
1.
2.
3.

### Expected
<!-- What you expected to happen -->

### Actual
<!-- What actually happened, with full stderr and exit code -->

```
$ sqlite-graphrag ...
[PASTE OUTPUT HERE]
```

## Logs

```bash
# Set SQLITE_GRAPHRAG_LOG_FORMAT=json to get machine-parseable logs
SQLITE_GRAPHRAG_LOG_FORMAT=json SQLITE_GRAPHRAG_LOG_LEVEL=debug sqlite-graphrag <cmd> 2>&1 | jaq '.'
```

Paste relevant trace here:

```
[PASTE LOGS HERE]
```

## Cross-References
- Related gap: (if any, e.g. G28, G29)
- Related memory in GraphRAG: (if you have access, e.g. `g28-process-proliferation`)
- Related discussion: (GitHub issue, Discord, email, etc.)

## Acceptance Criteria

This bug is considered FIXED when:
- [ ] The exact reproduction steps above no longer exhibit the broken behavior
- [ ] A regression test exists in `tests/` or inline `#[cfg(test)] mod tests`
- [ ] `cargo test --all-features` passes with the new test
- [ ] `CHANGELOG.md` has a `### Fixed` entry referencing the issue
- [ ] `gaps.md` is updated if the bug was not previously tracked

## Out of Scope

Please do NOT use this template for:
- Feature requests: use `.github/ISSUE_TEMPLATE/feature_request.md`
- Questions / support: open a GitHub Discussion instead
- Security disclosures: follow `SECURITY.md` policy
