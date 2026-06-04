---
name: Feature request
about: Suggest a new subcommand, flag, or behavior change
title: "[FEATURE] "
labels: ["enhancement", "triage"]
assignees: []
---

## Summary

<!-- One-paragraph description of the proposed feature -->

## Problem Statement

<!-- What user-facing problem does this solve? Quantify if possible. -->

## Proposed Solution

### CLI Surface
<!-- New subcommand or new flag with example invocations -->
```bash
sqlite-graphrag <new-subcommand> --<new-flag> <value>
```

### JSON Contract (if applicable)
```json
{
  "field": "value"
}
```

### Memory / GraphRAG Impact (if applicable)
- New memory type?
- New entity type?
- New relation type?
- New schema migration required?

## Alternatives Considered

<!-- What other approaches did you consider, and why is this one better? -->

## Cross-References
- Related gap: (if any, e.g. G28-A, G28-B, G28-D)
- Related PR or issue: (if any)
- Related external documentation: (e.g. SQLite docs, Claude Code CLI docs, etc.)

## Acceptance Criteria

This feature is considered DONE when:
- [ ] Subcommand or flag is implemented and exported
- [ ] JSON Schema updated in `docs/schemas/` for the new output contract
- [ ] Inline `#[cfg(test)] mod tests` cover the new behavior
- [ ] Integration test in `tests/` covers end-to-end usage
- [ ] `CHANGELOG.md` has a `### Added` entry referencing the issue
- [ ] `docs/AGENTS.md`, `docs/HOW_TO_USE.md`, `docs/COOKBOOK.md` updated (EN + PT-BR)
- [ ] `skill/sqlite-graphrag-{en,pt}/SKILL.md` updated to reference the new behavior
- [ ] `llms.txt`, `llms.pt-BR.txt`, `llms-full.txt` updated
- [ ] All 10 validation commands pass: `cargo check`, `clippy`, `fmt --check`, `doc`, `test`, `audit`, `deny check`, `publish --dry-run`, `llvm-cov`, `package --list`

## Out of Scope

Please do NOT use this template for:
- Bug reports: use `.github/ISSUE_TEMPLATE/bug_report.md`
- Questions / support: open a GitHub Discussion instead
- Security disclosures: follow `SECURITY.md` policy
