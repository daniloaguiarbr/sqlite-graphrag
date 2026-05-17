# sqlite-graphrag

[![Crates.io](https://img.shields.io/crates/v/sqlite-graphrag.svg)](https://crates.io/crates/sqlite-graphrag)
[![Docs.rs](https://docs.rs/sqlite-graphrag/badge.svg)](https://docs.rs/sqlite-graphrag)
[![CI](https://github.com/daniloaguiarbr/sqlite-graphrag/actions/workflows/ci.yml/badge.svg)](https://github.com/daniloaguiarbr/sqlite-graphrag/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](LICENSE)
[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg)](CODE_OF_CONDUCT.md)

> Memória persistente para agentes de IA em um único binário Rust com GraphRAG embutido.

- Versão em inglês disponível em [README.md](README.md)
- O pacote público e o repositório já estão disponíveis no GitHub e no crates.io
- Instale a última release publicada com `cargo install sqlite-graphrag --locked`
- Atualize uma instalação existente com `cargo install sqlite-graphrag --locked --force`
- Verifique o binário ativo com `sqlite-graphrag --version`
- Veja o histórico completo de releases em [CHANGELOG.pt-BR.md](CHANGELOG.pt-BR.md)
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
- Toda invocação pode continuar stateless, mas comandos pesados sobem um daemon persistente para inferência de embeddings automaticamente, reutilizando-o entre chamadas (este é o autostart do daemon, separado da extração automática de entidades)
- Após upgrades do binário, a CLI detecta automaticamente incompatibilidade de versão com o daemon em execução e o reinicia de forma transparente antes do primeiro request de embedding (desde v1.0.50)
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
- `entity_type` aceita exatamente 13 valores: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- `relation` (entrada CLI) aceita qualquer string em kebab-case ou snake_case. 12 valores canônicos são bem conhecidos: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`. Valores customizados (ex.: `implements`, `tested-by`, `blocks`) são aceitos com um `tracing::warn!`. A saída JSON normaliza para underscores (ex.: `applies_to`).
- `strength` é um float em `[0.0, 1.0]` representando o peso da aresta; mapeado para `weight` em todos os outputs de leitura
- Valores de `entity_type` não listados são rejeitados na escrita com código de saída 1. Valores customizados de `relation` são aceitos desde v1.0.49.
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
> **Flags obrigatórias para `remember`:** `--name`, `--type`, `--description`. Body via `--body "texto"`, `--body-file <caminho>`, ou `--body-stdin` (pipe do stdin).
> **Limite do body: 500 KB (512000 bytes).** Entradas maiores são rejeitadas com código de saída 6 (`limit exceeded`); divida em múltiplas memórias ou reduza antes de enviar.
- **GraphRAG está habilitado por padrão e roda automaticamente.** Cada subcomando auto-inicializa `graphrag.sqlite` no diretório de trabalho atual se ele não existir. `remember` e `ingest` podem extrair entidades e relacionamentos via GLiNER zero-shot NER local quando `--enable-ner` é passado. `recall` e `hybrid-search` auto-iniciam o daemon de embedding sob demanda.

### GLiNER zero-shot NER
- Passe `--enable-ner` ou defina `SQLITE_GRAPHRAG_ENABLE_NER=1` para ativar extração de entidades em `remember` e `ingest`
- Funciona com `--graph-stdin`: passe `"entities": []` no payload JSON e o GLiNER extrai entidades automaticamente
- Selecione variante do modelo com `--gliner-variant`: `fp32` (1,1 GB, melhor qualidade), `fp16` (580 MB), `int8` (349 MB, mais rápido), `q4` (894 MB), `q4f16` (472 MB)
- Sobrescreva modelo padrão via `SQLITE_GRAPHRAG_GLINER_MODEL`; ajuste confiança com `SQLITE_GRAPHRAG_GLINER_THRESHOLD` (padrão `0.5`)
- Campo `extraction_method` na resposta reporta: `gliner-<variant>+regex`, `regex-only` ou `none:extraction-failed`
- `--skip-extraction` está obsoleto desde v1.0.45; NER está desligado por padrão, use `--enable-ner` para ativar

- **`sqlite-graphrag init` é OPCIONAL** mas recomendado no primeiro uso porque pré-baixa o modelo de embedding e aquece um embedding de teste (comandos subsequentes são mais rápidos). Sem `init`, o primeiro comando paga o custo de download do modelo.
- **`graphrag.sqlite` é criado no diretório de trabalho atual por padrão** (sobrescreva com `--db <caminho>` ou `SQLITE_GRAPHRAG_DB_PATH`)
- Para o checkout local, `cargo install --path .` é suficiente
- Reexecute `sqlite-graphrag --version` após qualquer upgrade para confirmar o binário ativo
- Depois da release pública, prefira `--locked` para preservar o grafo de dependências validado para o MSRV


## Destaques da Versão

- **v1.0.55**: Auditoria completa de docs — export summary `total`→`exported`, campos de resposta do list corrigidos, exit code de `--tz` 1→2, exit 2 adicionado à tabela de exit codes, aliases legados do stats documentados
- **v1.0.54**: WAL checkpoint para `prune-relations` (último comando faltante), validação de body vazio com `--graph-stdin`, campo JSON `memory_type` em `list`/`export`, `Vec::with_capacity` em 9 cold paths
- **v1.0.53**: WAL checkpoint TRUNCATE após cada escrita para segurança com Dropbox/cloud-sync, correção do contrato `export --json`, `Vec::with_capacity` em 12 hot paths
- **v1.0.52**: 12 gaps corrigidos, novo subcomando `export`, exit code Duplicate 2→9 (breaking), `forget` not-found sem JSON (breaking)
- **v1.0.51**: Correção da env var de namespace (8 comandos), correção do remember em memória soft-deletada, watchdog de RSS por chunk (`--max-rss-mb`), cobertura de testes do daemon
- **v1.0.50**: Subcomando `prune-relations`, auto-restart do daemon em version mismatch, índice V011, 37 lacunas de docs corrigidas
- **v1.0.49**: Vocabulário extensível de relações, migração V010, 15 atualizações de docs
- **v1.0.48**: GLiNER NER funcional, 5 correções de bugs, auditoria completa de docs
- **v1.0.47**: Substituição do BERT NER pelo GLiNER zero-shot, 13 tipos de entidade customizados, flag `--gliner-variant`
- **v1.0.35**: Aliases de flags (`--from`/`--to`, `--old`/`--new`, `--limit` como alias de `--k`)


## Ciclo de Vida da Memória
### Sequência executável: init → remember → recall → forget → purge
```bash
# 1. Inicializar (uma vez por banco)
sqlite-graphrag init

# 2. Armazenar uma memória
sqlite-graphrag remember --name minha-nota --type user --description "demo" --body "primeira entrada"

# 3. Recuperar por similaridade semântica
sqlite-graphrag recall "primeira entrada" --k 5 --json

# 4. Exclusão suave (reversível)
sqlite-graphrag forget minha-nota

# 5. Remover permanentemente memórias soft-deleted com 0 dias de retenção
sqlite-graphrag purge --retention-days 0 --yes
```
> Todos os cinco comandos acima são seguros para executar em sequência em um banco recém-criado.


## Instalação
### Múltiplos canais de distribuição
- Instale a última release publicada com `cargo install sqlite-graphrag --locked`
- Atualize um binário publicado existente com `cargo install sqlite-graphrag --locked --force`
- Para fixar uma versão específica use `cargo install sqlite-graphrag --version <X.Y.Z> --locked`
- Instale a partir do checkout local com `cargo install --path .`
- Compile a partir do checkout local com `cargo build --release`


## Uso
### Inicialize o banco de dados
```bash
sqlite-graphrag init
sqlite-graphrag init --namespace projeto-foo
```
- Sem `--db` ou `SQLITE_GRAPHRAG_DB_PATH`, todo comando CRUD nessa pasta usa `./graphrag.sqlite`
### Grave uma memória com grafo de entidades explícito opcional
- Por padrão, `remember` NÃO executa extração automática de entidades (GLiNER NER desabilitado por padrão)
- Passe `--enable-ner` para ativar a extração GLiNER zero-shot nessa chamada, ou defina `SQLITE_GRAPHRAG_ENABLE_NER=1`
```bash
sqlite-graphrag remember \
  --name testes-integracao-postgres \
  --type feedback \
  --description "prefira Postgres real a mocks SQLite" \
  --body "Testes de integração devem usar banco real."
```
- A resposta JSON de `remember` inclui `urls_persisted` (URLs roteadas para a tabela `memory_urls`) e `relationships_truncated` (bool, ativo quando relacionamentos foram truncados)
- URLs são armazenadas em `memory_urls` via schema V007 e nunca poluem o grafo de entidades
- Exemplo de saída JSON ilustrando entidades e relacionamentos extraídos (chaves em inglês por convenção):
```json
{
  "memory": {"id": 42, "name": "audit-note", "type": "project"},
  "extracted_entities": [
    {"name": "OpenAI", "kind": "organization", "saliency": 0.92},
    {"name": "Rust", "kind": "technology", "saliency": 0.85}
  ],
  "extracted_relationships": [
    {"source": "OpenAI", "target": "GPT-4", "relation": "develops"}
  ],
  "urls_persisted": [],
  "relationships_truncated": false
}
```
### Ative auto-extração GLiNER NER para enriquecimento de entidades
- GLiNER zero-shot NER é desabilitado por padrão; passe `--enable-ner` para ativar extração automática de entidades/relacionamentos
- GLiNER substitui o modelo BERT NER anterior e resolve 13 tipos de entidade específicos do domínio vs. 4 tipos fixos do BERT
- Use `--gliner-variant` para equilibrar qualidade e tamanho de download: `fp32` (padrão, 1,1 GB), `fp16` (580 MB), `int8` (349 MB), `q4` (894 MB), `q4f16` (472 MB)
- O campo `extraction_method` é populado na resposta JSON quando NER roda

| Variante | Tamanho | Notas |
|----------|---------|-------|
| `fp32` | 1,1 GB | Padrão; melhor acurácia |
| `fp16` | 580 MB | Boa acurácia, metade do tamanho |
| `int8` | 349 MB | Menor; leve redução de acurácia |
| `q4` | 894 MB | Pesos quantizados em 4 bits |
| `q4f16` | 472 MB | Pesos 4 bits, ativações fp16 |

```bash
sqlite-graphrag remember \
  --name notas-de-release-v1 \
  --type document \
  --description "notas de release para v1.0.0" \
  --enable-ner \
  --gliner-variant fp16 \
  --body-stdin < notas.md
```
### Leia, esqueça, edite e renomeie usando argumento posicional
<!-- skip-test: forget soft-deleta a memória no meio do bloco, invalidando o edit/rename seguintes. O bloco ilustra o ciclo de vida; não é um script executável. -->
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
> **Retenção padrão: 90 dias.** Para purgar TODAS as memórias esquecidas independentemente da idade, passe `--retention-days 0`.

### Execute ou controle o daemon persistente de embeddings
<!-- skip-test: `daemon --idle-shutdown-secs` roda em foreground e bloquearia o teste indefinidamente. `--ping`/`--stop` exigem um daemon já em execução. -->
```bash
sqlite-graphrag daemon --idle-shutdown-secs 600
sqlite-graphrag daemon --ping --json
sqlite-graphrag daemon --stop --json
```

### Comportamento de auto-spawn do daemon

`recall`, `hybrid-search` e outros subcomandos com embeddings pesados sobem automaticamente um daemon em segundo plano (`sqlite-graphrag daemon`) quando nenhum está em execução, amortizando o custo de aquecimento do modelo entre múltiplas invocações.

**Padrão**: auto-spawn habilitado (timeout de ociosidade 600s).

**Desabilitar por invocação** via flag:

```bash
sqlite-graphrag recall "consulta" --autostart-daemon=false
```

**Desabilitar globalmente** via variável de ambiente:

```bash
export SQLITE_GRAPHRAG_DAEMON_DISABLE_AUTOSTART=1
```

A flag `--autostart-daemon` tem precedência sobre a variável de ambiente.

**Controle explícito do ciclo de vida** (foreground, timeout padrão de 600s):

<!-- skip-test: `daemon` roda em foreground e bloqueia; `--ping`/`--stop` requerem daemon em execução. -->
```bash
sqlite-graphrag daemon
sqlite-graphrag daemon --idle-shutdown-secs 3600
sqlite-graphrag daemon --ping            # verificação de saúde
sqlite-graphrag daemon --stop            # desligamento gracioso
```
> **Convenção do daemon:** usa FLAGS `--ping`/`--stop`/`--idle-shutdown-secs`, não subcomandos. Espelha flags no estilo systemd em vez do padrão verbo-substantivo do git.

### Ingestão em massa de arquivos Markdown em um diretório
<!-- skip-test: requer um diretório `./docs` com arquivos Markdown relativo ao cwd da invocação. -->
```bash
sqlite-graphrag ingest ./docs --type document --pattern '*.md' --recursive
```
### Ingestão em massa em modo de baixa memória (worker único)
<!-- skip-test: requer um diretório `./docs`; demonstra a flag --low-memory. -->
```bash
# Força ingest single-threaded para reduzir pressão de RSS (recomendado para
# ambientes com <4 GB de RAM e restrições de container/cgroup). Trade-off: 3-4x
# mais tempo de relógio.
sqlite-graphrag ingest ./docs --type document --pattern '*.md' --low-memory

# Ou via variável de ambiente (a flag CLI tem precedência):
SQLITE_GRAPHRAG_LOW_MEMORY=1 sqlite-graphrag ingest ./docs --type document
```
> `ingest` emite NDJSON no stdout: uma linha JSON por arquivo, seguida de uma linha de resumo.
> Valores de `status` por arquivo: `indexed` (criado), `skipped` (duplicata ou nome inválido), `failed` (erro).
> Duplicatas emitem `status: "skipped"` com `action: "duplicate"` e não contam como falhas.
> Passe `--dry-run` para pré-visualizar o mapeamento de nomes (basenames em kebab-case) sem escrever nada no banco.
> Schema: `docs/schemas/ingest-file-event.schema.json`, `docs/schemas/ingest-summary.schema.json`.

### Renomeie uma memória mantendo o histórico de versões
<!-- skip-test: nomes ilustrativos (`nome-antigo`, `nome-novo`) — a memória de origem não existe no banco isolado de teste. -->
```bash
sqlite-graphrag rename nome-antigo --new-name nome-novo --json
```
### Edite corpo ou descrição de uma memória (gera nova versão)
<!-- skip-test: depende da memória não ter sido soft-deleted por um bloco ilustrativo anterior. -->
```bash
sqlite-graphrag edit testes-integracao-postgres --body "Corpo atualizado."
sqlite-graphrag edit testes-integracao-postgres --description "Descrição atualizada."
```
### Restaure uma memória para uma versão anterior
<!-- skip-test: `restore --version 2` exige que a memória tenha pelo menos duas versões, o que não é o caso no banco isolado de exemplo. -->
```bash
sqlite-graphrag history testes-integracao-postgres --json
sqlite-graphrag restore --name testes-integracao-postgres --version 2 --json
```
### Aplique migrações de schema pendentes
```bash
sqlite-graphrag migrate --status --json
sqlite-graphrag migrate --json
```
### Resolva a precedência de namespace para a invocação atual
```bash
sqlite-graphrag namespace-detect --json
sqlite-graphrag namespace-detect --namespace projeto-foo --json
```
### Atualize as estatísticas do planejador de queries do SQLite
```bash
sqlite-graphrag optimize --json
```
### Recupere espaço em disco e faça checkpoint do WAL
```bash
sqlite-graphrag vacuum --json
```
### Crie um relacionamento tipado entre duas entidades
<!-- skip-test: requer que as entidades `OpenAI` e `GPT-4` já existam no namespace. -->
```bash
sqlite-graphrag link --from "OpenAI" --to "GPT-4" --relation uses --weight 0.8 --json
```
### Remova um relacionamento específico entre duas entidades
<!-- skip-test: requer o relacionamento criado pelo exemplo `link` anterior. -->
```bash
sqlite-graphrag unlink --from "OpenAI" --to "GPT-4" --relation uses --json
```
### Percorra memórias conectadas via grafo de entidades
```bash
sqlite-graphrag related primeira-memoria --max-hops 2 --limit 10 --json
```
> **Resultados vazios são normais** para memórias sem arestas no grafo ainda — extraia entidades primeiro via `remember` ou `ingest`. Arestas se formam quando ≥2 entidades co-ocorrem no mesmo corpo de memória.

### Exporte um snapshot do grafo em json, dot ou mermaid
<!-- skip-test: `--output graph.json` escreve um arquivo relativo ao cwd da invocação; polui o workspace de teste. Os demais subcomandos read-only do graph são exercitados pelos testes de integração do cookbook. -->
```bash
sqlite-graphrag graph --format json --output graph.json
sqlite-graphrag graph stats --json
sqlite-graphrag graph traverse --from "OpenAI" --depth 2 --json
sqlite-graphrag graph entities --entity-type organization --limit 50 --json
```
### Remova entidades órfãs sem memórias e sem relacionamentos
```bash
sqlite-graphrag cleanup-orphans --dry-run --json
sqlite-graphrag cleanup-orphans --yes --json
```
### Remoção em massa de relacionamentos por tipo
<!-- skip-test: requer que existam relacionamentos no namespace. -->
```bash
sqlite-graphrag prune-relations --relation mentions --dry-run --show-entities --json
sqlite-graphrag prune-relations --relation mentions --yes --json
```
### Limpe os modelos de embedding/NER em cache no diretório XDG
<!-- skip-test: apaga o cache de modelos de embedding; seguro em produção, mas no suite de integração obriga um re-download caro nos comandos seguintes. -->
```bash
sqlite-graphrag cache clear-models --yes
```
### Liste todas as versões de uma memória
<!-- skip-test: depende do estado do ciclo de vida estabelecido por blocos ilustrativos anteriores (também marcados `skip-test`). -->
```bash
sqlite-graphrag history testes-integracao-postgres --no-body --json
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
| `remember` | `--name`, `--type`, `--description`, `--body` (ou `--body-file`/`--body-stdin`), `--entities-file`, `--relationships-file`, `--graph-stdin`, `--enable-ner`, `--gliner-variant` | Salva memória com grafo de entidades opcional |
| `recall` | `<query>`, `-k`/`--k` (alias `--limit` desde v1.0.35), `--type`, `--max-hops`, `--max-distance`, `--all-namespaces`, `--no-graph` | Busca memórias semanticamente via KNN + travessia do grafo |
| `read` | `[nome]` ou `--name <nome>` | Recupera memória por nome kebab-case exato |
| `list` | `--type`, `--limit`, `--offset`, `--include-deleted` | Pagina memórias ordenadas por `updated_at` |
| `forget` | `[nome]` ou `--name <nome>` | Remove memória logicamente preservando histórico |
| `rename` | `[antigo]`, ou `--name`/`--old`/`--from <NOME>` (desde v1.0.35), `--new-name`/`--new`/`--to <NOME>` (desde v1.0.35) | Renomeia memória mantendo versões |
| `edit` | `[nome]` ou `--name`, `--body`, `--description` | Edita corpo ou descrição gerando nova versão |
| `history` | `[nome]` ou `--name <nome>` | Lista todas as versões da memória |
| `restore` | `--name`, `--version` | Restaura memória para versão anterior |
| `ingest` | `<DIR>`, `--type`, `--pattern <GLOB>` (padrão `*.md`), `--recursive`, `--ingest-parallelism N`, `--low-memory` (env `SQLITE_GRAPHRAG_LOW_MEMORY=1`), `--enable-ner`, `--gliner-variant`, `--fail-fast`, `--dry-run` | Ingere em massa cada arquivo correspondente como memória separada (saída NDJSON); `--dry-run` pré-visualiza o mapeamento de nomes sem gravar |
| `export` | `--namespace`, `--type`, `--include-deleted`, `--limit`, `--offset` | Exporta memórias como NDJSON para backup ou migração |
| `cache clear-models` | `--yes` | Remove arquivos de modelo de embedding/GLiNER do diretório XDG cache |

> **Validação de nomes de memória.** Nomes devem corresponder a `[a-z0-9-]+` (kebab-case, somente ASCII).
> Unicode e maiúsculas são rejeitados com exit code 1. Nomes maiores que 60 caracteres
> emitidos por `ingest` são truncados; revise o log WARN para identificar nomes mutilados.
### Recuperação e grafo
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `hybrid-search` | `<query>`, `--k`, `--rrf-k`, `--with-graph`, `--max-hops`, `--min-weight` | FTS5 combinado com vetor via Reciprocal Rank Fusion; `--with-graph` adiciona matches por graph traversal |
| `namespace-detect` | `--namespace <nome>` | Resolve precedência de namespace para invocação |
| `link` | `--from`, `--to`, `--relation`, `--weight`, `--create-missing`, `--entity-type` | Cria relacionamento explícito entre duas entidades; `--create-missing` cria automaticamente entidades inexistentes (tipo padrão: `concept`) |
| `unlink` | `--from`, `--to`, `--relation` | Remove um relacionamento específico entre duas entidades |
| `related` | `--name`, `--limit`, `--hops` | Percorre memórias conectadas pelo grafo a partir de uma memória base |
| `graph` | `--format`, `--output` | Exporta snapshot do grafo em `json`, `dot` ou `mermaid` |

> **Breaking change em v1.0.44.** O JSON de `graph entities` renomeou o array de nível superior
> de `items` para `entities`. Atualize filtros jaq/jq: `.items[]` vira `.entities[]`.
> O comando `list` continua usando `items`.

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
| `prune-relations` | `--relation <tipo>`, `--namespace`, `--dry-run`, `--yes`, `--show-entities` | Remove em massa todos os relacionamentos de um determinado tipo; `--show-entities` lista as entidades afetadas na pré-visualização do dry-run |

### Subcomandos de `cache`
| Subcomando | Descrição |
| --- | --- |
| `clear-models` | Remove os arquivos de modelo de embedding/NER em cache (força novo download no próximo `init`) |


## Variáveis de Ambiente
### Overrides de configuração em runtime
| Variável | Descrição | Padrão | Exemplo |
| --- | --- | --- | --- |
| `SQLITE_GRAPHRAG_DB_PATH` | Caminho para override do arquivo SQLite | `./graphrag.sqlite` no diretório da invocação | `/dados/graphrag.sqlite` |
| `SQLITE_GRAPHRAG_HOME` | Sobrescreve diretório base para `graphrag.sqlite` (usado quando `--db` e `SQLITE_GRAPHRAG_DB_PATH` estão ausentes) | indefinido | `/var/lib/sqlite-graphrag` |
| `SQLITE_GRAPHRAG_CACHE_DIR` | Diretório de override para cache do modelo e lock files | Diretório XDG cache | `~/.cache/sqlite-graphrag` |
| `SQLITE_GRAPHRAG_LANG` | Idioma da saída da CLI como `en` ou `pt` (alias: `pt-BR`, `portuguese`) | `en` | `pt` |
| `SQLITE_GRAPHRAG_LOG_LEVEL` | Nível do filtro de tracing para saída em stderr | `info` | `debug` |
| `SQLITE_GRAPHRAG_LOG_FORMAT` | Formato da saída de tracing em stderr (`pretty` ou `json`) | `pretty` | `json` |
| `SQLITE_GRAPHRAG_NAMESPACE` | Override de namespace ignorando detecção | nenhum | `projeto-foo` |
| `SQLITE_GRAPHRAG_DISPLAY_TZ` | Fuso horário IANA para campos `*_iso` no JSON | `UTC` | `America/Sao_Paulo` |
| `SQLITE_GRAPHRAG_DAEMON_FORCE_AUTOSTART` | Força o autostart do daemon mesmo quando os guards o pulariam | indefinido | `1` |
| `SQLITE_GRAPHRAG_DAEMON_DISABLE_AUTOSTART` | Desabilita completamente o autostart do daemon (útil em testes/CI) | indefinido | `1` |
| `SQLITE_GRAPHRAG_DAEMON_CHILD` | Flag INTERNA setada automaticamente ao spawnar o filho do daemon; não setar manualmente | indefinido | `1` |
| `SQLITE_GRAPHRAG_ENABLE_NER` | Habilita extração GLiNER NER automaticamente (equivalente a `--enable-ner` em toda chamada). Aceita `1`/`true`/`yes`/`on` (case-insensitive) | indefinido (NER desligado) | `1` |
| `SQLITE_GRAPHRAG_GLINER_VARIANT` | Variante de pesos ONNX do GLiNER: `fp32`, `fp16`, `int8`, `q4`, `q4f16` | `fp32` | `fp16` |
| `SQLITE_GRAPHRAG_GLINER_THRESHOLD` | Limiar de confiança para predições GLiNER (float em [0.0, 1.0]) | `0.5` | `0.3` |
| `SQLITE_GRAPHRAG_GLINER_MODEL` | Sobrescreve o identificador do repositório do modelo GLiNER | `onnx-community/gliner_multi-v2.1` | caminho personalizado |
| `SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS` | Budget de tokens para extração de entidades/relações por memória; valores fora de [512, 100.000] utilizam o padrão | `5000` | `8000` |
| `SQLITE_GRAPHRAG_MAX_ENTITIES_PER_MEMORY` | Máximo de entidades distintas persistidas por memória; valores fora de [1, 1.000] utilizam o padrão. Nota: o pipeline de extração limita internamente os candidatos a 30 antes da deduplicação, portanto o cap de persistência (padrão 50) funciona como teto de segurança e só é atingido se o extrator for estendido ou substituído. | `50` | `100` |
| `SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY` | Máximo de relações distintas persistidas por memória; valores fora de [1, 10.000] utilizam o padrão | `50` | `200` |
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
| Código | Significado | Causa Possível |
| --- | --- | --- |
| `0` | Sucesso | Comando concluído e payload JSON impresso quando solicitado |
| `1` | Erro de validação ou falha em runtime | `--type` inválido, `--relation` malformado (vazio ou fora de snake_case), violação de kebab-case, erro genérico anyhow |
| `2` | Erro de uso da CLI | Flag inválida, argumento obrigatório ausente, timezone `--tz` inválido (Clap `FromStr` rejeita antes do código da aplicação) |
| `9` | Duplicata detectada | `--name` existente sem `--force-merge`; o `ingest` pula o arquivo e emite `status: "skipped"` com `action: "duplicate"` |
| `3` | Conflito durante atualização otimista | `edit` ou `restore` competiu com outro escritor |
| `4` | Memória ou entidade não encontrada | Alvo de `read`, `forget`, `edit`, `rename`, `restore` ou `graph traverse` ausente |
| `5` | Namespace não pôde ser resolvido | Sem `SQLITE_GRAPHRAG_NAMESPACE`, sem flag, sem padrão detectado |
| `6` | Payload excedeu limites configurados | `--name` maior que 80 bytes, body acima de `512000` bytes, mais de `512` chunks |
| `10` | Erro do banco SQLite | Arquivo corrompido, schema divergente, migração ausente |
| `11` | Geração de embedding falhou | Erro ao carregar modelo ou falha de RPC do daemon de embedding |
| `12` | Extensão `sqlite-vec` falhou ao carregar | Extensão nativa ausente ou build do SQLite incompatível |
| `13` | Falha parcial em lote | `import`, `reindex` ou stdin batch com pelo menos um registro com falha |
| `14` | Erro de I/O do sistema de arquivos | Diretório de cache ou de banco sem permissão de escrita, diretório de destino `ingest` inexistente |
| `15` | Banco ocupado após tentativas | Contenção do WAL excedeu o orçamento de `with_busy_retry` |
| `20` | Erro interno ou de serialização JSON | Falha inesperada do serde ou violação de invariante |
| `75` | `EX_TEMPFAIL` lock timeout ou todos os slots ocupados | Cinco ou mais invocações concorrentes ou `flock` esperou mais de 300s |
| `77` | RAM disponível abaixo do mínimo | Menos de 2 GB de RAM livre detectados antes do load do modelo |


## Desempenho
### Medido em banco com 1000 memórias
- A latência em processo com modelo já aquecido continua muito menor que a latência da CLI stateless
- Invocações stateless da CLI tipicamente gastam cerca de um segundo recarregando o modelo em cada comando pesado
- Recall aquecido em processo pode ficar bem abaixo da latência da CLI stateless quando o modelo já está residente
- Primeiro `init` baixa o modelo quantizado uma vez e armazena em cache local
- Modelo de embedding usa aproximadamente 1100 MB de RAM por instância de processo após a calibração de RSS da v1.0.18 com daemon (regressão de 52 GiB na v1.0.17 reduzida a pico de 1.03 GiB)


## Requisitos de Memória
### Dimensionando RAM para cargas de ingest e recall
- Mínimo de 3 GB de RAM recomendado (4 GB+ para corpora grandes). O piso fica em torno de 2 GB apenas para carregar ONNX runtime + GLiNER NER + fastembed multilingual-e5-small.
- Paralelismo padrão (`--ingest-parallelism = min(4, cpus/2)`) aumenta o RSS de forma quase linear por worker. Com 4 workers, o ingest de 30 arquivos pico em torno de 4,4 GB.
- Modo de baixa memória: passe `--low-memory` (ou defina `SQLITE_GRAPHRAG_LOW_MEMORY=1`) para forçar ingest single-threaded. Equivale a `--ingest-parallelism 1` e sobrescreve qualquer valor explícito. Reduz o pico de RSS para cerca de 2,6 GB ao custo de 3-4x mais tempo de relógio.
- Usuários de container/cgroup: limite abaixo de 3 GB causa OOM-kill durante o load do modelo. Use cgroup `MemoryMax=4G` ou superior em produção.
- Acompanhamento upstream: veja https://github.com/microsoft/onnxruntime/issues/22271 sobre crescimento de memória da CPU no ONNX após muitas execuções.


## Espaço em Disco
### Tamanho esperado do banco em relação ao conteúdo ingerido
> **Overhead esperado: aproximadamente 8× o tamanho total dos corpos ingeridos** (ex.: 7,6 MB de texto → ~62,9 MB de banco).
> O overhead vem dos embeddings float de 384 dimensões, do índice FTS5 e do grafo de entidades/relacionamentos.
> Execute `sqlite-graphrag vacuum --json` após ciclos de `forget`+`purge` em massa para recuperar espaço.


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
### Segurança com cloud sync (Dropbox, iCloud, OneDrive)
- sqlite-graphrag usa modo WAL por padrão para escrita de alta concorrência
- Desde v1.0.54, todo comando de escrita executa `PRAGMA wal_checkpoint(TRUNCATE)` após commit (v1.0.53 cobriu 11 de 12; v1.0.54 adicionou o `prune-relations` faltante)
- Isso garante que o arquivo `.sqlite` esteja sempre autocontido quando ferramentas de cloud sync o leem
- Se ocorrer corrupção apesar do checkpoint, recupere com `sqlite3 corrompido.sqlite ".recover" | sqlite3 reparado.sqlite`

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


## JSON Schemas
### Contratos canônicos para cada resposta de subcomando
- JSON Schemas autoritativos para cada resposta `--json` ficam em [`docs/schemas/`](docs/schemas/) e são versionados junto com a crate
- 35 schemas cobrem `init`, `remember`, `recall`, `hybrid-search`, `list`, `read`, `forget`, `purge`, `rename`, `edit`, `history`, `restore`, `link`, `unlink`, `prune-relations`, `health`, `stats`, `migrate`, `vacuum`, `optimize`, `cleanup-orphans`, `sync-safe-copy`, `graph` (+ stats/traverse/entities), `related`, `namespace-detect`, `debug-schema`, `entities-input`, `relationships-input`, `ingest-file-event`, `ingest-summary`, `export-memory-line`, `export-summary`
- Trate estes schemas como o contrato de agente; SKILL.md documenta as mesmas formas em formato humano
- Valide consumidores downstream com qualquer validador JSON Schema padrão (e.g. `ajv`, `jsonschema`)


## Histórico de Mudanças
### Histórico de releases mantido em arquivo separado
- [PRD](docs/PRD.pt-BR.md) — Documento de Requisitos de Produto (fonte de verdade dos 31 contratos comportamentais)
- Leia o histórico completo de releases em [CHANGELOG.pt-BR.md](CHANGELOG.pt-BR.md)


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
