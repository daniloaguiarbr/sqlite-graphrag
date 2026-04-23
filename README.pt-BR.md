# sqlite-graphrag

27 agentes de IA. Um binário de 25 MB. Zero chamadas à nuvem.

[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](LICENSE)
[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg)](CODE_OF_CONDUCT.md)

> Memória persistente para 27 agentes de IA em um binário Rust de 25 MB

- Versão em inglês disponível em [README.md](README.md)
- O pacote público e o repositório já estão disponíveis no GitHub e no crates.io
- Instale a release publicada atual com `cargo install sqlite-graphrag --version 1.0.2 --locked`

```bash
cargo install --path .
```


## O que é?
### sqlite-graphrag entrega memória durável para agentes de IA
- Armazena memórias, entidades e relacionamentos em um único arquivo SQLite abaixo de 25 MB
- Gera embeddings localmente via `fastembed` com o modelo `multilingual-e5-small`
- Combina busca textual FTS5 com KNN do `sqlite-vec` em ranqueador híbrido Reciprocal Rank Fusion
- Extrai grafo de entidades com arestas tipadas para recuperação multi-hop entre memórias
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
- Toda invocação é stateless com códigos de saída explícitos para decisões de roteamento
- Nota: a CLI é stateless — cada invocação recarrega o modelo de embeddings (~1s); modo daemon com latência <50ms está planejado para v3.0.0
- Toda escrita é idempotente via restrições de unicidade em `--name` kebab-case
- Stdin aceita corpos ou payloads JSON para entidades e relacionamentos em lote
- Payloads de relacionamento usam `strength` em `[0.0, 1.0]`, mapeado para `weight` nas saídas
- Stderr carrega saída de tracing apenas sob `SQLITE_GRAPHRAG_LOG_LEVEL=debug`
- Comportamento cross-platform é idêntico em hosts Linux, macOS e Windows
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
cargo install --path .
sqlite-graphrag init
sqlite-graphrag remember --name primeira-memoria --type user --description "primeira memória" --body "olá graphrag"
sqlite-graphrag recall "graphrag" --k 5 --json
```
- Para o checkout local, `cargo install --path .` é suficiente
- Depois da release pública, prefira `--locked` para preservar o grafo de dependências validado para o MSRV


## Instalação
### Múltiplos canais de distribuição
- Instale a partir do checkout local com `cargo install --path .`
- Compile a partir do checkout local com `cargo build --release`
- Fórmula Homebrew planejada sob `brew install sqlite-graphrag`
- Bucket Scoop planejado sob `scoop install sqlite-graphrag`
- Imagem Docker planejada como `ghcr.io/daniloaguiarbr/sqlite-graphrag:1.0.2`


## Uso
### Inicialize o banco de dados
```bash
sqlite-graphrag init
sqlite-graphrag init --namespace projeto-foo
```
### Grave uma memória com grafo de entidades
```bash
sqlite-graphrag remember \
  --name testes-integracao-postgres \
  --type feedback \
  --description "prefira Postgres real a mocks SQLite" \
  --body "Testes de integração devem usar banco real."
```
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
| `health` | `--json` | Exibe integridade e status dos pragmas |
| `stats` | `--json` | Conta memórias, entidades e relacionamentos |
| `migrate` | `--json` | Aplica migrações pendentes via `refinery` |
| `vacuum` | `--json` | Faz checkpoint do WAL e libera espaço |
| `optimize` | `--json` | Executa `PRAGMA optimize` para atualizar estatísticas |
| `sync-safe-copy` | `--dest <caminho>` (alias `--output`) | Gera cópia segura para sincronização em nuvem |
### Ciclo de vida do conteúdo de memória
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `remember` | `--name`, `--type`, `--description`, `--body` | Salva memória com grafo de entidades opcional |
| `recall` | `<query>`, `--k`, `--type` | Busca memórias semanticamente via KNN |
| `read` | `--name <nome>` | Recupera memória por nome kebab-case exato |
| `list` | `--type`, `--limit`, `--offset` | Pagina memórias ordenadas por `updated_at` |
| `forget` | `--name <nome>` | Remove memória logicamente preservando histórico |
| `rename` | `--old <nome>`, `--new <nome>` | Renomeia memória mantendo versões |
| `edit` | `--name`, `--body`, `--description` | Edita corpo ou descrição gerando nova versão |
| `history` | `--name <nome>` | Lista todas as versões da memória |
| `restore` | `--name`, `--version` | Restaura memória para versão anterior |
### Recuperação e grafo
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `hybrid-search` | `<query>`, `--k`, `--rrf-k` | FTS5 combinado com vetor via Reciprocal Rank Fusion |
| `namespace-detect` | `--namespace <nome>` | Resolve precedência de namespace para invocação |
### Manutenção
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `purge` | `--retention-days <n>`, `--dry-run`, `--yes` | Apaga permanentemente memórias soft-deleted |


## Variáveis de Ambiente
### Overrides de configuração em runtime
| Variável | Descrição | Padrão | Exemplo |
| --- | --- | --- | --- |
| `SQLITE_GRAPHRAG_DB_PATH` | Caminho para override do arquivo SQLite | `./graphrag.sqlite` no diretório da invocação | `/dados/graphrag.sqlite` |
| `SQLITE_GRAPHRAG_CACHE_DIR` | Diretório de override para cache do modelo e lock files | Diretório XDG cache | `~/.cache/sqlite-graphrag` |
| `SQLITE_GRAPHRAG_LANG` | Idioma da saída da CLI como `en` ou `pt` (alias: `pt-BR`, `portuguese`) | `en` | `pt` |
| `SQLITE_GRAPHRAG_LOG_LEVEL` | Nível do filtro de tracing para saída em stderr | `info` | `debug` |
| `SQLITE_GRAPHRAG_NAMESPACE` | Override de namespace ignorando detecção | nenhum | `projeto-foo` |


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
### Build Docker Alpine para pipelines de CI
```dockerfile
FROM rust:1.88-alpine AS builder
RUN apk add musl-dev sqlite-dev
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
| `75` | `EX_TEMPFAIL`: todos os slots de concorrência ocupados |
| `77` | RAM disponível abaixo do mínimo para carregar o modelo |


## Desempenho
### Medido em banco com 1000 memórias
- Startup a frio abaixo de 50 milissegundos em Apple Silicon ARM64 nativo
- Recall com `--k 5` completa abaixo de 20 milissegundos após carga do modelo
- Hybrid search com RRF completa abaixo de 30 milissegundos em cache quente
- Primeiro `init` baixa o modelo quantizado uma vez e armazena em cache local
- Modelo de embedding usa aproximadamente 750 MB de RAM por instância de processo


## Invocação Paralela Segura
### Semáforo de contagem com quatro slots simultâneos
- Cada invocação carrega `multilingual-e5-small` consumindo aproximadamente 750 MB de RAM
- Até quatro instâncias executam em paralelo via `MAX_CONCURRENT_CLI_INSTANCES` padrão
- Arquivos de lock em `~/.cache/sqlite-graphrag/cli-slot-{1..4}.lock` usando `flock`
- Uma quinta invocação aguarda até 300 segundos e então encerra com código 75
- Use `--max-concurrency N` para ajustar o limite de slots na invocação atual
- Memory guard aborta com saída 77 quando há menos de 2 GB de RAM disponível
- SIGINT e SIGTERM disparam shutdown graceful via atômica `shutdown_requested()`


## Solução de Problemas
### Problemas comuns e correções
- O comportamento padrão sempre cria ou abre `graphrag.sqlite` no diretório atual
- Banco travado após crash exige `sqlite-graphrag vacuum` para fazer checkpoint do WAL
- Primeiro `init` leva cerca de um minuto enquanto `fastembed` baixa o modelo quantizado
- Permissão negada no Linux indica falta de escrita no diretório de cache do usuário
- Detecção de namespace cai para `global` quando não há override explícito
- Invocações paralelas acima de quatro slots recebem saída 75 e DEVEM tentar com backoff


## Crates Rust Compatíveis
### Invoque sqlite-graphrag de qualquer framework Rust de IA via subprocesso
- Cada crate chama o binário via `std::process::Command` com a flag `--json`
- Nenhuma memória compartilhada ou FFI necessária: o contrato é JSON puro em stdout
- Fixe a versão do binário no `Cargo.toml` do workspace para builds reproduzíveis
- Todos os 18 crates abaixo funcionam identicamente em Linux, macOS e Windows

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
