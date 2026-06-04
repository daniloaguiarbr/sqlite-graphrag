# ADR-009: Exact Pin of windows-sys 0.59.0 for HANDLE Type Stability

## Status
- Accepted (2026-06-03, v1.0.68)

## Context
- The `windows-sys` crate changed the type of `HANDLE` between versions 0.48/0.52 (`isize`) and 0.59+ (`*mut c_void`), as documented in [microsoft/windows-rs#171].
- v1.0.66 introduced `src/terminal.rs` with the expression `handle != 0 && handle as isize != -1` — a check that compiles only when `HANDLE = isize`.
- v1.0.67 was published with this code, but the `windows-sys` resolution from `Cargo.toml:111` (`version = "0.59"`) returned `windows-sys 0.59.0`, where `HANDLE = *mut c_void`.  This caused `cargo install sqlite-graphrag` on Windows to fail with `error[E0308]: mismatched types` in `src/terminal.rs:29:26`.
- The CI matrix on `windows-latest` failed to catch this because the binary's `cargo check` step runs on the runner OS, but the runner is Ubuntu (the matrix entry "windows-latest" applies to `clippy` and `test` jobs, not a dedicated cross-compile check).  See [.github/workflows/ci.yml] for the matrix; the `clippy` job (line 24) and `test` job (line 39) have `os: [ubuntu-latest, macos-latest, windows-latest]`, but the `cargo check` inside does not pass `--target x86_64-pc-windows-msvc`.

## Decision
### Code Fix
- Replace the unportable `handle != 0 && handle as isize != -1` in `src/terminal.rs:29` with the type-safe idiom:
  ```rust
  use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
  // ...
  let handle: HANDLE = GetStdHandle(handle_id);
  if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
      // ...
  }
  ```
- This idiom works for both type eras (`isize` and `*mut c_void`) and also catches the distinct `INVALID_HANDLE_VALUE` sentinel (`(HANDLE)-1`), which is different from NULL (`(HANDLE)0`).

### Dependency Pin
- Pin `windows-sys` to `=0.59.0` exact in `Cargo.toml:111`:
  ```toml
  [target.'cfg(windows)'.dependencies]
  windows-sys = { version = "=0.59.0", features = ["Win32_System_Console"] }
  ```
- Exact pin (`=`) instead of caret (`^`) because future patch versions in the 0.59.x line could regress on the type contract again.  The user must manually bump to 0.59.x or 0.60+ with code review.
- Comment in `Cargo.toml:111` documents the pin reason explicitly so a future maintainer doesn't "helpfully" loosen the version constraint.

### CI Gate
- New job `windows-build-check` in `.github/workflows/ci.yml`:
  ```yaml
  windows-build-check:
    name: Windows MSVC cross-compile (G29)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-pc-windows-msvc
      - uses: Swatinem/rust-cache@v2
      - run: timeout 600 cargo check --target x86_64-pc-windows-msvc --lib --all-features
  ```
- Runs on Ubuntu (faster than Windows runner) by installing the `x86_64-pc-windows-msvc` target via `rustup target add`.  No need for the `lib.exe` Windows linker because `cargo check` is type-only.
- Cost: ~$0.024-0.040 per build × ~50 PRs/month = ~$1-2/month on GitHub Actions.  Justified.

### Regression Test
- New `tests/terminal_compile_windows.rs` integration test that:
  - On ALL platforms: confirms `terminal::init_console` and `should_use_ansi` are callable from outside the crate
  - On Windows: additionally references the type-safe `HANDLE.is_null() + INVALID_HANDLE_VALUE` check to ensure the build still compiles
- The CI `windows-build-check` job is the canonical regression gate; the integration test is the local pre-publish sanity probe.

## Consequences
- v1.0.68 is the first release since v1.0.65 that compiles on Windows via `cargo install`.
- A user upgrading from v1.0.66 or v1.0.67 on Windows gets a successful build without manual patching.
- Future `windows-sys` version bumps require a deliberate commit that updates both the type contract and the `Cargo.toml` pin.
- The `windows-build-check` job adds ~3-5 minutes to the CI matrix but catches cross-platform regressions before publish.

## Alternatives Considered
- **Downgrade to `windows-sys = "0.52"`** — `HANDLE = isize` there, so the original code compiles.  Rejected because 0.52 is 7 versions behind and misses 0.53-0.58 fixes and feature additions.
- **Migrate to `windows = "0.58"` (high-level crate)** — provides type-safe wrappers and `is_invalid()` methods.  Rejected because it requires a refactor of the entire `terminal.rs` and `claude_runner.rs` modules, increases build time by ~30%, and adds a significant transitive dependency footprint.
- **Use `unsafe { transmute }` to force-cast the handle to `isize`** — works for both type eras but is semantically wrong (handle is a pointer, not an integer).  Rejected per the `rules-unsafe-ffi-pointers-nonnull-aliasing-volatile` policy.

## References
- Gap report: `gaps.md#G29`
- Type contract verification: `https://docs.rs/windows-sys/0.59.0/windows_sys/Win32/Foundation/type.HANDLE.html` (current) and `https://docs.rs/windows-sys/0.52.0/windows_sys/Win32/Foundation/type.HANDLE.html` (legacy)
- Historical issue: `https://github.com/microsoft/windows-rs/issues/171` (the HANDLE type toggle)
- Implementation: `src/terminal.rs:1-54`, `Cargo.toml:111`, `.github/workflows/ci.yml:122-137`, `tests/terminal_compile_windows.rs`
- Documentation: `docs/CROSS_PLATFORM.md#handle-type-and-the-windows-sys-0.59-boundary-g29-v1.0.68`, `docs/AGENTS.md#new-in-v1.0.68`
