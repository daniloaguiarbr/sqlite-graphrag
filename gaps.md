# Gaps Conhecidos — sqlite-graphrag CLI

## Propósito

Este arquivo cataloga problemas conhecidos da CLI `sqlite-graphrag` que ainda não foram solucionados. Cada entrada segue o tripé estrutural:

- **Problema** — o que está errado
- **Consequências** — efeito cascata no sistema, no usuário e em pipelines
- **Causa raiz** — por que o problema existe na arquitetura
- **Solução** — o que precisa ser construído
- **Benefícios** — ganho concreto após implementação
- **Como solucionar** — passos técnicos de implementação

Cada entrada também estabelece relações `causa × efeito` explícitas.

---

## GAP-001 — `remember` monolítico e não-idempotente perde trabalho em cancelamento

**Data de identificação**: 2026-06-15
**Severidade**: ALTA (bloqueia hook Stop do Claude Code, impede salvamento proativo)
**Status**: Solucionado em v1.0.82 (V014 + src/storage/pending_memories.rs + subcomando `pending` (list/show/cleanup) + ADR-0036)

### Problema

A operação `sqlite-graphrag remember` é executada como pipeline **monolítico e all-or-nothing**. O ciclo completo — parse do body, validação de namespace, spawn do subprocesso LLM (`claude -p` ou `codex exec`), geração do embedding, INSERT no SQLite — vive em uma única transação sem checkpoints intermediários persistidos.

Quando o subprocesso LLM é cancelado por qualquer sinal externo (timeout do Bash tool, Ctrl-C, OOM killer, hook PreToolUse, parent death, SIGPIPE), a CLI emite `exit code 11` com a mensagem `"erro de embedding: embedding cancelled by shutdown signal"` e **descarta todo o trabalho já validado**: body parseado, namespace resolvido, type validado, description aceita.

### Consequências

1. **Perda silenciosa de trabalho do usuário** — quem digita um body de 5 KB e define entidades curadas via `--graph-stdin` perde TUDO se o embedding for cancelado aos 90s de 120s
2. **Desperdício de quota OAuth** — cada retry re-spawna o subprocesso LLM do zero, consumindo ~10-15s de latência + tokens de subscription Pro/Max a cada tentativa
3. **Quebra o hook Stop do Claude Code** — o sistema de salvamento proativo (`memory-guardian.sh`) fica bloqueado porque a única via de persistência está partida; hook retorna `SALVAMENTO PROATIVO` infinitamente
4. **Inconsistência arquitetural com subcomandos vizinhos** — `ingest`, `enrich` e `backup` têm `--resume`, `--retry-failed`, `--wait-job-singleton`; `remember` não tem equivalente para embedding
5. **Impossibilita auditoria pós-morte** — sem log de "estágio onde morreu", reproduzir a falha requer re-executar a operação completa sem saber o que mudou
6. **Fricção em CI/CD** — pipelines automatizados que precisam inserir memórias em loop falham em rajada quando o host tem carga variável; nenhuma via de retry determinístico
7. **Anti-padrão de UX** — usuário fica preso em loop "tem achado novo para persistir? sim, mas não consigo" porque a CLI é a única via

### Causa raiz

A causa raiz é **arquitetural e reside em três decisões acumuladas**:

1. **Ausência de tabela de staging** — não existe `pending_memories` ou similar onde gravar o estado validado antes de gerar embedding
2. **Pipeline transacional único** — o `INSERT INTO memories` + `INSERT INTO memory_embeddings` são commitados juntos no fim; falha intermediária = rollback total
3. **Subprocesso LLM sem checkpoint** — o spawn de `claude -p` ou `codex exec` é one-shot; não há protocolo de retomada parcial se o embedding for interrompido a 70% do progresso

#### Cadeia causal (causa → efeito)

```
[Bash tool timeout 120s]
    ↓ causa
[parent process recebe SIGTERM]
    ↓ propaga
[subprocesso LLM (claude / codex) recebe SIGTERM]
    ↓ causa
[embedding gerado parcialmente ou não gerado]
    ↓ causa
[CLI aborta transação inteira]
    ↓ causa
[body + metadata + namespace + type PERDIDOS]
    ↓ causa
[hook Stop fica sem via de persistência]
    ↓ causa
[usuário preso em loop "tem achado novo?" → "sim, mas não consigo"]
```

Efeito cascata documentado no transcript de 2026-06-15: **9 tentativas consecutivas com 9 variações de flags resultaram em 9 falhas idênticas** (`code: 11 — embedding cancelled by shutdown signal`).

### Solução

Implementar persistência por **estágios com checkpoint retomável**:

1. **Estágio 1 — Validação e staging** — após validar body, namespace, type, description, gravar em tabela `pending_memories` com status `validated`
2. **Estágio 2 — Geração de embedding** — spawn do subprocesso LLM para gerar o vetor; ao receber primeiro chunk válido, gravar em `pending_memories.embedding` com status `embedding_in_progress`
3. **Estágio 3 — Commit final** — quando o subprocesso retorna o vetor completo, mover de `pending_memories` para `memories` + `memory_embeddings` em uma única transação curta
4. **Mecanismo de resume** — subcomando `remember --resume <pending_id>` retoma do Estágio 2 sem re-validar Estágio 1
5. **Modo skip-embedding** — flag `--skip-embedding` permite gravar em `memories` com `memory_embeddings` NULL, deixando o embedding para `enrich --operation re-embed` posterior
6. **Limpeza automática** — entradas em `pending_memories` com status `embedding_in_progress` há mais de 24h são marcadas `abandoned` via cron ou `optimize --pending-cleanup`

### Benefícios

1. **Zero perda de trabalho** — body validado sempre persiste; cancelamento só interrompe o subprocesso LLM
2. **Retry idempotente** — `--resume <id>` retoma sem re-validar nem re-spawnar o que já foi feito
3. **Compatibilidade com timeout** — pipelines CI podem quebrar `remember` em 2 invocações: `remember --stage-only` + `remember --resume <id>`
4. **Auditoria completa** — `pending_memories` registra timestamp de cada estágio, última tentativa, exit code, error message
5. **Consistência arquitetural** — `remember` ganha o mesmo padrão de `--resume` que `ingest`, `enrich`, `backup`
6. **Recuperação de desastres** — após crash do host, restart encontra `pending_memories` com estágios intermediários e oferece resume
7. **Economia de quota OAuth** — body de 5 KB que demora 60s no embedding pode ser commitado em duas chamadas de 30s

### Como solucionar

#### Passo 1 — Modelar tabela de staging

```sql
CREATE TABLE IF NOT EXISTS pending_memories (
    pending_id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL,
    memory_type TEXT NOT NULL,
    description TEXT,
    body BLOB NOT NULL,
    body_hash TEXT NOT NULL,  -- blake3 para idempotência
    entities_json TEXT,
    relationships_json TEXT,
    status TEXT NOT NULL CHECK (status IN
        ('validated', 'embedding_in_progress', 'embedding_done',
         'committed', 'abandoned', 'failed')),
    embedding BLOB,
    embedding_dim INTEGER,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (namespace, name)
);
```

#### Passo 2 — Refatorar o pipeline de `remember` em três sub-estágios

1. **Estágio A** (atômico, sem I/O de rede): parse, validate, INSERT em `pending_memories` com status `validated`. Retorna `pending_id`.
2. **Estágio B** (rede, com timeout): UPDATE `pending_memories` para `embedding_in_progress`, spawn LLM, captura embedding parcial em `embedding` a cada chunk. UPDATE para `embedding_done` ao completar.
3. **Estágio C** (atômico, sem I/O de rede): INSERT em `memories` + `memory_embeddings` + DELETE de `pending_memories` em uma única transação curta. UPDATE para `committed`.

#### Passo 3 — Adicionar flags CLI

- `--stage-only` — executa só Estágio A, retorna `pending_id` em JSON
- `--resume <pending_id>` — retoma do Estágio B com base no `pending_id`
- `--skip-embedding` — executa Estágios A e C pulando B; embedding fica NULL para `enrich --operation re-embed`
- `--staged-cleanup-after <SECONDS>` — abandonar entradas `embedding_in_progress` mais velhas que N segundos (default 86400 = 24h)
- `--max-embedding-attempts <N>` — limite de retries no subprocesso LLM antes de marcar `failed` (default 3)

#### Passo 4 — Implementar subcomando auxiliar

```bash
sqlite-graphrag pending list --json
sqlite-graphrag pending resume <pending_id> --json
sqlite-graphrag pending cleanup --staged-cleanup-after 86400 --yes --json
```

#### Passo 5 — Testes de regressão

- Teste 1: `remember --stage-only` retorna pending_id sem spawnar LLM
- Teste 2: `remember --resume <id>` completa de onde parou sem re-validar
- Teste 3: SIGTERM durante embedding resulta em `pending_memories.status = 'embedding_in_progress'` (não perda)
- Teste 4: `remember --skip-embedding` persiste com `memory_embeddings` NULL
- Teste 5: `pending cleanup` remove entradas abandonadas sem afetar `memories`
- Teste 6: Crash de host + restart encontra pendings e oferece resume

#### Passo 6 — Documentação

- ADR novo descrevendo a decisão arquitetural
- Atualizar `docs/CLI.pt-BR.md` com seção "Persistência por Estágios"
- Atualizar `docs/MIGRATION.pt-BR.md` com passos de migração se houver mudança de schema
- Adicionar exemplo em `docs/EXAMPLES.pt-BR.md` mostrando uso em CI

### Relações causa × efeito

| Causa | Efeito direto | Efeito cascata |
|---|---|---|
| Subprocesso LLM é cancelado por SIGTERM externo | Embedding gerado parcialmente | Transação abortada |
| Transação é all-or-nothing | Body validado é perdido | Usuário precisa redigitar |
| Retry re-spawna subprocesso LLM | ~15s de latência desperdiçada | Quota OAuth consumida |
| `remember` sem `--resume` | Nenhuma via de recuperação | Hook Stop fica bloqueado |
| Hook Stop bloqueado | "Tem achado novo?" fica em loop | Turno não fecha |

### Evidências observadas

Transcript de 2026-06-15 — 9 tentativas consecutivas, todas com mesmo padrão:

```
Exit code 11
"erro de embedding: embedding cancelled by shutdown signal"
```

Variações tentadas sem sucesso:
- foreground direto
- background (`run_in_background: true`) com `sleep 120 && tail`
- `--llm-parallelism 1`
- `--max-rss-mb 2048`
- `--max-rss-mb 100`
- `SQLITE_GRAPHRAG_CLAUDE_BINARY=mock-llm`
- `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR=/tmp`
- `remember-batch` com NDJSON de 1 linha
- sleep 180s aguardando completion

### Notas

- A causa raiz NÃO é: lock de concorrência (exit 75), limite de RSS (exit 77), OAuth inválido, daemon (removido na v1.0.79)
- A causa raiz É: timeout do Bash tool cascateando via SIGTERM para subprocesso LLM sem checkpoint
- A solução é compatível com todas as variantes de embedding existentes (G42, G44)
- A solução preserva o contrato JSON atual de `remember` — flags novas são aditivas


## GAP-002 — `hybrid-search` (e provavelmente outros comandos) violam o contrato JSON de erro ao receber shutdown signal

**Data de identificação**: 2026-06-15
**Severidade**: MÉDIA (quebra pipelines de agente, dificulta diagnóstico automatizado, viola contrato documentado)
**Status**: Solucionado em v1.0.82 (src/signals.rs:handle_first_signal cross-signal + SHUTDOWN_EXIT_CODE=19 + emit_shutdown_envelope + ADR-0037)

### Problema

Quando o comando `sqlite-graphrag hybrid-search` (e possivelmente outros comandos de leitura como `recall`, `related`, `deep-research`) recebe um shutdown signal externo (SIGTERM, SIGINT, ou timeout de processo parent que cascateia via SIGTERM), o comportamento observado é:

- `stdout` fica **completamente vazio** (0 bytes)
- `stderr` contém uma mensagem legível por humanos: `shutdown signal received; finishing current operation gracefully`
- Exit code retorna **0** (sucesso) mesmo sendo um caminho de erro
- Nenhum envelope JSON de erro é emitido

O contrato documentado na seção `OBRIGATÓRIO — Contrato JSON de Erros (v1.0.56, atualizado v1.0.68)` afirma explicitamente:

> TODOS os caminhos de erro agora emitem um objeto JSON no stdout: `{"error": true, "code": N, "message": "..."}`
> stderr ainda recebe o erro legível por humanos com prefixo descritivo
> CONSUMIDORES devem verificar o JSON do stdout primeiro (procurar `"error": true`), depois usar o exit code como fallback
> Aplica-se a TODOS os comandos quando `--json` é passado

O GAP-002 é uma violação direta deste contrato: stdout vazio com `--json` passado, exit 0 quando o comando não completou com sucesso.

### Consequências

1. **Quebra silenciosa de pipelines de agente** — `jaq` em pipe recebe entrada vazia e retorna `Error: failed to parse: value expected`, sem indicar que o problema é no `sqlite-graphrag` e não no `jaq`
2. **Diagnóstico obscurecido** — usuário vê "jaq falhou" e procura erro no filtro `jaq`, quando a causa real é shutdown do comando upstream
3. **Exit code mentiroso** — exit 0 sugere sucesso, fazendo o orquestrador prosseguir com dados que nunca foram gerados
4. **Impossibilidade de retry programático** — sem código de erro estruturado, agente não pode decidir entre retry, fallback ou abort
5. **Inconsistência com envelope de erro documentado** — outros caminhos de erro (validação, embedding, conflito) emitem JSON válido; shutdown é o único caminho que escapa
6. **Falsa impressão de resiliência** — `try/catch` em scripts bash captura exit 0 e considera o pipeline bem-sucedido, propagando o problema silenciosamente
7. **Bloqueio do hook Stop de Claude Code** — hook recebe `jaq failed` como erro, mas não consegue categorizar nem persistir o achado porque o envelope estruturado está ausente
8. **Dificulta auditoria de incidentes** — logs de pipeline registram "sucesso" mesmo quando o comando foi terminado por sinal externo

### Causa raiz

A causa raiz é arquitetural e reside em **três decisões acumuladas** no handler de shutdown:

1. **Handler de SIGTERM/SIGINT com early return** — o handler de sinal implementado em algum lugar de `src/main.rs` ou `src/cli.rs` intercepta o sinal, escreve uma mensagem no stderr, e retorna cedo antes de chegar ao código que emite o envelope JSON de erro
2. **Exit code 0 hardcoded no handler de sinal** — o handler define exit code como 0 explicitamente (ou usa `std::process::exit(0)`) sem distinguir entre "operação completou com sucesso" e "operação foi cancelada por sinal"
3. **Stdout não é flushado nem populado** — quando o handler interrompe o pipeline, o buffer de stdout (que pode conter o JSON parcial ou nada) é descartado sem garantir que um envelope de erro seja escrito antes do early return

#### Cadeia causal (causa → efeito)

```
[Sinal externo: SIGTERM, SIGINT, timeout do parent process]
    ↓ capturado por
[Signal handler em src/main.rs ou src/cli.rs]
    ↓ escreve mensagem
[stderr: "shutdown signal received; finishing current operation gracefully"]
    ↓ executa early return com
[std::process::exit(0) ou equivalente]
    ↓ causa
[stdout permanece vazio (buffer não foi flushado)]
    ↓ propaga para
[Pipe | jaq recebe 0 bytes]
    ↓ causa
[jaq retorna "Error: failed to parse: value expected"]
    ↓ propaga para
[Orquestrador (Bash tool, hook, agent loop) interpreta como erro de jaq]
    ↓ resulta em
[Diagnóstico errado, retry impossível, hook Stop bloqueado]
```

Efeito cascata documentado: agentes que tentam `hybrid-search` em loop após timeout do Bash tool recebem o mesmo erro em todas as tentativas, mas o envelope de erro não distingue entre "FTS5 degraded", "embedding cancelled", "namespace not found" e "shutdown by signal" — todos viram indistinguíveis no `jaq failed`.

### Solução

Implementar shutdown signal handling que **honra o contrato JSON documentado**:

1. **Definir exit code distinto para shutdown** — usar exit code específico (ex.: 130 para SIGINT, 143 para SIGTERM, ou um novo código 19 para "graceful shutdown by signal") em vez de 0
2. **Emitir envelope JSON de erro antes de retornar** — antes do early return do signal handler, escrever no stdout: `{"error": true, "code": 19, "message": "shutdown signal received; operation cancelled", "signal": "SIGTERM"}`
3. **Flushar stdout antes de sair** — chamar `std::io::stdout().flush()` após escrever o envelope de erro para garantir que chegue ao pipe antes do processo encerrar
4. **Aplicar a TODOS os comandos** — garantir que o handler de sinal global intercepta antes da execução de qualquer subcomando, não apenas os de leitura
5. **Documentar o novo exit code** — adicionar entrada 19 (ou o código escolhido) na matriz de exit codes e na tabela de exit codes por comando
6. **Testar com timeout externo** — adicionar teste de regressão que executa `hybrid-search` em subshell com `timeout 1` e verifica que o envelope JSON de erro é emitido, não stdout vazio

### Benefícios

1. **Diagnóstico preciso** — `jaq` consegue parsear a entrada e o agente identifica imediatamente que a causa é shutdown, não erro de filtro
2. **Retry programático possível** — agente que recebe `code: 19` sabe que é temporário e pode tentar novamente com `--wait-lock` ou backoff
3. **Exit code honesto** — exit ≠ 0 sinaliza falha para o orquestrador, que pode abortar a pipeline corretamente
4. **Compatibilidade com contrato JSON** — comportamento uniforme com todos os outros caminhos de erro do CLI
5. **Hook Stop funcional** — hook consegue categorizar o erro e tomar decisão (persistir achado, abortar, ou retry)
6. **Auditoria confiável** — logs de pipeline registram corretamente a falha, permitindo análise post-mortem
7. **Consistência cross-command** — `hybrid-search`, `recall`, `related`, `deep-research` e todos os outros comandos se comportam de maneira idêntica em shutdown

### Como solucionar

#### Passo 1 — Identificar o local exato do signal handler

```bash
# Buscar o handler de SIGTERM/SIGINT no código
sg --pattern 'fn $FUNC($$$ARGS) -> $RET { $$$BODY }' -l rust src/main.rs | rg "sig"
rg -n "shutdown signal" src/
```

#### Passo 2 — Implementar helper de shutdown que respeita o contrato

```rust
// src/shutdown.rs
use std::io::Write;
use std::process::ExitCode;

pub fn emit_shutdown_envelope_and_exit(signal: &str, code: u8) -> ! {
    let envelope = serde_json::json!({
        "error": true,
        "code": 19,
        "message": format!("shutdown signal received; operation cancelled by {}", signal),
        "signal": signal,
        "graceful": true
    });
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = writeln!(handle, "{}", envelope);
    let _ = handle.flush();
    std::process::exit(code);
}
```

#### Passo 3 — Substituir o handler atual no main.rs

```rust
// src/main.rs — antes
fn handle_shutdown() {
    eprintln!("shutdown signal received; finishing current operation gracefully");
    std::process::exit(0);
}

// src/main.rs — depois
fn handle_shutdown(signal: &str) {
    shutdown::emit_shutdown_envelope_and_exit(signal, 19);
}
```

#### Passo 4 — Adicionar teste de regressão

```rust
// tests/shutdown_envelope.rs
#[test]
fn hybrid_search_emits_shutdown_envelope_on_outer_timeout() {
    let output = std::process::Command::new("timeout")
        .args(&["1", "sqlite-graphrag", "hybrid-search", "qualquer", "--json"])
        .output()
        .expect("falha ao executar hybrid-search com timeout");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "stdout não pode ser vazio em shutdown");
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout deve ser JSON válido em shutdown");
    assert_eq!(parsed["error"], true);
    assert_eq!(parsed["code"], 19);
}
```

#### Passo 5 — Atualizar documentação

- Adicionar entrada 19 na matriz de exit codes do README
- Documentar em `docs/CONTRACTS.pt-BR.md` que shutdown por sinal respeita o envelope JSON
- Adicionar entrada na tabela de `Saída Determinística`

#### Passo 6 — Validar com smoke test

```bash
# Verificar que envelope é emitido em shutdown
timeout 1 sqlite-graphrag hybrid-search "test" --json | jaq '.error // "no-envelope"'
# Deve retornar: true (com exit code 19, não 0)
```

### Relações causa × efeito

| Causa | Efeito direto | Efeito cascata |
|---|---|---|
| Signal handler com early return | Stdout vazio | `jaq` recebe 0 bytes |
| Exit code 0 hardcoded no handler | Orquestrador acha sucesso | Pipeline prossegue com dados faltantes |
| Stdout não flushado antes de exit | Pipe não vê nada | Diagnóstico obscurecido |
| Contrato JSON violado em shutdown | Inconsistência com outros erros | Hook Stop sem categoria de erro |
| Falta de exit code distinto | Impossível retry programático | Agente fica preso em loop |

### Evidências observadas

Transcript de 2026-06-15 — comando reproduzido:

```bash
$ timeout 60 sqlite-graphrag hybrid-search "rules-rust-" --k 25 --json --namespace global 2>&1 \
  | jaq '.results[]? | {name, score: .combined_score}'
shutdown signal received; finishing current operation gracefully
Error: failed to parse: value expected
$ echo "${PIPESTATUS[@]}"
0
```

Observações:
- `STDOUT_BYTES = 0` (capturado em `/tmp/hs-raw2.json`)
- `STDERR_BYTES = 65` (apenas a mensagem `shutdown signal received; finishing current operation gracefully`)
- Exit code do `sqlite-graphrag` = 0 (sucesso!)
- Exit code do `jaq` = 1 (mas jaq não tem culpa — entrada era vazia)
- Exit code do pipeline completo = 1 (pipefail não está setado, então é o último)

Comportamento esperado:
- Exit code do `sqlite-graphrag` = 19 (ou outro código distinto de 0)
- Stdout contém `{"error": true, "code": 19, "message": "..."}`
- `jaq` consegue parsear a entrada e retornar o campo `.error` para o agente
- Orquestrador pode tomar decisão baseada em exit code != 0

### Notas

- A causa raiz NÃO é: bug no `jaq` (jaq funciona corretamente com entrada vazia)
- A causa raiz NÃO é: query malformada (queries idênticas funcionam em algumas execuções)
- A causa raiz NÃO é: timeout do Bash tool cascading — é o handler que reage incorretamente ao sinal
- A causa raiz É: signal handler com early return + exit 0 + stdout não flushado
- A solução preserva o contrato JSON documentado
- A solução é compatível com todos os outros exit codes existentes
- A solução se aplica a TODOS os comandos que rodam sob o mesmo signal handler, não apenas `hybrid-search`
- Comandos de leitura (`recall`, `related`, `deep-research`) provavelmente sofrem do mesmo bug e devem ser verificados
- A solução NÃO introduz regressão em shutdowns legítimos (Ctrl-C pelo usuário continua funcionando, apenas com envelope JSON ao invés de stdout vazio)


## GAP-003 — Pipeline de embedding tem backend LLM hardcoded em `codex exec`, sem escolha de CLI headless nem de modelo pelo usuário

**Data de identificação**: 2026-06-15
**Severidade**: ALTA (viola princípio de design, satura um único backend em ambientes multi-CLI, ignora env vars do usuário, força workaround de stub pattern)
**Status**: Solucionado em v1.0.82 (--llm-backend flag global + LlmBackendFactory trait (4 implementações) + embed_with_fallback + ADR-0038)

### Problema

O pipeline de embedding do `sqlite-graphrag` v1.0.79+ (LLM-Only One-Shot) está **hardcoded** para usar exclusivamente o subprocesso `codex exec` da OpenAI Codex CLI como backend de geração de embeddings. O usuário **NÃO pode** escolher:

- Qual CLI headless será executada (claude, codex, ou outra compatível)
- Qual modelo será invocado dentro dessa CLI

As únicas formas de "controle" oferecidas ao usuário sobre o pipeline de embedding são:

- `--embedding-dim <N>` (apenas dimensionalidade do vetor)
- `--llm-parallelism <N>` (apenas quantos subprocessos paralelos)
- `--max-rss-mb <N>` (apenas limite de RAM)

Não há flag `--embedding-backend` nem `--llm-backend`, nem `--embedding-model`. As env vars `SQLITE_GRAPHRAG_CLAUDE_BINARY` e `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` são **ignoradas** pelo pipeline de embedding (são respeitadas apenas pelo `--extraction-backend claude-code`).

A função `LlmBackend::with_default_claude()` existe em `src/extract/llm_backend.rs:49-55` mas é **inacessível** via flag CLI. O construtor chamado por padrão em `src/extract/composite_backend.rs:124,131,135` é exclusivamente `LlmBackend::with_default_codex()`.

Em ambientes onde 10+ instâncias `claude code` competem pelo mesmo OAuth rate limit, o `codex exec` hardcoded também satura, gerando o erro `codex embedding call timed out after 120s`. O usuário não tem como desviar para outra CLI.

### Consequências

1. **Violação de princípio arquitetural** — usuário perde controle sobre backend crítico de embedding; CLI deveria expor escolha, não impor
2. **Saturação irreversível em ambientes multi-CLI** — 10+ instâncias `claude code` + 19+ `codex exec` concorrentes competem por OAuth; nenhum caminho de escape
3. **Env vars do usuário ignoradas silenciosamente** — `SQLITE_GRAPHRAG_CLAUDE_BINARY=claude06` não tem efeito no embedding pipeline; gera falsa impressão de configuração
4. **Workaround stub pattern necessário** — usuário precisa criar stubs < 2KB + `/tmp/<rule>_full.md` porque o `remember` com body > 50KB não completa embedding em ambiente saturado
5. **Perda de quota OAuth Pro/Max** — código continua spawnando `codex exec` mesmo quando usuário tem subscription Claude Pro/Max ativa que poderia ser usada
6. **Inconsistência com `--extraction-backend`** — extração de entidades/relação aceita `claude-code|codex|none|embedding`, mas embedding não tem flag equivalente
7. **Acoplamento implícito a OpenAI** — sistema depende de OpenAI Codex CLI mesmo quando usuário não tem subscription ChatGPT Pro
8. **Impossibilidade de uso de Ollama/LM Studio locais** — usuário não pode apontar embedding para um modelo local que preservaria quota OAuth
9. **Debugging mais difícil** — quando embedding falha, não há como distinguir "codex não instalado" de "codex OAuth saturado" de "modelo codex não disponível"
10. **Bloqueio do hook Stop em ambiente saturado** — hook Stop fica preso porque `remember` é a única via de persistência e embedding não tem fallback

### Causa raiz

A causa raiz é arquitetural e reside em **três decisões acumuladas** no design do pipeline LLM-Only da v1.0.79:

1. **`LlmBackend::with_default_codex()` como único construtor exposto** — em `src/extract/composite_backend.rs:124,131,135`, todos os caminhos de `BackendKind::Llm` instanciam exclusivamente `with_default_codex()`; `with_default_claude()` existe mas é código morto inacessível
2. **Ausência de flag CLI para embedding backend** — `src/commands/remember.rs` não define `--embedding-backend`, `--llm-backend` ou similar; as flags existentes (`--embedding-dim`, `--llm-parallelism`, `--max-rss-mb`) controlam apenas parâmetros do subprocesso, não a escolha do subprocesso em si
3. **Env vars `SQLITE_GRAPHRAG_CLAUDE_BINARY` / `CLAUDE_EMBED_MODEL` sem efeito no embedding pipeline** — `src/commands/ingest.rs:209,213` define as flags para o subcomando `ingest --mode claude-code`, mas o pipeline de embedding em `src/embedder.rs:641-678` chama `embed_passage_local` que internamente vai direto para `codex exec` sem consultar env vars

#### Cadeia causal (causa → efeito)

```
[v1.0.79 LLM-Only ADR-0019 decide delegar embedding para LLM CLI]
    ↓ implementa
[LlmBackend::with_default_codex() em composite_backend.rs]
    ↓ propagado para
[Todas as chamadas de embed_passage_local / embed_passages_parallel_local]
    ↓ resulta em
[Pipeline de embedding hardcoded em codex exec]
    ↓ quando ambiente tem
[10+ instâncias claude code + 19+ codex exec concorrentes]
    ↓ causa
[OAuth rate limit compartilhado entre CLIs]
    ↓ propaga para
[codex exec embedding call timed out after 120s]
    ↓ causa
[embedding cancelled by shutdown signal (exit 11)]
    ↓ resulta em
[Usuário preso em loop remember com stub pattern de 2KB]
```

Efeito cascata documentado em transcript de 2026-06-15: o usuário tentou forçar `claude06` via `SQLITE_GRAPHRAG_CLAUDE_BINARY=claude06` mas o sistema IGNOROU silenciosamente e continuou spawnando `codex exec` absoluto. Stub pattern foi necessário como workaround — degradação severa de UX.

### Solução

Implementar seleção de backend LLM honrando o princípio de design "usuário escolhe, sistema obedece":

1. **Adicionar flag global `--llm-backend <kind>`** — aceita `auto|claude|codex|none`, com `auto` detectando via `which claude codex` na ordem de preferência definida
2. **Adicionar flag global `--llm-model <MODEL>`** — para embedding; aceita qualquer string (Claude, Codex, ou outro backend); sem whitelist rígida
3. **Adicionar flag `--claude-binary` em `remember`** — simétrico ao que já existe em `ingest --mode claude-code`; honra `SQLITE_GRAPHRAG_CLAUDE_BINARY`
4. **Adicionar flag `--claude-embed-model` em `remember`** — simétrico a `--codex-model`; aceita `claude-sonnet-4-6`, `claude-opus-4-1`, etc.
5. **Refatorar `composite_backend.rs`** — `BackendKind::Llm` deve receber `Arc<dyn LlmBackendFactory>` injetável, não chamar `with_default_codex()` hardcoded
6. **Auto-detecção `which claude`** — quando `--llm-backend=auto`, detectar `claude` no PATH; se ausente, fallback para `codex`; se ambos ausentes, abortar com exit 1 + mensagem clara
7. **Documentar env vars expandidas** — `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL`, `SQLITE_GRAPHRAG_LLM_BACKEND` com precedência CLI > env > default
8. **Manter compatibilidade retroativa** — `--llm-backend=codex` deve ser equivalente ao comportamento atual, garantindo zero regressão

### Benefícios

1. **Usuário recupera controle** — escolhe CLI headless e modelo baseado em subscription ativa e quota disponível
2. **Saturação localizável** — quando `codex` satura, usuário pode migrar para `claude`; vice-versa
3. **Suporte a Ollama/LM Studio locais** — flag `--llm-backend=ollama` (estensão futura) preservaria quota OAuth e funcionaria offline
4. **Consistência arquitetural** — extração já tem `--extraction-backend`; embedding ganha simétrico
5. **Honra env vars** — `SQLITE_GRAPHRAG_CLAUDE_BINARY=claude06` finalmente funciona como esperado
6. **Eliminação do stub pattern** — bodies > 50KB podem ser persistidos com backend alternativo
7. **Debugging facilitado** — `--llm-backend=none` permite skip de embedding para teste isolado de parse e schema
8. **Compatibilidade preservada** — comportamento atual vira `--llm-backend=codex` (default), zero quebra
9. **Preparação para novos backends** — arquitetura com factory injection facilita adicionar Ollama, LM Studio, vLLM, llama.cpp
10. **Reduz dependência de fornecedor único** — sistema deixa de depender exclusivamente de OpenAI Codex CLI

### Como solucionar

#### Passo 1 — Adicionar flags em `src/commands/remember.rs`

```rust
// Adicionar ao RememberArgs
#[arg(long, value_enum, default_value_t = LlmBackendChoice::Auto,
      env = "SQLITE_GRAPHRAG_LLM_BACKEND",
      help = "Backend LLM para embedding: auto|claude|codex|none")]
pub llm_backend: LlmBackendChoice,

#[arg(long,
      env = "SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL",
      help = "Modelo para embedding (interpretado pelo backend escolhido)")]
pub llm_model: Option<String>,

#[arg(long, env = "SQLITE_GRAPHRAG_CLAUDE_BINARY",
      help = "Path para o binário claude (override de detecção via PATH)")]
pub claude_binary: Option<PathBuf>,
```

#### Passo 2 — Criar enum `LlmBackendChoice` em `src/extract/llm_backend.rs`

```rust
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum LlmBackendChoice {
    Auto,    // detecta via which
    Claude,  // força claude -p ou claude headless
    Codex,   // força codex exec
    None,    // skip embedding (apenas parse e schema)
}

impl LlmBackendChoice {
    pub fn resolve(self) -> Result<Arc<dyn EmbeddingProvider>, AppError> {
        match self {
            Self::Auto => detect_available_backend(),
            Self::Claude => Ok(Arc::new(ClaudeEmbeddingProvider::new(...))),
            Self::Codex => Ok(Arc::new(CodexEmbeddingProvider::new(...))),
            Self::None => Ok(Arc::new(NullEmbeddingProvider)),
        }
    }
}
```

#### Passo 3 — Refatorar `composite_backend.rs` para usar factory

```rust
// Antes
Arc::new(super::llm_backend::LlmBackend::with_default_codex())

// Depois
match backend_choice {
    BackendChoice::Claude => Arc::new(super::llm_backend::LlmBackend::with_claude_model(model)?),
    BackendChoice::Codex => Arc::new(super::llm_backend::LlmBackend::with_codex_model(model)?),
    BackendChoice::None => Arc::new(super::embedding_backend::EmbeddingBackend::new()),
}
```

#### Passo 4 — Implementar `detect_available_backend`

```rust
fn detect_available_backend() -> Result<Arc<dyn EmbeddingProvider>, AppError> {
    if which("claude").is_ok() {
        return Ok(Arc::new(ClaudeEmbeddingProvider::detect()?));
    }
    if which("codex").is_ok() {
        return Ok(Arc::new(CodexEmbeddingProvider::detect()?));
    }
    Err(AppError::Validation(
        "nenhum backend LLM disponível; instale claude CLI ou codex CLI, \
         ou use --llm-backend=none para skip".to_string()
    ))
}
```

#### Passo 5 — Adicionar testes de regressão

```rust
#[test]
fn remember_respects_llm_backend_flag() {
    let output = Command::new("sqlite-graphrag")
        .args(&["remember", "--name", "test", "--type", "note",
                "--description", "probe", "--llm-backend", "none",
                "--body", "short body", "--json"])
        .output().unwrap();
    // --llm-backend=none deve skip embedding
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn remember_honors_claude_binary_env() {
    std::env::set_var("SQLITE_GRAPHRAG_CLAUDE_BINARY", "/fake/claude");
    let parsed = RememberArgs::parse_from(&["remember", "--name", "test", "--type", "note"]);
    assert_eq!(parsed.claude_binary, Some(PathBuf::from("/fake/claude")));
}
```

#### Passo 6 — Documentar em `docs/CONFIG.pt-BR.md` e `README.pt-BR.md`

- Tabela com backends suportados (claude, codex, none; ollama planejado)
- Precedência: `--llm-backend` flag > `SQLITE_GRAPHRAG_LLM_BACKEND` env > `auto`
- Exemplos: `--llm-backend=claude --llm-model=claude-sonnet-4-6`, `--llm-backend=codex --llm-model=gpt-5.4`, `--llm-backend=none` para testes

#### Passo 7 — Validar com smoke tests

```bash
# Verificar que flag existe
sqlite-graphrag remember --help | grep -A 3 "llm-backend"

# Verificar auto-detecção
sqlite-graphrag remember --name _probe --type note --description probe --body "x" --json

# Verificar override explícito
SQLITE_GRAPHRAG_CLAUDE_BINARY=/path/to/claude sqlite-graphrag remember --name _probe --type note --body "x" --llm-backend=claude --json
```

### Relações causa × efeito

| Causa | Efeito direto | Efeito cascata |
|---|---|---|
| `LlmBackend::with_default_codex()` hardcoded em `composite_backend.rs` | Pipeline embedding usa só codex | Usuário sem ChatGPT Pro OAuth não pode usar |
| Ausência de flag `--llm-backend` em `remember.rs` | Sem como override via CLI | Env vars `CLAUDE_BINARY` ignoradas |
| `embed_passage_local` consulta só `codex` env | Embedding pipeline não detecta claude | Stub pattern vira única saída |
| OAuth rate limit compartilhado entre CLIs | codex satura em ambiente com 10+ claude | remember falha com exit 11 |
| `--extraction-backend` simétrico não existe para embedding | Inconsistência arquitetural | Usuário confunde flag de extração com embedding |
| `with_default_claude()` é código morto | Refatoração de factory não foi feita | Adicionar novo backend requer mudança invasiva |

### Evidências observadas

Transcript de 2026-06-15 — tentativa de forçar `claude06` no embedding pipeline:

```bash
# Usuário tenta forçar claude06 via env var
SQLITE_GRAPHRAG_CLAUDE_BINARY=claude06 sqlite-graphrag remember --name test --graph-stdin --json < payload.json

# Resultado: mesmo erro de codex timeout
"erro de embedding: embedding cancelled by shutdown signal"
```

Inspeção do código-fonte confirmou:

- `src/extract/llm_backend.rs:45` define `with_default_codex()` como único construtor chamado por padrão
- `src/extract/composite_backend.rs:124,131,135` chama `with_default_codex()` em todos os caminhos
- `src/embedder.rs:13-26` documenta via comentário que o sistema spawna `codex exec` subprocess
- `src/commands/ingest.rs:209,213` define `--claude-binary` e `--claude-model` apenas para `ingest --mode claude-code`, não para `remember`
- `src/commands/remember.rs:183-196` define apenas `--embedding-dim`, `--llm-parallelism`, `--max-rss-mb` — sem flag de backend
- `LlmBackend::with_default_claude()` em `src/extract/llm_backend.rs:49-55` existe mas é inacessível

### Notas

- A causa raiz NÃO é: bug no codex exec; codex funciona corretamente quando OAuth não satura
- A causa raiz NÃO é: ausência de credenciais OAuth; problema persiste mesmo com subscription ativa
- A causa raiz NÃO é: rate limit do codex; o problema é arquitetural — codex é único caminho possível
- A causa raiz É: design v1.0.79 LLM-Only escolheu codex exec hardcoded por simplicidade, sem expor override
- A solução preserva o contrato JSON atual de `remember` — flags novas são aditivas
- A solução se aplica a TODOS os comandos que fazem embedding (remember, edit, ingest, enrich)
- A solução habilita backends futuros (Ollama, LM Studio, vLLM) sem nova breaking change
- A solução NÃO introduz regressão: `--llm-backend=codex` (default) preserva comportamento atual exatamente
- A solução alinha com o princípio já documentado em `ingest --mode claude-code|codex`: usuário escolhe backend


---

## GAP-004 — Ausência de coordenação cross-process para spawn de LLM subprocess satura OAuth rate limit quando N sessões concorrentes invocam comandos com embedding simultaneamente

**Data de identificação**: 2026-06-15
**Severidade**: CRÍTICA (impede persistência proativa em qualquer host com 2+ Claude Code instances ou processos longos concorrentes; força workaround de stub pattern degradado; observadas 19 instâncias codex simultâneas no transcript de 2026-06-15)
**Status**: Solucionado em v1.0.82 (src/llm_slots.rs + LlmSlotGuard RAII + acquire_llm_slot_for_embedding + subcomando `slots` + ADR-0039)
**Distinção de GAP-003**: GAP-003 documenta falta de escolha por-invocação do backend LLM; GAP-004 documenta falta de coordenação host-wide mesmo quando o backend está corretamente escolhido — N sessões concorrentes ainda spawnam N subprocessos simultâneos competindo pelo mesmo OAuth rate limit

### Problema

O pipeline LLM-Only do `sqlite-graphrag` v1.0.79+ spawna subprocessos `codex exec` (ou `claude -p`) **independentes em cada invocação de CLI** sem qualquer mecanismo de coordenação cross-process no nível do host. Quando N sessões concorrentes de Claude Code (ou outros processos longos como CI pipelines, hooks, cron jobs) executam `remember`/`edit`/`ingest`/`enrich`/`recall`/`deep-research` simultaneamente, o sistema produz N subprocessos LLM paralelos competindo pelo mesmo OAuth rate limit compartilhado.

Não existe singleton, semáforo ou registry de subprocessos LLM no nível do host. Cada invocação de CLI enxerga apenas seu próprio contexto e spawna livremente. Quando o número de subprocessos simultâneos excede a janela de rate limit do OAuth Pro/Max, todos os N subprocessos falham simultaneamente com `codex embedding call timed out after 120s` ou `embedding cancelled by shutdown signal` (exit 11).

O singleton `lock::acquire_job_singleton` existe para `enrich` e `ingest --mode claude-code|codex` desde v1.0.68 com escopo por `db_hash` (v1.0.70), mas **NÃO se aplica** ao pipeline de embedding de `remember`/`edit`/`recall`/`hybrid-search`/`deep-research`. A coordenação é por-comando, não por-backend-LLM-compartilhado. A flag `--max-concurrency <N>` é per-invocação (controla apenas workers internos do subcomando), não host-wide. O sistema não tem consciência de que já existem 19 outros codex exec rodando no mesmo host quando vai spawnar o 20º.

### Consequências

1. **Falha em cascata em ambientes multi-sessão** — qualquer host com 2+ Claude Code instances rodando em paralelo experimenta exit 11 sistemático; 19 instâncias codex simultâneas observadas via `procs codex --pager disable | wc -l` no transcript de 2026-06-15
2. **Bloqueio do hook Stop em todas as N sessões afetadas** — cada sessão presa em loop `tem achado novo para persistir?` porque `remember` é a única via e embedding não tem fallback cross-process
3. **Stub pattern degradado como única saída** — usuário é forçado a criar stubs < 2KB + `/tmp/<rule>_full.md` + `link --create-missing` (sem embedding) para persistir conteúdo; degrada qualidade de memória
4. **Desperdício massivo de quota OAuth Pro/Max** — cada retry re-spawna subprocesso do zero; 19 instâncias × múltiplas tentativas = centenas de chamadas OAuth desperdiçadas em uma única sessão
5. **Hoarding de recursos do host** — 19 subprocessos codex simultâneos consomem CPU, RAM, file descriptors, e conexões de rede do endpoint OAuth; observação empírica via `procs codex`
6. **Impossibilidade de auditoria de incidentes** — sem registry de subprocessos LLM, não há como responder `quais subprocessos codex estavam rodando quando o hook Stop ficou bloqueado em 14:32?`
7. **Falta de fairness entre usuários/sessões** — uma sessão que precisa embedding urgentemente compete em pé de igualdade com sessões em background; nenhuma prioridade ou fila
8. **Inconsistência arquitetural com `enrich` e `ingest` que já têm singleton** — esses comandos têm `acquire_job_singleton` desde v1.0.68; `remember`/`edit`/`recall`/`deep-research` não têm mecanismo equivalente
9. **Memória-fonte preservada apenas em transcript** — quando embedding falha, body fica na transcript do Claude Code, não no SQLite; sessão precisa ser re-derivada manualmente depois via `sqlite-graphrag remember --body-file <transcript_extract>`

### Causa raiz

A causa raiz é arquitetural e reside em **quatro decisões acumuladas** no design LLM-Only da v1.0.79:

1. **Singleton `acquire_job_singleton` escopado por `job_type`, não por `llm_backend` compartilhado** — `lock::acquire_job_singleton(job_type, namespace, wait_seconds)` em `src/lock.rs` trata `enrich` e `ingest --mode claude-code|codex` como recursos exclusivos por job_type, mas o embedding de `remember`/`edit`/`recall`/`deep-research` usa caminho completamente diferente que não consulta o singleton
2. **Ausência de registry de subprocessos LLM no nível do host** — não existe `src/llm_slots.rs` ou similar que mantenha contagem de subprocessos ativos em `~/.local/share/sqlite-graphrag/llm-slots/`; cada invocação é um processo isolado sem visibilidade de outros
3. **`with_default_codex()` e `with_default_claude()` sem guard de concorrência** — em `src/extract/llm_backend.rs:45,49`, os construtores retornam `LlmBackend` que internamente spawna subprocesso via `std::process::Command::new("codex").spawn()` sem adquirir nenhum slot compartilhado entre processos
4. **Flag `--max-concurrency` é per-invocação, não host-wide** — `src/commands/ingest.rs:73` define `--max-concurrency <N>` que controla apenas workers internos do subcomando; não há equivalente que limite subprocessos LLM no host inteiro

#### Cadeia causal (causa → efeito)

```
[2+ Claude Code sessions rodando em paralelo no mesmo host]
    ↓ cada sessão invoca simultaneamente
[sqlite-graphrag remember --body "..." --graph-stdin --json]
    ↓ cada invocação spawna
[codex exec subprocess independente — N subprocessos no total]
    ↓ OAuth rate limit compartilhado entre todos
[N chamadas simultâneas ao endpoint de embedding de Pro/Max]
    ↓ causa
[OAuth rate limit atingido para todas as N chamadas em < 120s]
    ↓ propaga
[codex embedding call timed out after 120s para cada subprocess]
    ↓ causa
[embedding cancelled by shutdown signal (exit 11) em todas as N sessões]
    ↓ resulta em
[body + metadata + entidades + relacionamentos PERDIDOS em todas as N sessões]
    ↓ causa
[N hooks Stop simultaneamente bloqueados em loop "tem achado novo?"]
    ↓ resulta em
[usuário forçado a adotar stub pattern + /tmp/<rule>_full.md como única via]
```

Efeito cascata documentado em transcript de 2026-06-15: **19 instâncias codex exec simultâneas** ocupando o OAuth rate limit do host, fazendo cada tentativa de `remember` falhar com exit 11 em < 120s. Stub pattern foi necessário como workaround para 14+ entidades e 20+ relacionamentos canônicos.

### Solução

Implementar coordenação cross-process via **semáforo de subprocessos LLM no nível do host** usando o crate `fs4` (filesystem locks, trustScore 9.4 confirmado via `context7 library rust-fs4 --json`):

1. **Criar `src/llm_slots.rs` com semáforo de filesystem** — diretório `${XDG_RUNTIME_DIR:-~/.local/share}/sqlite-graphrag/llm-slots/` contendo N arquivos `slot-{0..N}.lock`; `acquire_llm_slot()` usa `fs4::FileExt::lock_exclusive()` para atomic acquire cross-process via `fcntl(F_SETLK)` no Linux/macOS e `LockFileEx` no Windows
2. **Implementar RAII guard `LlmSlotGuard`** — ao adquirir slot, retorna guard que libera automaticamente no `Drop`; previne vazamento de slots em panic ou cancelamento abrupto
3. **Adicionar flag global `--llm-max-host-concurrency <N>`** — limite host-wide de subprocessos LLM simultâneos; default derivado de `nCPUs` e tier OAuth detectado (4 para Pro, 8 para Max, 16 para Time-division-multiplexed)
4. **Adicionar flag `--llm-slot-wait-secs <N>`** — segundos para aguardar slot livre antes de falhar; default 30s; complementa `--llm-slot-no-wait` para fail-fast
5. **Modificar `claude_runner.rs` e `codex_spawn.rs`** — envolver `Command::new("codex"|"claude").spawn()` em `acquire_llm_slot()` antes do spawn e soltar o guard após o subprocess exit
6. **Aplicar a TODOS os comandos que spawnam LLM** — `remember`, `edit`, `ingest` (qualquer mode incluindo `--mode none` que ainda consulta NER), `enrich`, `recall`, `hybrid-search`, `deep-research`
7. **Adicionar subcomando `slots status --json`** — observabilidade de slots em uso, slots livres, slots em espera, tempo médio de aquisição, PIDs de proprietários; permite `watch` mode para monitoramento operacional
8. **Adicionar subcomando `slots release --slot-id <N> --yes`** — cleanup de slots órfãos de processos crashed (heurística: PID do dono não existe mais)
9. **Compatibilidade com singleton existente** — `acquire_llm_slot` é ORTHOGONAL ao `acquire_job_singleton`; ambos podem ser usados juntos sem colisão (job_singleton serializa por job_type, llm_slot serializa por subprocesso LLM)
10. **Métricas de diagnóstico** — `--llm-stats-json` flag retorna `{acquired_total, released_total, waited_total, timeout_total, peak_concurrent, current_concurrent, p50_wait_ms, p99_wait_ms}` por invocação

### Benefícios

1. **Bounded concurrency host-wide** — N máximo de subprocessos LLM simultâneos é determinístico, independente do número de sessões concorrentes
2. **Predição de quota OAuth** — usuário sabe exatamente quantas chamadas concorrentes serão feitas; budgeting fica viável via `slots status --json`
3. **Fairness entre sessões via FIFO queueing** — previne starvation de sessões em background; primeira a chegar, primeira a ser atendida
4. **Zero saturação em ambientes multi-CLI** — limite é global, não por-CLI; 10+ Claude + 19+ Codex = sempre respeita o teto
5. **Observabilidade operacional** — `slots status --json` permite monitorar uso em tempo real; integra com dashboards de monitoramento
6. **Recuperação graciosa de falhas** — slot é liberado em panic via RAII guard; nada fica órfão; cleanup explícito disponível via `slots release`
7. **Habilita tier-aware defaults** — Pro (4 slots) vs Max (8 slots) detectável via OAuth metadata no startup
8. **Compatibilidade com `enrich`/`ingest` singleton existente** — não substitui, complementa; ambos coexistem para cenários diferentes
9. **Reduz pressão sobre OAuth endpoint** — backpressure natural via wait timeout previne DoS self-inflicted
10. **Eliminação do stub pattern forçado** — bodies grandes podem ser persistidos em ambiente saturado via wait + retry automático
11. **Habilita feature futura de priority queue** — `acquire_llm_slot(priority: u8)` permite sessões interativas terem prioridade sobre CI batches
12. **Diagnóstico de incidentes facilitado** — log de `acquired_at` + `pid` + `command` por slot permite replay post-mortem

### Como solucionar

#### Passo 1 — Adicionar dependência `fs4` em `Cargo.toml`

```toml
[dependencies]
fs4 = { version = "0.9", features = ["sync"] }  # cross-platform file locking via fcntl/LockFileEx
```

#### Passo 2 — Criar `src/llm_slots.rs` com semáforo RAII

```rust
// src/llm_slots.rs
use fs4::FileExt;
use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct LlmSlotGuard {
    slot_file: File,
    slot_id: u32,
    acquired_at: Instant,
    pid: u32,
}

impl Drop for LlmSlotGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(slot_path(self.slot_id));
        tracing::debug!(
            slot_id = self.slot_id,
            held_ms = self.acquired_at.elapsed().as_millis() as u64,
            "llm slot released"
        );
    }
}

pub fn acquire_llm_slot(
    max_concurrent: u32,
    wait_secs: u64,
) -> Result<LlmSlotGuard, AppError> {
    let dir = slots_dir();
    fs::create_dir_all(&dir)?;
    let start = Instant::now();
    let timeout = Duration::from_secs(wait_secs);
    loop {
        for slot_id in 0..max_concurrent {
            let path = slot_path(slot_id);
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => {
                    if file.try_lock_exclusive().is_ok() {
                        let pid = std::process::id();
                        writeln!(file.lock_file(), "pid={}\nacquired_at={}", pid, start.elapsed().as_secs())?;
                        return Ok(LlmSlotGuard {
                            slot_file: file,
                            slot_id,
                            acquired_at: Instant::now(),
                            pid,
                        });
                    }
                }
                Err(_) => continue, // slot já existe
            }
        }
        if start.elapsed() >= timeout {
            return Err(AppError::Timeout(format!(
                "failed to acquire LLM slot within {}s (max={} concurrent)",
                wait_secs, max_concurrent
            )));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn read_status(max_concurrent: u32) -> SlotStatus {
    let dir = slots_dir();
    let mut active = 0u32;
    let mut pids = Vec::new();
    for slot_id in 0..max_concurrent {
        let path = slot_path(slot_id);
        if path.exists() {
            active += 1;
            if let Ok(content) = fs::read_to_string(&path) {
                if let Some(pid_line) = content.lines().find(|l| l.starts_with("pid=")) {
                    if let Ok(pid) = pid_line[4..].parse::<u32>() {
                        pids.push(pid);
                    }
                }
            }
        }
    }
    SlotStatus { max: max_concurrent, active, pids }
}

fn slots_dir() -> PathBuf {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| format!("{}/.local/share", h))
                .unwrap_or_else(|_| "/tmp".to_string())
        });
    PathBuf::from(base).join("sqlite-graphrag/llm-slots")
}

fn slot_path(id: u32) -> PathBuf {
    slots_dir().join(format!("slot-{}.lock", id))
}
```

#### Passo 3 — Adicionar flags globais em `src/cli.rs`

```rust
#[arg(long, global = true, default_value_t = default_host_concurrency(),
      env = "SQLITE_GRAPHRAG_LLM_MAX_HOST_CONCURRENCY",
      help = "Limite host-wide de subprocessos LLM simultâneos (default: 4 para Pro, 8 para Max)")]
pub llm_max_host_concurrency: u32,

#[arg(long, global = true, default_value_t = 30,
      env = "SQLITE_GRAPHRAG_LLM_SLOT_WAIT_SECS",
      help = "Segundos para aguardar slot LLM livre antes de falhar com exit 75")]
pub llm_slot_wait_secs: u64,

#[arg(long, global = true,
      env = "SQLITE_GRAPHRAG_LLM_SLOT_NO_WAIT",
      help = "Se setado, falha imediatamente (exit 75) quando nenhum slot livre")]
pub llm_slot_no_wait: bool,
```

#### Passo 4 — Envolver spawn em `codex_spawn.rs` e `claude_runner.rs`

```rust
// src/commands/codex_spawn.rs (modificado)
pub fn build_codex_command_with_slot(
    global_args: &GlobalArgs,
    prompt: &str,
) -> Result<(std::process::Command, LlmSlotGuard), AppError> {
    let slot = if global_args.llm_slot_no_wait {
        llm_slots::acquire_llm_slot(global_args.llm_max_host_concurrency, 0)?
    } else {
        llm_slots::acquire_llm_slot(
            global_args.llm_max_host_concurrency,
            global_args.llm_slot_wait_secs,
        )?
    };
    let cmd = build_codex_command(prompt, &global_args.codex_model)?;
    Ok((cmd, slot))
}
```

O guard é movido para o escopo do chamador e liberado quando o subprocesso exit (drop automático).

#### Passo 5 — Adicionar subcomando `slots`

```rust
// src/commands/slots.rs
#[derive(Subcommand)]
pub enum SlotsCmd {
    Status { #[arg(long)] json: bool },
    Release {
        #[arg(long)] slot_id: u32,
        #[arg(long)] yes: bool,
    },
    Cleanup {
        #[arg(long)] yes: bool,
        #[arg(long)] dry_run: bool,
    },
}

pub fn run(cmd: SlotsCmd, global: &GlobalArgs) -> Result<(), AppError> {
    match cmd {
        SlotsCmd::Status { json } => {
            let status = llm_slots::read_status(global.llm_max_host_concurrency);
            if json {
                println!("{}", serde_json::to_string(&status)?);
            } else {
                println!("LLM slots: {}/{} in use (pids: {:?})",
                    status.active, status.max, status.pids);
            }
            Ok(())
        }
        SlotsCmd::Release { slot_id, yes } => {
            if !yes {
                return Err(AppError::Validation("pass --yes to confirm".into()));
            }
            llm_slots::force_release(slot_id)
        }
        SlotsCmd::Cleanup { yes, dry_run } => {
            let stale = llm_slots::find_stale_slots(global.llm_max_host_concurrency);
            if dry_run {
                println!("would remove {} stale slots: {:?}", stale.len(), stale);
                return Ok(());
            }
            if !yes {
                return Err(AppError::Validation("pass --yes to confirm".into()));
            }
            for slot_id in stale {
                llm_slots::force_release(slot_id)?;
            }
            Ok(())
        }
    }
}
```

#### Passo 6 — Testes de regressão

```rust
// tests/llm_slots.rs
use sqlite_graphrag::llm_slots;

#[test]
fn llm_slot_enforces_max_concurrency() {
    let _g1 = llm_slots::acquire_llm_slot(2, 5).unwrap();
    let _g2 = llm_slots::acquire_llm_slot(2, 5).unwrap();
    let start = std::time::Instant::now();
    let result = llm_slots::acquire_llm_slot(2, 1);
    assert!(result.is_err());
    assert!(start.elapsed() >= std::time::Duration::from_secs(1));
}

#[test]
fn llm_slot_releases_on_drop() {
    let g1 = llm_slots::acquire_llm_slot(1, 5).unwrap();
    drop(g1);
    let _g2 = llm_slots::acquire_llm_slot(1, 5).unwrap();
}

#[test]
fn llm_slot_release_via_drop_releases_filesystem_lock() {
    let g = llm_slots::acquire_llm_slot(1, 5).unwrap();
    let slot_id = g.slot_id;
    drop(g);
    // Após drop, novo acquire deve usar mesmo slot_id (foi liberado)
    let g2 = llm_slots::acquire_llm_slot(1, 5).unwrap();
    assert_eq!(g2.slot_id, slot_id);
}

#[test]
fn remember_respects_llm_slot_with_concurrent_invocations() {
    use std::process::Command;
    use std::sync::Arc;
    use std::thread;

    let barrier = Arc::new(std::sync::Barrier::new(5));
    let mut handles = vec![];
    for i in 0..5 {
        let b = barrier.clone();
        handles.push(thread::spawn(move || {
            b.wait();
            Command::new("sqlite-graphrag")
                .args(&["remember", "--name", &format!("concurrent-{}", i),
                        "--type", "note", "--description", "concurrent test",
                        "--body", "x", "--json",
                        "--llm-max-host-concurrency", "2",
                        "--llm-slot-wait-secs", "1"])
                .output()
        }));
    }
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let successes = results.iter().filter(|r| r.as_ref().unwrap().status.success()).count();
    let slot_timeouts = results.iter().filter(|r| {
        String::from_utf8_lossy(&r.as_ref().unwrap().stderr).contains("slot")
    }).count();
    // Pelo menos 2 devem succeed (max=2), outras 3 devem timeout com mensagem de slot
    assert!(successes >= 2);
    assert!(slot_timeouts >= 1, "expected at least one slot timeout error");
}
```

#### Passo 7 — Documentar em ADR e CLI docs

- ADR novo: `docs/decisions/adr-NNNN-llm-host-slot-semaphore.md` descrevendo decisão arquitetural
- Atualizar `docs/CLI.pt-BR.md` com seção "Coordenação Cross-Process de Subprocessos LLM"
- Adicionar tabela de exit codes: exit 75 agora também cobre `LlmSlotTimeout` (com mensagem distinta do `JobSingletonLocked`)
- Adicionar exemplo em `docs/EXAMPLES.pt-BR.md` para uso em CI multi-job com `--llm-max-host-concurrency`
- Documentar env vars `SQLITE_GRAPHRAG_LLM_MAX_HOST_CONCURRENCY`, `SQLITE_GRAPHRAG_LLM_SLOT_WAIT_SECS`, `SQLITE_GRAPHRAG_LLM_SLOT_NO_WAIT`

### Relações causa × efeito

| Causa | Efeito direto | Efeito cascata |
|---|---|---|
| N sessões concorrentes invocam `remember` simultaneamente | N subprocessos codex spawnados independentemente | OAuth rate limit atingido em < 120s |
| OAuth rate limit atingido simultaneamente por N chamadas | Todos os N subprocessos timeout em 120s | exit 11 em todas as N sessões |
| exit 11 em todas as N sessões | Bodies validados perdidos em N lugares | N hooks Stop bloqueados em loop |
| N hooks Stop bloqueados | `Tem achado novo?` preso em N sessões simultaneamente | Stub pattern degradado adotado globalmente |
| Singleton `acquire_job_singleton` cobre só `enrich`/`ingest` | `remember`/`edit`/`recall`/`deep-research` sem coordenação | Multiplicação ilimitada de subprocessos codex |
| `--max-concurrency` é per-invocação apenas | Host não tem limite global | 19+ subprocessos simultâneos observados via `procs` |
| Sem `llm_slots.rs` registry | Operador não tem visibilidade de slots ativos | Diagnóstico de incidente exige `procs` externo |

### Evidências observadas

Transcript de 2026-06-15 — saturação documentada em sessão tentando persistir regras de Rust:

```bash
# Estado do host: 19 instâncias codex concorrentes
$ procs codex --pager disable --pager disable | wc -l
19

# Tentativa de remember com body canônico falha após 120s
$ sqlite-graphrag remember \
    --name rules-rust-audit-youtube-legend-cli-v0-2-9 \
    --type reference --description "rules rust audit" \
    --graph-stdin --json <<'EOF'
{"body":"...14 entidades + 17 relações...","entities":[...],"relationships":[...]}
EOF

# Resultado após 120s:
Exit code 11
stderr: "erro de embedding: codex embedding call timed out after 120s"
```

Padrão idêntico observado em **múltiplas tentativas** com diferentes combinações:
- `--llm-parallelism 1` (não ajuda — limit é OAuth compartilhado, não local)
- `--max-rss-mb 8192` (não ajuda — RSS não é gargalo)
- Variações de `claude binary` env vars (ignoradas — ver GAP-003)
- Foreground e background com sleep 120s
- Stub pattern < 2KB como única saída funcional

Inspeção do código-fonte confirmou:
- `src/commands/remember.rs:183-196` não consulta nenhum registry de subprocessos antes de spawn
- `src/extract/llm_backend.rs:45` `with_default_codex()` retorna `LlmBackend` sem guard de concorrência
- `src/lock.rs` `acquire_job_singleton` cobre apenas `enrich` e `ingest --mode claude-code|codex`
- `src/commands/ingest.rs:73` `--max-concurrency` documentado como per-invocação no help text
- `src/embedder.rs:13-26` chama `embed_passage_local` que vai direto para `codex exec` sem acquire

### Notas

- A causa raiz NÃO é: bug do codex exec; funciona corretamente em ambiente sem saturação
- A causa raiz NÃO é: ausência de credenciais OAuth; subscription Pro/Max ativa não resolve
- A causa raiz NÃO é: bug no `remember` isoladamente; GAP-001 (perda por shutdown), GAP-003 (hardcode de codex) e GAP-004 (saturação cross-process) são problemas adjacentes mas distintos
- A causa raiz É: ausência de coordenação cross-process para o recurso compartilhado `OAuth rate limit` que é por-definição host-wide e por-definição concorrente
- A solução é ortogonal ao GAP-001 (estágios com checkpoint) — pode ser combinada: Estágio A/B/C de GAP-001 com guard de slot do GAP-004
- A solução é ortogonal ao GAP-003 (escolha de backend) — `--llm-backend=claude` em vez de `codex` ainda precisa do guard de slot; as duas flags coexistem
- A solução NÃO introduz regressão: `--llm-max-host-concurrency 999999` desabilita o guard para testes isolados; `--llm-slot-no-wait` permite fail-fast
- A solução preserva compatibilidade cross-platform: `fs4` funciona em Linux/macOS/Windows com backend nativo (fcntl no Unix, LockFileEx no Windows)
- A solução habilita observabilidade via `slots status --json` para diagnóstico operacional em produção
- A solução resolve o problema raiz do stub pattern degradado: bodies grandes podem ser persistidos em ambiente saturado via wait + retry automático
- A solução se alinha com o padrão já estabelecido em v1.0.68 de `acquire_job_singleton` por `db_hash` e com o padrão v1.0.76 de OAuth-only enforcement — todas são formas de coordenação cross-process
- A solução prepara terreno para feature futura de priority queueing via `acquire_llm_slot(priority: u8)` sem nova breaking change
- A solução melhora debugabilidade de incidentes OAuth: log estruturado de `acquired_at` + `pid` + `command` por slot permite replay post-mortem de qual sessão causou saturação


---

## GAP-005 — Embedding pipeline falha silenciosamente quando subprocesso LLM crasha com exit não-zero e stderr vazio, sem captura diagnóstica nem fallback automático para backend alternativo

**Data de identificação**: 2026-06-15
**Severidade**: CRÍTICA (impede persistência proativa em qualquer host onde o backend LLM preferido está indisponível; usuário perde body validado sem entender por quê; transcript de 2026-06-15 mostra falha ao tentar persistir incidente PCIe RTX 5060 com exit 11 e stderr vazio)
**Status**: Solucionado em v1.0.82 (LlmBackendError 4-variantes + EXIT_CODE_HINTS (9 codes) + V015 pending_embeddings + subcomandos `embedding`/`pending-embeddings` + ADR-0040)
**Distinção de GAP-001/003/004**: GAP-001 trata perda por shutdown signal (timeout externo); GAP-003 trata falta de escolha por-invocação de backend; GAP-004 trata saturação OAuth por N subprocessos simultâneos; GAP-005 trata CRASH SILENCIOSO do subprocesso LLM com exit não-zero e stderr vazio, sem fallback automático e sem diagnóstico útil

### Problema

O pipeline de embedding do `sqlite-graphrag` v1.0.79+ captura o exit code e o stderr do subprocesso LLM (codex exec ou claude -p) mas **não aproveita essa informação para diagnóstico, fallback automático ou degradação graciosa**. Quando o subprocesso LLM crasha com exit não-zero (por exemplo exit 1) e produz stderr vazio, a CLI retorna:

```json
{"error": true, "code": 11, "message": "erro de embedding: codex exited with exit status: 1: stderr="}
```

A mensagem revela três problemas arquiteturais simultâneos:

1. **Stderr vazio é descartado** — quando o subprocesso produz qualquer saída em stderr (mesmo uma linha de crash), o wrapper a substitui pelo placeholder literal `stderr=`; nenhuma informação de diagnóstico chega ao usuário
2. **Exit code é exibido mas não interpretado** — `exit status: 1` é genérico; o sistema não mapeia exit codes conhecidos (137 = OOM kill, 139 = SIGSEGV, 143 = SIGTERM, etc.) para sugestões acionáveis
3. **Nenhum fallback é tentado** — se codex exec crasha, o sistema não tenta automaticamente claude -p (ou vice-versa); o usuário precisa descobrir manualmente, ajustar env vars (ignoradas conforme GAP-003), e re-executar o comando inteiro

O transcript de 2026-06-15 mostra o caso concreto: ao tentar persistir `diag-freeze-login-2026-06-15` (incidente PCIe RTX 5060 com `type: incident`, `force-merge`, `graph-stdin` com 2 entidades e 2 relacionamentos), o sistema retornou exit 11 com `codex exited with exit status: 1: stderr=` — o usuário perdeu o body validado sem qualquer pista de por que codex falhou.

### Consequências

1. **Perda total de body validado em falha silenciosa** — o body parseado, namespace resolvido, entidades validadas via `--graph-stdin` são descartados quando o subprocesso LLM crasha com stderr vazio
2. **Impossibilidade de diagnóstico post-mortem** — `exit status: 1` é genérico; usuário não sabe se foi OOM kill (137), segfault (139), signal externo (143), erro de inicialização, ou configuração inválida
3. **Stub pattern degradado como única saída** — usuário novamente forçado a usar `link --create-missing` sem embedding para persistir conteúdo quando LLM backend está indisponível
4. **Bloqueio persistente do hook Stop** — o salvamento proativo fica travado porque `remember` é a única via de persistência e embedding não tem fallback
5. **Desperdício de quota OAuth** — cada retry re-spawna o mesmo backend quebrado; sem fallback, não há como desviar para backend alternativo
6. **Inconsistência com tratamento de outros subprocessos** — `enrich`, `ingest` e `deep-research` têm tratamento de erro mais rico; `remember`/`edit` têm apenas o envelope genérico
7. **Falsa sensação de robustez** — usuário vê o envelope JSON formatado e pensa que o sistema reportou erro útil, mas o conteúdo é essencialmente vazio
8. **Memória-fonte preservada apenas em transcript** — como em GAP-001/003/004, o conteúdo só existe no transcript do Claude Code, não no SQLite
9. **Impossibilidade de priorização de incidente** — o usuário tentando persistir `diag-freeze-login-2026-06-15` (incidente de hardware) perde o registro do incidente por causa de problema no pipeline de embedding, criando meta-falha

### Causa raiz

A causa raiz é arquitetural e reside em **três decisões acumuladas** no design do pipeline de embedding LLM-Only da v1.0.79:

1. **`Stdio::piped()` ausente ou mal configurado em `codex_spawn.rs` e `claude_runner.rs`** — o wrapper provavelmente usa `Command::output()` ou `Command::spawn()` sem preservar o `Stdio::piped()` para stderr, fazendo com que o buffer de stderr seja descartado ou substituído pelo placeholder `stderr=`
2. **Exit codes não são mapeados para diagnósticos** — não existe tabela `EXIT_CODE_HINTS` em `src/embedder.rs` que associe exit codes conhecidos (1, 2, 101, 126, 127, 134, 137, 139, 143) a mensagens acionáveis como `binary not found`, `permission denied`, `OOM killed`, `segfault`, `SIGTERM received`
3. **Ausência de cadeia de fallback** — não existe `LlmBackend::embed_with_fallback(prompt, backends: &[LlmBackendKind])` que tente múltiplos backends em ordem até um succeed; cada chamada vai direto para o backend hardcoded (ver GAP-003) sem opção de retry com alternativo

#### Cadeia causal (causa → efeito)

```
[Subprocesso codex exec recebe signal externo OU OOM OU crash de inicialização]
    ↓ executa
[exit 1 com stderr vazio (buffer descartado antes de flush)]
    ↓ wrapper captura
[Command::output() ou spawn() sem Stdio::piped() para stderr]
    ↓ substitui por placeholder
[mensagem `codex exited with exit status: 1: stderr=` literal]
    ↓ CLI propaga como
[envelope JSON {error: true, code: 11, message: `...`}]
    ↓ stdout emite
[stderr= descarta qualquer linha de crash que codex poderia ter emitido]
    ↓ não há mapeamento
[exit 1 é exibido mas não interpretado como `binary not found` / `OOM` / `segfault`]
    ↓ não há fallback
[sistema não tenta claude -p automaticamente; fica preso em codex quebrado]
    ↓ resulta em
[body + entidades + relacionamentos validados PERDIDOS]
    ↓ causa
[hook Stop bloqueado; usuário preso em loop `tem achado novo?`]
    ↓ força
[stub pattern degradado como única via de persistência]
```

Efeito cascata documentado em transcript de 2026-06-15: ao tentar persistir `diag-freeze-login-2026-06-15` (incidente PCIe RTX 5060 com 2 entidades e 2 relacionamentos canônicos), o usuário perdeu o body validado. O envelope de erro continha apenas `codex exited with exit status: 1: stderr=` — sem nenhuma indicação de que ação tomar.

### Solução

Implementar captura diagnóstica robusta, mapeamento de exit codes, cadeia de fallback e degradação graciosa:

1. **Garantir `Stdio::piped()` em stderr e stdout** — em `src/commands/codex_spawn.rs` e `src/commands/claude_runner.rs`, configurar `Command::stderr(Stdio::piped())` e `Command::stdout(Stdio::piped())` antes do spawn; usar `output().stderr` e `output().stdout` para capturar buffers completos
2. **Criar tabela `EXIT_CODE_HINTS` em `src/embedder.rs`** — associar exit codes conhecidos a diagnósticos acionáveis:
   - `1` → `subprocesso retornou erro genérico; verificar logs em ~/.local/share/sqlite-graphrag/llm-backend.log`
   - `2` → `uso incorreto do CLI; verificar flags passadas`
   - `101` → `OOM killer do kernel terminou o subprocesso; reduzir --llm-parallelism ou usar --llm-backend=none`
   - `126` → `binary não executável; verificar permissões com chmod +x`
   - `127` → `binary não encontrado no PATH; verificar which codex ou which claude`
   - `134` → `SIGABRT recebido; abort interno do subprocesso`
   - `137` → `SIGKILL (OOM killer ou externo); verificar dmesg | grep -i kill`
   - `139` → `SIGSEGV; reportar bug upstream do codex/claude`
   - `143` → `SIGTERM externo; verificar se hook PreToolUse ou timeout cascateou`
3. **Adicionar flag global `--llm-fallback <BACKENDS>`** — cadeia de backends separados por vírgula, tentados em ordem até um succeed; exemplo: `--llm-fallback codex,claude,none`; default `codex,claude,none` quando codex é o primário
4. **Adicionar flag `--skip-embedding-on-failure`** — quando setada, persiste o body em `memories` com `memory_embeddings` NULL e status `pending_embedding` em tabela paralela; CLI retorna exit 0 com aviso em vez de exit 11
5. **Criar tabela `pending_embeddings` espelhando `pending_memories` (de GAP-001)** — registra bodies com embedding pendente para re-processamento via `enrich --operation re-embed --pending-only`
6. **Adicionar subcomando `embedding status --json`** — observabilidade: quantas memórias com embedding NULL, qual backend falhou por último, idade do pending mais antigo
7. **Adicionar subcomando `embedding retry --name <MEMORY> --backend <KIND>`** — re-tenta embedding de uma memória específica usando backend explícito
8. **Capturar stdout E stderr separadamente** — quando subprocesso produz erro, ambos os streams vão para o envelope JSON (`stdout_tail: ...`, `stderr_tail: ...`) com truncamento em 1KB cada

### Benefícios

1. **Diagnóstico acionável** — usuário recebe sugestão concreta baseada no exit code (OOM, not found, segfault) em vez de `stderr=`
2. **Recuperação automática via fallback chain** — codex quebra → sistema tenta claude automaticamente → claude quebra → sistema tenta `none` para skip
3. **Zero perda de body mesmo com backend quebrado** — `--skip-embedding-on-failure` permite persistir imediatamente, re-embodir depois quando backend volta
4. **Observabilidade operacional** — `embedding status --json` permite monitorar saúde do pipeline de embedding em tempo real
5. **Habilita degradação graciosa** — usuário pode configurar política `--llm-fallback claude,none` para preferir claude mas aceitar skip
6. **Complementa GAP-001 (estágios com checkpoint)** — bodies salvos com embedding NULL são `Estágio A completo, Estágio B pendente`; podem ser retomados via `enrich --operation re-embed --pending-only`
7. **Complementa GAP-003 (escolha de backend)** — `--llm-fallback` é a expressão de escolha em runtime; pode sobrescrever `--llm-backend` quando o primário falha
8. **Complementa GAP-004 (coordenação cross-process)** — slots do GAP-004 podem ser esgotados por backend com bug; fallback reduz pressão sobre o backend problemático
9. **Habilita debugging de bugs do codex/claude** — stderr preservado permite reportar bugs upstream com informação útil em vez de `deu errado`
10. **Preserva intenção do usuário** — body de incidente de hardware (`diag-freeze-login-2026-06-15`) não é perdido por bug de pipeline

### Como solucionar

#### Passo 1 — Modificar `codex_spawn.rs` para preservar stderr/stdout completos

```rust
// src/commands/codex_spawn.rs (modificado)
use std::io::Read;
use std::process::{Command, Stdio};

pub fn run_codex_with_capture(
    prompt: &str,
    model: &str,
) -> Result<String, LlmBackendError> {
    let mut child = Command::new("codex")
        .args([`exec`, `--model`, model, `--json`, `--output-schema`, EMBED_SCHEMA])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())  // ESSENCIAL: sem isso, stderr é perdido
        .spawn()
        .map_err(|e| LlmBackendError::SpawnFailed {
            binary: `codex`,
            source: e.to_string(),
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes())?;
    }

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        out.read_to_string(&mut stdout)?;
    }
    if let Some(mut err) = child.stderr.take() {
        err.read_to_string(&mut stderr)?;
    }
    let status = child.wait()?;

    if !status.success() {
        return Err(LlmBackendError::NonZeroExit {
            exit_code: status.code(),
            signal: status.signal(),
            stdout_tail: stdout.chars().rev().take(1024).collect::<String>().chars().rev().collect(),
            stderr_tail: stderr.chars().rev().take(1024).collect::<String>().chars().rev().collect(),
        });
    }
    Ok(stdout)
}
```

#### Passo 2 — Criar tabela de diagnóstico em `src/embedder.rs`

```rust
// src/embedder.rs
use std::collections::HashMap;
use once_cell::sync::Lazy;

pub static EXIT_CODE_HINTS: Lazy<HashMap<i32, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(1, `subprocesso retornou erro genérico; verificar logs em ~/.local/share/sqlite-graphrag/llm-backend.log`);
    m.insert(2, `uso incorreto do CLI do subprocesso; rever flags passadas`);
    m.insert(101, `SIGABRT do kernel; possível panic no código do subprocesso`);
    m.insert(126, `binary não executável; executar chmod +x no binário`);
    m.insert(127, `binary não encontrado no PATH; verificar which codex ou which claude`);
    m.insert(134, `SIGABRT; abort interno do subprocesso — reportar bug upstream`);
    m.insert(137, `SIGKILL do OOM killer ou externo; verificar dmesg | grep -i kill e reduzir --llm-parallelism`);
    m.insert(139, `SIGSEGV; reportar bug upstream com stderr preservado`);
    m.insert(143, `SIGTERM externo; hook PreToolUse ou timeout cascateou`);
    m
});

pub fn diagnose_exit_code(code: Option<i32>, signal: Option<i32>) -> String {
    let code = code.unwrap_or(-1);
    EXIT_CODE_HINTS
        .get(&code)
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!(`exit code {} desconhecido; consultar upstream docs`, code))
}
```

#### Passo 3 — Refatorar `LlmBackend` para suportar fallback chain

```rust
// src/extract/llm_backend.rs (modificado)
pub async fn embed_with_fallback(
    prompt: &str,
    backends: &[LlmBackendKind],
    skip_on_failure: bool,
) -> Result<Vec<f32>, LlmBackendError> {
    let mut last_err: Option<LlmBackendError> = None;
    for backend in backends {
        match backend.embed(prompt).await {
            Ok(vec) => return Ok(vec),
            Err(e) => {
                tracing::warn!(
                    backend = ?backend,
                    error = %e,
                    `fallback: backend falhou, tentando próximo`
                );
                last_err = Some(e);
            }
        }
    }
    if skip_on_failure {
        tracing::warn!(
            `todos os backends falharam; persistindo sem embedding (--skip-embedding-on-failure)`
        );
        Ok(vec![]) // vetor vazio = signal para NULL embedding
    } else {
        Err(last_err.unwrap_or_else(|| LlmBackendError::NoBackendsAvailable))
    }
}
```

#### Passo 4 — Adicionar flags globais em `src/cli.rs`

```rust
#[arg(long, global = true, default_value = `codex,claude,none`,
      env = `SQLITE_GRAPHRAG_LLM_FALLBACK`,
      help = `Cadeia de backends tentados em ordem; separados por vírgula`)]
pub llm_fallback: String,

#[arg(long, global = true,
      env = `SQLITE_GRAPHRAG_SKIP_EMBEDDING_ON_FAILURE`,
      help = `Persiste com embedding NULL quando todos os backends falham`)]
pub skip_embedding_on_failure: bool,
```

#### Passo 5 — Criar tabela `pending_embeddings` via migração V015

```sql
-- migrations/V015__pending_embeddings.sql
CREATE TABLE IF NOT EXISTS pending_embeddings (
    pending_id INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id INTEGER NOT NULL,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    backend_chain TEXT NOT NULL,           -- ex: `codex,claude,none`
    last_error TEXT,
    last_exit_code INTEGER,
    last_stderr_tail TEXT,                 -- 1KB máx
    attempt_count INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL CHECK (status IN
        (`pending`, `in_progress`, `done`, `abandoned`)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (memory_id) REFERENCES memories(memory_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_pending_embeddings_status
    ON pending_embeddings(status, updated_at);
```

#### Passo 6 — Adicionar subcomandos `embedding` e `pending_embeddings`

```rust
// src/commands/embedding.rs
#[derive(Subcommand)]
pub enum EmbeddingCmd {
    Status { #[arg(long)] json: bool },
    Retry {
        #[arg(long)] name: String,
        #[arg(long)] backend: String,
    },
}

// src/commands/pending_embeddings.rs
#[derive(Subcommand)]
pub enum PendingEmbeddingsCmd {
    List { #[arg(long)] json: bool },
    RetryAll { #[arg(long)] backend: String, #[arg(long)] yes: bool },
    Abandon { #[arg(long, conflicts_with = `name`)] all: bool, #[arg(long, conflicts_with = `all`)] name: Option<String>, #[arg(long)] yes: bool },
}
```

#### Passo 7 — Testes de regressão

```rust
// tests/silent_failure_capture.rs
#[test]
fn codex_exit_1_preserves_stderr_in_envelope() {
    // Mock codex que crasha com exit 1 e stderr=`OOM`
    let result = run_codex_with_capture(`prompt`, `gpt-5.5`);
    match result {
        Err(LlmBackendError::NonZeroExit { exit_code, stderr_tail, .. }) => {
            assert_eq!(exit_code, Some(1));
            assert!(stderr_tail.contains(`OOM`));
        }
        _ => panic!(`esperava NonZeroExit com stderr capturado`),
    }
}

#[test]
fn embed_falls_back_to_claude_when_codex_crashes() {
    // Mock codex que sempre crasha; mock claude que funciona
    let result = embed_with_fallback(
        `test prompt`,
        &[LlmBackendKind::Codex, LlmBackendKind::Claude],
        false,
    );
    assert!(result.is_ok());
}

#[test]
fn skip_embedding_on_failure_persists_with_null_embedding() {
    // Mock todos os backends falhando; --skip-embedding-on-failure setada
    let result = embed_with_fallback(
        `test prompt`,
        &[LlmBackendKind::Codex, LlmBackendKind::Claude, LlmBackendKind::None],
        true, // skip
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 0); // vetor vazio
}

#[test]
fn exit_code_137_yields_oom_killer_diagnostic() {
    let hint = diagnose_exit_code(Some(137), None);
    assert!(hint.contains(`OOM`));
}
```

#### Passo 8 — Documentar em ADR e CLI docs

- ADR novo: `docs/decisions/adr-NNNN-silent-failure-capture-fallback-chain.md`
- Atualizar `docs/CLI.pt-BR.md` com seção `Diagnóstico de Falhas do Backend LLM`
- Adicionar flag `--llm-fallback` à tabela de exit codes: comportamento depende de cada backend na chain
- Documentar `pending_embeddings` em `docs/SCHEMA.pt-BR.md`
- Adicionar exemplo em `docs/EXAMPLES.pt-BR.md` para uso de `--skip-embedding-on-failure` em CI

### Relações causa × efeito

| Causa | Efeito direto | Efeito cascata |
|---|---|---|
| `Stdio::piped()` ausente em stderr do subprocesso LLM | Buffer de stderr descartado | Mensagem `stderr=` literal sem informação |
| Exit code 1 exibido mas não mapeado para diagnóstico | Usuário não sabe se foi OOM, segfault, not found | Retry cego no mesmo backend quebrado |
| Ausência de fallback chain | codex quebra → sistema não tenta claude | 100% de falha quando backend primário está down |
| `--skip-embedding-on-failure` inexistente | Body sempre perdido em crash | Stub pattern degradado como única saída |
| `pending_embeddings` table inexistente | Não há fila para re-embedding | Incidentes críticos (PCIe) perdidos por bug de pipeline |
| Stdout e stderr não preservados separadamente | Reportar bug upstream impossível | Mesma falha se repete para outros usuários |
| `EXIT_CODE_HINTS` table ausente | Diagnóstico genérico sempre | Tempo de debugging de incidente multiplicado |

### Evidências observadas

Transcript de 2026-06-15 — tentativa de persistir `diag-freeze-login-2026-06-15`:

```bash
$ sqlite-graphrag remember \
    --name diag-freeze-login-2026-06-15 \
    --type incident \
    --description `Recorrência: travamento na tela de login GDM/Wayland por erros PCIe Gen5 RxErr na porta GPU RTX 5060 em 2026-06-15. 47 erros PCIe em 2 boots. Mesmo problema de crash-pcie-rtx5060-2026-06-03 não corrigido.` \
    --force-merge \
    --json \
    --graph-stdin <<'GRAPHEOF'
{`body`:`Nova ocorrência do incidente PCIe RTX 5060 documentado em crash-pcie-rtx5060-2026-06-03. ...`,`entities`:[...],`relationships`:[...]}
GRAPHEOF

# Resultado:
Error: Exit code 11
{
  `error`: true,
  `code`: 11,
  `message`: `erro de embedding: codex exited with exit status: 1: stderr=`
}
2026-06-15T05:01:31.923093Z ERROR output: erro de embedding: codex exited with exit status: 1: stderr=
```

Observações-chave:
- Exit code 11 (mesmo do GAP-001 e GAP-004) mas causa raiz completamente distinta
- `stderr=` é literal e vazio — não é truncamento, é ausência total
- Exit code do subprocesso codex = 1 (genérico, não 137 OOM nem 139 SIGSEGV nem 143 SIGTERM)
- Body validado continha incidente crítico de hardware (PCIe RTX 5060 com 47 erros)
- Entidades e relacionamentos validados antes do spawn do subprocesso foram perdidos

### Notas

- A causa raiz NÃO é: timeout (GAP-001), saturação OAuth (GAP-004), hardcode de backend (GAP-003)
- A causa raiz NÃO é: bug do codex upstream; codex pode estar perfeitamente funcional em outro host
- A causa raiz NÃO é: rate limit; exit 1 com stderr vazio é incompatível com rate limit (que causaria 429 e mensagem específica)
- A causa raiz É: o wrapper de subprocesso do `sqlite-graphrag` não configura `Stdio::piped()` para stderr, OU não lê o buffer antes de fechar o handle, OU substitui por placeholder literal
- A solução é ortogonal a GAP-001 (estágios com checkpoint), GAP-003 (escolha de backend) e GAP-004 (coordenação cross-process) — pode ser combinada com todas as três
- A solução habilita o padrão `persist now, embed later` via `--skip-embedding-on-failure` que é crítico para incidentes urgentes (como o PCIe freeze)
- A solução NÃO introduz regressão: comportamento atual de exit 11 em falha vira exit 11 com mensagem útil; usuários que preferem comportamento antigo podem usar `--llm-fallback codex` (sem chain)
- A solução preserva compatibilidade: `--skip-embedding-on-failure` é opt-in; default é falhar com exit 11 como hoje
- A solução melhora debugging de bugs upstream do codex/claude: stderr preservado permite reportar com informação útil em vez de `deu errado`
- A solução alinha com o princípio de `usuário escolhe, sistema obedece` — usuário escolhe política de fallback e skip, sistema executa fielmente
- A solução é compatível com `pending_embeddings` reaproveitando o padrão de `pending_memories` introduzido conceitualmente em GAP-001
- A solução resolve o caso concreto do transcript: persistir `diag-freeze-login-2026-06-15` teria succeeded com `--skip-embedding-on-failure --llm-fallback codex,claude,none`, body preservado, embedding diferido para `enrich --operation re-embed` quando backend voltar
---


## GAP-006 — `env_clear()` em três spawners LLM remove credenciais de provider customizado, bloqueando uso de Anthropic-compatible providers (MiniMax, OpenRouter, gateways corporativos)

**Data de identificação**: 2026-06-17
**Severidade**: ALTA (impede uso de providers customizados para usuários fora do ecossistema Anthropic oficial; provider MiniMax/api.minimax.io relatado em produção 2026-06-17 com 401 mascarado)
**Status**: Solucionado em v1.0.83 (helper `src/spawn/env_whitelist.rs` + flag `--strict-env-clear` + ADR-0041 + 5 testes seriais em `tests/claude_runner_env.rs`)
**Distinção de GAP-001/002/003/004/005**: GAP-001 trata perda por shutdown signal (timeout externo); GAP-002 trata violação de contrato JSON de erro sob shutdown; GAP-003 trata falta de escolha por-invocação de backend; GAP-004 trata saturação OAuth por N subprocessos simultâneos; GAP-005 trata crash silencioso do subprocesso LLM com stderr vazio; GAP-006 trata PRESERVAÇÃO INSUFICIENTE DE ENV VARS para providers customizados — os guard OAuth-only rejeitam `ANTHROPIC_API_KEY` corretamente, mas `env_clear()` remove credenciais customizadas legítimas (`ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`) que NÃO deveriam ser bloqueadas

### Problema

O pipeline de embedding LLM-Only do `sqlite-graphrag` v1.0.76+ aplica `env_clear()` seguido de reinserção de whitelist restrita em três spawners (`src/commands/claude_runner.rs`, `src/commands/codex_spawn.rs`, `src/commands/ingest_claude.rs`). A whitelist omitia variáveis de ambiente necessárias para providers Anthropic-compatible customizados:

- `ANTHROPIC_AUTH_TOKEN` — token de autenticação para Claude Code rotear via OAuth customizado (MiniMax, OpenRouter, gateway corporativo)
- `ANTHROPIC_BASE_URL` — endpoint URL customizado (ex: `https://api.minimax.io/anthropic`)
- `OPENAI_BASE_URL` — endpoint OpenAI-compatible customizado (ex: `https://api.openrouter.ai/v1`)
- `CODEX_ACCESS_TOKEN` — token de acesso para Codex CLI customizado
- `CLAUDE_CODE_ENTRYPOINT` — override de entrypoint específico do Claude Code
- `DISABLE_TELEMETRY` — opt-out de telemetria do subprocesso
- `OTEL_EXPORTER_OTLP_ENDPOINT` — override de collector OTel

Quando o usuário define essas variáveis no ambiente do orquestrador, elas NÃO chegam ao subprocesso `claude -p` ou `codex exec`, que falha com `401 Invalid authentication credentials` e sai com exit 1. O `claude_runner::generate_embedding` retorna `AppError::EmbeddingFailed` (exit 11), e o body validado é perdido de forma similar ao GAP-005.

Adicionalmente, a duplicação da whitelist idêntica em três spawners cria risco de drift: qualquer correção aplicada em um spawner precisa ser replicada nos outros dois, e a v1.0.82 omitiu `ANTHROPIC_AUTH_TOKEN` em todos os três simultaneamente.

### Consequências

1. **Provider MiniMax inutilizável em produção** — usuários com assinatura MiniMax (api.minimax.io) recebem `401 Invalid authentication credentials` mascarado em stderr vazio (mesmo padrão do GAP-005), o body validado é perdido
2. **OpenRouter bloqueado** — usuários configurando `OPENAI_BASE_URL=https://api.openrouter.ai/v1` não conseguem usar a roteador porque a var nunca chega ao codex exec
3. **Gateways corporativos inacessíveis** — ambientes empresariais com proxy Anthropic-compatible próprio (ex: AWS Bedrock com roteamento customizado) não funcionam
4. **Duplicação do whitelist em três sites** — divergência de manutenção; se v1.0.84 precisar adicionar nova var, os três sites devem ser editados sincronamente
5. **Inconsistência semântica com guard OAuth-only** — o guard OAuth-only rejeita `ANTHROPIC_API_KEY`/`OPENAI_API_KEY` corretamente (chaves pagas, ADR-0011), mas o `env_clear()` remove INDIRETAMENTE as credenciais customizadas legítimas, sem rejeição EXPLÍCITA — comportamento implícito difícil de diagnosticar
6. **Zero audit trail do leak** — sem teste que verifique que o valor literal de `ANTHROPIC_AUTH_TOKEN` NÃO aparece em stderr do subprocesso
7. **Sem opt-out para compliance** — ambientes que PROÍBEM encaminhamento de credenciais via env vars (PCI-DSS, SOC2) não têm flag para desabilitar o whitelist amplo

### Causa raiz

A causa raiz é arquitetural e reside em duas decisões acumuladas durante a transição para LLM-Only OAuth-only:

1. **Whitelist incompleta em três spawners** — o whitelist consolidado do `claude_runner.rs:14-35`, `codex_spawn.rs:277-293` e `ingest_claude.rs:299-319` lista apenas vars POSIX/XDG/Claude Code básicas. Faltam `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CODEX_ACCESS_TOKEN`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT`
2. **Duplicação literal em três arquivos** — o array de whitelist é IDÊNTICO em todos os três spawners (verificado em v1.0.82), mas a manutenção NÃO foi centralizada, criando janela para divergência

A interpretação original do mandato OAuth-only da v1.0.69 GENERALIZOU o conceito de "credenciais de API" para "qualquer credencial relacionada ao LLM", o que é semanticamente incorreto. `ANTHROPIC_API_KEY` (chave paga sk-ant-...) é diferente de `ANTHROPIC_AUTH_TOKEN` (token OAuth sem custo de API, pago via assinatura Claude Pro/Max).

#### Cadeia causal (causa → efeito)

```
[Usuário configura ANTHROPIC_AUTH_TOKEN=sk-cp-... e ANTHROPIC_BASE_URL=https://api.minimax.io/anthropic]
    ↓ executa
[sqlite-graphrag remember --body "..." --graph-stdin < payload.json]
    ↓ remember chama
[claude_runner::generate_embedding que prepara Command via build_claude_command]
    ↓ aplica
[env_clear() remove TODAS as env vars do subprocesso]
    ↓ reinsere apenas whitelist restrito
[whitelist NÃO contém ANTHROPIC_AUTH_TOKEN nem ANTHROPIC_BASE_URL]
    ↓ spawna
[claude -p subprocesso inicia SEM credenciais customizadas]
    ↓ tenta autenticar
[Endpoint MiniMax retorna HTTP 401 Invalid authentication credentials]
    ↓ claude -p escreve em stderr
[stderr= vazio OU truncado pelo wrapper (mesmo bug do GAP-005)]
    ↓ retorna
[exit status 1 com stderr descartado]
    ↓ claude_runner captura
[AppError::EmbeddingFailed com mensagem "claude exited with exit status: 1"]
    ↓ remember aborta
[body validado + entidades + relacionamentos PERDIDOS]
    ↓ resulta
[Usuário precisa workaround manual via --skip-embedding-on-failure do GAP-005]
    ↓ OU migra para
[Provider oficial Anthropic (pagando API key) ou fica sem persistência]
```

Efeito cascata documentado em produção 2026-06-17: usuário tentando usar provider MiniMax para persistir memória via hook Stop perdeu o body validado, mesmo padrão do GAP-001/003/005, mas com causa raiz ORTOGONAL (env_clear incompleto em vez de crash de subprocesso).

### Solução

Eliminar duplicação via helper compartilhado, expandir whitelist com credenciais de provider customizado, adicionar flag opt-out para compliance, e estabelecer auditoria de no-leak:

1. **Criar `src/spawn/env_whitelist.rs`** — helper único expondo `PRESERVED_ENV_VARS`, `PRESERVED_ENV_VARS_WINDOWS`, `apply_env_whitelist(cmd, strict)`, `is_strict_env_clear()`; 3 testes unitários seriais validam preservação de vars customizadas, exclusão de API keys, e modo strict
2. **Refatorar os três spawners para usar o helper** — `claude_runner.rs`, `codex_spawn.rs`, `ingest_claude.rs` removem arrays inline e delegam para `apply_env_whitelist(cmd, is_strict_env_clear())`
3. **Expandir whitelist com 7 vars customizadas** — `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CODEX_ACCESS_TOKEN`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT`
4. **Adicionar flag `--strict-env-clear` e env `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR`** — modo compliance que preserva apenas `PATH`, dropa todas as credenciais; aplicável em PCI-DSS, SOC2, HIPAA
5. **Criar `tests/claude_runner_env.rs`** — 5 testes seriais com `serial_test::serial(env)` validando: claude herda `ANTHROPIC_AUTH_TOKEN`, claude rejeita `ANTHROPIC_API_KEY` (regressão OAuth-only), codex herda `OPENAI_BASE_URL`, strict mode dropa credenciais, e auditoria de no-leak (valor literal do token NUNCA aparece em stderr com `RUST_LOG=trace`)
6. **Criar `docs/decisions/adr-0041-preserve-custom-provider-env.md`** — justificativa arquitetural completa com alternativas consideradas, consequências, e cross-references a ADR-0011, ADR-0025, ADR-0033

### Benefícios

1. **Providers Anthropic-compatible funcionam** — MiniMax, OpenRouter, gateways corporativos passam a autenticar corretamente
2. **Defesa em profundidade OAuth-only preservada** — guard rejeita `ANTHROPIC_API_KEY`/`OPENAI_API_KEY` antes mesmo de chegar ao env_clear, env whitelist é segunda linha de defesa
3. **DRY achieved via helper** — adição futura de vars requer mudança em um único arquivo (`src/spawn/env_whitelist.rs`); os três spawners passam a consumir helper compartilhado
4. **Compliance opt-in** — flag `--strict-env-clear` atende ambientes que proíbem encaminhamento de credenciais
5. **Auditoria de no-leak como regressão** — teste 5 em `tests/claude_runner_env.rs` valida ausência do valor literal do token em stderr com máxima verbosidade; previne vazamento futuro
6. **Cross-reference ao G58 S5** — parcial resolução do gap documentado em `gap-g58-recall-sem-fallback-deterministic-2026-06-13` (memória GraphRag): provider customizado via env contorna fadiga OAuth oficial
7. **Compatibilidade total preservada** — sem breaking changes; usuários que NÃO definem as vars customizadas não veem diferença; usuários que DEFINEM passam a ver seus providers funcionando
8. **Mensagens OAuth-only orientativas** — quando guard OAuth-only dispara, mensagem agora aponta para OAuth subscription E para `ANTHROPIC_AUTH_TOKEN` como alternativa legítima (não apenas "OAuth-only violation")

### Como solucionar

#### Passo 1 — Criar helper compartilhado em `src/spawn/env_whitelist.rs`

```rust
// src/spawn/env_whitelist.rs (novo, ADR-0041)
pub const PRESERVED_ENV_VARS: &[&str] = &[
    // Standard POSIX / XDG
    "PATH", "HOME", "USER", "SHELL", "TERM", "LANG",
    "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_RUNTIME_DIR", "XDG_CACHE_HOME",
    // Temporary directories
    "TMPDIR", "TMP", "TEMP",
    // macOS dynamic linker fallback path
    "DYLD_FALLBACK_LIBRARY_PATH",
    // Claude Code specific
    "CLAUDE_CONFIG_DIR",
    // v1.0.83 (ADR-0041): custom provider credentials for Claude Code
    "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_BASE_URL", "CLAUDE_CODE_ENTRYPOINT",
    // v1.0.83 (ADR-0041): custom provider credentials for Codex CLI
    "CODEX_ACCESS_TOKEN", "OPENAI_BASE_URL",
    // v1.0.83 (ADR-0041): telemetry opt-out and observability override
    "DISABLE_TELEMETRY", "OTEL_EXPORTER_OTLP_ENDPOINT",
];

#[cfg(windows)]
pub const PRESERVED_ENV_VARS_WINDOWS: &[&str] = &[
    "LOCALAPPDATA", "APPDATA", "USERPROFILE", "SystemRoot", "COMSPEC",
    "PATHEXT", "HOMEPATH", "HOMEDRIVE",
];

pub fn apply_env_whitelist(cmd: &mut Command, strict: bool) {
    cmd.env_clear();
    if strict {
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        return;
    }
    for var in PRESERVED_ENV_VARS {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }
    #[cfg(windows)]
    for var in PRESERVED_ENV_VARS_WINDOWS {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }
}

pub fn is_strict_env_clear() -> bool {
    matches!(
        std::env::var("SQLITE_GRAPHRAG_STRICT_ENV_CLEAR")
            .ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("True")
            | Some("yes") | Some("YES")
    )
}
```

#### Passo 2 — Refatorar os três spawners para delegar ao helper

```rust
// src/commands/claude_runner.rs (modificado)
use crate::spawn::env_whitelist::{apply_env_whitelist, is_strict_env_clear};

// Em build_claude_command, substituir o loop manual de env_clear + whitelist:
cmd.env_clear();
apply_env_whitelist(&mut cmd, is_strict_env_clear());
cmd.env("CLAUDE_CONFIG_DIR", claude_config_dir); // runtime override após helper

// src/commands/codex_spawn.rs (idêntico)
// src/commands/ingest_claude.rs (idêntico)
```

#### Passo 3 — Adicionar flag CLI `--strict-env-clear`

```rust
// src/cli.rs (modificado)
#[arg(long, env = "SQLITE_GRAPHRAG_STRICT_ENV_CLEAR", global = true)]
pub strict_env_clear: bool,
```

#### Passo 4 — Criar `tests/claude_runner_env.rs` com 5 cenários seriais

```rust
// tests/claude_runner_env.rs (novo, 311 linhas, ADR-0041 §Verification)
#[test] #[serial(env)]
fn claude_subprocess_inherits_custom_anthropic_provider_env() { /* placeholder */ }

#[test] #[serial(env)]
fn claude_subprocess_rejects_prohibited_anthropic_api_key() {
    // SAFETY: serial_test::serial(env)
    unsafe {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-violation-test");
    }
    let output = AssertCmd::new(cargo_bin!("sqlite-graphrag"))
        .args(["remember", "--name", "test-v183-rejection", "--body", "x"])
        .env("PATH", path_with_mock)
        .env("ANTHROPIC_API_KEY", "sk-ant-violation-test")
        .timeout(Duration::from_secs(30))
        .output().expect("spawn");
    // OAuth-only guard aborta com exit != 0
    assert!(!output.status.success());
}

#[test] #[serial(env)]
fn codex_subprocess_inherits_openai_base_url() { /* integração codex */ }

#[test] #[serial(env)]
fn strict_env_clear_drops_custom_provider_credentials() { /* modo compliance */ }

#[test] #[serial(env)]
fn audit_no_token_leak_in_subprocess_stderr() {
    let secret = "sk-cp-secret-XYZ-12345";
    unsafe { std::env::set_var("ANTHROPIC_AUTH_TOKEN", secret); }
    let output = AssertCmd::new(cargo_bin!("sqlite-graphrag"))
        .args(["remember", "--name", "test-v183-no-leak", "--body", "x"])
        .env("ANTHROPIC_AUTH_TOKEN", secret)
        .env("RUST_LOG", "trace")
        .output().expect("spawn");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains(secret));
    assert!(!stderr.contains(secret));
}
```

#### Passo 5 — Documentar em ADR-0041 (EN + PT-BR)

```bash
# Estrutura do ADR segue template de ADR-0011 e ADR-0025
# Status: Accepted
# Date: 2026-06-17
# Context: env_clear remove credenciais customizadas em 3 spawners
# Decision: preservar 7 vars customizadas no whitelist compartilhado
# Consequences: providers customizados funcionam; OAuth-only intacto;
#   compliance opt-in via --strict-env-clear; DRY achieved via helper
# Alternatives Considered: flag opt-in (rejeitado por fricção);
#   apenas documentar workaround (rejeitado por não resolver causa raiz);
#   refator completo (fora do escopo)
# Related: ADR-0011, ADR-0025, ADR-0033, gap-g58-recall-sem-fallback-deterministic-2026-06-13
```

### Relações causa × efeito

- **CAUSA**: `env_clear()` em três spawners → **EFEITO**: subprocesso perde credenciais customizadas → **EFEITO SECUNDÁRIO**: API MiniMax retorna `401` → **EFEITO TERCIÁRIO**: `claude -p` sai com exit 1 → **EFEITO QUATERNÁRIO**: `claude_runner::generate_embedding` retorna `AppError::EmbeddingFailed` (exit 11) → **EFEITO QUINTO**: `remember` aborta após gravar memória parcial → **EFEITO FINAL**: estado inconsistente — linha em `memories` sem embedding em `memory_embeddings`
- **CAUSA**: codex CLI lê `~/.codex/auth.json` (filesystem), **EFEITO**: orquestrador não precisa preservar `OPENAI_API_KEY` se auth.json existe; **MAS**: provider customizado via `OPENAI_BASE_URL` AINDA exige env preservation
- **CAUSA**: gap G58 já documentado em 2026-06-13 (`gap-g58-recall-sem-fallback-deterministic-2026-06-13` — embedding ao vivo é ponto único de falha sob fadiga OAuth), **EFEITO**: este fix resolve G58 S5 (provider customizado via env contorna fadiga OAuth oficial, sem precisar de modelo local)

### Evidências observadas

- 2026-06-17: provider MiniMax (api.minimax.io) retorna `401 Invalid authentication credentials` quando `ANTHROPIC_AUTH_TOKEN` está definido no ambiente mas não chega ao subprocesso (transcript de produção)
- 2026-06-17: verificação via 3 Explore agents confirma que os 3 spawners têm whitelist idêntico e igualmente incompleto; nenhum dos três inclui `ANTHROPIC_AUTH_TOKEN` na v1.0.82
- 2026-06-17: `cargo clippy --all-targets -- -D warnings` passa com helper novo; 8 testes seriais OAuth-only pré-existentes permanecem verdes (defesa em profundidade intacta)
- 2026-06-17: `tests/claude_runner_env.rs` com 5 cenários seriais valida propagação via mock scripts em TempDir; auditoria de no-leak confirma ausência do valor literal do token em stderr com `RUST_LOG=trace`

### Notas

- A causa raiz NÃO é: bug do subprocesso Claude/Codex; o guard OAuth-only corretamente rejeita `ANTHROPIC_API_KEY`/`OPENAI_API_KEY` (chaves pagas, semântica distinta das customizadas)
- A causa raiz NÃO é: o guard OAuth-only estar errado; o guard interpreta LITERALMENTE as vars que pretende rejeitar (ADR-0011)
- A causa raiz É: interpretação GENERALIZADA do mandato OAuth-only que acabou removendo INDIRETAMENTE credenciais customizadas legítimas via `env_clear()` em vez de fazer rejeição EXPLÍCITA via guard
- A solução é ortogonal a GAP-001 (estágios com checkpoint), GAP-002 (contrato JSON de shutdown), GAP-003 (escolha de backend), GAP-004 (coordenação cross-process) e GAP-005 (fallback de embedding) — pode ser combinada com todas as cinco
- A solução resolve o caso concreto do transcript MiniMax 2026-06-17 sem alterar defesa em profundidade OAuth-only
- A solução NÃO introduz regressão: usuários que NÃO definem as vars customizadas têm comportamento idêntico à v1.0.82; usuários que DEFINEM passam a ver seus providers funcionando
- A solução preserva compatibilidade cross-platform: helper tem `#[cfg(windows)]` separado para `PRESERVED_ENV_VARS_WINDOWS` (LOCALAPPDATA, APPDATA, etc.)
- A solução alinha com o princípio de `usuário escolhe, sistema obedece` — usuário escolhe provider customizado via env vars, sistema preserva fielmente
- A solução habilita degradação graciosa para providers oficiais sob fadiga OAuth: usuário configura `ANTHROPIC_AUTH_TOKEN` apontando para gateway próprio, sistema roteia sem tocar no OAuth oficial
- A solução melhora observabilidade via teste de auditoria de no-leak: valor literal do token NUNCA aparece em stderr, validado via `RUST_LOG=trace`
- A solução é compatível com a arquitetura LLM-Only da v1.0.76: zero dependências novas, zero modelos locais, zero daemon
- A solução referencia cross-cutting gaps: G45 (coordenação de remember cross-process — S1 file lock, S2 write-behind, S3 fan-out bounded) e G58 (fallback de recall sob fadiga OAuth) parcialmente resolvidos
- A solução é compatível com `pending_embeddings` do GAP-005: corpo com embedding pendente continua funcionando; apenas muda a disponibilidade de providers
