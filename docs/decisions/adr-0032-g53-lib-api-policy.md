# ADR-0032: G53 Library API Stability Policy

## Status
- Accepted (2026-06-13)
- Deciders: Danilo Aguiar
- Scope: `Cargo.toml`, `README.md`, `README.pt-BR.md`, `.github/workflows/ci.yml` (semver-checks job)
- v1.0.80 — this ADR formalises the decision the G53 audit flagged as ABERTO.


## Context
- `sqlite-graphrag` is published as a dual lib+bin crate on crates.io.
- The library is consumed by a small set of embedded use cases (e.g. custom MCP servers wrapping the binary, long-running services that import `storage::memories` directly).
- As of v1.0.79, the published lib surface has 9 MAJOR-level breaking changes vs v1.0.78 according to `cargo semver-checks --baseline-version 1.0.78`:
  - 7 trait removals (`extraction_gliner::Extractor` family)
  - 2 type re-export removals
- These shipped in a **patch** bump (1.0.78 -> 1.0.79) because no published release enforces a `cargo semver-checks` gate in CI.
- Consumers who pinned to `^1.0.78` had their builds break on `cargo update`.


## Decision
- The **CLI is the public, stable contract**. The `--json` envelopes documented in `docs/schemas/*.schema.json` and the environment variables listed in `llms.txt` and `llms-full.txt` are stable across all v1.x.y releases. Bumps in either direction (1.0.78 -> 1.1.0 or 1.1.0 -> 2.0.0) MUST preserve the CLI contract or migrate it through a documented deprecation cycle.
- The **library API is unstable** within v1.x.y. Re-exports, public struct fields and function signatures may change in any v1.x.y release.
- Breaking changes to the library API ship as a **minor** bump (e.g. 1.0.79 -> 1.1.0), never patch. Patch bumps (1.0.79 -> 1.0.80) are limited to additive, non-breaking changes to the lib surface.
- A `cargo semver-checks` job is added to CI as **INFORMATIONAL** in v1.0.80 (`continue-on-error: true`) so existing PRs are not blocked before the 9 current MAJOR violations are resolved. The job is promoted to **BLOCKING** in v1.0.81 once a clean baseline is established.


## Consequences
### Positive
- CLI consumers (the dominant use case) get a stable, predictable contract across all v1.x.y releases.
- Library consumers are explicitly informed that the lib surface is unstable, removing the false expectation of SemVer guarantees.
- `cargo semver-checks` is in CI from v1.0.80 forward, providing a structured view of lib-API drift over time.
- The 9 current MAJOR violations become a tracked, visible debt rather than an invisible regression source.

### Negative
- Library consumers must pin to exact versions and read CHANGELOG.md on every minor bump. This is a higher-friction workflow than the implicit `^1.0.80` guarantee.
- The minor-bump-for-removals rule means a v1.0.x track can grow lib-API debt that only resolves at v2.0.0. We accept this trade-off: a 1.x.y cgroup of releases is treated as a "lib API can change" cycle, with v2.0.0 as the only hard break.

### Mitigation
- `Cargo.toml` exposes the standard `^1.0` SemVer shorthand, so `cargo add sqlite-graphrag` defaults to "follow CLI stability" — exactly the intent.
- Library consumers who need pinning are documented to use `sqlite-graphrag = "=1.0.80"` syntax.
- `CHANGELOG.md` and `CHANGELOG.pt-BR.md` are updated on every release with a "Library API Changes" section that lists re-export removals, public struct field changes and signature changes explicitly.


## Alternatives Considered
1. **Adopt v2.0.0 for removals, keep patch strict for v1.x.y.** Rejected because it would require publishing v1.0.80, v1.0.81 ... v1.0.89 with zero lib-API changes, accumulating technical debt without ever shipping. The minor-bump path allows forward motion.
2. **Bump to 2.0.0 immediately for the v1.0.79 -> v1.0.80 transition.** Rejected because v1.0.79 is the current production release and the breaking changes there were already shipped under a patch bump; bumping to 2.0.0 retroactively would be a documentation lie.
3. **Keep current behavior (no policy, no CI gate).** Rejected because G50 documented that 6 of the last 7 CI runs completed in failure including the v1.0.79 release; continuing without a gate is a known regression vector.


## Related
- G50: CI Vermelho Não Bloqueia Release (motivation for the gate)
- G53: Processo de Release (parent gap)
- ADR-0011: OAuth-only enforcement (precedent for CLI-as-contract stance)
- `docs/decisions/adr-0028-g41-phantom-v013-registration.md` (precedent for ADR scope = one gap or one decision)


## Implementation
- `Cargo.toml`: no change. Standard SemVer is already correct for the policy.
- `README.md` and `README.pt-BR.md`: new "Stability Policy" section added between "Why sqlite-graphrag?" and "Superpowers for AI Agents" in v1.0.80.
- `.github/workflows/ci.yml`: new `semver-checks` job with `continue-on-error: true` against `--baseline-version 1.0.79`. Promoted to blocking in v1.0.81.
- `llms.txt` and `llms-full.txt`: no change required (CLI doc is unchanged).
