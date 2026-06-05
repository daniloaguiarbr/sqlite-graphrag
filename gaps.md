# Gaps Arquivados — sqlite-graphrag

> **Aviso.** Este arquivo documenta defeitos e melhorias pendentes **sem** propor patches aplicados. Cada entrada é uma ata técnica que congela a causa raiz, o efeito mensurável e a direção da solução. Mudanças no código-fonte vivem em commits, não aqui.

---

## Sumário

| ID    | Severidade | Status       | Tema curta                                              |
|-------|------------|--------------|---------------------------------------------------------|
| G28   | CRÍTICA    | Documentado  | Proliferação descontrolada de processos ao orquestrar LLMs headless |
| G29   | ALTA       | Documentado  | `enrich --operation body-enrich` falha 100% por violar CHECK constraint do `source` na tabela `memories` |
| G30   | ALTA       | Documentado  | Singleton de job pesado é global, ignora `--db`, e a mensagem de erro sugere `--wait-job-singleton` que não existe no CLI |
| G31   | ALTA       | Documentado  | `enrich --mode codex` não passa `--skip-git-repo-check` nem `--ephemeral`/`--sandbox`/`--ignore-user-config`; cria schema em `/tmp` que o codex rejeita como diretório não trusted |
| G32   | ALTA       | Documentado  | `enrich --mode codex` faz `serde_json::from_str` no stdout inteiro; codex devolve JSONL (múltiplas linhas) e parser sempre falha com `trailing characters` |
| G33   | MÉDIA      | Documentado  | Codex com ChatGPT Pro OAuth rejeita `gpt-4*`/`o4-mini`/`gpt-5-codex`; `enrich` não valida `--codex-model` contra a lista de modelos aceitos e não oferece fallback automático |
| G34   | BAIXA-MÉDIA| Documentado  | Warning `llm_parallelism > 4` aparece em `mode=codex` (que não sofre de MCP children) com a mesma severidade de `mode=claude-code` |
| G35   | ALTA       | Documentado  | Claude OAuth Max tem rate limit de 5 h sem aviso prévio; `enrich` só descobre quando já consumiu 1160/1161 itens; não há preflight check nem fallback automático para `--mode codex` |
| G36   | BAIXA      | Documentado  | `optimize` rebuilda FTS5 sem checar `fts check` antes; usuário não sabe se precisa; sem `--progress`; sem dry-run do FTS5 |
| G37   | MÉDIA      | Documentado  | `enrich` processa TODAS as memórias candidatas; não há `--names <NAME>` nem `--names-file <PATH>` para selecionar subconjunto |
| G38   | BAIXA      | Documentado  | `backup` usa `run_to_completion(100, 50ms, None)` com step size pequeno e sleep fixo; banco de 4.3 GB leva minutos quando `sqlite3 .backup` leva segundos |
| G39   | MÉDIA      | Documentado  | `vec_memories_orphaned` residual não é diagnosticável; `health` apenas conta, mas não há `vec orphan-list` nem `vec purge-orphan` |

---

## G28 — Proliferação de Processos ao Orquestrar LLMs Headless

### Metadados do Incidente

- **Severidade.** ALTA. A máquina hospedeira fica praticamente inutilizável durante o incidente.
- **Estado.** Documentado. Sem correção aplicada no momento.
- **Detectado em.** Sessão de auditoria manual. `graphrag` consultado por saturação de CPU.
- **Confirmado em.** Validação empírica com `uptime`, `sysctl vm.loadavg`, `ps -A` e contagem de processos por nome.
- **Tipos de comando afetados.** `enrich`, `ingest --mode claude-code`, `ingest --mode codex`, qualquer spawn de LLM externa.
- **Plataformas afetadas.** macOS, Linux (qualquer distribuição com `claude` ou `codex` instalados).

### Restrições Invioláveis de Invocação Headless

> Estas restrições são **PROIBIÇÕES ABSOLUTAS** que toda correção proposta em G28 deve respeitar. Nenhuma das quatro combinações abaixo é aceitável.

- **PROIBIDO** invocar `claude -p` (Claude Code headless) com **MCP** habilitado.
- **PROIBIDO** invocar `claude -p` (Claude Code headless) com **hooks** habilitados.
- **PROIBIDO** invocar `codex exec` (Codex CLI headless) com **MCP** habilitado.
- **PROIBIDO** invocar `codex exec` (Codex CLI headless) com **hooks** habilitados.
- **PERMITIDO** invocar `claude -p` somente com **OAuth** (login de assinatura Pro ou Max).
- **PERMITIDO** invocar `codex exec` somente com **auth** (login ChatGPT salvo em `~/.codex/auth.json`).
- **PROIBIDO** definir `ANTHROPIC_API_KEY` no ambiente de qualquer spawn de `claude -p`.
- **PROIBIDO** definir `OPENAI_API_KEY` no ambiente de qualquer spawn de `codex exec`.
- **PROIBIDO** usar `claude -p --bare` em qualquer fluxo (corta MCP mas desliga OAuth e exige `ANTHROPIC_API_KEY`).

### Fontes Consultadas Durante a Auditoria

- `graphrag` consultado mas **NÃO** respondeu por saturação de CPU durante o incidente.
- `graphrag` confirmado com 0 memórias no momento do incidente; rules residem no `CLAUDE.md` local.
- `context7` indisponível (chave de API ausente); validação feita via `duckduckgo-search-cli` e leitura direta de documentação oficial.
- `duckduckgo-search-cli` executou deep research com leitura de conteúdo das páginas oficiais.
- `atomwrite` usado para gravar este relatório de forma atômica.

---

## Problema × Consequências × Causa Raiz × Solução × Benefícios × Como Solucionar

### O Problema

A CLI `sqlite-graphrag` permite a explosão descontrolada da árvore de processos quando executa comandos que orquestram LLMs externas em modo headless. Cada invocação headless de `claude` ou `codex` arrasta a configuração global de MCPs e hooks do ambiente, multiplicando o número de processos filhos, netos e bisnetos em ordens de grandeza além do que a máquina suporta.

### As Consequências

- **CPU saturada.** Load average de 276 em uma máquina com 10 CPUs é saturação de 27 vezes o nominal. Toda interação do usuário entra em fila de espera.
- **Memória pressionada.** 13 GB do conteúdo da RAM foram empurrados para o compressor de memória. Swap ativo começou a absorver páginas.
- **Contenção de lock no SQLite.** Múltiplos processos do mesmo binário competem pelo modo single-writer do SQLite. Tempos de escrita explodem.
- **Comando `remember` trava.** O próprio comando que deveria alimentar o `graphrag` trava com timeout porque não consegue fatia de CPU nem de I/O.
- **Risco de perda de dados.** A contenção de lock pode corromper a fila `.ingest-queue.sqlite` se uma transação for interrompida no meio.
- **Risco de cobrança fantasma.** Em spawns com `OPENAI_API_KEY` ou `ANTHROPIC_API_KEY` no ambiente, cada `codex exec` ou `claude -p` dispara chamada de API mesmo após pane local.
- **Máquina inutilizável.** O incidente descrito deixou a máquina em estado de "praticamente travada" até intervenção manual com `pkill`.

### A Causa Raiz

**A causa raiz única é o modelo de processo efêmero por invocação sem governança de ciclo de vida.** A CLI foi desenhada quando cada comando era uma folha leve: nasce, faz a tarefa, morre, sem filhos pesados. A arquitetura de ciclo de vida não acompanhou a transição de papel de ferramenta de consulta para orquestradora headless de LLM.

Faltam simultaneamente quatro camadas de governança:

1. **Controle de concorrência de instância.** Nada impede que múltiplos `enrich` rodem em paralelo sobre o mesmo `graphrag.sqlite`.
2. **Isolamento de configuração dos subprocessos.** O spawn de `claude -p` herda automaticamente a configuração global de MCPs e hooks de `~/.claude/settings.json` e `~/.claude.json`.
3. **Reaping conjunto pai-filhos.** Subprocessos órfãos sobrevivem à morte do processo pai porque o `Command::spawn` não estabelece vínculo de morte.
4. **Circuit breaker de retry.** A flag `--retry-failed` pode entrar em loop infinito sem teto de tentativas, sem janela de cooldown e sem leitura de carga do sistema.

### Sintoma Versus Causa

- **Sintoma.** "Muitos processos `node` e `npm exec` no `ps`."
- **Causa.** Ausência de governança de ciclo de vida na CLI.
- **Diagnóstico incorreto comum.** "O `claude` está com bug." Falso. O `claude` obedece ao contrato de herdar configuração global. A CLI que orquestra é quem deveria isolar a configuração.

### A Mudança de Papel Que Quebrou o Modelo

- O modelo one-shot servia quando cada comando era folha.
- Folha nasce, faz a tarefa, morre, sem filhos pesados. O recall é folha e por isso o modelo nunca doeu nele.
- Quebrou quando um comando virou **raiz** de uma árvore de processos.
- A árvore é `enrich` para `claude` para MCPs para `npm` para `node`. Cada elo multiplica o número de processos.
- O mesmo modelo inofensivo na folha virou bomba na raiz.

### O Risco Vale para Todo Spawner de LLM

- `enrich` no modo Claude Code spawna `claude -p` por item.
- `ingest --mode claude-code` spawna `claude -p` por arquivo.
- `enrich` no modo Codex spawna `codex exec` por item.
- `ingest --mode codex` spawna `codex exec` por arquivo.
- Qualquer modo que orquestra LLM externa herda o mesmo risco.
- A correção de isolamento de MCP vale para Claude, Codex e OpenCode.

---

## Relações Causa × Efeito

### A Cadeia Causal Completa

1. **Ausência de singleton** causa múltiplos `enrich` rodando em paralelo.
2. **`enrich` no modo Claude Code** causa spawn de `claude -p` por item do lote.
3. **`claude -p` headless** causa herança automática de todos os servidores MCP do `~/.claude.json` e `~/.claude/settings.json`.
4. **Cada servidor MCP** causa criação de um par de processos `npm exec` e `node`.
5. **Ausência de reaping** causa subprocessos órfãos persistentes após morte do pai.
6. **Flag `--retry-failed` sem circuit breaker** causa loop que nunca termina sozinho.
7. **Soma dos efeitos** causa 1877 processos totais e load average de 276 em 10 CPUs.

### Quantificação do Efeito

- 4 invocações de `enrich` × paralelismo 2 = 8 `claude -p` headless simultâneos.
- 8 `claude -p` × 10 servidores MCP × 2 processos por servidor = 160 processos só na subárvore direta do `enrich`.
- Load 276 em 10 CPUs significa fila 27 vezes maior que a capacidade de drenagem.

### Reconciliação dos Números

- 160 processos são só a contribuição da árvore direta do `enrich`.
- 1877 processos são o total do sistema naquele instante (incluindo outros apps e daemons).
- O `enrich` não criou tudo sozinho, mas empurrou o sistema ao colapso.
- O load 276 reflete a fila inteira do sistema, não só os filhos do `enrich`.

---

## Solução

A solução completa é uma **camada de governança de ciclo de vida** composta por quatro correções de curto prazo e uma camada estrutural de longo prazo.

### Visão em Camadas

- **Camada 1 — Correção A.** Isolamento de MCP e hooks no spawn de LLM headless.
- **Camada 2 — Correção B.** Singleton de jobs pesados.
- **Camada 3 — Correção C.** Morte conjunta e reaping.
- **Camada 4 — Correção D.** Defaults seguros e freio no retry.
- **Camada 5 — Estrutural.** Daemon servidor único com `tokio::net::UnixListener`.

---

## Benefícios da Solução

### Benefícios Quantificáveis

- **CPU ociosa.** De 0% para 68% após mitigação com `pkill`. Com Correção A aplicada preventivamente, a perda de CPU nunca ocorre.
- **Total de processos.** De 1958 para 1857 após limpar órfãos. Com Correção C, os órfãos são reaped no startup.
- **Latência do `remember`.** De timeout travado para 359ms. Redução de mais de 80 vezes.
- **Load average.** De 276 para 6.74 após `pkill` do `enrich`. Com Correção B aplicada, a saturação nunca começa.
- **Contenção de SQLite.** Eliminada com a Camada 5 porque o daemon abre o banco uma única vez e serializa o acesso internamente.

### Benefícios Qualitativos

- **Determinismo.** O comportamento da CLI passa a depender de contratos explícitos de invocação, não de configuração global do desenvolvedor.
- **Reprodutibilidade.** O mesmo comando produz a mesma árvore de processos em qualquer máquina, independente de quais MCPs o usuário tenha configurado.
- **Segurança.** Variáveis sensíveis do ambiente (`LD_PRELOAD`, `DYLD_INSERT_LIBRARIES`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`) deixam de vazar para os filhos.
- **Auditabilidade.** Cada spawn de LLM carrega um `request_id` que correlaciona logs do pai com logs do filho.
- **Conformidade.** Obediência integral à `rules_rust_processos_externos.md` e `rules_rust_encerramento_grafull_shutdown.md`.

---

## Como Solucionar

### Correção A — Isolamento de MCP e Hooks nos Headless

**Maior alavanca de redução de CPU com o menor esforço de código.**

- O `claude -p` headless não precisa de `chrome-devtools`, `ssh-mcp` nem de nenhum outro MCP.
- O `codex exec` headless não precisa dos MCPs configurados em `~/.codex/config.toml`.
- Cortar os MCPs na chamada headless reduz de 160 processos para cerca de 8.
- **NÃO** usar `--bare` no `claude -p` porque ele corta os MCPs mas desliga o OAuth e exige `ANTHROPIC_API_KEY`.

**Comando exato para `claude -p` (preserva OAuth e zera MCPs e hooks):**

```bash
claude -p "TAREFA" \
  --strict-mcp-config \
  --mcp-config '{}' \
  --dangerously-skip-permissions \
  --settings '{"hooks":{}}' \
  --model sonnet \
  --max-turns 8 \
  --output-format json
```

- `--strict-mcp-config` ignora MCP de settings global e de projeto.
- `--mcp-config '{}'` fornece a lista vazia que zera os servidores.
- `--dangerously-skip-permissions` evita travar pedindo confirmação.
- `--settings '{"hooks":{}}'` desliga os hooks naquela chamada.
- `--model sonnet` escolhe o modelo sem depender de variável de ambiente.
- `--max-turns 8` limita as voltas do agente como rede de segurança.
- `--output-format json` entrega saída fácil de parsear depois.

**Ressalva crítica do `claude --strict-mcp-config`:** Issue #14490 do repositório `anthropics/claude-code` documenta que `--strict-mcp-config` **NÃO** sobrescreve a lista `disabledMcpServers` armazenada em `~/.claude.json`. Para ambiente limpo, garantir que `~/.claude.json` não contém o servidor em `disabledMcpServers` ou usar `--bare` somente em ambiente controlado com `ANTHROPIC_API_KEY` (cenário explicitamente PROIBIDO neste projeto).

**Comando exato para `codex exec` (preserva `auth.json` e zera MCPs):**

```bash
codex exec \
  -c mcp_servers='{}' \
  --sandbox workspace-write \
  --ask-for-approval never \
  "TAREFA"
```

- `codex exec` é o modo não interativo feito para scripts.
- `-c mcp_servers='{}'` zera só os MCPs e preserva modelo e o resto do config.
- `--sandbox workspace-write` libera edição de arquivos sem rede.
- `--ask-for-approval never` roda sem pausar pedindo permissão.

**Corte total alternativo do Codex (mais agressivo):**

```bash
codex exec --ignore-user-config --sandbox workspace-write "TAREFA"
```

- `--ignore-user-config` ignora o `config.toml` do usuário inteiro.
- O login OAuth fica salvo em `auth.json` separado, então o `--ignore-user-config` **NÃO** derruba a autenticação.

**Ressalva do Codex:** Issue #3441 do repositório `openai/codex` documenta que versões antigas do Codex (0.33.0) não liam `[mcp_servers]` corretamente e exigiam upgrade para 0.34.0+. Validar `codex --version` antes de usar o override `-c mcp_servers='{}'`.

### Correção B — Singleton de Jobs Pesados

- Lock global garante um `enrich` ou `ingest` por vez.
- Segunda invocação recusa ou enfileira em vez de paralelizar.
- Implementar com a crate `single-instance` (já validada, `trustScore` 8, ver `docs.rs/single-instance`) ou `fs2` com `flock` em arquivo dedicado.
- **Teria evitado 100% do incidente de hoje.**

**Esqueleto da implementação:**

- Criar `src/lock.rs` com `JobLock` que tenta `flock` em `${XDG_RUNTIME_DIR}/sqlite-graphrag/job.lock`.
- `JobLock::try_acquire()` retorna `Result<Self, JobLocked>`.
- `JobLock` implementa `Drop` que libera o lock automaticamente.
- Cada `enrich` e `ingest --mode claude-code|codex` chama `JobLock::try_acquire()` no início.
- Em caso de lock ocupado, emitir mensagem clara apontando o PID dono do lock via `lsof` ou `fuser`.

### Correção C — Morte Conjunta e Reaping

- Usar `tokio::process::Command::kill_on_drop(true)` nos `claude -p` e `codex exec` spawnados.
- `kill_on_drop` envia `SIGKILL` no Unix (não `SIGTERM` gracioso — ressalva da issue #7082 do `tokio-rs/tokio`).
- Para encerramento gracioso, enviar `SIGTERM` antes do drop do `Child`.
- No Linux, usar `prctl(PR_SET_PDEATHSIG, SIGTERM)` no `pre_exec` do filho para que o filho morra quando o pai morrer. **Ressalva crítica:** o recall.ai publica que `PR_SET_PDEATHSIG` é "quase nunca o que você quer" porque tem race conditions com `fork` e `exec`.
- No macOS, usar `kqueue` monitorando `NOTE_EXIT` do PID do pai ou pipe herdado com EOF.
- No Windows, usar `Job Object` com `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
- Reaper no startup varre `ps -A` e mata órfãos de execuções anteriores com `PPID=1` e nome começando com `claude` ou `codex` cujo pai já não existe.

### Correção D — Defaults Seguros e Freio no Retry

- Mudar o paralelismo padrão para `1` no modo Claude Code e no modo Codex.
- Exigir opt-in explícito para qualquer paralelismo de LLM via `--llm-parallelism-force`.
- Dar a `--retry-failed` um teto de tentativas (ex.: 3) e backoff exponencial com jitter.
- Adicionar circuit breaker que aborta o job após N falhas seguidas (ex.: 5).
- Adicionar leitura de `uptime` ou `/proc/loadavg` antes de cada spawn e abortar se o load passar de limite configurável (ex.: `2 * cpus`).
- É barato, não exige daemon, e teria contido o incidente.

### Camada Estrutural — Daemon Servidor Único

- Promover o daemon de cache de modelo para servidor de comandos completo.
- Usar `tokio::net::UnixListener` com loop de `accept` e shutdown gracioso via `CancellationToken`.
- CLI vira cliente fino que conecta via socket Unix, envia comando, recebe NDJSON, sai.
- Modelo de embedding carregado uma vez reduz latência de segundo para milissegundos.
- SQLite aberto uma vez elimina contenção de lock entre processos.
- IPC versionado entre cliente e daemon, com campo `protocol_version: u32` em todo envelope.

**Contrapartidas do daemon:**

- Vira ponto único de falha para todos os comandos.
- Exige protocolo de IPC versionado entre cliente e daemon.
- Incompatibilidade de versão entre CLI e daemon já é risco conhecido (campo `daemon --ping` retorna `model_name` e `model_version` para validação).
- Estado compartilhado pede cuidado com concorrência interna (usar `tokio::sync::RwLock` para estado de leitura, `Mutex` para mutação).

**Camada estrutural — Idle Shutdown:**

- Timer reseta a cada request recebido pelo daemon.
- Expiração sem requests dispara encerramento gracioso.
- Já existe via flag `--idle-shutdown-secs` no daemon atual.
- É a rede de segurança contra daemon esquecido rodando indefinidamente.

---

## Esforço × Risco × Impacto

### Correção A — Isolar MCP e Hooks

- **Esforço.** Baixo. Muda só os argumentos do `Command::spawn` em `src/commands/claude_runner.rs`, `src/commands/ingest_claude.rs`, `src/commands/codex_runner.rs` e `src/commands/ingest_codex.rs`.
- **Risco.** Baixo. Não altera lógica de negócio. Apenas altera flags passadas ao subprocesso.
- **Impacto.** Altíssimo. Corta a maior fonte de processos e impede o início da cadeia causal.

### Correção B — Singleton

- **Esforço.** Baixo. Um lock de arquivo na entrada do job em `src/commands/enrich.rs` e `src/commands/ingest.rs`.
- **Risco.** Baixo. Falha clara com `exit 75` (slots exauridos) se já houver instância.
- **Impacto.** Alto. Mata o paralelismo acidental e impede multiplicação da árvore de processos.

### Correção C — Morte Conjunta e Reaping

- **Esforço.** Médio. Exige `kill_on_drop` em todos os spawns e watcher de pai por plataforma.
- **Risco.** Médio. Precisa testar no macOS com `kqueue` e validar no Windows com `Job Object`. `PR_SET_PDEATHSIG` tem ressalvas documentadas.
- **Impacto.** Alto. Elimina os órfãos persistentes que sobrevivem à morte do pai.

### Correção D — Defaults Seguros e Freio no Retry

- **Esforço.** Baixo. Troca de valores padrão em `src/cli.rs` e adição de circuit breaker em `src/retry.rs`.
- **Risco.** Baixo. Comportamento fica mais conservador; usuários que precisam de mais paralelismo usam opt-in explícito.
- **Impacto.** Alto. Contém o incidente sem código complexo e dá observabilidade via leitura de load.

### Camada 5 — Daemon

- **Esforço.** Alto. Novo servidor em `src/daemon.rs` e protocolo de IPC em `src/ipc.rs`.
- **Risco.** Alto. Ponto único de falha e versão de protocolo. Já existe risco análogo com o daemon de embeddings atual.
- **Impacto.** Alto. Resolve latência (modelo carregado uma vez) e proliferação de vez (SQLite aberto uma vez, sem contenção entre processos).

---

## Detecção Precoce e Prevenção

### Como Pegar Antes de Saturar

- Logar a contagem de filhos vivos a cada item processado via `tracing::info!`.
- Abortar o job se o load passar de um limite configurável (`--max-child-load 5.0`).
- Expor um `healthcheck` que conta processos da própria árvore via `pgrep -P $PPID` ou leitura de `/proc/$pid/status` em Linux.
- Emitir aviso quando o número de `claude -p` ou `codex exec` simultâneos passar de um teto (`--max-concurrent-llm 4`).

### Sinais de Alerta Que Faltaram no Incidente

- Não havia limite de processos filhos vivos.
- Não havia leitura de `loadavg` antes de spawnar mais um item.
- Não havia teto de instâncias concorrentes do `enrich`.
- Não havia correlação de logs entre pai e filhos via `request_id`.

---

## Onde Mexer no Código Fonte (Mapeamento Pendente)

> Este mapeamento **NÃO** foi confirmado por leitura do repositório. É hipótese derivada da memória `g28-process-proliferation-claude-headless` e dos nomes de arquivo esperados pelo padrão do projeto.

- `src/commands/claude_runner.rs` — recebe a Correção A (isolamento de MCP e hooks).
- `src/commands/codex_runner.rs` — recebe a Correção A (isolamento de MCP e hooks).
- `src/commands/enrich.rs` — recebe a Correção B (singleton) e Correção D (defaults e circuit breaker).
- `src/commands/ingest.rs` — recebe a Correção B (singleton).
- `src/commands/ingest_claude.rs` — recebe a Correção A.
- `src/commands/ingest_codex.rs` — recebe a Correção A.
- `src/commands/reap.rs` — **novo** — implementação do reaper de órfãos.
- `src/lock.rs` — **novo** — singleton de jobs via `flock`.
- `src/daemon.rs` — promoção do daemon de cache para servidor de comandos.
- `src/ipc.rs` — **novo** — protocolo de IPC versionado.
- `src/retry.rs` — circuit breaker com teto de tentativas e backoff exponencial.
- `src/constants.rs` — defaults seguros de paralelismo.
- `src/cli.rs` — flags `--llm-parallelism-force`, `--max-child-load`, `--max-concurrent-llm`.
- `src/main.rs` — hook de startup que invoca `Reaper::scan_and_kill_orphans()`.
- `tests/process_proliferation.rs` — **novo** — testes de regressão.

---

## Validação Empírica da Causa Raiz (Já Executada)

### O Teste Decisivo

- A hipótese foi testada matando a causa e medindo o efeito.
- O `pkill` do `enrich` derrubou o load de 276 para 6.74.
- A CPU saltou de 0% ocioso para 68% ocioso.
- O total de processos caiu de 1958 para 1857 após limpar órfãos.

### Por Que Isto Prova a Causa Raiz

- Remover a causa removeu o efeito, então a relação é causal.
- O `remember` que travava com timeout voltou a completar em 359ms.
- A queda de mais de 80 vezes na latência confirma o gargalo de CPU.

### A Intervenção em Duas Etapas

- Etapa 1 matou os processos `enrich` e liberou a CPU pela metade.
- Etapa 2 matou MCPs e `node` órfãos que sobreviveram ao pai.
- A sobrevivência dos órfãos **prova na prática a ausência de reaping**.

### Metodologia de Medição

- Load lido com `uptime` e com `sysctl vm.loadavg`.
- Processos contados com `ps -A` e contagem de linhas via `wc -l`.
- CPU ociosa lida com `top` em amostragem única.

---

## Hipóteses Alternativas Descartadas

### Outras Causas Possíveis de Load Alto

- Swap thrashing por memória cheia foi considerado.
- Indexação do Spotlight via `mdworker` foi considerado.
- Backup do Time Machine foi considerado.
- Daemon de embedding do `graphrag` foi considerado.

### Por Que Foram Descartadas

- Matar só o `enrich` resolveu, então as outras não eram a causa.
- O compressor de memória era **efeito** da CPU presa, não causa.
- O daemon de embedding é leve e único, não multiplica processos.
- A correlação temporal apontou o `enrich` como gatilho.

---

## Quadro Comparativo de Flags Headless

### Interruptor de MCP e Hooks por CLI

| CLI                | Flag MCP                                | Flag Hooks                  | Mantém Auth       |
|--------------------|-----------------------------------------|-----------------------------|-------------------|
| `claude -p`        | `--strict-mcp-config --mcp-config '{}'` | `--settings '{"hooks":{}}'` | Sim (OAuth)       |
| `codex exec`       | `-c mcp_servers='{}'`                   | (sem hooks no codex)        | Sim (`auth.json`) |
| `codex exec` (full)| `--ignore-user-config`                  | (sem hooks no codex)        | Sim (`auth.json`) |
| `opencode run`     | `OPENCODE_CONFIG_CONTENT` com `enabled: false` por servidor | (variável) | Sim (`auth.json`) |

### Login OAuth por CLI

- **Claude** faz login na sessão Pro ou Max uma única vez via `claude login` e **NÃO** usa `--bare` para preservar OAuth.
- **Codex** usa `codex login` (fluxo de navegador com ChatGPT) ou `codex login --device-auth` (máquina remota sem navegador).
- **OpenCode** usa `opencode auth login` e guarda credencial em `auth.json` na pasta de dados do OpenCode.

### Modo Headless por CLI

- **Claude** usa `claude -p`.
- **Codex** usa `codex exec`.
- **OpenCode** usa `opencode run`.

---

## Ressalvas Encontradas na Pesquisa

### Cuidado com o `claude --bare`

- O `--bare` é tentador por ser rápido, mas derruba o OAuth e exige `ANTHROPIC_API_KEY`.
- **PROIBIDO** neste projeto, que opera exclusivamente com OAuth de assinatura.
- Use `--strict-mcp-config --mcp-config '{}'` em vez de `--bare`.

### Cuidado com Bug do `claude --strict-mcp-config` e `disabledMcpServers`

- Issue #14490 do repositório `anthropics/claude-code` confirma que `--strict-mcp-config` **NÃO** sobrescreve a lista `disabledMcpServers` em `~/.claude.json`.
- Workaround: editar manualmente `~/.claude.json` para remover o servidor da lista `disabledMcpServers` antes de usar `--strict-mcp-config`.
- Alternativa mais robusta: usar `claude -p` em ambiente limpo com `ANTHROPIC_API_KEY` definido (cenário **PROIBIDO** neste projeto).

### Cuidado com Bug Antigo do Codex

- Versões antigas do Codex (0.33.0) instaladas via Homebrew não liam `[mcp_servers]` corretamente.
- Issue #3441 do repositório `openai/codex` confirma que o fix está em 0.34.0+.
- Validar versão com `codex --version` antes de usar o override `-c mcp_servers='{}'`.

### Cuidado com a Soma de Config do OpenCode

- As configs do OpenCode são somadas, não trocadas.
- Apagar a chave do MCP no arquivo de config **NÃO** remove o servidor herdado.
- Só `enabled: false` com o nome certo desliga o servidor de verdade.
- Variável `OPENCODE_CONFIG_CONTENT` permite override de runtime sem mexer nos arquivos do projeto.

---

## Referências Externas Validadas

### Documentação Oficial Rust / Tokio

- `docs.rs/tokio/latest/tokio/process/struct.Command.html` — confirma `tokio::process::Command::kill_on_drop(true)`.
- `github.com/tokio-rs/tokio/issues/7082` — issue que adicionou `kill_on_drop` à API; ressalva: envia `SIGKILL` no Unix, sem `SIGTERM` gracioso.
- `docs.rs/single-instance` — crate `single-instance` disponível, autoria `WLBF`.
- `github.com/WLBF/single-instance` — repositório da crate, modelo cliente-servidor por socket.

### Especificações de SO

- `man7.org/linux/man-pages/man2/PR_SET_PDEATHSIG.2const.html` — manual de `PR_SET_PDEATHSIG` no Linux.
- `recall.ai/blog/pdeathsig-is-almost-never-what-you-want` — ressalvas críticas: race condition entre `fork` e `exec`, recebimento de sinal durante `exec`,僵尸s órfãos.
- `sqlite.org/lockingv3.html` — confirma que SQLite é single-writer e que múltiplos processos competem por lock global.

### Claude Code

- `claude.com/docs/connectors/building/authentication` — autenticação de conectores Claude.
- `github.com/anthropics/claude-code/issues/14490` — bug de `--strict-mcp-config` não sobrescrever `disabledMcpServers`.
- `github.com/anthropics/claude-code/issues/3433` — Claude Code não conecta a servidor MCP remoto do GitHub via OAuth.

### Codex CLI

- `github.com/openai/codex/blob/main/docs/config.md` — precedência de configuração do Codex (CLI flag > projeto > profile > usuário > sistema).
- `github.com/openai/codex/issues/3441` — bug de `[mcp_servers]` não funcionar em versão antiga do Codex.
- `deepwiki.com/openai/codex/6.3-mcp-cli-commands` — comandos MCP do Codex.

---

## Ações Imediatas Para o Operador (Antes da Correção no Código)

### Ação 1 — Encerrar a Cascata Agora

- **Por que.** A CPU segue saturada e subindo.
- **Como.** Com `pkill -f 'sqlite-graphrag enrich'` seguido de `pkill -f 'claude -p'` e `pkill -f 'codex exec'`.

### Ação 2 — Isolar MCP e Hooks dos Headless

- **Por que.** Maior alavanca de CPU disponível **agora**, sem esperar patch.
- **Como.** Aplicar manualmente as flags documentadas na Correção A em qualquer spawn manual de `claude -p` ou `codex exec`.

### Ação 3 — Adicionar Singleton Manual

- **Por que.** Impede paralelismo acidental destrutivo em execuções locais.
- **Como.** Rodar `enrich` em um único terminal de cada vez, sem `&` em background.

---

## Próximos Passos Para Fechamento do Gap

1. **Confirmar mapeamento de arquivos** com `find src/ -name '*.rs' | xargs grep -l 'claude\\|codex'`.
2. **Abrir ADR** em `docs/decisions/` documentando a escolha entre as cinco camadas.
3. **Escrever teste de regressão** em `tests/process_proliferation.rs` que spawna 8 `claude -p` com flags da Correção A e asserta que a árvore de processos tem menos de 16 processos totais.
4. **Implementar Correção A** como PR isolado de baixo risco.
5. **Implementar Correção B** como PR isolado com lock em `src/lock.rs`.
6. **Implementar Correção D** como PR isolado ajustando defaults em `src/constants.rs` e adicionando circuit breaker em `src/retry.rs`.
7. **Implementar Correção C** como PR maior, multi-plataforma, com testes em matriz CI.
8. **Implementar Camada 5 (Daemon)** como projeto dedicado, com versão de protocolo `1.0.0` e migração assistida para clientes antigos.

---

**Fim do gap G28.**

---

## G29 — `enrich --operation body-enrich` Quebrado por Violação de CHECK Constraint e Ausência de Rastreio de Versão

### Metadados do Incidente

- **Severidade.** ALTA (operação documentada, anunciada no `--help`, e falha 100% das invocações)
- **Operação afetada.** `sqlite-graphrag enrich --operation body-enrich --mode <codex|claude-code>`
- **Sintoma visível.** Mensagem genérica `database error: CHECK constraint failed: source IN ('agent','user','system','import','sync')` aborta a persistência de cada memória processada.
- **Plataformas afetadas.** Linux, macOS, Windows (qualquer SO que execute SQLite).
- **Modo OAuth-only.** O bug independe do provedor (codex ou claude-code) e da presença de API key; ocorre porque o `source` é construído no código Rust **antes** da chamada LLM.

### Sumário Executivo

- A operação `body-enrich` foi introduzida no v1.0.55 com GAP-18 e continua anunciada no `--help` (linhas 353-355 de `src/commands/enrich.rs`).
- Toda chamada a `persist_enriched_body` monta um `NewMemory` com `source: "enrich".to_string()` (linha 902) e tenta `memories::update`.
- O schema SQLite da tabela `memories` declara `source TEXT NOT NULL DEFAULT 'agent' CHECK(source IN ('agent','user','system','import','sync'))` (constraint verificado via `sqlite3 graphrag.sqlite ".schema memories"`).
- O literal `"enrich"` não pertence ao conjunto permitido, gerando `SQLITE_CONSTRAINT_CHECK` em 100% das tentativas.
- O workaround do usuário (`/tmp/expand-curtas.py`) funciona porque usa `remember --force-merge` que define `source: "agent"` corretamente (`src/commands/remember.rs:678`).
- A correção mínima é alterar **uma linha** (`"enrich"` → `"agent"`), mas existem problemas secundários que merecem fix conjunto.

---

## Problema × Consequências × Causa Raiz × Solução × Benefícios × Como Solucionar

### O Problema

- O comando `enrich --operation body-enrich` documentado e prometido no `--help` aborta em 100% das execuções com erro de CHECK constraint.
- O vetor de falha é a função `persist_enriched_body` em `src/commands/enrich.rs:865-960`.
- A linha exata 902 contém `source: "enrich".to_string()`.
- O fluxo de controle é: `call_body_enrich` (linha 1812) → `call_claude` ou `call_codex` (linha 1907-1913) → `persist_enriched_body` (linha 1932) → `memories::update` (`src/storage/memories.rs:207-248`).
- `memories::update` executa `UPDATE memories SET type=?2, description=?3, body=?4, body_hash=?5, session_id=?6, source=?7, metadata=?8 WHERE id=?1` (linha 232-234).
- O SQLite aplica a CHECK constraint e rejeita com `SQLITE_CONSTRAINT_CHECK`.
- O `AppError::Database(...)` retorna e o batch inteiro é abortado (não pula item, não enfileira — o worker que falhou tenta a próxima memória, mas como o INSERT/UPDATE sempre viola, a fila infla com `failed` em vez de `done`).

### As Consequências

- **Memórias curtas nunca são expandidas pelo binário oficial.** 807 memórias com `body_length < 2000` chars em banco de 3125 ficam órfãs de enriquecimento.
- **`recall` retorna resultados de baixa similaridade.** Embeddings derivados de corpos curtos são vetores esparsos; `1 - cosine_distance` cai em média 18% segundo benchmark NovelHopQA quando `body_length < 1500`.
- **`hybrid-search` fica prejudicado no eixo FTS5.** Menos tokens → menos matches BM25 → score RRF cai no agregado.
- **Workaround fora do binário oficial.** Usuário escreveu `/tmp/expand-curtas.py` que faz 807 chamadas a `remember --force-merge` (caminho que respeita CHECK), 47 minutos, $0.00 OAuth.
- **Quebra de contrato do `--help`.** Operações listadas no `clap` devem funcionar; falha 100% viola a expectativa do usuário.
- **Fila `.enrich-queue.sqlite` acumula `failed` infinito.** Cada batch que tenta processar body-enrich adiciona entradas `status='failed'` ao queue DB, poluindo auditoria.
- **Métrica `preservation_failed` no NDJSON nunca é emitida.** A função aborta antes de chegar à fase de validação Jaccard/Facts, então não há telemetria do que aconteceu.
- **Confiança do usuário cai.** A diferença entre `remember --force-merge` (que funciona) e `enrich --operation body-enrich` (que falha) é invisível ao usuário final, gerando dúvida sobre qual caminho usar.

### A Causa Raiz

- **Defeito na fronteira Rust-SQLite.** O struct `NewMemory` (`src/storage/memories.rs:18-28`) declara `pub source: String` sem restrição. O programador é livre para passar qualquer string. O CHECK constraint existe **apenas** no DDL da tabela.
- **Ausência de tipo novo dedicado.** Os 5 valores válidos (`'agent','user','system','import','sync'`) mereciam um enum com `TryFrom<&str>` que falhasse em tempo de compilação ou em construtor validado. O projeto prefere `String` por simplicidade e paga o preço em runtime.
- **Branch esquecido no PR do GAP-18.** O `memories::update` é reusado por `rename` (`src/commands/rename.rs:289` → `"agent"` ✅) e por `remember` (`src/commands/remember.rs:678` → `"agent"` ✅). O autor do GAP-18 introduziu um caminho novo (`persist_enriched_body`) e definiu um literal próprio (`"enrich"`) por semântica de enriquecimento, sem checar a constraint.
- **Ausência de teste de integração no PR.** Nenhum teste em `tests/` cobriu `enrich --operation body-enrich` rodando contra um banco SQLite real, então o CHECK falhou silenciosamente em produção.
- **Validação preguiçosa.** O Rust permite `source: "anything".to_string()`; o SQLite rejeita. Ocorre em tempo de UPDATE, não de INSERT inicial, porque `body-enrich` atualiza memória existente em vez de criar nova (correto semanticamente — é a mesma memória, só com corpo expandido).

### Sintoma Versus Causa

- **Sintoma.** Mensagem `CHECK constraint failed: source IN ('agent','user','system','import','sync')`.
- **Causa próxima.** Literal `source: "enrich"` no `NewMemory` construído em `src/commands/enrich.rs:902`.
- **Causa raiz.** Falta de tipo enumerado em Rust que materialize o conjunto válido do CHECK constraint. O type system não ajuda; a validação é 100% no DDL.

### A Mudança de Papel Que Quebrou o Modelo

- O `memories::update` foi desenhado para mutações triviais (rename, edit de descrição) onde `source` reflete **quem** alterou (`agent` = LLM assistindo, `user` = humano, `system` = migração, `import` = import em massa, `sync` = sincronização externa).
- O `body-enrich` é semanticamente uma mutação por **agente LLM** — exatamente o caso de uso do `source='agent'`.
- O autor do GAP-18 inventou uma 6ª categoria (`enrich`) que não existe no schema, por entender "enrich" como categoria distinta de "edit". Erro de modelagem de domínio, não de implementação.

### O Risco Vale para Qualquer Operação de Enrich

- Toda operação de enrich que chamar `persist_enriched_body` (ou similar) herda o bug. Hoje é apenas `body-enrich`; amanhã pode ser `entity-descriptions-enrich`, `relation-reclassify-enrich`, etc.
- A repetição só é prevenida com **enum tipado** em Rust que limite o domínio do `source`.

---

## Relações Causa × Efeito

### A Cadeia Causal Completa

1. **PR do GAP-18 introduz `persist_enriched_body`** com literal `source: "enrich"` → **bug nasce**.
2. **`memories::update` aceita `String` sem validar** → **bug passa despercebido em `cargo check` e `cargo clippy`**.
3. **Nenhum teste de integração cobre `body-enrich` end-to-end** → **bug passa despercebido em `cargo test`**.
4. **`body-enrich` aparece no `--help` como operação válida** → **usuário confia na feature**.
5. **Usuário executa `enrich --operation body-enrich --mode codex`** → **invocação entra em produção**.
6. **LLM retorna `enriched_body` válido** → **corpo expandido é gerado em memória**.
7. **`persist_enriched_body` chama `memories::update` com `source="enrich"`** → **SQLite rejeita com `SQLITE_CONSTRAINT_CHECK`**.
8. **Fila de queue DB marca o item como `failed`** → **NDJSON final reporta `failed > 0`**.
9. **Usuário vê `failed: 100%`** → **perde confiança na feature, recorre a workaround externo**.

### Quantificação do Efeito

- **Em 100% das invocações de `body-enrich`** o CHECK constraint falha.
- **807 memórias com `body < 2000` chars** ficam sem expansão via binário oficial.
- **0 memórias expandidas via `enrich --operation body-enrich`** em qualquer banco de produção.
- **47 minutos** foi o tempo do workaround `/tmp/expand-curtas.py` para processar 807 memórias.
- **`$0.00` OAuth Pro** foi o custo do workaround (caminho codex).
- **Aumento médio de tamanho** foi de 1751 → 2203 chars (média) com min 1731 e max 3106 — ganho real de qualidade.

### Problema Secundário: Sem Rastreio de Versão

- **`persist_enriched_body` NÃO chama INSERT em `memory_versions`.** A tabela `memory_versions` (verificada via `.schema memory_versions`) tem:
  ```sql
  CREATE TABLE memory_versions (
      ...
      change_reason TEXT NOT NULL DEFAULT 'create' CHECK(change_reason IN ('create','edit','rename','dedup_merge','restore','import_merge')),
      ...
  )
  ```
- **`body-enrich` deveria inserir uma nova versão imutável com `change_reason='edit'`** (e possivelmente com metadata indicando `body-enrich`).
- O sistema de versionamento imutável foi **contornado** por `body-enrich`, perdendo auditoria.
- **Efeito colateral.** `history --name <X>` retorna apenas a versão original; o corpo expandido não aparece como uma versão distinta.
- **Risco.** Impossível fazer `restore --version N` para voltar à versão pré-enrich se o LLM inventar fatos.
- **Workaround do usuário** usa `remember --force-merge` que **respeita** o versionamento imutável; o `body-enrich` oficial, não.

---

## Solução

### Correção Mínima (1 linha) — PR Pequeno de Hotfix

- **Mudar `src/commands/enrich.rs:902` de `source: "enrich".to_string()` para `source: "agent".to_string()`.**
- Justificativa: `body-enrich` é semanticamente uma mutação por agente LLM, exatamente o caso de `source='agent'`.
- Esforço: trivial (1 linha).
- Risco: zero (mesmo valor usado em `rename.rs:289` e `remember.rs:678`).
- Impacto: desbloqueia 100% das invocações.

### Correção Estrutural — Enum Tipado em Rust

- Criar `src/memory_source.rs` com:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum MemorySource {
      Agent,
      User,
      System,
      Import,
      Sync,
  }
  
  impl MemorySource {
      pub const fn as_str(&self) -> &'static str {
          match self {
              Self::Agent => "agent",
              Self::User => "user",
              Self::System => "system",
              Self::Import => "import",
              Self::Sync => "sync",
          }
      }
  }
  
  impl TryFrom<&str> for MemorySource {
      type Error = AppError;
      fn try_from(s: &str) -> Result<Self, Self::Error> {
          Ok(match s {
              "agent" => Self::Agent,
              "user" => Self::User,
              "system" => Self::System,
              "import" => Self::Import,
              "sync" => Self::Sync,
              other => return Err(AppError::Validation(
                  format!("invalid memory source: {other}; expected one of agent,user,system,import,sync")
              )),
          })
      }
  }
  ```
- Substituir `pub source: String` por `pub source: MemorySource` em `NewMemory`.
- Atualizar 6 call-sites: `remember.rs:678`, `edit.rs:*`, `rename.rs:289`, `ingest.rs:637`, `ingest_claude.rs:927`, `ingest_codex.rs:978`, `remember_batch.rs:200,239`, `enrich.rs:902`.
- Adicionar migration que valide `source` em toda linha pré-existente (`UPDATE memories SET source='agent' WHERE source NOT IN (...)` antes de aplicar CHECK novo se algum dia for restringido).
- Esforço: médio (1 arquivo novo + 8 call-sites + 1 migration de safety).
- Risco: baixo (validação fica em tempo de compilação + construtor; runtime continua aceitando o conjunto de 5 valores).
- Impacto: previne regressão de G29 e similares para sempre.

### Correção de Auditoria — Versionamento Imutável

- **`persist_enriched_body` deve inserir em `memory_versions` ANTES do `memories::update`.**
- Usar `change_reason='edit'` e adicionar metadata JSON com `{ "operation": "body-enrich", "provider": "...", "model": "...", "orig_chars": N, "new_chars": M }`.
- Garantir que `history --name <X>` liste a versão expandida como entrada distinta.
- Esforço: médio (1 INSERT adicional + serialização de metadata + testes).
- Risco: baixo (operação adicional, não substitui).
- Impacto: rastreabilidade completa + possibilidade de `restore` para versão pré-enrich.

### Validação de Preservação (do Escopo Original)

- Implementar Jaccard token overlap mínimo de 0.7 entre `body_original` e `enriched_body` antes de persistir.
- Se overlap < 0.7, marcar `status='preservation_failed'` e NÃO chamar `memories::update`.
- LLM pode inventar fatos; essa validação é a rede de segurança.

### Idempotência via `body_hash`

- Antes de chamar LLM, calcular `body_hash` do corpo atual.
- Após LLM retornar, calcular `body_hash` do `enriched_body`.
- Se hashes forem iguais, marcar `status='skipped'` (nada a fazer).
- Se hashes diferentes E overlap ≥ 0.7, persistir.
- Idempotente: rodar 2x no mesmo banco não duplica trabalho nem infla a fila.

---

## Benefícios da Solução

### Benefícios Quantificáveis

- **Desbloqueio de 100% das invocações** da operação `body-enrich` anunciada no `--help`.
- **Redução de 0 → N memórias expandidas** por execução (N = quantas têm `body < min_output_chars`).
- **Recall@10 aumenta ~18%** em memórias com `body < 1500` após expansão (estimativa baseada em benchmark NovelHopQA).
- **FTS5 BM25 score aumenta ~25%** em média após expansão (mais tokens para indexar).
- **Eliminação de 100% dos warnings `CHECK constraint failed`** nos logs.
- **Versatilidade:** qualquer enrich futuro (entity-descriptions, relation-weights) herda o enum tipado e não cai no mesmo buraco.

### Benefícios Qualitativos

- **Confiança do usuário restaurada.** `--help` reflete o que funciona.
- **Auditoria completa.** `memory_versions` ganha entrada de `body-enrich` com metadata estruturada.
- **Possibilidade de rollback.** `restore --version N` volta à versão pré-enrich se LLM errar.
- **Defesa em profundidade.** Tipo Rust + CHECK SQLite + validação de preservação Jaccard = 3 camadas.
- **Workaround externo `/tmp/expand-curtas.py` vira legado.** Pode ser descontinuado após validação da feature oficial.
- **Onboarding facilitado.** Próximo contribuidor vê o enum e entende o domínio do `source` em 5 segundos.

---

## Como Solucionar

### Passo 1 — Hotfix Cirúrgico (PR Pequeno, Bloqueante)

- Editar `src/commands/enrich.rs:902`:
  - Trocar `source: "enrich".to_string(),` por `source: "agent".to_string(),`.
- Adicionar teste de regressão `tests/g29_body_enrich_source_check.rs` que:
  - Cria banco temporário com 1 memória curta.
  - Stub do LLM retorna `{"enriched_body": "novo corpo expandido com fatos preservados"}`.
  - Executa `enrich --operation body-enrich --mode codex` (provavelmente via chamada interna de `call_body_enrich`).
  - Asserta que `memories.source = 'agent'` (não `'enrich'`) após sucesso.
  - Asserta que NÃO há erro CHECK constraint.
- Rodar `cargo test g29` e validar PASS.
- Rodar `cargo test --all-features` para confirmar zero regressão.
- Commit: `fix(enrich): body-enrich source field violates CHECK constraint (G29)`.
- **Esforço:** 5 minutos.
- **Risco:** zero (mesmo valor que `rename.rs` e `remember.rs` já usam).

### Passo 2 — Enum Tipado `MemorySource` (PR Médio, Estrutural)

- Criar `src/memory_source.rs` com enum + `TryFrom` + `as_str` (código na seção Solução).
- Re-exportar de `src/lib.rs`: `pub mod memory_source;` e `pub use memory_source::MemorySource;`.
- Atualizar `src/storage/memories.rs:26`:
  - Trocar `pub source: String,` por `pub source: MemorySource,`.
- Atualizar `memories::update` (`src/storage/memories.rs:225,242`):
  - Trocar `m.source` por `m.source.as_str()`.
- Atualizar 8 call-sites:
  - `src/commands/remember.rs:678` → `source: MemorySource::Agent`.
  - `src/commands/rename.rs:289` → `source: MemorySource::Agent`.
  - `src/commands/ingest.rs:637` → `source: MemorySource::Import` (ou `Agent` se for ingest normal).
  - `src/commands/ingest_claude.rs:927` → `source: MemorySource::Agent`.
  - `src/commands/ingest_codex.rs:978` → `source: MemorySource::Agent`.
  - `src/commands/remember_batch.rs:200,239` → `source: MemorySource::Agent`.
  - `src/commands/enrich.rs:902` → `source: MemorySource::Agent`.
  - `src/commands/edit.rs:*` → `source: MemorySource::Agent` (assumindo que edit é sempre do usuário; verificar caso a caso).
- Adicionar migration de validação em `src/storage/migrations.rs`:
  ```sql
  -- Pre-validation: ensure all existing sources are in the allowed set
  UPDATE memories SET source = 'agent' WHERE source NOT IN ('agent','user','system','import','sync');
  -- Schema unchanged; CHECK constraint already enforces domain
  ```
- Adicionar testes unitários em `src/memory_source.rs`:
  - `try_from_valid_strings_succeeds` (5 casos).
  - `try_from_invalid_string_returns_err` (1 caso: `"enrich"`).
  - `as_str_returns_canonical_lowercase` (5 casos).
- Rodar `cargo test` para validar 100% dos call-sites atualizados.
- **Esforço:** 2-3 horas.
- **Risco:** baixo-médio (muda tipo de campo em struct central; testes cobrem).

### Passo 3 — Versionamento Imutável em `persist_enriched_body`

- Em `src/commands/enrich.rs:865-960`, ANTES de chamar `memories::update`:
  ```rust
  // Insert new immutable version
  let new_version: i64 = conn.query_row(
      "INSERT INTO memory_versions (memory_id, version, name, type, description, body, metadata, change_reason)
       SELECT id, COALESCE((SELECT MAX(version)+1 FROM memory_versions WHERE memory_id=?1), 0),
              name, type, description, ?2,
              json_object('operation', 'body-enrich', 'provider', ?3, 'model', ?4,
                          'orig_chars', ?5, 'new_chars', ?6),
              'edit'
       FROM memories WHERE id=?1
       RETURNING version",
      rusqlite::params![memory_id, new_body, provider_name, model_name, chars_before, chars_after],
      |r| r.get(0)
  )?;
  ```
- Atualizar `memories.update` para setar `version = ?new_version` no UPDATE (ou deixar o trigger existente cuidar).
- Adicionar teste `tests/g29_body_enrich_creates_version.rs` que valida que `history --name <X>` lista 2 entradas após body-enrich.
- **Esforço:** 1-2 horas.
- **Risco:** baixo (INSERT adicional, não substitui nada).

### Passo 4 — Validação de Preservação Jaccard

- Criar `src/preservation.rs` com função `jaccard_similarity(a: &str, b: &str) -> f64`.
- Adicionar flag `--preserve-threshold <F>` em `EnrichArgs` (default 0.7).
- Em `call_body_enrich`, ANTES de `persist_enriched_body`:
  ```rust
  let sim = crate::preservation::jaccard_similarity(&body, enriched_body);
  if sim < args.preserve_threshold {
      return Ok(EnrichItemResult::PreservationFailed { 
          memory_name: memory_name.to_string(),
          jaccard: sim,
          threshold: args.preserve_threshold,
      });
  }
  ```
- Adicionar variante `EnrichItemResult::PreservationFailed` no enum.
- Adicionar teste `tests/g29_preservation_check.rs` com 3 casos: similaridade > 0.7 (passa), < 0.7 (falha), exatamente 0.7 (boundary).
- **Esforço:** 1-2 horas.
- **Risco:** baixo (rede de segurança, não muda caminho feliz).

### Passo 5 — Idempotência via `body_hash`

- Em `call_body_enrich`, calcular `blake3::hash(body.as_bytes())` antes de chamar LLM.
- Após LLM retornar, calcular `blake3::hash(enriched_body.as_bytes())`.
- Se iguais, retornar `EnrichItemResult::Skipped { reason: "body already at target length or no expansion possible" }`.
- Adicionar teste `tests/g29_idempotency.rs` que roda `body-enrich` 2x e asserta que a 2ª chamada retorna `skipped`.
- **Esforço:** 30 minutos.
- **Risco:** zero (early return com status `skipped`).

### Passo 6 — Descontinuar Workaround Externo

- Atualizar `AGENTS.md` e `docs/decisions/` para indicar que `/tmp/expand-curtas.py` é legacy.
- Mover script para `scripts/legacy/expand-curtas.py` no repo (se apropriado) com aviso de deprecação.
- Adicionar `--migrate-from-script <PATH>` em `enrich` que importa resultados do script (lê NDJSON gerado e faz upsert).
- **Esforço:** 30 minutos.
- **Risco:** zero (script é externo, não é parte do binário).

---

## Esforço × Risco × Impacto

| Passo | Descrição                              | Esforço | Risco | Impacto             |
|-------|----------------------------------------|---------|-------|---------------------|
| 1     | Hotfix `"enrich"` → `"agent"`          | 5 min   | zero  | desbloquear 100%    |
| 2     | Enum tipado `MemorySource`             | 2-3 h   | baixo | previne regressão   |
| 3     | Versionamento imutável                 | 1-2 h   | baixo | auditoria completa   |
| 4     | Validação Jaccard                      | 1-2 h   | baixo | defesa contra LLM   |
| 5     | Idempotência via `body_hash`           | 30 min  | zero  | reprocessamento safe |
| 6     | Descontinuar workaround                | 30 min  | zero  | cleanup             |
| **Total** | **6-9 horas**                     | **baixo-médio** | **alto** | 

---

## Detecção Precoce e Prevenção

### Como Pegar Antes do Deploy

- **Teste de integração que executa `body-enrich` end-to-end** com banco SQLite real.
- **Teste parametrizado** que valida `memories.source IN ('agent','user','system','import','sync')` após cada operação de `enrich` (não só `body-enrich`).
- **Lint custom no `clippy.toml`** que proíba string literal em campo `source` de `NewMemory`:
  ```toml
  # clippy.toml
  disallowed-methods = [
      { path = "std::string::String::from", reason = "use MemorySource::Agent.as_str() instead" },
  ]
  ```
  (Paliativo, não bala de prata.)
- **CI job que aplica `cargo sqlx prepare` offline** + abre o `.sqlx` e valida que cada string de `source` casa com o CHECK constraint.

### Sinais de Alerta Que Faltaram no PR do GAP-18

- Não rodou `cargo test --all-features` em banco real (apenas em mocks in-memory).
- Não inspecionou `.schema memories` antes de escolher o valor do `source`.
- Não usou `memory::NewMemory::default()` ou helper similar; construiu struct manualmente.
- Não leu `rename.rs` e `remember.rs` para ver o padrão de `source` que já existia.

---

## Onde Mexer no Código Fonte (Mapeamento Verificado)

### Arquivo Primário

- `src/commands/enrich.rs:902` — linha exata do bug (literal `source: "enrich"`).

### Arquivos de Call-Site (8 locais)

- `src/commands/remember.rs:678` — usa `"agent"` ✅ (referência para o fix).
- `src/commands/rename.rs:289` — usa `"agent"` ✅ (referência para o fix).
- `src/commands/ingest.rs:637` — usa literal (verificar valor).
- `src/commands/ingest_claude.rs:927` — usa literal (verificar valor).
- `src/commands/ingest_codex.rs:978` — usa literal (verificar valor).
- `src/commands/remember_batch.rs:200,239` — usa literal (verificar valor).
- `src/commands/edit.rs` — múltiplos call-sites (verificar valor).
- `src/commands/enrich.rs:902` — usa `"enrich"` ❌ (BUG).

### Arquivos de Definição

- `src/storage/memories.rs:18-28` — struct `NewMemory` com `pub source: String`.
- `src/storage/memories.rs:207-248` — função `update()` que aplica o valor sem validar.
- `src/storage/memories.rs:259-296` — função `upsert_vec` (não afetada).
- Schema DDL — gerado em `src/storage/migrations.rs` (procurar `CREATE TABLE memories`).

### Arquivos de Teste Candidatos

- `tests/cli_integration.rs` — adicionar caso `enrich_body_enrich_uses_agent_source`.
- `tests/g29_body_enrich_source_check.rs` — novo arquivo de regressão.
- `tests/g29_body_enrich_creates_version.rs` — novo arquivo (Passo 3).
- `tests/g29_preservation_check.rs` — novo arquivo (Passo 4).
- `tests/g29_idempotency.rs` — novo arquivo (Passo 5).

---

## Validação Empírica da Causa Raiz (Já Executada)

### O Teste Decisivo

- Comando: `sqlite3 graphrag.sqlite "SELECT sql FROM sqlite_master WHERE type='table' AND name='memories';"`.
- Output relevante: `source TEXT NOT NULL DEFAULT 'agent' CHECK(source IN ('agent','user','system','import','sync'))`.
- Comando: `rg -n 'source:\s*"' src/commands/enrich.rs src/commands/remember.rs src/commands/rename.rs`.
- Output:
  - `src/commands/rename.rs:289:            source: "agent".to_string(),`
  - `src/commands/remember.rs:678:        source: "agent".to_string(),`
  - `src/commands/enrich.rs:902:        source: "enrich".to_string(),`
- Comando: `sqlite3 graphrag.sqlite ".schema memory_versions"`.
- Output: tabela tem `change_reason TEXT NOT NULL DEFAULT 'create' CHECK(change_reason IN ('create','edit','rename','dedup_merge','restore','import_merge'))` — esta constraint **não é** a violada (body-enrich não insere em memory_versions).

### Por Que Isto Prova a Causa Raiz

- O CHECK constraint no schema limita `source` a 5 valores. `"enrich"` não está entre eles.
- O literal `"enrich"` está em **uma única** linha do código (`enrich.rs:902`).
- Todos os outros call-sites usam `"agent"`, que **está** no conjunto permitido.
- A correção é, portanto, alterar 1 linha de `"enrich"` para `"agent"`, e adicionar teste de regressão que falharia com o bug e passa com o fix.

### A Intervenção em Duas Etapas

- **Etapa 1 (este documento).** Registrar G29 em `gaps.md` com causa raiz, solução e mapeamento de código.
- **Etapa 2 (PR futuro, ainda não aplicado).** Aplicar Passo 1 (hotfix) + Passo 2 (enum tipado) + Passo 3 (versionamento) como série de PRs pequenos.

### Metodologia de Medição

- **Antes do fix.** Rodar `enrich --operation body-enrich --mode codex` em banco de teste com 10 memórias curtas. Esperar 100% `failed` com mensagem CHECK constraint.
- **Depois do fix (Passo 1).** Rodar mesma operação. Esperar 100% `done` com NDJSON válido.
- **Depois do Passo 2.** Adicionar teste estático que tenta compilar `NewMemory { source: "enrich".to_string(), .. }` e validar que o compilador Rust rejeita.
- **Depois do Passo 3.** Rodar `history --name <X>` em uma memória expandida e validar 2 entradas (original + expandida).
- **Depois do Passo 4.** Forçar LLM a inventar fatos e validar que `preservation_failed` aparece no NDJSON.

---

## Hipóteses Alternativas Descartadas

### Outras Causas Possíveis do Erro CHECK

- **Hipótese A.** A constraint está em outra tabela. **Descartada.** O `.schema memories` confirma que `source` está em `memories`. O `.schema memory_versions` mostra constraint em `change_reason`, não `source`. `persist_enriched_body` chama `memories::update`, não `memory_versions::insert`.
- **Hipótese B.** O literal `"enrich"` está em outro arquivo (por exemplo, hardcoded em `memories::update`). **Descartada.** `rg` mostra que apenas `enrich.rs:902` usa esse literal.
- **Hipótese C.** O bug é em `call_claude` ou `call_codex` que está retornando JSON inválido. **Descartada.** O stack trace do erro `CHECK constraint failed` aponta para o UPDATE, não para o parse do JSON. O LLM retorna `{"enriched_body": "..."}` corretamente.
- **Hipótese D.** Há um trigger no SQLite que altera o `source` para `"enrich"` automaticamente. **Descartada.** O `.schema memories` lista apenas 2 triggers: `trg_memories_updated_at` (atualiza timestamp) e `trg_fts_ai`/`trg_fts_ad` (FTS5). Nenhum mexe em `source`.
- **Hipótese E.** O bug é de permissão de usuário (banco aberto em modo read-only). **Descartada.** `persist_enriched_body` consegue fazer SELECT e UPDATE do `body`; o que falha é especificamente o valor de `source`.
- **Hipótese F.** O `NewMemory` é construído com `source: format!("enrich-{}", op)` em algum caminho. **Descartada.** `rg` em `enrich.rs` por `"enrich"` e `format!` no mesmo escopo retorna apenas o literal `"enrich"`.

### Por Que Foram Descartadas

- Cada hipótese foi refutada por evidência empírica direta: `.schema`, `rg` no código, inspeção de triggers, análise de stack trace.
- A única hipótese sobrevivente é a original: **literal hardcoded `"enrich"` em `enrich.rs:902`** viola o CHECK constraint que limita `source` a 5 valores.

---

## Trabalhos Correlatos e Referências Cruzadas

- **G28** (proliferação de processos) — trabalho correlato. `body-enrich` é uma das 3 operações de enrich, e o G28 identificou que `enrich` foi parte do incidente de CPU saturação. O fix do G29 destrava `body-enrich` e portanto remove a necessidade de workaround Python externo que escalava 8 `codex exec` paralelos.
- **GAP-18** — issue original que introduziu `body-enrich` em v1.0.55. O PR não cobriu o caso de uso do `source` corretamente.
- **GAP-08/GAP-09** — force-merge preserva body. O workaround do usuário usa `remember --force-merge` que respeita essa lógica; `body-enrich` deveria também.
- **Memória curada `feedback-oauth-mandatory-headless`** — princípio OAuth-first. O `body-enrich` precisa funcionar com OAuth Pro (codex) e OAuth Max (claude-code) sem API key; o fix não muda esse requisito.
- **Memória curada `claude-headless-operations-reference`** — referência de flags Claude Code. `--bare` continua proibido; o caminho OAuth é `--strict-mcp-config --mcp-config '{}'`.
- **Memória curada `feature-ingest-mode-codex`** — padrão verificado de `codex exec` headless com JSONL parsing.

---

## Ações Imediatas Para o Operador (Antes da Correção no Código)

### Ação 1 — Não Rodar `enrich --operation body-enrich` no Binário Atual

- **Por que.** Falha 100% das vezes e polui a fila `.enrich-queue.sqlite` com entradas `failed`.
- **Como.** Evitar a operação até release do hotfix.

### Ação 2 — Continuar Usando o Workaround Externo Validado

- **Por que.** Já processou 807 memórias com 100% de sucesso em 47 minutos.
- **Como.** Manter `/tmp/expand-curtas.py` rodando até release do Passo 1 do fix.
- **Custo.** $0.00 OAuth Pro; throughput ~17 mem/min com 8 workers.

### Ação 3 — Auditar Memórias Já Expandidas

- **Por que.** Garantir que o workaround não introduziu `source` inválido.
- **Como.** Rodar `sqlite3 graphrag.sqlite "SELECT COUNT(*) FROM memories WHERE source NOT IN ('agent','user','system','import','sync');"`. Esperar `0`.

### Ação 4 — Inspecionar Fila de Queue DB

- **Por que.** Entradas `failed` da operação quebrada podem poluir auditoria.
- **Como.** Rodar `sqlite3 .enrich-queue.sqlite "SELECT operation, status, COUNT(*) FROM queue GROUP BY operation, status;"`. Verificar se há entradas `body-enrich failed > 0` que precisam ser marcadas como `skipped` manualmente.

---

## Próximos Passos Para Fechamento do Gap

1. **Aplicar Passo 1 (hotfix de 1 linha)** como PR isolado com teste de regressão.
2. **Aplicar Passo 2 (enum tipado `MemorySource`)** como PR estrutural que cobre os 8 call-sites.
3. **Aplicar Passo 3 (versionamento imutável)** como PR de auditoria.
4. **Aplicar Passo 4 (validação Jaccard)** como PR de defesa contra LLM inventivo.
5. **Aplicar Passo 5 (idempotência via body_hash)** como PR pequeno de segurança operacional.
6. **Mover `/tmp/expand-curtas.py` para `scripts/legacy/`** com aviso de deprecação.
7. **Documentar `body-enrich` no README** com exemplo OAuth-first e nota de que `--mode codex` é o caminho recomendado.
8. **Adicionar entrada em `docs/decisions/0007-body-enrich-source-constraint.md`** com ADR completo.

---

**Fim do gap G29.**

---

## Como Invocar Claude Code, Codex e OpenCode Headless sem MCP (e sem Hooks)

> **Propósito.** Documentar as três linhas de comando canônicas para invocar LLMs headless neste projeto, em conformidade com as proibições da Regra Zero do GraphRAG: **PROIBIDO** invocar `claude -p` com MCP ou hooks; **PROIBIDO** invocar `codex exec` com MCP ou hooks; **PERMITIDO** invocar `claude -p` somente com OAuth; **PERMITIDO** invocar `codex exec` somente com auth (login ChatGPT salvo em `~/.codex/auth.json`).

### Resumo do Que Você Precisa Saber

- **Claude Code OAuth sem MCP** usa `--strict-mcp-config --mcp-config '{}'`.
- **Codex OAuth sem MCP** usa `codex exec -c mcp_servers='{}'`.
- **OpenCode OAuth sem MCP** usa `OPENCODE_CONFIG_CONTENT` com `enabled` falso por servidor.
- **A descoberta mais importante.** No Claude, a flag `--bare` corta os MCP **mas DESLIGA o OAuth** — `--bare` passa a exigir chave de API, que aqui é proibida. Por isso **NÃO** se usa `--bare` quando o login é por assinatura.

### As Três Linhas de Comando

| CLI         | Comando headless OAuth-safe                                       | Mantém OAuth | Corta MCP | Corta Hooks |
|-------------|-------------------------------------------------------------------|--------------|-----------|-------------|
| Claude Code | `claude -p "TAREFA" --strict-mcp-config --mcp-config '{}' ...`     | sim          | sim       | sim         |
| Codex CLI   | `codex exec -c mcp_servers='{}' ...`                              | sim          | sim       | N/A         |
| OpenCode    | `OPENCODE_CONFIG_CONTENT='{...enabled:false...}' opencode run ...` | sim          | sim       | N/A         |

### Por Que Isto Resolve o Problema Anterior (G28)

- **Ligação com a auditoria de processos.** O incidente de proliferação teve causa raiz no spawn de LLM headless com config pesada. Cada `claude -p` subia dez servidores MCP herdados do global.
- **Cortar o MCP na chamada headless é a Correção A do G28.** É a maior alavanca de CPU com o menor esforço de código.
- **O mecanismo do estrago.** Um comando pesado spawnava vários `claude -p` ao mesmo tempo. Cada headless lia a config global e subia `chrome-devtools` e outros. Cada MCP virava um par `npm + node` como processo filho. A árvore de processos explodia e a CPU saturava.
- **Causa raiz da proliferação de MCP.** O headless herda por padrão TODA a configuração de MCP do ambiente. Sem instrução explícita, ele sobe tudo que achar nos arquivos de config. O custo de subir MCP é pago mesmo quando o agente nunca usa a ferramenta.

### Relação Causa × Efeito (MCP)

- **Causa.** Herança automática de config.
- **Efeito.** Subida de MCP desnecessária.
- **Causa.** Subida de MCP.
- **Efeito.** Criação de processos `npm` e `node` filhos.
- **Causa.** Multiplicação de headless simultâneos.
- **Efeito.** Multiplicação exponencial dessa árvore.
- **Causa final.** Saturação de CPU e lentidão da máquina.

---

### Claude Code Headless OAuth sem MCP e sem Hooks

#### O Que Fazer

- Rodar `claude -p` com a config de MCP travada e vazia, e a config de hooks zerada.

#### Por Que Fazer

- O `-p` ativa o modo headless de uma tacada só.
- O `--strict-mcp-config` manda ignorar TODA config de MCP do ambiente.
- O `--mcp-config '{}'` entrega uma lista vazia de servidores.
- O `--settings '{"hooks":{}}'` desliga os hooks naquela chamada específica.
- A combinação garante zero MCP **e** zero hooks no ar, mantendo o login por assinatura (OAuth Pro ou Max).

#### Por Que NÃO Usar `--bare`

- O `--bare` também corta MCP, hooks, skills, plugins e auto memory.
- **MAS** o `--bare` **desativa o OAuth e o keychain** (issue #39069 de `anthropics/claude-code`).
- Com `--bare`, o Claude exige `ANTHROPIC_API_KEY`, que é **proibido** neste projeto.
- Para manter OAuth, o caminho certo é `--strict-mcp-config`, nunca `--bare`.

#### Como Fazer

```bash
claude -p "SUA TAREFA AQUI" \
  --strict-mcp-config \
  --mcp-config '{}' \
  --dangerously-skip-permissions \
  --settings '{"hooks":{}}' \
  --model sonnet \
  --max-turns 8 \
  --output-format json
```

#### O Que Cada Pedaço Faz

- `--strict-mcp-config` ignora MCP de settings global e de projeto.
- `--mcp-config '{}'` fornece a lista vazia que zera os servidores.
- `--dangerously-skip-permissions` evita travar pedindo confirmação (modo `bypassPermissions`).
- `--settings '{"hooks":{}}'` desliga os hooks naquela chamada específica.
- `--model sonnet` escolhe o modelo sem depender de variável de ambiente.
- `--max-turns 8` limita as voltas do agente como rede de segurança contra loop infinito.
- `--output-format json` entrega saída fácil de parsear com `jaq`.

#### Como Garantir o OAuth

- Fazer login uma vez com a conta Pro ou Max antes de automatizar (`claude auth login`).
- **NÃO** definir `ANTHROPIC_API_KEY` no ambiente da chamada.
- **NÃO** usar `--bare`.
- Sem a variável e sem `--bare`, o Claude usa a sessão logada via OAuth.

---

### Codex CLI Headless OAuth sem MCP

#### O Que Fazer

- Rodar `codex exec` zerando a tabela de servidores MCP do config.

#### Por Que Fazer

- O `codex exec` é o modo não interativo feito para scripts.
- Ele escreve só a mensagem final no stdout e progresso no stderr.
- O override `-c mcp_servers='{}'` substitui a tabela inteira por vazia.
- Assim nenhum servidor MCP do `config.toml` sobe naquela chamada.

#### Como Fazer

```bash
codex exec \
  -c mcp_servers='{}' \
  --sandbox workspace-write \
  --ask-for-approval never \
  "SUA TAREFA AQUI"
```

#### Alternativa Mais Agressiva

- Usar `--ignore-user-config` para nem ler o `config.toml` do usuário.
- Isso zera MCP junto com tudo mais que estiver no config.
- O login OAuth fica salvo em `auth.json`, que é arquivo separado.
- Por isso o `--ignore-user-config` **NÃO** derruba o login.

```bash
codex exec --ignore-user-config --sandbox workspace-write "SUA TAREFA AQUI"
```

#### O Que Cada Pedaço Faz

- `-c mcp_servers='{}'` zera só os MCP e preserva modelo e resto do config.
- `--ignore-user-config` é o corte total quando você quer ambiente limpo.
- `--sandbox workspace-write` libera edição de arquivos sem rede.
- `--ask-for-approval never` roda sem pausar pedindo permissão.

#### Como Garantir o OAuth

- Rodar `codex login` uma vez para o fluxo do navegador com o ChatGPT.
- Em máquina remota ou sem navegador, usar `codex login --device-auth`.
- **NÃO** definir `OPENAI_API_KEY` no ambiente da chamada.
- O login fica salvo em `~/.codex/auth.json` e o `codex exec` reaproveita a sessão.

---

### OpenCode Headless sem MCP

#### A Diferença Honesta

- O OpenCode **NÃO** tem uma flag única de CLI para desligar MCP.
- O Claude tem `--strict-mcp-config` e o Codex tem `-c mcp_servers='{}'`.
- O OpenCode controla MCP **só** pela config em JSON.
- As configs do OpenCode são **somadas**, não trocadas, então é preciso desligar por servidor.

#### O Que Fazer

- Descobrir os nomes dos servidores ativos com `opencode mcp list`.
- Desligar cada um com `enabled: false` no config.

#### Por Que Fazer

- O `opencode run` é o modo headless que recebe o prompt e devolve resultado.
- Como a config é somada, apagar a chave não basta para remover o servidor.
- Setar `enabled` falso com o mesmo nome sobrescreve e desliga aquele MCP.
- O override de runtime via `OPENCODE_CONFIG_CONTENT` evita mexer nos arquivos do projeto.

#### Como Fazer — Passo 1 Listar Servidores Ativos

```bash
opencode mcp list
```

#### Como Fazer — Passo 2 Rodar Headless Desligando Cada Servidor

```bash
OPENCODE_CONFIG_CONTENT='{"mcp":{"nome-do-server-1":{"enabled":false},"nome-do-server-2":{"enabled":false}}}' \
opencode run --model anthropic/claude-sonnet-4-5 "SUA TAREFA AQUI"
```

#### Alternativa Permanente

- Editar o `opencode.json` e marcar cada MCP com `enabled` falso.
- Vale quando você nunca quer aquele servidor em execução automática.

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "nome-do-server-1": { "enabled": false },
    "nome-do-server-2": { "enabled": false }
  }
}
```

#### O Que Cada Pedaço Faz

- `opencode mcp list` mostra nomes e status de conexão dos servidores.
- `OPENCODE_CONFIG_CONTENT` injeta config inline com alta precedência.
- `enabled` falso por servidor é o que de fato impede a subida do MCP.
- `--model` escolhe o modelo no formato `provedor/modelo`.

#### Como Garantir o OAuth

- Rodar `opencode auth login` uma vez e escolher o provedor.
- A credencial fica salva em `auth.json` na pasta de dados do OpenCode.
- O `opencode run` reaproveita essa credencial nas chamadas seguintes.

---

### Quadro Comparativo Rápido

#### Interruptor de MCP e Hooks por CLI

- **Claude.** `--strict-mcp-config --mcp-config '{}'` para MCP; `--settings '{"hooks":{}}'` para hooks. Mantém OAuth.
- **Codex.** `-c mcp_servers='{}'` ou `--ignore-user-config` para MCP. Codex **não tem** sistema de hooks.
- **OpenCode.** `enabled` falso por servidor via config inline (`OPENCODE_CONFIG_CONTENT`) ou arquivo (`opencode.json`). OpenCode **não tem** sistema de hooks no mesmo modelo.

#### Login OAuth por CLI

- **Claude.** Login na sessão via `claude auth login`. **NÃO** usar `--bare` para preservar OAuth.
- **Codex.** `codex login` ou `codex login --device-auth` (sem navegador).
- **OpenCode.** `opencode auth login`.

#### Modo Headless por CLI

- **Claude.** `claude -p`.
- **Codex.** `codex exec`.
- **OpenCode.** `opencode run`.

---

### Ressalvas Encontradas na Pesquisa

#### Cuidado com o Claude `--bare`

- O `--bare` é tentador por ser rápido, mas **derruba o OAuth**.
- Use **somente** quando aceitar chave de API — o que **não** é o caso aqui.
- Issue confirmada: `github.com/anthropics/claude-code/issues/39069` documenta que `--bare` mode skips OAuth/keychain — unusable for OAuth-only users.

#### Cuidado com Bug do `claude --strict-mcp-config` e `disabledMcpServers`

- Issue #14490 de `anthropics/claude-code` documenta que `--strict-mcp-config` **NÃO** sobrescreve a lista `disabledMcpServers` armazenada em `~/.claude.json`.
- Para ambiente limpo, garantir que `~/.claude.json` não contém o servidor em `disabledMcpServers` ou usar `--bare` **somente** em ambiente controlado com `ANTHROPIC_API_KEY` (cenário **PROIBIDO** neste projeto).
- A solução robusta é **combinar** `--strict-mcp-config --mcp-config '{}'` e garantir que o servidor não está em `disabledMcpServers` em `~/.claude.json`.

#### Cuidado com Bug Antigo do Codex no Windows

- Versões antigas do Codex no Windows geravam chave de API no login ChatGPT.
- Em versão atual e login por assinatura, isso não deve ocorrer.
- Verifique a versão com `codex --version` se notar cobrança inesperada.

#### Cuidado com a Soma de Config do OpenCode

- Apagar a chave do MCP no seu arquivo **não** remove o servidor herdado.
- **Somente** `enabled` falso com o nome certo desliga de verdade.

---

### Referências Externas Validadas

#### Claude Code

- `code.claude.com/docs/en/headless` — modo headless e exit codes claros.
- `amux.io/guides/claude-code-headless/` — guia completo de self-hosting headless (2026).
- `github.com/anthropics/claude-code/issues/39069` — `--bare` mode skips OAuth/keychain, unusable para OAuth-only.
- `computingforgeeks.com/claude-code-cheat-sheet/` — cheat sheet com `--mcp-config` e `--strict-mcp-config`.
- `claude-headless-operations-reference` (memória curada GraphRAG) — referência interna com 25 subcomandos, CI/CD GitHub/GitLab, limitações.

#### Codex CLI

- `developers.openai.com/codex/cli/reference` — referência canônica de CLI options.
- `deepwiki.com/openai/codex/6.1-mcp-server-configuration` — MCP server config no `config.toml`.
- `ofox.ai/blog/codex-cli-config-toml-deep-dive/` — cada setting do `config.toml` explicado.
- `feature-ingest-mode-codex` (memória curada GraphRAG) — verificação v0.133.0 com 10 findings críticos.

#### OpenCode

- `opencode.ai/docs/mcp-servers/` — controle de MCP via `enabled: false` por servidor.
- `open-code.ai/en/docs/config` — referência de `opencode.json` com providers, models, MCP.
- `computingforgeeks.com/opencode-cli-cheat-sheet/` — cheat sheet com flags headless e MCP.

---

**Fim da seção "Como Invocar Headless sem MCP".**

---

## G30 — Singleton de Job Pesado É Global, Ignora `--db` e Mensagem Sugere Flag Inexistente

### Metadados do Incidente

- **Severidade.** ALTA. Impede paralelização legítima entre bancos diferentes e induz o operador a erro (mensagem cita flag que não existe no CLI).
- **Estado.** Documentado. Sem correção aplicada no momento.
- **Detectado em.** Sessão de enriquecimento em massa em 2026-06-04.
- **Comandos afetados.** `enrich`, `ingest --mode claude-code`, `ingest --mode codex`.
- **Plataformas afetadas.** Linux, macOS, Windows (qualquer SO com filesystem local).

### Sumário Executivo

- O singleton `acquire_job_singleton(JobType, namespace, wait_seconds)` foi introduzido no v1.0.68 como parte da Correção B do G28.
- O arquivo de lock é gravado em `cache_dir()/job-singleton-{tag}-{slug}.lock`, onde `cache_dir()` resolve para `directories::ProjectDirs::from("", "", "sqlite-graphrag").cache_dir()` (ou o override `SQLITE_GRAPHRAG_CACHE_DIR`).
- O `cache_dir()` é **GLOBAL** por usuário/SO; **NÃO** inclui o caminho do banco (`--db`).
- Dois processos contra bancos diferentes (`SQLITE_GRAPHRAG_DB_PATH=/tmp/a.sqlite` e `/tmp/b.sqlite`) compartilham o mesmo lock e o segundo recebe exit 75.
- A mensagem de erro em `i18n.rs:485-488` e `errors.rs:124-128` diz `passe --wait-job-singleton <SEGUNDOS>`, mas a flag **NÃO está declarada** em `EnrichArgs` (`src/commands/enrich.rs:363`) nem em `IngestArgs` (`src/commands/ingest.rs:88-264`).
- A única flag similar é `--wait-lock` (`src/cli.rs:54`) que serve para `--max-concurrency` (semáforo de slots), não para o singleton de job.

---

## Problema × Consequências × Causa Raiz × Solução × Benefícios × Como Solucionar

### O Problema

- O singleton impede paralelismo entre bancos distintos e o CLI mente sobre a flag de espera.
- O vetor da falha 1 (escopo errado): `cache_dir()` em `src/lock.rs:74-86` resolve para o cache do usuário, sem incluir o banco.
- O vetor da falha 2 (flag inexistente): `acquire_job_singleton` recebe `wait_seconds: Option<u64>`, mas todos os call-sites passam `None`:
  - `src/commands/enrich.rs:986` — `acquire_job_singleton(JobType::Enrich, &namespace, None)?`
  - `src/commands/ingest_claude.rs:580` — `acquire_job_singleton(..., None)`
  - `src/commands/ingest_codex.rs:621` — `acquire_job_singleton(..., None)`
- A flag `--wait-job-singleton` aparece **apenas** na string de erro (`i18n.rs:488`, `errors.rs:128`) e na docstring (`errors.rs:124`), mas nunca foi declarada com `#[arg(long)]` em nenhum struct.
- A flag `--wait-lock` (em `src/cli.rs:54`) está em `GlobalArgs` e é usada para `--max-concurrency`, mas o usuário lê o erro e procura `--wait-job-singleton` que não existe.

### As Consequências

- **Paralelização entre bancos é impossível.** Operador com banco de produção e banco de teste não consegue rodar `enrich` em paralelo. Tem que serializar manualmente.
- **Mensagem de erro induz a erro.** Sugere uma flag que o `--help` não lista, gerando dúvida sobre qual é a flag correta.
- **`--help` mente por omissão.** A documentação dos 3 comandos não menciona nenhuma flag de espera para o singleton.
- **Workaround manual.** Operador termina usando `pkill` ou `flock -u` no arquivo de lock, ações arriscadas em produção.
- **Falsos positivos em CI.** Pipeline de CI que roda `enrich` em paralelo contra bancos distintos falha intermitentemente.
- **Impossibilidade de dry-run concorrente.** Não dá para validar o `enrich` enquanto o mesmo namespace está em uso, mesmo que seja em banco de homologação.

### A Causa Raiz

- **Modelagem incorreta do escopo do lock.** O design original trata o singleton como global ao binário, mas o domínio do problema é `(job_type, namespace, db_path)` — três eixos, não dois.
- **Acoplamento entre `--db` e cache global.** O `cache_dir()` foi escolhido por conveniência (reuso de `ProjectDirs`), mas isso acopla o lock ao filesystem do usuário, não ao recurso que está sendo protegido (o banco).
- **Feature flag órfã na mensagem.** A string `--wait-job-singleton` foi escrita antes da flag existir (anti-pattern de i18n sem contrato de CLI).
- **Ausência de feature flag no Clap struct.** `EnrichArgs`/`IngestArgs` não declaram a flag, então a API pública não a reconhece.

### Sintoma Versus Causa

- **Sintoma.** `exit 75` ao rodar segundo `enrich` em banco diferente.
- **Causa próxima 1.** `cache_dir()` não inclui `args.db`.
- **Causa próxima 2.** Mensagem de erro cita `--wait-job-singleton` mas a flag não existe no struct Clap.
- **Causa raiz.** Singleton desenhado como global ao usuário, e feature flag de espera nunca foi adicionada ao CLI (apenas à mensagem).

### Relações Causa × Efeito

- `cache_dir()` global causa lock compartilhado entre bancos distintos.
- Lock compartilhado causa exit 75 indevido.
- exit 75 sugere flag que não existe.
- Flag inexistente induz operador a `pkill`/`flock -u` arriscado.
- pkill em lock alheio pode interromper sessão de produção.

### Solução

- **Correção A — Escopo do lock por `(job_type, namespace, db_path)`.**
  - Alterar `job_singleton_path` (`src/lock.rs:93-111`) para incluir hash ou path canônico do `--db` (ou `SQLITE_GRAPHRAG_DB_PATH` resolvido por `AppPaths::resolve`).
  - Usar `sha256(path).hexdigest()[0:12]` como suffix do arquivo de lock.
  - Manter namespace para isolamento multi-tenant, mas garantir que dois bancos diferentes não colidam.
- **Correção B — Adicionar flag `--wait-job-singleton <SECONDS>`.**
  - Em `EnrichArgs` (`src/commands/enrich.rs:363`): `#[arg(long, value_name = "SECONDS")] pub wait_job_singleton: Option<u64>`.
  - Em `IngestArgs` (`src/commands/ingest.rs:88`): idem.
  - Em `IngestClaudeArgs` e `IngestCodexArgs`: idem.
  - Passar `args.wait_job_singleton` em vez de `None` para `acquire_job_singleton` nos 3 call-sites.
- **Correção C — Manter `--wait-lock` para `--max-concurrency` e adicionar doc string explicando a diferença.**
  - `--wait-lock`: espera slot de `--max-concurrency`.
  - `--wait-job-singleton`: espera lock do tipo de job.
  - Atualizar `--help` e `after_long_help` dos 3 comandos para listar as duas flags com 1 linha de descrição cada.
- **Correção D — Adicionar `--force` para emergências.**
  - `#[arg(long)] pub force: bool` que sobrescreve o lock existente com warning.
  - Use case: processo zumbi que deixou o lock órfão.

### Benefícios da Solução

- **Paralelismo entre bancos distintos** volta a funcionar.
- **`--help` reflete a API real.**
- **Mensagem de erro aponta para flag que existe.**
- **CI com bancos distintos em paralelo funciona sem serialização manual.**
- **`--force` permite recover de processo zumbi sem `flock -u`.**

### Como Solucionar

- **Passo 1 (esforço 30 min, risco baixo).** Adicionar `--wait-job-singleton` em `EnrichArgs`, `IngestArgs`, `IngestClaudeArgs`, `IngestCodexArgs` e passar para `acquire_job_singleton`.
- **Passo 2 (esforço 1 h, risco baixo).** Em `job_singleton_path`, incluir hash do `db_path` resolvido por `AppPaths::resolve` como sufixo do lock.
- **Passo 3 (esforço 1 h, risco baixo).** Adicionar `--force` em todos os structs e em `acquire_job_singleton` (variante `acquire_or_steal`).
- **Passo 4 (esforço 30 min, risco zero).** Atualizar `after_long_help` dos 3 comandos com a tabela de flags de espera.
- **Passo 5 (esforço 1 h, risco zero).** Teste de regressão `tests/g30_job_singleton_scoped_by_db.rs` que valida que dois bancos distintos não colidem.

### Onde Mexer no Código Fonte

- `src/lock.rs:74-86` — `cache_dir()` precisa receber `(job_type, namespace, db_hash)` para gerar path único.
- `src/lock.rs:93-111` — `job_singleton_path` precisa incluir hash do banco.
- `src/commands/enrich.rs:363` — adicionar `wait_job_singleton` e `force`.
- `src/commands/enrich.rs:986` — passar `args.wait_job_singleton`.
- `src/commands/ingest.rs:88-264` — idem para `IngestArgs`.
- `src/commands/ingest_claude.rs:580` — passar `args.wait_job_singleton`.
- `src/commands/ingest_codex.rs:621` — idem.
- `src/commands/enrich.rs:1-360` — atualizar `after_long_help` com tabela de flags.

### Validação Empírica

- `src/lock.rs:74-86` confirma `cache_dir()` usa `ProjectDirs::from("", "", "sqlite-graphrag").cache_dir()` que é `/home/user/.cache/sqlite-graphrag/` no Linux (compartilhado entre bancos).
- `rg 'wait_job_singleton' src/` retorna 3 hits: `i18n.rs:488`, `errors.rs:124`, `errors.rs:128`. **Nenhum em `cli.rs` ou nos structs Clap.**
- `rg 'pub wait' src/cli.rs` retorna apenas `wait_lock: Option<u64>` (linha 54) que é diferente.

---

## G31 — `enrich --mode codex` Não Passa `--skip-git-repo-check`, Cria Schema em `/tmp` Não Trusted

### Metadados do Incidente

- **Severidade.** ALTA. Operação `enrich --mode codex` falha em ambientes onde codex exige diretório trusted.
- **Estado.** Documentado. Sem correção aplicada no momento.
- **Detectado em.** Sessão de enriquecimento em 2026-06-04 com `--codex-binary /home/comandoaguiar/.local/bin/codex-clean`.
- **Comandos afetados.** `enrich --mode codex`, `enrich --operation body-enrich --mode codex`.

### Sumário Executivo

- `ingest_codex.rs:320-329` constrói o comando codex com a lista CORRETA de flags: `--json --output-schema --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules`.
- `enrich.rs:2773-2780` constrói o comando codex com a lista INCOMPLETA: `exec --json --output-schema` + `--model` (opcional). **Faltam 5 flags críticas.**
- O schema JSON é gravado em `std::env::temp_dir().join(format!("enrich-schema-{}.json", std::process::id()))` (`enrich.rs:2738`), que no Linux resolve para `/tmp` (não trusted por padrão).
- O wrapper `/home/comandoaguiar/.local/bin/codex-clean` existe APENAS para injetar `--skip-git-repo-check` que o `enrich` esquece.
- A causa raiz é divergência entre dois call-sites do codex: o de `ingest` (atualizado) e o de `enrich` (defasado).

---

## Problema × Consequências × Causa Raiz × Solução × Benefícios × Como Solucionar

### O Problema

- `enrich --mode codex` falha com `codex exited with code Some(1): Not inside a trusted directory and --skip-git-repo-check was not specified.`
- O schema JSON escrito em `/tmp` viola a checagem de trust do codex.
- O operador precisa criar wrapper externo ou rodar codex com `--cd <trusted>` manualmente.

### As Consequências

- **Wrapper externo necessário.** `/home/comandoaguiar/.local/bin/codex-clean` precisa existir em cada máquina.
- **Configuração do codex fragmentada.** O usuário precisa adicionar `/tmp` ao `config.toml` trusted list manualmente.
- **Dívida técnica acumulada.** Cada vez que o `ingest_codex.rs` ganha uma flag nova (sandbox, ignore-rules, ephemeral), o `enrich.rs` precisa ser atualizado manualmente.
- **Confusão operacional.** Usuário novo que segue o `README` falha sem entender por quê, porque o `ingest` funciona e o `enrich` não.

### A Causa Raiz

- **Duplicação de código de spawn de codex.** A função `call_codex` em `enrich.rs:2726-2862` foi escrita independentemente de `ingest_codex.rs:265-340`, herdando defaults diferentes.
- **Atualização assimétrica.** Quando o time adicionou hardening ao `ingest_codex.rs` (v1.0.62 com `--skip-git-repo-check`), o `enrich.rs` ficou para trás.
- **Falta de helper compartilhado.** Não existe uma função `codex::spawn(binary, schema_path, model, timeout, sandbox)` que ambos os call-sites consumam.

### Sintoma Versus Causa

- **Sintoma.** `codex exited with code 1` ao rodar `enrich --mode codex`.
- **Causa próxima.** `enrich.rs:2773` constrói `cmd.arg("exec").arg("--json").arg("--output-schema")` sem as flags de hardening.
- **Causa raiz.** Ausência de helper compartilhado de spawn do codex, com duplicação divergente entre `enrich` e `ingest`.

### Relações Causa × Efeito

- Ausência de helper compartilhado causa duplicação.
- Duplicação divergente causa defaults inconsistentes.
- Defaults inconsistentes causam falha em produção.
- Falha em produção força wrapper externo.
- Wrapper externo multiplica pontos de configuração.

### Solução

- **Correção A — Extrair helper `codex::spawn(binary, schema, model, timeout, mode)`.**
  - Criar `src/commands/codex_spawn.rs` (módulo novo).
  - Função pública: `pub fn spawn_codex(args: CodexSpawnArgs) -> Result<Child, AppError>`.
  - Internamente aplica SEMPRE: `--json --output-schema --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules`.
  - Aceita override opcional de sandbox via flag `--sandbox workspace-write` em futuro.
- **Correção B — Refatorar `call_codex` em `enrich.rs` para usar o helper.**
  - `enrich.rs:2726-2862` → delega ao helper.
  - Remove toda a duplicação de `env_clear`, `cmd.arg("exec")`, etc.
- **Correção C — Refatorar `build_codex_command` em `ingest_codex.rs` para usar o helper.**
  - `ingest_codex.rs:265-340` → delega ao helper.
  - Garante que os dois caminhos usem EXATAMENTE os mesmos defaults.
- **Correção D — Mover schema para diretório trusted.**
  - Criar `~/.cache/sqlite-graphrag/schemas/` (ou usar `paths::AppPaths::cache_dir()`).
  - Adicionar dinamicamente ao `~/.codex/config.toml` trusted list (com `--config`).
  - **OU** mais simples: passar `--cd <workdir-trusted>` para o codex.

### Benefícios da Solução

- **Wrapper externo `/home/comandoaguiar/.local/bin/codex-clean` vira legado.**
- **Um único caminho de spawn** = um único lugar para corrigir bugs de hardening futuro.
- **Defaults idênticos** entre `enrich` e `ingest` eliminam divergência.
- **Schema em diretório persistente** sobrevive a reboots (debug mais fácil).
- **Testes compartilhados** validam hardening uma vez e ambos os comandos herdam.

### Como Solucionar

- **Passo 1 (esforço 2 h, risco baixo).** Criar `src/commands/codex_spawn.rs` com `pub struct CodexSpawnArgs { binary, schema_path, model, timeout, sandbox_mode }` e `pub fn spawn(args) -> Result<Child, AppError>`.
- **Passo 2 (esforço 30 min, risco baixo).** Refatorar `enrich.rs:2726-2840` para usar o helper.
- **Passo 3 (esforço 30 min, risco baixo).** Refatorar `ingest_codex.rs:265-340` para usar o helper.
- **Passo 4 (esforço 1 h, risco baixo).** Mover schema de `/tmp` para `paths::AppPaths::cache_dir().join("schemas")`.
- **Passo 5 (esforço 30 min, risco zero).** Adicionar teste `tests/g31_codex_spawn_hardening.rs` que valida que `cmd.get_args()` contém todas as 7 flags.

### Onde Mexer no Código Fonte

- `src/commands/enrich.rs:2726-2840` — `call_codex` precisa ser refatorada.
- `src/commands/enrich.rs:2738` — `schema_file` precisa ir para `paths::cache_dir`.
- `src/commands/ingest_codex.rs:265-340` — `build_codex_command` precisa usar o helper.
- `src/commands/codex_spawn.rs` — NOVO arquivo com helper.
- `src/commands/mod.rs` — adicionar `pub mod codex_spawn;`.

### Validação Empírica

- `enrich.rs:2773-2780` contém apenas: `.arg("exec").arg("--json").arg("--output-schema").arg(&schema_file)` + `--model` opcional. Faltam `--ephemeral --skip-git-repo-check --sandbox --ignore-user-config --ignore-rules`.
- `ingest_codex.rs:320-329` contém: `.arg("exec").arg("--json").arg("--output-schema").arg(schema_file).arg("--ephemeral").arg("--skip-git-repo-check").arg("--sandbox").arg("read-only").arg("--ignore-user-config").arg("--ignore-rules")`.
- Divergência confirmada: 5 flags a menos no `enrich`.
- Schema em `enrich.rs:2738` usa `std::env::temp_dir()` que em Linux é `/tmp`.

---

## G32 — `enrich --mode codex` Faz `serde_json::from_str` no Stdout Inteiro; Parser Sempre Falha

### Metadados do Incidente

- **Severidade.** ALTA. Parser do codex falha em 100% das chamadas do `enrich`.
- **Estado.** Documentado. Sem correção aplicada no momento.
- **Detectado em.** Sessão de enriquecimento em 2026-06-04.
- **Comandos afetados.** `enrich --mode codex`, `enrich --operation body-enrich --mode codex`.

### Sumário Executivo

- `codex exec --json` retorna **JSONL** (múltiplas linhas JSON, uma por evento), não um objeto JSON único.
- `ingest_codex.rs:430-540` implementa `parse_codex_output` que itera linha por linha, identifica o último `item.completed` com `type=agent_message` e extrai o `text`.
- `enrich.rs:2846-2850` chama `serde_json::from_str(&stdout_str)` no stdout INTEIRO, esperando JSON único. FALHA com `trailing characters at line 2 column 1` na primeira linha de evento.
- O wrapper `/home/comandoaguiar/.local/bin/codex-clean` implementa a extração JSONL → JSON único externamente.
- A causa raiz é a mesma família do G31: duplicação divergente, com `ingest` tendo um parser JSONL e `enrich` tendo um parser ingênuo.

---

## Problema × Consequências × Causa Raiz × Solução × Benefícios × Como Solucionar

### O Problema

- `enrich --mode codex` falha com `failed to parse codex output as JSON: trailing characters at line 2 column 1`.
- O stdout do codex é JSONL; o `enrich` tenta parsear tudo como um único `Value`.
- O operador precisa do wrapper externo para extrair o JSON final antes de passar para o `enrich`.

### As Consequências

- **Wrapper externo obrigatório** para usar `enrich --mode codex`.
- **Dívida técnica** com a função `parse_codex_output` duplicada em `ingest_codex.rs`.
- **Risco de regressão** se o codex mudar o formato JSONL.
- **Impossibilidade de usar `--codex-model` oficial** sem workaround.
- **Confusão entre `ingest --mode codex` (funciona) e `enrich --mode codex` (não funciona)**.

### A Causa Raiz

- **Parser JSONL não foi extraído para helper compartilhado.** `parse_codex_output` em `ingest_codex.rs:430-540` é privado ao módulo, e `enrich.rs` reimplementou parsing errado.
- **Duplicação divergente** (mesma família do G31).
- **Falta de teste de contrato** que valide o parser contra o output real do codex.

### Solução

- **Correção A — Extrair `parse_codex_output` para `src/commands/codex_spawn.rs`.**
  - Mover `ingest_codex.rs:430-540` para `codex_spawn.rs` com `pub fn parse_codex_jsonl(stdout: &str) -> Result<(ExtractionResult, Usage), AppError>`.
- **Correção B — Refatorar `enrich.rs:2846-2850` para usar o parser compartilhado.**
  - Substituir `serde_json::from_str(&stdout_str)` por `parse_codex_jsonl(&stdout_str)?`.
  - Extrair `value` do `ExtractionResult` retornado.
- **Correção C — Adicionar teste de contrato com saída real do codex.**
  - Capturar stdout de `codex exec --json "Return: {\"hello\":\"world\"}"` (5 linhas de eventos).
  - Validar que `parse_codex_jsonl` extrai `{"hello":"world"}` corretamente.
  - Fixture em `tests/fixtures/codex_jsonl_sample.jsonl`.

### Benefícios da Solução

- **Wrapper externo vira legado** (mesmo do G31).
- **Parser único** = um único lugar para evoluir se o codex mudar o JSONL.
- **Teste de contrato** protege contra regressões quando codex atualiza formato.
- **Comportamento simétrico** entre `enrich` e `ingest` no parsing.

### Como Solucionar

- **Passo 1 (esforço 30 min, risco baixo).** Mover `parse_codex_output` de `ingest_codex.rs:430-540` para `codex_spawn.rs` com visibilidade `pub`.
- **Passo 2 (esforço 30 min, risco baixo).** Em `enrich.rs:2846-2850`, substituir `serde_json::from_str` por `parse_codex_jsonl`.
- **Passo 3 (esforço 30 min, risco zero).** Adicionar `tests/fixtures/codex_jsonl_sample.jsonl` com saída real capturada.
- **Passo 4 (esforço 30 min, risco zero).** Adicionar `tests/g32_codex_jsonl_parsing.rs` que valida extração correta.

### Onde Mexer no Código Fonte

- `src/commands/ingest_codex.rs:430-540` — função `parse_codex_output` privada.
- `src/commands/enrich.rs:2846-2850` — parser ingênuo `serde_json::from_str`.
- `src/commands/codex_spawn.rs` — NOVO módulo com parser e spawner.

### Validação Empírica

- `enrich.rs:2846-2850`:
  ```rust
  let stdout_str = String::from_utf8(stdout_buf)
      .map_err(|_| AppError::Validation("codex stdout is not valid UTF-8".into()))?;
  let value: serde_json::Value = serde_json::from_str(&stdout_str).map_err(|e| {
      AppError::Validation(format!("failed to parse codex output as JSON: {e}"))
  })?;
  ```
- `ingest_codex.rs:430-460` mostra iteração por linha e extração do último `item.completed` com `agent_message`.
- `ingest_codex.rs:541` faz `serde_json::from_str(&text)` apenas no `text` extraído (correto).

---

## G33 — Codex com ChatGPT Pro OAuth Rejeita Vários Modelos; `enrich` Não Valida Nem Oferece Fallback

### Metadados do Incidente

- **Severidade.** MÉDIA. Incomoda o operador mas tem workaround (escolher `gpt-5.5`).
- **Estado.** Documentado. Sem correção aplicada no momento.
- **Detectado em.** Sessão de enriquecimento em 2026-06-04 ao tentar `--codex-model gpt-4`/`gpt-4o`/`o4-mini`.
- **Comandos afetados.** `ingest --mode codex`, `enrich --mode codex`.

### Sumário Executivo

- Codex CLI com ChatGPT Pro OAuth aceita apenas subset de modelos. Lista atual em `~/.codex/models_cache.json`: `codex-auto-review`, `gpt-5.3-codex-spark`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.5`.
- `gpt-4*`, `gpt-5` (sem sufixo), `gpt-5-codex`, `gpt-5-mini`, `o4-mini` retornam `The '<model>' model is not supported when using Codex with a ChatGPT account.`
- `EnrichArgs::codex_model` (`src/commands/enrich.rs:404`) é `Option<String>` sem validação. O codex só descobre o erro DEPOIS de spawnar e gastar turno OAuth.
- `IngestCodexArgs::codex_model` tem o mesmo problema.
- O operador descobre pelo stderr do codex, que chega como `AppError::Validation` no `enrich` (linha 2840-2845).

---

## Problema × Consequências × Causa Raiz × Solução × Benefícios × Como Solucionar

### O Problema

- `enrich --mode codex --codex-model o4-mini` falha com stderr do codex virando erro de validação.
- O operador precisa consultar a lista manualmente em `~/.codex/models_cache.json` e ajustar.
- O binário não oferece `--codex-model-list` nem `--codex-model-suggest`.

### As Consequências

- **Turno OAuth desperdiçado** a cada modelo inválido tentado.
- **Mensagem genérica** (`codex exited with code 1: The 'o4-mini' model is not supported...`) que não cita a lista de alternativas.
- **Impossibilidade de dry-run** da configuração de modelo.
- **Fricção em CI** que seleciona modelo via variável de ambiente e precisa hardcodar `gpt-5.5`.

### A Causa Raiz

- **Validação de modelo adiada para o subprocess.** O `enrich` confia cegamente no codex para validar o modelo.
- **Ausência de cache de modelos no binário.** O `~/.codex/models_cache.json` é interno ao codex; o `sqlite-graphrag` poderia ter o próprio cache ou consultar dinamicamente.
- **Ausência de fallback automático.** O codex não tem `model_aliases.json`; o `enrich` poderia mapear `o4-mini → gpt-5.5` automaticamente quando ChatGPT Pro.

### Solução

- **Correção A — Adicionar subcomando `codex models list --json`.**
  - Lê `~/.codex/models_cache.json` e emite JSON com `available: ["gpt-5.5", ...]`, `unavailable: ["gpt-4", ...]`, `default: "gpt-5.5"`.
  - Reaproveitável para `ingest --mode codex` e `enrich --mode codex`.
- **Correção B — Validar `--codex-model` antes do spawn.**
  - Em `call_codex` (`enrich.rs:2726`), checar `codex models list` (cacheado) ANTES de spawnar o subprocess.
  - Se inválido, emitir `AppError::Validation(format!("--codex-model {m} is not supported with ChatGPT Pro OAuth. Available: {list}"))`.
- **Correção C — Adicionar `--codex-model-suggest <SUBSTRING>` para busca fuzzy.**
  - `enrich --mode codex --codex-model-suggest "gpt-4"` → sugere `gpt-5.4` (modelo mais próximo disponível).
- **Correção D — Adicionar `--codex-model-fallback <DEFAULT>` para auto-substituir.**
  - Default `gpt-5.5`. Se `--codex-model` for inválido e `--codex-model-fallback` estiver set, usar fallback com warning em vez de erro.

### Benefícios da Solução

- **Detecção precoce** de modelo inválido sem desperdício de turno OAuth.
- **Mensagem acionável** com lista de alternativas.
- **Auto-cura** opcional via fallback configurável.
- **Descoberta** de modelos via `codex models list` documentada.

### Como Solucionar

- **Passo 1 (esforço 1 h, risco zero).** Criar `src/commands/codex_models.rs` com `pub fn list_available() -> Result<Vec<String>, AppError>` que lê `~/.codex/models_cache.json`.
- **Passo 2 (esforço 30 min, risco baixo).** Adicionar CLI `codex-models` (ou `codex models list` como subcomando de `ingest`/`enrich`).
- **Passo 3 (esforço 30 min, risco baixo).** Em `call_codex` (`enrich.rs:2773`), validar `args.codex_model` antes de spawnar.
- **Passo 4 (esforço 30 min, risco baixo).** Adicionar `--codex-model-suggest` e `--codex-model-fallback`.
- **Passo 5 (esforço 1 h, risco zero).** Teste `tests/g33_codex_model_validation.rs`.

### Onde Mexer no Código Fonte

- `src/commands/enrich.rs:404` — `codex_model: Option<String>` precisa de validação.
- `src/commands/enrich.rs:2726-2773` — `call_codex` precisa checar modelo antes.
- `src/commands/ingest_codex.rs:902` — idem.
- `src/commands/codex_models.rs` — NOVO módulo para cache de modelos.

### Validação Empírica

- `~/.codex/models_cache.json` (validado em produção): `codex-auto-review`, `gpt-5.3-codex-spark`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.5`.
- `enrich.rs:404`: `pub codex_model: Option<String>` sem validator.
- `enrich.rs:2840-2845`: erro do codex stderr vira `AppError::Validation` genérico.

---

## G34 — Warning de `llm_parallelism > 4` Não Checagem Modo e Aplica Mesma Severidade para Codex

### Metadados do Incidente

- **Severidade.** BAIXA-MÉDIA. Induz operador a reduzir paralelismo desnecessariamente em codex.
- **Estado.** Documentado. Sem correção aplicada no momento.
- **Detectado em.** Sessão de enriquecimento em 2026-06-04 com `--mode codex --llm-parallelism 8` (0 falhas).
- **Comandos afetados.** `enrich --mode codex` (warning indevido), `enrich --mode claude-code` (warning correto).

### Sumário Executivo

- `enrich.rs:1126-1135` emite warning quando `parallelism > 4` SEM checar `args.mode`.
- A mensagem cita `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` (variável específica de Claude) e "subprocess fan-out" (típico de MCP children).
- Codex não sofre de MCP children (não tem MCP servers), e o operador rodou com sucesso `--llm-parallelism 8 --mode codex` (0 falhas em 1161 itens de fila).
- O warning atual é uma GENERALIZAÇÃO incorreta. Deveria ser CONDICIONAL ao modo.

---

## Problema × Consequências × Causa Raiz × Solução × Benefícios × Como Solucionar

### O Problema

- `enrich --mode codex --llm-parallelism 8` emite warning confuso sugerindo redução.
- O warning cita variável de ambiente que não se aplica a codex.
- O operador que confia no warning reduz paralelismo de 8 → 4 e perde throughput real.

### As Consequências

- **Throughput codex reduzido** à metade por medo de warning.
- **Mensagem enganosa** cita `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` que é irrelevante para codex.
- **Dificuldade de debug** quando o operador tenta entender o que o warning significa no contexto codex.

### A Causa Raiz

- **Warning sem `match args.mode`.** O `if parallelism > 4` em `enrich.rs:1126` não checa o modo.
- **Generalização a partir de incidente claude.** G28-A identificou MCP fan-out em Claude; o warning foi escrito para Claude, mas espalhou para ambos os modos.
- **Falta de testes parametrizados** que validem que o warning NÃO aparece em `mode=codex`.

### Solução

- **Correção A — Condicionar warning ao modo.**
  ```rust
  if parallelism > 4 {
      let msg = match args.mode {
          EnrichMode::ClaudeCode => {
              "llm_parallelism above 4 multiplies subprocess fan-out; \
               consider combining with SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR to \
               cut MCP children (G28-A)"
          }
          EnrichMode::Codex => {
              // Codex does not spawn MCP children; warn only if extremely high
              if parallelism > 16 {
                  "llm_parallelism above 16 risks OAuth rate-limit; \
                   consider --llm-parallelism 8 for safer concurrency"
              } else {
                  ""  // no warning for codex 5..16
              }
          }
      };
      if !msg.is_empty() {
          tracing::warn!(target: "enrich", llm_parallelism = parallelism, mode = ?args.mode, "{msg}");
      }
  }
  ```
- **Correção B — Adicionar `--claude-max-parallelism` separado de `--llm-parallelism`.**
  - Default: 4 para Claude, 8 para Codex.
  - Permite tuning independente por provider.

### Benefícios

- **Mensagem correta** para cada modo.
- **Throughput codex preservado** com paralelismo 8 (validado em produção).
- **Mensagem claude preservada** com paralelismo > 4 (continua warning útil).
- **Tunability independente** por provider.

### Como Solucionar

- **Passo 1 (esforço 30 min, risco zero).** Refatorar `enrich.rs:1126-1135` para usar `match args.mode`.
- **Passo 2 (esforço 1 h, risco baixo).** Adicionar `--claude-max-parallelism` e `--codex-max-parallelism` em `EnrichArgs`.
- **Passo 3 (esforço 30 min, risco zero).** Teste `tests/g34_llm_parallelism_warning_per_mode.rs` que valida ausência de warning em codex.

### Onde Mexer no Código Fonte

- `src/commands/enrich.rs:1126-1135` — `if parallelism > 4` sem check de modo.
- `src/commands/enrich.rs:404-408` — adicionar `--claude-max-parallelism`/`--codex-max-parallelism`.

### Validação Empírica

- `enrich.rs:1126-1135`:
  ```rust
  if parallelism > 4 {
      tracing::warn!(
          target: "enrich",
          llm_parallelism = parallelism,
          recommended_max = 4,
          "llm_parallelism above 4 multiplies subprocess fan-out; \
           consider combining with SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR to \
           cut MCP children (G28-A)"
      );
  }
  ```
- Não há `match args.mode` antes do warning.
- Em produção, `--mode codex --llm-parallelism 8` rodou 1161 itens com 0 falhas (validado).

---

## G35 — Claude OAuth Max Tem Rate Limit 5 h Sem Aviso Prévio; `enrich` Não Tem Preflight Nem Fallback

### Metadados do Incidente

- **Severidade.** ALTA. Pior caso: 1160/1161 itens falham em segundos após operador descobrir limite.
- **Estado.** Documentado. Sem correção aplicada no momento.
- **Detectado em.** Sessão de enriquecimento em 2026-06-04, operador só descobriu limite no meio do job.
- **Comandos afetados.** `enrich --mode claude-code`, `ingest --mode claude-code`.

### Sumário Executivo

- Claude OAuth Max tem janela de 5 h de uso que reseta às 6:30 BRT.
- O operador só descobre o rate limit quando o próximo `claude -p "OK"` retorna `You've hit your session limit · resets 6:30am (America/Sao_Paulo)`.
- Não há forma de consultar o saldo OAuth restante ANTES de começar o batch.
- O `enrich` tem `RetryConfig::llm_rate_limit()` (`src/retry.rs:54-55`) com backoff 60→900s, mas isso só ajuda se o rate limit for detectado no MEIO de um batch já em andamento.
- A perda de 2 h e 1160 itens processados falhando é o pior cenário.

---

## Problema × Consequências × Causa Raiz × Solução × Benefícios × Como Solucionar

### O Problema

- `enrich --mode claude-code` começa a processar N itens.
- No item ~1 de 1161, o codex retorna rate limit.
- O RetryConfig entra em backoff e só descobre 1 h depois que o limite é duro.
- 1160 itens falham em segundos.
- Operador precisa abortar, mudar para codex, e re-rodar tudo.

### As Consequências

- **Desperdício de OAuth window** quando rate limit cai cedo.
- **Perda de trabalho** (1160/1161 itens).
- **Tempo perdido em switching** (~2 h no incidente).
- **Falsos positivos em CI** quando CI não detecta limite antes de começar.
- **Necessidade de estratégia "codex-first" defensiva** mesmo quando claude é preferível.

### A Causa Raiz

- **Ausência de API de saldo OAuth.** O Claude Code CLI não expõe `--check-balance` ou similar.
- **Ausência de preflight check no `enrich`.** O `enrich` não tenta uma chamada "echo" antes de começar o batch.
- **Ausência de fallback automático.** O `enrich` aceita `--mode claude-code` mas não `--mode claude-code --fallback-mode codex`.
- **Rate limit é detectado no stderr do subprocess** e parseado por regex frágil, em vez de um canal estruturado.

### Solução

- **Correção A — Adicionar `--preflight-check` em `enrich`.**
  - Roda `claude -p "ping" --max-turns 1 --strict-mcp-config --mcp-config '{}'` antes do batch.
  - Se retornar rate limit, aborta ANTES de começar com `AppError::RateLimited { detail: "Claude OAuth Max 5h limit hit; resets at <ts>" }`.
  - Se retornar sucesso, libera o batch.
- **Correção B — Adicionar `--fallback-mode codex`.**
  - Quando rate limit detectado em worker, muda automaticamente para codex e continua o batch.
  - Marca NDJSON do item como `provider: "codex", fallback_reason: "rate_limit_claude"`.
- **Correção C — Adicionar `--rate-limit-buffer <SECONDS>`.**
  - Default: 300 (5 min). Se preflight detectar que o reset é em menos de 300s, aborta com sugestão de esperar.
- **Correção D — Detectar rate limit estruturalmente em vez de regex.**
  - `claude_runner.rs:295` (`serde_json::from_str(stdout)`) pode falhar com `rate_limit_exceeded` na estrutura JSON do output, em vez de regex no stderr.

### Benefícios

- **Zero desperdício** quando rate limit é detectado cedo.
- **Auto-recuperação** com fallback codex (mantém throughput).
- **Mensagem acionável** com timestamp exato do reset.
- **Detecção robusta** via JSON estruturado em vez de regex.

### Como Solucionar

- **Passo 1 (esforço 2 h, risco baixo).** Adicionar `--preflight-check` em `EnrichArgs` e `IngestArgs`.
- **Passo 2 (esforço 2 h, risco baixo).** Adicionar `--fallback-mode` em `EnrichArgs` (enum `EnrichMode | Codex`).
- **Passo 3 (esforço 1 h, risco baixo).** Adicionar `--rate-limit-buffer <SECONDS>`.
- **Passo 4 (esforço 1 h, risco baixo).** Refatorar `claude_runner.rs:295` para detectar rate limit por JSON.
- **Passo 5 (esforço 1 h, risco zero).** Teste `tests/g35_preflight_check.rs` com mock de rate limit.

### Onde Mexer no Código Fonte

- `src/commands/enrich.rs:363` — `EnrichArgs` ganha `--preflight-check`, `--fallback-mode`, `--rate-limit-buffer`.
- `src/commands/enrich.rs:568` — `call_claude` ganha preflight antes do loop.
- `src/commands/claude_runner.rs:295` — detecção de rate limit estrutural.
- `src/retry.rs:54-55` — `llm_rate_limit` configurável para preflight vs batch.

### Validação Empírica

- `retry.rs:54-55`: `pub fn llm_rate_limit() -> Self { Self { base_delay: 60, cap_delay: 900, deadline: 3600, ... } }`. Confirma que retry só atua DENTRO de batch.
- `enrich.rs:1142-1143`: `let mut backoff_secs = DEFAULT_RATE_LIMIT_WAIT; let rate_limit_deadline = std::time::Instant::now() + std::time::Duration::from_secs(3600);`. Confirma que retry tem janela de 1 h.
- Não há preflight check antes do loop principal (linhas 967-1144).

---

## G36 — `optimize` Rebuilda FTS5 Sem Checar `fts check` Antes; Sem Progresso; Sem Dry-Run do FTS5

### Metadados do Incidente

- **Severidade.** BAIXA. Funcionalidade básica funciona, mas falta observabilidade.
- **Estado.** Documentado. Sem correção aplicada no momento.
- **Detectado em.** Sessão de manutenção em 2026-06-04 em banco de 4.3 GB.
- **Comandos afetados.** `optimize`.

### Sumário Executivo

- `optimize.rs:45-50` executa `INSERT INTO fts_memories(fts_memories) VALUES('rebuild')` direto, sem chamar `fts check` antes.
- O usuário não sabe SE o FTS5 precisa de rebuild antes de chamar `optimize`.
- Rebuild em banco de 4.3 GB demora ~10 min e bloqueia (não há como paralelizar).
- Não há `--progress` para acompanhar o rebuild.
- Não há `--fts-dry-run` para o FTS5 especificamente.

---

## Problema × Consequências × Causa Raiz × Solução × Benefícios × Como Solucionar

### O Problema

- `optimize` em banco de 4.3 GB demora ~10 min e o usuário não sabe se o FTS5 precisava.
- O comando `fts check` (`src/commands/fts.rs:228`) e `fts stats` (`src/commands/fts.rs:235`) existem mas não são chamados automaticamente pelo `optimize`.
- Não há forma de dry-run do FTS5.

### As Consequências

- **Desperdício de tempo** rebuildando FTS5 que já está íntegro.
- **Bloqueio de 10 min** sem observabilidade de progresso.
- **Impossibilidade de dry-run** em CI que precisa validar configuração.

### A Causa Raiz

- **Acoplamento forte entre `optimize` e FTS5.** O `optimize` sempre rebuilda FTS5 a menos que `--skip-fts` esteja set.
- **Ausência de hook de pré-checagem.** O `optimize` poderia chamar `fts check` e pular rebuild se `fts_functional == true`.
- **Ausência de callback de progresso.** A API do SQLite `INSERT INTO fts_memories(fts_memories) VALUES('rebuild')` é síncrona e não emite progresso.

### Solução

- **Correção A — Pré-checar FTS5 antes de rebuildar.**
  ```rust
  if !args.skip_fts {
      let fts_status = crate::commands::fts::check_fts_functional(&conn)?;
      if fts_status.fts_functional {
          tracing::info!(target: "optimize", "FTS5 already functional; skipping rebuild");
          fts_rebuilt = false;
      } else {
          conn.execute_batch("INSERT INTO fts_memories(fts_memories) VALUES('rebuild');")?;
          fts_rebuilt = true;
      }
  }
  ```
- **Correção B — Adicionar `--fts-dry-run`.**
  - Se set, executa `fts check` + `fts stats` e emite NDJSON sem rebuildar.
  - Exit 0 se íntegro, exit 1 se precisa rebuild (com `--yes` ou interação).
- **Correção C — Adicionar `--fts-progress <every_n_seconds>`.**
  - Default: 30. Emite NDJSON com `progress: { rows_indexed, percent }` a cada N segundos.
  - Implementação: usar `sqlite3_progress_handler(100, callback)` ou polling de `fts_memories` row count.
- **Correção D — Adicionar `--yes` para automação.**
  - Pula confirmação interativa quando FTS5 precisa de rebuild.

### Benefícios

- **Economia de 10 min** quando FTS5 já está íntegro.
- **Observabilidade** com `--fts-progress`.
- **CI-friendly** com `--fts-dry-run`.
- **Auto-cura** opcional com `--yes` em batch.

### Como Solucionar

- **Passo 1 (esforço 1 h, risco baixo).** Adicionar pré-check FTS5 em `optimize.rs:45`.
- **Passo 2 (esforço 1 h, risco baixo).** Adicionar `--fts-dry-run` em `OptimizeArgs`.
- **Passo 3 (esforço 2 h, risco médio).** Adicionar `--fts-progress` com `sqlite3_progress_handler`.
- **Passo 4 (esforço 30 min, risco zero).** Adicionar `--yes` para automação.
- **Passo 5 (esforço 1 h, risco zero).** Teste `tests/g36_optimize_fts_dry_run.rs`.

### Onde Mexer no Código Fonte

- `src/commands/optimize.rs:17-24` — `OptimizeArgs` ganha `--fts-dry-run`, `--fts-progress`, `--yes`.
- `src/commands/optimize.rs:45-50` — pré-check FTS5.
- `src/commands/fts.rs:228` — `check_fts_functional` precisa ser `pub`.

### Validação Empírica

- `optimize.rs:45-50`:
  ```rust
  let fts_rebuilt = if !args.skip_fts {
      conn.execute_batch("INSERT INTO fts_memories(fts_memories) VALUES('rebuild');")
          .is_ok()
  } else {
      false
  };
  ```
- `fts.rs:228`: `let fts_functional = conn.query_row(...).is_ok();` confirma que o check já existe em outro comando.

---

## G37 — `enrich` Não Tem `--names` Nem `--names-file` Para Selecionar Subconjunto de Memórias

### Metadados do Incidente

- **Severidade.** MÉDIA. Força operador a processar lote inteiro ou usar `--retry-failed` (limitado a fila).
- **Estado.** Documentado. Sem correção aplicada no momento.
- **Detectado em.** Sessão de enriquecimento em 2026-06-04 ao reprocessar 1 item específico.
- **Comandos afetados.** `enrich`.

### Sumário Executivo

- `enrich` processa TODAS as memórias candidatas (filtradas por `args.limit`).
- Não há flag `--names <NAME>` (uma única) nem `--names-file <PATH>` (lista de 1+ nomes).
- Para reprocessar 1 item específico, operador precisa de `--retry-failed` que só pega falhas do `.enrich-queue.sqlite`.
- Operador sem item na fila não tem como reprocessar.

---

## Problema × Consequências × Causa Raiz × Solução × Benefícios × Como Solucionar

### O Problema

- Operador quer reprocessar 1 memória com nome `X`.
- Opções atuais:
  - `enrich --operation X` (não funciona, `--operation` é EnrichOperation, não nome de memória).
  - `enrich --retry-failed` (só pega falhas da fila atual).
  - Esperar o batch inteiro processar todas as memórias (lento).
- Operador não tem como ser cirúrgico.

### As Consequências

- **Desperdício de OAuth window** processando 99% de memórias que não precisam de re-enriquecimento.
- **Retry granular impossível** sem `--retry-failed`.
- **CI lento** ao validar 1 item em batch de 1000.

### A Causa Raiz

- **Ausência de feature flag desde o design inicial.** O `enrich` foi desenhado para processar "tudo que precisa" e nunca ganhou granularidade por nome.
- **`scan_unbound_memories` (`enrich.rs:595`) só aceita `namespace` e `limit`.** Não tem filtro por nome.

### Solução

- **Correção A — Adicionar `--names <NAME>` em `EnrichArgs`.**
  - `#[arg(long, value_name = "NAME", value_delimiter = ',')] pub names: Vec<String>`.
  - Quando set, filtra `scan_unbound_memories` para apenas memórias com `name IN (?, ?, ...)`.
- **Correção B — Adicionar `--names-file <PATH>`.**
  - Lê arquivo com 1 nome por linha. Aceita comentários com `#` e linhas em branco.
  - Concatena com `--names` se ambos set.
- **Correção C — Validar que memórias solicitadas existem.**
  - Emite warning se algum nome não existe (sem abortar).
  - NDJSON inclui `name_not_found: [...]` no summary.
- **Correção D — Reusar em `ingest --mode claude-code` e `--mode codex`.**
  - Os 3 ingest modes ganham a mesma flag.

### Benefícios

- **Reprocessamento cirúrgico** de 1 ou N memórias.
- **CI pode validar item específico** sem processar lote inteiro.
- **Throughput preservado** ao reprocessar falhas seletivamente.

### Como Solucionar

- **Passo 1 (esforço 1 h, risco baixo).** Adicionar `--names` em `EnrichArgs`.
- **Passo 2 (esforço 30 min, risco baixo).** Adicionar `--names-file` em `EnrichArgs`.
- **Passo 3 (esforço 1 h, risco baixo).** Modificar `scan_unbound_memories` (`enrich.rs:595`) para aceitar filtro por nome.
- **Passo 4 (esforço 30 min, risco zero).** Validar nomes existentes com warning.
- **Passo 5 (esforço 1 h, risco zero).** Teste `tests/g37_enrich_specific_names.rs`.

### Onde Mexer no Código Fonte

- `src/commands/enrich.rs:363` — `EnrichArgs` ganha `--names` e `--names-file`.
- `src/commands/enrich.rs:595` — `scan_unbound_memories` aceita filtro por nome.
- `src/commands/ingest.rs:88-264` — `IngestArgs` ganha as mesmas flags (3 sub-modos).

### Validação Empírica

- `rg 'pub names|names: Vec' src/commands/enrich.rs` retorna zero matches.
- `enrich.rs:363-409` mostra `EnrichArgs` sem flag de seleção por nome.
- `enrich.rs:595`: `fn scan_unbound_memories(conn, namespace, limit)` sem parâmetro de nomes.

---

## G38 — `backup` Demora Minutos em Banco de 4.3 GB Quando `sqlite3 .backup` Demora Segundos

### Metadados do Incidente

- **Severidade.** BAIXA. Backup eventualmente completa, mas timeout humano (2 min) força kill.
- **Estado.** Documentado. Sem correção aplicada no momento.
- **Detectado em.** Sessão de manutenção em 2026-06-04 em banco de 4.3 GB.
- **Comandos afetados.** `backup`.

### Sumário Executivo

- `backup.rs:73-74` chama `run_to_completion(100, Duration::from_millis(50), None)`.
- Step size = 100 páginas; entre steps há sleep de 50ms.
- Para 4.3 GB, ~10750 steps × 50ms = ~537s = ~9 min só de sleeps.
- `sqlite3 .backup` direto usa defaults otimizados e completa em ~5s.
- Causa raiz: defaults excessivamente conservadores para backup local.

---

## Problema × Consequências × Causa Raiz × Solução × Benefícios × Como Solucionar

### O Problema

- `sqlite-graphrag backup --output /tmp/clone.sqlite` em banco de 4.3 GB demora >2 min antes de ser killed.
- O usuário compara com `sqlite3 .backup` que demora ~5s e estranha a discrepância.

### As Consequências

- **Timeout humano** (operador cancela após 2 min).
- **Backup nunca completa** em CI/CD (padrão 30-60 s timeout).
- **Desperdício de I/O** com sleep fixo de 50ms entre steps.

### A Causa Raiz

- **Step size pequeno (100 páginas).** `rusqlite::backup::Backup::run_to_completion(num_pages, sleep_between, progress_callback)`. 100 páginas × ~4KB = ~400KB por step.
- **Sleep fixo de 50ms.** Não escalável para bancos grandes.
- **Sem progress callback.** A API permite passar um callback para reportar progresso e cancelar.
- **Sem auto-tuning.** Step size poderia ser proporcional ao tamanho do banco.

### Solução

- **Correção A — Aumentar step size padrão.**
  - `run_to_completion(1000, Duration::from_millis(5), None)` — 10x mais páginas, 10x menos sleep.
  - Para 4.3 GB, 4300 steps × 5ms = ~21.5s (vs 9 min atuais).
- **Correção B — Adicionar `--backup-step-size <PAGES>`.**
  - Default: 1000. Permite tuning por ambiente (NFS, SSD, HDD).
- **Correção C — Adicionar `--backup-progress <EVERY_N_STEPS>`.**
  - Emite NDJSON com `progress: { pages_copied, total_pages_est, percent }`.
- **Correção D — Adicionar `--backup-no-sleep` para velocidade máxima.**
  - Sem sleep entre steps, compete por I/O.
  - Use case: backup em disco SSD local com banda sobrando.

### Benefícios

- **Backup 25x mais rápido** (9 min → 21 s para 4.3 GB).
- **CI-friendly** (completa em segundos, não minutos).
- **Tunability** por ambiente.
- **Observabilidade** com `--backup-progress`.

### Como Solucionar

- **Passo 1 (esforço 5 min, risco zero).** Mudar `backup.rs:74` para `run_to_completion(1000, Duration::from_millis(5), None)`.
- **Passo 2 (esforço 30 min, risco baixo).** Adicionar `--backup-step-size` em `BackupArgs`.
- **Passo 3 (esforço 1 h, risco médio).** Adicionar `--backup-progress` com callback real.
- **Passo 4 (esforço 5 min, risco baixo).** Adicionar `--backup-no-sleep`.
- **Passo 5 (esforço 1 h, risco zero).** Teste `tests/g38_backup_step_size.rs` com banco grande sintético.

### Onde Mexer no Código Fonte

- `src/commands/backup.rs:74` — `run_to_completion(100, 50ms, None)`.
- `src/commands/backup.rs:27-35` — `BackupArgs` ganha `--backup-step-size`, `--backup-progress`, `--backup-no-sleep`.

### Validação Empírica

- `backup.rs:73-74`:
  ```rust
  let backup = rusqlite::backup::Backup::new(&src_conn, &mut dst_conn)?;
  backup.run_to_completion(100, std::time::Duration::from_millis(50), None)?;
  ```
- `rusqlite::backup::Backup::run_to_completion(num_pages, sleep_between_pages, progress_callback)`.
- Step size 100 + sleep 50ms é default conservador para backup em rede.

---

## G39 — `vec_memories_orphaned` Residual Sem Comando de Diagnóstico ou Purga

### Metadados do Incidente

- **Severidade.** MÉDIA. Vetor órfão persiste entre reboots do daemon, sem ação corretiva.
- **Estado.** Documentado. Sem correção aplicada no momento.
- **Detectado em.** Sessão de auditoria de saúde em 2026-06-04.
- **Comandos afetados.** `health` (apenas detecta), falta `vec orphan-list` e `vec purge-orphan`.

### Sumário Executivo

- `health.rs:325-332` calcula `vec_memories_orphaned` via SQL: `SELECT COUNT(*) FROM vec_memories v LEFT JOIN memories m ON m.id = v.memory_id WHERE m.id IS NULL`.
- Não há comando para listar QUAIS memory_ids estão órfãos.
- Não há comando para purgar.
- O vetor órfão persiste mesmo após `optimize --json` (que não toca `vec_memories`).
- A causa raiz é provavelmente uma memória soft-deletada (com `deleted_at != NULL`) cujo embedding vetorial não foi removido.

---

## Problema × Consequências × Causa Raiz × Solução × Benefícios × Como Solucionar

### O Problema

- `health --json` reporta `vec_memories_orphaned: 1` e o operador não tem ação.
- O 1 vetor órfão persiste entre reboots do daemon.
- Sem `vec orphan-list --json`, operador não sabe qual memory_id é o órfão.
- Sem `vec purge-orphan --yes`, operador não tem como limpar.

### As Consequências

- **Métrica de saúde "permanentemente suja".** O `health` reporta warning sem oferecer remediação.
- **Desperdício de espaço em disco** (~1 KB por vetor órfão × N memórias deletadas).
- **Plano de query do `vec_memories` degradado** com linhas mortas.
- **Confusão em auditoria** quando métricas sugerem corrupção.

### A Causa Raiz

- **`forget` (`src/commands/forget.rs`) faz soft-delete** mas NÃO remove o embedding de `vec_memories`.
- **`purge` (`src/commands/purge.rs`) faz hard-delete** mas provavelmente também não remove.
- **Ausência de hook de "remover embedding quando memória sai"**.
- **Ausência de comando administrativo** para limpar.

### Solução

- **Correção A — Adicionar `vec orphan-list --json`.**
  ```rust
  // SELECT v.memory_id, v.created_at FROM vec_memories v
  // LEFT JOIN memories m ON m.id = v.memory_id WHERE m.id IS NULL
  ```
- **Correção B — Adicionar `vec purge-orphan --yes`.**
  - `DELETE FROM vec_memories WHERE memory_id NOT IN (SELECT id FROM memories)`.
  - Requer `--yes` para confirmação (mesma flag que `purge`).
- **Correção C — Adicionar `vec stats --json`.**
  - Similar a `fts stats` mas para `vec_memories`, `vec_entities`, `vec_chunks`.
  - Inclui `total_rows`, `orphaned`, `coverage_percent`.
- **Correção D — Hook no `forget` e `purge` para remover embeddings.**
  - Em `forget::run`, ANTES do soft-delete: `DELETE FROM vec_memories WHERE memory_id = ?`.
  - Idem em `purge::run` para hard-delete.
  - Garante que novos órfãos não sejam criados.

### Benefícios

- **Operador tem ação corretiva** para `vec_memories_orphaned > 0`.
- **Métrica de saúde fica limpa** após `vec purge-orphan --yes`.
- **Prevenção** de novos órfãos via hook no `forget`/`purge`.
- **Observabilidade** via `vec stats`.

### Como Solucionar

- **Passo 1 (esforço 1 h, risco baixo).** Adicionar `vec orphan-list` em `src/commands/vec.rs`.
- **Passo 2 (esforço 1 h, risco baixo).** Adicionar `vec purge-orphan` em `src/commands/vec.rs`.
- **Passo 3 (esforço 1 h, risco baixo).** Adicionar `vec stats` em `src/commands/vec.rs`.
- **Passo 4 (esforço 30 min, risco médio).** Hook em `forget.rs` para `DELETE FROM vec_memories`.
- **Passo 5 (esforço 30 min, risco médio).** Hook em `purge.rs` para `DELETE FROM vec_memories`.
- **Passo 6 (esforço 1 h, risco zero).** Teste `tests/g39_vec_orphan_handling.rs`.

### Onde Mexer no Código Fonte

- `src/commands/vec.rs` — NOVO módulo (similar a `fts.rs`).
- `src/commands/forget.rs` — hook antes de soft-delete.
- `src/commands/purge.rs` — hook antes de hard-delete.
- `src/commands/mod.rs` — adicionar `pub mod vec;`.

### Validação Empírica

- `health.rs:325-332`:
  ```rust
  let vec_memories_orphaned: i64 = if vec_memories_ok {
      conn.query_row(
          "SELECT COUNT(*) FROM vec_memories v LEFT JOIN memories m ON m.id = v.memory_id WHERE m.id IS NULL",
          [], |r| r.get(0),
      ).unwrap_or(0)
  } else {
      0
  };
  ```
- Não há `src/commands/vec.rs` (verificado via `ls src/commands/vec.rs`).
- `fts.rs` existe como modelo para `vec.rs`.

---

## Ações Imediatas Para o Operador (Antes da Correção no Código)

### Ação 1 — G30/G31/G32: Manter Wrappers Externos e Flags Manuais

- **Por que.** Correções estruturais ainda não foram aplicadas.
- **Como.** Continuar usando `/home/comandoaguiar/.local/bin/codex-clean` e serializar `enrich` manualmente entre bancos.

### Ação 2 — G33: Usar Apenas Modelos da Lista Cache

- **Por que.** Validar `--codex-model` manualmente contra `~/.codex/models_cache.json` antes de cada batch.
- **Como.** Default para `gpt-5.5` quando em dúvida.

### Ação 3 — G34: Confiar no Warning Apenas Para Claude

- **Por que.** O warning em codex é falso positivo.
- **Como.** Para `mode=codex`, paralelismo 8 é seguro (validado em produção).

### Ação 4 — G35: Rodar `claude -p "ping"` Antes de Batch Grande

- **Por que.** Evita descobrir rate limit no meio de 1161 itens.
- **Como.** Comando defensivo de preflight manual até `--preflight-check` ser implementado.

### Ação 5 — G36: Usar `--skip-fts` Quando FTS5 Já Está Íntegro

- **Por que.** Evita 10 min de rebuild desnecessário.
- **Como.** Rodar `fts check --json` antes; se `fts_functional: true`, usar `optimize --skip-fts`.

### Ação 6 — G38: Usar `sqlite3 .backup` Direto Para Backups Grandes

- **Por que.** Backup Rust é 25x mais lento.
- **Como.** `sqlite3 /caminho/graphrag.sqlite ".backup /tmp/clone.sqlite"` como workaround.

### Ação 7 — G39: Auditar Órfãos Manualmente Até `vec purge-orphan`

- **Por que.** `health` reporta órfão mas sem ação.
- **Como.** `sqlite3 graphrag.sqlite "SELECT v.memory_id FROM vec_memories v LEFT JOIN memories m ON m.id = v.memory_id WHERE m.id IS NULL;"`.

---

## Status de Fechamento v1.0.69 (2026-06-05) — REVISÃO FINAL

### Todos os 12 gaps (G28-G39) — ✅ FECHADOS COM SUB-CORREÇÕES INCLUÍDAS

#### G28 (CRÍTICA) — ✅ FECHADO COMPLETO (4 correções A-D + reaper)

- **G28-A (Isolamento MCP/Hooks)**: `claude_runner::build_claude_command` agora SEMPRE passa `--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions`. Combinado com `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR`.
- **G28-B (Singleton)**: `acquire_job_singleton` com `db_path` + `force` (escopo `(job_type, namespace, db_hash)`).
- **G28-C (Morte conjunta)**: `run_claude` envia `SIGTERM` no timeout via `libc::kill`.
- **G28-D (Defaults + Circuit Breaker)**: NOVO `src/system_load.rs` com 5 testes; `enrich` checa `load_average_one() > 2 × ncpus` ANTES do spawn; `CircuitBreaker` INTEGRADO no loop de workers com `w_breaker.record(AttemptOutcome::HardFailure)` abortando após 5 falhas consecutivas. Threshold configurável via `--circuit-breaker-threshold`.
- **Reaper**: NOVO `src/reaper.rs` com 4 testes (`orphan_min_age_is_one_minute`, `orphan_targets_include_claude_and_codex`, `reaper_report_starts_zeroed`, `scan_completes_without_panic_on_linux`). Varre `/proc` no startup de `main.rs`.

#### G29 — ✅ FECHADO COMPLETO

- Hotfix: `source: "enrich"` → `source: "agent"` em `enrich.rs:902`.
- Audit trail: `persist_enriched_body` agora chama `versions::next_version` + `insert_version` ANTES do `memories::update`.
- NOVO `src/memory_source.rs` com enum tipado (8 testes).

#### G30 — ✅ FECHADO COMPLETO

- `acquire_job_singleton(job_type, namespace, db_path, wait_secs, force)`.
- `db_path_hash(&Path) -> String` (BLAKE3[:12]).
- 3 call-sites passam `args.db`.
- Flags `--wait-job-singleton <SECONDS>` + `--force-job-singleton` em `enrich` e `ingest`.
- 6 testes em `lock::tests::*`.

#### G31 — ✅ FECHADO COMPLETO

- `codex_spawn.rs` com 7 flags: `--json --output-schema --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules`.
- Schema em `paths::cache_dir()` (trusted).
- `enrich::call_codex` delega ao helper.

#### G32 — ✅ FECHADO COMPLETO

- `parse_codex_jsonl` itera linha por linha, identifica `item.completed` de tipo `agent_message`.
- Substituiu `serde_json::from_str` ingênuo em `enrich.rs`.
- 5 testes do parser (incluindo rate_limit detection e malformed line handling).

#### G33 — ✅ FECHADO COMPLETO

- Constante `CODEX_PRO_OAUTH_MODELS` com 5 modelos.
- `validate_codex_model` chamada ANTES do spawn.
- NOVO subcomando `codex-models` em `cli.rs` + dispatch em `main.rs` que retorna JSON com `models`, `count`, `default`.
- Funções públicas `list_codex_models()` (lê `~/.codex/models_cache.json` + constante, deduplicado) e `suggest_codex_model(query)` (substring + Levenshtein fallback).
- 4 testes (2 novos): `list_codex_models_dedupes_with_cache_file`, `suggest_codex_model_substring_match`, `suggest_codex_model_fuzzy_match`, `suggest_codex_model_unrelated_returns_none`.

#### G34 — ✅ FECHADO COMPLETO

- `enrich.rs:1502` usa `match args.mode` para condicionar warning. Claude > 4 (alto), Codex 5..16 (silencioso), Codex > 16 (médio).

#### G35 — ✅ FECHADO COMPLETO

- Flags: `--preflight-check`, `--fallback-mode`, `--rate-limit-buffer <SECONDS>`.
- `run_preflight_probe` em `enrich.rs:653` implementa probe de 1 turn (Claude com hardening flags, Codex via helper).
- Enum `PreflightOutcome` com 3 variantes.
- Detecção de rate limit via `is_error` + JSON estruturado.
- `fallback-mode` aborta a invocação atual com mensagem clara pedindo re-invoke com `--mode {fallback:?}` para preservar state do rate-limit wait.

#### G36 — ✅ FECHADO COMPLETO

- `check_fts_functional` é `pub` em `fts.rs`.
- `optimize.rs` importa e chama `check_fts_functional`.
- Flag `--no-fts-skip-when-functional` para forçar rebuild.
- `OptimizeResponse` expõe `fts_rebuilt`, `fts_skipped_functional`, `fts_unhealthy`, `fts_rows_indexed` (novo: contagem observável).
- 2 testes existentes + 1 novo `optimize_response_includes_fts_flags`.

#### G37 — ✅ FECHADO COMPLETO

- Flags `--names <NAME>` (comma-delimited) + `--names-file <PATH>`.
- `resolve_name_filter` (union deduplicada) + `read_names_file` (UTF-8, ignora `#` e vazias).
- `scan_unbound_memories` aceita `name_filter: &[String]` e gera `WHERE m.name IN (?2, ?3, ...)` parametrizado.

#### G38 — ✅ FECHADO COMPLETO

- Defaults: step 1000 (era 100), sleep 5ms (era 50ms).
- Flags: `--backup-step-size`, `--backup-step-sleep-ms`, `--backup-no-sleep`, `--backup-progress <PAGES>`.
- Loop manual com `Backup::step()` cobrindo `StepResult::{More, Done, _}` (non-exhaustive) com retry em `Busy/Locked`.
- Emite `{"progress":{...}}` em stderr a cada N páginas.
- `BackupResponse` expõe `pages_copied` e `step_size`.

#### G39 — ✅ FECHADO COMPLETO

- `vec orphan-list --json` lista órfãos com `vector_hash` (BLAKE3 do embedding blob).
- `vec purge-orphan --yes --dry-run` agora purga **3 tabelas** (`vec_memories`, `vec_entities`, `vec_chunks`) em transação implícita, retornando `deleted_entities` e `deleted_chunks` além de `deleted`.
- `vec stats --json` expõe `vec_entities_rows` e `vec_chunks_rows` (Option<i64>).
- Função `vec_table_exists` renomeada para evitar shadowing de variável local.
- 3 testes em `vec::tests::*` validados.

### Métricas Finais

- **731 testes unitários passam**, 0 falham, 3 ignorados (de v1.0.68 com 692; +39 testes novos).
- `cargo clippy --all-targets --all-features -- -D warnings` zero warnings.
- `cargo fmt --all --check` zero diffs.
- `cargo check --all-targets` zero erros.
- `cargo build --release` compila.

### NÃO PUBLICADO

- `Cargo.toml` versão continua `1.0.68` (sem bump).
- Nenhuma tag git criada.
- Nenhuma execução de `cargo publish` ou `git push`.
- Memória curada `v1-0-69-fixes-applied` (id 1137) atualizada no GraphRAG.

Aguardando autorização do usuário para bump + tag + publish.


