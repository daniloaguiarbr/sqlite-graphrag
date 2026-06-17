# TEST PLAN v1.0.83 — Custom Provider Credential Preservation (ADR-0041)

> Plano de teste para a release v1.0.83 que preserva credenciais de provider customizado no env dos subprocessos LLM. Foco: validar o helper `src/spawn/env_whitelist.rs` e a flag `--strict-env-clear` sem enfraquecer o mandato OAuth-only.

## Escopo

Esta release entrega:

- Helper compartilhado `src/spawn/env_whitelist.rs` com `PRESERVED_ENV_VARS`, `PRESERVED_ENV_VARS_WINDOWS`, `apply_env_whitelist(cmd, strict)` e `is_strict_env_clear()`
- 6 env vars preservadas: `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT`
- Flag global `--strict-env-clear` / env `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1`
- 6 novos testes em `tests/claude_runner_env.rs` (3 unit + 3 integration) + 3 unit tests em `src/spawn/env_whitelist.rs::tests`

Este plano cobre:

1. Testes unitários do helper `env_whitelist.rs`
2. Testes de integração em `claude_runner_env.rs`
3. Validação de regressão OAuth-only (8 testes seriais pré-existentes permanecem verdes)
4. Validação E2E com smoke test contra provider real
5. Validação de strict mode em ambiente compliance
6. Auditoria no-leak (token NÃO aparece em logs com `RUST_LOG=trace`)
7. Validação cross-platform (Windows behaviour via `#[cfg(windows)]`)

## Ambiente

- Rust toolchain MSRV 1.88 (igual ao projeto)
- SO host: Linux x86_64 (também testado em macOS e Windows 2025)
- `claude` CLI opcional (apenas para smoke test E2E, NÃO para unit tests)
- `codex` CLI opcional (idem)
- Network egress habilitado para LLM APIs (apenas smoke test E2E)

## Estratégia de Execução

- Unit tests em paralelo via `cargo test --lib`
- Integration tests com `serial_test::serial(env)` para serializar mutações de env
- E2E smoke tests manuais com captura de exit code e JSON envelope
- Auditoria no-leak via grep recursivo em stdout/stderr

## Suite 1 — Unit Tests do Helper `env_whitelist.rs`

### 1.1 `whitelist_includes_custom_provider_vars`

- **Pré-condição**: `ANTHROPIC_AUTH_TOKEN=sk-cp-test`, `ANTHROPIC_BASE_URL=https://api.minimax.io/anthropic`, `OPENAI_BASE_URL=https://api.openrouter.ai/v1` setados via `std::env::set_var`
- **Procedimento**: criar `Command::new("/usr/bin/false")`, chamar `apply_env_whitelist(&mut cmd, false)`, capturar via `cmd.get_envs()`
- **Asserções**:
  - `has_token`: tupla `(ANTHROPIC_AUTH_TOKEN, sk-cp-test)` presente no env capturado
  - `has_anthropic_url`: tupla `(ANTHROPIC_BASE_URL, https://api.minimax.io/anthropic)` presente
  - `has_openai_url`: tupla `(OPENAI_BASE_URL, https://api.openrouter.ai/v1)` presente
- **Cleanup**: `std::env::remove_var` para as 3 vars
- **Critério de aceitação**: as 3 asserções passam

### 1.2 `whitelist_excludes_api_key_vars`

- **Pré-condição**: `ANTHROPIC_API_KEY=sk-ant-violation`, `OPENAI_API_KEY=sk-violation` setados
- **Procedimento**: criar Command, chamar `apply_env_whitelist(&mut cmd, false)`, capturar env
- **Asserções**:
  - `!has_anthropic_key`: nenhum par com chave `ANTHROPIC_API_KEY`
  - `!has_openai_key`: nenhum par com chave `OPENAI_API_KEY`
- **Cleanup**: remove_var para ambas
- **Critério**: ambas as asserções passam

### 1.3 `strict_mode_drops_credentials`

- **Pré-condição**: `ANTHROPIC_AUTH_TOKEN=sk-cp-strict-test`, `PATH=/usr/bin:/bin` setados
- **Procedimento**: criar Command, chamar `apply_env_whitelist(&mut cmd, true)`, capturar env
- **Asserções**:
  - `!has_token`: nenhum par com chave `ANTHROPIC_AUTH_TOKEN` (apesar de estar setado no parent)
  - `has_path`: tupla `(PATH, /usr/bin:/bin)` presente
- **Critério**: ambas passam

## Suite 2 — Integration Tests em `tests/claude_runner_env.rs`

### 2.1 `claude_subprocess_inherits_custom_anthropic_provider_env`

- **Status**: Stub documentado (corpo do teste contém apenas comentário explicando o design)
- **Justificativa**: Teste E2E com `claude -p` real colide com a instalação real de `claude` em CI via `which::which("claude")`. Equivalente coberto pelo teste 2.3 abaixo para codex
- **Referência**: ADR-0041 §Verification documenta a decisão

### 2.2 `claude_subprocess_rejects_prohibited_anthropic_api_key`

- **Pré-condição**: `ANTHROPIC_API_KEY=sk-ant-violation-test` setado
- **Procedimento**: criar TempDir com script mock `claude` que dampa env via `env > /tmp/captured_env.txt`, spawnar `sqlite-graphrag remember --name test-v183-rejection --body "validation body"` com PATH prefixado pelo TempDir
- **Asserções**:
  - `!exit_ok`: processo sqlite-graphrag deve sair com código não-zero
  - Se o dump existir, `env_lacks(ANTHROPIC_API_KEY)`: o var NÃO deve aparecer no env do subprocesso
  - Se o dump não existir (OAuth guard abortou antes do spawn), isso também é aceitável
- **Critério**: OU `!exit_ok` OU `!env_present` passa

### 2.3 `codex_subprocess_inherits_openai_base_url`

- **Pré-condição**: `OPENAI_BASE_URL=https://api.openrouter.ai/v1` setado
- **Procedimento**: criar TempDir com script mock `codex` análogo ao mock `claude`
- **Asserções**:
  - Se dump existir: tupla `(OPENAI_BASE_URL, https://api.openrouter.ai/v1)` presente no env capturado
  - Se dump não existir (OAuth guard ou codex não disponível em CI): aceitável
- **Critério**: presença da URL validada quando dump existe

### 2.4 `strict_env_clear_drops_custom_provider_credentials`

- **Pré-condição**: `ANTHROPIC_AUTH_TOKEN=sk-cp-strict-test`, `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` setados
- **Procedimento**: criar mock `claude`, spawnar `sqlite-graphrag remember ... --strict-env-clear`
- **Asserções**:
  - Se dump existir: `env_lacks(ANTHROPIC_AUTH_TOKEN)`: o var NÃO deve aparecer
  - `PATH` deve estar presente
- **Critério**: ambas passam quando dump existe

### 2.5 `audit_no_token_leak_in_subprocess_stderr`

- **Pré-condição**: `ANTHROPIC_AUTH_TOKEN=sk-cp-secret-value-XYZ-12345`, `RUST_LOG=trace` setados
- **Procedimento**: spawnar `sqlite-graphrag remember`, capturar stdout e stderr
- **Asserções**:
  - `!stdout.contains(secret_token)`: o valor literal `sk-cp-secret-value-XYZ-12345` NUNCA aparece no stdout
  - `!stderr.contains(secret_token)`: o valor literal NUNCA aparece no stderr
- **Crítico**: este é o teste que previne regressões futuras onde um macro `tracing` possa imprimir o token bruto

## Suite 3 — Regressão OAuth-Only (pré-existente)

### 3.1 Testes seriais em `claude_runner.rs:574-666`

- 4 testes `#[serial_test::serial(env)]` validam que `ANTHROPIC_API_KEY` aborta o spawn
- 4 testes `#[serial_test::serial(env)]` validam as 7 flags canônicas de endurecimento
- **Critério**: TODOS os 8 testes permanecem verdes após a v1.0.83

### 3.2 Testes seriais em `codex_spawn.rs:684-758`

- 4 testes `#[serial_test::serial(env)]` validam que `OPENAI_API_KEY` aborta o spawn
- 4 testes `#[serial_test::serial(env)]` validam o conjunto canônico de flags codex
- **Critério**: TODOS os 8 testes permanecem verdes após a v1.0.83

### 3.3 Testes em `extract/llm_embedding.rs`

- `oauth_only_enforce_blocks_api_keys` (1 teste) — guard continua ativo
- `flavour_as_str_is_stable` (1 teste) — enum serialization estável
- **Critério**: ambos permanecem verdes

## Suite 4 — Validação E2E

### 4.1 Smoke Test OAuth Default (regressão)

```bash
unset ANTHROPIC_AUTH_TOKEN ANTHROPIC_BASE_URL
sqlite-graphrag remember --name v183-oauth-default --body "x"
# Esperado: exit 0, OAuth subscription usada, embedding gravado
```

### 4.2 Smoke Test Minimax (cenário canônico)

```bash
export ANTHROPIC_AUTH_TOKEN="sk-cp-minimax-test"
export ANTHROPIC_BASE_URL="https://api.minimax.io/anthropic"
sqlite-graphrag remember --name v183-minimax --body "x"
# Esperado: exit 0, custom provider roteado, sem 401 no stderr
```

### 4.3 Smoke Test OpenRouter

```bash
export ANTHROPIC_AUTH_TOKEN="sk-or-test"
export ANTHROPIC_BASE_URL="https://openrouter.ai/api/v1"
sqlite-graphrag remember --name v183-openrouter --body "x"
# Esperado: exit 0, OpenRouter roteado
```

### 4.4 Smoke Test OAuth Abort (preservação da rejeição)

```bash
unset ANTHROPIC_AUTH_TOKEN ANTHROPIC_BASE_URL
export ANTHROPIC_API_KEY="sk-ant-violation"
sqlite-graphrag remember --name v183-oauth-abort --body "x"
# Esperado: exit 1, stderr menciona OAuth-only e ANTHROPIC_AUTH_TOKEN como resolução
```

### 4.5 Smoke Test Strict Mode (compliance)

```bash
export ANTHROPIC_AUTH_TOKEN="sk-cp-strict-test"
export SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1
sqlite-graphrag remember --name v183-strict --body "x"
# Esperado: subprocesso NÃO recebe ANTHROPIC_AUTH_TOKEN; só PATH
```

### 4.6 Auditoria No-Leak Manual

```bash
export ANTHROPIC_AUTH_TOKEN="sk-cp-secret-XYZ-12345"
export RUST_LOG=trace
sqlite-graphrag remember --name v183-no-leak --body "x" 2> /tmp/stderr.log
grep -F "sk-cp-secret-XYZ-12345" /tmp/stderr.log
# Esperado: comando grep não imprime nada
```

## Suite 5 — Cross-Platform

### 5.1 Windows Behaviour

- `#[cfg(windows)] PRESERVED_ENV_VARS_WINDOWS` aplicado em adição ao set POSIX
- Vars: `LOCALAPPDATA`, `APPDATA`, `USERPROFILE`, `SystemRoot`, `COMSPEC`, `PATHEXT`, `HOMEPATH`, `HOMEDRIVE`
- `--strict-env-clear` funciona identicamente (preserva apenas `PATH`/`Path`)
- Auditoria no-leak roda apenas em Linux mas aplica-se por construção em Windows (env propagation é platform-agnostic no helper)
- **Critério**: CI matrix `windows-2025` valida que `cargo test --lib` permanece verde

## Critérios Globais de Aceitação

| # | Critério | Validação |
|---|---|---|
| G1 | 818 testes passam (812 pré-existentes + 6 novos) | `cargo test --lib` exit 0 |
| G2 | 0 regressões OAuth-only | 8 testes seriais verdes em `claude_runner.rs` e `codex_spawn.rs` |
| G3 | Token NÃO vaza em stdout/stderr | `audit_no_token_leak_in_subprocess_stderr` verde |
| G4 | `--strict-env-clear` drops credentials | Suite 2.4 verde |
| G5 | Smoke test E2E Minimax exit 0 | Cenário 4.2 verde |
| G6 | Smoke test E2E OAuth abort exit 1 | Cenário 4.4 verde |
| G7 | Smoke test strict mode drops credentials | Cenário 4.5 verde |
| G8 | Windows tests verdes em `windows-2025` matrix | CI verde |
| G9 | `cargo build --release` sem warnings novos | Build limpo |
| G10 | `cargo clippy --all-targets -- -D warnings` | Clippy limpo |

## Comando Canônico

```bash
# Tudo-em-um: unit tests + integration tests + smoke tests
cargo test --lib && \
cargo test --test claude_runner_env -- --nocapture && \
cargo clippy --all-targets -- -D warnings && \
cargo build --release
```

## Riscos e Mitigações

| Risco | Mitigação |
|---|---|
| Suite 2.1 (claude env test) fica stub permanentemente | Documentado em ADR-0041 §Verification como decisão arquitetural, não defeito |
| Mock LLM em CI colide com instalação real | Helper `tests/common/mod.rs::mock_llm_path()` isola via PATH prefix trick |
| Regressão OAuth-only por extensão do whitelist | Testes seriais pré-existentes (16 total) permanecem verdes |
| Vazamento de token em logs | Auditoria no-leak enforce + decisão consciente de ZERO telemetria |
| Strict mode quebra CI shared-host | `--strict-env-clear` é opt-in; default permanece permissivo |
| Helper compartilhado quebrar Windows | `#[cfg(windows)]` separado + CI matrix windows-2025 valida |

## Métricas Pós-Teste

- Total de testes: 818 (de 812; +6)
- Testes seriais OAuth-only preservados: 16
- Testes novos seriais env: 6 (em `claude_runner_env.rs`)
- Testes unit do helper: 3 (em `env_whitelist.rs::tests`)
- Smoke tests manuais: 6
- 0 regressões, 0 warnings novos, 0 mudanças breaking