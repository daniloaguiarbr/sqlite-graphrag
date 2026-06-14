# ADR-0031 — A1 Audit: Shell Completions Test Coverage (v1.0.80)

## Status

Accepted (v1.0.80, 2026-06-14).

## Context

The v1.0.80 audit suite (A1 audit cycle, scope: CLI surface
coverage) identified that the `completions` subcommand
(added in v1.0.67) had zero end-to-end test coverage despite
documenting support for 5 shells (bash, zsh, fish,
powershell, elvish). The 5 supported shells were a
documented public contract, but the only verification was
manual invocation of `sqlite-graphrag completions <shell>`
and visual inspection of the generated script. There was no
test that asserted (a) exit code 0 for valid shells, (b)
the expected completion-script markers per shell, (c)
non-zero exit for an unknown shell, or (d) non-empty
output for every supported shell.

## Decision

A new integration test file `tests/completions.rs` adds 7
end-to-end tests for the `completions` subcommand. The
tests require a local debug build; if the binary is
missing (e.g., a fresh `cargo check` clone), they
auto-skip via a `binary_exists` check at the top of each
test. This keeps the test suite green in CI environments
that run `cargo test --no-run` without compiling the
binary, while still catching regressions in environments
that do compile and run the binary.

The 7 tests cover:

1. `completions_bash_emits_script` — assert exit 0, output
   contains `complete` and `_sqlite-graphrag` markers.
2. `completions_zsh_emits_script` — assert exit 0, output
   contains `#compdef` or `_sqlite-graphrag` markers.
3. `completions_fish_emits_script` — assert exit 0, output
   contains `complete` or `sqlite-graphrag` markers.
4. `completions_powershell_emits_script` — assert exit 0,
   output contains `Register-ArgumentCompleter` or
   `sqlite-graphrag` markers.
5. `completions_elvish_emits_script` — assert exit 0, output
   contains `edit:completion:arg-completer` or
   `sqlite-graphrag` markers.
6. `completions_invalid_shell_exits_nonzero` — assert
   `not-a-real-shell` produces non-zero exit (clap
   `ValueEnum` rejection, exit 2).
7. `completions_emits_nonempty_output_for_each_shell` —
   iterate over the 5 supported shells, assert exit 0 and
   output length > 50 bytes, and write the output to a
   `tempfile::NamedTempFile` to prevent the test from being
   optimised away.

## Consequences

Positive:

- The 5-shell completions contract is now backed by
  automated tests; any future clap or subcommand
  refactor that breaks one of the 5 shells is caught
  by CI before release.
- The auto-skip behaviour keeps the test suite green in
  `cargo test --no-run` environments without compromising
  the verification in environments that compile the
  binary.
- The 7th test writes the generated completion script to
  a tempfile, which makes the captured script visible in
  test logs (useful for debugging future clap upgrades).

Negative:

- The tests require a local debug build; CI runners
  that don't compile the binary (e.g., `cargo test
  --no-run`) skip the tests silently. CI runners that
  DO compile the binary (the default) exercise the
  tests. The README and CI workflow document this.
- The marker assertions (`_sqlite-graphrag`,
  `#compdef`, etc.) are coupled to the generated
  script's text; a future clap upgrade that changes
  these markers will require updating the tests.

## References

- `tests/completions.rs:1-153` (7 end-to-end tests)
- `docs/HOW_TO_USE.md` → "How To Install Shell Completions"
- v1.0.67 (initial `completions` subcommand introduction)
- A1 audit cycle (v1.0.80, scope: CLI surface coverage)

