# sqlite-graphrag para Agentes de IA


> Memória persistente para 27 agentes de IA em um único binário Rust de 25 MB

- Leia a versão em inglês em [AGENTS.md](AGENTS.md)


## A Pergunta Que Nenhum Framework Responde
### Open Loop — Por Que 27 Agentes de IA Escolhem Esta Como Sua Camada de Memória
- Por que 27 agentes de IA escolhem sqlite-graphrag como sua camada de memória persistente?
- Três razões técnicas: recall em menos de 50 ms, zero dependências cloud, JSON determinístico
- Cada agente ganha memória persistente sem gastar um único token adicional
- Versus MCPs pesados, sqlite-graphrag entrega contrato stdin/stdout determinístico
- O segredo que os frameworks jamais documentam mora em um único arquivo SQLite portátil


## Por Que Agentes Amam Esta CLI
### Cinco Diferenciais — Projetados Para Loops Autônomos
- Saída JSON determinística elimina cada hack de parser no código de orquestração
- Exit codes seguem `sysexits.h` para sua lógica de retry funcionar sem casar string
- Zero dependências de runtime entregam um binário estático com menos de 30 MB
- Stdin aceita payloads estruturados para seus agentes jamais escaparem argumentos shell
- Comportamento cross-platform permanece idêntico em Linux macOS e Windows desde o início
- O comportamento padrão sempre cria ou abre `graphrag.sqlite` no diretório atual


## Economia Que Converte
### Números Que Vendem A Troca
- Economize 200 dólares por mês substituindo Pinecone e chamadas de embedding OpenAI
- Reduza em até 80 por cento os tokens gastos em RAG via recall por grafo tipado
- Derrube a latência de retrieval de 800 ms em vector DB cloud para 8 ms em SSD local
- Corte o cold-start de 12 segundos de boot Docker para 90 ms de binário único
- Elimine 4 horas semanais de manutenção de cluster com banco zero-ops em um arquivo


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
| OpenClaw | comunidade | recente | Subprocess | `sqlite-graphrag list --type user --json` |
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
- Saída: JSON com array `results` contendo campos `name`, `score` e `updated_at`

### Z.ai
- Plataforma de agentes hospedada com planejamento multi-etapa e orquestração de tools
- Invoque sqlite-graphrag para persistir memória entre sessões de planejamento:
```bash
sqlite-graphrag remember --name "task-plan-$(date +%s)" --type project --description "plano de tarefa Z.ai" --body "$PLAN"
sqlite-graphrag recall "previous task plan" --json --k 3
```
- Saída: JSON determinístico com `results` ordenados por score de similaridade cosseno

### Ollama
- Servidor LLM local rodando modelos abertos em hardware consumer sem cloud
- Invoque sqlite-graphrag como tool para dar aos agentes Ollama conhecimento persistente:
```bash
sqlite-graphrag recall "conversation history" --json --k 5
sqlite-graphrag remember --name "ollama-session" --type project --description "sessão Ollama" --body "$CONTEXT"
```
- Saída: resposta JSON de recall com `elapsed_ms` abaixo de 50 em hardware moderno

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
- Recall em menos de 50 ms em grafo de 10 mil entradas medido em M1 e x86_64
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


## Contrato: Stdin e Stdout
### Entrada — Apenas Argumentos Estruturados
- Flags da CLI aceitam argumentos tipados validados por `clap` com parsing estrito
- Stdin aceita body puro quando `--body-stdin` está ativo em `remember` ou `edit`
- Stdin aceita payload JSON quando `--payload-stdin` está ativo em modos batch
- Variáveis de ambiente sobrescrevem defaults sem mutar o arquivo do banco de dados
- Idioma é controlado por `--lang <en|pt|pt-BR|portuguese|PT|pt-br>` para saída determinística


### Saída — Documentos JSON Determinísticos
- Cada subcomando emite exatamente um documento JSON quando `--json` está ativo
- Chaves permanecem estáveis entre releases dentro da mesma linha major corrente
- Timestamps seguem RFC 3339 com offset UTC sempre presente e explícito
- Campos nulos são omitidos para manter o payload enxuto para consumo por agentes
- Arrays preservam ordem determinística por `score` ou `updated_at` descendente


## Tabela de Exit Codes
### Contrato — Mapeie Cada Status A Uma Decisão De Roteamento
| Código | Significado | Ação Recomendada |
| --- | --- | --- |
| `0` | Sucesso | Continue o loop do agente |
| `1` | Falha de validação ou runtime | Logue e exiba ao operador |
| `2` | Erro de uso CLI ou duplicata | Corrija argumentos e repita |
| `3` | Conflito de optimistic update | Releia `updated_at` e repita |
| `4` | Memória ou entidade não encontrada | Trate recurso ausente graciosamente |
| `5` | Limite de namespace ou não resolvido | Passe `--namespace` explicitamente |
| `6` | Payload excedeu os limites permitidos | Divida o body em chunks menores |
| `10` | Erro SQLite no banco de dados | Rode `health` para inspecionar integridade |
| `11` | Falha na geração de embedding | Verifique arquivos do modelo e repita |
| `12` | Extensão `sqlite-vec` falhou | Reinstale o binário com extensão embutida |
| `13` | Operação em batch parcialmente falhou | Inspecione resultados parciais e repita os itens falhos |
| `15` | Banco ocupado após tentativas | Aguarde e repita a operação |
| `75` | Lock advisory ocupado ou todos os slots preenchidos | Aguarde e repita, ou reduza a pressão dos comandos pesados em vez de elevar a concorrência cegamente |
| `77` | Limite de memória baixo acionado | Libere RAM antes de repetir |


## Formato De Saída JSON
### Recall — KNN Puramente Vetorial
```json
{
  "query": "graphrag retrieval",
  "k": 3,
  "namespace": "global",
  "elapsed_ms": 12,
  "results": [
    { "name": "graphrag-intro", "score": 0.91, "type": "user", "updated_at": "2026-04-18T12:00:00Z" },
    { "name": "vector-search-notes", "score": 0.84, "type": "project", "updated_at": "2026-04-17T08:12:03Z" },
    { "name": "hybrid-ranker", "score": 0.77, "type": "feedback", "updated_at": "2026-04-16T21:04:55Z" }
  ]
}
```


### Hybrid Search — FTS5 Mais Vetor Via RRF
```json
{
  "query": "postgres migration",
  "k": 5,
  "rrf_k": 60,
  "weights": { "vec": 1.0, "fts": 1.0 },
  "elapsed_ms": 18,
  "results": [
    { "name": "postgres-migration-plan", "score": 0.96, "vec_rank": 1, "fts_rank": 1 },
    { "name": "db-migration-checklist", "score": 0.88, "vec_rank": 2, "fts_rank": 3 }
  ]
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
- `remember` usa `ON CONFLICT(name)` então chamadas duplicadas retornam exit code `2`
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
- Flag `--lang pt` (também `pt-BR`, `portuguese`, `PT`, `pt-br`) força mensagens em português
- Env `SQLITE_GRAPHRAG_LANG=pt` sobrescreve locale do sistema quando falta `--lang`
- Sem flag e sem env cai no fallback por `sys_locale::get_locale()` do runtime
- Locales desconhecidos caem em inglês sem emitir warning algum no stderr
- Env `SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo` define o fuso IANA aplicado a todos os campos `*_iso` no JSON de saída
- A flag `--tz <IANA>` tem prioridade sobre `SQLITE_GRAPHRAG_DISPLAY_TZ`; ambos caem para UTC quando ausentes
- Nomes IANA inválidos causam exit 2 com mensagem de erro `Validation` antes de qualquer comando executar
- Apenas campos string `*_iso` são afetados; campos epoch inteiros (`created_at`, `updated_at`) permanecem inalterados
- Env `SQLITE_GRAPHRAG_LOG_FORMAT=json` alterna saída de tracing para JSON delimitado por linha; padrão é `pretty`


## Flag de Saída JSON
### Formato — `--json` É Universal e `--format json` É Específico por Comando
- Todos os subcomandos aceitam `--json` para JSON determinístico no stdout
- Apenas comandos que expõem `--format` no help aceitam `--format json`
- `--json` é a forma curta — preferida em one-liners e pipelines de agentes
- `--format json` é a forma explícita — específica por comando, preferida onde também existem outros modos de saída


## Payloads de Entrada do Grafo
### Contrato — Arquivos do `remember`
- `--entities-file` aceita um array JSON de objetos de entidade
- Cada objeto de entidade DEVE incluir `name` e `entity_type`
- O campo alias `type` é aceito como sinônimo de `entity_type`
- Agentes NÃO DEVEM enviar `entity_type` e `type` no mesmo objeto de entidade
- Valores válidos para `entity_type` são `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard` e `issue_tracker`
- `--relationships-file` aceita um array JSON de objetos de relacionamento
- Cada objeto de relacionamento DEVE incluir `source`, `target`, `relation` e `strength`
- `strength` DEVE ser número de ponto flutuante no intervalo inclusivo `[0.0, 1.0]`
- As saídas do grafo expõem esse valor como `weight`
- Payloads de arquivo DEVEM usar nomes canônicos persistidos com underscore como `applies_to`, `depends_on` e `tracked_in`
- Flags CLI de `link` e `unlink` usam rótulos com hífen como `applies-to`, `depends-on` e `tracked-in`


## Schemas Legíveis por Máquina
### Arquivos JSON Schema Draft 2020-12 Para Cada Subcomando
- O diretório `docs/schemas/` contém um arquivo `.schema.json` por subcomando
- Todo schema declara `"additionalProperties": false` — chaves desconhecidas são violações de contrato
- Schemas usam `$defs` para subtipos compartilhados (ex: `RecallItem`, `HealthCheck`)
- Campos opcionais ficam fora do array `required` e são tipados com `["T", "null"]` quando anuláveis
- Validar resposta em tempo real: `sqlite-graphrag stats | jaq --from-file docs/schemas/stats.schema.json`
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
- Invocações seguintes iniciam frias em menos de 100 ms em hardware consumer moderno
- Remova com `cargo uninstall sqlite-graphrag` deixando o arquivo de banco intacto
