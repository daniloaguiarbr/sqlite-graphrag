# Invocação Headless — Claude Code, Codex, OpenCode sem MCP e sem Hooks

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
  --model sonnet \
  --max-turns 8 \
  --output-format json
```

### O Que Cada Pedaço Faz

- `--strict-mcp-config` ignora MCP de settings global e de projeto
- `--mcp-config '{}'` fornece a lista vazia que zera os servidores
- `--dangerously-skip-permissions` evita travar pedindo confirmação (modo `bypassPermissions`)
- `--settings '{"hooks":{}}'` desliga os hooks naquela chamada específica
- `--model sonnet` escolhe o modelo sem depender de variável de ambiente
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
codex exec --ignore-user-config --sandbox workspace-write "SUA TAREFA AQUI"
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
