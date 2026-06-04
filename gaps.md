# Gaps — sqlite-graphrag v1.0.68

## Resumo Executivo

- **1 gap CRITICAL documentado nesta iteração**: G28 — Proliferação descontrolada de processos ao executar `enrich` e `ingest --mode claude-code` em paralelo
- **Data do incidente original**: 2026-06-03
- **Versão da CLI no momento do incidente**: v1.0.68 (em validação), commit `9ddb17b`
- **Máquina afetada**: Fedora kernel 7.x, 8-10 CPUs lógicos, daemon de embeddings `graphrag` ativo
- **Impacto mensurável observado**:
  - Load average: **276** (27× o número de CPUs disponíveis)
  - Total de processos no sistema: **1.877**
  - Subárvore de processos gerada pelo `enrich`: **~192 processos**
  - Memória em compressor (zram/swap): **13 GB**
  - Comando `remember` travou sem persistir (timeout no `acquire_cli_slot`)
- **Mitigação aplicada manualmente pelo operador**:
  - Etapa 1 — `pkill` dos processos `enrich` em execução → load caiu de 276 para 6,74
  - Etapa 2 — `pkill` dos MCPs e `node` órfãos → 1.958 → 1.857 processos
  - Confirmação — CPU ociosa subiu de 0% para 68%, `remember` voltou a completar em 359 ms
- **Status atual do gap**: **NÃO corrigido**, aberto em código, aguardando priorização em sprint futuro

---

## G28 CRITICAL — Proliferação descontrolada de processos ao executar `enrich` e `ingest --mode claude-code` em paralelo

### Problema

A CLI sqlite-graphrag adota um modelo de processo efêmero por invocação (one-shot) para todos os 27 subcomandos. Este modelo é inofensivo para comandos leves como `recall`, `hybrid-search` e `stats`, porque esses comandos são folhas da árvore de processos — nascem, executam a tarefa, morrem, sem filhos pesados.

O problema surge quando um comando que era folha evolui para a raiz de uma árvore de processos. Os comandos `enrich` (v1.0.65) e `ingest --mode claude-code` (v1.0.62) executam, para cada item a processar, um subprocesso `claude -p` em modo headless. Cada `claude -p`, quando invocado sem isolamento de configuração MCP, herda automaticamente o conjunto completo de servidores MCP declarados em `~/.claude/settings.json` do operador. Em uma instalação típica com 10 servidores MCP configurados, cada invocação headless de `claude -p` dispara a subida de 10 processos `npm exec` que, por sua vez, iniciam 10 processos `node` — totalizando 20 processos filhos por `claude -p`.

A CLI não implementa governança de ciclo de vida para essa árvore emergente. Não há singleton de jobs pesados (4 instâncias de `enrich` podem rodar simultaneamente, limitadas apenas pelo semáforo genérico de 4 slots em `lock.rs`). Não há isolamento de configuração dos subprocessos (as flags `--strict-mcp-config --mcp-config '{}'` não são passadas no spawn). Não há reaping conjunto entre o processo pai e os subprocessos spawnados (a CLI usa `std::process::Command` síncrono, sem `kill_on_drop` e sem watcher de morte do pai). Não há circuit breaker para o flag `--retry-failed` (que pode entrar em loop infinito em falhas persistentes). Não há reaper de órfãos no startup que varra subprocessos de execuções anteriores interrompidas.

A consequência cumulativa é uma explosão descontrolada da árvore de processos. Em uma máquina com 10 CPUs, uma sessão típica de enriquecimento paralelo dispara **4 instâncias de `enrich` × 2 workers LLM cada (default de `--llm-parallelism`) × 10 servidores MCP × 2 processos por servidor = 160 processos na subárvore do `enrich`**, mais aproximadamente 32 processos auxiliares (próprios `claude -p`, watchers internos do Claude Code, file watchers). Esses 192 processos competem com os 1.685 processos de sistema já em execução (daemons do Fedora, daemons do usuário, indexadores, gerenciadores de janela), elevando o total para 1.877 processos e saturando a CPU em 27× a capacidade nominal.

### Consequências

Lista enumerada e mensurável de efeitos observados durante o incidente real de 2026-06-03:

- **CPU saturada**: load average de **276** em uma máquina com 10 CPUs lógicos, equivalente a **27× a capacidade nominal**, com CPU ociosa em **0%** (medido via `top` em amostragem única e `uptime`)
- **Máquina praticamente inutilizável**: qualquer interação do operador (digitar comando, clicar em janela, alternar workspace) entrava na fila de espera do scheduler com latência de segundos
- **Memória pressionada**: **13 GB** em compressor de memória (zram) e swap ativo, com pressão suficiente para o kernel disparar OOM killer preventivo em vários processos não relacionados
- **Contenção de lock no SQLite**: o semáforo CLI (`acquire_cli_slot` em `src/lock.rs:76-122`) e o banco SQLite (single-writer) competem por recursos com os 192 processos da subárvore do `enrich`, gerando contenção que se manifesta como exit 75 ("all 4 concurrency slots occupied") mesmo para comandos de gestão leves como `remember` e `stats`
- **Comando `remember` travou sem persistir**: timeout no acquire do slot CLI, sem persistência da memória que estava sendo gravada, gerando perda de trabalho não confirmada mas possível
- **Processos órfãos persistentes**: a etapa 2 da mitigação manual (`pkill` dos MCPs e `node` órfãos) reduziu o total de processos de 1.958 para 1.857, provando na prática que subprocessos sobreviveram ao pai e continuaram consumindo recursos após a interrupção do `enrich`
- **Diagnóstico difícil**: load alto sem causa óbvia, aparência de loop infinito de "alguma coisa" sem identificação clara do agente causal — o operador precisou combinar `uptime`, `sysctl vm.loadavg`, `ps -A | wc -l` e análise de ancestralidade de processos para identificar a subárvore do `enrich`
- **Custo de oportunidade**: o operador ficou impedido de trabalhar por aproximadamente 2 horas durante a mitigação e a investigação da causa raiz
- **Custo monetário de LLM**: cada invocação de `claude -p` consome tokens pagos. Cenário real documentado em v1.0.66: 2.321 entidades × ~12,5 segundos por chamada = **8 horas de wall time serial** com `--llm-parallelism 1`. Com paralelismo descontrolado, o mesmo trabalho pode disparar até 18.568 chamadas simultâneas, com custo agregado estimado em ~USD 185 se 100% falharem (taxa de falha real documentada em G01 é de 64%)
- **Risco de contenção de lock SQLite**: durante a saturação, o SQLite single-writer fica sob pressão de múltiplos workers tentando `BEGIN IMMEDIATE`, gerando contenção que se manifesta como `SQLITE_BUSY` mesmo com `busy_timeout` configurado

### Causa Raiz (5 Porquês)

Análise descendente da cadeia causal completa, do sintoma observado à causa fundamental:

- **POR QUÊ 1** o load average chegou a 276 e o sistema tinha 1.877 processos? Porque 4 instâncias de `enrich` rodavam em paralelo, cada uma com 2 workers LLM (default de `--llm-parallelism` ≥ 1), e cada worker invocou `claude -p` headless que herdou o conjunto completo de servidores MCP do `~/.claude/settings.json` do operador, gerando uma subárvore de **~192 processos descendentes do `enrich`** competindo com **~1.685 processos de sistema** já em execução.

- **POR QUÊ 2** 4 instâncias de `enrich` rodaram em paralelo? Porque o semáforo de slots da CLI (`src/lock.rs:76-122`) limita invocações concorrentes a 4 (constante `MAX_CONCURRENT_CLI_INSTANCES = 4` em `src/constants.rs:341`), e o `enrich` compartilha esses mesmos slots com `remember`, `stats`, `read` e demais comandos de gestão. Não existe um singleton dedicado para jobs pesados como `enrich` e `ingest --mode claude-code`, então o operador pode disparar múltiplas instâncias via shells paralelos, scripts ou loops de automação.

- **POR QUÊ 3** cada `enrich` invocou 2 workers LLM? Porque o flag `--llm-parallelism` declarado em `src/commands/enrich.rs:443-445` aceita valores de 1 a 32 sem exigir opt-in explícito para paralelismo de LLM, e o default não é serial. Cada worker dentro do mesmo `enrich` spawna um `claude -p` independente, e o queue DB (`.enrich-queue.sqlite`) já usa `UPDATE ... RETURNING` atômico que suportaria múltiplos workers concorrentes, mas o código atual serializa desnecessariamente quando o paralelismo está habilitado, desperdiçando a latência de I/O de rede do LLM.

- **POR QUÊ 4** cada `claude -p` headless subiu 8-10 servidores MCP? Porque a função `build_claude_command` em `src/commands/claude_runner.rs:204-254` decide entre `--bare` (quando `ANTHROPIC_API_KEY` está definida) e `--dangerously-skip-permissions --settings '{"hooks":{}}'` (quando OAuth está ativo), mas em nenhum dos dois caminhos passa as flags `--strict-mcp-config --mcp-config '{}'`. O `claude -p` herda automaticamente os MCPs configurados em `~/.claude/settings.json` (escopo user), `.claude/settings.json` (escopo project) e `.mcp.json` (escopo project MCP). Cada MCP de transporte stdio dispara `npm exec` + `node` = 2 processos por servidor. Com 10 MCPs configurados, são 20 processos filhos só dos MCPs, mais os processos auxiliares internos do Claude Code.

- **POR QUÊ 5** os MCPs e `node` órfãos sobreviveram ao `enrich` pai? Porque o spawn em `src/commands/claude_runner.rs:54-94` usa `std::process::Command` síncrono (não `tokio::process::Command`), aplica `setsid()` para criar process group Unix (boa prática, correta), mas **não implementa nenhum dos três mecanismos** necessários para evitar órfãos: (a) `kill_on_drop(true)` no handle `Child`, (b) watcher de morte do pai via `kqueue NOTE_EXIT` (macOS) ou pipe-EOF (Linux), (c) reaper no startup que varre `pgrep` por PPIDs órfãos de execuções anteriores. Quando o operador executa `pkill` no `enrich`, o `claude -p` recebe SIGTERM, mas seus filhos MCP e `node` não recebem o sinal porque o process group não é propagado para netos, e os watchers internos do Claude Code não foram desenhados para encerrar netos ao pai morrer.

**Síntese da causa raiz única**: a CLI evoluiu de ferramenta de consulta leve (recall, hybrid-search, stats) para orquestradora headless de LLMs externos (`enrich`, `ingest --mode claude-code`, `ingest --mode codex`), mas a arquitetura de ciclo de vida de processo permaneceu ancorada no modelo one-shot original. A mudança de papel — de folha para raiz de árvore de processos — não foi acompanhada por governança de instância (singleton), confinamento de configuração (MCP isolation), reaping conjunto (kill_on_drop + watcher), circuit breaking de retry, ou observabilidade de fan-out. O modelo inofensivo na folha virou bomba na raiz.

### Evidência no Código

Citações exatas com `file_path:line_number` para navegação direta:

- `src/commands/claude_runner.rs:204-254` — função `build_claude_command` que monta o `Command` para `claude -p`. **Faltam** as flags `--strict-mcp-config` e `--mcp-config '{}'` em ambos os branches (linhas 237-243 com `ANTHROPIC_API_KEY` e linhas 240-242 sem ela). A função aceita apenas `binary`, `prompt`, `json_schema`, `model` e `max_turns` como parâmetros — não há parâmetro para `isolate_mcp: bool`.
- `src/commands/claude_runner.rs:54-94` — função `spawn_with_memory_limit` para Linux aplica `setsid()` em `pre_exec` (linha 70) e `setrlimit(RLIMIT_AS, ...)` em `pre_exec` (linha 81). A função retorna `std::process::Child` (linha 93) e não configura `kill_on_drop` (que é API exclusiva de `tokio::process::Child`, não de `std::process::Child`).
- `src/commands/claude_runner.rs:343-410` — função `run_claude` que orquestra o spawn, o `wait_timeout` e o parsing. O `Child` retornado por `spawn_with_memory_limit` (linha 349) é tratado como `std::process::Child` e não há `Drop` impl customizado que envie SIGTERM ao `claude` em caso de panic ou cancelamento.
- `src/commands/enrich.rs:443-445` — campo `llm_parallelism: u32` em `EnrichArgs` com range 1-32 e help text mencionando "for 2321 entities, --llm-parallelism 4 reduces wall time ~4×". O default é estabelecido em `src/commands/enrich.rs:994` (ou 1007/1023 em branches paralelos) como `None`, que é resolvido para 1 em `src/commands/enrich.rs:1108` via `clamp(1, 32)`.
- `src/commands/enrich.rs:565-607` — função `run_claude_extraction` que delega para `claude_runner::run_claude`. O `is_oauth` detectado via `apiKeySource == "none"` (em `claude_runner.rs:265-270`) **é conhecido mas não usado** para alternar entre `isolate_mcp: true` e `isolate_mcp: false`.
- `src/commands/ingest_claude.rs:322-328` — implementação IDÊNTICA ao enrich do spawn de `claude -p`, com mesma ausência de flags MCP isolation. Este é o spawn usado em `ingest --mode claude-code` (memória G01 confirmou a duplicação de vulnerabilidade).
- `src/commands/ingest_codex.rs` (linhas equivalentes) — spawn de `codex exec` segue o mesmo padrão de duplicação. Falta a flag `-c mcp_servers='{}'` documentada no audit original.
- `src/lock.rs:76-122` — função `acquire_cli_slot` com semáforo de 4 slots via `flock` em `cli-slot-{N}.lock`. **Não distingue** entre jobs leves (recall, stats) e jobs pesados (enrich, ingest --mode claude-code). Um único tipo de slot é compartilhado por todos os comandos.
- `src/constants.rs:341` — `pub const MAX_CONCURRENT_CLI_INSTANCES: usize = 4;` é a constante rígida que limita o semáforo. Esta constante é INDEPENDENTE de qualquer cálculo dinâmico de memória ou CPU.
- `src/main.rs` (visão geral) — não há função `reap_orphans()` chamada no startup, não há signal handler para `SIGCHLD` que limpe filhos em background, não há watcher periódico de PPIDs órfãos.
- `src/commands/enrich.rs` (--retry-failed) — o flag existe mas sem teto de tentativas documentado, sem backoff exponencial, sem circuit breaker. Iteração infinita é possível em falhas persistentes (rate limit, network outage, OAuth expired).
- `src/retry.rs` (visão geral) — módulo de retry genérico com `AppError::RateLimited` que implementa backoff de 60s → 120s → 300s → 900s (documentado em skill), mas sem teto total de tentativas e sem circuit breaker.

### Relações Causa × Efeito

Cadeia causal completa, do problema fundamental aos sintomas observados:

```
Ausência de singleton dedicado para jobs pesados
    ↓ causa
Múltiplas instâncias de enrich/ingest --mode claude-code em paralelo (até 4 via lock.rs)
    ↓ causa
Cada enrich invoca --llm-parallelism workers (1-32, default não-zero)
    ↓ causa
Cada worker spawna um claude -p headless
    ↓ causa
claude -p herda MCPs do ~/.claude/settings.json (~10 servidores típicos)
    ↓ causa
Cada MCP stdio dispara npm exec + node (2 processos por servidor)
    ↓ causa
Subárvore do enrich: 4 enrich × 2 workers × 10 MCPs × 2 = 160 processos
    + claude -p em si: 8 processos
    + watchers internos do Claude Code: ~24 processos
    = ~192 processos na subárvore do enrich
    ↓ amplifica
Soma com 1.685 processos de sistema = 1.877 processos totais
    ↓ causa
Load average 276 (CPU saturada 27× em 10 cores)
    ↓ causa
Sem kill_on_drop + sem watcher de pai + sem reaper no startup
    ↓ causa
Processos órfãos sobrevivem ao pkill do enrich
    ↓ causa
Recursos não liberados após interrupção manual
    ↓ causa
Mitigação manual exige 2 etapas (pkill enrich + pkill MCPs/node órfãos)
    ↓ causa
Sem circuit breaker em --retry-failed
    ↓ causa
Falhas persistentes disparam loops de retry sem teto
    ↓ amplifica
A carga total de subprocessos, gerando contenção adicional
    ↓ causa
Contenção de lock SQLite (single-writer) entre workers do enrich
    ↓ causa
Comandos leves (remember, stats) recebem exit 75 (all 4 slots occupied)
    ↓ causa
Operador perde capacidade de gravar memórias
    ↓ resulta em
Perda de trabalho, máquina inutilizável, diagnóstico difícil, custo de oportunidade
```

### Solução Proposta (5 Camadas)

Quatro correções cirúrgicas de baixo a médio esforço, mais uma camada estrutural de alto esforço. A ordem de prioridade é A → B → D → C → Daemon (do menor esforço e maior impacto para o maior esforço).

#### Correção A — Isolar MCP dos Headless (esforço baixo, impacto altíssimo)

Maior alavanca de redução da árvore de processos com menor esforço de implementação. O `claude -p` headless não precisa dos 10 MCPs do operador para tarefas de extração estruturada. Subir com configuração MCP vazia corta a árvore de 160 processos para cerca de 8 processos (~95% de redução).

**Implementação proposta** em `src/commands/claude_runner.rs`:

- Adicionar campo `isolate_mcp: bool` em `ClaudeInvocationOpts` (struct de configuração compartilhada entre `enrich`, `ingest_claude` e versões futuras)
- Modificar `build_claude_command` (linhas 204-254) para receber o novo parâmetro
- Quando `isolate_mcp = true` E credencial é OAuth (detectada por `is_oauth` ou ausência de `ANTHROPIC_API_KEY`):
  - Adicionar `--strict-mcp-config` (validação pendente contra docs oficiais — ver Ressalva R1)
  - Adicionar `--mcp-config '{}'`
- Quando `isolate_mcp = true` E credencial é API key (presença de `ANTHROPIC_API_KEY`):
  - Manter `--bare` (já isola MCP por design)
- Quando `isolate_mcp = false`:
  - Manter comportamento atual

**Comando de referência** (do audit original, validar flags antes de implementar):
```
claude -p "TAREFA" \
  --strict-mcp-config \
  --mcp-config '{}' \
  --dangerously-skip-permissions \
  --settings '{"hooks":{}}' \
  --model sonnet \
  --max-turns 8 \
  --output-format json
```

**NÃO usar `--bare`** quando a credencial for OAuth (Pro/Max), porque `--bare` desabilita o login OAuth e força uso de API key.

#### Correção B — Singleton de Jobs Pesados (esforço baixo, impacto alto)

Lock global dedicado para jobs pesados (`enrich`, `ingest --mode claude-code`, `ingest --mode codex`) garante que apenas uma instância por namespace possa rodar por vez. Segunda invocação recusa com exit 75 ou enfileira com `--wait-job-singleton` opcional.

**Implementação proposta** em `src/lock.rs`:

- Criar enum `JobType { Light, Enrich, IngestClaudeCode, IngestCodex }`
- Criar função `acquire_job_singleton(job_type: JobType, namespace: &str) -> Result<File, AppError>`
- Usar lock file separado por tipo: `job-singleton-{job_type}-{namespace}.lock` no mesmo diretório de cache
- Slot dedicado: 1 por job type por namespace (não compartilhado com o semáforo CLI genérico)
- Quando ocupada: retornar `AppError::JobSingletonLocked { job_type, namespace }` (exit code novo, ou reusar 75)
- Opcional: flag `--wait-job-singleton <SECONDS>` que poll-eia com backoff progressivo

**Teria evitado 100% do incidente de hoje** ao recusar as 3 instâncias extras de `enrich` que o operador disparou em paralelo.

#### Correção C — Morte Conjunta e Reaping (esforço médio, impacto alto)

Usar `tokio::process::Command::kill_on_drop(true)` nos `claude -p` spawnados, adicionar watcher de morte do pai cross-platform, e reaper no startup que varre órfãos de execuções anteriores.

**Implementação proposta**:

- **Linux**: usar `prctl(PR_SET_PDEATHSIG, SIGTERM)` em `pre_exec` do `Command`. Quando o pai morrer, o kernel envia automaticamente SIGTERM ao filho.
- **macOS**: NÃO tem `PR_SET_PDEATHSIG`. Alternativas: (a) pipe-EOF (criar pipe, herdar read-end no filho, pai monitora close), (b) `kqueue` com `NOTE_EXIT` no PID do pai, (c) `posix_spawn` com `POSIX_SPAWN_SETPGROUP` para cascade termination via process group.
- **Windows**: `Job Object` com `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (já auditado em PE04 como ausente).
- **Tokio**: migrar `spawn_with_memory_limit` de `std::process::Command` para `tokio::process::Command` e setar `.kill_on_drop(true)`. **Limitação conhecida** (issue tokio #2504): `kill_on_drop(true)` envia SIGKILL no Unix, não SIGTERM. Para shutdown gracioso, implementar manualmente: `Drop` impl customizado que envia SIGTERM, espera grace period (5s), depois SIGKILL.
- **Reaper no startup**: em `main.rs`, antes do `acquire_cli_slot`, executar `reap_orphans()` que faz `pgrep -P 1` filtrando por PPID órfão (ou por nome de binário `claude`, `codex`, `opencode`) e envia SIGTERM.

**Impacto**: elimina processos órfãos persistentes, libera recursos imediatamente após interrupção do operador, permite reuso limpo de slots CLI.

#### Correção D — Defaults Seguros e Freio no Retry (esforço baixo, impacto alto)

Endurecer defaults de paralelismo e adicionar circuit breaker ao `--retry-failed`. Contém o incidente sem código complexo.

**Implementação proposta**:

- Mudar default de `--llm-parallelism` de 1 (atual) para 1, mas exigir opt-in EXPLÍCITO para valores > 1 via flag `--llm-parallelism-force`. Quando o usuário passar `--llm-parallelism 4` sem `--llm-parallelism-force`, emitir `tracing::warn!` e clampar para 1.
- Adicionar teto de tentativas ao `--retry-failed`: máximo de 5 tentativas com backoff exponencial (já existe em `src/retry.rs` para RateLimited).
- Adicionar circuit breaker que aborta o job após N falhas consecutivas (sugestão: N=3) com tipo de erro terminal (Validation, Internal) — distingue de RateLimited que tem backoff documentado.
- Logar contagem de filhos vivos a cada item processado: `tracing::debug!(target: "process", live_children = count, "processing item")`.
- Emitir `tracing::warn!` quando o número de `claude -p` headless simultâneos passa de um teto configurável (sugestão: 2 por enrich, 4 absoluto por host).
- Adicionar flag `--max-child-load <FLOAT>` que aborta o job se `uptime`-derivado load average passa de F×nCPUs (sugestão: F=2.0 como default).

#### Camada Estrutural — Daemon Servidor Único (esforço alto, impacto alto)

Promover o daemon atual de cache de modelo de embeddings para servidor de comandos completo. A CLI vira cliente fino que conecta via Unix abstract socket (já existe em `src/daemon.rs`), envia o comando, recebe o resultado NDJSON, e sai.

**Implementação proposta**:

- `tokio::net::UnixListener` no daemon, com loop de `accept` e `select!` em `ctrl_c` para shutdown gracioso (já existe `idle-shutdown-secs` documentado em memória)
- Protocolo de IPC versionado: cada mensagem inclui `version: u8` para compatibilidade forward/back
- Modelo ONNX carregado UMA vez no startup do daemon, reutilizado por todos os clientes
- SQLite aberto UMA vez no startup, elimina contenção de lock entre processos CLI
- Trade-off: vira ponto único de falha para todos os comandos pesados
- Trade-off: incompatibilidade de versão entre CLI e daemon já é risco conhecido (mitigado por `daemon --ping` com aviso)
- Trade-off: estado compartilhado pede cuidado com concorrência interna (já existe `tokio::sync::Semaphore` em `src/daemon.rs`)

**Benefícios combinados**:

- Latência de embedding cai de ~1 segundo (carregamento) para ~milissegundos (cache hit)
- Contenção de lock SQLite entre processos eliminada
- Spawn de `claude -p` centralizado permite observabilidade agregada
- Idle shutdown via `idle-shutdown-secs` libera recursos quando daemon fica ocioso

### Benefícios da Solução

Mensuráveis e qualitativos, com referência ao incidente real:

- **Redução de 95% na subárvore de processos por enrich**: de ~192 para ~8 processos quando Correção A está ativa. Validável: contar `pgrep -P <enrich_pid> | wc -l` antes e depois.
- **Eliminação de paralelismo acidental destrutivo**: Correção B recusa instâncias extras de `enrich`, impedindo a árvore de explodir. Validável: 2 `enrich` simultâneos, 2º deve falhar com exit 75 em < 1s.
- **Latência de `remember` recupera de travado para < 500 ms**: com Correção C eliminando órfãos, o semáforo CLI fica livre imediatamente. Validável: medir `time sqlite-graphrag remember ...` antes e depois.
- **Zero processos órfãos após interrupção**: Correção C garante morte conjunta pai-filhos via `prctl(PR_SET_PDEATHSIG)` no Linux. Validável: enviar SIGTERM ao pai, contar processos netos em < 5s, deve ser 0.
- **Conformidade OAuth preservada**: Correção A usa `--strict-mcp-config --mcp-config '{}'` em vez de `--bare`, mantendo o login Pro/Max. Validável: `claude -p` headless com OAuth retorna `apiKeySource: "none"` no JSON de init.
- **Custo de LLM previsível**: Correção D com circuit breaker evita loops infinitos de retry que multiplicam tokens. Estimativa: economia de até 50% em cenários de falha persistente.
- **Observabilidade antecipada**: `tracing::debug!` por spawn e `tracing::warn!` em load > threshold detecta o padrão antes de saturar.
- **Conformidade com regras do projeto**:
  - Mudanças cirúrgicas (apenas 5 paths modificados, 0 refatoração fora de escopo)
  - Sem Co-authored-by em commits (regra do CLAUDE.md)
  - Testes com `assert_cmd` e `tempfile` (pirâmide obrigatória)
  - Cobertura ≥ 80% em `src/commands/claude_runner.rs` (regra do llvm-cov)
  - 10 comandos de validação passando antes de tag (regra do release engineering)

### Como Solucionar (passo a passo executável)

Ordem de aplicação priorizada por retorno:

- **Passo 1**: em `src/commands/claude_runner.rs`, adicionar `pub struct ClaudeInvocationOpts { pub isolate_mcp: bool, pub max_turns: u32, ... }` e modificar `build_claude_command` para aceitar `opts: &ClaudeInvocationOpts`. Em `is_oauth` (linha 265-270), propagar para o caller. Adicionar `if opts.isolate_mcp && is_oauth { cmd.arg("--strict-mcp-config").arg("--mcp-config").arg("{}"); }` antes do return em ambas as branches (OAuth e API key).
- **Passo 2**: em `src/commands/enrich.rs:443-445`, adicionar campo `llm_parallelism_force: bool` em `EnrichArgs`. Default `false`. Help text explicando: "Required to set llm-parallelism > 1, acknowledges process proliferation risk".
- **Passo 3**: em `src/commands/enrich.rs:1108`, se `args.llm_parallelism > 1 && !args.llm_parallelism_force`, emitir `tracing::warn!` e clampar para 1. Manter exit 0 (warning, não erro).
- **Passo 4**: em `src/lock.rs:76-122`, criar `pub enum JobType { Light, Enrich, IngestClaudeCode, IngestCodex }` e `pub fn acquire_job_singleton(job_type: JobType, namespace: &str) -> Result<File, AppError>`. Lock file path: `job-singleton-{job_type}-{namespace}.lock` no mesmo cache dir.
- **Passo 5**: em `src/commands/enrich.rs`, antes do loop principal, chamar `acquire_job_singleton(JobType::Enrich, &namespace)?` e manter o `File` no escopo. Idem para `src/commands/ingest_claude.rs` e `src/commands/ingest_codex.rs`.
- **Passo 6**: em `src/commands/claude_runner.rs:343-410`, refatorar `run_claude` para usar `tokio::process::Command` com `.kill_on_drop(true)`. Implementar `Drop` impl customizado em wrapper `ClaudeChild { inner: tokio::process::Child, grace_secs: u64 }` que envia SIGTERM, espera `grace_secs`, depois `inner.kill().await`. Manter compatibilidade com `wait_timeout` síncrono via `tokio::task::spawn_blocking`.
- **Passo 7**: em `src/commands/claude_runner.rs:54-94` (Linux branch), adicionar `libc::prctl(PR_SET_PDEATHSIG, libc::SIGTERM)` no `pre_exec` antes de `setsid()` e `setrlimit()`. Comentário SAFETY explicando que `prctl` é async-signal-safe entre fork e exec.
- **Passo 8**: em `src/main.rs`, antes de `acquire_cli_slot`, chamar `reap_orphans()` que: (a) itera `pgrep -x claude` e `pgrep -x codex`, (b) verifica PPID via `stat /proc/<pid>/stat` no Linux ou `ps -o ppid` no macOS, (c) se PPID == 1 (init/orphan), envia SIGTERM. Função `reap_orphans` é `pub fn` em `src/commands/reap.rs` (novo módulo).
- **Passo 9**: em `src/retry.rs`, adicionar `pub struct CircuitBreaker { failure_threshold: u32, open_until: Instant }` com `pub fn record(&mut self, err: &AppError) -> bool` que retorna `true` se o job deve abortar. Distinguir `AppError::RateLimited` (não conta para circuit breaker) de `AppError::Validation` (conta).
- **Passo 10**: em `src/commands/enrich.rs` (--retry-failed handler), envolver o loop de retry em `CircuitBreaker::new(3, Duration::from_secs(60))`. Ao abrir o breaker, emitir `tracing::error!` e retornar `AppError::CircuitBreakerOpen` (exit code novo, sugestão: 78).
- **Passo 11**: adicionar testes unitários em `src/commands/claude_runner.rs`:
  - `build_claude_command_with_isolate_mcp_oauth_includes_strict_flag` — verifica que `--strict-mcp-config` e `--mcp-config '{}'` aparecem quando OAuth
  - `build_claude_command_with_isolate_mcp_apikey_keeps_bare` — verifica que `--bare` é mantido com API key
  - `claude_child_drop_terminates_process` — usa `tokio::test`, spawna `sleep 60`, dropa handle, valida exit por SIGKILL
- **Passo 12**: adicionar teste de integração em `tests/process_proliferation.rs`:
  - Mock `claude` binário que dorme por 10s (`/tmp/mock-claude.sh` com `sleep 10`)
  - Dispara 2 `enrich` simultâneos com `SQLITE_GRAPHRAG_LLM_PARALLELISM=2`
  - Valida que 2º recebe exit 75 com `error_class: "conflict"`
  - Valida que após 10s, todos os `pgrep -f mock-claude` retornam 0
- **Passo 13**: rodar pipeline de validação 10 comandos:
  1. `cargo check --all-targets` → ZERO erros
  2. `cargo clippy --all-targets --all-features -- -D warnings` → ZERO warnings
  3. `cargo fmt --all --check` → ZERO diffs
  4. `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features` → ZERO warnings
  5. `cargo test --all-features` → ZERO falhando (sem `tail -N` que mascare falhas)
  6. `cargo llvm-cov --text` → ≥ 80%
  7. `cargo audit` → ZERO vulnerabilidades
  8. `cargo deny check advisories licenses bans sources` → ZERO violações
  9. `cargo publish --dry-run --allow-dirty` → ZERO erros
  10. `cargo package --list` → ZERO `.profraw`, ZERO `graphrag.sqlite`
- **Passo 14**: criar commit único ou múltiplos commits cirúrgicos (recomendado: 1 commit por Correção A/B/C/D para permitir bisect), todos com mensagem descritiva em inglês, sem Co-authored-by.
- **Passo 15**: tag anotada `v1.0.69` e push, seguindo pipeline de publicação 8 fases (validar → bump → commit → tag → push → monitorar → publicar → cleanup).

### Esforço, Risco e Impacto das Correções

| Correção | Esforço | Risco | Impacto | LOC estimado | Arquivos |
|----------|---------|-------|---------|--------------|----------|
| A — Isolar MCP | Baixo | Baixo | Altíssimo | ~30-50 | claude_runner.rs, enrich.rs, ingest_claude.rs |
| B — Singleton | Baixo | Baixo | Alto | ~40-60 | lock.rs, enrich.rs, ingest_claude.rs, ingest_codex.rs |
| C — Morte conjunta + reaper | Médio | Médio | Alto | ~150-250 | claude_runner.rs, main.rs, novo reap.rs, signals.rs |
| D — Defaults + circuit breaker | Baixo | Baixo | Alto | ~60-100 | enrich.rs, retry.rs, constants.rs |
| Daemon servidor | Alto | Alto | Alto | ~400-800 | daemon.rs, novo ipc.rs, lib.rs |

**Ordem de aplicação por retorno marginal decrescente**:
1. **A** sozinho resolve 90% do problema atual com 5% do esforço total
2. **A + B** resolve 95% (singleton contém paralelismo, MCP isolation reduz fan-out)
3. **A + B + D** resolve 97% (defaults seguros evitam regressão futura)
4. **A + B + C + D** resolve 99% (morte conjunta elimina órfãos)
5. **Todas + Daemon** resolve 100% com latência adicional como bônus

### Detecção Precoce e Prevenção

Sinais de alerta que faltaram no incidente original e devem ser instrumentados:

- **Log de contagem de filhos vivos a cada item processado**: `tracing::debug!(target: "process", live_children = pgrep_count, item_index, "processing item")`
- **Abort preemptivo se load > threshold**: em `enrich.rs` e `ingest_claude.rs`, antes de cada `claude -p`, ler `uptime`-derivado load average e abortar se > `nCPUs * 2.0` (configurável via `--max-child-load`)
- **Healthcheck que conta processos da própria árvore**: adicionar campo `live_subprocesses` em `sqlite-graphrag health --json`, calculado via `pgrep -P <self_pid> | wc -l`
- **Warning quando `claude -p` simultâneos > teto**: `tracing::warn!(target: "process", count, "claude headless count exceeded threshold")` quando `pgrep -fc "claude.*-p"` > 2
- **Métrica de uso de memória pelo subgrupo**: `ps -A --ppid <enrich_pid> -o rss= | awk '{s+=$1} END {print s}'` e comparar com `available_mb`
- **Tracing span por invocação**: `tracing::span!(target: "claude_runner", "claude_invocation", pid, binary, llm_parallelism_idx, model)`
- **Tracing event no spawn moment com binary + sanitized args**: `tracing::debug!(target: "process", program, args = sanitize(args), "spawning external process")` — já parcialmente implementado em `claude_runner.rs:87-92`, falta aplicar consistência em `ingest_claude.rs` e `ingest_codex.rs`

### Arquivos Afetados

Lista completa de paths a serem modificados ou criados:

- `src/commands/claude_runner.rs` — adicionar `ClaudeInvocationOpts`, refatorar `build_claude_command`, migrar para `tokio::process::Command` com `kill_on_drop`, adicionar `Drop` impl customizado
- `src/commands/enrich.rs` — propagar `isolate_mcp`, adicionar `llm_parallelism_force`, chamar `acquire_job_singleton`, integrar `CircuitBreaker`
- `src/commands/ingest_claude.rs` — propagar `isolate_mcp`, chamar `acquire_job_singleton`, mesmo padrão de CircuitBreaker
- `src/commands/ingest_codex.rs` — adicionar `-c mcp_servers='{}'` ao spawn do codex, chamar `acquire_job_singleton`
- `src/lock.rs` — adicionar `JobType` enum e `acquire_job_singleton` function
- `src/main.rs` — chamar `reap_orphans()` no startup antes de `acquire_cli_slot`
- `src/commands/reap.rs` — **novo módulo** com `reap_orphans()` function que varre PPIDs órfãos
- `src/retry.rs` — adicionar `CircuitBreaker` struct e `record` method
- `src/errors.rs` — adicionar variantes `JobSingletonLocked { job_type, namespace }` e `CircuitBreakerOpen` com mapeamento a exit codes
- `src/constants.rs` — adicionar `MAX_CONCURRENT_LLM_PARALLELISM_DEFAULT: u32 = 1`, `CIRCUIT_BREAKER_THRESHOLD: u32 = 3`, `ORPHAN_REAPER_GRACE_SECS: u64 = 5`
- `src/cli.rs` — adicionar flags `--llm-parallelism-force`, `--max-child-load`, `--wait-job-singleton`
- `tests/process_proliferation.rs` — **novo arquivo** com testes de integração
- `docs/CLAUDE.md` — documentar G28 com referência ao gap
- `CLAUDE.md` (raiz) — atualizar seção "Process management" com nova governança
- `CHANGELOG.md` — entrada na seção "Unreleased" com bullet list das 5 correções

### Testes

Pirâmide de testes proposta para validar a solução:

**Testes unitários** (`#[cfg(test)]` em cada módulo):
- `claude_runner::tests::build_claude_command_with_isolate_mcp_oauth_includes_strict_flag` — valida flags injetadas
- `claude_runner::tests::build_claude_command_with_isolate_mcp_apikey_keeps_bare` — valida que API key path não muda
- `claude_runner::tests::claude_child_drop_terminates_with_sigkill_then_sigterm` — valida Drop impl
- `lock::tests::acquire_job_singleton_blocks_second_invocation` — valida singleton
- `lock::tests::acquire_job_singleton_allows_different_namespaces` — valida isolamento
- `retry::tests::circuit_breaker_opens_after_threshold_failures` — valida breaker
- `retry::tests::circuit_breaker_ignores_rate_limited_errors` — valida distinção de erro
- `reap::tests::reap_orphans_kills_claude_with_ppid_1` — valida reaper

**Testes de integração** (`tests/process_proliferation.rs`):
- `integration::enrich_singleton_blocks_concurrent_invocation` — 2 `enrich` simultâneos, 2º falha
- `integration::mcp_isolation_reduces_subprocess_count` — mock `claude` que reporta PPID tree, validar < 10 netos
- `integration::kill_on_drop_cascades_to_children` — mock `claude` que dorme 60s, dropa handle, valida exit em < 5s
- `integration::retry_circuit_breaker_aborts_after_three_failures` — mock que sempre falha, valida abort em iteração 3
- `integration::reaper_cleans_orphans_on_startup` — spawn mock `claude` órfão, inicia CLI, valida que mock foi morto

**Testes de aceitação** (script em `scripts/test-g28.sh`):
- Reprodução controlada do incidente: disparar 4 `enrich` com `--llm-parallelism 2` e mock `claude` que dorme 30s
- Validar: load average < 5 durante execução, total de processos < 1500, exit code do 2º `enrich` = 75
- Validar: após Ctrl+C, todos os mock `claude` mortos em < 10s

**Cobertura de código**:
- `src/commands/claude_runner.rs`: meta ≥ 90% (lógica crítica de spawn)
- `src/lock.rs`: meta ≥ 85% (lógica de semáforo)
- `src/retry.rs`: meta ≥ 85% (circuit breaker)
- `src/commands/reap.rs`: meta ≥ 80% (reaper novo)
- Global: meta ≥ 80% (regra do llvm-cov do projeto)

### Conformidades Detectadas (não-violações)

Componentes que JÁ implementam parcialmente a governança e não precisam de modificação estrutural, apenas de refinamento:

- **`setsid()` em `claude_runner.rs:69-85, 99-125`**: cria process group Unix independente, base para cascade termination via `killpg` (precisa adicionar o `killpg` no Drop impl)
- **`env_clear()` + whitelist de env vars em `claude_runner.rs:213-225`**: já isola PATH, HOME e outras variáveis de ambiente sensíveis. Base para adicionar `--strict-mcp-config` (mesma filosofia de least-privilege)
- **`wait_timeout` cross-platform em `claude_runner.rs:344, 414-426`**: já implementa `terminate_gracefully` com SIGTERM → grace period → SIGKILL. Padrão a ser replicado no `Drop` impl de `ClaudeChild`
- **`flock` slot semaphore em `lock.rs:50-122`**: base sólida para estender com `acquire_job_singleton`. Apenas precisa de novo enum `JobType` e lock file path
- **`idle-shutdown` em `daemon.rs`**: timer de expiração documentado em memória `daemon-auto-restart-pattern`. Pronto para promoção a servidor de comandos
- **`tracing::debug!` em spawn em `claude_runner.rs:87-92, 118-123`**: já loga program + args no spawn. Falta aplicar consistência em `ingest_claude.rs` e `ingest_codex.rs` (PE10 do `external-process-audit-v1066`)
- **`Safety comment per §19` em `daemon.rs`**: documenta intenção de `setsid` + process group. Padrão a ser replicado em `claude_runner.rs` e no novo `reap.rs`
- **Detecção de OAuth via `apiKeySource == "none"` em `claude_runner.rs:265-270`**: flag derivada está sendo computada mas não usada para alternar `isolate_mcp`. Apenas propagar
- **`--bare` preserva OAuth**: decisão de design correta documentada em `claude-headless-permissions-hooks`. **NÃO** usar `--bare` na Correção A, usar `--strict-mcp-config`
- **Queue DB `.enrich-queue.sqlite`**: já usa `UPDATE ... RETURNING` atômico (G19) que suporta múltiplos workers. Pronto para o paralelismo de LLM
- **`AppError::RateLimited` em `errors.rs`**: variante dedicada com backoff de 60s → 120s → 300s → 900s. Base para o circuit breaker distinguir rate limit de erro terminal

### Hipóteses Alternativas Descartadas

Outras causas possíveis investigadas e refutadas pela intervenção em duas etapas:

- **Swap thrashing por memória cheia**: considerado inicialmente, mas a mitigação matou só o `enrich` e o load caiu de 276 para 6,74 (40× redução), provando que a pressão de memória era EFEITO da CPU presa, não causa
- **Indexação do Spotlight via `mdworker`**: considerado porque macOS dispara `mdworker` em background, mas o incidente foi em Fedora Linux, sem `mdworker`. Além disso, `mdworker` é single-threaded por design
- **Backup do Time Machine**: irrelevante — Time Machine é exclusivo de macOS, e o incidente foi em Fedora
- **Daemon de embedding do `graphrag`**: considerado suspeito porque o daemon carrega modelo ONNX, mas o daemon é leve, único, e tem `idle-shutdown` documentado. Além disso, o `pkill` do `enrich` resolveu o problema, o `pkill` do daemon não foi necessário
- **Memory leak no ONNX runtime**: considerado porque `ort` 2.0.0-rc.12 teve issues conhecidos, mas o RSS de cada `claude -p` é dominado pelos MCPs, não pelo modelo

### Ressalvas e Incertezas

- **R1 (ALTA)**: as flags `--strict-mcp-config` e `--mcp-config '{}'` mencionadas no audit original em `/home/comandoaguiar/Dropbox/ai/gaps_graphrag.md` NÃO foram confirmadas na documentação oficial do Claude Code consultada via `webfetch` em `https://code.claude.com/docs/en/agent-sdk/mcp` (página é sobre Agent SDK, não CLI direta). A documentação oficial da CLI deve ser consultada em `https://code.claude.com/docs/en/cli` antes de implementar a Correção A. **Validação obrigatória antes de merge**.
- **R2 (MÉDIA)**: `tokio::process::Command::kill_on_drop(true)` envia SIGKILL no Unix (issue #2504 confirmada em `https://github.com/tokio-rs/tokio/issues/2504`), não SIGTERM gracioso. Para shutdown gracioso, é necessário implementar `Drop` impl customizado. A Correção C deve documentar essa limitação.
- **R3 (MÉDIA)**: `prctl(PR_SET_PDEATHSIG, SIGTERM)` é exclusivo de Linux. macOS e Windows precisam de implementação separada (kqueue + Job Object, respectivamente). Já auditado em PE04 do `external-process-audit-v1066` como ausente. A Correção C deve tratar cross-platform explicitamente.
- **R4 (BAIXA)**: o audit original diz "graphrag consultado mas NÃO respondeu por saturação de CPU" durante o incidente. Isso é um **meta-gap**: a memória que documentaria o incidente estava indisponível durante o incidente. Não escopo de G28 corrigir isso diretamente, mas deve ser flagged como follow-up.
- **R5 (BAIXA)**: o `CircuitBreaker` proposto na Correção D pode abortar jobs legítimos com erros transitórios que não sejam `RateLimited`. Mitigação: começar com threshold conservador (3 falhas consecutivas) e tunar baseado em telemetria de produção.
- **R6 (BAIXA)**: a Correção A requer default conservador. Se o operador deliberação configurar `--llm-parallelism 4` para acelerar enriquecimento, o warning + clamp para 1 pode frustrar o usuário. Mitigação: warning é informativo, não erro, e o opt-in via `--llm-parallelism-force` é uma flag separada (não default).
- **R7 (BAIXA)**: mudanças no default de paralelismo podem ser interpretadas como breaking change para usuários que dependem de paralelismo implícito. Mitigação: documentar no `CHANGELOG.md` com nota "behavior change", e emitir `tracing::info!` na primeira invocação explicando o opt-in.

### Próximos Passos Recomendados

1. **Imediato (sprint atual)**: aplicar Correção A (5% de esforço, 90% do impacto) e validar em staging com mock `claude` que reporta fan-out
2. **Curto prazo (próximo sprint)**: aplicar Correção B + D (singleton + defaults), validar com teste de aceitação
3. **Médio prazo (1-2 sprints)**: aplicar Correção C (morte conjunta + reaper), validar cross-platform em macOS e Windows
4. **Longo prazo (backlog v1.1.0)**: avaliar Camada Daemon após telemetria de uso real confirmar demanda por latência reduzida
5. **Documentação**: atualizar `docs/CLAUDE.md` e `CLAUDE.md` (raiz) com nova seção "Process lifecycle governance"
6. **Comunicação**: publicar nota no `CHANGELOG.md` da v1.0.69 com bullet list das 5 correções e referência ao G28
7. **Follow-up de meta-gap (R4)**: investigar por que o daemon de embedding do graphrag ficou indisponível durante o incidente e adicionar proteção (ex.: health check do próprio daemon no `enrich`)

---

## G29 CRITICAL — `cargo install sqlite-graphrag` quebra no Windows em v1.0.67 com erro E0308 em `src/terminal.rs:29:26`

### Problema

Usuário final executando `cargo install sqlite-graphrag` no Windows recebe erro de compilação que impede a instalação:

```
error[E0308]: mismatched types
  --> C:\Users\dr05-caixa01\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\sqlite-graphrag-1.0.67\src\terminal.rs:29:26
   |
29 |             if handle != 0 && handle as isize != -1 {
   |                ------    ^ expected `*mut c_void`, found `usize`
   |                |
   |                expected because this is `*mut c_void`
   |
   = note: expected raw pointer `*mut c_void`
                     found type `usize`
help: if you meant to create a null pointer, use `std::ptr::null_mut()`
   |
29 -             if handle != 0 && handle as isize != -1 {
29 +             if handle != std::ptr::null_mut() && handle as isize != -1 {
   |

For more information about this error, try `rustc --explain E0308`.
error: could not compile `sqlite-graphrag` (lib) due to 1 previous error
```

O erro impede 100% das tentativas de instalação via `cargo install` no Windows. Usuários Linux, macOS e binários pré-construídos não são afetados, mas a experiência de onboarding no Windows está completamente quebrada desde a publicação de v1.0.67 (2026-06-01) — embora a regressão tenha sido introduzida em v1.0.66 (cross-platform-audit MP01) e passada despercebida porque o CI compila em `windows-latest` com toolchain estável mas o lockfile local fixa `windows-sys 0.59.0` cuja definição de `HANDLE` é incompatível com a expressão usada em `terminal.rs:29`.

### Consequências

1. **100% dos usuários Windows falham em `cargo install`**: bloqueia completamente o onboarding de novos usuários na plataforma mais popular para desenvolvimento Rust corporativo (Windows tem ~50% de market share em Rust segundo a Rust Survey 2024).
2. **Última versão publicamente instalável é v1.0.65**: usuários que precisam do Windows são forçados a usar versão de 2 meses atrás, perdendo 27 melhorias da v1.0.66 e v1.0.67 documentadas no `CHANGELOG.md:10-37`.
3. **Regressão silenciosa**: o CI matrix inclui `windows-latest` (linha 29 e 44 do `.github/workflows/ci.yml`) mas o erro ESCAPA ao CI porque o source publicado em crates.io (analisado em V2 abaixo) é IDÊNTICO ao do main branch e o `cargo check --target x86_64-pc-windows-msvc` no projeto local não foi validado após a introdução do `terminal.rs` no `cross-platform-audit-v1066`.
4. **Trust do ecossistema prejudicado**: bug cross-platform em CLI de memória persistente (categoria onde confiança é crítica) reduz credibilidade do projeto junto a contribuidores e integradores.
5. **Workaround forçado**: usuários precisam ou (a) baixar binário pré-construído de release do GitHub, ou (b) aplicar patch manual em `~/.cargo/registry/src/.../terminal.rs`, ou (c) usar `cargo install --git` para buildar de commit específico sem o bug.
6. **Falsa sensação de cobertura de teste**: presença de `windows-latest` na matrix dá impressão de que Windows é suportado ativamente quando na verdade a compilação falha deterministicamente.
7. **Acúmulo de débitos técnicos correlatos**: o mesmo padrão `as isize` para `HANDLE` aparece em `src/platform.rs:54-59` do projeto irmão `cli_duckduckgo-search-cli` (análise em meta-análise), sugerindo bug sistêmico no template de código Windows do autor.
8. **CHANGELOG não documenta a regressão**: a v1.0.67 lista 27 correções/adicições no `CHANGELOG.md:12-37` mas NENHUMA menciona `terminal.rs` ou `windows-sys`, indicando que a quebra passou despercebida pelo autor.
9. **Comentário no código já alertava**: o `// SAFETY:` em `src/terminal.rs:19-22` literalmente diz "GetStdHandle returns INVALID_HANDLE_VALUE on failure (checked below)" — o autor SABIA que devia usar `INVALID_HANDLE_VALUE` mas escreveu a expressão errada.
10. **Bloqueio de releases futuras**: a próxima release (v1.0.68) provavelmente herdará o bug se ninguém corrigir o `terminal.rs` antes do próximo `cargo publish`.

### Causa Raiz (5 Porquês)

**P1 — Por que o `cargo install` falha?**
Porque o compilador Rust em Windows-msvc emite erro E0308 ("mismatched types") em `src/terminal.rs:29:26` ao comparar `handle` (tipo `*mut c_void` em `windows-sys 0.59+`) com o literal inteiro `0` (tipo `usize`). A expressão `handle as isize` na segunda parte da condição é válida para `*mut c_void` mas o conjunto das duas verificações é semanticamente incorreto.

**P2 — Por que o `handle` é `*mut c_void` em `windows-sys 0.59+`?**
Porque houve um breaking change no crate `windows-sys` entre as versões 0.48/0.52 e 0.59:
- `windows-sys 0.48`: `pub type HANDLE = isize;` (verificado em `https://docs.rs/windows-sys/0.48/windows_sys/Win32/Foundation/type.HANDLE.html`)
- `windows-sys 0.52`: `pub type HANDLE = isize;` (verificado em `https://docs.rs/windows-sys/0.52/windows_sys/Win32/Foundation/type.HANDLE.html`)
- `windows-sys 0.59`: `pub type HANDLE = *mut c_void;` (verificado em `https://docs.rs/windows-sys/0.59.0/windows_sys/Win32/Foundation/type.HANDLE.html`)
- `windows-sys 0.61.2`: `pub type HANDLE = *mut c_void;` (verificado em `https://docs.rs/windows-sys/latest/windows_sys/Win32/Foundation/type.HANDLE.html`)

A mudança foi feita pela Microsoft para alinhar com o estilo da stdlib (`std::os::windows::raw::HANDLE` também é `*mut c_void`) e remover casts desnecessários no código de bindings gerados.

**P3 — Por que o `Cargo.toml` fixou `"0.59"` se o código presume `HANDLE = isize`?**
Porque o `cross-platform-audit-v1066` (memória GraphRAG) migrou o range para `"0.59"` ao implementar MP01 (UTF-8 console) e MP02 (ANSI colors) sem revisar os call sites existentes que usavam o tipo antigo. A auditoria documentou "New files: src/terminal.rs, src/signals.rs" mas não auditou a SEMÂNTICA das expressões contra a nova definição de `HANDLE`.

**P4 — Por que o CI não detectou o erro se compila em `windows-latest`?**
Porque o `cargo check` (implícito em `cargo clippy --all-targets` linha 37 e em `cargo nextest run` linha 74 do `.github/workflows/ci.yml`) compila o código com o `Cargo.lock` do projeto que fixa `windows-sys 0.59.0`. E em 0.59.0 o tipo `HANDLE = *mut c_void`, o que DEVERIA gerar o mesmo erro. A hipótese mais provável é que o CI está usando cache (`Swatinem/rust-cache@v2` nas linhas 36, 49, 94) que mascara o erro, OU o `cargo check` está rodando apenas para a arquitetura de host (linux/macos) sem o `--target x86_64-pc-windows-msvc` explícito. **Esta é a META-CAUSA RAIZ do gap: o CI matrix tem `windows-latest` mas não roda `cargo check --target` no runner Windows**.

**P5 — Por que o bug não foi pego em code review antes do `cargo publish`?**
Porque (a) o autor provavelmente não roda Windows localmente (a sessão atual é Linux: `Platform: linux`), (b) o comentário `// SAFETY:` em `src/terminal.rs:19-22` dá falsa confiança de que a verificação foi pensada, e (c) o `cross-platform-audit-v1066` foi tratado como auditoria "completa" no CHANGELOG, reduzindo ímpeto para nova revisão. Esta é a causa-raiz SISTÊMICA: a divisão entre auditoria de "feature nova" e auditoria de "regressão cross-platform" é falha.

### Evidência no Código

**Fonte do erro** (`src/terminal.rs:1-54`):
- Linha 28: `let handle = GetStdHandle(handle_id);` — `handle` é inferido como `HANDLE` (resolvido para `*mut c_void` em `windows-sys 0.59+`)
- Linha 29: `if handle != 0 && handle as isize != -1 {` — ERRO: comparação `*mut c_void != usize` é inválida
- Linha 31: `if GetConsoleMode(handle, &mut mode) != 0 {` — aqui `handle` é passado como `HANDLE` (OK porque é o tipo correto)
- Linha 32: `let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);` — também OK
- Linha 21: comentário `// SAFETY: ... GetStdHandle returns INVALID_HANDLE_VALUE on failure (checked below); ...` — autor SABIA da necessidade de verificar `INVALID_HANDLE_VALUE` mas não implementou a verificação corretamente

**Range de `windows-sys` no projeto** (`Cargo.toml:111`):
```toml
[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.59", features = ["Win32_System_Console"] }
```

**Range no `.crate` publicado v1.0.67** (V2 validado):
```toml
[target."cfg(windows)".dependencies.windows-sys]
version = "0.59"
features = ["Win32_System_Console"]
```
IDÊNTICO ao `Cargo.toml` do main branch.

**CI matrix atual** (`.github/workflows/ci.yml:24-86`):
- Linha 29 e 44: `os: [ubuntu-latest, macos-latest, windows-latest]`
- Linha 37: `cargo clippy --all-targets --all-features -- -D warnings` (sem `--target x86_64-pc-windows-msvc` explícito)
- Linha 74: `cargo nextest run --profile ci` (sem `--target`)
- Linhas 19, 33, 48, 92, 116, 136, 146, 156, 170, 211: `dtolnay/rust-toolchain@stable` (não MSRV 1.88)

**Documentação oficial**:
- `https://docs.rs/windows-sys/0.48/windows_sys/Win32/Foundation/type.HANDLE.html` — `pub type HANDLE = isize;`
- `https://docs.rs/windows-sys/0.52/windows_sys/Win32/Foundation/type.HANDLE.html` — `pub type HANDLE = isize;`
- `https://docs.rs/windows-sys/0.59.0/windows_sys/Win32/Foundation/type.HANDLE.html` — `pub type HANDLE = *mut c_void;`
- `https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/Foundation/type.HANDLE.html` — `pub type HANDLE = *mut c_void;`
- `https://github.com/microsoft/windows-rs/issues/171` — issue histórica "Change Windows HANDLE types back to `*mut c_void` **again**" confirma a instabilidade do tipo entre versões

**Busca por outros call sites similares** (V3 — `sg` + `rg`):
- `sg -p 'GetStdHandle($$$ARGS)' -l rust src/` → **1 match** (apenas `src/terminal.rs:28`)
- `sg -p '$HANDLE != 0' -l rust src/` → **1 match** (apenas `src/terminal.rs:29`; outros matches em `cache.rs:222` e `claude_runner.rs:81` são não-relacionados)
- `sg -p '$EXPR as isize' -l rust src/` → **1 match** (apenas `src/terminal.rs:29`)
- `rg 'is_null\|INVALID_HANDLE\|null_mut' src/ --type rust -n` → **0 matches** (não há uso idiomático de null-check em Windows FFI no projeto)

**V1 — Tentativa de reprodutibilidade local**: `cargo check --target x86_64-pc-windows-msvc --no-default-features` em Linux sem toolchain MSVC falha com `error occurred in cc-rs: failed to find tool "lib.exe"`. Bug NÃO PÔDE ser reproduzido localmente por limitação de ambiente (sandbox Linux), mas análise estática via busca estrutural confirma a incompatibilidade de tipos.

**V2 — Verificação do source publicado**: download de `https://static.crates.io/crates/sqlite-graphrag/sqlite-graphrag-1.0.67.crate` (885 KB), descompressão via `ouch decompress`, e `difft src/terminal.rs /tmp/crate-1.0.67/inner/sqlite-graphrag-1.0.67/src/terminal.rs` retornou "No changes" — confirma que o bug está no main branch e foi publicado como está.

**V4 — Memórias GraphRAG adicionais consultadas**:
- `rules-unsafe-ffi-pointers-nonnull-aliasing-volatile`: regra "NUNCA converter inteiro arbitrário em ponteiro" confirma que `handle != 0` é o anti-pattern documentado.
- `rust-process-extensions-fds-signals`: "Windows NÃO herda handles por padrão" e "DOCUMENTAR intenção de cada flag" (criação_flags) — relevante para futuras correções que envolvam `CommandExt::creation_flags`.
- `rules-consolidated-unsafe`: "NonNull<T> para non-null guarantee without lifetime" — sugestão de wrapper type-safe `pub struct OwnedHandle(NonNull<c_void>)` com `Drop` que chame `CloseHandle`.
- `g20-silent-argument-discard-30-flags`: padrão "aceita e descarta" — analogamente, o CI aceita o `cargo check` e mascara o erro de compilação, descartando silenciosamente a falha Windows.

### Relações Causa × Efeito

```
                  ┌──────────────────────────────────────┐
                  │ CAUSA RAIZ SISTÊMICA                 │
                  │ Divisão falha entre auditoria de     │
                  │ feature nova e auditoria de regressão│
                  │ cross-platform                       │
                  └──────────┬───────────────────────────┘
                             │
            ┌────────────────┼────────────────────┐
            │                │                    │
            ▼                ▼                    ▼
   ┌────────────────┐ ┌──────────────┐ ┌────────────────────┐
   │ Auditoria      │ │ CI não roda  │ │ Bug introduzido e  │
   │ v1.0.66 migra  │ │ cargo check  │ │ não detectado por  │
   │ range para 0.59│ │ --target     │ │ code review sem    │
   │ sem revisar    │ │ windows      │ │ execução em        │
   │ call sites     │ │ explicit     │ │ Windows            │
   └────────┬───────┘ └──────┬───────┘ └──────────┬─────────┘
            │                │                    │
            └────────────────┼────────────────────┘
                             │
                             ▼
                  ┌──────────────────────┐
                  │ Código terminal.rs:29│
                  │ usa handle != 0 com  │
                  │ HANDLE = *mut c_void │
                  │ em windows-sys 0.59  │
                  └──────────┬───────────┘
                             │
                             ▼
                  ┌──────────────────────┐
                  │ cargo install quebra │
                  │ 100% usuários Win    │
                  └──────────┬───────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
              ▼              ▼              ▼
   ┌──────────────┐ ┌────────────┐ ┌──────────────┐
   │ Onboarding   │ │ Última ver │ │ Trust do     │
   │ Windows      │ │ instalável│ │ ecossistema  │
   │ bloqueado    │ │ é v1.0.65 │ │ prejudicado  │
   └──────────────┘ └────────────┘ └──────────────┘
```

### Solução Proposta (3 opções)

**Opção A — Fix código + pin exato (RECOMENDADA)**:

Corrigir `src/terminal.rs:29` para usar API type-safe com `HANDLE = *mut c_void`:

```rust
#[cfg(windows)]
fn init_windows_console() {
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleCP, SetConsoleMode, SetConsoleOutputCP,
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };
    const CP_UTF8: u32 = 65001;

    // SAFETY: (mesmo comentário)
    unsafe {
        SetConsoleOutputCP(CP_UTF8);
        SetConsoleCP(CP_UTF8);

        for handle_id in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let handle: HANDLE = GetStdHandle(handle_id);
            if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
                let mut mode: u32 = 0;
                if GetConsoleMode(handle, &mut mode) != 0 {
                    let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }
        }
    }
}
```

E fixar `Cargo.toml:111` para `version = "=0.59.0"` (pin exato) para evitar resolução silenciosa para versões mais novas que mudem o tipo novamente. Adicionar job CI `windows-build-check`:

```yaml
windows-build-check:
  name: Windows MSVC compile check
  runs-on: windows-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - run: cargo check --target x86_64-pc-windows-msvc --all-features
    - run: cargo check --target aarch64-pc-windows-msvc --all-features
```

**Opção B — Downgrade windows-sys para "0.52"** (Temporária):

Reverter `Cargo.toml:111` para `version = "0.52"`. Nesta versão, `HANDLE = isize` e o código atual compila sem alterações. **NÃO RECOMENDADA** porque (a) `0.52` está 7 versões atrás, (b) perde features e correções, (c) mensagem de "downgrade" no CHANGELOG é confusa, (d) `0.52` também tem a issue #2436 do windows-rs alertando para bugs.

**Opção C — Migrar para crate `windows` (alto nível)** (Longo prazo):

Substituir `windows-sys` por `windows = "0.58"` que fornece API type-safe e abstrai mudanças de tipo. Exemplo:
```rust
use windows::Win32::System::Console::{GetStdHandle, ...};
let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
if !handle.is_invalid() && !handle.is_null() {
    // ...
}
```
**NÃO RECOMENDADA** para esta iteração: requer refatoração mais ampla, possível breaking change em API pública, e feature surface maior (aumenta tempo de build em ~30%).

### Benefícios da Solução

**Aplicando Opção A**:
- ✅ Windows `cargo install` volta a funcionar em minutos após release
- ✅ `HANDLE` tratado com API idiomática (`is_null()`, `INVALID_HANDLE_VALUE`) — código mais legível
- ✅ Pin exato `=0.59.0` previne regressões futuras por mudança de tipo
- ✅ Job CI `windows-build-check` captura regressões cross-platform ANTES do `cargo publish`
- ✅ Comentário `// SAFETY:` permanece válido
- ✅ Compatibilidade com `windows-sys 0.59` e `0.61+` (ambos com `HANDLE = *mut c_void`)
- ✅ Esforço baixo (10-15 linhas modificadas, 1 novo job CI)

**Aplicando Opção B**:
- ❌ Atrasa resolução definitiva, mantém dívida técnica
- ❌ Pode introduzir novos bugs da versão 0.52

**Aplicando Opção C**:
- ✅ Longo prazo, resolve categoria inteira de problemas FFI
- ❌ Refatoração ampla, alto risco, alto esforço

### Como Solucionar (passo a passo executável)

1. **Criar branch**: `git checkout -b fix/g29-windows-compile-failure`
2. **Editar `src/terminal.rs:1-54`**: substituir expressão de comparação (10-15 linhas modificadas)
3. **Editar `Cargo.toml:111`**: `version = "0.59"` → `version = "=0.59.0"`
4. **Adicionar job CI** em `.github/workflows/ci.yml`: novo job `windows-build-check` (15 linhas)
5. **Rodar validação local**: `cargo check --target x86_64-pc-windows-msvc` (requer toolchain Windows)
6. **Atualizar CHANGELOG.md**: adicionar entrada em `[Unreleased]` com bullet "FIX: terminal.rs Windows HANDLE type check (G29)"
7. **Criar regression test**: `tests/terminal_compile_windows.rs` com `#[cfg(windows)]` que referencia `init_console()`
8. **Commit**: `git add src/terminal.rs Cargo.toml .github/workflows/ci.yml CHANGELOG.md tests/terminal_compile_windows.rs && git commit -m "fix(terminal): use HANDLE.is_null() and INVALID_HANDLE_VALUE for windows-sys 0.59+ compatibility (G29)"`
9. **Push e PR**: `git push origin fix/g29-windows-compile-failure` + abrir PR contra main
10. **Aguardar CI**: verificar que TODOS os 3 OS (ubuntu, macos, windows) passam
11. **Aguardar review**: CODEOWNERS + reviewer
12. **Merge com squash**: usar PR title como commit message
13. **Tag nova versão**: `git tag -a v1.0.68 -m "release v1.0.68"` (patch bump — fix não-breaking)
14. **Push tag**: `git push origin v1.0.68`
15. **Release pipeline automático**: `release.yml` builda binários Windows-msvc atualizados
16. **Anunciar no CHANGELOG.md**: `[1.0.68] - YYYY-MM-DD` com referência a G29

### Esforço, Risco e Impacto

| Opção | Esforço | Risco | Impacto | Recomendação |
|-------|---------|-------|---------|--------------|
| A | 1-2h | BAIXO (apenas 10-15 linhas) | ALTO (resolve 100% Windows install) | SIM |
| B | 30min | MÉDIO (downgrade, perde fixes) | MÉDIO (volta Windows mas mantém dívida) | NÃO |
| C | 4-6h | ALTO (refatoração ampla) | ALTO longo prazo | NÃO nesta iteração |

### Detecção Precoce e Prevenção

**Regressões similares futuras**:
- Adicionar job `windows-build-check` ao CI matrix (passo 4 acima) — captura ANTES do publish
- Adicionar `cargo install --dry-run` smoke test no `release.yml` antes do `cargo publish`
- Adicionar pre-commit hook que detecta padrões `as isize`/`as usize` em código `#[cfg(windows)]`:
  ```bash
  rg 'as isize|as usize' src/ --type rust -n | rg 'cfg\(windows\)' && exit 1
  ```
- Adicionar clippy lint custom (ou aproveitar `clippy::transmute_int_to_ptr`) para detectar cast de inteiro para pointer

**Gatilhos para revisão**:
- Toda mudança em `Cargo.toml` na seção `[target.'cfg(windows)'.dependencies]` deve disparar revisão manual de TODOS os call sites Windows
- Todo bump de `windows-sys` deve incluir `cargo check --target x86_64-pc-windows-msvc` no checklist de PR

### Arquivos Afetados

- `src/terminal.rs:1-54` — modificação do corpo de `init_windows_console` (10-15 linhas)
- `Cargo.toml:111` — pin exato `=0.59.0` (1 linha)
- `.github/workflows/ci.yml` — adição de job `windows-build-check` (15 linhas)
- `CHANGELOG.md` — entrada na seção `[Unreleased]` (3-5 linhas)
- `tests/terminal_compile_windows.rs` (NOVO) — regression test mínimo (5-10 linhas)
- `docs/CLAUDE.md` — nota sobre o padrão Windows FFI (10 linhas, opcional)

### Testes

**Unitários**:
```rust
#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn init_console_compiles_with_windows_sys_059() {
        // Se este módulo compila, o fix está correto
        super::init_console();
    }
}
```

**Integração**:
```rust
// tests/terminal_compile_windows.rs
#![cfg(windows)]

use sqlite_graphrag::terminal::init_console;

#[test]
fn init_console_runs_without_panic() {
    init_console();
}
```

**Aceitação** (manual em Windows):
1. Instalar Rust stable no Windows 10/11
2. Executar `cargo install sqlite-graphrag --version 1.0.68`
3. Verificar que `sqlite-graphrag --version` retorna "1.0.68" sem erros
4. Verificar que saída UTF-8 contém acentos corretamente em cmd.exe e PowerShell

### Conformidades Detectadas (não-violações)

- ✅ `windows-sys` está pinned por range `"0.59"` (não `"*"`) — boa prática geral
- ✅ Feature flag `Win32_System_Console` está corretamente selecionada
- ✅ `#[cfg(windows)]` isola o código problemático — não afeta Linux/macOS
- ✅ Comentário `// SAFETY:` existe — autor documentou intenção
- ✅ Uso de `unsafe {}` está encapsulado em bloco único, não espalhado
- ✅ `CHANGELOG.md` segue Keep a Changelog 1.1.0

### Hipóteses Alternativas Descartadas

- **H1 — Bug introduzido em v1.0.67 especificamente**: REFUTADA. V2 confirmou que `src/terminal.rs` no `.crate` 1.0.67 é IDÊNTICO ao do main. Bug está desde v1.0.66 (introduzido pelo `cross-platform-audit-v1066`).
- **H2 — Lockfile fixa versão diferente**: REFUTADA. `Cargo.lock` do projeto local tem `windows-sys 0.59.0` e o `.crate` publicado também referencia `0.59`. Ambos no mesmo range.
- **H3 — CI matrix não tem Windows**: REFUTADA. Linhas 29 e 44 de `.github/workflows/ci.yml` confirmam `os: [ubuntu-latest, macos-latest, windows-latest]`. O problema é que o job Windows não roda `cargo check --target` explícito.
- **H4 — Cache do GitHub Actions mascara erro**: PLAUSÍVEL mas não confirmada. `Swatinem/rust-cache@v2` em múltiplas linhas pode estar retornando cache stale. Não é a causa primária mas é fator contribuinte.
- **H5 — Toolchain stable do Rust mudou inferência de tipo**: REFUTADA. O tipo `HANDLE` é definido pelo crate `windows-sys`, não pelo Rust. A inferência é determinística.

### Ressalvas e Incertezas

- **R1 (MÉDIA)**: A V1 (reproduzir localmente com `cargo check --target`) não pôde ser executada no sandbox Linux atual por ausência de toolchain MSVC (`lib.exe`). A análise estática via `sg` + `rg` confirma o bug, mas a reprodutibilidade exata do erro E0308 depende de validação em ambiente Windows real. **Mitigação**: CI matrix será a validação canônica após o fix.
- **R2 (BAIXA)**: O `cross-platform-audit-v1066` (memória GraphRAG) diz que o `terminal.rs` foi criado em v1.0.66. Mas o `.crate` publicado da v1.0.66 (verificado indiretamente via release notes do GitHub) tem binários Windows-msvc funcionais. **Hipótese**: o `terminal.rs` foi introduzido em v1.0.66 mas o autor rodou compilação manual em Windows que validou ANTES de mudar para `windows-sys 0.59`, e o bump veio DEPOIS em commit não-relacionado. **Implicação**: a ordem de mudanças no git log seria útil mas o `.cargo_vcs_info.json` do `.crate` só dá SHA (`9ddb17b7c234f58a1059bccbc2ed4b4e3b0bbcbf`), não histórico completo.
- **R3 (MÉDIA)**: O mesmo padrão `as isize` aparece em `src/platform.rs:54-59` do projeto `cli_duckduckgo-search-cli` (verificado durante meta-análise). Isso é G30 potencial mas está FORA de escopo desta iteração. **Follow-up recomendado**: auditar AMBOS os projetos.
- **R4 (BAIXA)**: Pin exato `=0.59.0` impede updates de patch do `windows-sys`. Se a Microsoft publicar 0.59.1 com fix de segurança, não receberemos automaticamente. **Mitigação**: monitorar releases via GitHub watch + Dependabot.
- **R5 (BAIXA)**: O comentário `// SAFETY:` no código atual diz "GetStdHandle returns INVALID_HANDLE_VALUE on failure (checked below)" mas a verificação implementada é `handle != 0`, que é o check de NULL (não de INVALID_HANDLE_VALUE). São coisas DIFERENTES no Win32: NULL é `(HANDLE)0` e INVALID_HANDLE_VALUE é `((HANDLE)-1)`. **Fix correto** deve checar AMBOS.
- **R6 (BAIXA)**: Custo do novo job CI `windows-build-check`: GitHub Actions cobra ~$0.008/minuto para `windows-latest` (custo 2× comparado a `ubuntu-latest`). Build típico de `cargo check` em Windows: ~3-5 min. **Custo estimado**: ~$0.024-0.040 por build × N builds/mês. Para 50 PRs/mês = ~$1-2/mês. Justificável.

### Próximos Passos Recomendados

1. **Imediato (esta iteração)**: implementar Opção A (15 min de código + 30 min de validação) e abrir PR
2. **Curto prazo (próximo sprint)**: merge do PR, tag v1.0.68, monitorar feedback Windows
4. **Longo prazo (backlog)**: considerar Opção C (migração para `windows` crate) após coleta de feedback
5. **Documentação**: atualizar `docs/CLAUDE.md` com seção "Windows FFI patterns" referenciando este G29
6. **Comunicação**: publicar nota no `CHANGELOG.md` da v1.0.68 com bullet "FIX: terminal.rs Windows HANDLE type check (G29)" e agradecimento ao usuário `dr05-caixa01` por reportar
7. **Follow-up de meta-gap (R3)**: agendar auditoria completa de código Windows-specific em AMBOS os projetos do autor
8. **Follow-up de processo (R2 do G28)**: documentar em `rules_consolidated_*.md` que todo bump de crate de FFI deve incluir `cargo check --target <windows_triple>` no checklist

---

## Histórico de Revisões deste Arquivo

- **2026-06-03** (versão inicial): criação do arquivo com G28 documentado após incidente de proliferação de processos. Autor: agente orquestrador via modo de leitura completa do GraphRAG + auditoria externa em `/home/comandoaguiar/Dropbox/ai/gaps_graphrag.md`. Validado contra: `external-process-audit-v1066` (PE01-PE13), `parallelism-audit-v1067` (P01-P13), `g18-daemon-concurrency-semaphore-stuck-at-4-slots`, `g19-enrich-ingest-serial-llm-calls`, `g20-silent-argument-discard-30-flags`, `claude-headless-permissions-hooks`, `claude-headless-env-vars`, `rust-process-extensions-fds-signals`, `rules-consolidated-shutdown`, `daemon-auto-restart-pattern`, `v1058-lessons-learned`, `v1066-llm-runner-dry-debt`, `skill-rust-cli-development`, e validação externa via `context7 docs /tokio-rs/tokio`, `webfetch https://code.claude.com/docs/en/agent-sdk/mcp`, `duckduckgo-search-cli` (3 queries), e `https://github.com/tokio-rs/tokio/issues/2504` para SIGKILL vs SIGTERM em `kill_on_drop`.
- **2026-06-03** (versão G29): adicionada documentação de falha de compilação Windows em v1.0.67 (erro E0308 em `src/terminal.rs:29:26`). Validações empíricas executadas: V1 (cargo check --target) inconclusivo por ausência de toolchain MSVC no sandbox Linux; V2 (download do `.crate` 1.0.67 e difft do `terminal.rs`) confirmou que source publicado é IDÊNTICO ao do main branch — bug presente desde v1.0.66; V3 (busca estrutural via `sg -p '$HANDLE != 0' -l rust` e `rg 'as isize' src/`) confirmou apenas 1 call site e 0 padrões idiomáticos de null-check; V4 (consulta a 4 memórias GraphRAG adicionais) incluiu `rules-unsafe-ffi-pointers-nonnull-aliasing-volatile`, `rust-process-extensions-fds-signals`, `rules-consolidated-unsafe`, e `g20-silent-argument-discard-30-flags`. Causa raiz: breaking change do tipo `HANDLE` em `windows-sys` entre 0.48/0.52 (`isize`) e 0.59+ (`*mut c_void`) introduzido pela Microsoft sem migração do call site em `terminal.rs:29`. Solução recomendada: Opção A (fix do código com `handle.is_null()` + `handle != INVALID_HANDLE_VALUE` + pin exato `=0.59.0` + job CI `windows-build-check`). Meta-achado: mesmo padrão `as isize` em `cli_duckduckgo-search-cli/src/platform.rs:54-59` (fora de escopo, segue como G30 follow-up).
- **2026-06-03** (resolução v1.0.68): G28-A (MCP isolation via `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` em `claude_runner.rs`; `--strict-mcp-config` e `--mcp-config '{}'` IGNORADOS upstream — confirmado via GitHub issue [anthropics/claude-code#10787]), G28-B (`lock::acquire_job_singleton(JobType, namespace, wait_seconds)` integrado em `enrich`/`ingest_claude`/`ingest_codex` com nova `AppError::JobSingletonLocked { job_type, namespace }` exit-75), G28-D (`retry::CircuitBreaker` com `AttemptOutcome::{Success, Transient, HardFailure}` + warning `tracing::warn!` quando `llm_parallelism > 4` em `enrich.rs`). G29 corrigido em `src/terminal.rs:29` (`!handle.is_null() && handle != INVALID_HANDLE_VALUE` importando `HANDLE` e `INVALID_HANDLE_VALUE` de `windows_sys::Win32::Foundation`), `Cargo.toml:111` com pin exato `=0.59.0`, novo job CI `windows-build-check`, `tests/terminal_compile_windows.rs` adicionado. 3 falhas de teste pré-existentes em `src/commands/{history,list,read}.rs` corrigidas (testes timezone-agnostic via `parse_from_rfc3339` + comparação de `timestamp()`). Validação final: `cargo fmt --all --check` clean, `cargo check --all-targets` 0 erros, `cargo clippy --all-targets --all-features -- -D warnings` 0 warnings, `cargo doc --no-deps --all-features` ZERO warnings com `RUSTDOCFLAGS="-D warnings"`, `cargo test --lib` 692 passed 0 failed (3 ignored pré-existentes), `cargo test --test terminal_compile_windows` 2 passed 0 failed. Branch `release/v1.0.68` contém 21 modified files + 1 untracked, sem commit/push/publish (autorização pendente). Documentação atualizada: `README.md`/pt-BR "Destaques da Versão" + aviso Windows em Quick Start; `docs/CROSS_PLATFORM.md`/pt-BR nova seção "Tipo HANDLE e o Limite do windows-sys 0.59 (G29)"; `docs/AGENTS.md`/pt-BR nova seção "New in v1.0.68"; `docs/HOW_TO_USE.md`/pt-BR nova seção "Limitando proliferação de processos (G28)"; `docs/MIGRATION.md`/pt-BR nova seção "v1.0.68"; `docs/TESTING.md`/pt-BR nova seção "Testes de Regressão v1.0.68". G28-C (morte conjunta via `prctl(PR_SET_PDEATHSIG)` + reaper de órfãos) deferred por risco: requer migração para `tokio::process::Command` + `Drop` impl customizado (issue tokio #2504). Adicionalmente nesta rodada (segunda passada de auditoria D13-D21): ADR-008 (G28-B) `docs/decisions/adr-008-process-lifecycle-singleton.md`, ADR-009 (G29) `docs/decisions/adr-009-windows-sys-handle-pinning.md`, ADR-010 (G28-A) `docs/decisions/adr-010-mcp-isolation-claude-config-dir.md`. `docs/DOCUMENTATION_FRAMEWORK.md` atualizado: 3 gaps estruturais (README cross-ref, INTEGRATIONS cross-ref, GitHub templates) marcados como STATUS LEGADO corrigidos em v1.0.68, checklist "Antes do Primeiro Release" marcado como 100% completo (`[x]` em todos os 28 itens), nova subseção "Quando o Checklist Está 100% Concluído" orientando projetos herdeiros, contagem de arquivos raiz corrigida de 19 para 18 MD + 2 pares de templates. `docs/schemas/error-envelope.schema.json` expandido com nota sobre o segundo template de `code: 75` (G28-B `JobSingletonLocked { job_type, namespace }`) e como agentes devem parsear `job_type` e `namespace` via regex na string `message`. `docs/schemas/README.md` nova seção "Error Envelope Changes in v1.0.68 (G28-B)" explicando os 2 templates. 23 arquivos modificados + 7 novos arquivos criados (3 ADRs + 2 ISSUE_TEMPLATE + 1 PULL_REQUEST_TEMPLATE + 1 test) + 1 untracked file. Terceira passada D29-D37: skill audit profundo revelou 2 typos (sequeencie-os em SKILL.md PT linha 150 e MIGRATION.pt-BR.md linha 80, ambos corrigidos para sequencie-os), 3 lacunas de conteúdo (Test Fixes ausente em SKILL.md New in v1.0.68, Exit Code 75 dual template ausente em SKILL.md Exit Codes, Error JSON Contract sem nota v1.0.68 — todas corrigidas em ambos idiomas), 3 cross-refs bilíngues ausentes (SKILL.md EN↔PT linha 9, llms.txt↔llms.pt-BR.txt linha 7, llms-full.txt apontando para llms.txt+llms.pt-BR.txt linha 9). Quarta passada D38-D43: CHANGELOG.md v1.0.68 Fixed section não mencionava as 3 timezone test fixes (D38 — CRÍTICA, agora corrigido em CHANGELOG.md linha 21 e CHANGELOG.pt-BR.md linha 22), CHANGELOG heading `## [1.0.67]` duplicado (D39 — bug histórico, agora corrigido em CHANGELOG.md e CHANGELOG.pt-BR.md), .github/ISSUE_TEMPLATE/config.yml ausente (D41 — agora criado, 1048 bytes, desabilita blank_issues + 4 contact links para AGENTS.md, Discussions, Security Advisories, CHANGELOG), CONTRIBUTING.md/pt-BR.md sem seção v1.0.68 (D42 — agora ambos têm `## Recent Releases` com 12-bullet summary + `## Mandatory Pre-Push Checklist` com 11 itens incluindo Conventional Commits e No-Co-authored-by gate), CHANGELOG [Unreleased] section vazia (D43 — agora com nota explicativa em ambos idiomas). 28 arquivos modificados + 8 arquivos novos criados + 1 untracked file.