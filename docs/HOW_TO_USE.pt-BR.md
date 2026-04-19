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
neurographrag purge --days 30 --yes
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
  | jaq '.hits[] | {name, score, source}'
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
neurographrag sync-safe-copy --output ~/Dropbox/neurographrag.sqlite
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
      contexto: [.hits[] | {name, body, score}],
      gerado_em: now | todate
    }' \
  | claude --print "Use este contexto para responder: $QUERY_USUARIO"
```
- JSON estruturado flui limpo para qualquer orquestrador que leia de stdin nativamente
- Campo de score permite ao orquestrador descartar hits de baixa relevância antes do prompt
- Determinismo dos códigos de saída permite rotear erros sem parsear stderr manualmente
- Custo de tokens cai setenta por cento comparado ao context stuffing de corpus completo
- Latência ida e volta fica abaixo de cem milissegundos fim a fim localmente


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
- Leia `INTEGRATIONS.md` para configuração específica por vendor dos 21 agentes acima
- Leia `docs/AGENTS.md` para padrões multi-agente de orquestração via Agent Teams
- Leia `docs/CROSS_PLATFORM.md` para entender binários de targets nas nove plataformas
- Marque com estrela o repositório em github.com/daniloaguiarbr/neurographrag para acompanhar releases
