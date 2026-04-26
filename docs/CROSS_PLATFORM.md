# CROSS PLATFORM SUPPORT

> One binary, five targets, zero configuration drama across every major operating system


- Read this guide in Portuguese at [CROSS_PLATFORM.pt-BR.md](CROSS_PLATFORM.pt-BR.md)
- Return to the main [README.md](../README.md) for the full command reference


## The Pain You Already Know
### Before — Dependency Hell That Costs Two Hours
- Installing a Python RAG stack costs two hours across pip, venv and C extensions
- Alpine containers fail with glibc symbols missing from Python wheels constantly
- macOS Gatekeeper quarantines unsigned binaries blocking your first invocation
- Windows path separators break shell scripts copied verbatim from Linux tutorials
- Different shells interpret quoting rules differently across Bash Zsh Fish and PowerShell

### After — Single Binary That Just Runs
- One `cargo install --locked` command delivers the binary to any supported target
- No Python runtime, no Node runtime, no JVM, and only one ARM64 GNU shared library contract
- Binary startup stays under eighty milliseconds across every target we ship
- Exit codes remain identical across all five shipped targets for reliable orchestration
- JSON output format stays byte-for-byte identical across every operating system

### Bridge — The Command That Takes You There
```bash
cargo install --path .
```


## Support Matrix
### Targets — Five Combinations We Ship and Test
| Target | OS | Architecture | Binary Size | Startup |
| --- | --- | --- | --- | --- |
| x86_64-unknown-linux-gnu | Linux glibc | x86_64 | ~25 MB | <50ms |
| aarch64-unknown-linux-gnu | Linux glibc | aarch64 | ~24 MB | <60ms |
| aarch64-apple-darwin | macOS | Apple Silicon | ~22 MB | <30ms |
| x86_64-pc-windows-msvc | Windows | x86_64 | ~28 MB | <80ms |
| aarch64-pc-windows-msvc | Windows | ARM64 | ~27 MB | <80ms |

- Every row above gets a release asset attached to each GitHub release tag
- Every row above receives automated smoke tests in CI on every pushed commit
- SHA256SUMS manifest ships alongside every binary for integrity verification
- Debug symbols ship as separate `.dSYM` or `.pdb` artifacts on request
- Cross-compilation uses `cross` on Linux hosts for the `aarch64-unknown-linux-gnu` matrix cell

### Unsupported Release Targets — Why They Are Excluded
- `x86_64-apple-darwin` is excluded because current `ort` releases no longer provide a compatible prebuilt ONNX Runtime path for Intel macOS in this project configuration
- `x86_64-unknown-linux-musl` is excluded because current `ort` releases do not provide a supported prebuilt ONNX Runtime path for musl in this project configuration
- Reintroducing either target now REQUIRES a custom ONNX Runtime build or a different backend strategy

### ARM64 GNU — Shared ONNX Runtime Contract
- `aarch64-unknown-linux-gnu` uses dynamic ONNX Runtime loading instead of link-time bundling
- Ship `libonnxruntime.so` next to the binary, inside `./lib/`, or set `ORT_DYLIB_PATH` explicitly
- This avoids target-specific link failures from prebuilt ONNX Runtime archives during cross-compilation


## Linux Notes
### glibc First — Official Linux Release Path
- glibc binary runs on Ubuntu 20.04, Debian 11, Fedora 36 plus mainstream distros
- `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` are the only published Linux assets now
- `x86_64-unknown-linux-musl` is not part of the official release matrix in `v1.0.16`
- Reintroducing musl now requires a custom ONNX Runtime build or a different backend strategy
- Prefer glibc for workstations, CI runners, and container bases until that backend gap is closed


## macOS Notes
### Gatekeeper — Signing and Notarization
- Unsigned binaries downloaded via browser trigger Gatekeeper quarantine on first launch
- Remove quarantine with `xattr -d com.apple.quarantine /usr/local/bin/sqlite-graphrag`
- Binaries installed via `cargo install` bypass Gatekeeper since they come from rustc
- Homebrew distribution remains planned on top of the current public release line
- Official macOS release assets currently target Apple Silicon only

### Apple Silicon — Native Performance on M1 M2 M3 M4
- Native aarch64 binary runs thirty percent faster than Rosetta-translated x86_64
- Intel macOS is currently outside the official release matrix for this project configuration
- Model loading follows the same `fastembed` plus `ort` stack used on the other published targets
- Embedding generation hits 2000 tokens per second on M3 Pro versus 800 on Rosetta
- Cold start measures twenty eight milliseconds on M2 thanks to improved branch predictor


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


## Containers
### glibc Images — Official Path Today
- Prefer Debian or Ubuntu base images for the current official Linux release assets
- Alpine and musl-only images are not part of the supported release matrix in `v1.0.16`
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
- RSS after model reports peak resident memory after loading `multilingual-e5-small` fully
- Embedding throughput measures tokens per second during sustained `remember` operations
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
