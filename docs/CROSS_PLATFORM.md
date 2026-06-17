## Custom Provider Env Whitelist on Windows (v1.0.83+)
- The shared env whitelist helper `src/spawn/env_whitelist.rs` exposes a Windows-specific set via `PRESERVED_ENV_VARS_WINDOWS` behind `#[cfg(windows)]`: `LOCALAPPDATA`, `APPDATA`, `USERPROFILE`, `SystemRoot`, `COMSPEC`, `PATHEXT`, `HOMEPATH`, `HOMEDRIVE`
- The Windows set is applied in addition to the POSIX set; `apply_env_whitelist(cmd, false)` covers both via the `#[cfg(windows)]` second loop in the helper
- On Windows the custom-provider env vars `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT` flow to the LLM subprocess identically to Linux/macOS
- `LockFileEx` is used by the slot semaphore (ADR-0039, v1.0.82) on Windows; this release adds no new lock primitives
- The no-leak audit test `audit_no_token_leak_in_subprocess_stderr` runs on Linux only; the same assertion applies on Windows by construction (env propagation is platform-agnostic in the helper)
- `--strict-env-clear` flag and `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` env var work identically on Windows; only `PATH` (or `Path` on Windows, which the helper normalises) is forwarded in strict mode
- See `docs/decisions/adr-0041-preserve-custom-provider-env.md` and `docs/COOKBOOK.md#how-to-use-custom-anthropic-compatible-providers-v1083` for the full recipe
# CROSS PLATFORM SUPPORT

> One 6 MB binary, five targets, zero model download across every major operating system (v1.0.76 LLM-Only)


- Read this guide in Portuguese at [CROSS_PLATFORM.pt-BR.md](CROSS_PLATFORM.pt-BR.md)
- Return to the main [README.md](../README.md) for the full command reference


## v1.0.76 Architectural Note
- The default build is LLM-only and one-shot. There is no ONNX runtime to ship, no `libonnxruntime.so` to bundle, and no `multilingual-e5-small` model to download. Embedding generation delegates to a headless `claude code` or `codex` subprocess (OAuth) spawned per call.
- The `embedding-legacy` feature was REMOVED in v1.0.79 (ahead of the v1.1.0 schedule). Every build is LLM-only; the fastembed + ort + tokenizers pipeline and the ARM64 GNU ONNX contract no longer apply.
- The cross-platform table below describes the LLM-only build, which is now the only build.


## The Pain You Already Know
### Before — Dependency Hell That Costs Two Hours
- Installing a Python RAG stack costs two hours across pip, venv and C extensions
- Alpine containers fail with glibc symbols missing from Python wheels constantly
- macOS Gatekeeper quarantines unsigned binaries blocking your first invocation
- Windows path separators break shell scripts copied verbatim from Linux tutorials
- Different shells interpret quoting rules differently across Bash Zsh Fish and PowerShell

### After — Single Binary That Just Runs
- One `cargo install --locked` command delivers the binary to any supported target
- No Python runtime, no Node runtime, no JVM, no ONNX runtime, no 1.1 GB model download
- Binary startup stays under eighty milliseconds across every target we ship
- Exit codes remain identical across all five shipped targets for reliable orchestration
- JSON output format stays byte-for-byte identical across every operating system

### Bridge — The Command That Takes You There
```bash
cargo install --path .
# or
cargo install --locked sqlite-graphrag
```


## Support Matrix
### Targets — Five Combinations We Ship and Test
| Target | OS | Architecture | Binary Size | Startup |
| --- | --- | --- | --- | --- |
| x86_64-unknown-linux-gnu | Linux glibc | x86_64 | ~6 MB | <50ms |
| aarch64-unknown-linux-gnu | Linux glibc | aarch64 | ~6 MB | <60ms |
| aarch64-apple-darwin | macOS | Apple Silicon | ~6 MB | <30ms |
| x86_64-pc-windows-msvc | Windows | x86_64 | ~6 MB | <80ms |
| aarch64-pc-windows-msvc | Windows | ARM64 | ~6 MB | <80ms |

- Every row above gets a release asset attached to each GitHub release tag
- Every row above receives automated smoke tests in CI on every pushed commit (with the mock LLM CLI prepended to PATH)
- SHA256SUMS manifest ships alongside every binary for integrity verification
- Debug symbols ship as separate `.dSYM` or `.pdb` artifacts on request
- Cross-compilation uses `cross` on Linux hosts for the `aarch64-unknown-linux-gnu` matrix cell
- Sizes are for the LLM-only build (the only build since v1.0.79)

### Unsupported Release Targets — Why They Are Excluded
- `x86_64-apple-darwin` is excluded because the v1.0.76 build no longer requires a prebuilt ONNX Runtime path (and Intel macOS has been a long-deprecated macOS target since 2024)
- `x86_64-unknown-linux-musl` is excluded because no glibc-only native dependency remains in the default build, but a musl build is not part of the release matrix
- Reintroducing either target is a routine cross-compile task in v1.0.76 because no C extension needs to be linked

### ARM64 GNU — No More Shared ONNX Runtime Contract
- v1.0.76 has NO ONNX runtime dependency in the default build. The previous `aarch64-unknown-linux-gnu` ONNX contract (`libonnxruntime.so` next to the binary, `ORT_DYLIB_PATH` env var) is REMOVED.
- The dynamic loader contract was an artifact of the v1.0.74 fastembed pipeline. With the LLM subprocess as the model, the binary needs zero C shared libraries beyond libc.
- Historical note: builds with the removed `embedding-legacy` feature (v1.0.76-v1.0.78) shipped `libonnxruntime.so` on `aarch64-unknown-linux-gnu`. Since v1.0.79 no configuration needs the contract.


## Linux Notes
### glibc First — Official Linux Release Path
- glibc binary runs on Ubuntu 20.04, Debian 11, Fedora 36 plus mainstream distros
- `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` are the only published Linux assets now
- `x86_64-unknown-linux-musl` is not part of the official release matrix since `v1.0.16`
- With the LLM-only build, no glibc version constraint exists beyond what the LLM subprocess binary needs


## macOS Notes
### Gatekeeper — Signing and Notarization
- Unsigned binaries downloaded via browser trigger Gatekeeper quarantine on first launch
- Remove quarantine with `xattr -d com.apple.quarantine /usr/local/bin/sqlite-graphrag`
- Binaries installed via `cargo install` bypass Gatekeeper since they come from rustc
- Official macOS release assets currently target Apple Silicon only

### Apple Silicon — Native Performance on M1 M2 M3 M4
- Native aarch64 binary runs thirty percent faster than Rosetta-translated x86_64
- Intel macOS is currently outside the official release matrix for this project configuration
- The LLM subprocess (`claude` or `codex`) is the model; the Rust binary itself does not load any model
- Cold start measures twenty eight milliseconds on M2 thanks to improved branch predictor
- The only LLM-side latency is the 1-3 s subprocess spawn (claude / codex) per `remember` / `recall`


## Windows Notes
### Shell — PowerShell 7 and Windows Terminal
- PowerShell 7 or later runs every example from the README without modification
- Windows Terminal renders colored output and progress bars identically to Unix shells
- Legacy CMD.EXE works but strips ANSI colors unless `SQLITE_GRAPHRAG_FORCE_COLOR=1` is set
- WSL2 users should prefer the Linux glibc binary for full feature parity with Unix
- PowerShell ISE does NOT support interactive prompts used during `init` confirmation

### UTF-8 Console — The Only Required Tweak
```powershell
chcp 65001
$env:PYTHONIOENCODING = "utf-8"
sqlite-graphrag remember --name "memória-acentuada" --body "caracteres unicode funcionam"
```
- Code page 65001 switches the console to UTF-8 encoding for correct character rendering
- Without UTF-8 the binary still works but stdout prints replacement characters for accents
- Modern Windows Terminal defaults to UTF-8 eliminating the `chcp` command entirely
- Line endings stay LF inside the SQLite database regardless of console configuration
- Scripts persist correctly across Windows, Linux and macOS when saved in UTF-8

### HANDLE Type and the windows-sys 0.59 Boundary (G29, v1.0.68)
- The `windows-sys` crate changed the type of `HANDLE` between 0.48/0.52 (`isize`) and 0.59+ (`*mut c_void`); the breaking change was made by Microsoft in [windows-rs#171]
- `cargo install sqlite-graphrag` on Windows broke in v1.0.67 with `error[E0308]: mismatched types` in `src/terminal.rs:29:26` because the comparison `handle != 0 && handle as isize != -1` was only valid for the old type
- v1.0.68 replaces the comparison with the type-safe idiom `!handle.is_null() && handle != INVALID_HANDLE_VALUE`, which works for both type eras and also catches the distinct `INVALID_HANDLE_VALUE` sentinel (`(HANDLE)-1`) which is different from NULL
- `windows-sys` is pinned to `=0.59.0` exact in `Cargo.toml:111` to prevent silent resolution to a future 0.59.x that might re-break the type contract
- New CI job `windows-build-check` in `.github/workflows/ci.yml` runs `cargo check --target x86_64-pc-windows-msvc --lib --all-features` on every push and PR so future regressions are caught before publish
- Manual workaround for v1.0.66/v1.0.67 (only needed if you must stay on those versions): edit `~/.cargo/registry/src/index.crates.io-*/sqlite-graphrag-*/src/terminal.rs`, replace line 29 with `if !handle.is_null() && handle != INVALID_HANDLE_VALUE`, and add `INVALID_HANDLE_VALUE` to the `use windows_sys::Win32::Foundation::{...}` import.  Then `cargo install --path .` from the patched source.
- Reference: `https://docs.rs/windows-sys/0.59.0/windows_sys/Win32/Foundation/type.HANDLE.html` (current) and `https://docs.rs/windows-sys/0.52.0/windows_sys/Win32/Foundation/type.HANDLE.html` (legacy)

### CI Windows Infra Resilience (G53-WINDOWS-INFRA, ADR-0033, v1.0.80)
- The windows-2025 matrix jobs (`clippy` and `test`) gained 2 new steps each, gated on `if: matrix.os == 'windows-2025'` (no-op on ubuntu and macos): a pre-warm step that downloads the rustup toolchain into the runner cache before the build, and a verify step that re-checks `rustup show active-toolchain` after install
- The 2 historical infra failure modes are now recoverable on the first re-run instead of accumulating as red CI: (a) rustup download with transient network errors, (b) `E0463 can't find crate for core` when the target stdlib is missing
- Local cross-compile validation: `cargo check --target x86_64-pc-windows-msvc --lib --all-features` reproduces and the `E0463` is fixed by `rustup target add x86_64-pc-windows-msvc --toolchain 1.88`; the build then reaches the `cc-rs: failed to find tool "lib.exe"` frontier, which is the expected host-Linux cross-compile limit
- The explicit `windows-2025` runner label (replacing `windows-latest` since v1.0.73) remains the right call until the VS2026 redirect cutover (2026-06-15); see ADR-0033 for the full rationale and boundary conditions


## Containers
### glibc Images — Official Path Today
- Prefer Debian or Ubuntu base images for the current official Linux release assets
- Alpine and musl-only images are not part of the supported release matrix since `v1.0.16`
- A musl container path requires a custom backend decision before it becomes a supported workflow again


## Shell Support
### Bash Zsh Fish PowerShell Nushell — All First Class
```bash
# Bash and Zsh share identical syntax for every pipeline in this documentation
sqlite-graphrag recall "query" --json | jaq '.results[].name'
```
```fish
# Fish uses the same binary invocation with slightly different variable syntax
sqlite-graphrag recall "query" --json | jaq '.results[].name'
```
```powershell
# PowerShell pipes objects natively but jaq still accepts raw JSON on stdin
sqlite-graphrag recall "query" --json | jaq '.results[].name'
```
```nu
# Nushell consumes JSON directly into structured tables without external tooling
sqlite-graphrag recall "query" --json | from json | get results | select name
```
- Every shell above reads the same exit codes for identical orchestration semantics
- JSON output format stays byte-identical across all five shells simplifying pipelines
- Shell completion scripts are supported by the current CLI via `sqlite-graphrag completion <shell>`
- Environment variable precedence remains identical across all shells tested in CI
- Signals SIGINT and SIGTERM work identically enabling graceful shutdown universally


## File Paths and XDG
### Paths — Directories Crate Handles Every OS
- Default database path resolves to `./graphrag.sqlite` in the invocation directory
- macOS paths resolve to `~/Library/Application Support/sqlite-graphrag/` per Apple HIG
- Windows paths resolve to `%APPDATA%\sqlite-graphrag\` and `%LOCALAPPDATA%\sqlite-graphrag\`
- Override via `SQLITE_GRAPHRAG_DB_PATH` takes absolute priority on every operating system

### Environment Variables — Runtime Overrides
```bash
export SQLITE_GRAPHRAG_DB_PATH="/var/lib/graphrag.sqlite"
export SQLITE_GRAPHRAG_CACHE_DIR="/tmp/sqlite-graphrag-cache"
export SQLITE_GRAPHRAG_LANG="pt"
export SQLITE_GRAPHRAG_LOG_LEVEL="debug"
```
- `SQLITE_GRAPHRAG_DB_PATH` overrides the default `./graphrag.sqlite` path
- `SQLITE_GRAPHRAG_CACHE_DIR` isolates model cache and lock files for container and test scenarios
- `SQLITE_GRAPHRAG_LANG` switches CLI output between English and Brazilian Portuguese
- `SQLITE_GRAPHRAG_LOG_LEVEL` controls tracing verbosity with `debug` exposing every SQL query


## Performance by Target
### Benchmarks — Selected Supported Targets
| Target | Cold Start | Warm Recall | RSS After Model | Embedding Throughput |
| --- | --- | --- | --- | --- |
| x86_64-linux-gnu (i7-13700) | 48 ms | 4 ms | 820 MB | 1500 tok/s |
| aarch64-linux-gnu (Graviton3) | 58 ms | 5 ms | 810 MB | 1400 tok/s |
| aarch64-apple-darwin (M3 Pro) | 28 ms | 3 ms | 790 MB | 2000 tok/s |
| x86_64-windows-msvc (i7-12700) | 75 ms | 6 ms | 860 MB | 1300 tok/s |

- Cold start measures time from process spawn to first successful SQL query completion
- Warm recall measures second invocation with the database page cache already hot
- RSS after model reports peak resident memory of the LLM subprocess (`claude -p` or `codex exec`) during embedding; the Rust binary itself holds no model state
- Embedding throughput measures tokens per second during sustained `remember` operations, dominated by LLM subprocess spawn + JSON parse
- Every number above stays within ten percent variance across ten benchmark runs locally


## Agents Validated per Platform
### Twenty One Agents — Verified Across Every Target
- Claude Code from Anthropic runs identically on Linux, macOS and Windows native shells
- Codex from OpenAI uses the same binary on Linux containers and macOS developer laptops
- Gemini CLI from Google invokes the binary via its standard subprocess execution path
- Opencode open source harness integrates via stdin and stdout on every supported OS
- OpenClaw agent framework targets Linux containers primarily but works on macOS too
- Paperclip research assistant runs on macOS and Linux desktop environments simultaneously
- VS Code Copilot from Microsoft executes via integrated terminal tasks across OSes
- Google Antigravity platform runs the Linux glibc binary inside its sandboxed runtime
- Windsurf from Codeium targets macOS and Windows editor installations predominantly
- Cursor editor invokes the binary via its terminal across macOS, Linux and Windows
- Zed editor runs sqlite-graphrag as an external tool on macOS and Linux natively
- Aider coding agent targets Linux and macOS terminals for git-aware workflows daily
- Jules from Google Labs runs the Linux glibc binary in CI pipelines predominantly
- Kilo Code autonomous agent targets macOS developer workflows with native bindings
- Roo Code orchestrator runs on Linux servers and macOS workstations interchangeably
- Cline autonomous agent integrates via VS Code on every operating system the editor ships
- Continue open source assistant runs wherever its host editor runs natively supported
- Factory agent framework prefers Linux containers for reproducible multi-agent scenarios
- Augment Code assistant targets macOS and Linux engineering environments predominantly
- JetBrains AI Assistant runs sqlite-graphrag alongside IntelliJ IDEA on all three desktop OSes
- OpenRouter proxy executes the Linux binary in Kubernetes clusters and Docker hosts


### Codex CLI (v1.0.62)
- Codex CLI (`codex exec`) is available on macOS, Linux, and Windows
- Binary discovery follows: `--codex-binary` flag, `SQLITE_GRAPHRAG_CODEX_BINARY` env var, then PATH lookup
- On Windows, searches for `codex.exe` in PATH with `PATHEXT` extension resolution
- Subprocess uses `env_clear()` with platform-specific variable whitelist including Windows vars via `#[cfg(windows)]`


## OAuth-Only Authentication Across Platforms (v1.0.69)
### Behaviour Change Applies Identically on Every OS
- The `claude -p` and `codex exec` spawn ABORTS with `AppError::Validation` (exit code 1) when `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` are defined in the environment, on Linux glibc, aarch64 GNU, macOS, and Windows targets alike
- OAuth is the ONLY accepted credential mechanism across every published target
- The `--bare` flag is REMOVED from all executable code paths in every build variant
- Migration: run `claude login` (Claude Pro/Max) or `codex login` (ChatGPT Pro) once on each host and remove the env var from the shell rc
- Defence-in-depth: `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` are INTENTIONALLY ABSENT from the `env_clear` whitelists on every platform; even if a future refactor moves the OAuth-only guard, the variable never reaches the child
- See `docs/decisions/adr-0011-oauth-only-enforcement.md` for the full rationale and `src/commands/claude_runner.rs:574-666` and `src/commands/codex_spawn.rs:684-758` for the four OAuth-only conformance tests on each binary


## v1.0.82 Cross-Platform Behaviour
### SHUTDOWN_EXIT_CODE = 19 Constant (ADR-0037)
- The constant `SHUTDOWN_EXIT_CODE = 19` in `src/constants.rs` is emitted identically on Linux glibc, aarch64 GNU, macOS, and Windows targets when SIGTERM/SIGINT/SIGHUP arrives during an LLM subprocess
- The shutdown JSON envelope on stdout is byte-identical across platforms; validated by `tests/shutdown_envelope_regression.rs`
- The envelope schema is `docs/schemas/shutdown-envelope.schema.json` and is platform-agnostic by construction
- The `nix` crate (Unix) and the `windows-sys` crate (Windows) both call the same downstream `try_reset_shutdown` and `install_shutdown_handler` functions, so behaviour diverges only at the syscall boundary
### fs4 Cross-Process Lock (ADR-0039)
- The `fs4 = "0.9"` crate with feature `sync` provides cross-platform file locking via `FileExt::lock_exclusive` and `try_lock_exclusive`
- Linux and macOS use `fcntl(F_SETLK)` (POSIX advisory lock) on the underlying `slot-{0..N}.lock` file descriptor
- Windows uses `LockFileEx` with `LOCKFILE_EXCLUSIVE_LOCK` for the same exclusive semantics
- The lock is RELEASED automatically when the process exits (kernel reclaims the file descriptor) — no manual cleanup required for the happy path
- The `slots release --slot-id <N>` subcommand is the only way to forcibly reap an orphan lock held by a dead PID; it must be cross-platform because the orphan-detection heuristic (`kill -0 <pid>`) is also cross-platform via `std::process::Command`
### Stderr-Capture Fallback Chain (ADR-0040)
- The chain inspects `codex exec` stderr for the magic string `refresh_token_reused` (2026-06-14 incident) and routes to the next backend in `--llm-backend`
- The detection is regex-based and operates on the raw stderr bytes; it does not depend on the `codex` CLI version or platform
- The fallback writes the original failure to `tracing::warn!` (stderr) and persists the row in `pending_embeddings` if no backend succeeds
- Operator action `codex login` is required on every host where `codex` is the primary backend; the env-var-based refresh-token management is host-wide, not per-invocation
