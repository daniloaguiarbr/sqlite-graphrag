# sqlite-graphrag

[![Crates.io](https://img.shields.io/crates/v/sqlite-graphrag.svg)](https://crates.io/crates/sqlite-graphrag)
[![Docs.rs](https://docs.rs/sqlite-graphrag/badge.svg)](https://docs.rs/sqlite-graphrag)
[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](LICENSE)
[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg)](CODE_OF_CONDUCT.md)

> Memória persistente para agentes de IA em um único binário Rust com GraphRAG embutido.

- Versão em inglês disponível em [README.md](README.md)
- O pacote público e o repositório já estão disponíveis no GitHub e no crates.io
- Instale a última release publicada com `cargo install sqlite-graphrag --locked`
- Atualize uma instalação existente com `cargo install sqlite-graphrag --locked --force`
- Verifique o binário ativo com `sqlite-graphrag --version`
- Veja o histórico completo de releases em [CHANGELOG.md](CHANGELOG.md)
- A validação de release inclui as suítes de contrato `slow-tests` documentadas em `docs/TESTING.pt-BR.md`
- Faça o build direto do checkout local com `cargo install --path .`

```bash
cargo install sqlite-graphrag --locked --force
sqlite-graphrag --version
```


## O que é?
### sqlite-graphrag entrega memória durável para agentes de IA
- Armazena memórias, entidades e relacionamentos em um único arquivo SQLite abaixo de 25 MB
- Gera embeddings localmente via `fastembed` com o modelo `multilingual-e5-small`
- Combina busca textual FTS5 com KNN do `sqlite-vec` em ranqueador híbrido Reciprocal Rank Fusion
- Armazena e percorre um grafo explícito de entidades com arestas tipadas para recuperação multi-hop entre memórias
- Preserva cada edição em tabela imutável de versões históricas para auditoria completa
- Executa em Linux, macOS e Windows nativamente sem qualquer serviço externo necessário


## Por que sqlite-graphrag?
### Diferenciais contra stacks RAG em nuvem
- Arquitetura offline-first elimina custos recorrentes com embeddings OpenAI e Pinecone
- Armazenamento em arquivo SQLite único substitui clusters Docker de bancos vetoriais
- Recuperação com grafo supera RAG vetorial puro em perguntas multi-hop por design
- Saída JSON determinística habilita orquestração limpa por agentes de IA em pipelines
- Binário cross-platform nativo dispensa dependências Python, Node ou Docker


## Superpoderes para Agentes de IA
### Contrato de CLI de primeira classe para orquestração
- Todo subcomando aceita `--json` produzindo payloads determinísticos em stdout
- Toda invocação pode continuar stateless, mas comandos pesados de embedding agora sobem e reutilizam `sqlite-graphrag daemon` automaticamente quando necessário
- `sqlite-graphrag daemon` continua existindo para controle explícito, mas o caminho comum não exige mais startup manual
- Toda escrita é idempotente via restrições de unicidade em `--name` kebab-case
- Stdin é explícito: use `--body-stdin` para texto ou `--graph-stdin` para um objeto `{body?, entities, relationships}`; arrays crus de entidades e relacionamentos usam `--entities-file` e `--relationships-file`
- `remember` aceita payloads de body até `512000` bytes e até `512` chunks
- Payloads de relacionamento usam `strength` em `[0.0, 1.0]`, mapeado para `weight` nas saídas
- Stderr carrega saída de tracing apenas sob `SQLITE_GRAPHRAG_LOG_LEVEL=debug`
- `--help` é inglês por padrão; use `--lang` para mensagens humanas de runtime, não para o help estático do clap
- Comportamento cross-platform é idêntico em hosts Linux, macOS e Windows


## Schema do Grafo
### Tipos de entidade, rótulos de relação e peso de aresta
- `entity_type` aceita exatamente 10 valores: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`
- `relation` aceita exatamente 12 valores: `applies_to`, `uses`, `depends_on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked_in`
- `strength` é um float em `[0.0, 1.0]` representando o peso da aresta; mapeado para `weight` em todos os outputs de leitura
- Valores de `entity_type` ou `relation` não listados são rejeitados na escrita com código de saída 1
- Use `sqlite-graphrag graph --format json` para inspecionar o grafo completo armazenado a qualquer momento


### 27 agentes de IA e IDEs suportados de imediato
| Agente | Fornecedor | Versão mínima | Padrão de integração |
| --- | --- | --- | --- |
| Claude Code | Anthropic | 1.0 | Subprocesso com stdout `--json` |
| Codex | OpenAI | 1.0 | Tool call envolvendo `cargo run -- recall` |
| Gemini CLI | Google | 1.0 | Function call retornando JSON |
| Opencode | Opencode | 1.0 | Shell tool com `hybrid-search --json` |
| OpenClaw | Comunidade | 0.1 | Subprocesso via pipe para filtros `jaq` |
| Paperclip | Comunidade | 0.1 | Invocação direta da CLI por mensagem |
| VS Code Copilot | Microsoft | 1.85 | Subprocesso de terminal via tasks |
| Google Antigravity | Google | 1.0 | Agent tool com JSON estruturado |
| Windsurf | Codeium | 1.0 | Registro de comando customizado |
| Cursor | Anysphere | 0.42 | Integração terminal ou wrapper MCP |
| Zed | Zed Industries | 0.160 | Extensão envolvendo subprocesso |
| Aider | Paul Gauthier | 0.60 | Hook de shell por turno |
| Jules | Google Labs | 1.0 | Integração de shell no workspace |
| Kilo Code | Comunidade | 1.0 | Invocação via subprocesso |
| Roo Code | Comunidade | 1.0 | Comando customizado via CLI |
| Cline | Saoud Rizwan | 3.0 | Ferramenta de terminal registrada manualmente |
| Continue | Continue Dev | 0.9 | Provedor de contexto via shell |
| Factory | Factory AI | 1.0 | Tool call com resposta JSON |
| Augment Code | Augment | 1.0 | Envolvimento de comando de terminal |
| JetBrains AI Assistant | JetBrains | 2024.3 | External tool por IDE |
| OpenRouter | OpenRouter | 1.0 | Roteamento de função via shell |
| Minimax | Minimax | 1.0 | Invocação via subprocesso |
| Z.ai | Z.ai | 1.0 | Invocação via subprocesso |
| Ollama | Ollama | 0.1 | Invocação via subprocesso |
| Hermes Agent | Comunidade | 1.0 | Invocação via subprocesso |
| LangChain | LangChain | 0.3 | Subprocesso via tool |
| LangGraph | LangChain | 0.2 | Subprocesso via nó |


## Início Rápido
### Instale e grave sua primeira memória em quatro comandos
```bash
cargo install sqlite-graphrag --locked --force
sqlite-graphrag init
sqlite-graphrag remember --name primeira-memoria --type user --description "primeira memória" --body "olá graphrag"
sqlite-graphrag recall "graphrag" --k 5 --json
```
- Para o checkout local, `cargo install --path .` é suficiente
- Reexecute `sqlite-graphrag --version` após qualquer upgrade para confirmar o binário ativo
- Depois da release pública, prefira `--locked` para preservar o grafo de dependências validado para o MSRV


## Instalação
### Múltiplos canais de distribuição
- Instale a última release publicada com `cargo install sqlite-graphrag --locked`
- Atualize um binário publicado existente com `cargo install sqlite-graphrag --locked --force`
- Para fixar uma versão específica use `cargo install sqlite-graphrag --version <X.Y.Z> --locked`
- Instale a partir do checkout local com `cargo install --path .`
- Compile a partir do checkout local com `cargo build --release`
- Fórmula Homebrew planejada sob `brew install sqlite-graphrag`
- Bucket Scoop planejado sob `scoop install sqlite-graphrag`
- Imagem Docker planejada como `ghcr.io/daniloaguiarbr/sqlite-graphrag:<version>`
### Binários pré-compilados (GitHub Releases)
- `x86_64-unknown-linux-gnu` Linux Intel/AMD 64-bit
- `aarch64-unknown-linux-gnu` Linux ARM 64-bit (Raspberry Pi 4+, AWS Graviton)
- `aarch64-apple-darwin` macOS Apple Silicon (M1/M2/M3/M4)
- `x86_64-pc-windows-msvc` Windows Intel/AMD 64-bit
- `aarch64-pc-windows-msvc` Windows ARM 64-bit
### Usuários de Mac Intel (x86_64-apple-darwin)
- Não há binário pré-compilado para Macs Intel
- O GitHub aposentou o runner macos-13 em dezembro de 2025 e a Apple descontinuou suporte ao x86_64
- Compile localmente com `cargo install sqlite-graphrag --locked` (requer Rust 1.88+)
- Caminho de migração recomendado é para Apple Silicon quando viável


## Uso
### Inicialize o banco de dados
```bash
sqlite-graphrag init
sqlite-graphrag init --namespace projeto-foo
```
- Sem `--db` ou `SQLITE_GRAPHRAG_DB_PATH`, todo comando CRUD nessa pasta usa `./graphrag.sqlite`
### Grave uma memória com grafo de entidades explícito opcional
```bash
sqlite-graphrag remember \
  --name testes-integracao-postgres \
  --type feedback \
  --description "prefira Postgres real a mocks SQLite" \
  --body "Testes de integração devem usar banco real."
```
- A resposta JSON de `remember` inclui `urls_persisted` (URLs roteadas para a tabela `memory_urls`) e `relationships_truncated` (bool, ativo quando relacionamentos foram truncados)
- URLs são armazenadas em `memory_urls` via schema V007 e nunca poluem o grafo de entidades
### Pule auto-extração BERT NER para ingestão mais rápida
- `--skip-extraction` desabilita `extract_graph_auto` apenas para a chamada atual
- Use quando o body é curto, quando você fornece `--entities-file` upstream, ou quando memória do CI é restrita
- O campo `extraction_method` é omitido da resposta JSON quando ativo
```bash
sqlite-graphrag remember \
  --name notas-de-release-v1 \
  --type concept \
  --description "notas de release para v1.0.0" \
  --skip-extraction \
  --body-stdin < notas.md
```
### Leia, esqueça, edite e renomeie usando argumento posicional
```bash
sqlite-graphrag read testes-integracao-postgres --json
sqlite-graphrag forget testes-integracao-postgres
sqlite-graphrag history testes-integracao-postgres --json
sqlite-graphrag edit testes-integracao-postgres --body "Corpo atualizado."
sqlite-graphrag rename testes-integracao-postgres --new testes-postgres
```
- Nome posicional é equivalente a `--name <nome>` para `read`, `forget`, `history`, `edit` e `rename`

### Busque memórias por similaridade semântica
```bash
sqlite-graphrag recall "testes integração postgres" --k 3 --json
```
### Busca híbrida combinando FTS5 e KNN vetorial
```bash
sqlite-graphrag hybrid-search "rollback migração postgres" --k 10 --json
```
### Inspecione saúde e estatísticas do banco
```bash
sqlite-graphrag health --json
sqlite-graphrag stats --json
```
### Purgue memórias soft-deleted após período de retenção
```bash
sqlite-graphrag purge --retention-days 90 --dry-run --json
sqlite-graphrag purge --retention-days 90 --yes
```


## Comandos
### Núcleo de ciclo de vida do banco
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `init` | `--namespace <ns>` | Inicializa banco e baixa modelo de embedding |
| `daemon` | `--ping`, `--stop`, `--idle-shutdown-secs`, `--db`, `--json` | Executa ou controla o daemon persistente de embeddings |
| `health` | `--json` | Exibe integridade e status dos pragmas |
| `stats` | `--json` | Conta memórias, entidades e relacionamentos |
| `migrate` | `--json` | Aplica migrações pendentes via `refinery` |
| `vacuum` | `--json` | Faz checkpoint do WAL e libera espaço |
| `optimize` | `--json` | Executa `PRAGMA optimize` para atualizar estatísticas |
| `sync-safe-copy` | `--dest <caminho>` (alias `--output`) | Gera cópia segura para sincronização em nuvem |
### Ciclo de vida do conteúdo de memória
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `remember` | `--name`, `--type`, `--description`, `--body`, `--skip-extraction` | Salva memória com grafo de entidades opcional |
| `recall` | `<query>`, `--k`, `--type` | Busca memórias semanticamente via KNN |
| `read` | `[nome]` ou `--name <nome>` | Recupera memória por nome kebab-case exato |
| `list` | `--type`, `--limit`, `--offset` | Pagina memórias ordenadas por `updated_at` |
| `forget` | `[nome]` ou `--name <nome>` | Remove memória logicamente preservando histórico |
| `rename` | `[antigo]` ou `--old <nome>`, `--new <nome>` | Renomeia memória mantendo versões |
| `edit` | `[nome]` ou `--name`, `--body`, `--description` | Edita corpo ou descrição gerando nova versão |
| `history` | `[nome]` ou `--name <nome>` | Lista todas as versões da memória |
| `restore` | `--name`, `--version` | Restaura memória para versão anterior |
### Recuperação e grafo
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `hybrid-search` | `<query>`, `--k`, `--rrf-k` | FTS5 combinado com vetor via Reciprocal Rank Fusion |
| `namespace-detect` | `--namespace <nome>` | Resolve precedência de namespace para invocação |
| `link` | `--from`, `--to`, `--relation`, `--weight` | Cria relacionamento explícito entre duas entidades |
| `unlink` | `--relationship-id` | Remove um relacionamento específico entre duas entidades |
| `related` | `--name`, `--limit`, `--hops` | Percorre memórias conectadas pelo grafo a partir de uma memória base |
| `graph` | `--format`, `--output` | Exporta snapshot do grafo em `json`, `dot` ou `mermaid` |

### Subcomandos do graph
| Subcomando | Descrição | Flags principais |
| --- | --- | --- |
| `graph traverse --from <ENTIDADE>` | Percorre o grafo de entidades a partir de um nó inicial usando BFS | `--depth` (padrão 2), `--namespace` |
| `graph stats` | Imprime estatísticas do grafo (nós, arestas, distribuição de grau) | `--namespace` |
| `graph entities` | Lista entidades armazenadas no grafo com filtros opcionais | `--limit` (padrão 50), `--entity-type`, `--namespace` |

### Manutenção
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `purge` | `--retention-days <n>`, `--dry-run`, `--yes` | Apaga permanentemente memórias soft-deleted |
| `cleanup-orphans` | `--namespace`, `--dry-run`, `--yes` | Remove entidades sem memórias e sem relacionamentos |


## Variáveis de Ambiente
### Overrides de configuração em runtime
| Variável | Descrição | Padrão | Exemplo |
| --- | --- | --- | --- |
| `SQLITE_GRAPHRAG_DB_PATH` | Caminho para override do arquivo SQLite | `./graphrag.sqlite` no diretório da invocação | `/dados/graphrag.sqlite` |
| `SQLITE_GRAPHRAG_CACHE_DIR` | Diretório de override para cache do modelo e lock files | Diretório XDG cache | `~/.cache/sqlite-graphrag` |
| `SQLITE_GRAPHRAG_LANG` | Idioma da saída da CLI como `en` ou `pt` (alias: `pt-BR`, `portuguese`) | `en` | `pt` |
| `SQLITE_GRAPHRAG_LOG_LEVEL` | Nível do filtro de tracing para saída em stderr | `info` | `debug` |
| `SQLITE_GRAPHRAG_NAMESPACE` | Override de namespace ignorando detecção | nenhum | `projeto-foo` |
| `SQLITE_GRAPHRAG_DISPLAY_TZ` | Fuso horário IANA para campos `*_iso` no JSON | `UTC` | `America/Sao_Paulo` |
| `SQLITE_GRAPHRAG_DAEMON_FORCE_AUTOSTART` | Força o autostart do daemon mesmo quando os guards o pulariam | indefinido | `1` |
| `SQLITE_GRAPHRAG_DAEMON_DISABLE_AUTOSTART` | Desabilita completamente o autostart do daemon (útil em testes/CI) | indefinido | `1` |
| `SQLITE_GRAPHRAG_DAEMON_CHILD` | Flag INTERNA setada automaticamente ao spawnar o filho do daemon; não setar manualmente | indefinido | `1` |
| `ORT_DYLIB_PATH` | Caminho explícito para `libonnxruntime.so` no carregamento dinâmico de ARM64 GNU | autodiscovery | `/opt/sqlite-graphrag/libonnxruntime.so` |


## Padrões de Integração
### Compondo com pipelines e ferramentas Unix
```bash
sqlite-graphrag recall "testes auth" --k 5 --json | jaq -r '.results[].name'
```
### Alimente busca híbrida em endpoint sumarizador
```bash
sqlite-graphrag hybrid-search "migração postgres" --k 10 --json \
  | jaq -c '.results[] | {name, combined_score}' \
  | xh POST http://localhost:8080/summarize
```
### Backup com snapshot atômico e compressão
```bash
sqlite-graphrag sync-safe-copy --dest /tmp/ng.sqlite
ouch compress /tmp/ng.sqlite /tmp/ng-$(date +%Y%m%d).tar.zst
```
### Exemplo de subprocesso no Claude Code em Node
```javascript
const { spawn } = require('child_process');
const proc = spawn('sqlite-graphrag', ['recall', query, '--k', '5', '--json']);
```
### Build Docker Debian para pipelines de CI
```dockerfile
FROM rust:1.88-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo install --path .
```


## Códigos de Saída
### Status determinísticos para orquestração
| Código | Significado |
| --- | --- |
| `0` | Sucesso |
| `1` | Erro de validação ou falha em runtime |
| `2` | Duplicata detectada ou argumento CLI inválido |
| `3` | Conflito durante atualização otimista |
| `4` | Memória ou entidade não encontrada |
| `5` | Namespace não pôde ser resolvido |
| `6` | Payload excedeu limites configurados |
| `10` | Erro do banco SQLite |
| `11` | Geração de embedding falhou |
| `12` | Extensão `sqlite-vec` falhou ao carregar |
| `13` | Falha parcial em lote (import, reindex, stdin batch) |
| `14` | Erro de I/O do sistema de arquivos |
| `15` | Banco ocupado após tentativas (movido de 13 na linha legada) |
| `20` | Erro interno ou de serialização JSON |
| `73` | `EX_NOPERM`: guarda de memória rejeitou condição de pouca RAM |
| `75` | `EX_TEMPFAIL`: todos os slots de concorrência ocupados |
| `77` | RAM disponível abaixo do mínimo para carregar o modelo |


## Desempenho
### Medido em banco com 1000 memórias
- A latência em processo com modelo já aquecido continua muito menor que a latência da CLI stateless
- Invocações stateless da CLI tipicamente gastam cerca de um segundo recarregando o modelo em cada comando pesado
- Recall aquecido em processo pode ficar bem abaixo da latência da CLI stateless quando o modelo já está residente
- Primeiro `init` baixa o modelo quantizado uma vez e armazena em cache local
- Modelo de embedding usa aproximadamente 1100 MB de RAM por instância de processo após a calibração de RSS da v1.0.18 com daemon (regressão de 52 GiB na v1.0.17 reduzida a pico de 1.03 GiB)


## Invocação Paralela Segura
### Semáforo de contagem com até quatro slots simultâneos
- Cada invocação carrega `multilingual-e5-small` consumindo aproximadamente 1100 MB de RAM após a medição da v1.0.18
- `MAX_CONCURRENT_CLI_INSTANCES` continua sendo o teto rígido de 4 subprocessos cooperantes
- Comandos pesados `init`, `remember`, `recall` e `hybrid-search` podem ser reduzidos dinamicamente para baixo desse teto quando a RAM disponível não sustenta o paralelismo com segurança
- Arquivos de lock em `~/.cache/sqlite-graphrag/cli-slot-{1..4}.lock` usando `flock`
- Uma quinta invocação aguarda até 300 segundos e então encerra com código 75
- Use `--max-concurrency N` para solicitar o limite de slots na invocação atual; comandos pesados ainda podem ser reduzidos automaticamente
- Memory guard aborta com saída 77 quando há menos de 2 GB de RAM disponível
- SIGINT e SIGTERM disparam shutdown graceful via atômica `shutdown_requested()`


## Solução de Problemas
### Problemas comuns e correções
- O comportamento padrão sempre cria ou abre `graphrag.sqlite` no diretório atual
- Banco travado após crash exige `sqlite-graphrag vacuum` para fazer checkpoint do WAL
- Primeiro `init` leva cerca de um minuto enquanto `fastembed` baixa o modelo quantizado
- Em `aarch64-unknown-linux-gnu`, comandos pesados de embedding resolvem `libonnxruntime.so` a partir de `ORT_DYLIB_PATH`, do diretório do executável, de `./lib/` e depois do diretório de cache de modelos
- Se comandos de embedding falharem no ARM64 GNU, aponte `ORT_DYLIB_PATH` para a `libonnxruntime.so` exata distribuída junto da binária
- Permissão negada no Linux indica falta de escrita no diretório de cache do usuário
- Detecção de namespace cai para `global` quando não há override explícito
- Invocações paralelas que excedem o limite seguro efetivo recebem saída 75 e DEVEM tentar com backoff; durante auditorias inicie comandos pesados com `--max-concurrency 1`


## Crates Rust Compatíveis
### Invoque sqlite-graphrag de qualquer framework Rust de IA via subprocesso
- Cada crate chama o binário via `std::process::Command` com a flag `--json`
- Nenhuma memória compartilhada ou FFI necessária: o contrato é JSON puro em stdout
- Fixe a versão do binário no `Cargo.toml` do workspace para builds reproduzíveis
- Todos os 18 crates abaixo funcionam identicamente em Linux, macOS Apple Silicon e Windows

### rig-core
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "project goals", "--k", "5", "--json"])
    .output().unwrap();
```

### swarms-rs
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "agent memory", "--k", "10", "--json"])
    .output().unwrap();
```

### autoagents
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "task-context", "--type", "project",
           "--description", "current sprint goal", "--body", "finish auth module"])
    .output().unwrap();
```

### graphbit
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "decision log", "--k", "3", "--json"])
    .output().unwrap();
```

### agentai
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "previous decisions", "--k", "5", "--json"])
    .output().unwrap();
```

### llm-agent-runtime
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "user preferences", "--k", "5", "--json"])
    .output().unwrap();
```

### anda
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["stats", "--json"])
    .output().unwrap();
```

### adk-rust
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "tool outputs", "--k", "5", "--json"])
    .output().unwrap();
```

### rs-graph-llm
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "graph relations", "--k", "10", "--json"])
    .output().unwrap();
```

### genai
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "model context", "--k", "5", "--json"])
    .output().unwrap();
```

### liter-llm
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "session-notes", "--type", "user",
           "--description", "resumo da sessão", "--body", "discutimos arquitetura"])
    .output().unwrap();
```

### llm-cascade
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "fallback context", "--k", "3", "--json"])
    .output().unwrap();
```

### async-openai
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "system prompt history", "--k", "5", "--json"])
    .output().unwrap();
```

### async-llm
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "chat context", "--k", "5", "--json"])
    .output().unwrap();
```

### anthropic-sdk
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "tool use patterns", "--k", "5", "--json"])
    .output().unwrap();
```

### ollama-rs
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "local model outputs", "--k", "5", "--json"])
    .output().unwrap();
```

### mistral-rs
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "inference context", "--k", "10", "--json"])
    .output().unwrap();
```

### llama-cpp-rs
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "llama session context", "--k", "5", "--json"])
    .output().unwrap();
```


## Contribuindo
### Pull requests são bem-vindos
- Leia as diretrizes de contribuição em [CONTRIBUTING.md](CONTRIBUTING.md)
- Abra issues no repositório do GitHub para bugs ou pedidos de funcionalidade
- Siga o código de conduta descrito em [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)


## Segurança
### Política de divulgação responsável
- Reportes de segurança seguem a política descrita em [SECURITY.md](SECURITY.md)
- Contate o mantenedor em privado antes de divulgar vulnerabilidades publicamente


## Histórico de Mudanças
### Histórico de releases mantido em arquivo separado
- Leia o histórico completo de releases em [CHANGELOG.md](CHANGELOG.md)


## Agradecimentos
### Construído sobre excelente código aberto
- `fastembed` fornece modelos de embedding locais quantizados sem complicação de ONNX
- `sqlite-vec` adiciona índices vetoriais dentro do SQLite como extensão nativa
- `refinery` executa migrações de schema com garantias transacionais
- `clap` potencializa o parsing de argumentos da CLI com macros derive
- `rusqlite` encapsula o SQLite com bindings Rust seguros e build embutido


## Licença
### Licença dual MIT OR Apache-2.0
- Licenciado sob Apache License 2.0 ou MIT License à sua escolha
- Veja `LICENSE-APACHE` e `LICENSE-MIT` na raiz do repositório para texto completo
