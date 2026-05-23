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
- RODAR `optimize --json` para refrescar estatísticas do planner; resposta inclui `fts_rebuilt` (bool) indicando se o índice FTS5 também foi reconstruído
- USAR `optimize --skip-fts --json` para pular a etapa de reconstrução do FTS5 (mais rápido, usar quando FTS5 foi reconstruído recentemente)
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
- NOTAR que `SQLITE_GRAPHRAG_NAMESPACE` agora é respeitado por todos os comandos (corrigido na v1.0.51; anteriormente 8 comandos ignoravam a variável)
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
- ABORTAR quando nome IANA inválido retorna exit 2 (parsing de argumentos Clap)
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
- DECLARAR `--type` entre `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`; `--type` e `--description` são OPCIONAIS quando `--force-merge` é usado (herdados da memória existente)
- PREFERIR `--body-stdin` para corpos longos
- USAR `--body-file <PATH>` para evitar escape shell em Markdown
- PASSAR `--force-merge` em loops idempotentes; também restaura memórias soft-deleted e atualiza em um passo (desde v1.0.51)
- USAR `--dry-run` para validar inputs sem persistir ou rodar embeddings
- USAR `--clear-body` para limpar explicitamente o corpo de uma memória existente ao usar `--force-merge`; sem `--clear-body`, `--force-merge` com body vazio PRESERVA o corpo existente
- NER desabilitado por padrão; passar `--enable-ner` ou definir `SQLITE_GRAPHRAG_ENABLE_NER=1` para ativar extração GLiNER
- Campo `extraction_method` na resposta reporta: `gliner-<variant>+regex`, `regex-only` ou `none:extraction-failed`
- `--skip-extraction` está obsoleto desde v1.0.45 e não tem efeito; usar `--enable-ner` para ativar NER
- RESPEITAR limite de 512000 bytes e 512 chunks por body
- USAR `--max-rss-mb <MiB>` para abortar embedding se o RSS do processo ultrapassar o threshold (padrão 8192 MiB); reduzir em ambientes com memória restrita
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
- NUNCA depender de auto-extração GLiNER em CI sensível a RAM
- NUNCA exceder o cap de relações por memória sem ajustar env
- NUNCA usar `remember` em loop quando `ingest` cobre o caso
- NUNCA passar body vazio sem entidades via `--graph-stdin`; desde v1.0.54 retorna exit 1 (Validation) em vez de criar silenciosamente uma memória inerte com zero chunks
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
- USAR `--max-rss-mb <MiB>` para abortar se o RSS do processo ultrapassar o threshold durante o embedding (padrão 8192 MiB)
### OBRIGATÓRIO — Dois Eixos de Paralelismo
- `--max-concurrency <N>` controla CLI invocations simultâneas
- `--ingest-parallelism <N>` controla extract mais embed em paralelo
- PADRÃO de `--max-concurrency` é 4
- PADRÃO de `--ingest-parallelism` é `min(4, max(1, cpus/2))`
- DISTINGUIR claramente os dois eixos antes de ajustar
- AMPLIAR `--wait-lock <SECONDS>` para esperar slot antes de exit 75
### OBRIGATÓRIO — Performance e Extração
- NER desabilitado por padrão; passar `--enable-ner` para ativar extração GLiNER
- GLiNER NER adiciona aproximadamente 100-200 ms por arquivo com modelo carregado em hardware moderno
- GLiNER NER adiciona 2 a 30 segundos por arquivo em `--low-memory` ou no primeiro carregamento
- GLiNER NER baixa o modelo ONNX no primeiro run (fp32: 1,1 GB, int8: 349 MB via `--gliner-variant`)
- USAR `--gliner-variant int8` para CI/containers para reduzir modelo de 1,1 GB para 349 MB
- USAR `--enable-ner` apenas quando enriquecimento automático de entidades for valioso
- Campo `extraction_method` na resposta reporta: `gliner-<variant>+regex`, `regex-only` ou `none:extraction-failed`
- Duplicatas no ingest emitem `status: "skipped"` com `action: "duplicate"` em vez de `status: "failed"`
- PREFERIR `--graph-stdin` com entidades curadas por LLM para melhor qualidade (NER está desligado por padrão; `--skip-extraction` está obsoleto desde v1.0.45)
- USAR `--dry-run` para visualizar o mapeamento arquivo-nome sem carregar o modelo ONNX ou persistir dados
- Eventos NDJSON por arquivo incluem o campo `original_filename` preservando o basename do arquivo antes da normalização para kebab-case
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
- Linha por arquivo: `file`, `name`, `status` (`"indexed"` `"skipped"` `"failed"`), `truncated`, `original_name?`, `memory_id?`, `action?`, `error?`, `body_length?`
- Linha summary final: `summary` (true), `dir`, `pattern`, `recursive`, `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- Eventos de extração NER vão para stderr, NÃO stdout
- USAR `--max-name-length N` para sobrescrever o limite padrão de truncamento de 60 caracteres para nomes de memória
- Basenames numéricos (ex.: `123.md`) recebem o prefixo automático `doc-` para produzir nomes kebab-case válidos (ex.: `doc-123`)
### OBRIGATÓRIO — Modos de Ingestão (v1.0.62)
- `--mode none` (padrão): ingestão apenas do body, sem extração de entidades/relações
- `--mode gliner`: extração NER com GLiNER (requer `--enable-ner`, modelo ONNX local)
- `--mode claude-code`: extração curada por LLM via Claude Code CLI instalado localmente (`claude -p` headless)
- Modo Claude Code spawna `claude -p` por arquivo com `--json-schema` para saída estruturada garantida
- Requer Claude Code >= 2.1.0 instalado na máquina com assinatura Pro/Max ativa
- Extrai entidades do domínio e relações tipadas restritas a enums canônicos
- `--resume` continua ingestão interrompida a partir do queue DB; `--retry-failed` retenta apenas falhas
- `--max-cost-usd <N>` para quando custo acumulado exceder o orçamento
- `--claude-binary <PATH>` sobrescreve busca no PATH; `--claude-model <MODEL>` seleciona modelo
- --claude-timeout <S> define timeout por arquivo (padrão 300s); mata processos travados
- Queue DB `.ingest-queue.sqlite` rastreia progresso por arquivo; `--keep-queue` retém após conclusão
- Rate limit: backoff exponencial automático (60s → 120s → 300s → 900s)
- `--dry-run` com `--mode claude-code` emite eventos `status: "preview"` sem spawnar Claude — zero tokens consumidos
- Re-ingestão do mesmo diretório ATUALIZA memórias existentes (force-merge) em vez de falhar com UNIQUE constraint
- Falha de cold-start `--json-schema` automaticamente retentada uma vez após 2s (workaround para Claude Code Issue #23265)
- Subprocesso roda com `env_clear()` + injeção seletiva para hardening de segurança
- Usa `--bare` quando `ANTHROPIC_API_KEY` está definido (startup mais rápido, sem plugins); `--dangerously-skip-permissions` para OAuth
- Eventos NDJSON por arquivo incluem campos `entities` (contagem), `rels` (contagem), `cost_usd`
- Summary inclui `entities_total`, `rels_total`, `cost_usd` totais
- Schemas: `ingest-claude-phase.schema.json`, `ingest-claude-file-event.schema.json`, `ingest-claude-summary.schema.json`
- `--mode codex`: extração curada por LLM via OpenAI Codex CLI (`codex exec --json` headless por arquivo)
- Modo Codex requer Codex CLI >= 0.120.0 com API key OpenAI ativa; usa `--output-schema` para JSON estruturado
- `--codex-binary <PATH>` sobrescreve busca no PATH; `--codex-model <MODEL>` seleciona modelo; `--codex-timeout <S>` (padrão 300s)
- Variável de ambiente `SQLITE_GRAPHRAG_CODEX_BINARY` sobrescreve busca no PATH
- Pipeline completo de embedding aplicado — memórias ficam pesquisáveis via `recall` e `hybrid-search`
- Modo Codex reutiliza o mesmo formato NDJSON do claude-code: `ingest-claude-phase.schema.json`, `ingest-claude-file-event.schema.json`, `ingest-claude-summary.schema.json`
### Padrão Correto — Exemplos de Ingestão Claude Code
- `sqlite-graphrag ingest ./docs --mode claude-code --recursive --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --resume --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --max-cost-usd 5.00 --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --claude-model claude-sonnet-4-6 --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --claude-timeout 600 --max-cost-usd 10.00 --json`
### Padrão Correto — Exemplos de Ingestão Codex
- `sqlite-graphrag ingest ./docs --mode codex --recursive --json`
- `sqlite-graphrag ingest ./docs --mode codex --codex-model o4-mini --json`
- `sqlite-graphrag ingest ./docs --mode codex --codex-timeout 600 --json`
- `sqlite-graphrag ingest ./docs --mode codex --codex-binary /usr/local/bin/codex --json`


## CRUD — Read com read e list
### OBRIGATÓRIO — Leitura Direta por Nome (read)
- USAR `read --name <kebab-case>` para fetch O(1) por nome
- PARSEAR campos `body`, `description`, `created_at_iso`, `updated_at_iso`
- TRATAR exit code 4 como memória inexistente no namespace
- APLICAR `--tz` para localizar timestamps na saída
### OBRIGATÓRIO — Enumeração com Filtros (list)
- USAR `list --type <kind>` para filtrar por tipo de memória
- AJUSTAR `--limit <N>`; padrão é TODOS os registros no modo JSON, 50 no modo texto
- PAGINAR via `--offset <N>` para datasets grandes
- INCLUIR memórias soft-deletadas via `--include-deleted`
- EXPORTAR full dump com `--limit 10000 --json` antes de backup
- RESPOSTA agora inclui `total_count` (total de registros encontrados), `truncated` (bool), e `body_length` (int) por item
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
- v1.0.56: bug de dessincronização do FTS5 corrigido — memórias editadas ficam imediatamente localizáveis via busca full-text
### OBRIGATÓRIO — Renomeação Preservando Histórico (rename)
- USAR `rename --name <antigo> --new-name <novo>`
- ACEITAR `--old`/`--new` e `--from`/`--to` como aliases desde v1.0.35
- PRESERVAR todas as versões e conexões do grafo
- TRATAR exit code 4 como memória de origem ausente
- JSON response: `memory_id`, `name` (novo), `action` ("renamed"), `version`, `elapsed_ms`
- v1.0.56: bug de dessincronização do FTS5 corrigido — memórias renomeadas ficam imediatamente localizáveis via busca full-text
### OBRIGATÓRIO — Restauração de Versão Antiga (restore)
- INSPECIONAR versões via `history --name <nome>` primeiro
- USAR `restore --name <nome> --version <N>` para versão específica
- OMITIR `--version` seleciona última versão não-restore automaticamente
- RESTORE cria nova versão sem sobrescrever histórico anterior
- RE-EMBED ocorre automaticamente para recall vetorial voltar a encontrar
- JSON response inclui `action: "restored"`, `memory_id`, `name`, `version`, `restored_from`, `elapsed_ms`
- v1.0.56: bug de dessincronização do FTS5 corrigido — memórias restauradas ficam imediatamente localizáveis via busca full-text
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
- Desde v1.0.52: forget NÃO emite JSON quando a memória não é encontrada; retorna apenas erro no stderr + exit 4
### OBRIGATÓRIO — Remoção Física (purge)
- USAR `purge --retention-days <N> --yes` em automação
- PADRÃO de retenção é 90 dias para memórias soft-deletadas
- EXECUTAR `--dry-run` primeiro para auditar contagem
- APAGA permanentemente linhas e reclama espaço em disco
### OBRIGATÓRIO — Remoção de Aresta (unlink)
- USAR `unlink --from <a> --to <b> --relation <tipo>` para remoção direcionada
- `--relation` agora é OPCIONAL; omitir remove todas as arestas entre `--from` e `--to`
- USAR `--entity <nome> --all` para remover em massa TODOS os relacionamentos de uma entidade (qualquer direção)
- ACEITAR `--source`/`--target` como aliases de `--from`/`--to`
- TRATAR exit code 4 como aresta inexistente
- `--relation` aceita qualquer string em kebab-case ou snake_case; valores não canônicos emitem `tracing::warn!` desde v1.0.50
### OBRIGATÓRIO — Limpeza de Entidades Órfãs (cleanup-orphans)
- EXECUTAR `cleanup-orphans --dry-run` para auditar
- APLICAR `--yes` em pipelines automatizados
- REMOVE entidades sem memórias vinculadas nem arestas
- RODAR periodicamente após operações `forget` em massa
### OBRIGATÓRIO — Remoção em Massa de Relacionamentos (prune-relations)
- USAR `prune-relations --relation <tipo> --yes` para remoção em massa de todos os relacionamentos de um tipo
- USAR `--dry-run` para visualizar a contagem antes de confirmar
- USAR `--show-entities` com `--dry-run` para listar os nomes das entidades afetadas na resposta
- USAR `--yes` para pular confirmação interativa em pipelines automatizados
- ACEITA qualquer string em kebab-case ou snake_case como relação
- EXECUTAR `cleanup-orphans` depois para remover entidades sem relacionamentos restantes
- JSON response: `action` (`"pruned"` `"dry_run"`), `relation`, `count`, `entities_affected`, `affected_entity_names?`, `namespace`, `elapsed_ms`
### Padrão Correto — Round-Trip Forget e Restore
- `sqlite-graphrag forget --name decisao-x`
- `sqlite-graphrag history --name decisao-x --json | jaq '.deleted'`
- `sqlite-graphrag restore --name decisao-x`
- `sqlite-graphrag recall "decisão" --json`


## Gerenciamento de Entidades (v1.0.56)
### OBRIGATÓRIO — Validação de Nome de Entidade (v1.0.58)
- TODOS os caminhos de criação de entidade (`link --create-missing`, `remember --graph-stdin`, `ingest --enable-ner`, `rename-entity --new-name`) validam nomes via `validate_entity_name()`
- REJEITA nomes com menos de 2 caracteres (exit 1)
- REJEITA nomes contendo caracteres de quebra de linha (exit 1)
- REJEITA abreviações ALL_CAPS de 4 caracteres ou menos como ruído de NER (exit 1)
### OBRIGATÓRIO — Remover Entidade (delete-entity)
- USAR `delete-entity --name <entidade> --json` para remover permanentemente um nó de entidade
- ADICIONAR `--cascade` para também remover todos os relacionamentos e bindings de memória vinculados
- SEM `--cascade` o comando falha com exit 1 se a entidade tiver relacionamentos
- JSON response: `action`, `entity_name`, `relationships_removed`, `bindings_removed`, `elapsed_ms`
- TRATAR exit code 4 como entidade não encontrada
### OBRIGATÓRIO — Reclassificar Tipo de Entidade (reclassify)
- USAR `reclassify --name <entidade> --entity-type <novo> --json` para alterar o tipo de uma entidade individual
- USAR `reclassify --from-type <antigo> --to-type <novo> --batch --json` para reclassificar em massa todas as entidades de um tipo
- JSON response: `action`, `count`, `description_updated?`, `namespace`, `elapsed_ms`
### OBRIGATÓRIO — Mesclar Entidades (merge-entities)
- USAR `merge-entities --names "a,b,c" --into <alvo> --json` para mesclar múltiplas entidades em uma
- TODOS os relacionamentos das entidades de origem são movidos para `<alvo>`
- ENTIDADES de origem são deletadas após a mesclagem
- JSON response: `action`, `sources`, `target`, `relationships_moved`, `entities_removed`, `elapsed_ms`
- TRATAR exit code 4 como qualquer entidade nomeada não encontrada
### OBRIGATÓRIO — Listar Entidades de uma Memória (memory-entities)
- USAR `memory-entities --name <memória> --json` para listar todas as entidades vinculadas a uma memória específica
- USAR `memory-entities --entity <nome-entidade> --json` para listar todas memórias vinculadas a uma entidade (busca reversa, v1.0.58)
- JSON response direta: `memory_name`, `entities: [{entity_id, name, entity_type}]`, `count`, `elapsed_ms`
- JSON response reversa: `entity_name`, `memories: [{memory_id, name, description, memory_type}]`, `count`, `elapsed_ms`
- TRATAR exit code 4 como memória ou entidade não encontrada; exit 0 com count 0 significa que existe mas sem vínculos
### OBRIGATÓRIO — Remover Bindings NER (prune-ner)
- USAR `prune-ner --entity <nome> --json` para remover bindings NER de uma entidade específica
- USAR `prune-ner --all --yes --json` para remover TODOS os bindings NER do namespace
- JSON response: `action`, `bindings_removed`, `elapsed_ms`
- Bindings NER são os vínculos criados automaticamente pela extração GLiNER; links manuais de grafo NÃO são afetados


## Histórico Imutável de Versões
### OBRIGATÓRIO — Inspeção com history
- USAR `history --name <nome> --json` para listar versões
- USAR `history --name <nome> --diff --json` para incluir estatísticas de diff de caracteres entre versões
- VERSÕES começam em 1 e incrementam a cada `edit` ou `restore`
- ORDEM cronológica reversa por padrão
- INCLUI memórias soft-deletadas com flag `deleted: true`
- COM `--diff`, cada versão inclui `changes: {added_chars, removed_chars}` com o diff em relação à versão anterior
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
- FILTRAR arestas fracas com `--min-weight <F>` (padrão 0.3)
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
- Filtro `--relation` aceita qualquer string em kebab-case ou snake_case; valores não canônicos emitem `tracing::warn!` desde v1.0.50
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
- `hybrid-search` resposta agora inclui `fts_degraded` (bool), `fts_error` (string?), `fts_auto_rebuilt` (bool); quando `fts_degraded` é true, apenas resultados vetoriais são retornados
- Campos por resultado do `hybrid-search` também incluem `normalized_score` (score combinado normalizado 0-1), `vec_distance` (float?), `fts_bm25` (float?)
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
- USAR `--strict-relations` para falhar com exit 1 quando um tipo de relação não canônico for usado; resposta inclui campo `warnings` listando relações não canônicas quando não estiver no modo estrito
### OBRIGATÓRIO — Exportação com graph
- EXPORTAR snapshot via `graph --format json`
- USAR `--format dot` para Graphviz offline
- USAR `--format mermaid` para embutir em Markdown
- GRAVAR direto em arquivo via `--output <PATH>`
- INSPECIONAR `nodes` e `edges` no JSON exportado
- EDGES referenciando entidades inexistentes são logadas via `tracing::warn!` e ignoradas desde v1.0.50
### OBRIGATÓRIO — Enumeração de Entidades (graph entities)
- USAR `graph entities --json` para listar todas as entidades
- ACESSAR via `jaq -r '.entities[].name'` (campo é `entities`, NÃO `items`)
- FILTRAR por `--entity-type <tipo>` quando necessário
- PAGINAR com `--limit` e `--offset`
- USAR antes de planejar travessias ou links em lote
- ORDENAR via `--sort-by degree|name|created_at` (padrão `name`)
- DEFINIR direção via `--order asc|desc` (padrão `asc`)
- RESPOSTA agora inclui campo `degree` por entidade (número de relacionamentos conectados)
### OBRIGATÓRIO — Estatísticas (graph stats)
- USAR `graph stats --json` antes de travessias caras
- INSPECIONAR `node_count`, `edge_count`, `avg_degree`, `max_degree`
- ESCOLHER profundidade de travessia baseada em densidade real
- DETECTAR isolamento de subgrafos antes de planejar buscas
### Vocabulário Canônico de Relações
- `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`
- `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- Tipos customizados de relação (ex.: `implements`, `tested-by`, `blocks`) são aceitos desde v1.0.49; valores não canônicos emitem `tracing::warn!`
### Tipos Válidos de Entidade
- `project`, `tool`, `person`, `file`, `concept`, `incident`
- `decision`, `memory`, `dashboard`, `issue_tracker`
- `organization`, `location`, `date`


## Qualidade do Grafo Dirigida por LLM
### OBRIGATÓRIO — Tabela de Mapeamento de Relações
- MAPEAR relações não canônicas para equivalentes canônicos antes de persistir
- `adds` mapeia para `causes` (criação implica causalidade)
- `creates` mapeia para `causes` (mesma lógica)
- `implements` mapeia para `supports` (implementação suporta um design)
- `blocks` mapeia para `contradicts` (bloqueio contradiz progresso)
- `tested-by` mapeia para `related` (teste é uma forma de relação)
- `part-of` mapeia para `applies-to` (parte se aplica ao todo)
- PREFERIR o valor canônico sobre strings customizadas para evitar ruído de `tracing::warn!`
- RELAÇÕES customizadas são aceitas mas canônicas geram melhor recall cross-memory
### OBRIGATÓRIO — Curadoria de Entidades
- EXTRAIR apenas conceitos específicos do domínio: projetos reais, ferramentas, pessoas, decisões, arquivos
- NUNCA criar entidades de stop words, artigos, pronomes ou verbos genéricos
- NUNCA criar entidades de UUIDs, hashes, timestamps ou números de linha
- NUNCA criar entidades de caracteres únicos ou abreviações de duas letras
- ESCOLHER entity_type deliberadamente: `concept` para ideias abstratas, `tool` para software, `decision` para escolhas arquiteturais, `project` para codebases, `person` para contribuidores, `file` para caminhos de fonte
- PREFERIR menos entidades de alta qualidade sobre muitas de baixo sinal
- DEDUPLICAR: buscar `graph entities --json` antes de criar para evitar quase-duplicatas como "auth" e "authentication"
### OBRIGATÓRIO — Curadoria de Relações
- `depends-on`: A não funciona sem B (dependência forte)
- `uses`: A utiliza B mas poderia substituí-lo (dependência suave)
- `supports`: A reforça ou viabiliza B (design sustentando implementação)
- `causes`: A dispara ou produz B (cadeia causal)
- `fixes`: A resolve um problema descrito em B (correção de bug, resolução de incidente)
- `contradicts`: A conflita com ou invalida B (designs concorrentes, bloqueios)
- `applies-to`: A é relevante para ou tem escopo dentro de B (regra se aplica a módulo)
- `follows`: A vem depois de B em sequência ou prioridade (ordenação de workflow)
- `replaces`: A substitui B (migração, depreciação)
- `tracked-in`: A é monitorado ou gerenciado em B (issue em tracker, métrica em dashboard)
- `related`: A e B compartilham contexto mas nenhuma relação mais forte se aplica (usar com parcimônia, nunca como padrão)
- `mentions`: A referencia B sem implicar relacionamento (usar APENAS para citações, nunca como catch-all)
- ATRIBUIR `strength` baseado em acoplamento: 0.9 para dependências fortes, 0.7 para relações de design, 0.5 para links contextuais, 0.3 para referências fracas
### OBRIGATÓRIO — Enrichment de Descrições
- DESCRIÇÕES genéricas como "ingested from docs/README.md" desperdiçam o campo description
- ATUALIZAR via `edit --name <nome> --description "resumo semântico conciso"`
- BOA descrição responde: sobre o que é esta memória e POR QUE ela importa?
- RUIM: "ingested from auth.md" → BOM: "JWT token rotation strategy with 15-min expiry and refresh flow"
- RUIM: "user feedback" → BOM: "user prefers single bundled PR over many small ones for refactors"
- LIMITAR a uma frase, 10-20 palavras, focando no insight único
- EXECUTAR `list --type <tipo> --json | jaq '.items[] | select(.description | test("ingested|imported|added")) | .name'` para encontrar descrições genéricas
- ENRIQUECIMENTO em lote: encaminhar nomes para loop chamando `edit --description` para cada
### OBRIGATÓRIO — Workflow de Melhoria de Qualidade do Grafo
- PASSO 1 — Auditar: `graph stats --json` para medir node_count, edge_count, avg_degree
- PASSO 2 — Identificar ruído: `list --json | jaq '.items[] | select(.description | test("ingested|imported")) | .name'`
- PASSO 3 — Enriquecer descrições: `edit --name <nome> --description "resumo semântico"`
- PASSO 4 — Podar relações de baixo sinal: `prune-relations --relation mentions --dry-run --json`
- PASSO 5 — Executar poda: `prune-relations --relation mentions --yes --json`
- PASSO 6 — Limpar órfãos: `cleanup-orphans --yes --json`
- PASSO 7 — Verificar: `health --json | jaq '.integrity_ok'`
- AGENDAR este workflow após operações `ingest` em massa
### PROIBIDO — Anti-padrões de LLM no Grafo
- NUNCA usar `mentions` como relação padrão; adiciona ruído sem sinal
- NUNCA criar entidades de detalhes de implementação (nomes de variáveis, números de linha, hashes de commit)
- NUNCA definir todos os strengths como 1.0; diferenciar níveis de acoplamento
- NUNCA deixar descrições "ingested from" sem enriquecimento
- NUNCA criar edges redundantes (se A depends-on B, não adicionar também A uses B)
- NUNCA persistir estado efêmero (branch atual, progresso WIP, workarounds temporários)
- NUNCA pular deduplicação; buscar `hybrid-search` ou `graph entities` antes de criar


## Daemon e Latência Reduzida
### OBRIGATÓRIO — Reuso do Modelo de Embeddings
- INICIAR `sqlite-graphrag daemon` em sessões longas de agente
- VERIFICAR saúde via `daemon --ping --json`
- ENCERRAR via `daemon --stop` ao fim da sessão
- DEIXAR `init`, `remember`, `ingest`, `recall`, `hybrid-search` reusarem automaticamente
- TRATAR daemon como opcional para invocações single-shot
- INSPECIONAR contador de embedding requests no `--ping`
- `daemon --ping` avisa quando versão do daemon difere do binário CLI; reiniciar com `daemon --stop` seguido de `daemon` após upgrades
- Desde v1.0.50, a CLI reinicia automaticamente o daemon em caso de incompatibilidade de versão antes do primeiro request de embedding; `daemon --stop` manual após upgrades não é mais necessário
- Resposta de `daemon --ping` agora inclui os campos `model_name` e `model_variant` com o modelo de embedding atualmente carregado


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
### OBRIGATÓRIO — Contrato JSON de Erros (v1.0.56)
- TODOS os caminhos de erro agora emitem um objeto JSON no stdout: `{"error": true, "code": N, "message": "..."}`
- stderr ainda recebe o erro legível por humanos com prefixo descritivo
- CONSUMIDORES devem verificar o JSON do stdout primeiro (procurar `"error": true`), depois usar o exit code como fallback
- Aplica-se a TODOS os comandos quando `--json` é passado; sem `--json`, erros vão apenas para stderr
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
- `list` response-level: `items[]`, `elapsed_ms`; cada item tem `id`, `memory_id`, `name`, `namespace`, `type`, `memory_type`, `description`, `snippet`, `updated_at`, `updated_at_iso`, `deleted_at?`, `deleted_at_iso?`
- `export` por linha: `name`, `type`, `memory_type`, `description`, `body`, `namespace`, `created_at_iso`, `updated_at_iso`, `deleted_at_iso?`; linha summary: `summary` (true), `exported`, `namespace`, `elapsed_ms`
- `health` retorna `integrity_ok`, `schema_ok`, `vec_memories_ok`, `vec_entities_ok`, `vec_chunks_ok`, `fts_ok`, `model_ok`, `counts`, `wal_size_mb`, `journal_mode`, `db_path`, `db_size_bytes`, `checks[]`
- `health.counts` contém: `memories`, `entities`, `relationships`, `vec_memories`
- `health` opcionalmente retorna `mentions_ratio` (float) e `mentions_warning` (string) quando mentions excedem 50% dos relacionamentos
- `health` agora inclui `fts_query_ok` (bool) indicando se uma query FTS5 ao vivo teve sucesso (além da integridade de schema), e `sqlite_version` (string) com a versão do SQLite em uso
- `stats` retorna dados GLOBAIS (sem filtro por namespace): `memories`, `entities`, `relationships`, `chunks_total`, `avg_body_len`, `namespaces[]`, `db_size_bytes`, `schema_version`, `elapsed_ms`; também inclui aliases legados `db_bytes`, `edges`, `memories_total`, `entities_total`, `relationships_total`
- `ingest` por arquivo: `file`, `name`, `status` (`"indexed"`/`"skipped"`/`"failed"`), `truncated`, `original_name?`, `original_filename?`, `memory_id?`, `action?`, `error?`
- `ingest` summary: `summary` (true), `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- `ingest --mode claude-code` phase: `phase` (`"validate"`/`"scan"`), `claude_path?`, `version?`, `dir?`, `files_total?`, `files_new?`, `files_existing?`
- `ingest --mode claude-code` por arquivo: `file`, `name`, `status` (`"done"`/`"failed"`/`"preview"`), `memory_id?`, `entities?`, `rels?`, `cost_usd?`, `elapsed_ms?`, `error?`, `index`, `total`
- `ingest --mode claude-code` summary: `summary` (true), `files_total`, `completed`, `failed`, `skipped`, `entities_total`, `rels_total`, `cost_usd`, `elapsed_ms`
- `cache list` retorna modelos com tamanho em bytes e total de disco
- `prune-relations` retorna `action` (`"pruned"`/`"dry_run"`), `relation`, `count`, `entities_affected`, `affected_entity_names?`, `namespace`, `elapsed_ms`
- `fts rebuild` retorna `action` ("rebuilt"), `rows_indexed`, `elapsed_ms`
- `fts check` retorna `action` ("checked"), `integrity_ok`, `detail?`, `elapsed_ms`
- `fts stats` retorna `total_rows`, `shadow_pages?`, `fts_functional`, `elapsed_ms`
- `backup` retorna `action` ("backed_up"), `source`, `destination`, `size_bytes`, `elapsed_ms`
- `delete-entity` retorna `action` ("deleted"), `entity_name`, `namespace`, `relationships_removed`, `bindings_removed`, `elapsed_ms`
- `reclassify` retorna `action` ("reclassified"), `count`, `description_updated?` (bool, presente quando `--description` aplicado), `namespace`, `elapsed_ms`
- `merge-entities` retorna `action` ("merged"), `sources[]`, `target`, `namespace`, `relationships_moved`, `entities_removed`, `elapsed_ms`
- `memory-entities` forward retorna `memory_name`, `entities[].{entity_id, name, entity_type}`, `count`, `elapsed_ms`
- `memory-entities` reverse (`--entity`) retorna `entity_name`, `memories[].{memory_id, name, description, memory_type}`, `count`, `elapsed_ms`
- `prune-ner` retorna `action` (`"pruned"`/`"dry_run"`/`"aborted"`), `bindings_removed`, `namespace`, `entity?`, `elapsed_ms`
- `link` retorna `action` ("linked"), `from`, `to`, `relation`, `weight`, `namespace`, `elapsed_ms`, `created_entities?` (array, com `--create-missing`), `warnings?` (array, com relação não canônica)
- `unlink` retorna `action` ("deleted"), `from_name`, `to_name`, `relation`, `relationships_removed`, `namespace`, `elapsed_ms`
- `rename-entity` retorna `action` ("renamed"), `old_name`, `new_name`, `entity_id`, `namespace`, `elapsed_ms`


## Códigos de Saída e Estratégia de Retry
### OBRIGATÓRIO — Tratamento Completo de Exit Codes
- `0` igual sucesso, parsear stdout
- `1` igual validação (peso inválido, self-link, max-files excedido)
- `2` igual erro de parsing de argumento Clap (flag inválida, timezone inválido, argumento obrigatório ausente)
- `9` igual duplicata (memória já existe sem `--force-merge`); desde v1.0.51 também retornado quando a memória é soft-deleted — use `--force-merge` para restaurar e atualizar, ou `restore` para reviver
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
- NUNCA confundir exit 1 (validação) com exit 9 (duplicata)


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


## Gerenciamento FTS5 (v1.0.56)
### OBRIGATÓRIO — Comandos FTS5
- USAR `fts rebuild --json` para reconstruir completamente o índice full-text FTS5; response: `{action, rows_indexed, elapsed_ms}`
- USAR `fts check --json` para executar a integrity-check do FTS5; response: `{action, integrity_ok, detail, elapsed_ms}`
- USAR `fts stats --json` para inspecionar a saúde do FTS5; response: `{total_rows, shadow_pages, fts_functional, elapsed_ms}`
- EXECUTAR `fts rebuild` quando `hybrid-search` retornar `fts_degraded: true` ou após suspeita de corrupção do índice
- EXECUTAR `fts check` como parte das auditorias periódicas de saúde junto com `health --json`
- TRATAR `fts_functional: false` no `fts stats` como sinal para executar `fts rebuild`


## Backup Seguro (v1.0.56)
### OBRIGATÓRIO — Comando backup
- USAR `backup --output <caminho> --json` para backup seguro e online via SQLite Online Backup API
- BACKUP é consistente mesmo com escritas em andamento — não é necessário parar o daemon
- JSON response: `{action, source, destination, size_bytes, elapsed_ms}`
- PREFERIR `backup` sobre `sync-safe-copy` para backups programáticos; ambos são seguros, mas `backup` usa a API nativa do SQLite
- TRATAR exit code 14 como erro de I/O (destino não gravável, disco cheio)


## Operações de Entidade (v1.0.56)
### OBRIGATÓRIO — delete-entity
- USAR `delete-entity --name <entidade> --cascade --json` para remover uma entidade e todos seus relacionamentos e bindings de memória
- FLAG `--cascade` é obrigatória como portão de confirmação; sem ela o comando sai com erro de validação
- JSON response: `{action, entity_name, namespace, relationships_removed, bindings_removed, elapsed_ms}`
- EXECUTAR `cleanup-orphans` depois para remover entidades recém-órfãs
- TRATAR exit code 4 como entidade não encontrada
### OBRIGATÓRIO — rename-entity (v1.0.58)
- USAR `rename-entity --name <antigo> --new-name <novo> --json` para renomear entidade preservando todos os relacionamentos e vínculos
- RE-GERA o vetor da entidade com o novo nome para precisão na busca semântica
- JSON response: `{action: "renamed", old_name, new_name, entity_id, namespace, elapsed_ms}`
- TRATAR exit code 4 como entidade não encontrada; exit 1 se novo nome já existe ou falha na validação (menor que 2 caracteres, contém quebras de linha, ou abreviação ALL_CAPS curta)
- TODOS os relacionamentos e memory_entities usam FK inteiro e não são afetados pela mudança de nome
### OBRIGATÓRIO — reclassify
- USAR `reclassify --name <entidade> --new-type <tipo> --json` para alteração individual de tipo de entidade
- USAR `reclassify --from-type <antigo> --to-type <novo> --batch --json` para reclassificação em massa
- USAR `reclassify --name <entidade> --description "texto" --json` para atualizar descrição da entidade no modo individual (v1.0.58)
- COMBINAR `--new-type` com `--description` para alterar tipo e descrição em uma operação
- JSON response: `{action, count, description_updated?, namespace, elapsed_ms}`
- TRATAR count 0 no modo batch como indicação de que --from-type pode conter erro de digitação
### OBRIGATÓRIO — merge-entities
- USAR `merge-entities --names "a,b" --into <alvo> --json` para fundir entidades de origem em um alvo
- TODOS os relacionamentos dos nós de origem são redirecionados para o alvo via UPDATE OR IGNORE
- RELACIONAMENTOS duplicados são removidos automaticamente após redirecionamento
- JSON response: `{action, sources, target, namespace, relationships_moved, entities_removed, elapsed_ms}`
- TRATAR exit code 4 como entidade alvo não encontrada
### OBRIGATÓRIO — memory-entities
- USAR `memory-entities --name <memória> --json` para listar todas entidades vinculadas a uma memória específica
- USAR `memory-entities --entity <nome-entidade> --json` para listar todas memórias vinculadas a uma entidade (busca reversa, v1.0.58)
- RESPOSTA direta: `{memory_name, entities: [{entity_id, name, entity_type}], count, elapsed_ms}`
- RESPOSTA reversa: `{entity_name, memories: [{memory_id, name, description, memory_type}], count, elapsed_ms}`
- TRATAR exit code 4 como memória/entidade não encontrada; exit 0 com count 0 significa que existe mas sem vínculos
- USAR busca reversa antes de rename-entity ou delete-entity para avaliação de impacto
### OBRIGATÓRIO — prune-ner
- USAR `prune-ner --entity <nome> --dry-run --json` para pré-visualizar remoção de bindings NER
- USAR `prune-ner --entity <nome> --yes --json` para remover bindings NER de uma única entidade
- USAR `prune-ner --all --yes --json` para remover TODOS os bindings NER no namespace
- JSON response: `{action, bindings_removed, namespace, entity, elapsed_ms}`
- EXECUTAR `cleanup-orphans` depois para remover nós de entidade sem bindings restantes


## Manutenção e Backup
### OBRIGATÓRIO — Higiene Periódica
- AGENDAR `purge --retention-days 30 --yes` semanalmente
- EXECUTAR `vacuum` após purges grandes
- RODAR `optimize` para refrescar estatísticas do planner
- LIMPAR órfãos via `cleanup-orphans --yes` após forget em massa
### OBRIGATÓRIO — Backup Seguro
- DESDE v1.0.53, todo comando de escrita executa `PRAGMA wal_checkpoint(TRUNCATE)` após commit, garantindo que o arquivo `.sqlite` esteja sempre autocontido quando ferramentas de cloud sync (Dropbox, iCloud, OneDrive) o leem
- USAR `sync-safe-copy --dest <path>` para snapshots atômicos antes de operações críticas
- COMPRIMIR snapshots via `ouch compress` para upload remoto
- EXPORTAR memórias via `sqlite-graphrag export` como NDJSON (uma linha JSON por memória + summary); suporta `--namespace`, `--type`, `--include-deleted`, `--limit`
- VERSIONAR banco com Git LFS quando viável
- SE ocorrer corrupção apesar do checkpoint, recuperar com `sqlite3 corrompido.sqlite ".recover" | sqlite3 reparado.sqlite`
### OBRIGATÓRIO — Diagnóstico de Schema
- USAR `__debug_schema --json` para troubleshooting
- INSPECIONAR `schema_version`, `objects`, `migrations`
- VERSÃO atual do schema é 11 (V011 adiciona índice `idx_relationships_ns_relation`)
- COMANDO oculto do `--help`, invocar pelo nome exato
### Padrão Correto — Cron Semanal
- `sqlite-graphrag purge --retention-days 30 --yes`
- `sqlite-graphrag cleanup-orphans --yes`
- `sqlite-graphrag prune-relations --relation mentions --yes` (quando edges geradas por NER precisam de limpeza)
- `sqlite-graphrag vacuum --json`
- `sqlite-graphrag optimize --json`
- `sqlite-graphrag sync-safe-copy --dest ~/Dropbox/graphrag.sqlite`
