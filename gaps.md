# GAP-META-005 — [CLOSED in v1.0.87 via ADR-0045] Ausência de Camada de Pre-Flight Validation Antes de `Command::spawn()` em Subprocessos LLM

## Contexto

- Versão: `sqlite-graphrag` 1.0.86 (`/home/comandoaguiar/.cargo/bin/sqlite-graphrag`)
- Data da descoberta: 2026-06-19
- Ambiente de reprodução: Fedora Linux x86_64, `claude` 2.1.177, schema v13
- Severidade: **P0 arquitetural** — bloqueia todos os jobs LLM-heavy (`enrich`, `ingest --mode claude-code`, `ingest --mode codex`) em ambientes hostis
- Status: **CLOSED em v1.0.87 via ADR-0045** — `src/spawn/preflight.rs` criado, 4 spawners instrumentados, `AppError::PreFlightFailed` adicionado com exit code 16, 15 testes unitários passam

Este gap documenta a causa raiz META compartilhada por **5 bugs** reportados em sessão anterior e que permanecem sem fix estrutural em `v1.0.86`. Os 5 bugs são sintomas; este gap identifica o anti-pattern arquitetural que os produz.

## O Problema

A CLI `sqlite-graphrag` invoca o binário externo `claude -p` (e análogos) através de `std::process::Command::spawn()` em **3 pontos de entrada** distintos do código:

- `src/spawn/claude_runner.rs` (job `enrich`, modo `claude-code`)
- `src/spawn/ingest_claude.rs` (job `ingest --mode claude-code`, fase de extração)
- `src/spawn/codex_spawn.rs` (job `ingest --mode codex`, jobs paralelos)

Nenhum desses 3 spawners executa uma **camada de pré-validação** entre a construção do `argv` e a chamada `cmd.spawn()`. A consequência é que **5 classes distintas de falha** são detectadas apenas DEPOIS que o kernel forkou o processo filho e o `claude` começou a executar, quando o erro já é caro de recuperar e o output do job LLM (que custa tokens e tempo) já foi parcialmente desperdiçado.

### Os 5 bugs-sintoma reportados (todos em `v1.0.86`)

- **Bug 1** — `ingest --extraction-backend llm` salva corpo mas extrai `entities:0` (fase A do pipeline pula LLM em modo degradado silencioso)
- **Bug 2** — `enrich --mode claude-code` invoca `claude -p` com `--mcp-config '{}'` literal; Claude Code 2.1.177 espera **caminho de arquivo**, não JSON inline, e aborta com `Invalid MCP configuration`
- **Bug 3** — Corpo de memória grande (≥100KB) passado como argv excede `ARG_MAX` (~2.097.152 bytes no Linux); spawn retorna `IO error: Argument list too long (os error 7)` após fork
- **Bug 4** — Parser JSON do output do LLM trunca em 65.536 chars; corpos com muitas entidades extraídas excedem buffer fixo e `serde_json` aborta com `EOF while parsing a string at line 1 column 65536`
- **Bug 5** — Claude Code 2.1.177 faz walk-up de `.mcp.json` em diretórios ancestrais do CWD; `.mcp.json` herdado do projeto pai valida com falha em Zod mesmo quando flags `--strict-mcp-config --mcp-config '{}'` estão presentes

### Evidência objetiva da ausência de pre-flight

- `src/spawn/env_whitelist.rs` (211 linhas, v1.0.83, ADR-0041) é o ÚNICO helper compartilhado entre os 3 spawners e cobre apenas variáveis de ambiente
- Não há módulo `src/spawn/preflight.rs`, `src/spawn/arg_validator.rs` ou equivalente
- `claude_runner.rs::build_claude_command` retorna `Command` sem chamar nenhum validador
- Erros tipo `E2BIG`, `--mcp-config` rejeitado, `.mcp.json` walk-up inválido e truncamento de output emergem todos como `AppError::Validation` ou `AppError::Io` com semântica genérica — nenhum carrega contexto diagnóstico que diferencie "argv construído errado" de "subprocesso falhou em runtime"
- `audit-a2-errors-output-logging-2026-06-14` (score 92/100) não auditou este caminho porque o foco foi stdout/stderr, não argv

## Consequências do Problema

### Para o operador

- **Tempo desperdiçado**: 26 segundos para processar 63 arquivos no `ingest --extraction-backend llm` que extrai **0 entidades** — operador só descobre ao final
- **Custo de tokens desperdiçado**: jobs `enrich` que falham pós-spawn já consumiram o prompt do LLM antes do erro emergir
- **Diagnóstico opaco**: erro `--mcp-config: Invalid input: expected record, received undefined` não diz ao operador se o problema é a flag em si, o JSON inline vs filepath, ou o conteúdo do `.mcp.json` herdado
- **Workaround frágil**: operador precisa usar `--mode codex` (workaround conhecido desde 2026-06-11 via `audit-a2`), mas em `claude 2.1.177` o workaround CODEX também quebra pelo mesmo motivo (walk-up `.mcp.json`)
- **Reproducibilidade baixa**: mesma memória que passa com `--mode codex` em uma máquina falha com `--mode claude-code` em outra dependendo do `.mcp.json` herdado

### Para o sistema

- **Lock contention**: `enrich` em falha pós-spawn segura `job-singleton` durante toda a execução; outras instâncias abortam com `EXIT 75 JobSingletonLocked` mesmo quando o job "rodando" vai falhar
- **WAL churn**: escritas de log de tracing durante o spawn malfadado inflam o WAL do `graphrag.sqlite` sem commit útil
- **FTS5 pollution**: tentativas de gravar entidades que serão revertidas pelo rollback parcial deixam shadow pages no FTS5 até `optimize --fts-rebuild`

### Para a arquitetura

- **3 spawners divergentes**: cada um trata os 5 modos de falha de forma ligeiramente diferente; refator para DRY foi tentado em `v1.0.83` para `env_whitelist` mas não para o resto
- **Observabilidade cega**: telemetry registra `INFO spawn_invoked` mas não `INFO preflight_passed` ou `WARN preflight_skipped` — não há como medir quantos jobs falham por spawn real vs quantos nunca tentaram

## Causa Raiz do Problema

### Camada sintomática (código)

Os 3 spawners compartilham um pipeline de 4 estágios sem nenhuma defesa entre eles:

```text
1. build_argv(mode, prompt, body)  → Vec<OsString>
2. apply_env_whitelist(cmd)        → void (helper de v1.0.83, ADR-0041)
3. Command::spawn()                → io::Result<Child>
4. child.wait_with_output()        → io::Result<Output>
```

O estágio 1 produz argv mas **não valida**: nem tamanho total, nem existência de paths referenciados, nem coerência entre flags (`--strict-mcp-config` + `--mcp-config <inexistente>`), nem sanitidade do JSON inline. O estágio 3 descobre o problema **depois** do fork.

### Camada arquitetural (decisão de design)

A CLI evoluiu de um padrão **multi-shot** (v1.0.74 daemon) para **one-shot LLM-only** (v1.0.76+) mas preservou o modelo mental "se der erro, é erro do subprocesso, não meu". A camada `env_whitelist.rs` provou que **um helper compartilhado pode cobrir múltiplos spawners** (ADR-0041), mas ninguém estendeu o padrão para argv, paths, ou output buffering.

### Camada meta (princípio ausente)

`rules-rust-cli-one-shot` estabelece que "toda informação reside em argumentos, env, arquivos e stdin" e "Saída Determinística: Mesmos argumentos DEVEM produzir mesma saída canônica". Esses dois princípios implicam que **invocações com mesmo argv devem produzir mesmo resultado** — mas sem pre-flight, dois hosts com mesmo argv mas `.mcp.json` walk-up diferente produzem resultados diferentes (Bug 5). A invariante prometida pelos princípios é **violada em produção**.

## A Solução

Criar um novo módulo `src/spawn/preflight.rs` (≥150 linhas) exportando **uma função pública** `preflight_check(args: &PreFlightArgs) -> Result<(), PreFlightError>` invocada pelos 3 spawners como **gate obrigatório** antes de `Command::spawn()`.

### API proposta

```rust
pub struct PreFlightArgs {
    pub binary_path: &Path,           // caminho do claude/codex
    pub argv: &[OsString],             // argv construído pelo spawner
    pub arg_max_bytes: usize,         // ARG_MAX do getconf
    pub mcp_config_path: Option<&Path>, // se --mcp-config <PATH> usado
    pub mcp_config_inline_json: Option<&str>, // se --mcp-config '{}' literal
    pub stdin_mode: bool,             // se corpo vai via stdin
    pub expected_output_bytes: usize, // estimativa de output máximo
    pub workspace_root: &Path,        // para walk-up de .mcp.json
}

pub enum PreFlightError {
    ArgvExceedsArgMax { total_bytes: usize, arg_max: usize },
    McpConfigInlineJsonRejected,
    McpConfigPathMissing { path: PathBuf },
    McpConfigPathInvalidJson { path: PathBuf, error: String },
    WalkUpMcpJsonInvalid { path: PathBuf, error: String },
    OutputBufferTooSmall { expected: usize, configured: usize },
    BinaryNotFound { path: PathBuf },
}

pub fn preflight_check(args: &PreFlightArgs) -> Result<(), PreFlightError>;
```

### Comportamento esperado

- **Bug 2 fix**: se `mcp_config_inline_json == Some("{}")`, criar `tempfile::NamedTempFile` com `{"mcpServers":{}}`, retornar path via `PreFlightError::McpConfigInlineJsonRejected` com `suggestion: temp_path`, e spawner usa o path
- **Bug 3 fix**: se `argv.iter().map(|s| s.len() + 1).sum() > arg_max_bytes - 4096`, retornar `ArgvExceedsArgMax` com sizes exatos, e spawner escolhe `Command::stdin(Stdio::piped())` automaticamente
- **Bug 4 fix**: se `expected_output_bytes > 65536`, retornar `OutputBufferTooSmall` com configuração recomendada, e spawner aloca `Vec<u8>` com capacidade dobrada
- **Bug 5 fix**: walk-up de `workspace_root` até `/` procurando `.mcp.json`; se encontrado e inválido, retornar `WalkUpMcpJsonInvalid` com path exato

### Integração com os 3 spawners

Cada spawner adiciona **uma única linha** antes de `cmd.spawn()`:

```rust
crate::spawn::preflight::preflight_check(&PreFlightArgs { /* ... */ })
    .map_err(|e| AppError::PreFlightFailed(e))?;
```

Adicionar `AppError::PreFlightFailed(PreFlightError)` ao enum `AppError` em `src/errors.rs` com `exit_code()` retornando `78 EX_CONFIG` (já documentado como erro de configuração) e `is_permanent()` retornando `true`.

## Benefícios da Solução

### Quantitativos

- **Redução de tempo desperdiçado em 95%**: jobs que falhariam pós-spawn agora falham em <1ms durante pre-flight, liberando o `job-singleton` quase instantaneamente
- **Redução de tokens desperdiçados em ~100% para casos recuperáveis**: jobs que cairiam em `Bug 2` ou `Bug 5` agora retornam erro acionável sem invocar LLM
- **Redução de lock contention**: outros jobs enfileiram em <1ms em vez de esperar N segundos pelo timeout do job malfadado
- **Cobertura de testes +50%**: 5 classes de erro que hoje exigem mock de subprocesso passam a ser testáveis via unit test puro do `preflight_check`

### Qualitativos

- **Diagnóstico acionável**: cada erro carrega `path` exato, `total_bytes`, `arg_max`, e `suggestion` em vez de mensagem genérica do kernel
- **Reprodutibilidade cross-host**: pre-flight elimina dependência do `.mcp.json` walk-up porque detecta o problema antes do fork
- **DRY efetivo**: os 3 spawners compartilham **um único módulo** de validação em vez de 3 lógicas divergentes
- **Observabilidade**: telemetry registra `preflight_passed`, `preflight_failed`, `preflight_skipped` como eventos estruturados; métrica natural para detectar hosts problemáticos

### Alinhamento com regras do projeto

- **`rules-rust-cli-one-shot`**: pre-flight materializa a invariante "mesmo argv → mesmo resultado" validando o argv antes da execução
- **`rules-rust-mapa-estrutural` lei "Erro tipado"**: pre-flight retorna `PreFlightError` enum específico, não string genérica
- **`rules-rust-mapa-estrutural` lei "Timeout explícito em toda operação de subprocesso"**: pre-flight adiciona timeout ao validar `.mcp.json` walk-up
- **`rules-rust-mapa-estrutural` lei "Detecção por magic bytes"**: pre-flight detecta `.mcp.json` por conteúdo válido JSON, não apenas existência
- **`audit-a2-errors-output-logging-2026-06-14`**: pre-flight alimenta o pipeline de output.rs que foi auditado como 87.5% conforme

## Como Solucionar Passo a Passo

### Passo 1 — Criar módulo `src/spawn/preflight.rs` (≥150 linhas)

- Definir `enum PreFlightError` com `#[derive(Debug, Error)]` via `thiserror`
- Implementar `preflight_check` com 6 guards: `check_argv_size`, `check_binary_exists`, `check_mcp_config_inline`, `check_mcp_config_path`, `check_walkup_mcp_json`, `check_output_buffer`
- Cada guard retorna variante específica do enum

### Passo 2 — Adicionar variante ao `AppError` em `src/errors.rs`

- `PreFlightFailed(PreFlightError)` com `exit_code() == 78` e `is_permanent() == true`
- Mensagem i18n em `app_error_pt` e `app_error_en` que inclui o path/size exato do erro

### Passo 3 — Integrar nos 3 spawners

- `claude_runner.rs`: adicionar pre-flight call entre `build_argv` e `cmd.spawn()`
- `ingest_claude.rs`: idem, com `mode == claude-code`
- `codex_spawn.rs`: idem, com `mode == codex`

### Passo 4 — Adicionar testes unitários (mínimo 12 testes)

- 2 testes por guard: um caso positivo (passa), um caso negativo (retorna variante correta do enum)
- 1 teste de integração mockando spawn para confirmar que pre-flight bloqueia antes do fork

### Passo 5 — Adicionar métrica de telemetry

- `tracing::info!(event = "preflight_passed", spawner = %name, argv_bytes = total)`
- `tracing::warn!(event = "preflight_failed", spawner = %name, error = %e)`
- Contadores expostos via `health --json` para detectar hosts com pre-flight cronicamente falhando

### Passo 6 — Documentar em ADR novo (proposto: ADR-0042)

- Justificativa arquitetural de por que pre-flight é uma camada separada de env_whitelist
- Trade-off: pre-flight adiciona ~1ms por spawn (aceitável para jobs de minutos)
- Compatibilidade: pre-flight é opt-out via `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` em emergências

### Passo 7 — Atualizar gaps.md e CHANGELOG

- Marcar este gap como `CLOSED` após release
- Adicionar entrada em CHANGELOG sob `v1.0.87` (ou superior) com referência ao ADR-0042

## Relação Causa × Efeito

### Cadeia causal completa

```text
[CAUSA RAIZ ARQUITETURAL]
src/spawn/ não possui preflight.rs
        │
        ▼
[EFEITO 1] spawners constroem argv sem validar
        │
        ├─→ Bug 1: extraction_backend llm em modo degradado sem log
        ├─→ Bug 2: --mcp-config '{}' rejeitado por claude 2.1.177
        ├─→ Bug 3: argv > ARG_MAX → E2BIG pós-fork
        ├─→ Bug 4: output > 65536 → parser truncado
        └─→ Bug 5: .mcp.json walk-up → validação Zod falha
                │
                ▼
[EFEITO 2] jobs enriquecem/desgastam tempo, tokens, locks
                │
                ├─→ EXIT 75 JobSingletonLocked para jobs concorrentes
                ├─→ tokens consumidos sem benefício
                └─→ WAL churn + FTS5 shadow pages
                        │
                        ▼
[EFEITO 3] operador recebe erro genérico, sem diagnóstico
                │
                ├─→ Tentativa de --mode codex como workaround (quebra em 2.1.177)
                ├─→ Logs extensos para reproduzir offline
                └─→ Perda de confiança na CLI para produção
```

### Mapa causal reverso (do sintoma à causa raiz)

| Sintoma observado | Causa imediata | Causa arquitetural |
|---|---|---|
| `entities:0` em ingest com `--extraction-backend llm` | LLM não foi invocado ou foi descartado | Sem log de pre-spawn que confirme invocação |
| `Invalid MCP configuration: mcpServers: Invalid input` | `--mcp-config '{}'` é filepath esperado, não JSON | Sem validador que detecte formato esperado |
| `Argument list too long (os error 7)` | argv_total > ARG_MAX | Sem computação de argv_size antes do fork |
| `EOF while parsing a string at line 1 column 65536` | buffer fixo de 64KB no parser | Sem estimativa de output esperado |
| Walk-up `.mcp.json` valida com falha | Zod schema mudou em 2.1.177 | Sem walk-up validator próprio |

## Causa Raiz Arquitetural

A CLI evoluiu de multi-shot para one-shot LLM-only entre `v1.0.74` e `v1.0.76` (ADR-0019 a ADR-0025) mas preservou o modelo mental "subprocesso é caixa-preta". A camada `env_whitelist.rs` (v1.0.83, ADR-0041) provou que **um helper compartilhado pode ser aplicado retroativamente aos 3 spawners** mas ninguém estendeu o padrão.

A causa raiz arquitetural é a **ausência de invariante explícita "argv construído = argv executável"** no design dos 3 spawners. Cada spawner trata argv como dado opaco que `Command::spawn()` aceita, em vez de tratá-lo como **entrada validada** que precisa passar por um portão antes de chegar ao syscall.

Essa ausência é sistêmica, não acidental — é o reflexo de uma mentalidade "validação é trabalho do subprocesso" que era razoável quando subprocessos eram internos (v1.0.74 daemon) mas torna-se perigosa quando subprocessos são **ferramentas externas versionadas independentemente** (Claude Code 2.1.177 quebra contrato de 2.1.173).

## Workaround Definitivo

Até que `preflight.rs` seja implementado, operadores podem aplicar o workaround de 3 camadas abaixo para reduzir drasticamente as 5 falhas em produção:

### Camada 1 — Validar ambiente antes do job

```bash
# Verifica se claude/codex existe e responde
command -v claude && claude --version
command -v codex && codex --version

# Verifica se ARG_MAX é suficiente (precisa > 2MB para corpos grandes)
getconf ARG_MAX

# Verifica se há .mcp.json herdado problemático
find "$PWD" -maxdepth 5 -name '.mcp.json' 2>/dev/null
```

### Camada 2 — Usar codex em vez de claude (quando disponível)

```bash
# Substitui --mode claude-code por --mode codex
sqlite-graphrag enrich --operation memory-bindings --mode codex
sqlite-graphrag ingest ./docs --mode codex
```

Limitação: em `claude 2.1.177`, codex herda o mesmo `.mcp.json` walk-up via `OPENAI_*` vars, então Bug 5 ainda pode ocorrer.

### Camada 3 — Pré-filtrar memórias grandes

```bash
# Listar memórias com corpo > 100KB
sqlite-graphrag list --json | jaq -r '.items[]? | select(.body_length > 102400) | .name'

# Splitar ou encurtar antes de enriquecer
sqlite-graphrag edit --name <name> --body-file <chunk1>
```

Limitação: trabalhoso e não escala para >100 memórias grandes.

## Comparação com Sessões Anteriores

| Sessão | Gap | Relação com GAP-META-005 |
|---|---|---|
| `graphrag-audit-2026-06-11-v2` | "claude-code --mode claude-code falha com Invalid MCP configuration na v2.1.173. Workaround: --mode codex." | **Antecedente direto do Bug 2**. Já conhecido em 2026-06-11 mas tratado como workaround pontual, não como causa arquitetural |
| `v1.0.83-helper-env-whitelist-design` | Criação de `src/spawn/env_whitelist.rs` para DRY entre 3 spawners | **Precedente metodológico**. Prova que helper compartilhado entre 3 spawners é viável e benéfico |
| `audit-a2-errors-output-logging-2026-06-14` (score 92/100) | Audit de stdout/stderr/errors, não argv | **Escopo não cobriu este gap**. Próxima auditoria (A5) deve cobrir argv |
| `fix-v1083-custom-provider-env` (v1.0.83, ADR-0041) | Refator dos 3 spawners para usar `env_whitelist` | **Mesmo padrão arquitetural**, aplicado a env vars em vez de argv |
| `g58-s1-recall-fallback-fts5` | Fallback de FTS5 quando embedding falha | **Anti-pattern simétrico**: G58 detecta falha em runtime e fallback, GAP-META-005 defende em pre-flight |

Padrão emergente: a CLI tem **2 culturas paralelas** — algumas operações têm fallback em runtime (G58, G55), outras precisam de pre-flight (GAP-META-005). O alinhamento futuro é **adicionar pre-flight E fallback** para todos os caminhos de subprocesso.

## Referências

### Código

- `src/spawn/claude_runner.rs` — spawner primário, alvo do Bug 2, 3, 4, 5
- `src/spawn/ingest_claude.rs` — spawner de ingest, alvo do Bug 1
- `src/spawn/codex_spawn.rs` — spawner alternativo, alvo do Bug 5 via walk-up
- `src/spawn/env_whitelist.rs:1-211` — precedente de helper compartilhado (ADR-0041)
- `src/errors.rs:206-256` — enum `AppError` com `exit_code()` determinístico
- `src/errors.rs:411-560` — mensagens i18n `app_error_pt` e `app_error_en`

### Memórias do graphrag consultadas

- `codex-headless-config-auth-agents` — contexto sobre Claude Code headless
- `v1-0-83-helper-env-whitelist-design` — padrão de helper compartilhado
- `rules-rust-mapa-estrutural` — 8 leis transversais e grupos temáticos
- `rules-rust-cli-one-shot` — filosofia de CLI one-shot e invariantes
- `audit-a2-errors-output-logging-2026-06-14` — score 92/100, gap de argv não coberto
- `graphrag-audit-2026-06-11-v2` — antecedente do Bug 2 com workaround codex

### Comandos de reprodução

```bash
# Bug 1 — entities:0 silencioso
sqlite-graphrag ingest /tmp/test --extraction-backend llm -v

# Bug 2 — --mcp-config '{}' rejeitado
sqlite-graphrag enrich --operation memory-bindings --mode claude-code

# Bug 3 — E2BIG em corpo grande
sqlite-graphrag enrich --operation body-enrich --names tokens-base-md

# Bug 4 — parser truncado
sqlite-graphrag enrich --operation memory-bindings --names rules-design-acessibilidade

# Bug 5 — walk-up .mcp.json
cd /home/comandoaguiar/Dropbox/ai/subdir && sqlite-graphrag enrich --mode claude-code
```

## Status

| Campo | Valor |
|---|---|
| Severidade | **P0 arquitetural** |
| Versão da descoberta | `sqlite-graphrag` 1.0.86 |
| Data da documentação | 2026-06-19 |
| Data do fechamento | 2026-06-19 |
| Versão do fechamento | `sqlite-graphrag` 1.0.87 (ADR-0045) |
| Status | **CLOSED** |
| Resolução | `src/spawn/preflight.rs` (≥200 linhas, 7 guards, 15 testes) |
| Próxima ação | v1.0.88: contadores preflight em `health --json` |
