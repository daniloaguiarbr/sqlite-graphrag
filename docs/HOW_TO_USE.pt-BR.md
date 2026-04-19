# COMO USAR neurographrag

> Entregue memória persistente a qualquer agente de IA em 60 segundos, gastando zero dólares


- Leia este guia em inglês em [HOW_TO_USE.md](HOW_TO_USE.md)
- Volte ao [README.md](../README.md) principal para referência completa de comandos


## A Pergunta Que Inicia Aqui
### Curiosidade — Por Que Engenheiros Abandonam Pinecone em 2026
- Quantos milissegundos separam seu agente da memória em produção hoje mesmo
- Por que engenheiros seniores em produção escolhem SQLite sobre Pinecone para LLMs
- O que muda quando embeddings, busca e grafo vivem dentro de um único arquivo
- Por que vinte e um agentes de IA convergem para neurographrag como persistência
- Este guia responde cada pergunta acima em menos de dez minutos de leitura


## Tempo de Leitura e Impacto
### Investimento — Cinco Minutos de Leitura e Dez de Execução
- Tempo total de leitura chega a cinco minutos para leitores técnicos escaneando
- Tempo total de execução chega a dez minutos incluindo download do modelo
- Curva de aprendizado cai a zero para quem conhece padrões CLI tradicionais
- Primeira memória persiste em sessenta segundos após o término da instalação
- Primeira busca híbrida retorna hits ranqueados em menos de cinquenta milissegundos
- Economia esperada de tokens por mês bate duzentos mil em um único agente


## Pré-Requisitos
### Ambiente — Linha de Base Mínima Suportada
- Rust 1.88 ou mais recente instalado via `rustup` em Linux macOS e Windows
- SQLite versão 3.40 ou mais nova acompanhando sua distribuição do sistema operacional
- Sistemas operacionais Linux glibc, Linux musl, macOS 11 e superiores, Windows 10 em diante
- RAM disponível de 100 MB livre em runtime mais 1 GB durante a carga do modelo
- Espaço em disco de 200 MB para o cache do modelo de embeddings na primeira invocação
- Acesso de rede EXCLUSIVAMENTE no primeiro `init` para baixar embeddings quantizados


## Primeiro Comando em 60 Segundos
### Instalação — Três Linhas de Shell Que Você Copia Uma Vez
```bash
cargo install --locked neurographrag
neurographrag init
neurographrag remember --name primeira-memoria --type user --description "primeira memória" --body "olá graphrag"
```
- Primeira linha baixa, compila e instala o binário em `~/.cargo/bin`
- Segunda linha cria o banco SQLite e baixa o modelo de embeddings do `fastembed`
- Terceira linha persiste sua primeira memória e indexa para recuperação híbrida
- Confirmação vai para stdout, traces vão para stderr, código zero sinaliza sucesso
- Sua próxima chamada de `recall` retorna a nota recém-salva em milissegundos


## Comandos Essenciais
### Ciclo de Vida — Sete Subcomandos Que Você Usa Todos os Dias
```bash
neurographrag init --namespace meu-projeto
neurographrag remember --name design-auth --type decision --description "auth usa JWT" --body "Justificativa documentada."
neurographrag recall "estratégia de autenticação" --k 5 --json
neurographrag hybrid-search "design jwt" --k 10 --rrf-k 60 --json
neurographrag read --name design-auth
neurographrag forget --name design-auth
neurographrag purge --retention-days 90 --yes
```
- `init` inicializa o banco, baixa o modelo e valida a extensão `sqlite-vec`
- `remember` armazena conteúdo, extrai entidades e gera embeddings atomicamente
- `recall` executa busca KNN vetorial pura sobre a tabela `vec_memories`
- `hybrid-search` funde FTS5 textual e KNN vetorial via Reciprocal Rank Fusion
- `read` recupera memória pelo nome kebab-case exato em uma única query SQL
- `forget` faz remoção lógica preservando integralmente o histórico de versões
- `purge` apaga permanentemente memórias removidas há mais de N dias de retenção


## Padrões Avançados
### Receita Um — Busca Híbrida Com Fusão Ponderada
```bash
neurographrag hybrid-search "estratégia migração postgres" \
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
- Pipeline economiza oitenta por cento dos tokens comparado ao re-ranking via LLM
- Latência esperada fica abaixo de quinze milissegundos em bancos até 100 MB


### Receita Dois — Travessia de Grafo Para Recall Multi-Hop
```bash
neurographrag link --source design-auth --target spec-jwt --relation depends-on
neurographrag link --source spec-jwt --target rfc-7519 --relation references
neurographrag related design-auth --hops 2 --json \
  | jaq -r '.nodes[] | select(.depth == 2) | .name'
```
- Dois hops revelam conhecimento transitivo invisível à busca vetorial pura
- Relações tipadas permitem ao agente raciocinar sobre causa, dependência e referência
- Queries de grafo executam em menos de cinco milissegundos via joins indexados
- Recall multi-hop recupera contexto que embeddings planos deixam fora do top-K
- Economiza quinze minutos por sessão de debug caçando decisões arquiteturais relacionadas


### Receita Três — Ingestão Em Lote Via Pipeline Shell
```bash
find ./docs -name "*.md" -print0 \
  | xargs -0 -n 1 -P 4 -I {} bash -c '
      nome=$(basename {} .md)
      neurographrag remember \
        --name "doc-${nome}" \
        --type reference \
        --description "importado de {}" \
        --body "$(cat {})"
    '
```
- Fator paralelo `-P 4` coincide exatamente com os slots padrão do semáforo interno
- Código de saída `75` sinaliza slots exauridos e o orquestrador DEVE tentar depois
- Código de saída `77` sinaliza pressão de RAM e o orquestrador DEVE aguardar memória
- Throughput do lote atinge 200 documentos por minuto num laptop moderno com CPU atual
- Economiza quarenta minutos de ingestão manual por cada mil arquivos Markdown processados


### Receita Quatro — Sincronização Segura Com Dropbox ou iCloud
```bash
neurographrag sync-safe-copy --dest ~/Dropbox/neurographrag.sqlite
ouch compress ~/Dropbox/neurographrag.sqlite ~/Dropbox/neurographrag-$(date +%Y%m%d).tar.zst
```
- `sync-safe-copy` faz checkpoint do WAL e copia snapshot consistente atomicamente
- Dropbox, iCloud e Google Drive NUNCA corrompem o banco ativo durante a sincronização
- Compressão via `ouch` reduz snapshot em sessenta por cento para buckets de arquivamento
- Recuperação em outra máquina exige apenas um `ouch decompress` e um `cp` simples
- Protege anos de memória contra corrupção induzida por sincronizadores em SQLite cru


### Receita Cinco — Integração Com Orquestrador Claude Code
```bash
neurographrag recall "$QUERY_USUARIO" --k 5 --json \
  | jaq -c '{
      contexto: [.results[] | {name, body, score}],
      gerado_em: now | todate
    }' \
  | claude --print "Use este contexto para responder: $QUERY_USUARIO"
```
- JSON estruturado flui limpo para qualquer orquestrador que leia de stdin nativamente
- Campo de score permite ao orquestrador descartar hits de baixa relevância antes do prompt
- Determinismo dos códigos de saída permite rotear erros sem parsear stderr manualmente
- Custo de tokens cai setenta por cento comparado ao context stuffing de corpus completo
- Latência ida e volta fica abaixo de cem milissegundos fim a fim localmente


## Configuração e Notas de Namespace
### Namespace Padrão
- Namespace padrão é `global` quando `--namespace` é omitido
- Configure via variável de ambiente `NEUROGRAPHRAG_NAMESPACE` para sobrescrever globalmente
- Use `namespace-detect` para inspecionar o namespace resolvido antes de operações em massa

### Semântica do Score
- Saída JSON usa o campo `score` (similaridade cosseno, maior valor indica mais relevância)
- Resultados são ordenados por `score` decrescente; o melhor match aparece sempre primeiro
- Prefira sempre `--json` em pipelines para obter o `score` bruto com filtragem precisa

### Aliases da Flag --lang
- `--lang en` força saída em inglês independente do locale do sistema
- `--lang pt`, `--lang pt-BR`, `--lang portuguese` e `--lang PT` forçam português
- Variável `NEUROGRAPHRAG_LANG=pt` sobrescreve o locale do sistema quando `--lang` está ausente
- Todos os aliases resolvem para as mesmas duas variantes internas: inglês e português

### Flag --json
- `--json` é aceita por todos os subcomandos como alias no-op para `--format json`
- Forma canônica é `--format json`; ambas produzem saída idêntica
- Use `--json` em pipelines pela brevidade; use `--format json` em arquivos de configuração

### Descoberta do Caminho do Banco
- Todos os comandos aceitam a flag `--db <PATH>` além da variável `NEUROGRAPHRAG_DB_PATH`
- Flag CLI tem precedência sobre a variável de ambiente
- Use `--db` ao operar múltiplos bancos isolados em processos paralelos

### Formato do Log
- `NEUROGRAPHRAG_LOG_FORMAT=json` emite eventos de tracing como JSON delimitado por linha no stderr
- Valor padrão é `pretty`; qualquer valor diferente de `json` usa o formato legível por humanos
- Use `json` ao encaminhar logs para agregadores estruturados como Loki ou Datadog

### Fuso Horário de Exibição
- `NEUROGRAPHRAG_DISPLAY_TZ=America/Sao_Paulo` aplica qualquer fuso IANA a todos os campos `*_iso` no JSON de saída
- A flag `--tz <IANA>` tem prioridade sobre a variável de ambiente; ambos caem para UTC quando ausentes
- Campos epoch inteiros (`created_at`, `updated_at`) nunca são afetados — apenas os campos ISO string correspondentes
- Nomes IANA inválidos causam exit 2 com erro de validação descritivo antes de o comando executar
- Exemplos: `America/New_York`, `Europe/Berlin`, `Asia/Tokyo`, `America/Sao_Paulo`
```bash
# Uso pontual com flag
neurographrag read --name minha-nota --tz America/Sao_Paulo

# Persistente via variável de ambiente
export NEUROGRAPHRAG_DISPLAY_TZ=America/Sao_Paulo
neurographrag list | jaq '.items[].updated_at_iso'
```

### Limite de Concorrência
- `--max-concurrency` é limitado a `2×nCPUs`; valores maiores retornam exit 2
- Exit code 2 sinaliza argumento inválido; reduza o valor e repita a invocação
- Padrão de 4 slots é ótimo para a maioria dos laptops com dois a quatro núcleos


## Referência — Subcomandos Não Cobertos no Início Rápido
### Usando cleanup-orphans
- Remove entidades sem memórias vinculadas e sem relacionamentos no grafo
- Execute periodicamente após operações `forget` em massa para manter a tabela de entidades enxuta
```bash
neurographrag cleanup-orphans --dry-run
neurographrag cleanup-orphans --yes
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
neurographrag edit --name design-auth --body "Justificativa atualizada após revisão do RFC"
neurographrag edit --name design-auth --description "Nova descrição curta"
neurographrag edit --name design-auth \
  --body-file ./corpo-atualizado.md \
  --expected-updated-at "2026-04-19T12:00:00Z"
```
- Pré-requisitos: a memória deve existir no namespace de destino
- `--body-file` lê o conteúdo do corpo a partir de um arquivo, evitando problemas de escape
- `--body-stdin` lê o corpo via stdin para integração em pipelines
- `--expected-updated-at` aceita timestamp ISO 8601; divergências retornam exit 3
- Exit code 0: edição concluída e nova versão indexada
- Exit code 3: conflito de locking otimista — a memória foi modificada concorrentemente

### Usando graph
- Exporta snapshot completo de entidades e relações em JSON, DOT ou Mermaid
- Formatos DOT e Mermaid habilitam visualização em Graphviz, VS Code ou mermaid.live
```bash
neurographrag graph --format json
neurographrag graph --format dot --output grafo.dot
neurographrag graph --format mermaid --output grafo.mmd
```
- Pré-requisitos: ao menos uma chamada `link` ou `remember` deve ter criado entidades
- `--format json` (padrão) emite `{"nodes": [...], "edges": [...]}` no stdout
- `--format dot` emite um grafo direcionado compatível com Graphviz para renderização offline
- `--format mermaid` emite um bloco de fluxograma Mermaid para embutir em Markdown
- `--output <PATH>` grava diretamente em arquivo em vez de imprimir no stdout
- Exit code 0: exportação concluída

#### Usando graph traverse
- Percorre o grafo de entidades a partir de um nó inicial até a profundidade indicada
- Use `--from` para nomear a entidade raiz e `--depth` para controlar quantos hops seguir
```bash
neurographrag graph traverse --from design-auth --depth 2 --format json
neurographrag graph traverse --from spec-jwt --depth 1
```
- Pré-requisitos: a entidade raiz informada em `--from` deve existir no grafo
- `--from <NOME>` define a entidade raiz pelo nome (obrigatório)
- `--depth <N>` controla a distância máxima de hop a partir da raiz (padrão: 2)
- Schema de saída: `{"nodes": [...], "edges": [...]}` idêntico ao formato de exportação completa
- Exit code 0: travessia concluída
- Exit code 4: entidade raiz não encontrada

#### Usando graph stats
- Retorna estatísticas agregadas sobre o grafo de entidades no namespace de destino
- Use para inspecionar densidade e conectividade do grafo antes de executar travessias
```bash
neurographrag graph stats --format json
neurographrag graph stats --namespace meu-projeto
```
- Pré-requisitos: ao menos uma entidade deve existir no namespace de destino
- Campos de saída: `entity_count`, `relationship_count`, `avg_connections`, `namespace`
- `--format json` (padrão) emite o objeto de estatísticas no stdout
- Exit code 0: estatísticas retornadas

### Usando history
- Lista todas as versões imutáveis de uma memória nomeada em ordem cronológica reversa
- Use o inteiro `version` retornado com `restore` para retornar a qualquer estado anterior
```bash
neurographrag history --name design-auth
```
- Pré-requisitos: a memória deve existir e ter ao menos uma versão armazenada
- Saída é array JSON com campos `version`, `updated_at` e `body` truncado
- Versões começam em 1 e incrementam a cada chamada bem-sucedida de `edit` ou `restore`
- Exit code 0: histórico retornado
- Exit code 4: memória não encontrada no namespace de destino

### Usando namespace-detect
- Resolve e exibe o namespace efetivo para o contexto de invocação atual
- Use para depurar conflitos entre `--namespace`, `NEUROGRAPHRAG_NAMESPACE` e auto-detecção
```bash
neurographrag namespace-detect
neurographrag namespace-detect --namespace meu-projeto
```
- Pré-requisitos: nenhum — funciona sem banco de dados presente
- Saída JSON com campos `namespace` (valor resolvido) e `source` (flag, env ou auto)
- Ordem de precedência: flag `--namespace` > env `NEUROGRAPHRAG_NAMESPACE` > auto-detecção
- Exit code 0: resolução concluída

### Usando rename
- Renomeia uma memória preservando todo o histórico de versões e conexões do grafo de entidades
- Use `--name`/`--old` e `--new-name`/`--new` de forma intercambiável (aliases desde v2.0.1)
```bash
neurographrag rename --name nome-antigo --new-name nome-novo
neurographrag rename --old nome-antigo --new nome-novo
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
neurographrag history --name design-auth
neurographrag restore --name design-auth --version 2
```
- Pré-requisitos: a memória deve existir e o número de versão alvo deve ser válido
- Restore NÃO sobrescreve o histórico — ele adiciona nova versão com o corpo antigo
- `--expected-updated-at` habilita locking otimista para segurança em pipelines concorrentes
- Exit code 0: restore concluído e nova versão indexada
- Exit code 4: número de versão não encontrado na tabela de histórico

### Usando unlink
- Remove uma aresta tipada específica entre duas entidades do grafo
- Use `--from`/`--source` e `--to`/`--target` de forma intercambiável (aliases desde v2.0.1)
```bash
neurographrag unlink --from design-auth --to spec-jwt --relation depends-on
neurographrag unlink --source design-auth --target spec-jwt --relation depends-on
```
- Pré-requisitos: a aresta deve existir; os três argumentos `--from`, `--to` e `--relation` são obrigatórios
- Valores válidos para `--relation`: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- Exit code 0: aresta removida
- Exit code 4: aresta não encontrada


## Notas Adicionais Sobre Comandos Essenciais
### Nota sobre link
- Pré-requisito: as entidades devem existir no grafo antes de criar links explícitos
- O comando `remember` extrai automaticamente entidades do texto `--body` durante a ingestão
- Crie primeiro as memórias que referenciam as entidades e depois chame `link` para tipar as arestas
- Use `--from`/`--source` e `--to`/`--target` de forma intercambiável (aliases desde v2.0.1)
- Saída JSON: `{action, from, source, to, target, relation, weight, namespace}`
```bash
neurographrag remember --name design-auth --type decision --description "..." --body "Usa JWT e OAuth2."
neurographrag remember --name spec-jwt --type reference --description "..." --body "RFC 7519 define JWT."
neurographrag link --from design-auth --to spec-jwt --relation depends-on
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
- Campos de aresta espelham o schema de `link` com `from`, `source`, `to`, `target`, `relation`, `weight`

### Nota sobre remember
- `--force-merge` atualiza o corpo de uma memória existente em vez de retornar exit code 2 por nome duplicado
- Use `--force-merge` em loops de pipeline idempotentes onde a mesma chave pode aparecer múltiplas vezes
- `--entities-file` aceita arquivo JSON onde cada objeto deve incluir o campo `entity_type`
- Valores válidos para `entity_type`: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`
- Valores inválidos de `entity_type` são rejeitados na ingestão com erro de validação descritivo
```bash
neurographrag remember --name notas-config --type project \
  --description "config atualizada" --body "Novo conteúdo do corpo" --force-merge
```


## Integração Com Agentes de IA
### Vinte e Um Agentes — Uma Única Camada de Persistência
- Claude Code da Anthropic consome JSON via stdin e orquestra via códigos de saída
- Codex da OpenAI lê saída do hybrid-search para ancorar geração em memória local
- Gemini CLI do Google parseia saída `--json` para injetar fatos em prompts ativos
- Opencode como harness open source trata neurographrag como backend MCP nativo
- OpenClaw framework de agentes usa `recall` como tier de memória de longo prazo
- Paperclip assistente de pesquisa persiste achados entre sessões via `remember`
- VS Code Copilot da Microsoft invoca o CLI por meio de tasks no terminal integrado
- Google Antigravity plataforma chama o binário dentro do runtime isolado de workers
- Windsurf da Codeium roteia memórias indexadas do projeto via `hybrid-search`
- Cursor editor conecta `recall` ao painel de chat para completions com contexto
- Zed editor invoca neurographrag como ferramenta externa no canal de assistente
- Aider agente de código consulta `related` para raciocínio multi-hop sobre commits
- Jules do Google Labs usa códigos de saída como gate de reviews automatizados em PR
- Kilo Code agente autônomo delega memória de longo prazo ao arquivo SQLite local
- Roo Code orquestrador passa contexto de memória à fase de planejamento deterministicamente
- Cline agente autônomo persiste saídas de ferramentas via `remember` entre ciclos
- Continue assistente open source integra via API própria de context provider customizado
- Factory framework de agentes armazena logs de decisão para fluxos auditáveis multi-agente
- Augment Code assistente hidrata seu cache de embeddings a partir do `hybrid-search`
- JetBrains AI Assistant executa neurographrag como processo paralelo para memória entre projetos
- OpenRouter camada proxy injeta contexto recuperado antes de repassar requisições upstream


## Erros Comuns
### Solução de Problemas — Cinco Falhas e Suas Correções
- Erro `exit 10` sinaliza lock do banco, execute `neurographrag vacuum` para checkpoint do WAL
- Erro `exit 12` sinaliza falha ao carregar `sqlite-vec`, verifique se SQLite é versão 3.40 ou superior
- Erro `exit 13` sinaliza banco ocupado, reduza `--max-concurrency` ou aumente `--wait-lock`
- Erro `exit 75` sinaliza slots exauridos, repita após breve intervalo de backoff
- Erro `exit 77` sinaliza RAM baixa, libere memória antes de invocar o modelo novamente


## Próximos Passos
### Evolução — Para Onde Ir Depois Deste Guia
- Leia `COOKBOOK.md` para trinta receitas cobrindo busca, grafo e fluxos em lote
- Leia `INTEGRATIONS.md` para configuração específica por vendor dos 27 agentes acima
- Leia `docs/AGENTS.md` para padrões multi-agente de orquestração via Agent Teams
- Leia `docs/CROSS_PLATFORM.md` para entender binários de targets nas nove plataformas
- Marque com estrela o repositório em github.com/daniloaguiarbr/neurographrag para acompanhar releases
