## Description

<!-- Concise summary of the changes -->

## Related Gap or Issue

- Gap: (e.g. G28, G29, G31, G32)
- Issue: (e.g. #123)
- Memory: (e.g. `g29-cargo-install-windows-compile-failure`)

## Type of Change

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to change)
- [ ] Documentation update (no code change)
- [ ] Schema migration (adds a new `V<NNN>__*.sql` file)
- [ ] Dependency update (Cargo.toml change)
- [ ] CI / workflow change (`.github/workflows/*.yml`)

## Validation Checklist (REQUIRED)

All boxes must be checked before requesting review.  Use the command as written, do not substitute flags.

- [ ] `cargo fmt --all --check` — clean
- [ ] `cargo check --all-targets` — zero errors
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` — zero warnings
- [ ] `cargo doc --no-deps --all-features` (with `RUSTDOCFLAGS="-D warnings"`) — zero warnings
- [ ] `cargo test --all-features` — zero failures (test count in description)
- [ ] `cargo test --test <new-test-file>` (if applicable) — passes
- [ ] `cargo audit` — zero vulnerabilities
- [ ] `cargo deny check advisories licenses bans sources` — zero violations
- [ ] `cargo publish --dry-run --allow-dirty` — zero errors
- [ ] `cargo package --list` — zero `.profraw`, zero `graphrag.sqlite`

## Documentation Checklist (REQUIRED if code changed)

- [ ] `CHANGELOG.md` (EN) updated with `### Added` / `### Fixed` / `### Changed` / `### Removed` entry
- [ ] `CHANGELOG.pt-BR.md` updated mirroring the EN entry
- [ ] `docs/AGENTS.md` updated if JSON contract changed
- [ ] `docs/AGENTS.pt-BR.md` updated mirroring the EN change
- [ ] `docs/HOW_TO_USE.md` updated if new flag or subcommand
- [ ] `docs/HOW_TO_USE.pt-BR.md` updated mirroring the EN change
- [ ] `docs/COOKBOOK.md` updated with a new recipe if user-visible behavior added
- [ ] `docs/COOKBOOK.pt-BR.md` updated mirroring the EN change
- [ ] `docs/MIGRATION.md` updated if breaking change (with rollback section)
- [ ] `docs/MIGRATION.pt-BR.md` updated mirroring the EN change
- [ ] `docs/schemas/*.schema.json` updated if JSON contract changed
- [ ] `docs/CROSS_PLATFORM.md` updated if Windows/macOS/Linux behavior changed
- [ ] `docs/CROSS_PLATFORM.pt-BR.md` updated mirroring the EN change
- [ ] `skill/sqlite-graphrag-{en,pt}/SKILL.md` updated if operational behavior changed
- [ ] `llms.txt`, `llms.pt-BR.txt`, `llms-full.txt` updated
- [ ] `INTEGRATIONS.md` updated if new external integration
- [ ] `INTEGRATIONS.pt-BR.md` updated mirroring the EN change
- [ ] `gaps.md` updated if the gap was previously tracked

## Commit Hygiene (REQUIRED)

- [ ] Commit messages are in English
- [ ] Commit messages do NOT contain `Co-authored-by:` lines referencing AI agents or bots
- [ ] Commit messages use the imperative mood ("Add X" not "Added X")
- [ ] First line of commit message is ≤ 72 characters
- [ ] Body wraps at 72 characters and explains the WHY
- [ ] Each commit is atomic (one logical change per commit)

## Test Coverage

- [ ] New unit tests added for new functions
- [ ] New integration test added in `tests/` for new subcommands
- [ ] Coverage threshold of 80% maintained (`cargo llvm-cov --text`)

## Risk Assessment

<!-- What could break? What is the rollback plan? -->

## Reviewer Notes

<!-- Anything specific the reviewer should focus on, e.g. performance, security, API compatibility -->
