# Livro de Receitas sqlite-graphrag


> 34 receitas de nível produção que poupam horas da sua equipe toda semana

- Leia a versão em inglês em [COOKBOOK.md](COOKBOOK.md)


## Aliases de Flags CLI (desde v1.0.35)
- `recall` e `hybrid-search` aceitam `--limit` como alias de `-k`/`--k`. As receitas abaixo usam `--k`; ambos funcionam.
- `rename` aceita `--from`/`--to` como aliases de `--name`/`--new-name`.
- Campos JSON `schema_version` (`init`, `stats`, `migrate`, `health`) são emitidos como números JSON desde v1.0.35.
- `rename` aceita argumentos posicionais: `rename <antigo> <novo>` (desde v1.0.44)
- `related` aceita argumento posicional de nome: `related <nome>` (desde v1.0.44)
- `graph entities` JSON response usa `entities` como chave de array top-level (renomeado de `items` em v1.0.44)
- `link --create-missing` auto-cria entidades inexistentes durante link (desde v1.0.44)
- `hybrid-search --with-graph` habilita travessia de grafo semeada dos top resultados RRF (desde v1.0.44)


## Nota de Latência
- O CLI pode rodar de forma stateless, mas `sqlite-graphrag daemon` mantém o modelo de embeddings residente para comandos pesados repetidos
- Para fluxos de produção com menor latência, inicie `sqlite-graphrag daemon` uma vez e deixe `init`, `remember`, `recall` e `hybrid-search` reutilizarem esse processo automaticamente
- O `recall` single-shot atual leva aproximadamente 1 segundo em hardware moderno
- Pipelines em lote amortizam esse custo invocando o binário uma vez por documento em paralelo
- `daemon --ping --json` verifica se o daemon está ativo; `daemon --stop` encerra graciosamente
- Veja Receita "Como Iniciar E Monitorar O Daemon Para Menor Latência" para detalhes de setup


## Referência de Valores Padrão
- `recall --k` padrão é 10 (não 5) — ajuste conforme o tradeoff precisão-revocação
- `list --limit` padrão é 50 — use `--limit 10000` para exportações completas antes de backup
- `hybrid-search --weight-vec` e `--weight-fts` ambos têm padrão 1.0
- `purge --retention-days` padrão é 90 — reduza para políticas de limpeza mais agressivas
- `ingest --max-files` padrão é 10000 — cap de segurança all-or-nothing, não janela deslizante
- `ingest --ingest-parallelism` padrão é `min(4, max(1, cpus/2))`
- `ingest --type` padrão é `document` quando omitido
- `link --weight` padrão é 0.5
- `graph traverse --depth` padrão é 2
- `hybrid-search --min-weight` padrão é 0.3 quando `--with-graph` está ativo


## Como Bootstrapar O Banco De Memória Em 60 Segundos
### Problem
- Seu laptop novo não tem banco de memória e seu agente perde contexto o tempo todo
- Cada onboarding queima 30 minutos com scripts frágeis e caça ao README


### Solution
```bash
cargo install --path .
sqlite-graphrag init --namespace global
sqlite-graphrag health --json
```


### Explanation
- Comando `init` cria o arquivo SQLite e baixa `multilingual-e5-small` localmente
- Flag `--namespace global` fixa o escopo inicial para seus agentes concordarem no alvo
- Comando `health` valida a integridade com `PRAGMA integrity_check` devolvendo JSON
- Exit code `0` sinaliza que o banco está pronto para leitura e escrita por qualquer agente
- Poupa 30 minutos por laptop contra bootstrap Pinecone mais Docker mais Python


### Variants
- Defina `SQLITE_GRAPHRAG_DB_PATH=/data/team.sqlite` para compartilhar arquivo entre pods dev
- Rode `sqlite-graphrag migrate --json` após bump de versão para aplicar upgrade de schema


### See Also
- Receita "Como Integrar sqlite-graphrag Com Loop Subprocess Do Claude Code"
- Receita "Como Agendar Purge E Vacuum Em Cron Ou GitHub Actions"


## Como Iniciar E Monitorar O Daemon Para Menor Latência
### Problem
- Cada chamada de `recall` e `remember` paga 1 segundo de cold start para carregar o modelo ONNX de embeddings
- Sua sessão interativa de agente fica lenta porque o modelo carrega e descarrega a cada invocação


### Solution
```bash
sqlite-graphrag daemon
sqlite-graphrag daemon --ping --json
# Ao final da sessão:
sqlite-graphrag daemon --stop
```


### Explanation
- O daemon mantém o modelo de embeddings residente em memória com auto-shutdown após 600 segundos ocioso
- Comandos `init`, `remember`, `ingest`, `recall` e `hybrid-search` reutilizam o daemon automaticamente
- `--ping` retorna JSON de health check incluindo contador de requisições de embedding desde o startup
- `--stop` solicita shutdown gracioso; o daemon encerra após processar embeddings em andamento
- Trate o daemon como opcional para invocações únicas; é uma otimização de performance, não um requisito


### Variants
- Ajuste timeout ocioso via `--idle-shutdown-secs 1800` para sessões longas de codificação com intervalos
- Desabilite auto-spawn em CI com `SQLITE_GRAPHRAG_DAEMON_DISABLE_AUTOSTART=1` para evitar processos em background


### See Also
- Receita "Como Bootstrapar O Banco De Memória Em 60 Segundos"
- Receita "Como Fazer Benchmark De hybrid-search Contra recall Vetorial Puro"


## Como Importar Em Massa Um Diretório De Base De Conhecimento
### Problem
- Seus 2000 arquivos Markdown ficam parados porque nenhum loader fala o schema sqlite-graphrag
- Entrada manual queima uma tarde inteira para cada cem arquivos de onboarding simples


### Solution
```bash
sqlite-graphrag ingest ./docs --recursive --pattern "*.md" --json \
  | jaq -c 'select(.status == "indexed") | .name'
```


### Explanation
- `ingest` substitui o loop `fd | xargs remember` por um único comando atômico com recursão e nomeação
- `--recursive` desce em subdiretórios; sem ele apenas o nível raiz é processado
- `--pattern "*.md"` filtra por extensão; padrão é `*.md` então a flag é mostrada para clareza
- Saída é NDJSON: uma linha JSON por arquivo com campo `status`, mais uma linha final de resumo com `summary: true`
- Nomes derivam dos basenames dos arquivos em kebab-case; nomes com mais de 60 caracteres são truncados com `truncated: true` no NDJSON
- Poupa 4 horas por mil arquivos contra scripts de importação artesanais ou loops `fd | xargs`


### Variants
- GLiNER NER desabilitado por padrão; use `--enable-ner` ou `SQLITE_GRAPHRAG_ENABLE_NER=1` para ativar extração automática de entidades
- `--skip-extraction` está obsoleto desde v1.0.45 e não tem efeito; NER está desabilitado por padrão, use `--enable-ner` para ativar
- Campo de resposta `extraction_method` informa o método utilizado: `gliner-<variant>+regex` (GLiNER bem-sucedido), `regex-only` (GLiNER indisponível ou desabilitado), ou `none:extraction-failed` (GLiNER tentado mas com erro)
- Arquivos duplicados retornam `status: "skipped"` com `action: "duplicate"` em vez de `status: "failed"`
- Use `--fail-fast` para abortar no primeiro erro por arquivo em vez de continuar com report inline


### See Also
- Receita "Como Importar Corpora Grandes Em Hosts Com Memória Limitada"
- Receita "Como Exportar Memórias Para NDJSON Para Backup"


## Como Importar Um Diretório Tipado Com Progresso Em Streaming
### Problem
- Seu pipeline CI ingere 2000 documentos de decisão mas não tem visibilidade de progresso durante a execução
- A abordagem de resumo final esconde falhas por arquivo até o lote inteiro completar


### Solution
```bash
sqlite-graphrag ingest ./decisions --type decision --recursive --json \
  | while IFS= read -r line; do
      status=$(echo "$line" | jaq -r '.status // empty')
      if [ "$status" = "failed" ]; then
        echo "FAIL: $(echo "$line" | jaq -r '.file')" >&2
      elif [ "$status" = "skipped" ]; then
        echo "SKIP: $(echo "$line" | jaq -r '.file') (duplicate or invalid name)"
      fi
    done
```


### Explanation
- `--type decision` marca cada arquivo ingerido como memória do tipo `decision`; tipo padrão é `document`
- Saída NDJSON transmite uma linha por arquivo seguida de uma linha resumo com `summary: true`
- O loop `while read` processa cada linha ao chegar em vez de esperar o lote completo
- Filtre por `select(.status)` para ignorar a linha resumo que não tem campo `status`
- Valores válidos de `--type`: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- Invoque `ingest` separadamente por tipo quando um diretório contém conteúdo misto


### Variants
- Agregue estatísticas finais: `| jaq -sc '[.[] | select(.status)] | group_by(.status) | map({status: .[0].status, count: length})'`
- Use `--pattern "memo-*"` para filtrar por prefixo de basename em vez de extensão


### See Also
- Receita "Como Importar Em Massa Um Diretório De Base De Conhecimento"
- Receita "Como Exportar Memórias Para NDJSON Para Backup"


## Como Combinar Busca Vetorial E FTS Com Pesos Ajustáveis
### Problem
- Recall vetorial puro perde matches exatos de token tipo `TODO-1234` em comentários de código
- FTS puro perde paráfrases que seus usuários digitaram em sinônimos e abreviações


### Solution
```bash
sqlite-graphrag hybrid-search "postgres migration deadlock" \
  --k 10 --rrf-k 60 --weight-vec 1.0 --weight-fts 1.0 --json
```


### Explanation
- `--rrf-k 60` é a constante de suavização Reciprocal Rank Fusion recomendada na literatura
- `--weight-vec 1.0` e `--weight-fts 1.0` são os padrões — ambas as fontes têm peso igual
- Ajuste os pesos apenas para tradeoffs explícitos entre semântica e precisão de tokens
- JSON emite `vec_rank` e `fts_rank` por resultado para agentes downstream auditarem a fusão
- Poupa 50 por cento dos tokens contra pedir a um LLM para re-rankear após vetor puro


### Variants
- Defina `--weight-vec 1.0 --weight-fts 0.0` para reproduzir um baseline `recall` puro em A/B
- Eleve `--k` para 50 antes de um re-ranker agent podar até os 5 hits finais
- Passe `--with-graph --max-hops 2` para semear travessia de grafo dos top resultados RRF; leia ambos `results[]` e `graph_matches[]` na saída (desde v1.0.44)


### See Also
- Receita "Como Debugar Queries Lentas Com Health E Stats"
- Receita "Como Expandir Hybrid Search Com Contexto De Grafo"


## Como Expandir Hybrid Search Com Contexto De Grafo
### Problem
- Seu hybrid search encontra as memórias seed certas mas perde conceitos relacionados conectados via grafo de entidades
- Rodar um comando `related` separado após cada hybrid search adiciona complexidade e latência ao pipeline


### Solution
```bash
sqlite-graphrag hybrid-search "authentication architecture" \
  --k 10 --with-graph --max-hops 2 --min-weight 0.3 --json \
  | jaq -r '(.results[], .graph_matches[]) | .name' | sort -u
```


### Explanation
- `--with-graph` habilita travessia de grafo de entidades semeada dos top resultados RRF (corrigido em v1.0.44)
- Matches de grafo aparecem em `graph_matches[]`, um array SEPARADO de `results[]`; leia AMBOS arrays
- `graph_matches[]` usa schema RecallItem: `name`, `distance`, `source` ("graph"), `graph_depth`
- `--min-weight 0.3` filtra arestas fracas do grafo para reduzir ruído de relações de baixa confiança
- `--max-hops 2` controla profundidade de travessia; aumente apenas após checar densidade via `graph stats`
- Elimina a necessidade de chamada separada de `related`, reduzindo etapas do pipeline de três para duas


### Variants
- Defina `--min-weight 0.0` para incluir todas as arestas independente do peso para máximo recall com mais ruído
- Extraia nomes de ambos arrays: `jaq -r '(.results[], .graph_matches[]) | .name' | sort -u > seeds.txt`


### See Also
- Receita "Como Combinar Busca Vetorial E FTS Com Pesos Ajustáveis"
- Receita "Como Explorar O Grafo De Entidades Com Stats, Entities E Traverse"


## Como Percorrer O Grafo De Entidades Para Recall Multi-Hop
### Problem
- Sua query acerta uma memória mas perde notas conectadas que compartilham o mesmo grafo
- RAG vetorial puro pontua tokens similares e ignora relações tipadas que importam


### Solution
```bash
sqlite-graphrag related authentication-flow --hops 2 --json
```


### Explanation
- `related` percorre relacionamentos tipados do grafo entre entidades com contagem controlada
- `--hops 2` inclui memórias amigas-de-amigos conectadas via entidades compartilhadas
- Saída JSON reporta o caminho da travessia para o LLM raciocinar sobre cadeias de relação
- Argumento posicional de nome suportado desde v1.0.44: `related <nome>` é equivalente a `related --name <nome>`
- Poupa custo de re-embedding porque a expansão roda como grafo SQLite e não KNN
- Revela contexto que o RAG vetorial puro ignora com 80 por cento menos tokens


### Variants
- Use `graph --json` para dump completo quando um auditor humano quiser análise offline
- Encadeie `related` em `hybrid-search` filtrando candidatos ao conjunto percorrido


### See Also
- Receita "Como Combinar Busca Vetorial E FTS Com Pesos Ajustáveis"
- Receita "Como Orquestrar Recall Paralelo Entre Namespaces"


## Como Encadear Recuperação Profunda Em 3 Camadas
### Problema
- Seu agente dispara um único recall e perde tanto o body completo quanto os vizinhos transitivos do grafo
- Despejar todas as memórias em markdown queima 72x mais tokens de contexto do que uma cadeia de recuperação focada


### Solução
```bash
# Camada 1: hybrid-search encontra memórias seed via FTS5 + vetor RRF
SEED=$(sqlite-graphrag hybrid-search "arquitetura de autenticação" --k 3 --json \
  | jaq -r '.results[0].name')

# Camada 2: read expande o corpo completo do top seed
sqlite-graphrag read "$SEED" --json | jaq -r '.body'

# Camada 3: related descobre conhecimento transitivo via o grafo de entidades
sqlite-graphrag related "$SEED" --hops 2 --json \
  | jaq -r '.results[].name'
```


### Explicação
- Camada 1 (hybrid-search) encontra as memórias mais relevantes usando ranking combinado de texto e vetor
- Camada 2 (read) recupera o corpo completo do melhor resultado (hybrid-search retorna snippets truncados)
- Camada 3 (related) percorre o grafo de entidades para descobrir memórias conectadas invisíveis à busca vetorial
- Este padrão reduz tokens de contexto em até 72x versus dump de todas memórias em markdown
- Encadeie no prompt do LLM coletando o body da Camada 2 mais os nomes da Camada 3 para uma janela de contexto focada


### Variantes
- Troque `--k 3` por `--k 1` quando suas queries forem altamente específicas e você confiar no top hit
- Aumente `--hops` para 3 quando o grafo de entidades tiver conectividade esparsa entre tópicos


### Veja Também
- Receita "Como Combinar Busca Vetorial E FTS Com Pesos Ajustáveis"
- Receita "Como Percorrer O Grafo De Entidades Para Recall Multi-Hop"


## Como Linkar Entidades Com Auto-Criação
### Problem
- Criar arestas de grafo requer que entidades existam antes, forçando um workflow tedioso de duas etapas
- Seu script de automação falha com exit code 4 cada vez que tenta linkar entidades não pré-registradas


### Solution
```bash
sqlite-graphrag link \
  --from auth-service --to postgres-db \
  --relation depends-on --weight 0.8 \
  --create-missing --entity-type tool
```


### Explanation
- `--create-missing` auto-cria entidades inexistentes com tipo padrão `concept` (desde v1.0.44)
- `--entity-type tool` sobrescreve o tipo padrão para todas entidades auto-criadas nesta invocação
- JSON response inclui `created_entities: ["auth-service", "postgres-db"]` quando entidades foram criadas
- `--weight` é opcional com padrão 0.5; valores devem estar no intervalo `[0.0, 1.0]`
- Vocabulário canônico de relações: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- Tipos válidos de entidade: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`


### Variants
- Omita `--create-missing` quando entidades devem pré-existir; exit code 4 sinaliza entidade ausente
- Aceite `--source`/`--target` como aliases de `--from`/`--to` para scripts que usam terminologia source/target


### See Also
- Receita "Como Remover Uma Aresta Do Grafo Com Unlink"
- Receita "Como Explorar O Grafo De Entidades Com Stats, Entities E Traverse"


## Como Remover Uma Aresta Do Grafo Com Unlink
### Problem
- Uma aresta `depends-on` incorreta entre duas entidades polui travessias de grafo com caminhos irrelevantes
- A única opção de remoção que sua equipe conhece é deletar a memória inteira, destruindo corpo e histórico


### Solution
```bash
sqlite-graphrag unlink --from auth-service --to legacy-db --relation depends-on
```


### Explanation
- Os três argumentos `--from`, `--to` e `--relation` são obrigatórios sem exceção
- `--source`/`--target` são aceitos como aliases de `--from`/`--to` para consistência com `link`
- A operação remove apenas a aresta de relacionamento; entidades e memórias permanecem intactas
- Exit code 4 sinaliza que a aresta especificada não existe no namespace atual
- Execute `cleanup-orphans` depois se as entidades desvinculadas não tiverem conexões restantes


### Variants
- Encadeie `graph entities --json | jaq '.entities[].name'` para descobrir nomes de entidades antes de desvincular
- Use `graph stats` antes e depois para verificar que a contagem de arestas diminuiu como esperado


### See Also
- Receita "Como Linkar Entidades Com Auto-Criação"
- Receita "Como Limpar Entidades Órfãs Após Deleção Em Massa"


## Como Limpar Entidades Órfãs Após Deleção Em Massa
### Problem
- Após esquecer 500 memórias, o grafo de entidades ainda contém centenas de nós órfãos sem arestas
- Travessia de grafo desperdiça ciclos visitando entidades sem saída que não referenciam nada


### Solution
```bash
sqlite-graphrag cleanup-orphans --dry-run --json
sqlite-graphrag cleanup-orphans --yes --json
```


### Explanation
- `--dry-run` audita contagem de órfãos sem modificar o banco; sempre execute isso primeiro
- `--yes` ignora o prompt de confirmação interativo para uso em pipelines automatizados
- Remove entidades que têm zero memórias vinculadas E zero arestas no grafo
- Agende periodicamente após operações em massa de `forget` ou `unlink`
- Não toca em memórias ou histórico de versões; apenas entidades de grafo são afetadas


### Variants
- Encadeie com `purge --retention-days 30 --yes` e `vacuum` em um cron semanal para higiene completa
- Inspecione candidatos primeiro com `graph entities --json | jaq '.entities[] | select(.degree == 0)'` se disponível


### See Also
- Receita "Como Agendar Purge E Vacuum Em Cron Ou GitHub Actions"
- Receita "Como Remover Uma Aresta Do Grafo Com Unlink"


## Como Explorar O Grafo De Entidades Com Stats, Entities E Traverse
### Problem
- Seu grafo cresceu para milhares de entidades e você não tem visibilidade sobre sua densidade ou conectividade
- Planejar profundidade de travessia sem conhecer `avg_degree` desperdiça tempo em subgrafos vazios ou fan-outs sobrecarregados


### Solution
```bash
sqlite-graphrag graph stats --json | jaq '{node_count, edge_count, avg_degree}'
sqlite-graphrag graph entities --entity-type person --json | jaq '.entities[].name'
sqlite-graphrag graph traverse --from acme-corp --depth 3 --json
sqlite-graphrag graph --format mermaid --output graph.md
```


### Explanation
- `graph stats` reporta `node_count`, `edge_count`, `avg_degree` e `max_degree` para informar planejamento de travessia
- `graph entities` lista todas entidades; campo é `.entities[]` NÃO `.items[]` desde v1.0.44
- `graph traverse` parte de uma entidade tipada (não um nome de memória) e caminha até `--depth` hops
- Hops retornam `entity`, `relation`, `direction`, `weight` e `depth` por aresta visitada
- Formatos de exportação incluem `json`, `dot` (Graphviz) e `mermaid`; grave em arquivo via `--output <PATH>`
- Exit code 4 de `graph traverse` sinaliza entidade raiz inexistente


### Variants
- Filtre entidades por tipo: `--entity-type tool` mostra apenas nós do tipo tool
- Pagine listas grandes de entidades: `--limit 100 --offset 200` para datasets com milhares de entidades


### See Also
- Receita "Como Expandir Hybrid Search Com Contexto De Grafo"
- Receita "Como Debugar Queries Lentas Com Health E Stats"


## Como Integrar sqlite-graphrag Com Loop Subprocess Do Claude Code
### Problem
- Claude Code reinicia a cada sessão e esquece decisões feitas cinco minutos atrás
- Seu orquestrador não tem memória determinística entre iterações do agente


### Solution
```bash
# .claude/hooks/pre-task.sh
CONTEXT=$(sqlite-graphrag recall "$USER_PROMPT" --k 5 --json)
printf 'Relevant memories:\n%s\n' "$CONTEXT"

# .claude/hooks/post-task.sh
sqlite-graphrag remember \
  --name "session-$(date +%s)" \
  --type project \
  --description "decision log" \
  --body "$ASSISTANT_RESPONSE"
```


### Explanation
- Hook pre-task injeta memórias relevantes no prompt do agente antes de gerar resposta
- Hook post-task persiste a saída do agente no vector store para sessões futuras
- Scripts de hook rodam como subprocess respeitando exit codes e limites de slots
- Exit code `13` ou `75` dispara retry dentro do hook sem matar o agente
- Poupa 40 por cento dos tokens de contexto e mantém decisões entre restarts do Claude Code


### Variants
- Troque `recall` por `hybrid-search` quando seus prompts misturam palavras e conceitos
- Adicione `--namespace $CLAUDE_PROJECT` para isolar memória por projeto em hosts multi-repo


### See Also
- Receita "Como Integrar Com Codex CLI Via AGENTS.md"
- Receita "Como Configurar Painel Assistente Windsurf Ou Zed Com sqlite-graphrag"


## Como Integrar Com Codex CLI Via AGENTS.md
### Problem
- Codex lê `AGENTS.md` mas pula qualquer capacidade sem sintaxe exata de invocação listada
- Sua equipe de ops perde 10 minutos por sessão ensinando Codex o mesmo CLI de memória


### Solution
```md
<!-- AGENTS.md na raiz do repo -->
## Memory Layer
- Use `sqlite-graphrag recall "<query>" --k 5 --json` to fetch prior decisions
- Use `sqlite-graphrag remember --name "<kebab-name>" --type project --description "<sumário>" --body "<text>"` to persist output
- Prefer `hybrid-search` when the query mixes keywords and natural language
- Respect exit code 75 as retry-later rather than error
```


### Explanation
- AGENTS.md expõe o contrato CLI como parte do contexto do sistema Codex automaticamente
- Codex invoca comandos subprocess listados em AGENTS.md sem prompt adicional do operador
- Exit codes determinísticos permitem Codex reintentar em `75` sem intervenção humana
- Saída JSON integra com camada de parsing do Codex sem regex ou plugin customizado
- Poupa 10 minutos por sessão e sobrevive a upgrades do Codex sem quebrar o contrato


### Variants
- Adicione `SQLITE_GRAPHRAG_NAMESPACE=$REPO_NAME` no `.envrc` para Codex isolar memória por projeto
- Inclua um one-liner de exemplo sob cada comando para ancorar Codex em uso real


### See Also
- Receita "Como Integrar sqlite-graphrag Com Loop Subprocess Do Claude Code"
- Receita "Como Integrar Com Terminal Do Cursor Para Memória No Editor"


## Como Integrar Com Terminal Do Cursor Para Memória No Editor
### Problem
- Cursor perde contexto toda vez que você fecha o editor ou troca de branch localmente
- Sua sessão LLM pareada reinicia fria e repete as mesmas perguntas toda manhã


### Solution
```jsonc
// Snippet do settings.json do Cursor
{
  "terminal.integrated.env.osx": { "SQLITE_GRAPHRAG_NAMESPACE": "${workspaceFolderBasename}" },
  "cursor.ai.rules": "Before answering, run `sqlite-graphrag recall \"${selection}\" --k 5 --json` and use hits as context"
}
```


### Explanation
- Env var por workspace isola memória pelo nome da pasta do projeto sem config manual
- Regras AI do Cursor instruem o modelo embutido a chamar a CLI antes de responder prompts
- A CLI lê apenas o código selecionado então a latência fica abaixo de 50 ms em queries pequenas
- Exit code `0` com hits vazios mantém Cursor calado em vez de alucinar contexto
- Poupa 15 minutos por dia re-perguntando as mesmas coisas em sessões do Cursor


### Variants
- Troque `recall` por `hybrid-search` quando o código mistura docstring inglês e comentários português
- Adicione um hook `post-save` que chama `remember` com o diff como body para memória da sessão


### See Also
- Receita "Como Configurar Painel Assistente Windsurf Ou Zed Com sqlite-graphrag"
- Receita "Como Integrar Com Codex CLI Via AGENTS.md"


## Como Configurar Painel Assistente Windsurf Ou Zed Com sqlite-graphrag
### Problem
- Painéis assistentes do Windsurf e Zed saem sem backend de memória plugável por padrão
- Seu fluxo multi-IDE fragmenta memória entre silos Cursor Windsurf e Zed


### Solution
```bash
# Comando de terminal compartilhado que ambos IDEs podem rodar
sqlite-graphrag hybrid-search "$EDITOR_CONTEXT" --k 10 --json > /tmp/ng.json
```


### Explanation
- Windsurf e Zed chamam tarefas de terminal direto do painel assistente nativamente
- `/tmp/ng.json` atua como lingua franca consumida por ambos painéis para prompts
- Binário CLI único substitui três plugins dedicados evitando manutenção por IDE
- Exit code `0` com hits vazios é benigno então o painel degrada graciosamente
- Poupa horas por semana unificando memória entre editores sem rebuild de plugin


### Variants
- Mapeie o comando para um atalho tipo `Cmd+Shift+M` para invocação de recall com uma tecla
- Canalize a saída por `jaq` para transformar o payload no schema exato que cada IDE prefere


### See Also
- Receita "Como Integrar Com Terminal Do Cursor Para Memória No Editor"
- Receita "Como Orquestrar Recall Paralelo Entre Namespaces"


## Como Prevenir Corrupção Por Dropbox Ou iCloud Com sync-safe-copy
### Problem
- Seu arquivo SQLite mora no Dropbox e sincroniza no meio de uma escrita corrompendo o WAL
- Snapshots `cp` clássicos durante escrita produzem arquivos inválidos que não abrem depois


### Solution
```bash
sqlite-graphrag sync-safe-copy --dest ~/Dropbox/sqlite-graphrag/snapshot.sqlite
```


### Explanation
- O comando força um checkpoint WAL antes da cópia então o snapshot fica transacionalmente consistente
- Arquivo de saída recebe `chmod 600` em Unix para outros usuários não lerem memórias sensíveis
- Cópia roda atômica via `SQLite Online Backup API` eliminando risco de escrita parcial
- Exit code `0` garante que o snapshot abre limpo em qualquer máquina com o mesmo binário
- Poupa fins de semana de recovery quando o Dropbox corromperia o arquivo vivo


### Variants
- Agende de hora em hora via `launchd` no macOS ou `systemd --user` no Linux para backup contínuo
- Comprima com `ouch compress snapshot.sqlite snapshot.tar.zst` para upload cloud mais rápido


### See Also
- Receita "Como Agendar Purge E Vacuum Em Cron Ou GitHub Actions"
- Receita "Como Versionar O Banco SQLite Com Git LFS"


## Como Agendar Purge E Vacuum Em Cron Ou GitHub Actions
### Problem
- Memórias soft-deletadas empilham e incham o uso de disco após meses de uso pesado por agentes
- Seu arquivo SQLite estoura 10 GB porque `VACUUM` nunca roda na automação


### Solution
```yaml
# .github/workflows/ng-maintenance.yml
name: sqlite-graphrag maintenance
on:
  schedule: [{ cron: "0 3 * * 0" }]
jobs:
  maintenance:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo install --path .
      - run: sqlite-graphrag purge --retention-days 30 --yes
      - run: sqlite-graphrag vacuum --json
      - run: sqlite-graphrag optimize --json
```


### Explanation
- `purge --retention-days 30` apaga definitivamente linhas soft-deletadas mais antigas que a janela
- `vacuum` reclama páginas da freelist e faz checkpoint do WAL para o arquivo principal
- `optimize` refresca estatísticas do planner para recall mais rápido na próxima execução
- Cron semanal às 03:00 de domingo evita contenção com horário comercial de agentes
- Poupa 70 por cento do disco ao longo de 6 meses contra deploy sem manutenção


### Variants
- Rode `cron 0 3 * * *` todas as noites quando seu time escreve milhares de memórias por dia
- Substitua GitHub Actions por `systemd.timer` para ambientes air-gapped sem internet


### See Also
- Receita "Como Prevenir Corrupção Por Dropbox Ou iCloud Com sync-safe-copy"
- Receita "Como Debugar Queries Lentas Com Health E Stats"


## Como Exportar Memórias Para NDJSON Para Backup
### Problem
- Backups SQLite são opacos e exigem o binário instalado para qualquer auditoria de restore
- Compliance pede exports em texto puro para diff entre snapshots mensais


### Solution
```bash
sqlite-graphrag list --limit 10000 --json \
  | jaq -c '.items[]' > memories-$(date +%Y%m%d).ndjson
```


### Explanation
- `list --limit 10000` enumera memórias até o teto com ordenação determinística estável
- `jaq -c '.items[]'` achata o array `items` em NDJSON legível por qualquer ferramenta instantaneamente
- Arquivo resultante abre em `rg` `bat` ou planilhas sem conhecimento de SQLite algum
- Diff dois snapshots com `difft` para auditar o que mudou entre backups mensais limpo
- Poupa tempo do auditor porque NDJSON é legível por humano ao contrário de binário opaco


### Variants
- Canalize por `ouch compress` para um arquivo `zst` antes de upload em buckets S3 ou GCS
- Loop em shell para paginar por namespaces se a instância hospeda memória multi-tenant


### See Also
- Receita "Como Versionar O Banco SQLite Com Git LFS"
- Receita "Como Agendar Purge E Vacuum Em Cron Ou GitHub Actions"


## Como Versionar O Banco SQLite Com Git LFS
### Problem
- Seu arquivo SQLite de 500 MB quebra limites de push do GitHub e incha todos os clones
- Rebases de branch corrompem blobs binários quando o Git tenta merge com lógica textual


### Solution
```bash
git lfs install
git lfs track "*.sqlite"
echo "*.sqlite filter=lfs diff=lfs merge=lfs -text" >> .gitattributes
git add .gitattributes graphrag.sqlite
git commit -m "chore: track sqlite-graphrag db via LFS"
```


### Explanation
- Git LFS guarda arquivos SQLite em cache remoto então o repo Git fica abaixo de 100 MB
- Atributo `-text` impede o Git de tentar merge baseado em linha em conteúdo binário
- `sync-safe-copy` antes do commit garante que o arquivo está transacionalmente consistente
- Colegas clonam com `git lfs pull` baixando o DB só quando precisam de fato
- Poupa 90 por cento do tempo de clone para colegas que não precisam do banco local


### Variants
- Tag snapshots com `git tag db-2026-04-18` para fixar estado da memória em release
- Pule LFS e guarde saídas de sync-safe-copy em object storage com URL assinada


### See Also
- Receita "Como Exportar Memórias Para NDJSON Para Backup"
- Receita "Como Prevenir Corrupção Por Dropbox Ou iCloud Com sync-safe-copy"


## Como Orquestrar Recall Entre Namespaces Com Segurança
### Problem
- Seu agente multi-projeto precisa executar um recall por namespace no mesmo host
- Fan-out paralelo cego pode estourar RAM porque cada subprocesso de `recall` pode carregar o modelo ONNX de forma independente


### Solution
```bash
for ns in project-a project-b project-c project-d; do
  SQLITE_GRAPHRAG_NAMESPACE="$ns" \
    sqlite-graphrag --max-concurrency 1 recall "error rate" --k 5 --json
done
```


### Explanation
- O loop permanece serial de forma intencional porque `recall` é comando pesado de embedding
- `--max-concurrency 1` evita oversubscription local durante auditorias, CI e uso em desktop
- Env var `SQLITE_GRAPHRAG_NAMESPACE` escopa cada subprocesso ao seu próprio projeto limpo
- Um documento JSON por namespace ainda cai no stdout para um agregador downstream fundir ranks
- Esse padrão prioriza segurança do host e progresso determinístico em vez de redução agressiva de wall-clock


### Variants
- Reserve fan-out paralelo para comandos leves como `stats` ou `list`, não para `recall`
- Só aumente concorrência de comandos pesados depois de medir RSS, observar swap e confirmar que o host permanece estável


### See Also
- Receita "Como Combinar Busca Vetorial E FTS Com Pesos Ajustáveis"
- Receita "Como Fazer Benchmark De hybrid-search Contra recall Vetorial Puro"


## Como Tratar Exit Codes Em Pipelines Automatizados
### Problem
- Seu pipeline CI trata todo exit não-zero como fatal, matando operações retriáveis como exit 75 (slots esgotados)
- Debugar falhas de pipeline leva 30 minutos porque seu wrapper não distingue validação de conflitos de locking


### Solution
```bash
sqlite-graphrag remember --name "$NAME" --type project \
  --description "$DESC" --body-stdin < "$FILE"
rc=$?
case $rc in
  0)  echo "Success" ;;
  2)  echo "Duplicate: use --force-merge" ;;
  3)  echo "Conflict: re-read and retry" ;;
  6)  echo "Payload too large: split body" ;;
  15) echo "Busy: widen --wait-lock" ;;
  75) echo "Slots full: wait, do NOT raise concurrency" ;;
  77) echo "RAM pressure: free memory first" ;;
  *)  echo "Fatal: rc=$rc" >&2; exit 1 ;;
esac
```


### Explanation
- 16 exit codes de 0 a 77 seguindo convenções sysexits.h para roteamento de erros parseável por máquina
- Exit 3 significa conflito de locking otimista: recarregue a memória com `read --json` e tente novamente
- Exit 13 significa falha parcial em lote: reprocesse apenas os itens falhos, NÃO o lote inteiro
- Exit 75 e 77 sinalizam pressão de recursos: NUNCA aumente concorrência após receber esses códigos
- Exit 15 significa banco ocupado: amplie `--wait-lock <ms>` para esperar mais antes de falhar
- Tabela completa de códigos: 0=sucesso, 1=validação, 2=duplicata, 3=conflito, 4=não-encontrado, 5=namespace, 6=payload, 10=database, 11=embedding, 12=sqlite-vec, 13=parcial, 14=I/O, 15=ocupado, 20=interno, 75=slots, 77=RAM


### Variants
- Envolva o case em um loop de retry com backoff exponencial para códigos 3, 15, 75 e 77
- Logue `stderr` separadamente: `2>error.log` captura mensagens legíveis enquanto `stdout` captura JSON


### See Also
- Receita "Como Orquestrar Recall Entre Namespaces Com Segurança"
- Receita "Como Editar Uma Memória Com Locking Otimista"


## Como Debugar Queries Lentas Com Health E Stats
### Problem
- Seu recall que retornava em 8 ms agora leva 400 ms depois de meses de escrita
- Você não enxerga qual tabela inchou ou qual índice ficou stale ao longo do tempo


### Solution
```bash
sqlite-graphrag health --json | jaq '{integrity, wal_size_mb, journal_mode}'
sqlite-graphrag stats --json | jaq '{memories, entities, edges, avg_body_len}'
SQLITE_GRAPHRAG_LOG_LEVEL=debug sqlite-graphrag recall "slow query" --k 5 --json
sqlite-graphrag optimize --json
sqlite-graphrag __debug_schema --json | jaq '{schema_version, objects: (.objects | length)}'
```


### Explanation
- `health` reporta `integrity`, tamanho WAL e `journal_mode` para detectar fragmentação rápido
- `stats` conta linhas revelando qual tabela cresceu desproporcionalmente desde a última auditoria
- `SQLITE_GRAPHRAG_LOG_LEVEL=debug` emite tempos por estágio SQLite em stderr para tracing
- Comparar `avg_body_len` atual ao baseline mostra se os bodies cresceram além dos defaults
- `optimize` atualiza estatísticas do query planner para que o próximo recall ou hybrid-search use índices atualizados
- `__debug_schema` é um comando oculto que despeja versão do schema, contagem de objetos e histórico de migrações para troubleshooting de drift
- Poupa horas de tuning às cegas expondo o caminho lento exato em três comandos


### Variants
- Agende um painel que raspa `stats --json` toda hora e alerta em picos de crescimento
- Rode `optimize` seguido de `vacuum` quando o WAL passa de 100 MB para reclamar performance


### See Also
- Receita "Como Agendar Purge E Vacuum Em Cron Ou GitHub Actions"
- Receita "Como Fazer Benchmark De hybrid-search Contra recall Vetorial Puro"


## Como Gerenciar O Cache Do Modelo De Embeddings
### Problem
- Seu ambiente CI fica sem espaço em disco porque modelos ONNX em cache acumulam entre upgrades do binário
- Você não consegue diagnosticar por que o primeiro recall leva 30 segundos sem saber quais modelos estão em cache


### Solution
```bash
sqlite-graphrag cache list --json
sqlite-graphrag cache clear-models --yes
```


### Explanation
- `cache list` mostra modelos em cache com tamanho em bytes e uso total de disco para planejamento de capacidade
- `clear-models` força re-download do modelo de embeddings na próxima operação de embedding
- Útil após upgrades do binário quando o formato do modelo pode ter mudado entre versões
- `--yes` ignora o prompt de confirmação interativo para uso em scripts de limpeza automatizada
- Limpar o cache não afeta embeddings existentes armazenados no banco; apenas operações futuras fazem re-download


### Variants
- Agende `cache clear-models --yes` após cada `cargo install` upgrade em CI para evitar artefatos de modelo obsoletos
- Combine com `health --json | jaq '.model_ok'` para verificar integridade do modelo antes de limpar


### See Also
- Receita "Como Debugar Queries Lentas Com Health E Stats"
- Receita "Como Agendar Purge E Vacuum Em Cron Ou GitHub Actions"


## Como Fazer Benchmark De hybrid-search Contra recall Vetorial Puro
### Problem
- Você não tem dados para justificar habilitar hybrid search em produção contra vetor puro
- Seus stakeholders querem evidência numérica antes de aprovar o overhead de índice


### Solution
```bash
hyperfine --warmup 3 \
  'sqlite-graphrag recall "postgres migration" --k 10 --json > /dev/null' \
  'sqlite-graphrag hybrid-search "postgres migration" --k 10 --json > /dev/null'
```


### Explanation
- `hyperfine` mede ambos comandos com runs de warmup removendo ruído de cache frio
- Saída reporta latência média desvio padrão e speedup relativo em uma tabela limpa
- Resultados permitem comparar qualidade de recall contra latência em workload real
- Evidência numérica empodera conversas de tradeoff com stakeholders de produto e finanças
- Poupa semanas de debate ancorando a decisão em dados em vez de intuição


### Variants
- Troque a query única por 100 queries amostradas para computar p50 p95 p99 de latência
- Integre `hyperfine --export-json` em CI para detectar regressões entre pull requests


### See Also
- Receita "Como Combinar Busca Vetorial E FTS Com Pesos Ajustáveis"
- Receita "Como Orquestrar Recall Paralelo Entre Namespaces"


## Como Integrar Com rig-core Para Memória De Agente
### Problem
- Seu agente `rig-core` perde contexto entre invocações sem armazenamento persistente
- Reconstruir embeddings a cada execução desperdiça 50 minutos de compute e budget de API por semana

### Solution
```rust
use std::process::Command;
use serde_json::Value;

fn lembrar_contexto_agente(namespace: &str, conteudo: &str) -> anyhow::Result<()> {
    let name = format!(
        "rig-context-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", namespace,
            "--name", &name,
            "--type", "project",
            "--description", "contexto do agente rig-core",
            "--body", conteudo,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "sqlite-graphrag remember falhou");
    Ok(())
}

fn recuperar_contexto_agente(namespace: &str, consulta: &str, k: u8) -> anyhow::Result<Vec<String>> {
    let output = Command::new("sqlite-graphrag")
        .args(["recall", "--namespace", namespace, "--k", &k.to_string(), "--json", consulta])
        .output()?;
    anyhow::ensure!(output.status.success(), "sqlite-graphrag recall falhou");
    let parsed: Value = serde_json::from_slice(&output.stdout)?;
    let itens = parsed["results"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v["snippet"].as_str().map(str::to_owned))
        .collect();
    Ok(itens)
}
```

### Explanation
- `Command::new("sqlite-graphrag")` executa o binário de 25 MB sem custo de FFI
- `--namespace` isola a memória do agente rig prevenindo contaminação entre agentes
- `--json` retorna saída estruturada que `serde_json` parseia sem regex frágil
- `anyhow::ensure!` converte falhas de exit-code em erros tipados que o agente trata
- Reduz 50 minutos de reconstrução de contexto por execução para uma chamada CLI de 5 milissegundos

### Variants
- Substitua `Command` por `tokio::process::Command` para pipelines async sem bloqueio
- Envolva as duas funções em um struct `RigMemoryAdapter` que implementa um trait `MemoryStore`

### See Also
- Receita "Como Inicializar Banco De Dados De Memória Em 60 Segundos"
- Receita "Como Executar Ollama Offline Com ollama-rs E Memória Persistente"


## Como Integrar Com swarms-rs Para Memória Multi-Agente
### Problem
- Seu swarm de agentes sobrescreve memórias uns dos outros ao compartilhar um namespace
- Depurar qual agente escreveu o quê leva horas de grep em arquivos de log não estruturados

### Solution
```rust
use std::process::Command;

fn swarm_lembrar(agent_id: &str, conteudo: &str) -> anyhow::Result<()> {
    let namespace = format!("swarm-{agent_id}");
    let name = format!(
        "swarm-note-{agent_id}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", &namespace,
            "--name", &name,
            "--type", "project",
            "--description", "nota do agente swarm",
            "--body", conteudo,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "swarm remember falhou para agent {agent_id}");
    Ok(())
}

fn swarm_recuperar_todos(agent_ids: &[&str], consulta: &str) -> anyhow::Result<Vec<(String, String)>> {
    let mut resultados = Vec::new();
    for agent_id in agent_ids {
        let namespace = format!("swarm-{agent_id}");
        let output = Command::new("sqlite-graphrag")
            .args(["recall", "--namespace", &namespace, "--k", "5", "--json", consulta])
            .output()?;
        if output.status.success() {
            let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            if let Some(itens) = parsed["results"].as_array() {
                for item in itens {
                    if let Some(snippet) = item["snippet"].as_str() {
                        resultados.push((agent_id.to_string(), snippet.to_owned()));
                    }
                }
            }
        }
    }
    Ok(resultados)
}
```

### Explanation
- Namespace por agente `swarm-{agent_id}` isola memórias sem alterações de schema
- Um único arquivo SQLite hospeda todos os namespaces eliminando múltiplos bancos
- Iterar namespaces no coordenador coleta resultados ranqueados de cada membro do swarm
- Saída JSON estruturada com `serde_json` torna atribuição trivial versus logs de texto puro
- Reduz tempo de depuração multi-agente de horas para minutos tornando autoria explícita

### Variants
- Use `tokio::task::JoinSet` para recuperar todos os namespaces concorrentemente em swarms async
- Adicione um namespace `coordinator` onde o orquestrador grava decisões sintetizadas do swarm

### See Also
- Receita "Como Orquestrar Recall Paralelo Entre Namespaces"
- Receita "Como Integrar Com rig-core Para Memória De Agente"


## Como Usar genai Com sqlite-graphrag Para Memória Universal De LLM
### Problem
- Trocar provedores de LLM via `genai` reseta a memória do agente porque embeddings diferem por vendor
- Seu time perde 40 minutos por migração de provedor reconstruindo índices de busca semântica

### Solution
```rust
use std::process::Command;

async fn armazenar_turno_llm(
    namespace: &str,
    role: &str,
    conteudo: &str,
) -> anyhow::Result<()> {
    let entrada = format!("[{role}] {conteudo}");
    let name = format!(
        "llm-turn-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", namespace,
            "--name", &name,
            "--type", "project",
            "--description", "turno de conversa LLM",
            "--body", &entrada,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "falhou ao persistir turno LLM");
    Ok(())
}

async fn recuperar_contexto_relevante(
    namespace: &str,
    consulta_usuario: &str,
    k: u8,
) -> anyhow::Result<String> {
    let output = Command::new("sqlite-graphrag")
        .args([
            "hybrid-search",
            "--namespace", namespace,
            "--k", &k.to_string(),
            "--json",
            consulta_usuario,
        ])
        .output()?;
    anyhow::ensure!(output.status.success(), "hybrid-search falhou");
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let contexto = parsed["results"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v["body"].as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");
    Ok(contexto)
}
```

### Explanation
- sqlite-graphrag armazena embeddings usando `multilingual-e5-small` independente do provedor LLM
- Trocar de OpenAI para Mistral via `genai` não invalida entradas de memória existentes
- `hybrid-search` combina similaridade vetorial e FTS dando contexto mais rico que vetor puro
- Formatar turnos como `[role] conteudo` preserva estrutura de conversa no body da memória
- Elimina 40 minutos de reconstrução de índice por migração com uma camada agnóstica a provedor

### Variants
- Injete contexto recuperado como system message antes de cada request `genai::chat` automaticamente
- Armazene nome do modelo e temperatura junto ao body do turno para auditar qual modelo gerou cada resposta

### See Also
- Receita "Como Combinar Busca Vetorial E FTS Com Pesos Ajustáveis"
- Receita "Como Cascatear Com llm-cascade E Fallback De Memória"


## Como Cascatear Com llm-cascade E Fallback De Memória
### Problem
- Seu pipeline LLM em cascata perde tentativas anteriores quando um provedor falha e reexecuta
- Rederetear chamadas falhas sem contexto faz o modelo de fallback repetir erros custosos

### Solution
```rust
use std::process::Command;

fn persistir_tentativa_cascade(
    namespace: &str,
    provider: &str,
    prompt: &str,
    resultado: &str,
    sucesso: bool,
) -> anyhow::Result<()> {
    let rotulo = if sucesso { "SUCCESS" } else { "FAILURE" };
    let entrada = format!("[CASCADE:{rotulo}:{provider}] prompt={prompt} resultado={resultado}");
    let name = format!(
        "cascade-attempt-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", namespace,
            "--name", &name,
            "--type", "project",
            "--description", "log de tentativa llm-cascade",
            "--body", &entrada,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "falhou ao persistir tentativa cascade");
    Ok(())
}

fn carregar_historico_cascade(namespace: &str, prompt: &str) -> anyhow::Result<String> {
    let output = Command::new("sqlite-graphrag")
        .args([
            "recall",
            "--namespace", namespace,
            "--k", "10",
            "--json",
            prompt,
        ])
        .output()?;
    anyhow::ensure!(output.status.success(), "recall falhou para histórico cascade");
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let historico = parsed["results"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v["snippet"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(historico)
}
```

### Explanation
- Rotular entradas com `CASCADE:SUCCESS:provider` permite ao fallback pular provedores já falhos
- Recuperar histórico antes de cada tentativa revela quais modelos já tentaram o mesmo prompt
- Um namespace por execução de pipeline garante isolamento sem gerenciar múltiplos bancos
- Rótulos estruturados parseiam com `str::contains` simples evitando overhead JSON na consulta
- Economiza falhas repetidas custosas dando ao fallback consciência plena do estado cascade anterior

### Variants
- Crie um struct `CascadeMemory` que chama `persistir` e `carregar` automaticamente em cada tentativa
- Filtre entradas `FAILURE` na seleção de fallback para pular provedores comprovadamente falhos

### See Also
- Receita "Como Usar genai Com sqlite-graphrag Para Memória Universal De LLM"
- Receita "Como Integrar Com rig-core Para Memória De Agente"


## Como Executar Ollama Offline Com ollama-rs E Memória Persistente
### Problem
- Seu agente `ollama-rs` offline perde todo o contexto de conversa quando o processo reinicia
- Ambientes air-gapped não podem usar vector stores em nuvem então cada sessão começa do zero

### Solution
```rust
use std::process::Command;

fn lembrar_offline(conteudo: &str) -> anyhow::Result<()> {
    let name = format!(
        "ollama-turn-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", "ollama-local",
            "--name", &name,
            "--type", "project",
            "--description", "contexto offline do ollama",
            "--body", conteudo,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "lembrar offline falhou: exit code não zero");
    Ok(())
}

fn recuperar_offline(consulta: &str, k: u8) -> anyhow::Result<Vec<String>> {
    let output = Command::new("sqlite-graphrag")
        .args([
            "recall",
            "--namespace", "ollama-local",
            "--k", &k.to_string(),
            "--json",
            consulta,
        ])
        .output()?;
    anyhow::ensure!(output.status.success(), "recuperar offline falhou");
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let itens = parsed["results"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v["snippet"].as_str().map(str::to_owned))
        .collect();
    Ok(itens)
}

fn construir_prompt_com_contexto(consulta: &str, memorias: &[String]) -> String {
    let contexto = memorias.join("\n---\n");
    format!("Contexto relevante da memória:\n{contexto}\n\nConsulta do usuário: {consulta}")
}
```

### Explanation
- sqlite-graphrag embarca o modelo ONNX `multilingual-e5-small` então zero chamadas de rede ocorrem
- O binário de 25 MB grava em um arquivo SQLite local que sobrevive a reinicializações do processo
- `--namespace ollama-local` mantém memórias offline isoladas de namespaces de agentes em rede
- `construir_prompt_com_contexto` injeta memórias recuperadas no prompt Ollama antes de cada inferência
- Entrega memória vetorial persistente em ambientes totalmente air-gapped sem dependências de nuvem

### Variants
- Encadeie `recuperar_offline` com `sqlite-graphrag link` para construir grafo de conhecimento das saídas Ollama
- Chame `sqlite-graphrag vacuum` periodicamente para recuperar espaço SQLite conforme o banco offline cresce

### See Also
- Receita "Como Inicializar Banco De Dados De Memória Em 60 Segundos"
- Receita "Como Integrar Com rig-core Para Memória De Agente"


## Como Exibir Timestamps no Fuso Horário Local
### Problem
- Saída JSON de todos os subcomandos inclui campos `*_iso` em UTC por padrão
- Agentes rodando em região específica querem timestamps localizados para log e exibição
- Pipelines que leem `created_at_iso` precisam de strings com offset para ordenação correta

### Solution
```bash
# Flag pontual: exibir timestamps no fuso horário de São Paulo
sqlite-graphrag read --name minha-nota --tz America/Sao_Paulo

# Variável de ambiente persistente: todos os comandos da sessão usam o fuso configurado
export SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo
sqlite-graphrag list --json | jaq '.items[].updated_at_iso'

# Pipeline CI: forçar UTC explicitamente para evitar surpresas de fuso do sistema
SQLITE_GRAPHRAG_DISPLAY_TZ=UTC sqlite-graphrag recall "notas de deploy" --json

# Extrair apenas a parte do offset para verificar que o fuso foi aplicado
sqlite-graphrag read --name plano-deploy --tz Europe/Berlin --json \
  | jaq -r '.created_at_iso' \
  | rg '\+\d{2}:\d{2}$'
```

### Explanation
- Flag `--tz <IANA>` sobrescreve todas as configurações e aplica o fuso IANA informado
- Variável `SQLITE_GRAPHRAG_DISPLAY_TZ` mantém a configuração entre invocações sem a flag
- Ambos caem para UTC quando ausentes, garantindo saída determinística retrocompatível
- Apenas campos string terminando em `_iso` são afetados; campos inteiros permanecem epoch Unix
- Nomes IANA inválidos causam exit 2 com mensagem de erro `Validation` no stderr
- Formato produzido: `2026-04-19T04:00:00-03:00` (offset explícito, sem sufixo `Z`)

### Variants
- Use `America/New_York` para Eastern Time (UTC-5/UTC-4 dependendo do horário de verão)
- Use `Asia/Tokyo` para Japan Standard Time (UTC+9, sem horário de verão)
- Use `Europe/Berlin` para Central European Time (UTC+1/UTC+2 dependendo do horário de verão)
- Use `UTC` para resetar ao padrão explicitamente em ambientes com variável de ambiente conflitante
- Use `--lang pt` para forçar mensagens stderr legíveis em português; stdout JSON permanece independente de idioma

### See Also
- Receita "Como Inicializar Banco De Dados De Memória Em 60 Segundos"
- Receita "Como Configurar Saída de Idioma Com a Flag --lang"


## Como Fazer Round-Trip De Forget E Restore Em Uma Memória
### Problema
- Você rodou `forget --name decisao-importante` e agora `recall` não retorna nada
- Ler SQL de `memory_versions` para recuperar a linha não faz parte do seu trabalho
- v1.0.21 deixava `history` rejeitando memórias esquecidas e `restore` exigindo `--version`


### Solução
```bash
sqlite-graphrag forget --name decisao-importante
sqlite-graphrag history --name decisao-importante --json | jaq '.deleted'
sqlite-graphrag restore --name decisao-importante
sqlite-graphrag recall "decisão" --json
```


### Explicação
- `history` em v1.0.22 retorna versões de memórias soft-deletadas com flag `deleted: true`
- `restore` sem `--version` escolhe automaticamente a última versão não-`restore`
- Juntos tornam `forget` reversível ponta-a-ponta sem inspecionar SQL
- `vec_memories` é re-embeddado no restore para que recall vetorial volte a encontrar a memória
- Round-trip é idempotente: esquecer uma memória já esquecida é um no-op


### Variantes
- Passe `--version N` explicitamente quando precisar voltar a uma edição específica
- Combine com `list --include-deleted --json | jaq '.items[] | select(.deleted)'` para auditar todas as esquecidas
- Pipe `history --json` para detectar estado esquecido programaticamente antes de restaurar


### Veja Também
- Receita "Como Agendar Purge E Vacuum Em Cron Ou GitHub Actions"
- Receita "Como Exportar Memórias Para NDJSON Para Backup"


## Como Editar Uma Memória Com Locking Otimista
### Problem
- Dois agentes editando a mesma memória simultaneamente causa corrupção silenciosa de last-write-wins
- Sem detecção de conflito, seu pipeline sobrescreve mudanças de um colega sem aviso


### Solution
```bash
UPDATED=$(sqlite-graphrag read --name design-auth --json | jaq -r '.updated_at')
sqlite-graphrag edit --name design-auth \
  --body-file ./revised.md \
  --expected-updated-at "$UPDATED"
```


### Explanation
- Cada `edit` cria uma nova versão imutável preservando o histórico completo de edições anteriores
- `--expected-updated-at` habilita locking otimista; exit code 3 sinaliza modificação concorrente
- No exit code 3, releia a memória com `read --json` para obter o novo `updated_at` e tente novamente
- `--body-file` lê o novo corpo de um arquivo; alternativas são `--body` (inline) e `--body-stdin` (pipe)
- Altere apenas a descrição sem tocar o corpo: `edit --name <nome> --description "nova desc"`
- JSON response inclui `memory_id`, `name`, `action` ("updated"), `version` e `elapsed_ms`


### Variants
- Use `--body-stdin` para canalizar o corpo de outro comando: `cat revised.md | sqlite-graphrag edit --name design-auth --body-stdin`
- Omita `--expected-updated-at` quando escritas concorrentes são impossíveis (pipelines de agente único)


### See Also
- Receita "Como Fazer Round-Trip De Forget E Restore Em Uma Memória"
- Receita "Como Renomear Uma Memória Preservando Todo O Histórico"


## Como Renomear Uma Memória Preservando Todo O Histórico
### Problem
- Sua equipe renomeou o projeto de `auth-v1` para `authentication-flow` mas todos links do grafo ainda apontam para o nome antigo
- Delete-e-recrie manual perde histórico de versões e quebra auditorias de compliance


### Solution
```bash
sqlite-graphrag rename auth-v1 authentication-flow
sqlite-graphrag history --name authentication-flow --json | jaq '.versions | length'
```


### Explanation
- Argumentos posicionais `rename <antigo> <novo>` são suportados desde v1.0.44
- Todas versões e conexões de grafo transferem para o novo nome automaticamente
- `--from`/`--to` e `--name`/`--new-name` são aceitos como aliases de flag desde v1.0.35
- Exit code 4 sinaliza que a memória de origem não existe no namespace atual
- JSON response inclui `memory_id`, `name` (novo), `action` ("renamed"), `version` e `elapsed_ms`


### Variants
- Aplique locking otimista: `--expected-updated-at` previne renomear uma memória que mudou desde sua última leitura
- Verifique preservação do histórico: `history --name <novo> --json | jaq '.versions[].created_at_iso'`


### See Also
- Receita "Como Editar Uma Memória Com Locking Otimista"
- Receita "Como Fazer Round-Trip De Forget E Restore Em Uma Memória"


## Como Importar Corpora Grandes Em Hosts Com Memória Limitada
### Problem
- Seu pipeline de ingestão de 5000 arquivos leva horas porque GLiNER NER roda em cada corpo
- Carregar o modelo GLiNER (1,1 GB fp32 padrão, 349 MB com `--gliner-variant int8`) na primeira execução excede o orçamento de memória do CI


### Solution
```bash
sqlite-graphrag ingest ./big-corpus --recursive \
  --low-memory --max-files 50000 --json \
  | jaq -c 'select(.summary) | {files_total, files_succeeded, elapsed_ms}'
```


### Explanation
- GLiNER NER desabilitado por padrão; passe `--enable-ner` para ativar (adiciona aproximadamente 100-200 ms por arquivo em cache quente)
- Use `--gliner-variant int8` com `--enable-ner` para reduzir download do modelo de 1,1 GB para 349 MB com perda mínima de acurácia
- `--low-memory` força `--ingest-parallelism 1`, reduzindo RSS em aproximadamente 40 por cento para hosts restritos
- `--max-files 50000` eleva o cap de segurança do padrão 10000; a operação é rejeitada inteiramente se contagem de arquivos exceder o cap
- Dois eixos de paralelismo existem: `--max-concurrency` controla invocações CLI, `--ingest-parallelism` controla threads de extract+embed
- Trade-off é 3 a 4 vezes mais tempo de wall-clock para footprint de memória significativamente menor
- Linha resumo NDJSON reporta `files_total`, `files_succeeded`, `files_failed` e `elapsed_ms` para auditoria de pipeline


### Variants
- Defina `SQLITE_GRAPHRAG_LOW_MEMORY=1` como env var persistente em vez de passar `--low-memory` por invocação
- Combine com chamadas separadas de `remember --entities-file` para grafos curados em documentos críticos


### See Also
- Receita "Como Importar Em Massa Um Diretório De Base De Conhecimento"
- Receita "Como Tratar Exit Codes Em Pipelines Automatizados"
