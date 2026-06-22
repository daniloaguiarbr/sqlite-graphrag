## Preservação de Env de Custom Provider em Invocação Headless (v1.0.83+)
- O pipeline de invocação headless (`claude_runner`, `codex_spawn`, `ingest_claude`) agora preserva seis env vars de custom-provider ao spawnar subprocessos: `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT`
- Os três spawners delegam para `apply_env_whitelist(cmd, strict)` de `src/spawn/env_whitelist.rs` em vez de inlinear o array de whitelist. Isso elimina o drift entre os três blocos duplicados de `env_clear` + re-injeção
- O guard OAuth-only em `claude_runner.rs:273`, `codex_spawn.rs:259`, `ingest_claude.rs:282`, `extract/llm_embedding.rs:237-253` permanece inalterado; `ANTHROPIC_API_KEY` e `OPENAI_API_KEY` ainda abortam com `AppError::Validation` (exit 1) e a nova mensagem de erro referencia `ANTHROPIC_AUTH_TOKEN` e `~/.codex/auth.json` como resoluções legítimas
- Nova flag global `--strict-env-clear` / `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` ativa modo estrito que preserva apenas `PATH`. Use em ambientes compliance (PCI-DSS, SOC2, HIPAA) onde encaminhamento de credenciais via env vars é proibido por política
- As 7 flags de endurecimento para `claude -p` (`--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions --output-schema` mais model e prompt) e o conjunto canônico para `codex exec` permanecem inalterados. A mudança no whitelist de env é puramente aditiva no passo de whitelist entre `env_clear()` e a construção das flags canônicas
- Sem telemetria nova: o fix é silencioso. O teste de auditoria no-leak `audit_no_token_leak_in_subprocess_stderr` em `tests/claude_runner_env.rs` garante que o valor literal do token NUNCA aparece em stdout ou stderr mesmo com `RUST_LOG=trace`
- Veja `docs/decisions/adr-0041-preserve-custom-provider-env.pt-BR.md` para a justificativa arquitetural completa
# Invocação Headless — Claude Code, Codex, OpenCode sem MCP e sem Hooks (v1.0.89 — Camada Pre-flight + Hotfixes)

> Como invocar LLMs headless neste projeto sem herdar MCPs ou hooks do ambiente, mantendo o login OAuth de assinatura.

- Versão em inglês deste guia vive em [HEADLESS_INVOCATION.md](HEADLESS_INVOCATION.md)
- Voltar ao [README.md](../README.md) para referência de comandos


## Resumo

- Claude Code OAuth sem MCP usa `--strict-mcp-config --mcp-config '{}'`
- Codex OAuth sem MCP usa `codex exec -c mcp_servers='{}'`
- OpenCode OAuth sem MCP usa `OPENCODE_CONFIG_CONTENT` com `enabled` falso por servidor
- A descoberta mais importante: no Claude, a flag `--bare` corta os MCP mas DESLIGA o OAuth. `--bare` passa a exigir chave de API, que aqui é proibida. Por isso NÃO se usa `--bare` quando o login é por assinatura


## Tabela de Comandos OAuth-Safe

| CLI | Comando headless OAuth-safe | Mantém OAuth | Corta MCP | Corta Hooks |
| --- | --- | --- | --- | --- |
| Claude Code | `claude -p "TAREFA" --strict-mcp-config --mcp-config '{}' ...` | sim | sim | sim |
| Codex CLI | `codex exec -c mcp_servers='{}' ...` | sim | sim | N/A |
| OpenCode | `OPENCODE_CONFIG_CONTENT='{...enabled:false...}' opencode run ...` | sim | sim | N/A |


## Claude Code Headless OAuth sem MCP e sem Hooks

### O Que Fazer

Rodar `claude -p` com a config de MCP travada e vazia, e a config de hooks zerada.

### Por Que Fazer

- O `-p` ativa o modo headless de uma tacada só
- O `--strict-mcp-config` manda ignorar TODA config de MCP do ambiente
- O `--mcp-config '{}'` entrega uma lista vazia de servidores
- O `--settings '{"hooks":{}}'` desliga os hooks naquela chamada específica
- A combinação garante zero MCP e zero hooks no ar, mantendo o login por assinatura (OAuth Pro ou Max)

### Atualização v1.0.79 — O Isolamento Real É `CLAUDE_CONFIG_DIR` Vazio

- A issue #10787 de `anthropics/claude-code` documenta que `--strict-mcp-config` e `--mcp-config` são silenciosamente IGNORADAS pelo upstream
- O único mecanismo que o upstream honra é `CLAUDE_CONFIG_DIR` apontando para um diretório vazio
- Desde a v1.0.79 (G42/S6), o pipeline de embedding da CLI usa `CLAUDE_CONFIG_DIR` vazio POR PADRÃO: honra `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR`, senão um diretório gerenciado `~/.local/state/sqlite-graphrag/claude-empty-config` (modo 0700, copia `.credentials.json` quando presente)
- Um `~/.claude` populado custava ~223k tokens de cache-creation por chamada (~40-50s); o config dir vazio derruba para ~10-15s
- As flags abaixo continuam sendo passadas por defesa em profundidade, mas NÃO confie nelas como isolamento

### Por Que NÃO Usar `--bare`

- O `--bare` também corta MCP, hooks, skills, plugins e auto memory
- MAS o `--bare` desativa o OAuth e o keychain (issue #39069 de `anthropics/claude-code`)
- Com `--bare`, o Claude exige `ANTHROPIC_API_KEY`, que é proibido neste projeto
- Para manter OAuth, o caminho certo é `--strict-mcp-config`, nunca `--bare`

### Como Fazer

```bash
claude -p "SUA TAREFA AQUI" \
  --strict-mcp-config \
  --mcp-config '{}' \
  --dangerously-skip-permissions \
  --settings '{"hooks":{}}' \
  --model claude-sonnet-4-6 \
  --max-turns 8 \
  --output-format json
```

### O Que Cada Pedaço Faz

- `--strict-mcp-config` ignora MCP de settings global e de projeto
- `--mcp-config '{}'` fornece a lista vazia que zera os servidores
- `--dangerously-skip-permissions` evita travar pedindo confirmação (modo `bypassPermissions`)
- `--settings '{"hooks":{}}'` desliga os hooks naquela chamada específica
- `--model claude-sonnet-4-6` escolhe o modelo sem depender de variável de ambiente
- `--max-turns 8` limita as voltas do agente como rede de segurança contra loop infinito
- `--output-format json` entrega saída fácil de parsear com `jaq`

### Como Garantir o OAuth

- Fazer login uma vez com a conta Pro ou Max antes de automatizar (`claude auth login`)
- NÃO definir `ANTHROPIC_API_KEY` no ambiente da chamada
- NÃO usar `--bare`
- Sem a variável e sem `--bare`, o Claude usa a sessão logada via OAuth

### Ressalva do Bug Conhecido

- Issue #14490 do `anthropics/claude-code` documenta que `--strict-mcp-config` NÃO sobrescreve a lista `disabledMcpServers` armazenada em `~/.claude.json`
- Para ambiente limpo, garantir que `~/.claude.json` não contém o servidor em `disabledMcpServers` ou usar `--bare` somente em ambiente controlado com `ANTHROPIC_API_KEY` (cenário explicitamente PROIBIDO neste projeto)
- A solução robusta é combinar `--strict-mcp-config --mcp-config '{}'` e garantir que o servidor não está em `disabledMcpServers` em `~/.claude.json`


## Codex CLI Headless OAuth sem MCP

### O Que Fazer

Rodar `codex exec` zerando a tabela de servidores MCP do config.

### Por Que Fazer

- O `codex exec` é o modo não interativo feito para scripts
- Ele escreve só a mensagem final no stdout e progresso no stderr
- O override `-c mcp_servers='{}'` substitui a tabela inteira por vazia
- Assim nenhum servidor MCP do `config.toml` sobe naquela chamada

### Como Fazer

```bash
codex exec \
  --model gpt-5.5 \
  -c mcp_servers='{}' \
  --sandbox workspace-write \
  --ask-for-approval never \
  "SUA TAREFA AQUI"
```

### Alternativa Mais Agressiva

- Usar `--ignore-user-config` para nem ler o `config.toml` do usuário
- Isso zera MCP junto com tudo mais que estiver no config
- O login OAuth fica salvo em `auth.json`, que é arquivo separado
- Por isso o `--ignore-user-config` NÃO derruba o login

```bash
codex exec --model gpt-5.5 --ignore-user-config --sandbox workspace-write "SUA TAREFA AQUI"
```

### O Que Cada Pedaço Faz

- `-c mcp_servers='{}'` zera só os MCP e preserva modelo e resto do config
- `--ignore-user-config` é o corte total quando você quer ambiente limpo
- `--sandbox workspace-write` libera edição de arquivos sem rede
- `--ask-for-approval never` roda sem pausar pedindo permissão

### Como Garantir o OAuth

- Rodar `codex login` uma vez para o fluxo do navegador com o ChatGPT
- Em máquina remota ou sem navegador, usar `codex login --device-auth`
- NÃO definir `OPENAI_API_KEY` no ambiente da chamada
- O login fica salvo em `~/.codex/auth.json` e o `codex exec` reaproveita a sessão

### Ressalva do Bug Antigo

- Versões antigas do Codex (0.33.0) instaladas via Homebrew não liam `[mcp_servers]` corretamente
- Issue #3441 do repositório `openai/codex` confirma que o fix está em 0.34.0+
- Validar versão com `codex --version` antes de usar o override `-c mcp_servers='{}'`


## OpenCode Headless sem MCP

### A Diferença Honesta

- O OpenCode NÃO tem uma flag única de CLI para desligar MCP
- O Claude tem `--strict-mcp-config` e o Codex tem `-c mcp_servers='{}'`
- O OpenCode controla MCP só pela config em JSON
- As configs do OpenCode são somadas, não trocadas, então é preciso desligar por servidor

### O Que Fazer

- Descobrir os nomes dos servidores ativos com `opencode mcp list`
- Desligar cada um com `enabled: false` no config

### Por Que Fazer

- O `opencode run` é o modo headless que recebe o prompt e devolve resultado
- Como a config é somada, apagar a chave não basta para remover o servidor
- Setar `enabled` falso com o mesmo nome sobrescreve e desliga aquele MCP
- O override de runtime via `OPENCODE_CONFIG_CONTENT` evita mexer nos arquivos do projeto

### Como Fazer — Passo 1 Listar Servidores Ativos

```bash
opencode mcp list
```

### Como Fazer — Passo 2 Rodar Headless Desligando Cada Servidor

```bash
OPENCODE_CONFIG_CONTENT='{"mcp":{"nome-do-server-1":{"enabled":false},"nome-do-server-2":{"enabled":false}}}' \
  opencode run --model anthropic/claude-sonnet-4-5 "SUA TAREFA AQUI"
```

### Alternativa Permanente

- Editar o `opencode.json` e marcar cada MCP com `enabled` falso
- Vale quando você nunca quer aquele servidor em execução automática

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "nome-do-server-1": { "enabled": false },
    "nome-do-server-2": { "enabled": false }
  }
}
```

### O Que Cada Pedaço Faz

- `opencode mcp list` mostra nomes e status de conexão dos servidores
- `OPENCODE_CONFIG_CONTENT` injeta config inline com alta precedência
- `enabled` falso por servidor é o que de fato impede a subida do MCP
- `--model` escolhe o modelo no formato `provedor/modelo`

### Como Garantir o OAuth

- Rodar `opencode auth login` uma vez e escolher o provedor
- A credencial fica salva em `auth.json` na pasta de dados do OpenCode
- O `opencode run` reaproveita essa credencial nas chamadas seguintes


## Login OAuth por CLI

- Claude: login na sessão via `claude auth login`. NÃO usar `--bare` para preservar OAuth
- Codex: `codex login` ou `codex login --device-auth` (sem navegador)
- OpenCode: `opencode auth login`


## Modo Headless por CLI

- Claude: `claude -p`
- Codex: `codex exec`
- OpenCode: `opencode run`


## Atualização v1.0.80 — Resiliência de SHUTDOWN e a Receita de Bypass em 3 Camadas

A v1.0.80 (ADR-0034) endurece o handler em `src/signals.rs` para que
o cenário de processo órfão que a auditoria G42/C2 identificou
não dispare mais `SIGABRT` em `BrokenPipe`. O terceiro Ctrl-C
consecutivo sai com código 130 e **ZERO I/O**, casando com o
contrato abaixo.

Para jobs longos de embedding que o harness do agente (ou qualquer
orquestrador em background) pode matar via SIGINT, use a receita
de bypass em 3 camadas. As 3 camadas são independentes e a receita
compõe aditivamente:

```bash
# Camada 1 — PATH: roteia o subprocesso LLM via o mock-llm no CI
export PATH="$PWD/tests/mock-llm:$PATH"

# Camada 2 — env: diz ao embedder para ignorar a checagem de SHUTDOWN
export SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1

# Camada 3 — grupo de processos: desanexa a CLI do pgroup do harness
setsid -w timeout 600 \
  sqlite-graphrag remember --graph-stdin < payload.json
```

- **Camada 1 (PATH)**: roteia qualquer `claude -p` ou `codex exec`
  spawned via a mock CLI determinística commitada em
  `tests/mock-llm/`. O subprocesso LLM real é desviado; SIGINT não
  consegue matar um subprocesso que não existe. É a camada mais
  barata e o default certo em CI.
- **Camada 2 (env)**: faz o `if should_obey_shutdown()` do embedder
  curto-circuitar para `true`, então o braço de cancelamento do
  `tokio::select!` é descartado e o batch roda até a conclusão
  mesmo se o cancellation token já estiver cancelled. Zero
  overhead em produção porque a leitura da env é um único
  `std::env::var` por chamada de `should_obey_shutdown()`, não
  em hot path.
- **Camada 3 (setsid)**: dá à CLI seu próprio grupo de processos via
  `setsid -w`, então SIGINT do harness pai não se propaga para o
  filho. `timeout` adiciona um teto rígido de wall-clock (binário
  Rust `timeout-cli` v0.1.0, somente inteiros em segundos —
  `600` é 10 minutos; não passe `10m`).

A receita é agora a referência canônica para qualquer harness de
agente rodando jobs longos de embedding em background. O bypass é
explicitamente opt-in: código de produção NUNCA deve chamar
`try_reset_shutdown()`, e a env var NUNCA deve ser setada em
produção. Tests e invocações de auditoria são os únicos
consumidores válidos.

Se a execução for interrompida entre as camadas, o arquivo SQLite
permanece consistente (WAL, commit atômico, sem escritas
parciais), e `restore` ou `enrich --operation re-embed --resume`
podem retomar a partir da última memória bem-sucedida.

## Camada de Validação Pre-Flight em Invocação Headless (v1.0.87, ADR-0045, GAP-META-005)
- O módulo `src/spawn/preflight.rs` (≥200 linhas, 7 guards, 15 testes unitários) porta todo spawn de subprocesso LLM ANTES do fork
- Os 7 guards em ordem: `check_argv_size`, `check_binary_exists`, `check_mcp_config_inline`, `check_mcp_config_path`, `check_walkup_mcp_json`, `check_output_buffer`, `check_claude_config_dir`
- Falhas retornam `AppError::PreFlightFailed(PreFlightError)` com `exit_code() == 16` (`EX_CONFIG`, `is_permanent() == true`)
- A variante `McpConfigInlineJsonRejected` (Bug 2 do GAP-META-005) é crítica em invocação headless: Claude Code 2.1.177 rejeita `--mcp-config '{}'` literal. O preflight substitui automaticamente por tempfile com `{"mcpServers":{}}`
- A variante `WalkUpMcpJsonInvalid` (Bug 5) detecta `.mcp.json` inválido em diretórios ancestrais do CWD — walk-up de até 16 níveis via `std::path::Path::ancestors()`
- A variante `ArgvExceedsArgMax` (Bug 3) protege contra `E2BIG` pós-fork para corpos de memória grandes. Threshold: `ARG_MAX - 4096` bytes (margem de 4 KB para env vars do kernel)
- A variante `BinaryNotFound` verifica que `claude` ou `codex` está em PATH antes do fork. Usa `which::which` em POSIX e `where` em Windows
- Bypass em emergências: `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` desabilita todos os 7 guards. Bypass reverte para `Command::spawn()` direto e herda todas as 5 classes BUG do GAP-META-005
- O preflight compartilha o helper `apply_env_whitelist` (ADR-0041) — ordem de execução: env_clear primeiro, depois preflight
- Cada spawner adiciona uma única linha antes de `cmd.spawn()`: `preflight_check(&PreFlightArgs { ... }).map_err(|e| AppError::PreFlightFailed(e))?`
- Telemetria: `tracing::info!(event = "preflight_passed", spawner = %name, argv_bytes = total)` em sucesso; `tracing::warn!(event = "preflight_failed", spawner = %name, error = %e)` em falha
- Veja `docs/decisions/adr-0045-preflight-validation-layer.md` (en + pt-BR) para a justificativa arquitetural completa

## Hotfixes BUG-11/12/13 em Invocação Headless (v1.0.88, ADR-0046, ADR-0047)
- **BUG-11 (CRÍTICO)**: falha pre-flight em `extract/llm_embedding.rs:563-565` não propagava para `remember`, que silenciosamente persistia a memória com `backend_invoked: "none"` e zero chunks. Corrigido com `embed_via_backend_strict`. Repro: `CLAUDE_CONFIG_DIR=/tmp/bad-config-with-mcp remember --name X --type note --description x --body y` retorna exit 11 + envelope JSON de erro
- **BUG-12 (MÉDIO)**: enforço OAuth-only emitia 2 linhas stderr idênticas (uma de `tracing::error!`, uma de `eprintln!`). Corrigido removendo `eprintln!` duplicado em `src/output.rs`. Teste: `oauth_stderr_emits_single_line_v1088`. Repro: `ANTHROPIC_API_KEY=sk-test /path/bin/sqlite-graphrag init` agora emite 1 linha stderr (eram 2)
- **BUG-13 (MÉDIO)**: `link --create-missing` bypassava validação de nome de entidade. Corrigido validando ANTES de normalizar em `src/commands/link.rs`. 8 testes em `tests/entity_validation_integration.rs`
- Nova variante `AppError::PreFlightFailed(PreFlightError)` com `exit_code() == 16` e `is_permanent() == true`. Substitui os 3 spawners chamando `std::process::exit(16)` diretamente
- Veja `docs/decisions/adr-0046-preflight-remediation.md` e `adr-0047-stderr-deduplication.md` (en + pt-BR)

## Schema Drift e Flag Parity para Agentes Headless (v1.0.89, ADR-0048, ADR-0049)
- `health.schema.json` regenerado via `schemars 0.8` derive macro. 17 novos campos adicionados. `additionalProperties: true` (política Must-Ignore por RFC 7493 I-JSON)
- Agentes que validam resposta de `health --json` devem migrar de `additionalProperties: false` (strict) para Must-Ignore para receber benefícios de evolução de schema
- `--db <PATH>` agora aceito em `embedding status`, `embedding list`, `embedding abandon`, `pending list`, `pending show` — operadores headless podem apontar para múltiplos bancos sem flag global
- `codex-models --json` retorna envelope JSON `{"action":"codex_models","count":N,"default":"...","models":[...]}`
- `migrate --dry-run --json` reporta migrações pendentes sem aplicar. Adicionado `--confirm` para exigir confirmação literal antes de apply
- `ingest --auto-describe` (padrão true) extrai descrição da primeira linha significativa do corpo. Substitui a antiga `"ingested from <path>"` genérica
- `health --namespace <NS> --json` filtra contagens para um único namespace — útil em ambientes multi-tenant
- Binário medido em 15.323.128 bytes (14.6 MiB), dentro de 1 MiB do documentado em `Cargo.toml:6`. Drift viral "6 MB" eliminado
- 1877 testes passando (843 lib + 1013 integração + 21 doc)

## Atualização v1.0.89 — Propagação de Flags LLM e Seleção de Modelo (ADR-0050)

A v1.0.89 corrige uma classe crítica de bugs de flag morta: 7 flags
globais de CLI eram aceitas pelo clap mas nunca propagadas para os
módulos internos de embedding. Todas as 7 agora funcionam via CLI ou
variável de ambiente.

### Flags globais novas e corrigidas

- `--llm-model <MODEL>` / `SQLITE_GRAPHRAG_LLM_MODEL` — seleciona o
  modelo de embedding. Padrões: `gpt-5.5` (codex), `claude-sonnet-4-6`
  (claude). Sobrescreve as variáveis por backend
  `SQLITE_GRAPHRAG_CODEX_EMBED_MODEL` e
  `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL`
- `--llm-backend <auto|codex|claude|none>` /
  `SQLITE_GRAPHRAG_LLM_BACKEND` — seleciona qual CLI spawna o
  subprocesso de embedding. `auto` (padrão) sonda o PATH: codex
  primeiro, depois claude
- `--codex-binary <PATH>` / `SQLITE_GRAPHRAG_CODEX_BINARY` —
  sobrescreve a localização do binário codex (novo na v1.0.89;
  `--claude-binary` existe desde a v1.0.82)
- `--llm-fallback <chain>` / `SQLITE_GRAPHRAG_LLM_FALLBACK` — cadeia
  de fallback quando o backend primário falha (padrão:
  `codex,claude,none`)
- `--skip-embedding-on-failure` /
  `SQLITE_GRAPHRAG_SKIP_EMBEDDING_ON_FAILURE` — persiste a memória sem
  embedding quando o LLM falha (exit 0 em vez de exit 11)
- `--llm-max-host-concurrency <N>` /
  `SQLITE_GRAPHRAG_LLM_MAX_HOST_CONCURRENCY` — limita os subprocessos
  LLM concorrentes em todo o host
- `--llm-slot-wait-secs <N>` / `SQLITE_GRAPHRAG_LLM_SLOT_WAIT_SECS` —
  segundos para esperar por um slot livre antes de falhar
- `--llm-slot-no-wait` / `SQLITE_GRAPHRAG_LLM_SLOT_NO_WAIT` — falha
  imediatamente se nenhum slot estiver disponível

### BoolishValueParser para env vars booleanas

Flags booleanas com `env = "SQLITE_GRAPHRAG_*"` agora aceitam `1`,
`yes`, `on`, `true` (e `0`, `no`, `off`, `false`). Antes só
`true`/`false` eram aceitos, causando exit 2 para scripts que setavam
`SQLITE_GRAPHRAG_SKIP_EMBEDDING_ON_FAILURE=1`.

### Invocação headless com modelo explícito

```bash
# Claude com modelo explícito
claude -p "SUA TAREFA" \
  --model claude-sonnet-4-6 \
  --strict-mcp-config --mcp-config '{}' \
  --dangerously-skip-permissions \
  --settings '{"hooks":{}}' \
  --output-format json

# Codex com modelo explícito
codex exec \
  --model gpt-5.5 \
  -c mcp_servers='{}' \
  --sandbox workspace-write \
  --ask-for-approval never \
  "SUA TAREFA"
```

### sqlite-graphrag com override de backend e modelo

```bash
# Força o backend claude com modelo específico
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name example --type note --body "text" --json

# Força o backend codex com modelo específico
sqlite-graphrag --llm-backend codex --llm-model gpt-5.5 \
  recall "query" --k 5 --json

# Pula o embedding em caso de falha (persiste a memória sem vetor)
sqlite-graphrag --skip-embedding-on-failure \
  remember --name resilient --type note --body "text" --json
```


## Referências Externas Validadas

### Claude Code

- `code.claude.com/docs/en/headless` — modo headless e exit codes claros
- `amux.io/guides/claude-code-headless/` — guia completo de self-hosting headless (2026)
- `github.com/anthropics/claude-code/issues/39069` — `--bare` mode skips OAuth/keychain, unusable para OAuth-only
- `computingforgeeks.com/claude-code-cheat-sheet/` — cheat sheet com `--mcp-config` e `--strict-mcp-config`
- `github.com/anthropics/claude-code/issues/14490` — `--strict-mcp-config` não sobrescreve `disabledMcpServers`

### Codex CLI

- `developers.openai.com/codex/cli/reference` — referência canônica de CLI options
- `deepwiki.com/openai/codex/6.1-mcp-server-configuration` — MCP server config no `config.toml`
- `ofox.ai/blog/codex-cli-config-toml-deep-dive/` — cada setting do `config.toml` explicado
- `github.com/openai/codex/issues/3441` — bug de `[mcp_servers]` não funcionar em versão antiga do Codex

### OpenCode

- `opencode.ai/docs/mcp-servers/` — controle de MCP via `enabled: false` por servidor
- `open-code.ai/en/docs/config` — referência de `opencode.json` com providers, models, MCP
- `computingforgeeks.com/opencode-cli-cheat-sheet/` — cheat sheet com flags headless e MCP
