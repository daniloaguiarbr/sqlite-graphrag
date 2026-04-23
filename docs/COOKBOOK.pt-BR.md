# Livro de Receitas sqlite-graphrag


> 15 receitas de nível produção que poupam horas da sua equipe toda semana

- Leia a versão em inglês em [COOKBOOK.md](COOKBOOK.md)


## Nota de Latência
- O CLI é stateless: cada invocação carrega o modelo de embeddings (aproximadamente 1 segundo)
- Para fluxos de produção que exigem latência abaixo de 50 ms, use o modo daemon (previsto para v3.0.0)
- O `recall` single-shot atual leva aproximadamente 1 segundo em hardware moderno
- Pipelines em lote amortizam esse custo invocando o binário uma vez por documento em paralelo


## Referência de Valores Padrão
- `recall --k` padrão é 10 (não 5) — ajuste conforme o tradeoff precisão-revocação
- `list --limit` padrão é 50 — use `--limit 10000` para exportações completas antes de backup
- `hybrid-search --weight-vec` e `--weight-fts` ambos têm padrão 1.0
- `purge --retention-days` padrão é 90 — reduza para políticas de limpeza mais agressivas


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


## Como Importar Em Massa A Base De Conhecimento Via Pipeline Stdin
### Problem
- Seus 2000 arquivos Markdown ficam parados porque nenhum loader fala o schema sqlite-graphrag
- Entrada manual queima uma tarde inteira para cada cem arquivos de onboarding simples


### Solution
```bash
fd -e md docs/ -0 | xargs -0 -n 1 -I{} sh -c '
  sqlite-graphrag remember \
    --name "$(basename {} .md)" \
    --type user \
    --description "imported from {}" \
    --body-stdin < {}
'
```


### Explanation
- `fd -e md -0` emite caminhos Markdown null-delimited seguros contra espaços e aspas
- `xargs -0 -n 1` invoca `sqlite-graphrag remember` uma vez por arquivo sem corrida de concorrência
- `--body-stdin` canaliza o corpo Markdown sem acidente de escape shell ou aspas
- Exit code `2` sinaliza duplicatas para você pular limpamente no loop externo
- Poupa 4 horas por mil arquivos contra loaders CSV feitos à mão


### Variants
- Mantenha imports de `remember` em modo serial com `--max-concurrency 1`; cada subprocesso pode carregar cerca de 1,1 GB de RSS ONNX na v1.0.3
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


### See Also
- Receita "Como Debugar Queries Lentas Com Health E Stats"
- Receita "Como Fazer Benchmark De hybrid-search Contra recall Vetorial Puro"


## Como Percorrer O Grafo De Entidades Para Recall Multi-Hop
### Problem
- Sua query acerta uma memória mas perde notas conectadas que compartilham o mesmo grafo
- RAG vetorial puro pontua tokens similares e ignora relações tipadas que importam


### Solution
```bash
sqlite-graphrag related authentication-flow --hops 2 --json
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


## Como Debugar Queries Lentas Com Health E Stats
### Problem
- Seu recall que retornava em 8 ms agora leva 400 ms depois de meses de escrita
- Você não enxerga qual tabela inchou ou qual índice ficou stale ao longo do tempo


### Solution
```bash
sqlite-graphrag health --json | jaq '{integrity, wal_size_mb, journal_mode}'
sqlite-graphrag stats --json | jaq '{memories, entities, edges, avg_body_len}'
SQLITE_GRAPHRAG_LOG_LEVEL=debug sqlite-graphrag recall "slow query" --k 5 --json
```


### Explanation
- `health` reporta `integrity_check` tamanho WAL e journal mode para detectar fragmentação rápido
- `stats` conta linhas revelando qual tabela cresceu desproporcionalmente desde a última auditoria
- `SQLITE_GRAPHRAG_LOG_LEVEL=debug` emite tempos por estágio SQLite em stderr para tracing
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
    let status = Command::new("sqlite-graphrag")
        .args(["remember", "--namespace", namespace, conteudo])
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
        .filter_map(|v| v["body"].as_str().map(str::to_owned))
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
    let status = Command::new("sqlite-graphrag")
        .args(["remember", "--namespace", &namespace, conteudo])
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
                    if let Some(body) = item["body"].as_str() {
                        resultados.push((agent_id.to_string(), body.to_owned()));
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
    let status = Command::new("sqlite-graphrag")
        .args(["remember", "--namespace", namespace, &entrada])
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
    let status = Command::new("sqlite-graphrag")
        .args(["remember", "--namespace", namespace, &entrada])
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
        .filter_map(|v| v["body"].as_str())
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
    let status = Command::new("sqlite-graphrag")
        .args(["remember", "--namespace", "ollama-local", conteudo])
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
        .filter_map(|v| v["body"].as_str().map(str::to_owned))
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

### See Also
- Receita "Como Inicializar Banco De Dados De Memória Em 60 Segundos"
- Receita "Como Configurar Saída de Idioma Com a Flag --lang"
