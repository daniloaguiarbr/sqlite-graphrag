# COMO USAR sqlite-graphrag

> Entregue memória persistente a qualquer agente de IA com uma binária local e zero dependências cloud


- Leia este guia em inglês em [HOW_TO_USE.md](HOW_TO_USE.md)
- Volte ao [README.md](../README.md) principal para referência completa de comandos


## A Pergunta Que Inicia Aqui
### Curiosidade — Por Que Engenheiros Abandonam Pinecone em 2026
- Quantos milissegundos separam seu agente da memória em produção hoje mesmo
- Por que engenheiros seniores em produção escolhem SQLite sobre Pinecone para LLMs
- O que muda quando embeddings, busca e grafo vivem dentro de um único arquivo
- Por que vinte e um agentes de IA convergem para sqlite-graphrag como persistência
- Este guia responde cada pergunta acima em menos de dez minutos de leitura


## Tempo de Leitura e Impacto
### Investimento — Cinco Minutos de Leitura e Dez de Execução
- Leitores técnicos conseguem escanear este guia rapidamente pelos títulos
- O tempo da primeira execução depende principalmente do download único do modelo
- Padrões CLI tradicionais mantêm a curva de aprendizado baixa para usuários de shell
- A primeira memória pode ser persistida logo após a instalação e a inicialização
- A primeira busca híbrida depende do hardware, da residência do modelo e do tamanho do banco
- Armazenamento local remove dependências recorrentes de retrieval em cloud do fluxo


## Pré-Requisitos
### Ambiente — Linha de Base Mínima Suportada
- Rust 1.88 ou mais recente instalado via `rustup` em Linux macOS e Windows
- SQLite versão 3.40 ou mais nova acompanhando sua distribuição do sistema operacional
- Os assets publicados cobrem Linux glibc, macOS Apple Silicon e Windows em x86_64 ou ARM64
- RAM disponível de 100 MB livre em runtime mais 1 GB durante a carga do modelo
- Espaço em disco de 200 MB para o cache do modelo de embeddings na primeira invocação
- Acesso de rede EXCLUSIVAMENTE no primeiro `init` para baixar embeddings quantizados


## Primeiro Comando em 60 Segundos
### Instalação — Três Linhas de Shell Que Você Copia Uma Vez
```bash
cargo install --path .
sqlite-graphrag init
sqlite-graphrag remember --name primeira-memoria --type user --description "primeira memória" --body "olá graphrag"
```
- Primeira linha baixa, compila e instala o binário em `~/.cargo/bin`
- Segunda linha cria o banco SQLite e baixa o modelo de embeddings do `fastembed`
- Terceira linha persiste sua primeira memória e indexa para recuperação híbrida
- Confirmação vai para stdout, traces vão para stderr, código zero sinaliza sucesso
- Sua próxima chamada de `recall` retorna a nota recém-salva assim que o modelo estiver pronto


## Comandos Essenciais
### Ciclo de Vida — Sete Subcomandos Que Você Usa Todos os Dias
```bash
sqlite-graphrag init --namespace meu-projeto
sqlite-graphrag remember --name design-auth --type decision --description "auth usa JWT" --body "Justificativa documentada."
sqlite-graphrag recall "estratégia de autenticação" --k 5 --json
sqlite-graphrag hybrid-search "design jwt" --k 10 --rrf-k 60 --json
sqlite-graphrag read --name design-auth
sqlite-graphrag forget --name design-auth
sqlite-graphrag purge --retention-days 90 --yes
```
- `init` inicializa o banco, baixa o modelo e valida a extensão `sqlite-vec`
- `remember` armazena conteúdo e gera embeddings atomicamente; nós e arestas do grafo são persistidos quando fornecidos explicitamente
- `recall` executa KNN vetorial sobre `vec_memories` e expande matches de grafo por padrão, exceto com `--no-graph`
- `hybrid-search` funde FTS5 textual e KNN vetorial via Reciprocal Rank Fusion
- `read` recupera memória pelo nome kebab-case exato em uma única query SQL
- `forget` faz remoção lógica preservando integralmente o histórico de versões
- `purge` apaga permanentemente memórias removidas há mais de N dias de retenção


## Daemon Persistente
### Reuse O Modelo De Embeddings Entre Comandos Pesados
```bash
sqlite-graphrag daemon
sqlite-graphrag daemon --ping
sqlite-graphrag daemon --stop
sqlite-graphrag daemon --db ./graphrag.sqlite --ping --json
```
- `init`, `remember`, `recall` e `hybrid-search` tentam usar o daemon automaticamente primeiro
- Se o daemon não estiver disponível, esses comandos sobem o processo sob demanda antes de cair para o caminho local
- Subir `sqlite-graphrag daemon` manualmente agora é opcional e útil principalmente para supervisão explícita ou debug
- Use `--ping` para confirmar que o daemon está vivo e inspecionar o contador de requests de embedding atendidos
- Use `--stop` para shutdown gracioso após sessões longas de agentes ou ingestão em lote
- `--db` e `--json` são aceitos para manter o mesmo contrato global da CLI usado por pipelines de agentes


## Padrões Avançados
### Receita Um — Busca Híbrida Com Fusão Ponderada
```bash
sqlite-graphrag hybrid-search "estratégia migração postgres" \
  --k 20 \
  --rrf-k 60 \
  --weight-vec 0.7 \
  --weight-fts 0.3 \
  --json \
  | jaq '.results[] | {name, score, source}'
```
- Combina similaridade vetorial densa e matches textuais esparsos em ranqueamento único
- Ajuste de pesos permite favorecer proximidade semântica sobre precisão de palavras
- Constante RRF `--rrf-k 60` coincide com o padrão recomendado pelo paper original
- O pipeline mantém campos de ranking explícitos para a orquestração downstream
- A latência depende do hardware, da residência do modelo e do tamanho do banco


### Receita Dois — Travessia de Grafo Para Recall Multi-Hop
```bash
sqlite-graphrag link --from design-auth --to spec-jwt --relation depends-on
sqlite-graphrag link --from spec-jwt --to rfc-7519 --relation mentions
sqlite-graphrag related design-auth --hops 2 --json \
  | jaq -r '.results[] | select(.hop_distance == 2) | .name'
```
- Dois hops revelam conhecimento transitivo invisível à busca vetorial pura
- Relações tipadas permitem ao agente raciocinar sobre causa, dependência e referência
- Queries de grafo permanecem locais dentro de joins SQLite e relações tipadas
- Recall multi-hop recupera contexto que o primeiro passe vetorial frequentemente não traz
- Distância de hop entrega ao orquestrador um sinal explícito de profundidade de expansão


### Receita Três — Ingestão Em Lote Via Pipeline Shell
```bash
find ./docs -name "*.md" -print0 \
  | xargs -0 -n 1 -P 1 -I {} bash -c '
      nome=$(basename "$1" .md)
      sqlite-graphrag remember \
        --max-concurrency 1 \
        --name "doc-${nome}" \
        --type reference \
        --description "importado de $1" \
        --body-file "$1"
    ' _ {}
```
- Inicie ingestão em lote com `-P 1` e só aumente após medir RSS no host atual
- Código de saída `75` sinaliza slots exauridos e o orquestrador DEVE tentar depois
- Código de saída `77` sinaliza pressão de RAM e o orquestrador DEVE aguardar memória
- `--body-file` evita deriva de quoting shell em corpos Markdown
- Throughput de ingestão pesada depende do hardware, do daemon e do tamanho dos documentos


### Receita Quatro — Sincronização Segura Com Dropbox ou iCloud
```bash
sqlite-graphrag sync-safe-copy --dest ~/Dropbox/graphrag.sqlite
ouch compress ~/Dropbox/graphrag.sqlite ~/Dropbox/graphrag-$(date +%Y%m%d).tar.zst
```
- `sync-safe-copy` faz checkpoint do WAL e copia snapshot consistente atomicamente
- O snapshot reduz o risco de um sincronizador copiar um banco SQLite em mutação
- A taxa de compressão varia com o conteúdo do banco e o estado do WAL
- A recuperação continua simples com uma descompressão e uma cópia
- Use a cópia com checkpoint em vez de sincronizar o banco vivo diretamente


### Receita Cinco — Integração Com Orquestrador Claude Code
```bash
sqlite-graphrag recall "$QUERY_USUARIO" --k 5 --json \
  | jaq -c '{
      contexto: [.results[] | {name, snippet, distance, source}],
      gerado_em: now | todate
    }' \
  | claude --print "Use este contexto para responder: $QUERY_USUARIO"
```
- JSON estruturado flui limpo para qualquer orquestrador downstream que leia o stdout deste comando pelo próprio stdin
- Campo `distance` permite ao orquestrador descartar hits fracos de recall antes do prompt
- Determinismo dos códigos de saída permite rotear erros sem parsear stderr manualmente
- Recall retorna snippets em vez de corpos completos, o que ajuda a manter prompts menores
- A latência fim a fim depende da CLI local e do runtime do modelo downstream


## Configuração e Notas de Namespace
### Namespace Padrão
- Namespace padrão é `global` quando `--namespace` é omitido
- Configure via variável de ambiente `SQLITE_GRAPHRAG_NAMESPACE` para sobrescrever globalmente
- Use `namespace-detect` para inspecionar o namespace resolvido antes de operações em massa

### Semântica do Score
- `recall` emite `distance`, onde valores menores significam matches mais similares
- `hybrid-search` emite `score` e `combined_score`, onde valores maiores sobem no ranking
- Prefira sempre `--json` em pipelines para o orquestrador usar os campos brutos realmente retornados

### Aliases da Flag --lang
- `--lang en` força saída em inglês independente do locale do sistema
- `--lang pt`, `--lang pt-BR`, `--lang portuguese` e `--lang PT` forçam português
- Variável `SQLITE_GRAPHRAG_LANG=pt` sobrescreve o locale do sistema quando `--lang` está ausente
- Todos os aliases resolvem para as mesmas duas variantes internas: inglês e português

### Flag --json
- `--json` é aceita por todos os subcomandos como flag ampla de compatibilidade para JSON determinístico no stdout
- `--format json` é aceita apenas pelos comandos que expõem `--format` no help
- Use `--json` em pipelines quando quiser uma grafia única que funcione na CLI inteira
- Quando `--json` aparece com um `--format` não JSON, `--json` vence e stdout continua JSON
- Use `--format json` apenas nos comandos que anunciam `--format`

### Flags de Formato de Saída Padronizadas
- Todos os subcomandos emitem JSON por padrão no stdout
- `--json` é a forma curta — preferida em one-liners e pipelines de agentes
- `--format json` é a forma explícita — disponível apenas nos comandos que expõem `--format`
- Saída humana `text` e `markdown` existe hoje apenas em um subconjunto de comandos
- Matriz atual de suporte a flags:

| Subcomando | `--json` | `--format json` | Saída padrão |
|---|---|---|---|
| `remember` | sim | sim | json |
| `recall` | sim | sim | json |
| `read` | sim | não | json |
| `list` | sim | sim | json |
| `forget` | sim | não | json |
| `link` | sim | sim | json |
| `unlink` | sim | sim | json |
| `stats` | sim | não | json |
| `health` | sim | não | json |
| `history` | sim | não | json |
| `edit` | sim | não | json |
| `rename` | sim | sim | json |
| `restore` | sim | sim | json |
| `purge` | sim | não | json |
| `cleanup-orphans` | sim | sim | json |
| `optimize` | sim | não | json |
| `migrate` | sim | não | json |
| `init` | sim | não | json |
| `sync-safe-copy` | sim | não | json |
| `hybrid-search` | sim | sim | json |
| `related` | sim | sim | json |
| `namespace-detect` | sim | não | json |
| `daemon` | sim | não | json |

```bash
# Forma curta — preferida em pipelines
sqlite-graphrag recall "auth" --json | jaq '.results[].name'

# Forma explícita — saída idêntica
sqlite-graphrag recall "auth" --format json | jaq '.results[].name'

# Ambas as formas aceitas no mesmo pipeline
sqlite-graphrag stats --json && sqlite-graphrag recall "auth" --format json
```

### Descoberta do Caminho do Banco
- O comportamento padrão sempre usa `graphrag.sqlite` no diretório atual
- Todos os comandos aceitam a flag `--db <PATH>` além da variável `SQLITE_GRAPHRAG_DB_PATH`
- Flag CLI tem precedência sobre a variável de ambiente
- Use `--db` somente quando precisar intencionalmente de um banco fora do diretório atual

### Contrato do ONNX Runtime em ARM64 GNU
- Em `aarch64-unknown-linux-gnu`, comandos pesados de embedding usam `ort/load-dynamic` em vez de linkar o ONNX Runtime no build
- A binária procura `libonnxruntime.so` nesta ordem: `ORT_DYLIB_PATH`, diretório do executável, `./lib/` ao lado do executável e depois o diretório de cache de modelos
- Se nenhum desses caminhos contiver a biblioteca, o processo inicia mas a primeira operação de embedding falha quando `ort` não consegue carregar o runtime
- Distribua `libonnxruntime.so` ao lado da binária ou exporte `ORT_DYLIB_PATH` explicitamente em unidades de serviço e jobs de CI
- Este contrato se aplica a `init`, `remember`, `recall` e `hybrid-search` nos builds ARM64 GNU

### Formato do Log
- `SQLITE_GRAPHRAG_LOG_FORMAT=json` emite eventos de tracing como JSON delimitado por linha no stderr
- Valor padrão é `pretty`; qualquer valor diferente de `json` usa o formato legível por humanos
- Use `json` ao encaminhar logs para agregadores estruturados como Loki ou Datadog

### Fuso Horário de Exibição
- `SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo` aplica qualquer fuso IANA a todos os campos `*_iso` no JSON de saída
- A flag `--tz <IANA>` tem prioridade sobre a variável de ambiente; ambos caem para UTC quando ausentes
- Campos epoch inteiros (`created_at`, `updated_at`) nunca são afetados — apenas os campos ISO string correspondentes
- Nomes IANA inválidos causam exit 2 com erro de validação descritivo antes de o comando executar
- Exemplos: `America/New_York`, `Europe/Berlin`, `Asia/Tokyo`, `America/Sao_Paulo`
```bash
# Uso pontual com flag
sqlite-graphrag read --name minha-nota --tz America/Sao_Paulo

# Persistente via variável de ambiente
export SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo
sqlite-graphrag list | jaq '.items[].updated_at_iso'
```

### Limite de Concorrência
- `--max-concurrency` é limitado a `2×nCPUs`; valores maiores retornam exit 2 ainda no parse dos argumentos
- Comandos pesados de embedding podem ser reduzidos ainda mais em runtime com base na RAM disponível e no orçamento de RSS por processo medido para o modelo ONNX
- Trate `init`, `remember`, `recall` e `hybrid-search` como comandos pesados ao planejar automação ou auditorias
- Exit code 2 sinaliza argumento inválido; reduza o valor e repita a invocação
- O teto rígido continua em 4 subprocessos cooperantes, mas o limite seguro efetivo pode ser menor no host atual
- Em auditorias inicie comandos pesados com `--max-concurrency 1` e só aumente após medir RSS e swap

### Idioma dos Textos de Ajuda das Flags Globais
- As flags globais `--max-concurrency`, `--wait-lock`, `--lang` e `--tz` exibem textos de ajuda em inglês no output de `--help`
- Isso é deliberado: o help do clap fica estático e consistente entre screenshots, docs e transcrições de shell
- A flag `--lang` altera apenas mensagens humanas de runtime em stderr; o JSON stdout e o help do clap permanecem determinísticos


## Referência — Subcomandos Não Cobertos no Início Rápido
### Usando cleanup-orphans
- Remove entidades sem memórias vinculadas e sem relacionamentos no grafo
- Execute periodicamente após operações `forget` em massa para manter a tabela de entidades enxuta
```bash
sqlite-graphrag cleanup-orphans --dry-run
sqlite-graphrag cleanup-orphans --yes
```
- Pré-requisitos: nenhum — funciona em qualquer banco inicializado
- `--dry-run` exibe a contagem de entidades órfãs sem remover nada
- `--yes` suprime a confirmação interativa para pipelines automatizados
- Exit code 0: limpeza concluída (ou nada a limpar)
- Exit code 75: slot exaurido, repita após breve backoff

### Usando edit
- Altera o corpo ou a descrição de uma memória existente criando nova versão imutável
- Use `--expected-updated-at` para locking otimista em pipelines de agentes concorrentes
```bash
sqlite-graphrag edit --name design-auth --body "Justificativa atualizada após revisão do RFC"
sqlite-graphrag edit --name design-auth --description "Nova descrição curta"
sqlite-graphrag edit --name design-auth \
  --body-file ./corpo-atualizado.md \
  --expected-updated-at "2026-04-19T12:00:00Z"
```
- Pré-requisitos: a memória deve existir no namespace de destino
- `--body-file` lê o conteúdo do corpo a partir de um arquivo, evitando problemas de escape
- `--body-stdin` lê o corpo via stdin para integração em pipelines
- `--body`, `--body-file` e `--body-stdin` são mutuamente exclusivos
- `--expected-updated-at` aceita epoch Unix ou RFC 3339; divergências retornam exit 3
- Exit code 0: edição concluída e nova versão indexada
- Exit code 3: conflito de locking otimista — a memória foi modificada concorrentemente

### Usando graph
- Exporta snapshot completo de entidades e relações em JSON, DOT ou Mermaid
- Formatos DOT e Mermaid habilitam visualização em Graphviz, VS Code ou mermaid.live
```bash
sqlite-graphrag graph --format json
sqlite-graphrag graph --format dot --output grafo.dot
sqlite-graphrag graph --format mermaid --output grafo.mmd
```
- Pré-requisitos: ao menos uma chamada `link` ou `remember` deve ter criado entidades
- `--format json` (padrão) emite `{"nodes": [...], "edges": [...]}` no stdout
- `--format dot` emite um grafo direcionado compatível com Graphviz para renderização offline
- `--format mermaid` emite um bloco de fluxograma Mermaid para embutir em Markdown
- `--json` força JSON no stdout mesmo quando `--format dot`, `--format mermaid` ou `graph stats --format text` também estiver presente
- `--output <PATH>` grava diretamente em arquivo em vez de imprimir no stdout, exceto quando `--json` está presente
- Exit code 0: exportação concluída

#### Usando graph traverse
- Percorre o grafo de entidades a partir de um nó inicial até a profundidade indicada
- Use `--from` para nomear a entidade raiz e `--depth` para controlar quantos hops seguir
```bash
sqlite-graphrag graph traverse --from AuthDecision --depth 2 --format json
sqlite-graphrag graph traverse --from JwtSpec --depth 1
```
- Pré-requisitos: a entidade raiz informada em `--from` deve existir no grafo
- `--from <NOME>` define a entidade raiz pelo nome (obrigatório)
- `--depth <N>` controla a distância máxima de hop a partir da raiz (padrão: 2)
- Schema de saída: `{"from": "...", "namespace": "...", "depth": N, "hops": [...], "elapsed_ms": N}`
- Cada hop carrega `entity`, `relation`, `direction`, `weight` e `depth`
- Exit code 0: travessia concluída
- Exit code 4: entidade raiz não encontrada

#### Usando graph stats
- Retorna estatísticas agregadas sobre o grafo de entidades no namespace de destino
- Use para inspecionar densidade e conectividade do grafo antes de executar travessias
```bash
sqlite-graphrag graph stats --format json
sqlite-graphrag graph stats --namespace meu-projeto
```
- Pré-requisitos: ao menos uma entidade deve existir no namespace de destino
- Campos de saída: `namespace`, `node_count`, `edge_count`, `avg_degree`, `max_degree`, `elapsed_ms`
- `--format json` (padrão) emite o objeto de estatísticas no stdout
- `--format text` emite uma linha compacta legível por humano
- Exit code 0: estatísticas retornadas

#### Usando graph entities
- Lista entidades tipadas do grafo com filtros opcionais por tipo, namespace, limite e offset
- Use para enumerar todas as entidades conhecidas pelo grafo antes de executar `traverse` ou `link`
```bash
sqlite-graphrag graph entities --json
sqlite-graphrag graph entities --entity-type concept --limit 20
sqlite-graphrag graph entities --entity-type person --namespace meu-projeto --json
sqlite-graphrag graph entities --limit 50 --offset 100 --json
```
- Pré-requisitos: ao menos uma entidade deve existir — criada via `remember` ou `link` explícito
- `--entity-type <TIPO>` filtra resultados por um único tipo; tipos válidos: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- `--limit <N>` limita a contagem de resultados (padrão: 50); `--offset <N>` habilita paginação por cursor
- Schema de saída: `{"items": [...], "total_count": N, "limit": N, "offset": N, "namespace": "...", "elapsed_ms": N}`
- Cada item contém `id`, `name`, `entity_type`, `namespace` e `created_at`
- Exit code 0: lista retornada (array `items` vazio quando nenhuma entidade corresponde ao filtro)
- Exit code 4: namespace não encontrado

### Usando health
- Executa verificação de integridade e reporta estatísticas de armazenamento do banco ativo
- Use em scripts de inicialização de agentes para detectar bancos corrompidos antes de processar
```bash
sqlite-graphrag health
sqlite-graphrag health --json
```
- Pré-requisitos: um banco inicializado deve existir
- Executa `PRAGMA integrity_check` primeiro; retorna exit code 10 com `integrity_ok: false` se corrupção for detectada
- Schema de saída: `{"status":"ok","integrity":"ok","integrity_ok":true,"schema_ok":true,"counts":{"memories":N,"entities":N,"relationships":N,"vec_memories":N},"db_path":"...","db_size_bytes":N,"schema_version":N,"wal_size_mb":N.N,"journal_mode":"wal","checks":[{"name":"integrity","ok":true}],"elapsed_ms":N}`
- `journal_mode` reporta o modo de journaling do SQLite (`wal` ou `delete`)
- `wal_size_mb` reporta o tamanho atual do arquivo WAL em megabytes (0.0 quando não está em modo WAL)
- `checks` é um array de objetos diagnósticos com `name` e `ok`
- `integrity_ok` é `true` quando `integrity_check` retorna `"ok"` e `false` caso contrário
- Exit code 0: banco está íntegro
- Exit code 10: verificação de integridade falhou — trate como banco corrompido

### Usando history
- Lista todas as versões imutáveis de uma memória nomeada em ordem cronológica reversa
- Use o inteiro `version` retornado com `restore` para retornar a qualquer estado anterior
```bash
sqlite-graphrag history --name design-auth
```
- Pré-requisitos: a memória deve existir e ter ao menos uma versão armazenada
- Saída é objeto JSON com `name`, `namespace`, `versions` e `elapsed_ms`
- Versões começam em 1 e incrementam a cada chamada bem-sucedida de `edit` ou `restore`
- Exit code 0: histórico retornado
- Exit code 4: memória não encontrada no namespace de destino

### Usando namespace-detect
- Resolve e exibe o namespace efetivo para o contexto de invocação atual
- Use para depurar conflitos entre `--namespace`, `SQLITE_GRAPHRAG_NAMESPACE` e auto-detecção
```bash
sqlite-graphrag namespace-detect
sqlite-graphrag namespace-detect --namespace meu-projeto
```
- Pré-requisitos: nenhum — funciona sem banco de dados presente
- Saída JSON com campos `namespace`, `source`, `cwd` e `elapsed_ms`
- Ordem de precedência: flag `--namespace` > env `SQLITE_GRAPHRAG_NAMESPACE` > auto-detecção
- Exit code 0: resolução concluída

### Usando __debug_schema
- Subcomando diagnóstico oculto que exibe o schema SQLite completo e o histórico de migrações
- Use ao solucionar problemas de deriva de schema entre versões do binário ou após migrações com falha
```bash
sqlite-graphrag __debug_schema
sqlite-graphrag __debug_schema --db /caminho/para/custom.db
```
- Pré-requisitos: um banco de dados inicializado deve existir no caminho padrão ou especificado
- Schema de saída: `{"schema_version": N, "user_version": N, "objects": [...], "migrations": [...], "elapsed_ms": N}`
- `schema_version` espelha `PRAGMA user_version`; `user_version` é o valor bruto do PRAGMA
- `objects` lista todos os objetos do schema SQLite (tabelas, índices, tabelas virtuais) com `name` e `type`
- `migrations` lista todas as linhas de `refinery_schema_history` com `version`, `name` e `applied_on`
- Este subcomando está intencionalmente oculto do `--help`; invoque-o pelo nome exato
- Exit code 0: dump do schema concluído

### Usando rename
- Renomeia uma memória preservando todo o histórico de versões e conexões do grafo de entidades
- Use `--name`/`--old` e `--new-name`/`--new` de forma intercambiável; aliases legados continuam suportados
```bash
sqlite-graphrag rename --name nome-antigo --new-name nome-novo
sqlite-graphrag rename --old nome-antigo --new nome-novo
```
- Pré-requisitos: a memória de origem deve existir; o nome de destino deve estar disponível
- `--expected-updated-at` habilita locking otimista para evitar conflitos de rename concorrente
- Entradas do histórico permanecem vinculadas ao nome original para integridade da trilha de auditoria
- Exit code 0: rename concluído
- Exit code 3: conflito de locking otimista
- Exit code 4: memória de origem não encontrada

### Usando restore
- Cria nova versão de uma memória a partir do corpo de uma versão antiga sem sobrescrever o histórico
- Use `history` primeiro para descobrir os números de versão disponíveis antes de chamar `restore`
```bash
sqlite-graphrag history --name design-auth
sqlite-graphrag restore --name design-auth --version 2
```
- Pré-requisitos: a memória deve existir e o número de versão alvo deve ser válido
- Restore NÃO sobrescreve o histórico — ele adiciona nova versão com o corpo antigo
- `--expected-updated-at` habilita locking otimista para segurança em pipelines concorrentes
- Exit code 0: restore concluído e nova versão indexada
- Exit code 4: número de versão não encontrado na tabela de histórico

### Usando unlink
- Remove uma aresta tipada específica entre duas entidades do grafo
- Use `--from`/`--source` e `--to`/`--target` de forma intercambiável; aliases legados continuam suportados
```bash
sqlite-graphrag unlink --from design-auth --to spec-jwt --relation depends-on
sqlite-graphrag unlink --source design-auth --target spec-jwt --relation depends-on
```
- Pré-requisitos: a aresta deve existir; os três argumentos `--from`, `--to` e `--relation` são obrigatórios
- Valores válidos para `--relation`: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- Ambas as entidades `--from`/`--to` devem ser nós tipados do grafo; tipos válidos: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- Exit code 0: aresta removida
- Exit code 4: aresta não encontrada


## Notas Adicionais Sobre Comandos Essenciais
### Nota sobre link
- Pré-requisito: as entidades devem existir no grafo antes de criar links explícitos
- Crie primeiro memórias com payloads explícitos de grafo e depois chame `link` para tipar arestas adicionais
- Use `--from`/`--source` e `--to`/`--target` de forma intercambiável; aliases legados continuam suportados
- Ambas as entidades `--from` e `--to` devem ser nós tipados do grafo; tipos válidos: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- Tentar vincular entidades cujos nomes não correspondam a nó tipado retorna exit code 4
- Saída JSON: `{action, from, source, to, target, relation, weight, namespace}`
```bash
sqlite-graphrag remember --name design-auth --type decision --description "..." --body "Usa JWT e OAuth2."
sqlite-graphrag remember --name spec-jwt --type reference --description "..." --body "RFC 7519 define JWT."
sqlite-graphrag link --from design-auth --to spec-jwt --relation depends-on
```

### Nota sobre forget
- `forget` executa remoção lógica; a memória desaparece dos resultados de `recall` e `list`
- Saída JSON: `{forgotten, name, namespace}`
- Execute `purge` depois para apagar permanentemente as linhas removidas e recuperar espaço em disco

### Nota sobre optimize e migrate
- `optimize --json` retorna `{db_path, status}`
- `migrate --json` retorna `{db_path, schema_version, status}`
- Execute `migrate` após toda atualização do binário para aplicar mudanças de schema com segurança

### Nota sobre cleanup-orphans
- Saída JSON: `{orphan_count, deleted, dry_run, namespace}`
- Execute `--dry-run` primeiro para confirmar a contagem antes de passar `--yes` em automação

### Nota sobre o schema dos nós do grafo
- `graph --format json` emite `{"nodes": [...], "edges": [...]}`
- Campos de nó: `{id, name, namespace, kind, type}` onde `kind` e `type` carregam o mesmo valor
- Campos de aresta são `{from, to, relation, weight}`

### Nota sobre remember
- `--force-merge` atualiza o corpo de uma memória existente em vez de retornar exit code 2 por nome duplicado
- Use `--force-merge` em loops de pipeline idempotentes onde a mesma chave pode aparecer múltiplas vezes
- `--entities-file` aceita arquivo JSON onde cada objeto deve incluir o campo `entity_type`
- O campo alias `type` também é aceito como sinônimo de `entity_type`
- NÃO envie `entity_type` e `type` no mesmo objeto porque o parser trata isso como campo duplicado
- Valores válidos para `entity_type`: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- Valores inválidos de `entity_type` são rejeitados na ingestão com erro de validação descritivo
- `--relationships-file` aceita um array JSON onde cada objeto deve incluir `source`, `target`, `relation` e `strength`; `from` e `to` são aceitos como aliases de `source` e `target`
- `--graph-stdin` aceita um objeto JSON com `body` opcional, `entities` e `relationships`; JSON inválido falha e não é salvo como texto do body
- `--graph-stdin` é mutuamente exclusivo com `--body`, `--body-file`, `--body-stdin`, `--entities-file` e `--relationships-file`
- `remember` aceita payloads de body até `512000` bytes e até `512` chunks; payloads maiores retornam exit code `6`
- `strength` deve ser número de ponto flutuante no intervalo inclusivo `[0.0, 1.0]`
- `strength` é mapeado para o campo `weight` nas saídas de relacionamentos e travessia de grafo
- `relation` em `--relationships-file` aceita rótulos canônicos persistidos como `uses`, `supports`, `applies_to`, `depends_on` e `tracked_in`; aliases com hífen como `depends-on` e `tracked-in` são normalizados antes da gravação

```json
[
  { "name": "SQLite", "entity_type": "tool" },
  { "name": "GraphRAG", "type": "concept" }
]
```

```json
[
  {
    "source": "SQLite",
    "target": "GraphRAG",
    "relation": "supports",
    "strength": 0.8,
    "description": "SQLite suporta GraphRAG local"
  }
]
```
```bash
sqlite-graphrag remember --name notas-config --type project \
  --description "config atualizada" --body "Novo conteúdo do corpo" --force-merge
```


## Integração Com Agentes de IA
### Vinte e Um Agentes — Uma Única Camada de Persistência
- Claude Code da Anthropic consome JSON do stdout e orquestra via códigos de saída
- Codex da OpenAI lê saída do hybrid-search para ancorar geração em memória local
- Gemini CLI do Google parseia saída `--json` para injetar fatos em prompts ativos
- Opencode como harness open source trata sqlite-graphrag como backend MCP nativo
- OpenClaw framework de agentes usa `recall` como tier de memória de longo prazo
- Paperclip assistente de pesquisa persiste achados entre sessões via `remember`
- VS Code Copilot da Microsoft invoca o CLI por meio de tasks no terminal integrado
- Google Antigravity plataforma chama o binário dentro do runtime isolado de workers
- Windsurf da Codeium roteia memórias indexadas do projeto via `hybrid-search`
- Cursor editor conecta `recall` ao painel de chat para completions com contexto
- Zed editor invoca sqlite-graphrag como ferramenta externa no canal de assistente
- Aider agente de código consulta `related` para raciocínio multi-hop sobre commits
- Jules do Google Labs usa códigos de saída como gate de reviews automatizados em PR
- Kilo Code agente autônomo delega memória de longo prazo ao arquivo SQLite local
- Roo Code orquestrador passa contexto de memória à fase de planejamento deterministicamente
- Cline agente autônomo persiste saídas de ferramentas via `remember` entre ciclos
- Continue assistente open source integra via API própria de context provider customizado
- Factory framework de agentes armazena logs de decisão para fluxos auditáveis multi-agente
- Augment Code assistente hidrata seu cache de embeddings a partir do `hybrid-search`
- JetBrains AI Assistant executa sqlite-graphrag como processo paralelo para memória entre projetos
- OpenRouter camada proxy injeta contexto recuperado antes de repassar requisições upstream


## Erros Comuns
### Solução de Problemas — Cinco Falhas e Suas Correções
- Erro `exit 10` sinaliza lock do banco, execute `sqlite-graphrag vacuum` para checkpoint do WAL
- Erro `exit 12` sinaliza falha ao carregar `sqlite-vec`, verifique se SQLite é versão 3.40 ou superior
- Erro `exit 13` sinaliza falha parcial em batch, inspecione os resultados parciais e repita apenas os itens falhos
- Erro `exit 15` sinaliza banco ocupado após tentativas, reduza a pressão de escrita ou aumente `--wait-lock`
- Erro `exit 75` sinaliza slots exauridos, repita após breve intervalo de backoff
- Erro `exit 77` sinaliza RAM baixa, libere memória antes de invocar o modelo novamente


## Próximos Passos
### Evolução — Para Onde Ir Depois Deste Guia
- Leia `COOKBOOK.md` para trinta receitas cobrindo busca, grafo e fluxos em lote
- Leia `INTEGRATIONS.md` para configuração específica por vendor dos 27 agentes acima
- Leia `docs/AGENTS.md` para padrões multi-agente de orquestração via Agent Teams
- Leia `docs/CROSS_PLATFORM.md` para entender binários de targets nas nove plataformas
- Marque com estrela o repositório público quando `sqlite-graphrag` for publicado para acompanhar releases
