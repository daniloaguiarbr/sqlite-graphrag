# Análise de Defeitos — sqlite-graphrag


## GAP-SPAWN-001: Herança de .mcp.json em Subprocessos de Embedding — RESOLVIDO (v1.0.91, 2026-06-23)

### Resolução
- `apply_cwd_isolation()` adicionado em `src/spawn/mod.rs` — aplica `current_dir(temp_dir)` e `CLAUDE_CONFIG_DIR=temp_dir` em TODOS os subprocessos LLM
- 10 spawn sites corrigidos: `llm_embedding.rs` (3), `codex_spawn.rs` (1), `claude_runner.rs` (1), `opencode_runner.rs` (2), `ingest_claude.rs` (1), `enrich.rs` (1), preflight `workspace_root` (1)
- Testes de regressão: `test_spawn_isolation_dir_creates_in_temp`, `test_apply_cwd_isolation_modifies_command`
- 877 testes passando, ZERO clippy warnings, ZERO erros de formatação


## Problema
- O `sqlite-graphrag` falha silenciosamente ou com timeout ao executar embedding via subprocessos `codex exec` ou `claude -p`
- O subprocesso LLM herda o `.mcp.json` do diretório de trabalho do chamador
- Servidores MCP do projeto (ex: `pg-flowaiper`, `ssh-flowaiper-farmacia`) tentam inicializar dentro do subprocesso headless
- O subprocesso headless NÃO precisa de NENHUM servidor MCP — ele só precisa gerar embeddings


## Consequências do Problema
- Timeout de 60s no backend Codex porque o subprocesso tenta conectar ao PostgreSQL MCP antes de processar o prompt de embedding
- Erro 401 no backend Claude porque o subprocesso herda config MCP que exige autenticação de servidores externos
- O usuário precisa descobrir manualmente que deve prefixar `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1 CLAUDE_CONFIG_DIR=/tmp/graphrag-empty-config` em TODA invocação
- Primeira experiência do usuário com `remember` ou `recall` falha em QUALQUER projeto que tenha `.mcp.json` na árvore de diretórios
- A workaround exige conhecimento interno da arquitetura — viola o princípio de menor surpresa (POLA)
- O preflight guard `check_walkup_mcp_json` (v1.0.87) valida se o JSON é sintaticamente válido, mas NÃO impede que servidores MCP válidos causem interferência no subprocesso
- Embedding falha com exit 11 e a memória é persistida sem embedding (`backend_invoked: "none"`) — degradação silenciosa de qualidade de busca semântica


## Causa Raiz do Problema
- CAUSA PRIMÁRIA: o subprocesso LLM (`codex exec` ou `claude -p`) faz walk-up do filesystem buscando `.mcp.json` na cadeia ancestral do CWD
- CAUSA SECUNDÁRIA: o `sqlite-graphrag` executa o subprocesso com `Command::new()` SEM chamar `.current_dir()` — o subprocesso herda o CWD do chamador, que contém `.mcp.json` do projeto do usuário
- CAUSA TERCIÁRIA: as flags de endurecimento passam `--mcp-config '{}'` para zerar servidores MCP, MAS o Claude Code 2.1.177+ ignora o inline `'{}'` e faz walk-up mesmo assim
- CAUSA QUATERNÁRIA: o `check_mcp_config_inline` (`preflight.rs:276`) reescreve `'{}'` para tempfile com `{"mcpServers":{}}`, porém o walk-up do CWD PREVALECE sobre o `--mcp-config` quando ambos existem
- CAUSA QUINÁRIA: `workspace_root: std::path::Path::new(".")` em `llm_embedding.rs:739` ancora o walk-up do preflight no CWD do chamador em vez de um diretório efêmero limpo

### Evidência no Código-Fonte

Arquivo `src/extract/llm_embedding.rs`:
- Linha 739: `workspace_root: std::path::Path::new(".")` — o preflight ancora no CWD
- Linhas 760-762: `env_clear()` limpa env vars, mas NÃO define `current_dir()`
- Linha 769: `cmd.env("CLAUDE_CONFIG_DIR", &config_dir)` — só ativo quando env var manual existe

Arquivo `src/spawn/preflight.rs`:
- Linhas 330-371: `check_walkup_mcp_json` faz walk-up a partir de `workspace_root` (`.`) buscando `.mcp.json`
- Linha 358-363: rejeita `.mcp.json` com `mcpServers` não-vazio — MAS só se o preflight NÃO for pulado
- O guard detecta o problema mas a solução exige env var manual (`SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1`)

### Cadeia Causal Completa

```
CWD do usuário contém .mcp.json
        |
        v
sqlite-graphrag spawna subprocesso LLM com Command::new()
        |
        v
subprocesso herda CWD do chamador (nenhum .current_dir() definido)
        |
        v
claude/codex faz walk-up e encontra .mcp.json do projeto
        |
        v
MCP servers do projeto (PostgreSQL, SSH, etc) tentam inicializar
        |
        v
servidores MCP exigem rede, auth, ou portas indisponíveis no contexto headless
        |
        v
timeout (codex 60s) OU 401 auth error (claude) OU conexão recusada
        |
        v
embedding falha com exit 11
        |
        v
memória NÃO é persistida OU é persistida sem embedding (backend_invoked: "none")
        |
        v
recall e hybrid-search retornam resultados degradados ou vazios
```


## Solução
- O `sqlite-graphrag` DEVE isolar o CWD do subprocesso LLM para um diretório temporário limpo que NÃO contenha `.mcp.json`
- Implementar `Command::new("codex").current_dir(temp_dir)` onde `temp_dir` é um diretório efêmero sem `.mcp.json` na cadeia ancestral
- Complementar com `CLAUDE_CONFIG_DIR` apontando para diretório vazio para evitar herança de config user-level
- Tornar esse comportamento o DEFAULT — sem exigir env vars manuais do usuário


## Benefícios da Solução
- Embedding funciona em primeira execução em QUALQUER projeto, independente da presença de `.mcp.json`
- Elimina a necessidade do workaround `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1 CLAUDE_CONFIG_DIR=/tmp/graphrag-empty-config`
- Reduz tempo de embedding removendo tentativas falhas de conexão a servidores MCP irrelevantes
- O preflight guard `check_walkup_mcp_json` se torna redundante para o caso de interferência (mantém valor para diagnóstico)
- Experiência zero-config para o usuário final
- Alinhamento com a Lei Transversal de Timeout Explícito das rules-rust: o timeout de 60s deixa de ser desperdiçado com inicialização de MCP irrelevante
- Alinhamento com POLA (Principle of Least Astonishment) das rules-rust: `remember` funciona sem surpresas


## Como Solucionar

### Passo 1 — Criar diretório efêmero no spawn
- Em `src/extract/llm_embedding.rs`, antes de `Command::spawn()`
- Criar diretório via `std::env::temp_dir().join(format!("sqlite-graphrag-spawn-{}", std::process::id()))`
- O diretório DEVE estar em um path sem `.mcp.json` em NENHUM ancestral
- Recomendação: `/tmp/sqlite-graphrag-spawn-<PID>/` — `/tmp` NUNCA terá `.mcp.json`

### Passo 2 — Definir CWD do subprocesso
```rust
// ANTES (herda CWD do chamador — causa raiz do bug)
let mut cmd = Command::new(&self.binary);
cmd.arg("exec")
    .env_clear()
    .env("PATH", std::env::var("PATH").unwrap_or_default())
    .env("HOME", std::env::var("HOME").unwrap_or_default())
    // ... demais args
    .spawn()

// DEPOIS (isola CWD para diretório limpo)
let spawn_dir = std::env::temp_dir()
    .join(format!("sqlite-graphrag-spawn-{}", std::process::id()));
std::fs::create_dir_all(&spawn_dir)?;

let mut cmd = Command::new(&self.binary);
cmd.current_dir(&spawn_dir)  // ISOLAMENTO: walk-up não encontra .mcp.json
    .arg("exec")
    .env_clear()
    .env("PATH", std::env::var("PATH").unwrap_or_default())
    .env("HOME", std::env::var("HOME").unwrap_or_default())
    // ... demais args
    .spawn()
```

### Passo 3 — Definir CLAUDE_CONFIG_DIR no env do subprocesso
```rust
// Para o backend Claude: definir CLAUDE_CONFIG_DIR para diretório limpo
// SEMPRE, não apenas quando a env var manual existe
cmd.current_dir(&spawn_dir)
    .env("CLAUDE_CONFIG_DIR", &spawn_dir)
    // ... demais args
```

### Passo 4 — Atualizar workspace_root do preflight
```rust
// ANTES (ancora no CWD do chamador)
let preflight_args = PreFlightArgs {
    workspace_root: std::path::Path::new("."),
    // ...
};

// DEPOIS (ancora no diretório efêmero limpo)
let preflight_args = PreFlightArgs {
    workspace_root: &spawn_dir,
    // ...
};
```

### Passo 5 — Cleanup do diretório efêmero
- Remover `spawn_dir` após o subprocesso terminar
- O diretório é reutilizável entre invocações do mesmo processo (mesmo PID)
- Cleanup no `Drop` ou via `scopeguard` para garantir remoção mesmo em panic

### Passo 6 — Remover necessidade de env vars manuais
- O `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` NÃO deve ser necessário para uso normal
- O `CLAUDE_CONFIG_DIR=/tmp/graphrag-empty-config` NÃO deve ser necessário para uso normal
- Manter ambas as env vars como override manual para casos extremos, mas o default DEVE funcionar sem elas

### Passo 7 — Aplicar a TODOS os spawn sites
- `src/extract/llm_embedding.rs` — 3 funções: `invoke_claude`, `invoke_opencode`, `build_codex_embedding_command`
- `src/commands/codex_spawn.rs` — spawn de codex para extração
- `src/commands/claude_runner.rs` — spawn de claude para extração
- `src/commands/opencode_runner.rs` — spawn de opencode para extração
- `src/commands/ingest_claude.rs` — ingest com extração LLM
- `src/commands/enrich.rs` — enrich com LLM

### Passo 8 — Testes de regressão
- Criar teste que valida que subprocesso com `.mcp.json` no CWD NÃO herda servidores MCP
- Criar teste que valida que `current_dir` aponta para diretório efêmero sem `.mcp.json`
- Atualizar testes existentes em `src/spawn/preflight.rs` que usam `workspace_root: dir.path()` para cobrir cenário de isolamento


## Evidência do Incidente (2026-06-23)

### Ambiente
- Projeto: `web_flowaiper_farmacia`
- `.mcp.json` presente com servidor `pg-flowaiper` (PostgreSQL MCP stdio)
- `sqlite-graphrag` v1.0.90
- `codex-cli` v0.141.0
- `claude` v2.1.186

### Sequência de falhas observada
- `--llm-backend codex --llm-model gpt-5.4-mini` — timeout após 60s (exit 11)
- `--llm-backend claude --llm-model claude-sonnet-4-6` — 401 Invalid authentication credentials (exit 11)
- `--llm-backend opencode --llm-model opencode/big-pickle` — mesmo 401 (exit 11)

### Workaround que funcionou
```bash
SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1 CLAUDE_CONFIG_DIR=/tmp/graphrag-empty-config \
  sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name test --type note --description "x" --graph-stdin --force-merge
```
- `backend_invoked: "codex"` — sucesso em 81s

### Prova de conceito do isolamento
- `codex exec` funciona quando invocado diretamente SEM herdar `.mcp.json`
- `SELECT 1` no MCP PostgreSQL funciona independentemente — o problema NÃO é de rede
- O timeout ocorre DENTRO do subprocesso tentando inicializar servidores MCP, NÃO na geração de embedding


## Relações Causa × Efeito

| Causa | Efeito |
|-------|--------|
| `Command::new()` sem `.current_dir()` | Subprocesso herda CWD com `.mcp.json` |
| Walk-up de `.mcp.json` pelo Claude/Codex | Servidores MCP do projeto inicializam no contexto headless |
| Servidores MCP exigem rede/auth indisponível | Timeout ou erro 401 no subprocesso |
| Timeout/erro 401 no subprocesso | Embedding falha com exit 11 |
| Embedding falha | Memória persistida sem embedding (score 0.0) |
| Memória sem embedding | `recall` e `hybrid-search` retornam resultados degradados |
| Resultados degradados | Usuário perde confiança na ferramenta |
| Workaround exige env vars manuais | Viola POLA e aumenta barreira de entrada |
| `workspace_root: Path::new(".")` no preflight | Guard detecta mas não resolve o problema |


## Regras Rust Aplicáveis
- Lei Transversal 4 — Timeout explícito em toda operação de subprocesso: o timeout de 60s é desperdiçado com inicialização de MCP irrelevante
- Lei Transversal 5 — stdout dados / stderr logs / exit codes específicos: exit 11 é genérico demais para este caso
- POLA — comportamento consistente com expectativa do leitor: `remember` deveria funcionar sem surpresas
- Fail Fast — detecção de erro no ponto mais próximo da origem: o erro deveria ser detectado ANTES do spawn, não após 60s de timeout
- KISS — escolher a solução mais direta: `current_dir(temp_dir)` é a correção mais simples e direta


## BUG-14: Teste opencode_adapter_build_args assertava string incorreta — RESOLVIDO (v1.0.91, 2026-06-23)

### Problema
- O teste `opencode_adapter_build_args` em `tests/spawn_version_adapter.rs:106` assertava `args.contains(&"headless".to_string())`
- O `OpencodeAdapter::build_args()` NUNCA retornou `"headless"` — retorna `["run", "--format", "json", "--dangerously-skip-permissions", prompt]`
- Bug pré-existente desde v1.0.90 (commit 978e370) — o adapter foi refatorado mas o teste NÃO foi atualizado

### Causa Raiz
- O adapter originalmente usava `"headless"` como subcomando do OpenCode
- A implementação foi alterada para usar `"run"` mas o teste permaneceu com a string antiga

### Correção
- `tests/spawn_version_adapter.rs:106`: `"headless"` substituído por `"run"`
- 877 testes passando após a correção

### Relações Causa x Efeito
| Causa | Efeito |
|-------|--------|
| Refatoração do adapter sem atualizar teste | Teste assertando string inexistente |
| Teste falhando | Suite de testes reporta 1 failure |


## GAP-SPAWN-002: Diretórios de spawn órfãos acumulam em /tmp — RESOLVIDO (v1.0.91, 2026-06-23)

### Resolução
- `cleanup_spawn_dir()` adicionado em `src/main.rs` — remove diretório de spawn do PID atual ao final da execução
- Cleanup executado em TODOS os caminhos de saída: sucesso, erro e shutdown
- `std::fs::remove_dir()` (não-recursivo) — seguro: falha silenciosamente se não estiver vazio

### Problema
- A função `spawn_isolation_dir()` cria diretórios `/tmp/sqlite-graphrag-spawn-{PID}/` para cada processo
- Cada invocação da CLI cria um PID diferente e portanto um diretório diferente
- Os diretórios NÃO são removidos automaticamente após o subprocesso terminar
- Acúmulo observado: 240 diretórios vazios (40 bytes cada, 9.6 KB total)

### Impacto
- BAIXO: impacto de disco negligível (40 bytes por diretório)
- Os diretórios são limpos automaticamente pelo sistema operacional no reboot (tmpfs)
- Nenhum dado sensível nos diretórios (estão vazios)

### Causa Raiz
- `spawn_isolation_dir()` chama `create_dir_all()` mas NÃO implementa cleanup no Drop ou após spawn
- O design original priorizou KISS sobre cleanup — o diretório é reutilizado dentro do mesmo PID via `create_dir_all` idempotente

### Solução Proposta
- Opção A: cleanup no final de `main()` via `std::fs::remove_dir()` (não recursivo, seguro para diretórios vazios)
- Opção B: usar `tempfile::TempDir` com Drop automático (mais robusto mas muda a assinatura do helper)
- Opção C: não corrigir — impacto negligível, tmpfs limpa no reboot

### Relações Causa x Efeito
| Causa | Efeito |
|-------|--------|
| `create_dir_all()` sem cleanup | Diretórios vazios acumulam em `/tmp` |
| PID diferente por invocação | Cada invocação cria diretório novo |
| tmpfs limpa no reboot | Impacto limitado à sessão de uptime |


## BUG-15: Enum `backend_invoked` incompleta em 7 JSON schemas — RESOLVIDO (v1.0.91, 2026-06-23)

### Problema
- 7 JSON schemas em `docs/schemas/` declaravam `backend_invoked` com enum `["claude", "codex", "none"]`
- O código em `src/commands/embedding.rs` e outros módulos retorna `"opencode"` e `"auto"` desde v1.0.90
- Consumidores que validam contra schema rejeitariam respostas válidas com `backend_invoked: "opencode"` ou `"auto"`

### Causa
- OpenCode backend (v1.0.90) adicionou `"opencode"` ao enum de runtime mas NÃO atualizou os schemas
- `"auto"` nunca foi incluído nos schemas originais (v1.0.82) embora exista no código desde a criação

### Correção
- 7 schemas atualizados de `["claude", "codex", "none"]` para `["claude", "codex", "opencode", "none", "auto"]`
- Arquivos: `embedding-status`, `enrich-summary`, `hybrid-search`, `recall`, `remember`, `ingest-summary`, `edit`

### Relações Causa x Efeito
| Causa | Efeito |
|-------|--------|
| OpenCode backend (v1.0.90) sem atualização de schemas | Schema rejeita `"opencode"` válido |
| `"auto"` omitido desde v1.0.82 | Schema rejeita `"auto"` retornado pelo code path Auto |
| Enum restritiva em 7 arquivos | Validação JSON Schema falha em 2 de 5 valores possíveis |


## BUG-16: Campo `vec_degraded` ausente no schema `deep-research.schema.json` — RESOLVIDO (v1.0.91, 2026-06-23)
- Severidade: MÉDIA
- Impacto: validação estrita de JSON Schema falha para o output de `deep-research`
- Descoberto por: suite `schema_contract_strict` (teste `schema_36_deep_research`)

### Problema
- O struct `ResearchStats` em `src/commands/deep_research.rs:202` declara `vec_degraded: bool`
- O campo é serializado SEMPRE (sem `skip_serializing_if`)
- O schema `docs/schemas/deep-research.schema.json` NÃO declarava `vec_degraded` em `ResearchStats`
- Schema usa `additionalProperties: false` (política Must-Validate)
- Resultado: validador rejeita o output real com `AdditionalProperties { unexpected: ["vec_degraded"] }`

### Causa Raiz
- Campo `vec_degraded` adicionado ao struct Rust sem atualização correspondente no schema JSON
- Suite `schema_contract_strict` requer feature `slow-tests` e NÃO roda no CI padrão

### Correção
- Adicionado `"vec_degraded": { "type": "boolean" }` ao `ResearchStats` no schema
- Adicionado `"vec_degraded"` ao array `required` do `ResearchStats`
- Arquivo: `docs/schemas/deep-research.schema.json`

### Relações Causa x Efeito
| Causa | Efeito |
|-------|--------|
| Campo adicionado ao struct sem atualizar schema | Violação `additionalProperties: false` |
| Suite de validação gated por feature flag | Bug não detectado no CI padrão |
| Schema Must-Validate para deep-research | Validação estrita rejeita campo não declarado |


## BUG-17: `entities.degree` inflado por `increment_degree` em `remember` e `ingest` — RESOLVIDO (v1.0.91, 2026-06-23)

### Severidade: ALTA

### Sintoma
- `graph stats` reporta `max_degree` diferente de `graph entities[].degree` para a mesma entidade
- `graph stats` usa campo `degree` armazenado na tabela `entities` (inflado)
- `graph entities` recalcula via subquery `COUNT(*) FROM relationships` (correto)
- Exemplo: entidade `audit-r4` com 2 relações reais mostra `degree=3` na tabela (aparecia em 3 chamadas de `remember`)

### Causa Raiz
- `remember.rs:931` e `ingest.rs:862` chamavam `increment_degree()` dentro do loop de entidades
- `increment_degree()` incrementa cegamente +1 por entidade por memória, MESMO quando a entidade NÃO participa de nenhuma relação naquela chamada
- Além disso, o `increment_degree` rodava ANTES da inserção de relações — mesmo para entidades COM relações, o grau era calculado sem considerar as relações da chamada atual
- Os subcomandos `link`, `unlink`, `delete-entity`, `merge-entities` usavam `recalculate_degree()` corretamente

### Correção
- `remember.rs`: removido `increment_degree` do loop de entidades; adicionado collect de `affected_entity_ids` (entidades + endpoints de relações); `recalculate_degree` chamado para TODAS as entidades afetadas DEPOIS da inserção de TODAS as relações
- `ingest.rs`: mesma correção aplicada — `recalculate_degree` APÓS inserção de relações
- GAP-17 warning (`max_entity_degree`) movido para DEPOIS do recálculo com grau correto
- Verificação E2E: entidade `shared-entity` com 3 relações reais mostra `degree=3` em `graph stats`, `graph entities` E tabela `entities` — todos consistentes

### Relações Causa x Efeito
| Causa | Efeito |
|-------|--------|
| `increment_degree` em vez de `recalculate_degree` | `degree` armazenado infla a cada `remember` sem relação real |
| `recalculate_degree` antes da inserção de relações | Grau calculado sem considerar relações da chamada atual |
| `graph stats` usa campo armazenado | `max_degree` inflado — dados de observabilidade incorretos |
| `health` warnings de super-hub usam campo armazenado | Falsos positivos de `super_hub_warning` |
| Divergência entre `graph stats` e `graph entities` | Inconsistência visível ao consumidor da API |
