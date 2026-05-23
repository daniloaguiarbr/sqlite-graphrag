# sqlite-graphrag para Agentes de IA


> Memória persistente para 27 agentes de IA em um único binário Rust de 25 MB

- Leia a versão em inglês em [AGENTS.md](AGENTS.md)


## Aliases de Flags CLI (desde v1.0.35)
- `recall` e `hybrid-search` aceitam `--limit` como alias de `-k`/`--k`. Os snippets abaixo usam `--k` e continuam válidos.
- `rename` aceita `--from`/`--to` como aliases de `--name`/`--new-name`.
- `rename` aceita argumentos posicionais: `rename <antigo> <novo>` (desde v1.0.44)
- `related` aceita argumento posicional de nome: `related <nome>` (desde v1.0.44)
- `graph entities` JSON response usa `entities` como chave de array top-level (renomeado de `items` em v1.0.44)
- Campos JSON `schema_version` (`init`, `stats`, `migrate`, `health`) são emitidos como números JSON desde v1.0.35.


## Novos Comandos em v1.0.56
### Manutenção do Índice FTS5
- `fts rebuild --json` — reconstrói o índice full-text FTS5 do zero; use após importações em massa ou suspeita de corrupção do índice
- `fts check --json` — executa integrity-check do FTS5 e reporta inconsistências; seguro para rodar em bancos em uso
- `fts stats --json` — retorna estatísticas do índice FTS5 incluindo contagem de linhas, tokens e segmentos
### Backup
- `backup --output <path> --json` — cria backup consistente do SQLite usando a SQLite Backup API; seguro para rodar com o banco em uso; destino existente é substituído atomicamente via tempfile-rename
### Operações de Entidade
- `delete-entity --name <entity> --json` — deleta um nó de entidade; use `--cascade` para remover também todas as arestas conectadas à entidade; sem `--cascade` falha com exit 4 se houver arestas
- `reclassify --name <entity> --new-type <type> --json` — altera o `entity_type` de uma entidade existente in place sem tocar nas arestas ou links de memória
- `merge-entities --names "a,b,c" --into <target> --json` — funde dois ou mais nós de entidade em um nó alvo; todas as arestas dos nós fonte são redirecionadas para o alvo; nós fonte são deletados após a fusão
- `memory-entities --name <memory> --json` — lista todos os nós de entidade vinculados a uma dada memória; retorna o mesmo schema dos itens de `graph entities`
- `prune-ner --entity <name> --json` — remove todos os bindings derivados de NER para um dado nome de entidade sem deletar o nó da entidade; útil para limpar entidades extraídas automaticamente com baixa qualidade

## Novidades na v1.0.58
### Correções de Bugs
- `remember --force-merge` agora sincroniza o índice FTS5 após atualização — anteriormente cada force-merge corrompia silenciosamente o índice de busca textual (correção CRÍTICA)
- `merge-entities` usa `UPDATE OR IGNORE` para tabela `memory_entities` — corrige falhas de UNIQUE constraint quando entidades compartilham vínculos com memórias
### Novos Comandos e Funcionalidades
- `rename-entity --name <antigo> --new-name <novo> --json` — renomeia entidade preservando todos os relacionamentos e vínculos; re-gera vetor com o novo nome
- `memory-entities --entity <nome> --json` — busca reversa: lista todas as memórias vinculadas a uma entidade (complementando a direção existente memória→entidades)
- `reclassify --name <entidade> --description "texto" --json` — atualiza descrição da entidade no modo individual (anteriormente só o tipo podia ser alterado)
### Melhorias
- Resposta do `purge` agora inclui campo `action` (`"purged"` ou `"dry_run"`) para consistência com demais comandos
- Validação de nomes de entidade rejeita nomes com quebras de linha, menores que 2 caracteres, ou abreviações ALL_CAPS curtas (prevenção de ruído NER)
- `fts --help` mostra seção EXAMPLES para subcomandos
- Comando `health` emite `tracing::info!` nos checkpoints para debugging com `-vv`
- `reclassify --help` lista todos os tipos de entidade válidos
- Campo JSON de `history --diff` se chama `changes` (contendo `added_chars` e `removed_chars`), não `diff`


## A Pergunta Que Nenhum Framework Responde
### Open Loop — Por Que 27 Agentes de IA Escolhem Esta Como Sua Camada de Memória
- Por que 27 agentes de IA escolhem sqlite-graphrag como sua camada de memória persistente?
- Três razões técnicas: memória local durável, zero dependências cloud, JSON determinístico
- Cada agente ganha memória persistente sem gastar um único token adicional
- Versus MCPs pesados, sqlite-graphrag entrega contrato stdin/stdout determinístico
- O segredo que os frameworks jamais documentam mora em um único arquivo SQLite portátil


## Por Que Agentes Amam Esta CLI
### Cinco Diferenciais — Projetados Para Loops Autônomos
- Saída JSON determinística elimina cada hack de parser no código de orquestração
- Exit codes seguem `sysexits.h` para sua lógica de retry funcionar sem casar string
- Nenhum runtime Python ou Node acompanha a binária Rust da CLI
- Stdin aceita payloads estruturados para seus agentes jamais escaparem argumentos shell
- Comandos pesados de embedding podem subir e reutilizar `sqlite-graphrag daemon` automaticamente em vez de pagar cold-start em cada loop
- Comportamento cross-platform permanece idêntico em Linux macOS e Windows desde o início
- O comportamento padrão sempre cria ou abre `graphrag.sqlite` no diretório atual


## Economia Que Converte
### Números Que Vendem A Troca
- Remova dependências recorrentes de bancos vetoriais cloud nos fluxos locais de agentes
- Mantenha o retrieval local na workstation ou no runner de CI em vez de uma stack RAG remota
- Reduza a superfície operacional para um arquivo SQLite e uma CLI
- Reuse o daemon nos comandos pesados em vez de pagar cold-start completo em cada loop
- Preserve a orquestração determinística com JSON estável e exit codes estáveis


## Soberania Como Vantagem Competitiva
### Por Que Memória Local Vence Em 2026
- Seus dados proprietários NUNCA saem da workstation do desenvolvedor ou do runner de CI
- Sua superfície de compliance encolhe para um arquivo SQLite sob sua própria criptografia
- Seu lock-in de fornecedor desaparece porque o schema é documentado e portátil
- Sua trilha de auditoria mora na tabela `memory_versions` com histórico imutável
- Sua indústria regulada ganha RAG offline-first sem cláusulas de dependência cloud


## Agentes e Orquestradores Compatíveis
### Catálogo — 27 Integrações Suportadas
| Agente | Fornecedor | Versão Mínima | Tipo de Integração | Exemplo |
| --- | --- | --- | --- | --- |
| Claude Code | Anthropic | 1.0+ | Subprocess | `sqlite-graphrag recall "query" --json` |
| Codex CLI | OpenAI | 0.5+ | AGENTS.md + subprocess | `sqlite-graphrag remember --name X --type user --description "..." --body "..."` |
| Gemini CLI | Google | recente | Subprocess | `sqlite-graphrag hybrid-search "query" --json --k 5` |
| Opencode | open source | recente | Subprocess | `sqlite-graphrag recall "auth flow" --json --k 3` |
| OpenClaw | comunidade | recente | Subprocess | `sqlite-graphrag recall "auth flow" --json --k 3` |
| Paperclip | comunidade | recente | Subprocess | `sqlite-graphrag read --name onboarding-note --json` |
| VS Code Copilot | Microsoft | 1.90+ | tasks.json | `{"command": "sqlite-graphrag", "args": ["recall", "$selection", "--json"]}` |
| Google Antigravity | Google | recente | Runner | `sqlite-graphrag hybrid-search "prompt" --k 10 --json` |
| Windsurf | Codeium | recente | Terminal | `sqlite-graphrag recall "plano refactor" --json` |
| Cursor | Cursor | 0.40+ | Terminal | `sqlite-graphrag remember --name cursor-ctx --type project --description "..." --body "..."` |
| Zed | Zed Industries | recente | Assistant Panel | `sqlite-graphrag recall "abas abertas" --json --k 5` |
| Aider | open source | 0.60+ | Shell | `sqlite-graphrag recall "alvo refactor" --k 5 --json` |
| Jules | Google Labs | preview | automação CI | `sqlite-graphrag stats --json` |
| Kilo Code | comunidade | recente | Subprocess | `sqlite-graphrag recall "tarefas recentes" --json` |
| Roo Code | comunidade | recente | Subprocess | `sqlite-graphrag hybrid-search "contexto repo" --json` |
| Cline | comunidade | extensão VS Code | Terminal | `sqlite-graphrag list --limit 20 --json` |
| Continue | open source | VS Code ou JetBrains | Terminal | `sqlite-graphrag recall "docstring" --json` |
| Factory | Factory | recente | API ou subprocess | `sqlite-graphrag recall "contexto pr" --json` |
| Augment Code | Augment | recente | IDE | `sqlite-graphrag hybrid-search "code review" --json` |
| JetBrains AI Assistant | JetBrains | 2024.2+ | IDE | `sqlite-graphrag recall "stacktrace" --json` |
| OpenRouter | OpenRouter | qualquer | Roteador multi-LLM | `sqlite-graphrag recall "regra roteamento" --json` |
| Minimax | Minimax | recente | Subprocess | `sqlite-graphrag recall "preferencias usuario" --json --k 5` |
| Z.ai | Z.ai | recente | Subprocess | `sqlite-graphrag hybrid-search "contexto tarefa" --json --k 10` |
| Ollama | Ollama | 0.1+ | Subprocess | `sqlite-graphrag remember --name ollama-ctx --type project --description "..." --body "..."` |
| Hermes Agent | comunidade | recente | Subprocess | `sqlite-graphrag recall "historico tool call" --json` |
| LangChain | LangChain | 0.3+ | Subprocess via tool | `sqlite-graphrag hybrid-search "contexto chain" --json --k 5` |
| LangGraph | LangChain | 0.2+ | Subprocess via node | `sqlite-graphrag recall "estado grafo" --json --k 3` |


## Detalhes de Integração por Agente
### Minimax
- Agente multimodal open-source com raciocínio em vídeo áudio e texto
- Invoque sqlite-graphrag como subprocess dentro de uma definição de tool Minimax:
```bash
sqlite-graphrag recall "user session context" --json --k 5
```
- Saída: JSON com entradas em `results` contendo `name`, `snippet`, `distance` e `source`

### Z.ai
- Plataforma de agentes hospedada com planejamento multi-etapa e orquestração de tools
- Invoque sqlite-graphrag para persistir memória entre sessões de planejamento:
```bash
sqlite-graphrag remember --name "task-plan-$(date +%s)" --type project --description "plano de tarefa Z.ai" --body "$PLAN"
sqlite-graphrag recall "previous task plan" --json --k 3
```
- Saída: JSON determinístico com `results`, `direct_matches` e `graph_matches`

### Ollama
- Servidor LLM local rodando modelos abertos em hardware consumer sem cloud
- Invoque sqlite-graphrag como tool para dar aos agentes Ollama conhecimento persistente:
```bash
sqlite-graphrag recall "conversation history" --json --k 5
sqlite-graphrag remember --name "ollama-session" --type project --description "sessão Ollama" --body "$CONTEXT"
```
- Saída: JSON determinístico de recall com `elapsed_ms` e campos estáveis de resultado

### Hermes Agent
- Framework de agente comunitário projetado para loops de tool-calling no estilo ReAct
- Invoque sqlite-graphrag no início de cada ciclo ReAct para carregar contexto anterior:
```bash
sqlite-graphrag hybrid-search "tool call results" --json --k 5
```
- Saída: JSON hybrid-search combinando BM25 full-text e ranking vetorial por cosseno

### LangChain
- Framework Python de orquestração LLM com abstrações de chains tools e retrievers
- Invoque sqlite-graphrag como tool de retriever customizado via subprocess do Python:
```bash
sqlite-graphrag hybrid-search "chain input query" --json --k 10 --lang en
```
- Saída: array JSON `results` consumível via `json.loads` no wrapper de tool LangChain

### LangGraph
- Framework de máquina de estado baseado em grafo para workflows multi-agente sobre LangChain
- Invoque sqlite-graphrag dentro de cada nó do grafo para persistir e recuperar estado:
```bash
sqlite-graphrag recall "graph node output" --json --k 3
sqlite-graphrag remember --name "node-result-$(date +%s)" --type project --description "resultado do nó LangGraph" --body "$OUTPUT"
```
- Saída: JSON estruturado para travessia stateful entre execuções de LangGraph


## Integrações com Crates Rust
### Crates de Agente e LLM — Chame sqlite-graphrag como Subprocess
- Todo crate Rust que spawna um agente LLM pode chamar sqlite-graphrag via `std::process::Command`
- Recall por subprocesso determinístico permite a crates Rust reutilizarem um contrato estável de memória
- Zero tokens adicionais: memória vive no SQLite não dentro da janela de contexto
- Cada crate ganha memória persistente sem importar nenhuma dependência do sqlite-graphrag

### rig-core
- Framework modular para construir pipelines LLM sistemas RAG e agentes autônomos
- Cargo.toml:
```toml
[dependencies]
rig-core = "0.35.0"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "project context", "--json"])
    .output()?;
```
- Caso de uso: persistir resultados de tools de agente entre invocações do pipeline rig

### swarms-rs
- Framework de orquestração multi-agente com suporte MCP nativo e topologias de swarm
- Cargo.toml:
```toml
[dependencies]
swarms-rs = "0.2.1"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "swarm task result", "--json", "--k", "5"])
    .output()?;
```
- Caso de uso: compartilhar contexto persistente entre agentes do swarm sem vector DB central

### autoagents
- Runtime multi-agente com atores Ractor loops ReAct e isolamento WASM sandbox
- Cargo.toml:
```toml
[dependencies]
autoagents = "0.3.7"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "react-step", "--type", "agent", "--body", "step output"])
    .output()?;
```
- Caso de uso: salvar checkpoint de etapas ReAct para replay e auditoria em loops autoagents

### agentai
- Camada de agente fina sobre genai com abstração ToolBox simples para registro de tools
- Cargo.toml:
```toml
[dependencies]
agentai = "0.1.5"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "tool call context", "--json", "--k", "3"])
    .output()?;
```
- Caso de uso: injetar histórico de tool calls anteriores no ToolBox antes de cada execução

### llm-agent-runtime
- Runtime completo de agente com memória episódica checkpointing e orquestração de tools
- Cargo.toml:
```toml
[dependencies]
llm-agent-runtime = "1.74.0"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "episode context", "--json"])
    .output()?;
```
- Caso de uso: estender memória episódica do llm-agent-runtime com persistência SQLite durável

### anda
- Framework de agentes para ambientes TEE e integrações blockchain com ICP
- Cargo.toml:
```toml
[dependencies]
anda = "0.4.10"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["read", "--name", "anda-agent-state", "--json"])
    .output()?;
```
- Caso de uso: persistir estado verificável do agente fora do TEE para continuidade entre sessões

### adk-rust
- Kit modular de desenvolvimento de agentes inspirado nos padrões LangChain e Autogen
- Cargo.toml:
```toml
[dependencies]
adk-rust = "0.6.0"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "agent memory query", "--json", "--k", "10"])
    .output()?;
```
- Caso de uso: substituir o store de contexto em memória do adk-rust por recall por grafo persistente

### genai
- Cliente API unificado para OpenAI Anthropic Gemini xAI e Ollama em um único crate
- Cargo.toml:
```toml
[dependencies]
genai = "0.6.0-beta.17"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "llm response cache", "--json"])
    .output()?;
```
- Caso de uso: armazenar respostas custosas do genai para reutilização em execuções seguintes

### liter-llm
- Cliente LLM universal com suporte a 143 ou mais provedores e rastreamento OpenTelemetry
- Cargo.toml:
```toml
[dependencies]
liter-llm = "1.2.1"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "litellm-trace", "--type", "agent", "--body", "trace payload"])
    .output()?;
```
- Caso de uso: armazenar snapshots de trace OpenTelemetry no sqlite-graphrag para replay de agente

### llm-cascade
- Cliente LLM em cascata com failover automático e circuit breaker entre provedores
- Cargo.toml:
```toml
[dependencies]
llm-cascade = "0.1.0"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "fallback provider result", "--json"])
    .output()?;
```
- Caso de uso: persistir decisões de cascata para que o circuit breaker aprenda com falhas anteriores

### async-openai
- Cliente async nativo Rust para a API REST completa da OpenAI com modelos type-safe
- Cargo.toml:
```toml
[dependencies]
async-openai = "0.34.0"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "openai assistant output", "--json", "--k", "5"])
    .output()?;
```
- Caso de uso: armazenar mensagens de thread de assistente para recall durável entre sessões

### anthropic-sdk
- Cliente Rust direto para a API Anthropic incluindo tool use e respostas streaming
- Cargo.toml:
```toml
[dependencies]
anthropic-sdk = "0.1.5"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "claude conversation context", "--json"])
    .output()?;
```
- Caso de uso: injetar turnos anteriores da conversa Claude antes de cada chamada à API

### ollama-rs
- Cliente Rust idiomático para a API do servidor de inferência local Ollama
- Cargo.toml:
```toml
[dependencies]
ollama-rs = "0.3.4"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "ollama-output", "--type", "agent", "--body", "generated text"])
    .output()?;
```
- Caso de uso: persistir outputs do ollama-rs para recuperação em chamadas de inferência seguintes

### llama-cpp-rs
- Bindings Rust para llama.cpp para inferência on-device com modelos quantizados
- Cargo.toml:
```toml
[dependencies]
llama-cpp-rs = "0.3.0"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "on-device inference context", "--json", "--k", "5"])
    .output()?;
```
- Caso de uso: carregar contexto persistente no prompt do llama-cpp-rs antes de cada inferência local

### mistralrs
- Engine de inferência local de alta performance para modelos Mistral com suporte a quantização
- Cargo.toml:
```toml
[dependencies]
mistralrs = "0.8.1"
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "mistral inference context", "--json", "--k", "5"])
    .output()?;
```
- Caso de uso: injetar contexto persistente do sqlite-graphrag nos prompts do mistralrs antes da inferência

### graphbit
- Engine de workflow baseado em grafo para orquestração determinista de pipelines LLM em Rust
- Cargo.toml:
```toml
[dependencies]
graphbit = { git = "https://github.com/graphbit-rs/graphbit" }
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "workflow node state", "--json", "--k", "3"])
    .output()?;
```
- Caso de uso: persistir outputs de nós do workflow graphbit para travessia stateful entre execuções

### rs-graph-llm
- Workflows de grafo tipados e interativos para pipelines LLM com segurança em tempo de compilação
- Cargo.toml:
```toml
[dependencies]
rs-graph-llm = { git = "https://github.com/rs-graph-llm/rs-graph-llm" }
```
- Integração com sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "graph node output", "--json", "--k", "5"])
    .output()?;
```
- Caso de uso: armazenar resultados tipados do rs-graph-llm para memória persistente entre execuções


## Princípios Fundamentais
### OBRIGATÓRIO — Filosofia de Uso
- TRATAR sqlite-graphrag como camada local de memória persistente
- INVOCAR sempre como subprocesso via `std::process::Command`
- LER stdout para dados estruturados em JSON ou NDJSON
- LER stderr para logs de tracing e mensagens humanas
- VERIFICAR exit code antes de parsear stdout
- PRESERVAR contexto entre sessões via arquivo SQLite único
- DELEGAR memória de longo prazo ao binário sem reimplementar
### PROIBIDO — Anti-padrões
- NUNCA expor o binário como servidor MCP ou serviço HTTP
- NUNCA depender de vector DB cloud como Pinecone ou Weaviate
- NUNCA escrever direto no SQLite paralelo ao binário
- NUNCA editar o arquivo `.sqlite` com outra ferramenta
- NUNCA assumir saída sem validar exit code antes
- NUNCA confundir `distance` com `combined_score` no ranking
- NUNCA misturar stdout estruturado com logs humanos
- NUNCA usar `fd | xargs remember` quando `ingest` cobre o caso


## Inicialização e Verificação de Saúde
### OBRIGATÓRIO — Bootstrap do Banco
- EXECUTAR `sqlite-graphrag init --namespace <projeto>` no primeiro uso
- AGUARDAR download offline do modelo `multilingual-e5-small`
- VALIDAR com `sqlite-graphrag health --json` antes de operar
- TRATAR exit code 10 como erro de database ou banco corrompido
- TRATAR exit code 15 como lock pendente, ampliar `--wait-lock`
- ABORTAR pipeline quando `integrity_ok` retornar `false`
- RODAR `migrate --json` após cada upgrade do binário
### OBRIGATÓRIO — Verificação Contínua
- INSPECIONAR `wal_size_mb` no `health` para detectar fragmentação
- CONFERIR `journal_mode` igual a `wal` em produção
- RODAR `optimize --json` para refrescar estatísticas do planner
- DETECTAR deriva de schema via `__debug_schema` em troubleshooting
- VERIFICAR `mentions_ratio` (float) e `mentions_warning` (string) no `health --json` quando relacionamentos `mentions` dominam o grafo acima de 50%
### Padrão Correto — Sequência de Bootstrap
- `sqlite-graphrag init --namespace meu-projeto`
- `sqlite-graphrag health --json | jaq '.integrity_ok'`
- `sqlite-graphrag migrate --json`
- `sqlite-graphrag stats --json | jaq '.memories'`


## Configuração Global
### OBRIGATÓRIO — Caminho do Banco
- USAR `--db <PATH>` quando o banco não está no diretório atual
- DEFINIR `SQLITE_GRAPHRAG_DB_PATH` para configuração persistente
- LEMBRAR que `--db` tem precedência sobre a variável de ambiente
- PADRÃO é `graphrag.sqlite` no diretório atual de invocação
### OBRIGATÓRIO — Namespace
- DEFINIR namespace via `--namespace` ou `SQLITE_GRAPHRAG_NAMESPACE`
- VALIDAR resolução com `namespace-detect --json`
- USAR `global` como namespace padrão quando ausente
- DESDE v1.0.51 TODOS os comandos respeitam `SQLITE_GRAPHRAG_NAMESPACE`; anteriormente `list`, `read`, `edit`, `forget`, `history`, `rename`, `restore` e `remember` o ignoravam
- ISOLAR projetos via namespace por repositório
- ADOTAR `swarm-<agent_id>` para enxames multi-agente
### OBRIGATÓRIO — Idioma da Saída
- USAR `--lang en` ou `--lang pt` para forçar idioma
- DEFINIR `SQLITE_GRAPHRAG_LANG=pt` para override de sessão
- LEMBRAR que `--lang` afeta apenas stderr humano
- STDOUT JSON permanece determinístico independente do idioma
### OBRIGATÓRIO — Fuso Horário de Exibição
- APLICAR `--tz America/Sao_Paulo` em saídas localizadas
- USAR `SQLITE_GRAPHRAG_DISPLAY_TZ=<IANA>` para persistir
- AFETA apenas campos `*_iso` no JSON
- CAMPOS epoch inteiros permanecem em UTC
- ABORTAR quando nome IANA inválido retorna exit 2 (parsing de argumentos Clap)
### OBRIGATÓRIO — Formato de Logs
- ATIVAR `SQLITE_GRAPHRAG_LOG_FORMAT=json` para agregadores
- PADRÃO `pretty` serve apenas para humanos no terminal
- ELEVAR detalhe via `SQLITE_GRAPHRAG_LOG_LEVEL=debug` em diagnóstico
- USAR `-v`, `-vv`, `-vvv` para info, debug e trace nos subcomandos
### OBRIGATÓRIO — Controle de Memória RAM Global
- ATIVAR `SQLITE_GRAPHRAG_LOW_MEMORY=1` em containers restritos
- APLICAR em hosts com menos de 4 GB de RAM disponível
- HONRA cgroup constraints automaticamente quando definido
- TRADE-OFF é 3 a 4 vezes mais tempo de wall clock
- COMBINAR com flag `--low-memory` em `ingest` específico
### OBRIGATÓRIO — ONNX Runtime em ARM64 GNU
- DISTRIBUIR `libonnxruntime.so` ao lado da binária
- DEFINIR `ORT_DYLIB_PATH` explicitamente em CI e systemd
- AFETA comandos pesados de embedding em `aarch64-unknown-linux-gnu`
- FALHA na primeira operação de embedding sem o runtime acessível


## CRUD — Create com remember
### OBRIGATÓRIO — Escrita de Memórias Individuais
- USAR nome kebab-case único por memória
- DECLARAR `--type` entre `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- PREFERIR `--body-stdin` para corpos longos
- USAR `--body-file <PATH>` para evitar escape shell em Markdown
- PASSAR `--force-merge` em loops idempotentes; também restaura memórias soft-deleted e atualiza em um passo (desde v1.0.51); `--type` e `--description` são opcionais com `--force-merge` — valores existentes são herdados quando omitidos
- USAR `--dry-run` para validar o payload (tamanho do body, schema de entidades/relacionamentos, unicidade do nome) sem persistir nada; retorna 0 em sucesso, não-zero em falha de validação
- USAR `--clear-body` com `--force-merge` para definir explicitamente o body como string vazia em vez de herdar o body existente
- NER desabilitado por padrão; passar `--enable-ner` ou definir `SQLITE_GRAPHRAG_ENABLE_NER=1` para ativar extração GLiNER
- `--skip-extraction` está obsoleto desde v1.0.45 e não tem efeito; NER está desabilitado por padrão, use `--enable-ner` para ativar
- Campo de resposta `extraction_method` informa o método utilizado: `gliner-<variant>+regex` (GLiNER bem-sucedido), `regex-only` (GLiNER indisponível ou desabilitado), ou `none:extraction-failed` (GLiNER tentado mas com erro)
- RESPEITAR limite de 512000 bytes e 512 chunks por body
- USAR `--max-rss-mb <MiB>` para limitar o RSS do processo durante embedding (padrão: 8192 MiB); aborta com exit 77 se excedido
### OBRIGATÓRIO — Anexar Grafo no remember
- USAR `--entities-file` com array JSON tipado
- USAR `--relationships-file` para arestas tipadas
- INCLUIR campo `entity_type` em cada objeto de entidade
- ACEITAR `type` como sinônimo, nunca os dois juntos
- USAR `strength` entre `0.0` e `1.0` em relationships
- MAPEAR `from`/`to` como aliases de `source`/`target`
- USAR `--graph-stdin` para JSON único com `body`, `entities` e `relationships`
### PROIBIDO — Erros de Escrita
- NUNCA enviar `entity_type` e `type` no mesmo objeto JSON
- NUNCA usar `strength` fora do intervalo `[0.0, 1.0]`
- NUNCA duplicar nome sem `--force-merge` explícito
- NUNCA misturar `--body`, `--body-file`, `--body-stdin`, `--graph-stdin`
- NUNCA depender de auto-extração GLiNER em CI sensível a RAM
- NUNCA exceder o cap de relações por memória sem ajustar env
- NUNCA usar `remember` em loop quando `ingest` cobre o caso
### Padrão Correto — Exemplos de remember
- `sqlite-graphrag remember --name design-auth --type decision --description "auth JWT" --body-stdin < doc.md`
- `sqlite-graphrag remember --name doc-readme --type document --description "import" --body-file README.md --force-merge`
- `sqlite-graphrag remember --name spec-x --type reference --description "spec" --body "..." --entities-file ents.json --relationships-file rels.json`
### Valores Válidos de --type
- `user`, `feedback`, `project`, `reference`
- `decision`, `incident`, `skill`, `document`, `note`


## CRUD — Bulk Ingest com ingest
### OBRIGATÓRIO — Quando Usar ingest
- USAR `ingest <DIR>` para importar diretórios inteiros como memórias
- PREFERIR sobre loop `fd | xargs remember` em qualquer caso
- USAR `ingest --dry-run` para visualizar o mapeamento arquivo→nome sem carregar o modelo ONNX nem persistir nada
- A saída de `--dry-run` é NDJSON com `status: "preview"` por arquivo; use para detectar truncamentos e colisões antes de confirmar
- CADA arquivo correspondente ao pattern vira memória individual
- NOME da memória deriva do basename do arquivo sem extensão em kebab-case
- NOMES com mais de 60 caracteres são TRUNCADOS automaticamente
- NDJSON inclui `truncated: true` e `original_name` quando trunca
- AGENTE deve usar `original_name` ou `name` do NDJSON para acessar a memória
- SAÍDA é NDJSON, uma linha JSON por arquivo mais uma linha summary final
- CONSUMIR linha a linha em streaming via `jaq -c` ou `while read`
### OBRIGATÓRIO — Padrão de Arquivos com --pattern
- PADRÃO é `*.md` apenas, mude conforme necessário
- ACEITA `*.<ext>` para extensão genérica
- ACEITA `<prefixo>*` para prefixo de basename
- ACEITA filename exato sem caracteres glob
- GLOB completo POSIX não é suportado pelo ingest
### OBRIGATÓRIO — Recursão e Limites
- LIGAR `--recursive` para descer em subdiretórios
- SEM `--recursive` apenas top-level é processado
- RESPEITAR `--max-files 10000` como cap padrão de segurança
- `--max-files` REJEITA a operação inteira com exit 1 se contagem exceder o cap
- `--max-files` NÃO limita aos primeiros N, é validação all-or-nothing
- AUMENTAR cap apenas após auditoria de volume real
- USAR `--fail-fast` para parar na primeira falha por arquivo
- SEM `--fail-fast` o loop continua e reporta cada erro no NDJSON
### OBRIGATÓRIO — Tipo de Memória em Massa
- DECLARAR `--type` aplicado a TODOS os arquivos da invocação
- PADRÃO é `document` quando omitido
- VALORES válidos: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- INVOCAR `ingest` separadamente por tipo quando misturar
- AGRUPAR arquivos por diretório conforme o tipo desejado
### OBRIGATÓRIO — Controle de Memória RAM
- USAR `--low-memory` em containers com menos de 4 GB
- DEFINIR `SQLITE_GRAPHRAG_LOW_MEMORY=1` como override persistente
- `--low-memory` força `--ingest-parallelism 1` internamente
- TRADE-OFF é 3 a 4 vezes mais tempo de execução
- ESCOLHER quando RSS for restrição maior que latência
- USAR `--max-rss-mb <MiB>` para abortar se o RSS do processo exceder o limite durante embedding (padrão: 8192 MiB)
### OBRIGATÓRIO — Dois Eixos de Paralelismo
- `--max-concurrency <N>` controla CLI invocations simultâneas
- `--ingest-parallelism <N>` controla extract mais embed em paralelo
- PADRÃO de `--max-concurrency` é 4
- PADRÃO de `--ingest-parallelism` é `min(4, max(1, cpus/2))`
- DISTINGUIR claramente os dois eixos antes de ajustar
- AMPLIAR `--wait-lock <SECONDS>` para esperar slot antes de exit 75
### OBRIGATÓRIO — Performance e Extração
- NER desabilitado por padrão; passar `--enable-ner` para ativar extração GLiNER
- `--skip-extraction` está obsoleto desde v1.0.45 e não tem efeito; NER está desabilitado por padrão, use `--enable-ner` para ativar
- Campo de resposta `extraction_method` informa o método utilizado: `gliner-<variant>+regex` (GLiNER bem-sucedido), `regex-only` (GLiNER indisponível ou desabilitado), ou `none:extraction-failed` (GLiNER tentado mas com erro)
- GLiNER NER adiciona aproximadamente 100-200 ms por arquivo com modelo carregado em hardware moderno
- GLiNER NER adiciona 2 a 30 segundos por arquivo em `--low-memory` ou no primeiro carregamento
- GLiNER NER baixa o modelo ONNX no primeiro run (fp32: 1,1 GB, int8: 349 MB via `--gliner-variant`)
- USAR `--enable-ner` apenas quando enriquecimento automático de entidades for valioso
- PREFERIR `--graph-stdin` com entidades curadas por LLM para melhor qualidade (NER desabilitado por padrão)
### PROIBIDO — Anti-padrões de ingest
- NUNCA usar `fd | xargs sqlite-graphrag remember` quando `ingest` existe
- NUNCA omitir `--recursive` esperando descida automática
- NUNCA passar pattern com glob complexo não suportado
- NUNCA ignorar exit 75 de slot exausto em loops automatizados
- NUNCA misturar tipos diferentes na mesma invocação
- NUNCA elevar `--max-files` sem medir RAM e disco antes
- NUNCA usar `--force-merge` no ingest (flag exclusiva do `remember`)
### Padrão Correto — Exemplos de ingest
- `sqlite-graphrag ingest ./docs --recursive --pattern "*.md" --json`
- `sqlite-graphrag ingest ./decisoes --type decision --json`
- `sqlite-graphrag ingest ./large-corpus --low-memory --max-files 50000 --json`
- `sqlite-graphrag ingest ./skills --type skill --recursive --fail-fast --json`
- `sqlite-graphrag ingest ./notas --type note --pattern "memo-*" --recursive --json`
### Padrão Correto — Consumo do NDJSON
- `sqlite-graphrag ingest ./docs --recursive --json | jaq -c 'select(.status == "indexed")'`
- `sqlite-graphrag ingest ./docs --recursive --json | tee resultados.ndjson`
- NDJSON contém `files_total + 1` linhas: uma por arquivo mais uma summary final
- FILTRAR por `select(.status)` para ignorar a summary line que não tem campo `status`
- `jaq -sc '[.[] | select(.status)] | group_by(.status) | map({status: .[0].status, count: length})' < resultados.ndjson`
### OBRIGATÓRIO — Schema NDJSON por Tipo de Linha
- Linha por arquivo: `file`, `name`, `status` (`"indexed"` `"skipped"` `"failed"` `"preview"`), `truncated`, `original_name?`, `original_filename?`, `memory_id?`, `action?`, `error?`
- `original_filename` preserva o basename do arquivo antes da normalização kebab-case; presente quando o basename difere do nome derivado (ex.: espaços, acentos, caracteres especiais)
- Linha summary final: `summary` (true), `dir`, `pattern`, `recursive`, `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- Eventos de extração NER vão para stderr, NÃO stdout
### OBRIGATÓRIO — Modos de Ingestão (v1.0.62)
- USAR `--mode none` (padrão) para ingestão body-only sem extração
- USAR `--mode gliner` para extração NER local com GLiNER (requer `--enable-ner`)
- USAR `--mode claude-code` para extração curada por LLM via Claude Code CLI instalado localmente
- Modo Claude Code requer binário `claude` >= 2.1.0 no PATH com assinatura Pro/Max ativa
- USAR `--resume` para continuar ingestão claude-code interrompida a partir do queue DB
- USAR `--retry-failed` para retentar apenas arquivos com falha
- USAR `--max-cost-usd <N>` para definir teto de orçamento — ingestão para quando custo acumulado exceder
- USAR `--claude-binary <PATH>` para especificar caminho explícito do binário Claude Code
- USAR `--claude-model <MODEL>` para sobrescrever o modelo (ex.: `claude-sonnet-4-6`)
- USAR --claude-timeout <S> para definir timeout por arquivo (padrão 300s); mata processos claude -p travados
- Eventos NDJSON por arquivo no modo claude-code incluem campos `entities`, `rels`, `cost_usd`
- Queue DB `.ingest-queue.sqlite` rastreia progresso por arquivo; usar `--keep-queue` para reter após conclusão
- Rate limit: backoff exponencial automático (60s → 120s → 300s → 900s)
- `--mode codex` spawna `codex exec --json` por arquivo para extração curada via OpenAI Codex CLI
- Requer Codex CLI instalado; usa `--output-schema` para saída JSON estruturada
- Flags Codex: `--codex-binary`, `--codex-model`, `--codex-timeout` (padrão 300s)
- Variável de ambiente `SQLITE_GRAPHRAG_CODEX_BINARY` sobrescreve busca no PATH
- Pipeline completo de embedding aplicado para recall e hybrid-search
### Padrão Correto — Exemplos de Ingestão Claude Code
- `sqlite-graphrag ingest ./docs --mode claude-code --recursive --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --resume --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --max-cost-usd 5.00 --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --claude-model claude-sonnet-4-6 --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --claude-timeout 600 --max-cost-usd 10.00 --json`
### Padrão Correto — Exemplos de Ingestão Codex
- `sqlite-graphrag ingest ./docs --mode codex --recursive --json`
- `sqlite-graphrag ingest ./docs --mode codex --codex-model o4-mini --json`
- `sqlite-graphrag ingest ./docs --mode codex --codex-timeout 600 --json`
- `sqlite-graphrag ingest ./docs --mode codex --codex-binary /usr/local/bin/codex --json`


## CRUD — Read com read e list
### OBRIGATÓRIO — Leitura Direta por Nome (read)
- USAR `read --name <kebab-case>` para fetch O(1) por nome
- PARSEAR campos `body`, `description`, `created_at_iso`, `updated_at_iso`
- TRATAR exit code 4 como memória inexistente no namespace
- APLICAR `--tz` para localizar timestamps na saída
### OBRIGATÓRIO — Enumeração com Filtros (list)
- USAR `list --type <kind>` para filtrar por tipo de memória
- PADRÃO de limit é TODAS as memórias quando `--json` está ativo; padrão é 50 para saída em texto
- AJUSTAR `--limit <N>` para limitar resultados quando o padrão JSON (todos) for muito amplo
- PAGINAR via `--offset <N>` para datasets grandes
- INCLUIR memórias soft-deletadas via `--include-deleted`
- EXPORTAR full dump com `--limit 10000 --json` antes de backup
- RESPOSTA inclui `total_count` (total de linhas correspondentes ignorando o limit), `truncated` (true quando limit foi aplicado) e `body_length` por item (tamanho em bytes do body armazenado)
### OBRIGATÓRIO — Export em Streaming (export)
- USAR `export` para transmitir todas as memórias como NDJSON para backup ou migração portátil
- SUPORTA `--namespace`, `--type`, `--include-deleted`, `--limit` e `--offset`
- SAÍDA é NDJSON: uma linha JSON por memória mais uma linha summary final
- REDIRECIONAR para arquivo para backup: `sqlite-graphrag export --limit 1000 > backup.ndjson`
- FILTRAR por tipo e namespace: `sqlite-graphrag export --type decision --namespace meu-projeto > decisoes.ndjson`
### Padrão Correto — Exemplos de Leitura
- `sqlite-graphrag read --name design-auth --json`
- `sqlite-graphrag list --type decision --limit 100 --json`
- `sqlite-graphrag list --include-deleted --json | jaq '.items[] | select(.deleted)'`


## CRUD — Update com edit, rename e restore
### OBRIGATÓRIO — Edição de Corpo e Descrição (edit)
- USAR `edit --name <nome> --body <texto>` para corpos curtos
- PREFERIR `--body-file` ou `--body-stdin` para corpos longos
- ALTERAR descrição via `--description <texto>`
- CADA edit cria nova versão imutável preservando histórico
- VALIDAR exit code 3 como conflito de locking otimista
- JSON response: `memory_id`, `name`, `action` ("updated"), `version`, `elapsed_ms`
### OBRIGATÓRIO — Renomeação Preservando Histórico (rename)
- USAR `rename --name <antigo> --new-name <novo>`
- ACEITAR `--old`/`--new` e `--from`/`--to` como aliases desde v1.0.35
- PRESERVAR todas as versões e conexões do grafo
- TRATAR exit code 4 como memória de origem ausente
- JSON response: `memory_id`, `name` (novo), `action` ("renamed"), `version`, `elapsed_ms`
### OBRIGATÓRIO — Restauração de Versão Antiga (restore)
- INSPECIONAR versões via `history --name <nome>` primeiro
- USAR `restore --name <nome> --version <N>` para versão específica
- OMITIR `--version` seleciona última versão não-restore automaticamente
- RESTORE cria nova versão sem sobrescrever histórico anterior
- RE-EMBED ocorre automaticamente para recall vetorial voltar a encontrar
- JSON response inclui campo `action: "restored"`, consistente com os demais comandos CRUD
### OBRIGATÓRIO — Locking Otimista
- PASSAR `--expected-updated-at <epoch_ou_RFC3339>` em pipelines concorrentes
- TRATAR exit code 3 como concorrência detectada
- RECARREGAR `read --json` para obter novo `updated_at` antes de retentar
- APLICAR locking em `edit`, `rename` e `restore`
### Padrão Correto — Fluxos de Update
- `sqlite-graphrag edit --name design-auth --body-file ./revisado.md --expected-updated-at "2026-04-19T12:00:00Z"`
- `sqlite-graphrag rename --from nome-antigo --to nome-novo`
- `sqlite-graphrag history --name design-auth --json && sqlite-graphrag restore --name design-auth --version 2`


## CRUD — Delete com forget, purge, unlink e cleanup-orphans
### OBRIGATÓRIO — Remoção Lógica (forget)
- USAR `forget --name <nome>` para soft-delete reversível
- MEMÓRIA desaparece de `recall` e `list` por padrão
- HISTÓRICO de versões permanece intacto no banco
- REVERSÍVEL via `restore` enquanto não houver purge
- JSON response: `action` (`"soft_deleted"` `"already_deleted"`), `forgotten`, `name`, `namespace`, `deleted_at?`, `deleted_at_iso?`, `elapsed_ms`
- Desde v1.0.52: quando a memória não é encontrada, `forget` não emite mais JSON para stdout; apenas mensagem de erro em stderr e exit code 4 são produzidos
### OBRIGATÓRIO — Remoção Física (purge)
- USAR `purge --retention-days <N> --yes` em automação
- PADRÃO de retenção é 90 dias para memórias soft-deletadas
- EXECUTAR `--dry-run` primeiro para auditar contagem
- APAGA permanentemente linhas e reclama espaço em disco
### OBRIGATÓRIO — Remoção de Aresta (unlink)
- USAR `unlink --from <a> --to <b> --relation <tipo>`
- ACEITAR `--source`/`--target` como aliases de `--from`/`--to`
- TRATAR exit code 4 como aresta inexistente
- `--relation` agora é OPCIONAL; omitir remove TODOS os relacionamentos entre o par independente do tipo
- NOVO MODO: `unlink --entity <name> --all` remove todas as arestas (em ambas as direções) de uma dada entidade em uma única chamada
### OBRIGATÓRIO — Limpeza de Entidades Órfãs (cleanup-orphans)
- EXECUTAR `cleanup-orphans --dry-run` para auditar
- APLICAR `--yes` em pipelines automatizados
- REMOVE entidades sem memórias vinculadas nem arestas
- RODAR periodicamente após operações `forget` em massa
### OBRIGATÓRIO — Poda em Massa de Relacionamentos (prune-relations)
- USAR `prune-relations --relation <tipo> --yes` para remover em massa todos os relacionamentos de um tipo
- USAR `--dry-run` para visualizar a contagem antes de confirmar
- USAR `--show-entities` durante `--dry-run` para incluir `affected_entity_names` na resposta
- EXECUTAR `cleanup-orphans` depois para remover entidades sem relacionamentos restantes
### Padrão Correto — Round-Trip Forget e Restore
- `sqlite-graphrag forget --name decisao-x`
- `sqlite-graphrag history --name decisao-x --json | jaq '.deleted'`
- `sqlite-graphrag restore --name decisao-x`
- `sqlite-graphrag recall "decisão" --json`


## Histórico Imutável de Versões
### OBRIGATÓRIO — Inspeção com history
- USAR `history --name <nome> --json` para listar versões
- VERSÕES começam em 1 e incrementam a cada `edit` ou `restore`
- ORDEM cronológica reversa por padrão
- INCLUI memórias soft-deletadas com flag `deleted: true`
### OBRIGATÓRIO — Semântica de Versões
- CADA `edit` cria nova versão imutável preservando anteriores
- CADA `restore` cria nova versão com corpo de versão antiga
- AUDIT TRAIL completo de quem mudou o que e quando
- RETENTION POLICY controla quando purgar definitivamente
### Padrão Correto — Auditoria de Mudanças
- `sqlite-graphrag history --name design-auth --json | jaq '.versions[].created_at_iso'`


## Pesquisa GraphRAG
### OBRIGATÓRIO — Quatro Comandos de Busca
- USAR `recall` para busca KNN vetorial com expansão automática de grafo
- USAR `hybrid-search` para fusão de FTS5 e vetorial via RRF
- USAR `related` para travessia multi-hop a partir de memória conhecida
- USAR `graph traverse` para travessia a partir de entidade tipada
- COMBINAR os quatro no padrão de três camadas canônico
### OBRIGATÓRIO — Padrão de Três Camadas Canônico
- CAMADA 1 — `hybrid-search` para encontrar memórias seed por nome
- CAMADA 2 — `read --name` para expandir corpo completo da memória
- CAMADA 3 — `related` ou `graph traverse` para subgrafo multi-hop
- APLICAR camadas em ordem, parando quando contexto basta
- INJETAR resultados consolidados no prompt do LLM
### OBRIGATÓRIO — Camada 1 com hybrid-search
- USAR `hybrid-search <query> --k 10 --rrf-k 60 --json`
- COMBINA FTS5 textual e KNN vetorial via Reciprocal Rank Fusion
- AJUSTAR `--weight-vec` e `--weight-fts` apenas com evidência numérica
- PADRÃO de ambos os pesos é `1.0` com fusão equilibrada
- EXTRAIR apenas `name` via `jaq -r '.results[].name'` para next stage
### OBRIGATÓRIO — hybrid-search com Expansão de Grafo
- ATIVAR travessia de grafo via `--with-graph` para descobrir memórias conectadas
- AJUSTAR profundidade com `--max-hops <N>` (padrão 2)
- FILTRAR arestas fracas com `--min-weight <F>` (padrão 0.3)
- RESULTADOS do grafo ficam em `graph_matches[]`, SEPARADOS de `results[]`
- `graph_matches[]` usa schema RecallItem: `name`, `distance`, `source` ("graph"), `graph_depth`
- LER AMBOS `results[]` e `graph_matches[]` quando `--with-graph` ativo
- EXTRAIR via `jaq -r '(.results[] , .graph_matches[]) | .name'`
### OBRIGATÓRIO — Camada 1 Alternativa com recall
- USAR `recall <query> --k 5 --json` para queries semânticas puras
- ACEITAR `--limit` como alias de `--k` desde v1.0.35
- RECALL expande automaticamente via grafo por padrão
- DESLIGAR expansão automática de grafo via `--no-graph`
- INTERPRETAR `distance` crescente como similaridade decrescente
- INTERPRETAR `score` como `1.0 - distance`, clamped a `[0.0, 1.0]`
- CAMPO `source` indica origem: `"direct"` (KNN) ou `"graph"` (travessia)
- CAMPO `graph_depth` presente apenas em resultados com `source: "graph"`
- RecallResponse separa `direct_matches[]`, `graph_matches[]` e `results[]` (agregado)
- USAR quando query não mistura tokens exatos com linguagem natural
### OBRIGATÓRIO — Camada 2 com read --name
- USAR `read --name <nome>` para obter corpo completo da memória seed
- EXPANDIR contexto além do snippet retornado pela camada 1
- LOOP sobre os top-k nomes para construir bundle de contexto
- PARSEAR campos `body`, `description`, `created_at_iso`
### OBRIGATÓRIO — Camada 3 com related
- USAR `related <nome> --hops <N>` para travessia multi-hop
- DOIS hops revelam conhecimento transitivo invisível à busca vetorial
- DISTÂNCIA de hop entrega sinal explícito ao orquestrador
- USAR quando a query exige raciocínio multi-passo encadeado
### OBRIGATÓRIO — Camada 3 Alternativa com graph traverse
- USAR `graph traverse --from <raiz> --depth <N>` para subgrafo focado
- PADRÃO de profundidade é 2 quando omitido
- TRATAR exit code 4 como entidade raiz inexistente
- HOPS retornam `entity`, `relation`, `direction`, `weight`, `depth`
- PARTIR de entidade tipada, não de nome de memória
### OBRIGATÓRIO — Semântica dos Scores e Distâncias
- `recall` retorna `distance` (menor é mais similar) e `score` (1.0 - distance)
- `recall` retorna `source` (`"direct"` ou `"graph"`) e `graph_depth` (quando graph)
- `hybrid-search` retorna `combined_score`, maior é melhor ranking
- `hybrid-search` expõe `vec_rank` e `fts_rank` para auditar fusão
- `hybrid-search` com `--with-graph` adiciona `graph_matches[]` em campo separado
- `related` retorna `hop_distance`, profundidade explícita no grafo
- `graph traverse` retorna `depth` por hop visitado
- DESCARTAR hits fracos antes de gastar tokens no prompt
### OBRIGATÓRIO — Escolha do Comando por Tipo de Query
- QUERY conceitual ampla, `recall` com `--k 5`
- QUERY mista de tokens e linguagem natural, `hybrid-search` com `--rrf-k 60`
- QUERY mista com contexto de grafo, `hybrid-search --with-graph --max-hops 2`
- QUERY exploratória partindo de memória, `related --hops 2`
- QUERY exploratória partindo de entidade, `graph traverse --depth 2`
- QUERY de auditoria do grafo, `graph entities` ou `graph stats`
### PROIBIDO — Anti-padrões de Pesquisa
- NUNCA usar busca textual nativa SQLite paralela ao binário
- NUNCA confundir `distance` com `combined_score` no ranking
- NUNCA aumentar `--hops` sem inspecionar `graph stats` antes
- NUNCA injetar resultados sem filtrar por threshold de relevância
- NUNCA paralelizar buscas pesadas sem medir RSS do host
- NUNCA pular camada 2 quando o snippet for insuficiente
- NUNCA ler apenas `.results[]` quando `--with-graph` ativo (perderá `graph_matches[]`)
### Padrão Correto — Pipeline Canônico de Três Camadas
- `sqlite-graphrag hybrid-search "auth jwt design" --k 10 --json | jaq -r '.results[].name' > seeds.txt`
- `while read -r nome; do sqlite-graphrag read --name "$nome" --json; done < seeds.txt > corpos.ndjson`
- `sqlite-graphrag related "$(head -n1 seeds.txt)" --hops 2 --json > grafo.json`
- `paste -d '\n' corpos.ndjson <(cat grafo.json) | claude --print`
### Padrão Correto — Pipeline com Expansão de Grafo
- `sqlite-graphrag hybrid-search "auth" --k 5 --with-graph --json | jaq -r '(.results[], .graph_matches[]) | .name' | sort -u > seeds.txt`
### Padrão Correto — Ajuste Fino de Pesos no hybrid-search
- `--weight-vec 1.0 --weight-fts 1.0` igual peso, padrão recomendado
- `--weight-vec 1.0 --weight-fts 0.0` reproduz baseline recall puro
- `--weight-vec 0.0 --weight-fts 1.0` reproduz FTS5 puro
- `--weight-vec 0.7 --weight-fts 0.3` favorece semântica sobre tokens
- `--weight-vec 0.3 --weight-fts 0.7` favorece tokens sobre semântica
### Ganhos Mensurados do Padrão de Três Camadas
- REDUÇÃO de tokens de contexto em até 72x versus dump de markdown
- AUMENTO de accuracy em até 18% sobre vector retrieval puro
- AUMENTO de multi-hop accuracy de 30% a 50% segundo Microsoft
- LATÊNCIA aproximada de 1 segundo em hardware moderno com daemon


## Grafo — Construção e Inspeção
### OBRIGATÓRIO — Criação de Arestas (link)
- USAR `link --from <a> --to <b> --relation <tipo>`
- ENTIDADES devem existir como nós tipados antes do link, exceto com `--create-missing`
- USAR `--create-missing` para auto-criar entidades inexistentes durante o link
- USAR `--entity-type <tipo>` para definir tipo das entidades auto-criadas (padrão `concept`)
- JSON response inclui `created_entities: ["a", "b"]` quando entidades foram criadas
- ACEITAR `--source`/`--target` como aliases de `--from`/`--to`
- DEFINIR `--weight` opcional para peso da relação (padrão 0.5)
- TRATAR exit code 4 como entidade inexistente (sem `--create-missing`)
### OBRIGATÓRIO — Exportação com graph
- EXPORTAR snapshot via `graph --format json`
- USAR `--format dot` para Graphviz offline
- USAR `--format mermaid` para embutir em Markdown
- GRAVAR direto em arquivo via `--output <PATH>`
- INSPECIONAR `nodes` e `edges` no JSON exportado
### OBRIGATÓRIO — Enumeração de Entidades (graph entities)
- USAR `graph entities --json` para listar todas as entidades
- ACESSAR via `jaq -r '.entities[].name'` (campo é `entities`, NÃO `items`)
- FILTRAR por `--entity-type <tipo>` quando necessário
- PAGINAR com `--limit` e `--offset`
- ORDENAR com `--sort-by degree|name|created_at` (padrão `name`) e `--order asc|desc` (padrão `asc`)
- RESPOSTA inclui `degree` por entidade (total de arestas, ambas as direções)
- USAR antes de planejar travessias ou links em lote
### OBRIGATÓRIO — Estatísticas (graph stats)
- USAR `graph stats --json` antes de travessias caras
- INSPECIONAR `node_count`, `edge_count`, `avg_degree`, `max_degree`
- ESCOLHER profundidade de travessia baseada em densidade real
- DETECTAR isolamento de subgrafos antes de planejar buscas
### Vocabulário Canônico de Relações
- `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`
- `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- Tipos customizados de relação (ex.: `implements`, `tested-by`, `blocks`) são aceitos desde v1.0.49; valores não canônicos emitem `tracing::warn!`
### Tipos Válidos de Entidade
- `project`, `tool`, `person`, `file`, `concept`, `incident`
- `decision`, `memory`, `dashboard`, `issue_tracker`
- `organization`, `location`, `date`


## Daemon e Latência Reduzida
### OBRIGATÓRIO — Reuso do Modelo de Embeddings
- INICIAR `sqlite-graphrag daemon` em sessões longas de agente
- VERIFICAR saúde via `daemon --ping --json`
- ENCERRAR via `daemon --stop` ao fim da sessão
- DEIXAR `init`, `remember`, `ingest`, `recall`, `hybrid-search` reusarem automaticamente
- TRATAR daemon como opcional para invocações single-shot
- INSPECIONAR contador de embedding requests no `--ping`
- `daemon --ping` emite um aviso quando a versão do daemon em execução difere da versão do binário CLI; reinicie o daemon após upgrades com `daemon --stop` seguido de `daemon`
- Desde v1.0.50, a CLI reinicia automaticamente um daemon desatualizado em caso de incompatibilidade de versão antes do primeiro request de embedding; `daemon --stop` manual após upgrades não é mais necessário


## Cache — Gestão de Modelos
### OBRIGATÓRIO — Manutenção de Cache
- LISTAR modelos em cache via `cache list --json`
- REMOVER cache de modelos via `cache clear-models --json`
- `clear-models` força re-download na próxima operação de embedding
- USAR `cache list` para diagnosticar uso de disco por modelos ONNX


## Contrato JSON e Pipelines
### OBRIGATÓRIO — Saída Determinística
- USAR `--json` em todos os subcomandos antes de piping
- PREFERIR `--json` sobre `--format json` em one-liners
- FILTRAR campos via `jaq` em vez de regex sobre stdout
- LER apenas campos efetivamente retornados pelo subcomando
- TRATAR JSON como API versionada por SemVer
### OBRIGATÓRIO — Matriz --json versus --format json
- `--json` é aceito por TODOS os subcomandos
- `--format json` aceito apenas em subset com `--format`
- QUANDO ambos presentes, `--json` vence em conflito
- USAR `--json` por padrão em pipelines portáteis
### OBRIGATÓRIO — Distinção Entre JSON e NDJSON
- COMANDOS individuais emitem JSON envelope único no stdout
- `ingest` emite NDJSON, uma linha JSON por arquivo mais summary no stdout
- CONSUMIR NDJSON via `jaq -c` ou `while read -r linha`
- AGREGAR NDJSON em array via `jaq -s` quando necessário
### OBRIGATÓRIO — Campos Críticos por Comando
- `recall` retorna `results[].name`, `snippet`, `distance`, `score`, `source` (`"direct"`/`"graph"`), `graph_depth?`
- `recall` response-level: `query`, `k`, `direct_matches[]`, `graph_matches[]`, `results[]`, `elapsed_ms`
- `hybrid-search` retorna `results[].name`, `combined_score`, `score`, `vec_rank`, `fts_rank`, `source`, `body`, `normalized_score`, `vec_distance`, `fts_bm25`
- `hybrid-search` response-level: `query`, `k`, `rrf_k`, `weights`, `results[]`, `graph_matches[]`, `elapsed_ms`, `fts_degraded`, `fts_error`, `fts_auto_rebuilt`
- `hybrid-search` `graph_matches[]` usa RecallItem: `name`, `distance`, `source` ("graph"), `graph_depth`
- `related` retorna `results[].name`, `hop_distance`, `relation`, `source_entity`, `target_entity`, `weight`
- `graph traverse` retorna `hops[].entity`, `relation`, `direction`, `weight`, `depth`
- `read` retorna `name`, `body`, `description`, `created_at_iso`, `updated_at_iso`
- `edit` retorna `memory_id`, `name`, `action` ("updated"), `version`, `elapsed_ms`
- `rename` retorna `memory_id`, `name` (novo), `action` ("renamed"), `version`, `elapsed_ms`
- `forget` retorna `action` (`"soft_deleted"`/`"already_deleted"`), `forgotten`, `name`, `namespace`, `elapsed_ms`
- `list` response-level: `items[]`, `total_count`, `truncated`, `elapsed_ms`; cada item inclui `body_length` (tamanho em bytes do body armazenado) além dos campos existentes
- `link` response inclui `warnings` (array de strings) para tipos de relação não canônicos ou outros avisos
- `graph entities` itens incluem `degree` (total de arestas da entidade, ambas as direções)
- `health` retorna `integrity_ok`, `schema_ok`, `vec_memories_ok`, `vec_entities_ok`, `vec_chunks_ok`, `fts_ok`, `fts_query_ok`, `model_ok`, `counts`, `wal_size_mb`, `journal_mode`, `db_path`, `db_size_bytes`, `sqlite_version`, `checks[]`; também emite `mentions_ratio` (float) e `mentions_warning` (string) quando arestas `mentions` ultrapassam 50% de todos os relacionamentos
- `health.counts` contém: `memories`, `entities`, `relationships`, `vec_memories`
- `stats` retorna dados GLOBAIS (sem filtro por namespace): `memories`, `entities`, `relationships`, `chunks_total`, `avg_body_len`, `namespaces[]`, `db_size_bytes`, `schema_version`, `elapsed_ms`; também inclui aliases legados `db_bytes`, `edges`, `memories_total`, `entities_total`, `relationships_total`
- `ingest` por arquivo: `file`, `name`, `status` (`"indexed"`/`"skipped"`/`"failed"`/`"preview"`), `truncated`, `original_name?`, `original_filename?`, `memory_id?`, `action?`, `error?`, `body_length?` (tamanho em bytes do body indexado, presente em linhas `"indexed"`)
- `ingest` summary: `summary` (true), `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- `export` por memória: uma linha JSON por memória (NDJSON); linha summary final inclui `exported`, `namespace`, `elapsed_ms`; suporta `--namespace`, `--type`, `--include-deleted`, `--limit`, `--offset`
- `restore` retorna `memory_id`, `name`, `action` ("restored"), `version`, `elapsed_ms`
- `prune-relations` retorna `action` (`"pruned"`/`"dry_run"`), `relation`, `count`, `entities_affected`, `affected_entity_names?`, `namespace`, `elapsed_ms`
- `cache list` retorna modelos com tamanho em bytes e total de disco
- `daemon --ping` retorna os campos existentes mais `model_name` (identificador do modelo de embedding ativo) e `model_variant` (ex.: `"fp32"` ou `"int8"`)


## Envelope JSON de Erro
### OBRIGATÓRIO — Formato de Erro Legível por Máquina
- TODOS os erros emitem um objeto JSON no stdout quando `--json` está ativo: `{"error": true, "code": N, "message": "..."}`
- `code` corresponde ao exit code do processo (veja tabela de Exit Codes)
- `message` é uma string estável em inglês adequada para logging e roteamento
- Stderr continua a carregar saída de tracing legível por humanos independente de `--json`
- Faça parse do booleano `error` no stdout ANTES de acessar outros campos quando o exit code for não-zero
- Exemplo: `{"error": true, "code": 4, "message": "memory not found: design-auth"}`


## Códigos de Saída e Estratégia de Retry
### OBRIGATÓRIO — Tratamento Completo de Exit Codes
- `0` igual sucesso, parsear stdout
- `1` igual validação (peso inválido, self-link, max-files excedido)
- `2` igual erro de parsing de argumentos Clap (flags inválidas, timezone inválido, args obrigatórios ausentes)
- `9` igual duplicata (memória já existe sem `--force-merge`); desde v1.0.51 também retornado quando a memória é soft-deleted — use `--force-merge` para restaurar e atualizar, ou `restore` para reviver
- `3` igual conflito de locking otimista, recarregar e repetir
- `4` igual entidade, memória ou versão não encontrada
- `5` igual erro de namespace (nome inválido ou conflito)
- `6` igual payload acima do limite de tamanho
- `10` igual erro de database, executar `vacuum` e `health`
- `11` igual falha de embedding (modelo corrompido ou ORT ausente)
- `12` igual falha ao carregar `sqlite-vec`, verificar SQLite ≥ 3.40
- `13` igual falha parcial em batch, reprocessar apenas falhos
- `14` igual erro de I/O (arquivo inacessível, permissão, disco cheio)
- `15` igual banco ocupado (busy), ampliar `--wait-lock`
- `20` igual erro interno ou falha de serialização JSON
- `75` igual slots exauridos no ingest ou outro pesado
- `77` igual pressão de RAM, aguardar memória livre
### PROIBIDO — Anti-padrões de Erro
- NUNCA ignorar exit code não-zero como sucesso
- NUNCA reprocessar lote inteiro após exit 13
- NUNCA aumentar concorrência após receber 75 ou 77
- NUNCA tentar `restore` sem inspecionar `history` antes
- NUNCA culpar ambiguidade sem ler stderr primeiro
- NUNCA confundir exit 1 (validação) com exit 9 (duplicata)


## Concorrência e Recursos
### OBRIGATÓRIO — Controle de Carga
- INICIAR comandos pesados com `--max-concurrency 1`
- AUMENTAR apenas após medir RSS e swap do host
- RESPEITAR teto rígido de `2×nCPUs` em comandos pesados
- TRATAR `init`, `remember`, `ingest`, `recall`, `hybrid-search` como pesados
- AMPLIAR `--wait-lock <ms>` quando contenção for esperada
- LIMITAR ingestão paralela em CI sem daemon ativo
### OBRIGATÓRIO — Dois Eixos de Paralelismo no ingest
- `--max-concurrency` governa invocações CLI simultâneas
- `--ingest-parallelism` governa extract mais embed paralelos
- AJUSTAR ambos independentemente conforme RAM e CPU
- USAR `--low-memory` para forçar paralelismo unitário
- HONRAR `SQLITE_GRAPHRAG_LOW_MEMORY=1` em hosts restritos


## Manutenção e Backup
### OBRIGATÓRIO — Higiene Periódica
- AGENDAR `purge --retention-days 30 --yes` semanalmente
- EXECUTAR `vacuum` após purges grandes
- RODAR `optimize` para refrescar estatísticas do planner
- LIMPAR órfãos via `cleanup-orphans --yes` após forget em massa
### OBRIGATÓRIO — Backup Seguro
- USAR `sync-safe-copy --dest <path>` antes de sincronizar Dropbox ou iCloud
- COMPRIMIR snapshots via `ouch compress` para upload remoto
- EXPORTAR memórias via `list --limit 10000 --json` para NDJSON
- VERSIONAR banco com Git LFS quando viável
### OBRIGATÓRIO — Diagnóstico de Schema
- USAR `__debug_schema --json` para troubleshooting
- INSPECIONAR `schema_version`, `objects`, `migrations`
- COMANDO oculto do `--help`, invocar pelo nome exato
### Padrão Correto — Cron Semanal
- `sqlite-graphrag purge --retention-days 30 --yes`
- `sqlite-graphrag cleanup-orphans --yes`
- `sqlite-graphrag vacuum --json`
- `sqlite-graphrag optimize --json`
- `sqlite-graphrag sync-safe-copy --dest ~/Dropbox/graphrag.sqlite`


## Contrato: Stdin e Stdout
### Entrada — Apenas Argumentos Estruturados
- Flags da CLI aceitam argumentos tipados validados por `clap` com parsing estrito
- Stdin aceita body puro quando `--body-stdin` está ativo em `remember` ou `edit`
- Stdin aceita objeto JSON de grafo com `body` opcional, `entities` e `relationships` quando `--graph-stdin` está ativo em `remember`; JSON inválido falha em vez de virar body de memória
- Fontes de corpo como `--body`, `--body-file`, `--body-stdin` e `--graph-stdin` são rejeitadas quando combinadas de forma ambígua
- `remember` aceita payloads de body até `512000` bytes e até `512` chunks; payloads maiores retornam exit code `6`
- Variáveis de ambiente sobrescrevem defaults sem mutar o arquivo do banco de dados
- O caminho padrão do banco é sempre `./graphrag.sqlite` no diretório atual de invocação
- Idioma é controlado por `--lang <en|pt|pt-BR|portuguese|PT|pt-br>` para saída determinística


### Saída — Documentos JSON Determinísticos
- Cada subcomando emite exatamente um documento JSON quando `--json` está ativo
- Chaves permanecem estáveis entre releases dentro da mesma linha major corrente
- Timestamps seguem RFC 3339 com offset UTC sempre presente e explícito
- Campos opcionais podem ser omitidos ou serializados como `null`; agentes devem aceitar ambas as formas
- Arrays preservam ordem determinística por `score` ou `updated_at` descendente


## Tabela de Exit Codes
### Contrato — Mapeie Cada Status A Uma Decisão De Roteamento
| Código | Significado | Ação Recomendada |
| --- | --- | --- |
| `0` | Sucesso | Continue o loop do agente |
| `1` | Falha de validação ou runtime | Logue e exiba ao operador |
| `2` | Erro de parsing de argumentos (Clap) | Corrija argumentos e repita |
| `9` | Memória duplicata (inclui soft-deleted) | Use `--force-merge` para restaurar e atualizar |
| `3` | Conflito de optimistic update | Releia `updated_at` e repita |
| `4` | Memória ou entidade não encontrada | Trate recurso ausente graciosamente |
| `5` | Limite de namespace ou não resolvido | Passe `--namespace` explicitamente |
| `6` | Payload excedeu os limites permitidos | Divida o body em chunks menores |
| `10` | Erro SQLite no banco de dados | Rode `health` para inspecionar integridade |
| `11` | Falha na geração de embedding | Verifique arquivos do modelo e repita |
| `12` | Extensão `sqlite-vec` falhou | Reinstale o binário com extensão embutida |
| `13` | Operação em batch parcialmente falhou | Inspecione resultados parciais e repita os itens falhos |
| `14` | Erro de I/O (arquivo, permissão, disco cheio) | Verifique acesso ao arquivo e espaço em disco disponível |
| `15` | Banco ocupado após tentativas | Aguarde e repita a operação |
| `20` | Erro interno ou falha de serialização | Reporte bug com saída completa do stderr |
| `75` | Lock advisory ocupado ou todos os slots preenchidos | Aguarde e repita, ou reduza a pressão dos comandos pesados em vez de elevar a concorrência cegamente |
| `77` | Limite de memória baixo acionado | Libere RAM antes de repetir |


## Formato De Saída JSON
### Recall — KNN Vetorial Puro
```json
{
  "query": "graphrag retrieval",
  "k": 3,
  "direct_matches": [
    { "memory_id": 1, "name": "graphrag-intro", "namespace": "global", "type": "user", "description": "intro doc", "snippet": "GraphRAG combines...", "distance": 0.09, "source": "vec" }
  ],
  "graph_matches": [],
  "results": [
    { "memory_id": 1, "name": "graphrag-intro", "namespace": "global", "type": "user", "description": "intro doc", "snippet": "GraphRAG combines...", "distance": 0.09, "source": "vec" }
  ],
  "elapsed_ms": 12
}
```


### Hybrid Search — FTS5 Mais Vetor Via RRF
```json
{
  "query": "postgres migration",
  "k": 5,
  "rrf_k": 60,
  "weights": { "vec": 1.0, "fts": 1.0 },
  "results": [
    { "memory_id": 1, "name": "postgres-migration-plan", "namespace": "global", "type": "project", "description": "migration plan", "body": "Step 1...", "combined_score": 0.96, "score": 0.96, "source": "hybrid", "vec_rank": 1, "fts_rank": 1 },
    { "memory_id": 2, "name": "db-migration-checklist", "namespace": "global", "type": "reference", "description": "checklist", "body": "Check indexes...", "combined_score": 0.88, "score": 0.88, "source": "hybrid", "vec_rank": 2, "fts_rank": 3 }
  ],
  "graph_matches": [],
  "elapsed_ms": 18
}
```


## Idempotência e Efeitos Colaterais
### Comandos Read-Only — Zero Mutação Garantida
- `recall` lê tabelas de vetor e metadados sem tocar o estado em disco
- `read` busca uma única linha por nome e emite JSON sem efeito colateral
- `list` pagina memórias ordenadas deterministicamente com cursores estáveis
- `health` roda `PRAGMA integrity_check` e reporta sem escrever em disco
- `stats` conta linhas em transações read-only seguras para agentes concorrentes


### Comandos Write — Optimistic Locking Protege Concorrência
- `remember` usa `ON CONFLICT(name)` então chamadas duplicadas retornam exit code `9`
- `rename` exige `--expected-updated-at` para detectar escrita stale via exit `3`
- `edit` cria nova linha em `memory_versions` preservando histórico imutável
- `restore` retrocede o conteúdo criando uma nova versão em vez de sobrescrever
- `forget` é soft-delete então repetir a chamada é seguro e idempotente por design


## Limites De Payload
### Tetos — Aplicados Pelo Binário
- `EMBEDDING_MAX_TOKENS` vale 512 tokens medidos pelo tokenizador do modelo
- `TEXT_BODY_PREVIEW_LEN` vale 200 caracteres em snippets de list e recall
- `MAX_CONCURRENT_CLI_INSTANCES` vale como teto rígido de 4 entre agentes subprocess cooperando, mas comandos pesados podem ser reduzidos dinamicamente pela RAM disponível
- `CLI_LOCK_DEFAULT_WAIT_SECS` vale 300 segundos antes do exit code `75`
- `PURGE_RETENTION_DAYS_DEFAULT` vale 90 dias antes do hard delete ficar permitido


## Controle De Idioma
### Saída Bilíngue — Uma Flag Troca O Locale
- Flag `--lang en` força mensagens em inglês independentemente do locale do sistema
- Flag `--lang pt` ou `--lang pt-BR` ou `--lang portuguese` ou `--lang PT` força mensagens em português
- Códigos curtos `en` e `pt` são as formas canônicas; os aliases mais longos são aceitos sem erro
- Env `SQLITE_GRAPHRAG_LANG=pt` sobrescreve locale do sistema quando falta `--lang`
- Sem flag e sem env cai no fallback por `sys_locale::get_locale()` do runtime
- Locales desconhecidos caem em inglês sem emitir warning algum no stderr
- Env `SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo` define o fuso IANA aplicado a todos os campos `*_iso` no JSON de saída
- A flag `--tz <IANA>` tem prioridade sobre `SQLITE_GRAPHRAG_DISPLAY_TZ`; ambos caem para UTC quando ausentes
- Nomes IANA inválidos causam exit 2 com mensagem de erro `Validation` antes de qualquer comando executar
- Apenas campos string `*_iso` são afetados; campos epoch inteiros (`created_at`, `updated_at`) permanecem inalterados
- Env `SQLITE_GRAPHRAG_LOG_FORMAT=json` alterna saída de tracing para JSON delimitado por linha; padrão é `pretty`


## Contrato de Runtime em ARM64 GNU
### Carregamento Dinâmico do ONNX Runtime — O Que Agentes DEVEM Fornecer
- Em `aarch64-unknown-linux-gnu`, comandos de embedding NÃO dependem de linkedição do ONNX Runtime no build
- Agentes DEVEM tornar `libonnxruntime.so` alcançável via `ORT_DYLIB_PATH`, diretório do executável, `./lib/` ou diretório de cache de modelos
- Os comandos pesados afetados são `init`, `remember`, `recall` e `hybrid-search`
- Se a biblioteca compartilhada estiver ausente, a primeira operação de embedding falha em runtime mesmo com a binária iniciando corretamente


## Flag de Saída JSON
### Formato — `--json` É Universal e `--format json` É Específico por Comando
- Todos os subcomandos aceitam `--json` para JSON determinístico no stdout
- Apenas comandos que expõem `--format` no help aceitam `--format json`
- `--json` é a forma curta — preferida em one-liners e pipelines de agentes
- Se `--json` aparece com um `--format` não JSON, `--json` vence e stdout continua JSON
- `--format json` é a forma explícita — específica por comando, preferida onde também existem outros modos de saída


## Payloads de Entrada do Grafo
### Contrato — Arquivos do `remember`
- `--entities-file` aceita um array JSON de objetos de entidade
- Cada objeto de entidade DEVE incluir `name` e `entity_type`
- O campo alias `type` é aceito como sinônimo de `entity_type`
- Agentes NÃO DEVEM enviar `entity_type` e `type` no mesmo objeto de entidade
- Valores válidos para `entity_type` são `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location` e `date`
- `--relationships-file` aceita um array JSON de objetos de relacionamento
- Cada objeto de relacionamento DEVE incluir `source`/`from`, `target`/`to`, `relation` e `strength`
- `strength` DEVE ser número de ponto flutuante no intervalo inclusivo `[0.0, 1.0]`
- As saídas do grafo expõem esse valor como `weight`
- Payloads de arquivo PODEM usar nomes canônicos persistidos com underscore como `applies_to`, `depends_on` e `tracked_in`; aliases com hífen são normalizados antes da gravação
- Flags CLI de `link` e `unlink` usam rótulos com hífen como `applies-to`, `depends-on` e `tracked-in`
- `--graph-stdin` aceita um único objeto com `body` opcional e os mesmos arrays `entities` e `relationships`
- `link --create-missing` cria automaticamente entidades inexistentes durante a linkagem, com tipo padrão `concept`; use `--entity-type` para sobrescrever (adicionado em v1.0.44)
- `hybrid-search --with-graph` habilita graph traversal a partir dos top resultados RRF; matches do grafo aparecem no array `graph_matches` junto ao array `results` (corrigido em v1.0.44 — era um no-op antes)
- `graph entities` JSON response usa chave top-level `entities` (renomeado de `items` em v1.0.44); atualize scripts `jaq` existentes de `.items[]` para `.entities[]`


## Schemas Legíveis por Máquina
### Arquivos JSON Schema Draft 2020-12 Para Cada Subcomando
- O diretório `docs/schemas/` contém um arquivo `.schema.json` por subcomando
- Todo schema declara `"additionalProperties": false` — chaves desconhecidas são violações de contrato
- Schemas usam `$defs` para subtipos compartilhados (ex: `RecallItem`, `HealthCheck`)
- Campos opcionais ficam fora do array `required` e são tipados com `["T", "null"]` quando anuláveis
- Validar resposta em tempo real com um validador JSON Schema real: `jsonschema --instance <(sqlite-graphrag stats) docs/schemas/stats.schema.json`
- O arquivo `docs/schemas/debug-schema.schema.json` cobre o subcomando diagnóstico oculto `__debug_schema`
- Schemas são atualizados a cada breaking change e seguem a versão major SemVer da CLI


## Resumo Dos Superpoderes
### Cinco Razões Para Seu Orquestrador Permanecer
- Saída determinística elimina parsing frágil por regex no código de glue do agente
- Exit codes roteiam decisões sem raspar stderr por mensagens legíveis a humanos
- Binário único implanta idêntico em Docker GitHub Actions e laptops de dev
- Durabilidade do SQLite sobrevive a kernel panic e kill de container sem corromper
- Retrieval por grafo revela contexto multi-hop que o vetor puro jamais devolve


## Comece Em 30 Segundos
### Instalação — Um Comando Instala A Stack Inteira
```bash
cargo install --path . && sqlite-graphrag init
```
- Flag `--locked` reusa o `Cargo.lock` enviado para proteger MSRV de drift transitivo
- Comando `init` cria `graphrag.sqlite` no diretório atual e baixa o modelo de embedding localmente
- Primeira invocação pode levar um minuto enquanto `fastembed` baixa `multilingual-e5-small`
- Invocações seguintes evitam apenas o primeiro download do modelo, mas comandos pesados ainda dependem da residência do modelo e do daemon
- Remova com `cargo uninstall sqlite-graphrag` deixando o arquivo de banco intacto
