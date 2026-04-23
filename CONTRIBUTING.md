Read this document in [Portuguese (pt-BR)](CONTRIBUTING.pt-BR.md).


# Contributing to sqlite-graphrag


## Welcome
- Thank you for considering a contribution: every pull request strengthens local GraphRAG memory
- Your improvements directly affect developers using LLMs with durable memory in a single SQLite file
- Code, documentation, tests, bug reports, and ideas are equally valued contributions
- This guide keeps your onboarding under 10 minutes from clone to first local test run


## Quick Start
- Use this repository normally; the public `sqlite-graphrag` repository already exists
- The same validation commands apply locally and in the public repository workflow
- No command should print errors on a clean checkout of `main`
```bash
timeout 120 cargo check --all-targets
timeout 300 cargo nextest run --all-features
RUSTDOCFLAGS="-D warnings" timeout 120 cargo doc --no-deps --all-features
```


## Development Setup
### Toolchain requirements
- MSRV is Rust 1.88 declared in `rust-version` inside `Cargo.toml`
- JAMAIS bump MSRV without opening an RFC-style issue for discussion first
- Install Rust via `rustup` and pin the toolchain with `rustup default 1.88.0` when reproducing CI
### Dependency pinning
- Direct pin `constant_time_eq = "=0.4.2"` protects MSRV 1.88 from transitive drift via `blake3`
- JAMAIS run `cargo update` indiscriminately; always open a PR explaining the version bump
- Lockfile `Cargo.lock` MUST be committed because this repository ships a binary CLI
### Runtime requirements
- SQLite 3.40 or newer is required at runtime due to `sqlite-vec` and FTS5 external-content
- On Linux you may need `libssl-dev` and `pkg-config` for some transitive dev dependencies


## Branching Strategy
- Branch `main` is protected and requires a passing CI pipeline for merge
- Feature branches SHOULD use the prefix `feature/<short-kebab-case-description>`
- Bug fix branches SHOULD use the prefix `fix/<short-kebab-case-description>`
- Documentation-only branches SHOULD use the prefix `docs/<short-kebab-case-description>`
- Maintenance branches SHOULD use the prefix `chore/<short-kebab-case-description>`


## Commit Convention
- Follow the Conventional Commits 1.0.0 specification for every commit message on shared branches
- Use `feat` for new user-visible features
- Use `fix` for bug fixes landing on main
- Use `perf` for performance improvements without user-visible behavior changes
- Use `refactor` for code restructuring that neither adds features nor fixes bugs
- Use `docs` for documentation-only changes
- Use `chore` for tooling, CI, or repository maintenance
- Use `test` for adding or improving tests
- Use `ci` for CI pipeline changes
- JAMAIS add `Co-authored-by` for AI agents in commit messages: this is enforced by CI


## Pull Request Process
### Before opening the PR
- Rebase onto the latest `main` and resolve conflicts locally
- Keep the PR scope focused on a single logical change when possible
- Write a PR description explaining the motivation, the change, and any trade-offs
### PR Validation Checklist
- [ ] `cargo check --all-targets` passes with zero errors
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [ ] `cargo fmt --all --check` passes with zero diffs
- [ ] `cargo doc --no-deps --all-features` with `RUSTDOCFLAGS="-D warnings"` runs clean
- [ ] `cargo nextest run --all-features` runs every test to success
- [ ] `cargo llvm-cov --text` keeps coverage at or above the 80 percent minimum
- [ ] `cargo audit` reports zero vulnerabilities
- [ ] `cargo deny check advisories licenses bans sources` passes with zero violations


## Testing
- Run the full test suite with `cargo nextest run --all-features` for a fast runner with isolation
- Measure coverage with `cargo llvm-cov --text` and keep coverage at or above 80 percent
- Unit tests live inside `#[cfg(test)] mod tests` blocks within the implementation file
- Integration tests live under `tests/` and SHOULD use `assert_cmd` plus `wiremock` for HTTP mocks
- A hidden flag `--skip-memory-guard` exists exclusively for tests that do not perform real allocation
- Treat `init`, `remember`, `recall`, and `hybrid-search` as heavy-memory commands during manual validation
- Start heavy-command validation with `--max-concurrency 1` and scale only after measuring RSS and swap behavior
- JAMAIS issue real HTTP requests or touch real filesystem paths outside a `TempDir` in tests


## Documentation
- Every public API MUST have `///` doc comments with at least one testable example when reasonable
- Run `cargo doc --no-deps --all-features` with `RUSTDOCFLAGS="-D warnings"` locally before pushing
- Documentation formatting rules are defined in `docs_rules/rules_rust_documentacao.md`
- Bilingual README, CONTRIBUTING, SECURITY, and CODE_OF_CONDUCT MUST stay synchronized across EN and pt-BR
- Update the CHANGELOG under the Unreleased section for every user-visible change


## How to Report Bugs
- Open an issue using the Bug Report template on GitHub
- Include a minimal reproduction case, ideally under 20 lines of invocation or code
- Include the output of `cargo --version` and `rustc --version`
- Include your OS, architecture, SQLite version, and sqlite-graphrag version
- Include the exact command you ran, the observed output, and the expected output


## How to Request Features
- Open an issue using the Feature Request template on GitHub
- Describe the concrete use case and who benefits; avoid abstract wish-list framing
- Describe at least one alternative you considered and why it did not fit
- Reference any upstream PRD section or related issue when applicable


## Release Process
- Maintainers bump `version` in `Cargo.toml` following Semantic Versioning 2.0.0
- Maintainers update the CHANGELOG moving Unreleased entries under the new version with ISO date
- Maintainers tag the release commit as `vX.Y.Z` using `git tag -a vX.Y.Z -m "Release vX.Y.Z"`
- Pushing the tag triggers `.github/workflows/release.yml` which builds release artifacts and GitHub release assets
- Final publication to crates.io is done manually with `cargo publish --locked`


## Recognition
- Contributors are credited in the CHANGELOG next to the version that shipped their change
- Contributors are also listed in each GitHub Release note when the contribution was user-visible
- JAMAIS add `Co-authored-by` trailers for AI agents in any commit or PR description


## Questions
- Open a GitHub Discussion for design questions or broader topics not tied to a specific issue
- Use Security Advisories for anything that resembles a security issue; see SECURITY.md
