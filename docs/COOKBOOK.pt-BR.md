# Livro de Receitas neurographrag


> 15 receitas de nível produção que poupam horas da sua equipe toda semana

- Leia a versão em inglês em [COOKBOOK.md](COOKBOOK.md)


## Como Bootstrapar O Banco De Memória Em 60 Segundos
### Problem
- Seu laptop novo não tem banco de memória e seu agente perde contexto o tempo todo
- Cada onboarding queima 30 minutos com scripts frágeis e caça ao README


### Solution
```bash
cargo install --locked neurographrag
neurographrag init --namespace default
neurographrag health --json
```


### Explanation
- Comando `init` cria o arquivo SQLite e baixa `multilingual-e5-small` localmente
- Flag `--namespace default` fixa o escopo inicial para seus agentes concordarem no alvo
- Comando `health` valida a integridade com `PRAGMA integrity_check` devolvendo JSON
- Exit code `0` sinaliza que o banco está pronto para leitura e escrita por qualquer agente
- Poupa 30 minutos por laptop contra bootstrap Pinecone mais Docker mais Python


### Variants
- Defina `NEUROGRAPHRAG_DB_PATH=/data/team.sqlite` para compartilhar arquivo entre pods dev
- Rode `neurographrag migrate --json` após bump de versão para aplicar upgrade de schema


### See Also
- Receita "Como Integrar neurographrag Com Loop Subprocess Do Claude Code"
- Receita "Como Agendar Purge E Vacuum Em Cron Ou GitHub Actions"


## Como Importar Em Massa A Base De Conhecimento Via Pipeline Stdin
### Problem
- Seus 2000 arquivos Markdown ficam parados porque nenhum loader fala o schema neurographrag
- Entrada manual queima uma tarde inteira para cada cem arquivos de onboarding simples


### Solution
```bash
fd -e md docs/ -0 | xargs -0 -n 1 -I{} sh -c '
  neurographrag remember \
    --name "$(basename {} .md)" \
    --type user \
    --description "imported from {}" \
    --body-stdin < {}
'
```


### Explanation
- `fd -e md -0` emite caminhos Markdown null-delimited seguros contra espaços e aspas
- `xargs -0 -n 1` invoca `neurographrag remember` uma vez por arquivo sem corrida de concorrência
- `--body-stdin` canaliza o corpo Markdown sem acidente de escape shell ou aspas
- Exit code `2` sinaliza duplicatas para você pular limpamente no loop externo
- Poupa 4 horas por mil arquivos contra loaders CSV feitos à mão


### Variants
- Adicione `parallel -j 4` para respeitar `MAX_CONCURRENT_CLI_INSTANCES` e reduzir wall-clock
- Estenda o one-liner para extrair `--description` do primeiro heading Markdown do arquivo


### See Also
- Receita "Como Exportar Memórias Para NDJSON Para Backup"
- Receita "Como Orquestrar Recall Paralelo Entre Namespaces"


## Como Combinar Busca Vetorial E FTS Com Pesos Ajustáveis
### Problem
- Recall vetorial puro perde matches exatos de token tipo `TODO-1234` em comentários de código
- FTS puro perde paráfrases que seus usuários digitaram em sinônimos e abreviações


### Solution
```bash
neurographrag hybrid-search "postgres migration deadlock" \
  --k 10 --rrf-k 60 --vec-weight 0.6 --fts-weight 0.4 --json
```


### Explanation
- `--rrf-k 60` é a constante de suavização Reciprocal Rank Fusion recomendada na literatura
- `--vec-weight 0.6` pende o recall em direção à similaridade semântica com maior fidelidade
- `--fts-weight 0.4` mantém matches exatos de palavra visíveis nos ranks fundidos do topo
- JSON emite `rank_vec` e `rank_fts` por hit para agentes downstream auditarem a fusão
- Poupa 50 por cento dos tokens contra pedir a um LLM para re-rankear após vetor puro


### Variants
- Defina `--vec-weight 1.0 --fts-weight 0.0` para reproduzir um baseline `recall` puro em A/B
- Eleve `--k` para 50 antes de um re-ranker agent podar até os 5 hits finais


### See Also
- Receita "Como Debugar Queries Lentas Com Health E Stats"
- Receita "Como Fazer Benchmark De hybrid-search Contra recall Vetorial Puro"


## Como Percorrer O Grafo De Entidades Para Recall Multi-Hop
### Problem
- Sua query acerta uma memória mas perde notas conectadas que compartilham o mesmo grafo
- RAG vetorial puro pontua tokens similares e ignora relações tipadas que importam


### Solution
```bash
neurographrag related authentication-flow --hops 2 --json
```


### Explanation
- `related` percorre arestas tipadas armazenadas em `entity_edges` com contagem controlada
- `--hops 2` inclui memórias amigas-de-amigos conectadas via entidades compartilhadas
- Saída JSON reporta o caminho da travessia para o LLM raciocinar sobre cadeias de relação
- Poupa custo de re-embedding porque a expansão roda como grafo SQLite e não KNN
- Revela contexto que o RAG vetorial puro ignora com 80 por cento menos tokens


### Variants
- Use `graph --json` para dump completo quando um auditor humano quiser análise offline
- Encadeie `related` em `hybrid-search` filtrando candidatos ao conjunto percorrido


### See Also
- Receita "Como Combinar Busca Vetorial E FTS Com Pesos Ajustáveis"
- Receita "Como Orquestrar Recall Paralelo Entre Namespaces"


## Como Integrar neurographrag Com Loop Subprocess Do Claude Code
### Problem
- Claude Code reinicia a cada sessão e esquece decisões feitas cinco minutos atrás
- Seu orquestrador não tem memória determinística entre iterações do agente


### Solution
```bash
# .claude/hooks/pre-task.sh
CONTEXT=$(neurographrag recall "$USER_PROMPT" --k 5 --json)
printf 'Relevant memories:\n%s\n' "$CONTEXT"

# .claude/hooks/post-task.sh
neurographrag remember \
  --name "session-$(date +%s)" \
  --type agent \
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
- Receita "Como Configurar Painel Assistente Windsurf Ou Zed Com neurographrag"


## Como Integrar Com Codex CLI Via AGENTS.md
### Problem
- Codex lê `AGENTS.md` mas pula qualquer capacidade sem sintaxe exata de invocação listada
- Sua equipe de ops perde 10 minutos por sessão ensinando Codex o mesmo CLI de memória


### Solution
```md
<!-- AGENTS.md na raiz do repo -->
## Memory Layer
- Use `neurographrag recall "<query>" --k 5 --json` to fetch prior decisions
- Use `neurographrag remember --name "<kebab-name>" --type agent --body "<text>"` to persist output
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
- Adicione `NEUROGRAPHRAG_NAMESPACE=$REPO_NAME` no `.envrc` para Codex isolar memória por projeto
- Inclua um one-liner de exemplo sob cada comando para ancorar Codex em uso real


### See Also
- Receita "Como Integrar neurographrag Com Loop Subprocess Do Claude Code"
- Receita "Como Integrar Com Terminal Do Cursor Para Memória No Editor"


## Como Integrar Com Terminal Do Cursor Para Memória No Editor
### Problem
- Cursor perde contexto toda vez que você fecha o editor ou troca de branch localmente
- Sua sessão LLM pareada reinicia fria e repete as mesmas perguntas toda manhã


### Solution
```jsonc
// Snippet do settings.json do Cursor
{
  "terminal.integrated.env.osx": { "NEUROGRAPHRAG_NAMESPACE": "${workspaceFolderBasename}" },
  "cursor.ai.rules": "Before answering, run `neurographrag recall \"${selection}\" --k 5 --json` and use hits as context"
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
- Receita "Como Configurar Painel Assistente Windsurf Ou Zed Com neurographrag"
- Receita "Como Integrar Com Codex CLI Via AGENTS.md"


## Como Configurar Painel Assistente Windsurf Ou Zed Com neurographrag
### Problem
- Painéis assistentes do Windsurf e Zed saem sem backend de memória plugável por padrão
- Seu fluxo multi-IDE fragmenta memória entre silos Cursor Windsurf e Zed


### Solution
```bash
# Comando de terminal compartilhado que ambos IDEs podem rodar
neurographrag hybrid-search "$EDITOR_CONTEXT" --k 10 --json > /tmp/ng.json
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
neurographrag sync-safe-copy --output ~/Dropbox/neurographrag/snapshot.sqlite
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
name: neurographrag maintenance
on:
  schedule: [{ cron: "0 3 * * 0" }]
jobs:
  maintenance:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo install --locked neurographrag
      - run: neurographrag purge --days 30 --yes
      - run: neurographrag vacuum --json
      - run: neurographrag optimize --json
```


### Explanation
- `purge --days 30` apaga definitivamente linhas soft-deletadas mais antigas que a janela
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
neurographrag list --limit 10000 --json \
  | jaq -c '.memories[]' > memories-$(date +%Y%m%d).ndjson
```


### Explanation
- `list --limit 10000` enumera memórias até o teto com ordenação determinística estável
- `jaq -c '.memories[]'` achata o array em NDJSON legível por qualquer ferramenta instantaneamente
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
git add .gitattributes neurographrag.sqlite
git commit -m "chore: track neurographrag db via LFS"
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


## Como Orquestrar Recall Paralelo Entre Namespaces
### Problem
- Seu agente multi-projeto roda quatro buscas em série desperdiçando 2 segundos por iteração
- Seu orquestrador CI dispara um subprocess por namespace e estoura a concorrência segura


### Solution
```bash
parallel -j 4 'NEUROGRAPHRAG_NAMESPACE={} neurographrag recall "error rate" --k 5 --json' \
  ::: project-a project-b project-c project-d
```


### Explanation
- GNU parallel limita a concorrência em 4 batendo com `MAX_CONCURRENT_CLI_INSTANCES` interno
- Env var `NEUROGRAPHRAG_NAMESPACE` escopa cada subprocess ao seu próprio projeto limpo
- Exit code `75` dispara retry automático já que `parallel` lê exit codes nativamente
- Quatro documentos JSON caem no stdout para um agregador downstream fundir ranks
- Poupa 75 por cento do wall-clock contra recall serial entre os mesmos namespaces


### Variants
- Troque `parallel` por `xargs -P 4` se prefere tooling POSIX puro em imagens enxutas
- Canalize o JSON agregado em um agente RRF que funde ranks cross-namespace juntos


### See Also
- Receita "Como Combinar Busca Vetorial E FTS Com Pesos Ajustáveis"
- Receita "Como Fazer Benchmark De hybrid-search Contra recall Vetorial Puro"


## Como Debugar Queries Lentas Com Health E Stats
### Problem
- Seu recall que retornava em 8 ms agora leva 400 ms depois de meses de escrita
- Você não enxerga qual tabela inchou ou qual índice ficou stale ao longo do tempo


### Solution
```bash
neurographrag health --json | jaq '{integrity, wal_size_mb, journal_mode}'
neurographrag stats --json | jaq '{memories, entities, edges, avg_body_len}'
NEUROGRAPHRAG_LOG_LEVEL=debug neurographrag recall "slow query" --k 5 --json
```


### Explanation
- `health` reporta `integrity_check` tamanho WAL e journal mode para detectar fragmentação rápido
- `stats` conta linhas revelando qual tabela cresceu desproporcionalmente desde a última auditoria
- `NEUROGRAPHRAG_LOG_LEVEL=debug` emite tempos por estágio SQLite em stderr para tracing
- Comparar `avg_body_len` atual ao baseline mostra se os bodies cresceram além dos defaults
- Poupa horas de tuning às cegas expondo o caminho lento exato em três comandos


### Variants
- Agende um painel que raspa `stats --json` toda hora e alerta em picos de crescimento
- Rode `optimize` seguido de `vacuum` quando o WAL passa de 100 MB para reclamar performance


### See Also
- Receita "Como Agendar Purge E Vacuum Em Cron Ou GitHub Actions"
- Receita "Como Fazer Benchmark De hybrid-search Contra recall Vetorial Puro"


## Como Fazer Benchmark De hybrid-search Contra recall Vetorial Puro
### Problem
- Você não tem dados para justificar habilitar hybrid search em produção contra vetor puro
- Seus stakeholders querem evidência numérica antes de aprovar o overhead de índice


### Solution
```bash
hyperfine --warmup 3 \
  'neurographrag recall "postgres migration" --k 10 --json > /dev/null' \
  'neurographrag hybrid-search "postgres migration" --k 10 --json > /dev/null'
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
