---
name: sqlite-graphrag
description: Use esta skill SEMPRE que o usuário perguntar sobre adicionar memória persistente ou GraphRAG ou contexto de longo prazo ao Claude Code Codex Cursor Windsurf ou qualquer agente de IA de código. DEVE acionar para queries mencionando lembrar disso, salvar conversa, recuperar contexto anterior, busca híbrida, grafo de entidades, memória SQLite, RAG local, embeddings offline, fastembed, sqlite-vec, multilingual-e5, busca KNN, cópia memory-safe, fusão FTS5 e vec. Auto-invoca mesmo sem menção explícita quando usuário descreve problema de agente perdendo contexto entre sessões ou quer banco vetorial offline em Rust. Keywords memória RAG GraphRAG SQLite vetor embeddings Claude Codex Cursor Windsurf offline local persistente grafo entidade.
---


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
- ABORTAR quando nome IANA inválido retorna exit 1 (Validation)
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
- PASSAR `--force-merge` em loops idempotentes
- NER desabilitado por padrão; passar `--enable-ner` para ativar extração BERT
- RESPEITAR limite de 512000 bytes e 512 chunks por body
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
- NUNCA depender de auto-extração BERT em CI sensível a RAM
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
### OBRIGATÓRIO — Dois Eixos de Paralelismo
- `--max-concurrency <N>` controla CLI invocations simultâneas
- `--ingest-parallelism <N>` controla extract mais embed em paralelo
- PADRÃO de `--max-concurrency` é 4
- PADRÃO de `--ingest-parallelism` é `min(4, max(1, cpus/2))`
- DISTINGUIR claramente os dois eixos antes de ajustar
- AMPLIAR `--wait-lock <SECONDS>` para esperar slot antes de exit 75
### OBRIGATÓRIO — Performance e Extração
- NER desabilitado por padrão; passar `--enable-ner` para ativar extração BERT
- BERT NER adiciona aproximadamente 150 ms por arquivo com daemon ativo em hardware moderno
- BERT NER adiciona 2 a 30 segundos por arquivo em `--low-memory` ou sem daemon
- BERT NER adiciona aproximadamente 30 segundos no primeiro run para carregar modelo
- USAR `--enable-ner` apenas quando enriquecimento automático de entidades for valioso
- PREFERIR `--graph-stdin --skip-extraction` com entidades curadas por LLM para melhor qualidade
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
- Linha por arquivo: `file`, `name`, `status` (`"indexed"` `"skipped"` `"failed"`), `truncated`, `original_name?`, `memory_id?`, `action?`, `error?`
- Linha summary final: `summary` (true), `dir`, `pattern`, `recursive`, `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- Eventos de extração NER vão para stderr, NÃO stdout


## CRUD — Read com read e list
### OBRIGATÓRIO — Leitura Direta por Nome (read)
- USAR `read --name <kebab-case>` para fetch O(1) por nome
- PARSEAR campos `body`, `description`, `created_at_iso`, `updated_at_iso`
- TRATAR exit code 4 como memória inexistente no namespace
- APLICAR `--tz` para localizar timestamps na saída
### OBRIGATÓRIO — Enumeração com Filtros (list)
- USAR `list --type <kind>` para filtrar por tipo de memória
- AJUSTAR `--limit <N>` com padrão 50 conforme volume esperado
- PAGINAR via `--offset <N>` para datasets grandes
- INCLUIR memórias soft-deletadas via `--include-deleted`
- EXPORTAR full dump com `--limit 10000 --json` antes de backup
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
### OBRIGATÓRIO — Remoção Física (purge)
- USAR `purge --retention-days <N> --yes` em automação
- PADRÃO de retenção é 90 dias para memórias soft-deletadas
- EXECUTAR `--dry-run` primeiro para auditar contagem
- APAGA permanentemente linhas e reclama espaço em disco
### OBRIGATÓRIO — Remoção de Aresta (unlink)
- USAR `unlink --from <a> --to <b> --relation <tipo>`
- ACEITAR `--source`/`--target` como aliases de `--from`/`--to`
- TRATAR exit code 4 como aresta inexistente
- TODOS os três argumentos são obrigatórios sem exceção
### OBRIGATÓRIO — Limpeza de Entidades Órfãs (cleanup-orphans)
- EXECUTAR `cleanup-orphans --dry-run` para auditar
- APLICAR `--yes` em pipelines automatizados
- REMOVE entidades sem memórias vinculadas nem arestas
- RODAR periodicamente após operações `forget` em massa
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
- FILTRAR arestas fracas com `--min-weight <F>` (padrão 0.0)
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
- USAR antes de planejar travessias ou links em lote
### OBRIGATÓRIO — Estatísticas (graph stats)
- USAR `graph stats --json` antes de travessias caras
- INSPECIONAR `node_count`, `edge_count`, `avg_degree`, `max_degree`
- ESCOLHER profundidade de travessia baseada em densidade real
- DETECTAR isolamento de subgrafos antes de planejar buscas
### Vocabulário Canônico de Relações
- `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`
- `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
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
- `hybrid-search` retorna `results[].name`, `combined_score`, `score`, `vec_rank`, `fts_rank`, `source`, `body`
- `hybrid-search` response-level: `query`, `k`, `rrf_k`, `weights`, `results[]`, `graph_matches[]`, `elapsed_ms`
- `hybrid-search` `graph_matches[]` usa RecallItem: `name`, `distance`, `source` ("graph"), `graph_depth`
- `related` retorna `results[].name`, `hop_distance`, `relation`, `source_entity`, `target_entity`, `weight`
- `graph traverse` retorna `hops[].entity`, `relation`, `direction`, `weight`, `depth`
- `read` retorna `name`, `body`, `description`, `created_at_iso`, `updated_at_iso`
- `edit` retorna `memory_id`, `name`, `action` ("updated"), `version`, `elapsed_ms`
- `rename` retorna `memory_id`, `name` (novo), `action` ("renamed"), `version`, `elapsed_ms`
- `forget` retorna `action` (`"soft_deleted"`/`"already_deleted"`), `forgotten`, `name`, `namespace`, `elapsed_ms`
- `health` retorna `integrity_ok`, `schema_ok`, `vec_memories_ok`, `vec_entities_ok`, `vec_chunks_ok`, `fts_ok`, `model_ok`, `counts`, `wal_size_mb`, `journal_mode`, `db_path`, `db_size_bytes`, `checks[]`
- `health.counts` contém: `memories`, `entities`, `relationships`, `vec_memories`
- `stats` retorna dados GLOBAIS (sem filtro por namespace): `memories`, `entities`, `relationships`, `chunks_total`, `avg_body_len`, `namespaces[]`, `db_size_bytes`, `schema_version`, `elapsed_ms`
- `ingest` por arquivo: `file`, `name`, `status` (`"indexed"`/`"skipped"`/`"failed"`), `truncated`, `original_name?`, `memory_id?`, `action?`, `error?`
- `ingest` summary: `summary` (true), `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- `cache list` retorna modelos com tamanho em bytes e total de disco


## Códigos de Saída e Estratégia de Retry
### OBRIGATÓRIO — Tratamento Completo de Exit Codes
- `0` igual sucesso, parsear stdout
- `1` igual validação (peso inválido, self-link, timezone ruim, max-files excedido)
- `2` igual duplicata (memória já existe sem `--force-merge`)
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
- NUNCA confundir exit 1 (validação) com exit 2 (duplicata)


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
