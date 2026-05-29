# gaps.md — Auditoria sqlite-graphrag v1.0.65

- Data: 2026-05-29
- Binario: ~/.cargo/bin/sqlite-graphrag v1.0.65
- Testes executados: ~165 (13 categorias)
- Resultado global: 6 BUGs (inc. BUG-06 link weight), 16 achados HIGH (inc. 1 CRITICAL promovido), 11 achados MEDIUM (inc. 1 informativo nao-reproduzido), 5 melhorias LOW = 35 gaps totais


## BUG-01 CRITICAL — reclassify-relation crash: no such column updated_at

- Comando: `sqlite-graphrag reclassify-relation --from-relation mentions --to-relation related --batch --json`
- Erro: `database error: no such column: updated_at` (exit 10)
- Causa raiz: `src/commands/reclassify_relation.rs` linhas 194, 304, 314 usam SQL `SET relation = ?1, updated_at = unixepoch()` mas a tabela `relationships` (migrations/V010) NAO tem coluna `updated_at`
- Impacto: TODA execucao nao-dry-run do reclassify-relation falha; comando novo v1.0.65 nao funcional em producao
- Fix: remover `updated_at = unixepoch()` dos 3 SQL UPDATE statements, ou adicionar coluna via V012 migration
- Testes bloqueados: RC02, RC06 (collision merge)


## BUG-02 HIGH — link --create-missing NAO normaliza nomes de entidades

- Comando: `sqlite-graphrag link --from "Mixed Case Name" --to "Another Name" --relation uses --create-missing --json`
- Resultado: `created_entities: ["Mixed Case Name", "Another Name"]` — nomes NAO normalizados
- Esperado (GAP-15): deveria criar entidades como "mixed-case-name" e "another-name"
- Causa raiz: o path `link --create-missing` provavelmente nao chama `normalize_entity_name()` antes de criar as entidades
- Impacto: entidades com mixed-case sao criadas via link, quebrando a garantia de GAP-15 (normalizacao em todos os paths de escrita)
- Arquivos: `src/commands/link.rs` — verificar se `normalize_entity_name()` e chamado antes de `insert_entity()`


## BUG-03 MEDIUM — body limit off-by-one: 512000 bytes rejeitado

- Comando: `python3 -c "print('x' * 512000)" | sqlite-graphrag remember --name test --type note --description "d" --body-stdin --json`
- Erro: `limite excedido: corpo excede 512000 bytes` (exit 6)
- Esperado: body de EXATAMENTE 512000 bytes deveria ser aceito (MAX_MEMORY_BODY_LEN = 512_000 em constants.rs)
- Causa provavel: python3 `print()` adiciona `\n` gerando 512001 bytes; OU validacao usa `>` em vez de `>=`
- Impacto: BAIXO se causado por newline extra; investigar com `printf` sem newline
- Confianca: MEDIA — requer investigacao de `body.len() > MAX_MEMORY_BODY_LEN` vs `>=`


## BUG-04 MEDIUM — deep-research NAO decompoe queries curtas

- Comando: `sqlite-graphrag deep-research "authentication JWT tokens" --k 10 --max-sub-queries 2 --json`
- Resultado: sub_queries=1 (source="original"), results=1
- Esperado: query com 2+ conceitos deveria gerar >=2 sub_queries
- Causa raiz: heuristica de decomposicao em `src/commands/deep_research.rs` pode exigir queries mais longas ou multiplos conceitos separados por conjuncao
- Impacto: queries curtas nao se beneficiam da decomposicao multi-hop; deep-research degenera para recall simples
- Nota: queries longas ("authentication and database migration") geram decomposicao correta (sub_queries=2)


## BUG-05 LOW — abort em body com bytes invalidos UTF-8

- Comando: `sqlite-graphrag remember --name test --body "$(printf '\xff\xfe')" --json`
- Resultado: `Abortado (imagem do nucleo gravada)` — crash do processo com core dump
- Causa raiz: bytes invalidos UTF-8 no body causam panic/abort em algum ponto da pipeline (serde, embedding, ou SQLite binding)
- Impacto: baixo em uso normal (bodies sao texto); ALTO se binario recebe input de fontes nao confiaveis
- Fix: validar UTF-8 no body antes de processar, retornar exit 1 (Validation) em vez de abort


## HIGH-01 CRITICAL — deep-research evidence chains SEMPRE vazias (seed entity flooding)

### Problema

O deep-research retorna `evidence_chains_found: 0` em TODAS as queries testadas, independente do tamanho do grafo ou da conectividade entre entidades. As evidence chains sao a feature principal que diferencia deep-research do recall/hybrid-search simples — sem elas, o comando degenera para um recall com overhead.

### Consequencias

- Evidence chains nunca retornam dados uteis, tornando o campo uma promessa vazia no JSON
- LLM agents nao recebem cadeias de raciocinio multi-hop, degradando qualidade de analise
- O campo `evidence_chains: []` no output induz o LLM a concluir "sem conexoes no grafo" quando na verdade o grafo esta rico
- `graph traverse --from X --depth 3` retorna centenas de hops corretamente no mesmo banco, provando que o grafo funciona
- Feature documentada como fix de GAP-09 (v1.0.65) mas inoperante em producao

### Causa Raiz (verificada empiricamente)

Localizacao: `deep_research.rs:690-703` (seed collection) + BFS filter

O algoritmo de evidence chains usa BFS a partir de "seed entities" para descobrir caminhos. O problema e que os seeds sao TODAS as entidades de TODAS as memorias retornadas pelo KNN:

1. hybrid-search com k=20 retorna TODAS as memorias (em bases pequenas/medias com <100 memorias)
2. memory_entities dessas memorias cobre 100% das entidades do namespace
3. `seed_entity_ids` apos dedup = TODAS as entidades (ex: 169/169 em banco de 11 memorias)
4. BFS inicializa `entity_depth` com todos os 169 seeds
5. O filtro `.filter(!entity_depth.contains_key(id))` filtra TODOS os vizinhos porque ja sao seeds
6. `predecessor` map = vazio → zero evidence chains

Em resumo: quando k >= numero de memorias, o BFS parte de TODOS os nos do grafo e nao tem para onde expandir.

### Solucao

Limitar seed entities a top-k entity KNN (5-10) em vez de incluir entidades de TODAS as memorias retornadas. A expansao por grafo deve DESCOBRIR nos novos, nao partir de todos.

```rust
// ANTES (bugado): seeds = entidades de TODAS as memorias retornadas
let seed_memory_ids: Vec<i64> = fused_results.iter().map(|r| r.memory_id).collect();
// seed_entity_ids pode ser 100% do grafo

// DEPOIS (fix): seeds = entidades das top-5 memorias apenas
let top_seed_count = 5.min(fused_results.len());
let seed_memory_ids: Vec<i64> = fused_results[..top_seed_count].iter().map(|r| r.memory_id).collect();
// seed_entity_ids = subset pequeno, BFS tem espaco para expandir
```

### Beneficios

- Evidence chains passam a funcionar em bases de qualquer tamanho
- BFS encontra caminhos dirigidos seed→target com predecessor map populado
- Qualidade do deep-research sobe drasticamente: memorias + cadeias de raciocinio
- Fix de ~5 linhas, complexidade baixa, sem breaking changes no JSON output

### Como Solucionar

1. Em `deep_research.rs:690-703`, limitar `seed_memory_ids` aos top-5 por score
2. Adicionar `tracing::debug!` logando contagem de seeds vs total entities para diagnostico
3. Escrever teste com banco de 10+ memorias verificando chains > 0
4. Verificar com `graph traverse` que os mesmos paths sao cobertos


## HIGH-01b — deep-research resultados vazios com memorias relevantes

### Problema

Alem do seed flooding (HIGH-01), o deep-research pode retornar `results: []` mesmo com memorias relevantes no banco. Observado com 2 memorias seedadas ("JWT auth" + "PostgreSQL migration") e query "authentication and database migration" → sub_queries=2, results=0.

### Causa Raiz

O RRF fusion com `--graph-min-score 0.2` (default) filtra resultados cujo score combinado fica abaixo do threshold. Em bases pequenas, os scores podem ficar baixos por:
1. Embeddings pouco distintos para memorias com vocabulario distante
2. FTS5 BM25 penaliza documentos curtos (bodies de 1 frase)
3. Graph pool vazio (consequencia do HIGH-01 seed flooding) remove boost de grafo

### Solucao

- Reduzir `--graph-min-score` default de 0.2 para 0.05 para bases pequenas
- OU: adicionar fallback que retorna top-k por score absoluto quando RRF fusion retorna vazio
- OU: emitir `tracing::warn!` quando fusion retorna 0 resultados apesar de KNN/FTS terem encontrado candidatos


## HIGH-02 — enrich dry-run deixa processos residuais

- Teste EA07: apos enrich --dry-run, `procs sqlite-graphrag` mostra 6 processos
- Esperado: --dry-run NAO deveria deixar processos residuais
- Impacto: processos daemon podem acumular em sessoes de teste longas
- Nota: pode ser daemon auto-spawned legitimamente; verificar se processos sao daemon instances vs orfaos reais


## HIGH-03 — debug-schema subcomando com nome oculto inconsistente

- Comando documentado no CLAUDE.md: `debug-schema`
- Comando real no binario: `__debug_schema` (double underscore prefix)
- Erro ao usar nome documentado: `unrecognized subcommand 'debug-schema'` (exit 2)
- Impacto: documentacao (CLAUDE.md, SKILL.md, llms.txt) referencia nome errado
- Fix: atualizar documentacao para `__debug_schema` ou renomear o subcomando para `debug-schema`


## HIGH-04 — max-entity-degree warning NAO emitido

- Comando: `sqlite-graphrag link --from X --to Y --relation related --create-missing --max-entity-degree 5 --json` (com entidade X tendo >5 relacoes)
- Resultado: nenhum warning emitido no stderr
- Esperado: `tracing::warn!` quando degree excede o cap (GAP-17)
- Impacto: usuarios nao sao alertados sobre super-hub growth no grafo
- Investigar: flag aceita pela CLI mas logica de warning pode ter condicao errada ou log level insuficiente para stderr


## HIGH-05 — Cascata de falhas em LLM agents: nomenclatura inconsistente + envelope JSON opaco

### Problema

LLM agents (Claude Code, Codex, etc.) chamando a CLI sqlite-graphrag via subprocesso sofrem falhas silenciosas (jaq exit 5) ao tentar parsear output JSON. Os agentes escrevem expressoes jaq que parecem corretas mas falham em producao.

Comandos que falharam em sessao real:
- `sqlite-graphrag graph --format json | jaq '.entities[:30]'` → exit 5 (campo real: `.nodes`)
- `sqlite-graphrag list --json | jaq '[.[] | {id: .memory_id, name, type}]'` → exit 5 (campo real: `.items[]`)
- `sqlite-graphrag list --json | jaq -c '.[] | {id: .memory_id, name, type}'` → exit 5 (idem)
- `sqlite-graphrag list --json | jaq '.memories[] | select(.name | test("2026-05-28")) | .name'` → exit 5: `cannot use null as iterable` (campo `.memories` nao existe; campo real: `.items[]`)

### Consequencias

- LLM agents ficam incapacitados de ler o grafo de entidades, quebrando pipelines de introspeccao
- LLM agents nao conseguem listar memorias, quebrando pipelines de auditoria
- Cada falha gera retry + tokens gastos sem sucesso (3-5 tentativas por comando = 15+ chamadas desperdicadas por sessao)
- jaq exit 5 nao produz mensagem de erro util — o agente nao sabe o que corrigir
- O agente cancela chamadas paralelas na cascata, amplificando perda de contexto
- CLAUDE.md documenta a estrutura correta em ALGUNS lugares mas a inconsistencia entre subcomandos confunde o agente

### Causa Raiz (3 facetas interligadas)

**Faceta 1 — Nomenclatura inconsistente entre subcomandos graph:**

| Subcomando | Campo real | Campo que LLM espera | Resultado |
|---|---|---|---|
| `graph --format json` | `.nodes` | `.entities` | `.entities` = null → jaq exit 5 |
| `graph entities --json` | `.entities` | `.entities` | OK ✓ |
| `graph stats --json` | `.node_count` | `.node_count` | OK ✓ |
| `graph traverse --json` | `.hops` | `.hops` | OK ✓ |

O LLM ve `.entities` no comando `graph entities` e extrapola para `graph --format json`. O schema `graph.schema.json` usa `nodes` corretamente, mas LLMs nao leem schemas — leem documentacao textual e exemplos.

**Faceta 2 — Envelope JSON opaco em list:**

`list --json` retorna `{"items": [...], "total_count": N, "truncated": bool, "elapsed_ms": N}`, NAO um array raiz. O LLM escreve `.[]` (iteracao de array) em vez de `.items[]` (acesso ao campo). Em jaq, `.[]` sobre um objeto itera os VALUES (o array items, o numero total_count, etc.), nao as chaves — causando type error quando tenta acessar `.memory_id` em um numero.

**Faceta 3 — jaq exit 5 silencioso:**

Quando jaq encontra null (campo inexistente) ou type mismatch, retorna exit 5 SEM mensagem de erro no stderr (quando stderr e redirecionado com 2>/dev/null). O LLM ve "exit 5" e nao sabe se e problema do sqlite-graphrag (exit 5 = NamespaceError) ou do jaq. Esta ambiguidade de exit code entre as duas ferramentas amplifica a confusao.

**Faceta 4 — Nomes genericos vs domain-specific (padrao sistematico):**

LLMs consistentemente escolhem nomes de campo DOMAIN-SPECIFIC sobre genericos. O mapa completo de inconsistencias:

| Comando | Campo real | Campo que LLM espera | Domain-specific? |
|---|---|---|---|
| `list --json` | `.items[]` | `.memories[]` | NAO — "items" e generico |
| `graph --format json` | `.nodes[]` | `.entities[]` | NAO — "nodes" e grafo-generico |
| `graph entities --json` | `.entities[]` | `.entities[]` | SIM ✓ |
| `recall --json` | `.results[]` | `.results[]` | SIM ✓ |
| `hybrid-search --json` | `.results[]` | `.results[]` | SIM ✓ |
| `history --json` | `.versions[]` | `.versions[]` | SIM ✓ |

Padrao: comandos que usam nomes domain-specific (entities, results, versions) NUNCA causam erros de LLM. Comandos que usam nomes genericos (items, nodes) SEMPRE causam erros.

O caso `.memories[]` e especialmente revelador: o LLM sabe que esta listando memorias, entao escreve `.memories[]` — nome perfeitamente logico que simplesmente nao existe no schema.

### Solucao

**S1 — Aliases domain-specific em TODOS os comandos afetados (RECOMENDADA, retrocompativel):**

Para `list --json`:
- Adicionar campo `.memories` como alias de `.items`
- JSON tera AMBOS: `{"items": [...], "memories": [...], "total_count": N}`
- Arquivo: `src/commands/list.rs` — adicionar `memories: self.items.clone()` no Serialize

Para `graph --format json`:
- Adicionar campo `.entities` como alias de `.nodes`
- JSON tera AMBOS: `{"nodes": [...], "entities": [...], "edges": [...]}`
- Arquivo: `src/commands/graph_export.rs` — adicionar `entities: self.nodes.clone()` no serialize

**S2 — Documentacao com exemplos jaq EXATOS (COMPLEMENTAR):**
- Adicionar no CLAUDE.md uma tabela de "jaq one-liners por comando" com COPY-PASTE exato
- Exemplo: `graph --format json | jaq '.nodes[:30]'` (nao `.entities`)
- Exemplo: `list --json | jaq '.items[] | {name}'` (nao `.[]`)
- Colocar proximo a cada secao de comando para maximizar probabilidade do LLM encontrar

**S3 — Normalizacao de resposta: array wrapper (ALTERNATIVA para list):**
- Adicionar flag `--array` ou `--flat` que retorna o array raiz diretamente em vez do envelope
- `list --json --flat` retornaria `[{...}, {...}]` em vez de `{"items": [...]}`
- Trade-off: perde metadata (total_count, truncated) — inaceitavel para pipelines completos
- RECOMENDACAO: nao implementar; S2 resolve melhor

### Beneficios da Solucao

- S1: LLM agents que escrevem `.entities` funcionam imediatamente sem mudanca de prompt
- S2: reduz tentativas falhadas de 3-5 para 0-1 por sessao
- Estimativa de economia: 10-15 tokens/chamada × 5 retries × 3 sessoes/dia = 150-225 chamadas/dia eliminadas
- Compatibilidade: S1 nao quebra clientes existentes que usam `.nodes`
- Auditabilidade: tabela de one-liners no CLAUDE.md serve como teste de regressao informal

### Como Solucionar

1. Em `src/commands/graph_export.rs`, na struct `GraphSnapshot` (ou equivalente):
   - Adicionar `#[serde(alias = "entities")]` no campo `nodes` OU
   - Adicionar campo `entities` computed como clone de `nodes` no `Serialize` impl
2. Atualizar `docs/schemas/graph.schema.json` para incluir `entities` como campo opcional
3. Adicionar secao "jaq Quick Reference" no CLAUDE.md com one-liners testados:
   - `graph --format json | jaq '.nodes[:N]'` ou `.entities[:N]`
   - `graph entities --json | jaq '.entities[].name'`
   - `list --json | jaq '.items[] | {name, type: .memory_type}'`
   - `recall "query" --json | jaq '.results[] | {name, score}'`
   - `hybrid-search "query" --json | jaq '.results[] | {name, combined_score}'`
4. Adicionar teste de contrato validando que `graph --format json` aceita AMBOS `.nodes` e `.entities`
5. Considerar emitir `jaq` hint no stderr quando graph/list output e piped: `tracing::debug!("hint: use .nodes for graph export, .items for list")`


## HIGH-06 — deep-research: decomposicao sem LLM gera sub-queries de baixa qualidade

### Problema

A funcao `decompose_query()` (`deep_research.rs:464-542`) usa split puramente textual (por `;`, `,`, `and`, `e`) que:
- Query "danilo" → 1 sub-query (sem decomposicao, perde facetas tematicas)
- Query "riscos financeiros e regulatorios do danilo" → ["riscos financeiros", "regulatorios do danilo"] (quebra frase coerente, perde contexto)
- Query "como autocuidado impacta produtividade" → 1 sub-query (sem conjuncao = sem split)

### Consequencias

- Queries curtas e semanticamente ricas nao se beneficiam da decomposicao multi-hop
- deep-research degenera para recall simples em ~60% dos casos de uso reais
- Sub-queries geradas perdem contexto entre si ("regulatorios" perde "riscos")
- A feature principal do comando (multi-hop parallel research) fica inacessivel sem conjuncoes textuais

### Causa Raiz

A decomposicao e puramente lexica — split por delimitadores textuais sem compreensao semantica. Nao ha acesso ao grafo de entidades nem ao contexto do dominio durante a decomposicao.

### Solucao

Adicionar `--mode claude-code` e `--mode codex` ao deep-research, seguindo o padrao ja estabelecido no `ingest`:

```
sqlite-graphrag deep-research "query" --mode none        # padrao atual (heuristico)
sqlite-graphrag deep-research "query" --mode claude-code  # decomposicao + sintese via Claude
sqlite-graphrag deep-research "query" --mode codex        # decomposicao + sintese via Codex
```

Pipeline com LLM em 3 fases:
- FASE 1 DECOMPOSICAO (LLM): query + contexto do grafo (entidades + descriptions) → sub-queries semanticas otimizadas
- FASE 2 RETRIEVAL (local, identico ao atual): KNN + FTS + BFS por sub-query
- FASE 3 SINTESE (LLM): query + resultados + evidence chains + bodies → relatorio estruturado com findings, gaps, connections

### Beneficios

- Queries curtas ("danilo") geram 4-5 sub-queries tematicas em vez de 1
- Sub-queries usam terminologia do dominio (extraida das entidades do grafo)
- Sintese final produz relatorio estruturado com claims + confidence + gaps
- Reutiliza ~70% do codigo de `ingest_claude.rs` e `ingest_codex.rs`
- Flags CLI seguem padrao existente: `--claude-binary`, `--claude-model`, `--codex-binary`, `--codex-model`, `--llm-timeout`

### Como Solucionar

1. Adicionar `DeepResearchMode` enum (None, ClaudeCode, Codex) em `deep_research.rs`
2. Dispatch em `run()`: `None → run_heuristic()`, `ClaudeCode/Codex → run_with_llm()`
3. Reutilizar `find_claude_binary()`, `validate_claude_version()`, padrao `env_clear()` de `ingest_claude.rs`
4. DECOMPOSITION_SCHEMA com `sub_queries[].{text, rationale, expected_coverage}`
5. SYNTHESIS_SCHEMA com `{summary, findings[].{claim, evidence, confidence, source_memories}, gaps[], connections[]}`
6. Flags: `--mode`, `--claude-binary`, `--claude-model`, `--codex-binary`, `--codex-model`, `--llm-timeout`, `--skip-synthesis`
7. Considerar extrair `src/llm_runner.rs` compartilhado (3o consumidor: ingest + enrich + deep-research justifica DRY)
8. Complexidade: ~400 linhas novo, 70% reutilizado


## HIGH-07 — deep-research: logging insuficiente (-vvv nao emite diagnostico de sub-queries)

### Problema

Com `-vvv` (trace level), apenas 2 linhas INFO sao emitidas durante deep-research:
```
INFO Computing per-sub-query embeddings...
INFO Heavy command detected; available memory: 27847 MB
```

Nenhum diagnostico sobre: KNN count por sub-query, FTS count, RRF fusion count, seed entities count, BFS depth, predecessor map size, evidence chains encontradas.

### Consequencias

- Impossivel diagnosticar por que evidence chains sao vazias (HIGH-01) sem ler codigo fonte
- Usuarios avancados nao conseguem tunear `--graph-decay`, `--graph-min-score`, `--max-neighbors-per-hop` sem feedback
- Bug reports ficam incompletos — stderr vazio alem das 2 linhas INFO

### Causa Raiz

As funcoes `execute_sub_query()` e o BFS rodam dentro de `tokio::spawn`, e simplesmente nao ha instrucoes `tracing::debug!` no codigo desses hot paths.

### Solucao

Adicionar `tracing::debug!` em pontos chave de cada sub-query:

```rust
tracing::debug!(sub_query_id, text = %query_text, "starting sub-query");
tracing::debug!(sub_query_id, knn_count = knn_ids.len(), "KNN search complete");
tracing::debug!(sub_query_id, fts_count = fts_ids.len(), "FTS search complete");
tracing::debug!(sub_query_id, fused_count = fused.len(), "RRF fusion complete");
tracing::debug!(sub_query_id, seed_entities = seed_entity_ids.len(), "seed entity collection");
tracing::debug!(sub_query_id, bfs_depth = entity_depth.len(), predecessors = predecessor.len(), "BFS complete");
tracing::debug!(sub_query_id, chains_found = chains.len(), "evidence chains built");
```

### Beneficios

- Diagnostico de HIGH-01 (seed flooding) seria trivial com um `debug!` mostrando `seed_entities=169 total_entities=169`
- Usuarios podem tunear parametros com feedback em tempo real
- Bug reports incluem dados quantitativos uteis

### Como Solucionar

1. Adicionar ~15 linhas de `tracing::debug!` em `execute_sub_query()` e funcoes BFS
2. Complexidade: baixa (15 linhas, sem mudanca de logica)


## HIGH-08 — deep-research: source sempre "knn" em bases pequenas/medias

### Problema

O campo `results[].source` e classificado como "knn" para TODAS as memorias quando KNN com k=20 retorna todas (bases < 20 memorias):

```rust
let source = if knn_distance_map.contains_key(memory_id) {
    "knn"
} else {
    "fts"
};
```

Como `knn_distance_map` contem todas as memorias, nenhuma e classificada como "fts" mesmo que FTS tambem tenha encontrado. Memorias do graph traversal sao adicionadas depois como "graph", mas `seen_ids` ja contem todas.

### Consequencias

- Campo `source` perde valor informativo — sempre "knn"
- LLM agents nao sabem se o match foi por semantica (KNN), texto (FTS) ou grafo
- Impossivel avaliar qualidade do RRF fusion sem fonte por resultado

### Solucao

Classificar source como:
- `"hybrid"` quando AMBAS KNN e FTS retornaram a memoria
- `"knn"` quando apenas KNN retornou
- `"fts"` quando apenas FTS retornou
- `"graph"` quando apenas grafo retornou

```rust
let in_knn = knn_distance_map.contains_key(&memory_id);
let in_fts = fts_ids.contains(&memory_id);
let source = match (in_knn, in_fts) {
    (true, true) => "hybrid",
    (true, false) => "knn",
    (false, true) => "fts",
    (false, false) => "graph",
};
```

### Como Solucionar

1. Alterar classificacao em `deep_research.rs:660-664` (~10 linhas)
2. Atualizar schema `deep-research.schema.json` para incluir "hybrid" no enum de source
3. Complexidade: baixa


## HIGH-09 — deep-research (futuro --mode claude-code/codex) sem trilhos de seguranca de custo

### Problema

O deep-research ATUAL (v1.0.65, `--mode none` heuristico) nao tem custo LLM — e 100% local. Porem, o HIGH-06 propoe adicionar `--mode claude-code` e `--mode codex` que spawnariam 2 invocacoes LLM por execucao (decomposicao + sintese). Sem trilhos de seguranca de custo, um agente LLM em loop poderia executar deep-research --mode claude-code repetidamente e acumular custos descontrolados.

Os comandos `ingest --mode claude-code` e `enrich --mode claude-code` JA implementam `--max-cost-usd` como trilho de seguranca — mas:
1. Esse flag NAO existe no deep-research atual
2. O design proposto no HIGH-06 nao menciona controle de custo
3. Nao existe flag global de budget cross-comando (cada comando controla isoladamente)
4. Nao existe mecanismo de alerta ANTES de gastar (apenas DEPOIS de exceder o budget)

### Consequencias

**Sem trilhos de custo no futuro --mode claude-code:**
- Agente LLM em loop de deep-research pode acumular US$ 50+ em minutos sem perceber
- Usuarios OAuth (assinatura Pro/Max) nao sao cobrados por chamada de API, mas consomem turns do plano — sem feedback de consumo, podem esgotar turns mensais
- Usuarios com API key pagam por token — sem --max-cost-usd, nao ha freio automatico
- O padrao existente em ingest/enrich nao se propaga automaticamente para novos comandos

**Inconsistencia entre comandos:**
- `ingest --mode claude-code --max-cost-usd 10.00` → funciona
- `enrich --mode claude-code --max-cost-usd 10.00` → funciona
- `deep-research --mode claude-code --max-cost-usd 10.00` → flag nao existe (proposta)
- LLM agents que aprendem o padrao de um comando assumem que funciona em todos

**Sem alerta pre-execucao:**
- O --max-cost-usd ABORTA apos exceder o budget, nao ANTES
- Nao ha estimativa de custo pre-execucao ("this will cost approximately $X")
- Nao ha prompt de confirmacao interativo para custos acima de threshold

### Causa Raiz

Tres facetas:

**Faceta 1 — Design incremental sem padrao transversal:**
Os comandos ingest, enrich e deep-research foram implementados incrementalmente (v1.0.60, v1.0.65, proposta). Cada um reimplementa cost tracking independentemente. Nao existe modulo compartilhado `cost_tracker.rs` nem trait `CostBudget` que force consistencia.

**Faceta 2 — deep-research e local-only hoje:**
Como o deep-research v1.0.65 e 100% local (zero custo LLM), nao houve necessidade de --max-cost-usd. Mas ao adicionar --mode claude-code (HIGH-06), o custo sobe de $0 para ~$0.01-0.05 por execucao (decomposicao + sintese com modelo fast).

**Faceta 3 — Ausencia de budget cross-sessao:**
Cada invocacao do CLI e independente — nao existe tracking de custo acumulado entre invocacoes. O agente LLM pode chamar deep-research 100x e cada invocacao ve budget=0 acumulado.

### Solucao

**S1 — --max-cost-usd no deep-research (OBRIGATORIO para HIGH-06):**

Quando HIGH-06 for implementado (--mode claude-code/codex), DEVE incluir:
```
#[arg(long, value_name = "USD")]
pub max_cost_usd: Option<f64>,
```

Logica identica ao padrao existente em `ingest_claude.rs:1205-1217` e `enrich.rs:1160-1164`:
- Check budget ANTES de cada invocacao LLM
- Se OAuth detectado: log + ignorar (nao ha custo por chamada)
- Se API key: abortar se `cost_total >= budget`
- Reportar `cost_usd` no JSON de stats

**S2 — Estimativa de custo pre-execucao (RECOMENDADA):**

Antes de spawnar o LLM, emitir:
```rust
tracing::info!(
    target: "deep-research",
    estimated_cost_usd = estimated,
    budget_remaining_usd = budget.map(|b| b - cost_total),
    "LLM phase starting"
);
```

Estimativa baseada em:
- Decomposicao: ~500 tokens input + ~200 tokens output ≈ $0.002 (Sonnet)
- Sintese: ~2000 tokens input + ~500 tokens output ≈ $0.008 (Sonnet)
- Total estimado por execucao: ~$0.01

**S3 — Budget tracker cross-comando compartilhado (FUTURA):**

Extrair para `src/cost_tracker.rs`:
```rust
pub struct CostTracker {
    budget: Option<f64>,
    spent: f64,
    oauth_detected: bool,
}

impl CostTracker {
    pub fn check_budget(&self) -> Result<(), AppError> { ... }
    pub fn record_cost(&mut self, cost: f64) { ... }
    pub fn remaining(&self) -> Option<f64> { ... }
}
```

Reutilizado por ingest, enrich e deep-research — elimina duplicacao de logica de budget entre 3 comandos.

**S4 — Budget persistente cross-sessao (AVANCADA, nao urgente):**

Armazenar custo acumulado em tabela SQLite `cost_log`:
```sql
CREATE TABLE cost_log (
    id INTEGER PRIMARY KEY,
    command TEXT NOT NULL,
    cost_usd REAL NOT NULL,
    model TEXT,
    created_at INTEGER DEFAULT (unixepoch())
);
```

Permitiria:
- `sqlite-graphrag stats --json` incluir `total_cost_usd_30d`
- Budget cross-sessao: `--max-cost-usd-daily 5.00` abortaria quando soma do dia exceder
- Alertas: "you have spent $4.50 today across 23 invocations"

### Beneficios

- S1: paridade de seguranca com ingest/enrich — LLM agents podem confiar que --max-cost-usd funciona em todos os comandos com LLM
- S2: usuarios veem custo ANTES de gastar, podem cancelar se muito caro
- S3: elimina 3 implementacoes duplicadas de budget check, previne inconsistencias futuras
- S4: visibilidade de custo acumulado para gestao financeira de agentes autonomos

### Como Solucionar

**Para v1.0.66 (junto com HIGH-06):**
1. Adicionar `max_cost_usd: Option<f64>` ao `DeepResearchArgs` struct
2. Adicionar check de budget ANTES de cada `call_claude()` / `call_codex()` (copiar padrao de enrich.rs:1160-1164)
3. Reportar `cost_usd` no `stats` do JSON output
4. Emitir `tracing::info!` com estimativa pre-execucao
5. Complexidade: baixa (~20 linhas, padrao ja existe)

**Para v1.1.0 (melhoria):**
6. Extrair `CostTracker` para `src/cost_tracker.rs`
7. Refatorar ingest_claude.rs, enrich.rs e deep_research.rs para usar CostTracker
8. Complexidade: media (~100 linhas de refatoracao)

**Para v2.0.0 (avancada):**
9. Tabela `cost_log` no SQLite principal
10. Flag `--max-cost-usd-daily`
11. Complexidade: media-alta (~200 linhas)


## HIGH-10b — remember NAO aceita body como argumento posicional (inconsistencia com recall/search/read)

### Problema

O LLM agent tentou:
```
sqlite-graphrag remember "Registro de ideias e experiencias do Danilo em 28/05/2026..." --name "registro-ideias-danilo-2026-05-28" --type note --json
```

Resultado: exit 2 — `unexpected argument '...' found`

O remember e o UNICO subcomando que NAO aceita seu conteudo principal como argumento posicional:

| Comando | Arg posicional | O que aceita | Intuitivo? |
|---|---|---|---|
| `recall` | SIM `<QUERY>` | query como 1o arg | SIM |
| `hybrid-search` | SIM `<QUERY>` | query como 1o arg | SIM |
| `deep-research` | SIM `<QUERY>` | query como 1o arg | SIM |
| `related` | SIM `[NAME]` | name como 1o arg | SIM |
| `read` | SIM `[NAME]` | name como 1o arg | SIM |
| `edit` | SIM `[NAME]` | name como 1o arg | SIM |
| `forget` | SIM `[NAME]` | name como 1o arg | SIM |
| `remember` | **NAO** | `--body` flag obrigatorio | **NAO** |

### Consequencias

- LLM agents que aprendem o padrao "1o arg = conteudo principal" de recall/search extrapolam para remember → exit 2
- O agente perde 1-3 tentativas antes de descobrir que precisa de --body
- Cada tentativa falhada gasta tokens + tempo sem feedback util (exit 2 diz "unexpected argument" sem sugerir --body)
- Mensagem de erro do Clap ("unexpected argument") nao sugere a flag correta
- O padrão posicional e tao forte que MESMO apos ver a mensagem de erro, o LLM pode tentar `remember --body-stdin` sem perceber que body e necessario via flag

### Causa Raiz

**Faceta 1 — Design intencional mas inconsistente:**

O `remember` requer `--body` como flag explicita (nao posicional) porque:
- Tem MULTIPLAS formas de input: `--body`, `--body-file`, `--body-stdin`, `--graph-stdin`
- Um arg posicional seria ambiguo: e o body? o name? o tipo?
- Clap nao permite posicional OPCIONAL com tantas flags mutuamente exclusivas

Esta decisao e tecnicamente correta, mas cria inconsistencia com recall/search que aceitam `<QUERY>` posicional.

**Faceta 2 — Erro de Clap nao e acionavel:**

O erro `unexpected argument 'texto longo...' found` NAO sugere `--body`. Deveria dizer algo como:
`unexpected positional argument. Did you mean --body "..." or --body-stdin?`

### Solucao

**S1 — Adicionar `--description` e melhorar mensagem de erro (RECOMENDADA, facil):**

Nao mudar a interface, mas melhorar a mensagem via `after_long_help` ou `error` override:
```
Tip: remember does not accept positional arguments.
  Use --body "..." for inline content
  Use --body-file path for file content
  Use --body-stdin for piped content
  Use --graph-stdin for JSON with entities and relationships
```

**S2 — Aceitar body como argumento posicional opcional (ALTERNATIVA, breaking-ish):**

Adicionar `body` como posicional opcional com precedencia menor que --body:
```rust
#[arg(value_name = "BODY", help = "Inline body (alternative to --body flag)")]
pub positional_body: Option<String>,
```

Validacao: se positional_body E --body ambos presentes → exit 1.

Trade-off: aumenta superficie de API; pode confundir se o usuario quer name posicional (como read/edit) vs body posicional.

**S3 — Documentacao e prompt engineering (COMPLEMENTAR):**

Adicionar no CLAUDE.md/SKILL.md secao "Common LLM mistakes":
```
WRONG: sqlite-graphrag remember "body text" --name X --type note
RIGHT: sqlite-graphrag remember --name X --type note --body "body text"
RIGHT: echo "body text" | sqlite-graphrag remember --name X --type note --body-stdin
```

### Beneficios

- S1: zero breaking changes, LLM recebe hint acionavel no proximo retry
- S2: paridade com recall/search, mas trade-off de complexidade
- S3: previne o erro antes que aconteca

### Como Solucionar

**v1.0.66 (S1 + S3):**
1. Adicionar `after_long_help` no Clap do remember com hint sobre --body
2. Documentar "Common LLM mistakes" no CLAUDE.md
3. Complexidade: baixa (~10 linhas)

**v2.0.0 (S2, se decidido):**
4. Avaliar se body posicional vale a complexidade
5. Implementar com validacao de exclusividade mutua
6. Complexidade: media (~30 linhas + testes)


## HIGH-10c — LLM confunde taxonomia de entity_type com memory_type em MULTIPLOS comandos

### Problema

O problema manifesta-se em pelo menos 2 comandos distintos:

**Caso 1 — remember --graph-stdin com entity_type invalido:**
```json
{"name": "Principles of Psychology", "entity_type": "reference"}
```
Resultado: exit 1 — `unknown variant 'reference'`
Impacto: TODAS as 14 entidades e 15 relacoes do payload descartadas por 1 tipo invalido.

**Caso 2 — reclassify --new-type com memory_type em vez de entity_type:**
```bash
sqlite-graphrag reclassify --name "mantos-2020-10-12-..." --new-type document --json
```
Resultado: exit 2 — `invalid value 'document' for '--new-type <TYPE>'`
Impacto: LLM tentou reclassificar 17 entidades em loop, TODAS falharam silenciosamente (exit 2, no output). O LLM constatou que "reclassify nao esta persistindo" e fez 3 tentativas antes de perceber que `document` nao e entity type.

O LLM confunde as duas taxonomias distintas do sqlite-graphrag:
- **Memory types** (9): user, feedback, project, reference, decision, incident, skill, document, note
- **Entity types** (13): concept, date, dashboard, decision, file, incident, issue_tracker, location, memory, organization, person, project, tool

6 memory types NAO sao entity types: `reference`, `skill`, `document`, `note`, `user`, `feedback`.
Simetricamente, 10 entity types NAO sao memory types: `concept`, `date`, `dashboard`, `file`, `issue_tracker`, `location`, `memory`, `organization`, `person`, `tool`.

### Consequencias

- remember --graph-stdin: TODAS as entidades descartadas por 1 tipo invalido (perda total do payload)
- reclassify --new-type: falha silenciosa em loop — LLM ve "no output" e tenta novamente sem entender o porquê
- reclassify em batch (`while read` loop): 17 chamadas todas com exit 2, zero output, zero feedback util
- O LLM perde 3-5 tentativas ate descobrir que memory types ≠ entity types
- Overlap parcial (project, decision, incident existem em AMBAS) cria falsa sensacao de que o mapeamento e 1:1
- Nenhum dos dois erros sugere o tipo valido mais proximo (ex: `document` → `file`, `reference` → `concept`)

### Causa Raiz

**Faceta 1 — Duas taxonomias com nomes sobrepostos:**

| Valor | Memory type? | Entity type? | Confusao |
|---|---|---|---|
| project | SIM | SIM | Sem confusao |
| decision | SIM | SIM | Sem confusao |
| incident | SIM | SIM | Sem confusao |
| reference | SIM | NAO | LLM confunde ✗ |
| skill | SIM | NAO | LLM pode confundir |
| document | SIM | NAO | LLM pode confundir |
| note | SIM | NAO | LLM pode confundir |
| user | SIM | NAO | LLM pode confundir |
| feedback | SIM | NAO | LLM pode confundir |
| tool | NAO | SIM | Sem confusao |
| person | NAO | SIM | Sem confusao |
| concept | NAO | SIM | Sem confusao |

6 memory types NAO sao entity types. O LLM ve `--type reference` funcionar para memorias e extrapola para entidades.

**Faceta 2 — Rejeicao total por 1 entidade invalida:**

O `--graph-stdin` valida o payload JSON inteiro de uma vez. Se UMA entidade tem tipo invalido, TODO o payload e rejeitado — incluindo entidades validas e relacoes. Nao ha fallback parcial ("rejeitar entidade invalida, aceitar as validas").

**Faceta 3 — Mensagem de erro nao sugere alternativa:**

O erro diz `unknown variant 'reference'` e lista todos 13 tipos, mas NAO sugere: "did you mean 'concept'?" ou "note: 'reference' is a valid memory type, not entity type".

### Solucao

**S1 — Adicionar `reference` como entity type (AVALIACAO NECESSARIA):**

Adicionar `Reference` ao enum `EntityType` em `src/entity_type.rs` e migration V012. Justificativa: livros, papers, specs, URLs sao entidades legitimas do dominio de conhecimento.

Trade-off: inflaciona a taxonomia (13→14 tipos); pode ser coberto por `concept` ou `file`.

**S2 — Partial accept: aceitar entidades validas, rejeitar invalidas com warning (RECOMENDADA):**

Em vez de rejeitar TODO o payload, aceitar entidades com tipo valido e emitir `tracing::warn!` para cada entidade com tipo invalido:

```rust
for entity in entities {
    match EntityType::from_str(&entity.entity_type) {
        Ok(t) => valid_entities.push((entity, t)),
        Err(_) => {
            tracing::warn!(
                name = %entity.name,
                invalid_type = %entity.entity_type,
                "skipping entity with invalid type; valid types: concept, tool, person, ..."
            );
            skipped_count += 1;
        }
    }
}
```

Beneficio: as outras 13 entidades e 15 relacoes seriam preservadas em vez de perdidas.

**S3 — Mensagem de erro com sugestao e mapeamento automatico (COMPLEMENTAR):**

Melhorar a mensagem de validacao para sugerir tipo mais proximo:
```
invalid entity_type "reference" for entity "Principles of Psychology".
Valid entity types: concept, date, dashboard, decision, file, incident, issue_tracker, location, memory, organization, person, project, tool.
Hint: "reference" is a valid MEMORY type (--type reference) but not an entity type. For books/papers, consider "concept" or "file".
```

Tabela de mapeamento sugerido (memory_type → entity_type mais proximo):
| Memory type (invalido como entity) | Sugestao entity_type | Justificativa |
|---|---|---|
| `reference` | `concept` | referencias bibliograficas sao conceitos de conhecimento |
| `document` | `file` | documentos sao arquivos de informacao |
| `skill` | `concept` | habilidades sao conceitos abstratos |
| `note` | `concept` | notas capturam conceitos |
| `user` | `person` | usuarios sao pessoas |
| `feedback` | `concept` | feedback e conceito abstrato |

O mapeamento poderia ser implementado como auto-correcao com warning:
```rust
fn suggest_entity_type(invalid: &str) -> Option<&'static str> {
    match invalid {
        "reference" | "skill" | "note" | "feedback" => Some("concept"),
        "document" => Some("file"),
        "user" => Some("person"),
        _ => None,
    }
}
```

**S4 — Documentacao com tabela explicita das duas taxonomias (COMPLEMENTAR):**

Adicionar no CLAUDE.md secao clara:
```
## Taxonomias — Memory Types vs Entity Types
MEMORY types (--type): user, feedback, project, reference, decision, incident, skill, document, note
ENTITY types (entity_type in JSON): concept, date, dashboard, decision, file, incident, issue_tracker, location, memory, organization, person, project, tool
WARNING: reference, skill, document, note, user, feedback are MEMORY types ONLY — NOT valid for entities
Mapping: reference→concept, document→file, skill→concept, note→concept, user→person, feedback→concept
```

### Beneficios

- S1: elimina a confusao para `reference` especificamente
- S2: resiliencia — payload com 1 erro nao perde 13 entidades validas
- S3: LLM corrige na proxima tentativa sem precisar de --help; auto-correcao elimina retries
- S4: previne a confusao antes que aconteca

### Como Solucionar

**v1.0.66 (S3 + S4):**
1. Melhorar mensagem de erro em `src/commands/remember.rs` ou validacao de graph-stdin (~10 linhas)
2. Adicionar tabela de taxonomias no CLAUDE.md
3. Complexidade: baixa

**v1.1.0 (S2):**
4. Refatorar validacao de graph-stdin para partial accept
5. Adicionar campo `skipped_entities` no JSON response
6. Complexidade: media (~40 linhas)

**v2.0.0 (S1, se decidido):**
7. Adicionar `Reference` ao EntityType enum
8. Migration V012 com ALTER TABLE ou rebuild
9. Complexidade: baixa-media (~20 linhas + migration)


## MEDIUM-07 — jaq operator precedence causa cascata de falhas em auditorias LLM

### Problema

O LLM agent escreveu uma expressao jaq para auditar distribuicao de tamanho de descricoes:
```bash
sqlite-graphrag list --json | jaq '{
  desc_under_30: [.items[] | select(.description | length < 30 and .description | length < 80)] | length
}'
```

Resultado: exit 5 — `cannot index "Diagnostico completo do GraphRAG..." with "description"`

A expressao PARECE correta mas falha por precedencia de operadores em jaq.

### Consequencias

- A auditoria de qualidade de descricoes falha completamente
- Como o LLM usou parallel tool calls, a falha cascateou: self-loop check e duplicate-edge check foram CANCELADOS
- 3 auditorias perdidas (descricao + self-loops + duplicatas) por 1 erro de sintaxe jaq
- O LLM nao entende a mensagem de erro (`cannot index string with "description"`) e tenta solucoes erradas

### Causa Raiz

**Faceta 1 — jaq pipe precedence diferente do esperado:**

O LLM escreveu:
```
select(.description | length < 30 and .description | length < 80)
```

jaq parseia como:
```
select(.description | (length < 30 and .description) | length < 80)
```

Porque `and` tem precedencia MAIS BAIXA que `|` em jaq. O resultado de `length < 30 and .description` e a string de descricao (truthy), e depois `| length < 80` tenta acessar `.description` nessa string → erro.

Sintaxe CORRETA (parenteses explicitos):
```
select((.description | length) < 30 and (.description | length) < 80)
```

**Faceta 2 — Nao e bug do sqlite-graphrag:**

Este e um gap de usabilidade do ecossistema: jaq tem precedencia de operadores diferente do que LLMs (e humanos) esperam. O sqlite-graphrag nao pode resolver isso diretamente, mas pode mitigar com:
- Exemplos jaq corretos na documentacao (ja proposto em HIGH-05 S2)
- Funcionalidades built-in que evitem jaq complexo (ex: `list --json --stats` com distribuicao de descricoes embutida)

### Solucao

**S1 — Documentacao de jaq pitfalls (COMPLEMENTAR ao HIGH-05 S2):**

Adicionar na secao "jaq Quick Reference" do CLAUDE.md:
```
# WRONG — operator precedence trap:
select(.description | length < 30 and .description | length < 80)
# RIGHT — explicit parentheses:
select((.description | length) >= 30 and (.description | length) < 80)
```

**S2 — CLI built-in stats para evitar jaq complexo (OPCIONAL):**

Adicionar `list --stats --json` que emite distribuicao de metadados diretamente:
```json
{
  "total": 986,
  "by_type": {"note": 400, "project": 200, ...},
  "description_length": {"min": 5, "max": 499, "avg": 87, "p50": 72},
  "body_length": {"min": 10, "max": 512000, "avg": 2400, "p50": 1200}
}
```

Beneficio: elimina necessidade de jaq complexo para auditorias comuns.

### Como Solucionar

1. Adicionar exemplos de jaq com parenteses explicitos no CLAUDE.md (junto com HIGH-05 S2)
2. Considerar `list --stats` para v1.1.0
3. Complexidade: S1 = documentacao pura; S2 = media (~50 linhas)


## BUG-06 HIGH — link NAO atualiza peso de edges existentes + JSON response mente sobre o peso

### Problema

O comando `link` quando a aresta ja existe:
1. Retorna `"action": "already_exists", "weight": 0.85` (o peso SOLICITADO)
2. Mas o peso NO BANCO permanece `0.5` (o peso ORIGINAL)

Verificado empiricamente:
```bash
# Criar edge com peso 0.5
sqlite-graphrag link --from test-a --to test-b --relation uses --weight 0.5 --create-missing
# → action: "created", weight: 0.5

# Tentar atualizar peso
sqlite-graphrag link --from test-a --to test-b --relation uses --weight 0.85
# → action: "already_exists", weight: 0.85  ← JSON DIZ 0.85

# Verificar banco
sqlite-graphrag graph --format json | jaq '.edges[0].weight'
# → 0.5  ← BANCO MANTÉM 0.5
```

### Consequencias

- **640 edges** (23.9% de 2674) no banco real do usuario estao com peso default 0.5 que NAO PODE ser atualizado via `link`
- O workaround "unlink + link" e necessario para recalibrar pesos — operacao destrutiva que remove `memory_relationships` bindings
- A resposta JSON com `"weight": 0.85` quando o banco tem `0.5` viola o contrato de honestidade do JSON output
- LLM agents que fazem `link --weight 0.85` confiam no response e acreditam que o peso foi atualizado
- Auditorias de peso pos-calibracao mostram pesos inalterados, gerando confusao

### Causa Raiz

A funcao `create_or_fetch_relationship()` em `src/storage/entities.rs:293-296`:

```rust
let existing = find_relationship(conn, source_id, target_id, relation)?;
if let Some(row) = existing {
    return Ok((row.id, false));  // ← RETORNA SEM ATUALIZAR NADA
}
```

Quando a relacao ja existe, a funcao retorna o ID existente e `was_created = false` **sem fazer nenhum UPDATE** do peso.

Em contraste, a funcao `upsert_relationship()` (linhas 163-178) usa `ON CONFLICT DO UPDATE SET weight = excluded.weight` — mas esta funcao e usada APENAS pelo `remember --graph-stdin`, NAO pelo `link`.

O response em `link.rs:244` usa `weight` dos args da CLI (o peso SOLICITADO), nao o peso real do banco:
```rust
let response = LinkResponse {
    weight,  // ← args.weight, NÃO o peso do banco
    ...
};
```

### Solucao

**S1 — Atualizar peso em create_or_fetch_relationship (RECOMENDADA):**

```rust
let existing = find_relationship(conn, source_id, target_id, relation)?;
if let Some(row) = existing {
    // UPDATE weight if different from requested
    if (row.weight - weight).abs() > f64::EPSILON {
        conn.execute(
            "UPDATE relationships SET weight = ?1 WHERE id = ?2",
            params![weight, row.id],
        )?;
    }
    return Ok((row.id, false));
}
```

E no response, retornar o peso efetivo (do banco) em vez do solicitado.

**S2 — Usar upsert_relationship em vez de create_or_fetch (ALTERNATIVA):**

Substituir `create_or_fetch_relationship` por `upsert_relationship` no `link.rs`, que ja tem `ON CONFLICT DO UPDATE SET weight = excluded.weight`. Requer ajuste do return type.

**S3 — Reportar peso real no response (OBRIGATORIA independente de S1/S2):**

```rust
// Ler peso real do banco após operação
let actual_weight: f64 = tx.query_row(
    "SELECT weight FROM relationships WHERE id = ?1",
    params![rel_id], |r| r.get(0))?;
let response = LinkResponse { weight: actual_weight, ... };
```

### Beneficios

- S1/S2: `link --weight 0.85` efetivamente atualiza o peso — elimina necessidade de unlink+link
- S3: JSON response honesto — LLM agents e scripts confiam no peso reportado
- 640 edges com peso default 0.5 podem ser recalibradas com um loop simples de `link`
- Elimina o workaround destrutivo de unlink+link que perde memory_relationships

### Como Solucionar

**v1.0.66 (S1 + S3):**
1. Em `src/storage/entities.rs:293-296`, adicionar UPDATE do peso quando diferente (~5 linhas)
2. Em `src/commands/link.rs:244`, usar peso real do banco no response (~3 linhas)
3. Adicionar teste: link existente com peso diferente → peso atualizado no DB
4. Complexidade: baixa (~10 linhas)


## HIGH-12 — remember --graph-stdin rejeita >50 relacoes com exit 6 (constante hardcoded ignora env var override)

### Problema

O LLM agent tenta criar uma memoria rica com 51+ relacionamentos:
```bash
sqlite-graphrag remember --name mem --type note --description "d" --graph-stdin --json <<'GRAPHEOF'
{"body":"...","entities":[...],"relationships":[... 55 items ...]}
GRAPHEOF
```

Resultado: exit 6 — `limit exceeded: relationships exceed limit of 50`

O LLM e forcado a decidir quais relacoes remover, perdendo contexto semantico. Em dominios ricos (pesquisa academica, genealogia, compliance), 50 relacoes por memoria e insuficiente.

### Consequencias

- Memorias com grafos ricos sao truncadas ou rejeitadas — perda de informacao
- O LLM gasta 1-3 tentativas removendo relacoes ate caber no limite
- A env var `SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY` deveria permitir override, mas o remember usa a CONSTANTE (`MAX_RELATIONSHIPS_PER_MEMORY = 50`) em vez da funcao `max_relationships_per_memory()` que le a env var
- INCONSISTENCIA no mesmo arquivo: linha 314 REJEITA com constante (50), linha 385 TRUNCA com constante (50), mas a funcao de override `max_relationships_per_memory()` lendo env var NAO E CHAMADA em nenhum dos dois pontos
- O `ingest` tambem usa a constante diretamente em `ingest.rs:485-486`

### Causa Raiz

**Faceta 1 — Constante hardcoded em vez de funcao configuravel:**

`remember.rs:314`:
```rust
if graph.relationships.len() > MAX_RELATIONSHIPS_PER_MEMORY {  // ← constante 50
    return Err(AppError::LimitExceeded(...));
}
```

Deveria ser:
```rust
if graph.relationships.len() > max_relationships_per_memory() {  // ← le env var
    return Err(AppError::LimitExceeded(...));
}
```

A funcao `max_relationships_per_memory()` JA EXISTE em `constants.rs:117-123` e le `SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY`. Porem `remember.rs` usa a constante diretamente, ignorando o override.

**Faceta 2 — Rejeicao vs truncamento inconsistente:**

Dois comportamentos no MESMO arquivo `remember.rs`:
- Linha 314: `--graph-stdin` path → REJEITA com exit 6 (perda total)
- Linha 385: NER extraction path → TRUNCA silenciosamente (perda parcial)

O `ingest.rs:485-486` tambem TRUNCA em vez de rejeitar.

Rejeicao total (exit 6) e o comportamento mais agressivo — deveria pelo menos truncar com warning como faz o NER path.

**Faceta 3 — Limite de 50 e arbitrariamente baixo para uso LLM:**

O limite de 50 foi calibrado para NER automatico (extraction.rs) onde relacoes sao inferidas e podem ser ruidosas. Para input curado por LLM via --graph-stdin, o limite deveria ser mais alto pois as relacoes foram deliberadamente selecionadas.

### Solucao

**S1 — Usar funcao em vez de constante (FIX MINIMO, ~3 linhas):**

Substituir `MAX_RELATIONSHIPS_PER_MEMORY` por `max_relationships_per_memory()` nas linhas 314, 316, 385, 387 de `remember.rs` e 485, 486 de `ingest.rs`. Permite override via env var.

**S2 — Truncar com warning em vez de rejeitar (RECOMENDADA):**

Alinhar comportamento da linha 314 com linha 385: truncar e emitir warning em vez de rejeitar:
```rust
let cap = max_relationships_per_memory();
if graph.relationships.len() > cap {
    tracing::warn!(
        count = graph.relationships.len(),
        cap = cap,
        "truncating relationships to cap"
    );
    graph.relationships.truncate(cap);
    relationships_truncated = true;
}
```

Reportar `relationships_truncated: true` no JSON response (ja existe o campo).

**S3 — Flag --max-relationships no remember (COMPLEMENTAR):**

Adicionar flag de CLI para override por invocacao:
```rust
#[arg(long, value_name = "N", help = "Max relationships per memory (default: 50, env: SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY)")]
pub max_relationships: Option<usize>,
```

**S4 — Documentar env var prominentemente (COMPLEMENTAR):**

A env var `SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY` esta documentada em `constants.rs` mas NAO no CLAUDE.md, --help, ou llms.txt. LLM agents e usuarios nao sabem que o override existe.

### Beneficios

- S1: override via env var funciona (atualmente nao funciona apesar de documentado)
- S2: memorias ricas nao sao completamente rejeitadas
- S3: controle fino por invocacao sem env var global
- S4: LLM agents descobrem o override e usam proativamente

### Como Solucionar

**v1.0.66 (S1 + S2 + S4):**
1. `remember.rs:314,316`: substituir `MAX_RELATIONSHIPS_PER_MEMORY` por `max_relationships_per_memory()` (~2 linhas)
2. `remember.rs:385,387`: idem (~2 linhas)
3. `ingest.rs:485,486`: idem (~2 linhas)
4. Considerar truncar+warn em vez de rejeitar na linha 314 (~5 linhas)
5. Documentar env var no CLAUDE.md e --help do remember
6. Complexidade: baixa (~10 linhas de codigo + documentacao)


## HIGH-13 — Tripla inconsistencia de normalizacao: relacoes snake/kebab + entidades 4 formatos + duplicatas acentuadas

### Problema

Tres facetas de inconsistencia de normalizacao coexistem no banco de producao:

**Faceta 1 — Relacoes snake_case E kebab-case coexistindo (2.703+ edges afetadas):**

| Canonico (kebab) | Count | Snake_case | Count | Total |
|---|---|---|---|---|
| applies-to | 381 | applies_to | 2.322 | 2.703 |
| depends-on | 198 | depends_on | 1.204 | 1.402 |
| tracked-in | 178 | tracked_in | 19 | 197 |

A mesma relacao semantica existe em DOIS formatos. O CHANGELOG v1.0.63 documenta que `normalize_relation()` normaliza para snake_case antes de inserir, mas relacoes legadas em kebab-case nao foram migradas.

**Faceta 2 — Entidades com 4 padroes de nomenclatura (6.243 entidades):**

| Padrao | Quantidade | % | Exemplo |
|---|---|---|---|
| Lowercase kebab-case | 3.399 | 54% | pdca-ciclo |
| Kebab com maiusculas | 1.607 | 26% | Faixa-Marrom |
| Com espacos | 1.041 | 17% | Green Belt |
| Inicio maiusculo sem hifen | 196 | 3% | EBITDA |

O v1.0.65 adicionou `normalize_entity_name()` que normaliza para kebab-case ASCII minusculo em paths de ESCRITA, mas entidades legadas com maiusculas, espacos e mixed-case NAO foram migradas. O comando `normalize-entities --yes` deveria resolver, mas faz normalizacao incompleta (ver Faceta 3).

**Faceta 3 — Duplicatas acentuadas/nao-acentuadas (6 pares confirmados):**

| Com acento | Grau | Sem acento | Grau |
|---|---|---|---|
| orcamento-base-zero | 14 | orcamento-base-zero | 14 |
| parentificacao-destrutiva | 2 | parentificacao-destrutiva | 4 |
| ticket-medio | 4 | ticket-medio | 2 |
| tripe-comercial | 1 | tripe-comercial | 3 |
| preco-e-rei | 2 | preco-e-rei | 1 |
| tecnica-5-porques | 1 | tecnica-5-porques | 4 |

10 entidades com acentos no nome: consistencia-matematica-metas, cross-selling-farmacia, orcamento-base-zero, parentificacao-destrutiva, preco-e-rei, ticket-medio, tripe-comercial, tecnica-5-porques, yokoten-expansao-horizontal, matriz-consistencia-causas.

### Consequencias

- `graph entities` e `graph traverse` sao case-sensitive: "Faixa-Marrom" e "faixa-marrom" sao nos DIFERENTES no grafo
- Buscas por relacao retornam resultados parciais: `--relation applies-to` encontra 381 mas perde 2.322 em `applies_to`
- Evidence chains do deep-research podem falhar por nao encontrar path entre nos duplicados
- Os 6 pares acentuados tem relacoes DIVIDIDAS entre os dois nos — metade do grau em cada
- `merge-entities` pode resolver pares individuais, mas nao ha operacao em batch para os 1.607+ entidades com maiusculas

### Causa Raiz

**Faceta 1 — Migracao retroativa incompleta:**

O v1.0.63 adicionou `normalize_relation()` que converte kebab→snake nos writes, mas o banco ja tinha ~580 relacoes em kebab-case de versoes anteriores. Nao houve migracao de dados legados.

O comando `reclassify-relation` DEVERIA poder converter, mas tem BUG-01 (crash updated_at). Mesmo sem o bug, seria necessario 3 invocacoes (uma por relacao afetada).

**Faceta 2 — normalize_entity_name() aplicada apenas em writes novos:**

O v1.0.65 normaliza em paths de escrita mas entidades criadas antes de v1.0.65 (pela ingestao Codex, NER, ou graph-stdin manual) nao foram retroativamente normalizadas. O `normalize-entities --yes` executa a normalizacao mas o usuario pode nao ter rodado ainda.

**Faceta 3 — normalize_entity_name() nao remove acentos:**

A funcao faz NFKD decomposition + filtro ASCII, mas entidades criadas ANTES da funcao existir mantem acentos. O `normalize-entities` deveria detectar e mesclar esses pares mas pode nao estar tratando decomposicao NFKD dos acentos existentes.

### Solucao

**S1 — Migration de relacoes legadas (CRITICA, bloqueia Faceta 1):**

Primeiro, corrigir BUG-01 (reclassify-relation crash). Depois:

```sql
-- Migration V012: normalize legacy kebab-case relations to snake_case
UPDATE relationships SET relation = 'applies_to' WHERE relation = 'applies-to';
UPDATE relationships SET relation = 'depends_on' WHERE relation = 'depends-on';
UPDATE relationships SET relation = 'tracked_in' WHERE relation = 'tracked-in';
```

OU: adicionar ao `reclassify-relation` um modo batch sem `updated_at`:
```bash
sqlite-graphrag reclassify-relation --from-relation applies-to --to-relation applies_to --batch --yes
```

**S2 — Executar normalize-entities --yes (IMEDIATA, Faceta 2):**

```bash
sqlite-graphrag normalize-entities --yes --json
```

Deveria resolver as 2.844 entidades nao-kebab-case (26% maiusculas + 17% espacos + 3% caps). Verificar se merge de colisoes funciona corretamente.

**S3 — Mesclar pares acentuados manualmente (Faceta 3):**

```bash
for pair in "orcamento-base-zero:orcamento-base-zero" "ticket-medio:ticket-medio"; do
  IFS=: read -r accented plain <<< "$pair"
  sqlite-graphrag merge-entities --names "$accented" --into "$plain" --json
done
```

Se `normalize-entities` ja faz NFKD + strip acentos, S2 pode resolver S3 automaticamente.

**S4 — Validacao pos-normalizacao:**

```bash
# Verificar formatos remanescentes
sqlite-graphrag graph entities --json --limit 10000 | jaq '[.entities[].name | select(test("[A-Z ]"))] | length'
# Verificar relacoes remanescentes
sqlite-graphrag graph --format json | jaq '[.edges[].relation | select(contains("-"))] | unique'
```

### Beneficios

- S1: buscas por relacao retornam 100% dos resultados (2.703 applies_to em vez de 381 ou 2.322)
- S2: entidades unificadas — "Faixa-Marrom" e "faixa-marrom" viram um unico no
- S3: pares acentuados mesclados — grau combinado em vez de dividido
- Qualidade do deep-research melhora: evidence chains encontram paths que antes eram bloqueados por duplicatas

### Como Solucionar

**Imediato (dados, S2 + S3):**
1. `sqlite-graphrag normalize-entities --yes --json` (resolve Faceta 2 e possivelmente 3)
2. Mesclar pares acentuados remanescentes com `merge-entities`
3. Verificar com S4

**v1.0.66 (codigo, S1):**
4. Corrigir BUG-01 (reclassify-relation crash) — pre-requisito
5. Rodar `reclassify-relation` para as 3 relacoes kebab→snake
6. OU: adicionar SQL migration V012 com UPDATE direto
7. Complexidade: baixa (3 SQL statements)

**v1.1.0 (prevencao):**
8. Adicionar validacao em `health` que detecta formatos mistos e emite warning
9. Adicionar `graph audit --json` que reporta inconsistencias de normalizacao
10. Complexidade: media (~50 linhas)


## MEDIUM-08b — 28 entidades tipo date com naming inconsistente e degree 1

### Problema

28 entidades com `entity_type: "date"` tem 3 padroes de nomenclatura incompativeis:

| Padrao | Exemplo | Contagem | ISO? |
|---|---|---|---|
| ISO (correto) | `2022-06-14` | ~10 | SIM |
| PT-BR extenso | `janeiro-26-2026` | ~8 | NAO |
| Prefixo data- | `data-2022-05-19` | ~6 | NAO |
| Misto | `data-bloqueios-2026-01-28` | ~4 | NAO |

A maioria tem degree 1 — conectada a apenas 1 memoria, sem valor de hub.

### Consequencias

- Buscas por data nao encontram todas as datas relevantes: "2026-01-26" nao encontra "janeiro-26-2026"
- graph traverse partindo de datas retorna resultados incompletos
- Entidades date com degree 1 inflam o grafo com nos de baixo valor (28 nos que poderiam ser apenas metadata da memoria)
- Formato nao-padronizado impede ordenacao cronologica programatica

### Causa Raiz

**Faceta 1 — NER extrai datas em formato natural da lingua do body:**

Quando o body esta em portugues, o NER/regex extrai "janeiro de 2026" que vira entidade "janeiro-26-2026" apos kebab-case. Em ingles, extrai "2022-06-14" que ja esta em ISO.

**Faceta 2 — Sem normalizacao de formato de data na pipeline de entidades:**

`normalize_entity_name()` normaliza case e espacos para kebab-case mas NAO normaliza formatos de data para ISO 8601. Nao existe `normalize_date_entity()`.

**Faceta 3 — Datas de degree 1 sao ruido vs sinal:**

Entidades date com degree 1 nao servem como hubs de conexao — nao adicionam paths de traversal. Poderiam ser metadata de memoria (campo `created_at`/`event_date`) em vez de nos no grafo.

### Solucao

**S1 — Rename manual para ISO (IMEDIATA, zero codigo):**

```bash
sqlite-graphrag rename-entity --name "janeiro-26-2026" --new-name "2026-01-26" --json
sqlite-graphrag rename-entity --name "janeiro-28-2026" --new-name "2026-01-28" --json
sqlite-graphrag rename-entity --name "maio-19-2022" --new-name "2022-05-19" --json
sqlite-graphrag rename-entity --name "marco-06-2024" --new-name "2024-03-06" --json
sqlite-graphrag rename-entity --name "maio-2025" --new-name "2025-05" --json
sqlite-graphrag rename-entity --name "data-2022-05-19" --new-name "2022-05-19" --json
sqlite-graphrag rename-entity --name "data-2026-02-26" --new-name "2026-02-26" --json
sqlite-graphrag rename-entity --name "data-bloqueios-2026-01-28" --new-name "2026-01-28-bloqueios" --json
```

**S2 — Avaliar e podar datas degree 1 (COMPLEMENTAR):**

```bash
# Listar datas com degree 1
sqlite-graphrag graph entities --entity-type date --json | jaq '[.entities[] | select(.degree <= 1)] | .[].name'

# Para datas puramente processuais sem contexto util:
sqlite-graphrag delete-entity --name "2022-06-14" --cascade --json
```

Criterio: manter datas que conectam 2+ memorias (hub value); podar datas que conectam apenas 1 memoria (metadata, nao entidade).

**S3 — Normalizacao automatica de datas na pipeline (FUTURA):**

Adicionar `normalize_date_entity()` que detecta padroes comuns e converte para ISO:
```rust
fn normalize_date_entity(name: &str) -> Option<String> {
    // "janeiro-26-2026" → "2026-01-26"
    // "data-2022-05-19" → "2022-05-19"
    // "maio-2025" → "2025-05"
    // já ISO → None (sem mudança)
}
```

Chamada em `normalize_entity_name()` quando `entity_type == "date"`.

### Beneficios

- S1: formato ISO uniforme permite ordenacao cronologica e busca por range
- S2: reduz ruido no grafo (28 nos com degree 1 → ~10 hubs uteis)
- S3: previne recorrencia do problema em futuras ingestoes

### Como Solucionar

**Imediato (S1):** 8 invocacoes de rename-entity, zero codigo
**v1.0.66 (S2):** avaliar e podar datas de baixo valor apos rename
**v1.1.0 (S3):** normalize_date_entity() (~30 linhas)


## MEDIUM-09 — Sem comando para re-indexar vetores de memorias desincronizadas (fts rebuild existe, vec rebuild NAO)

### Problema

Quando memorias perdem sincronizacao com seus vetores de embedding (`vec_memories`, `vec_chunks`, `vec_entities`), NAO existe comando para reconstruir o indice vetorial. Em contraste, o FTS5 TEM comando de reconstrucao:

| Indice | Rebuild existe? | Comando |
|---|---|---|
| FTS5 (full-text) | SIM | `fts rebuild --json` |
| vec_memories (KNN) | NAO | nenhum |
| vec_chunks (KNN) | NAO | nenhum |
| vec_entities (KNN) | NAO | nenhum |

Estado atual do banco: memories=1000, vec_memories=1000 (sincronizado). Porem, o desync JA ocorreu historicamente em pelo menos 4 cenarios documentados no CHANGELOG:

### Cenarios que Causam Desync (verificados no historico)

1. **v1.0.62 G01 CRITICAL**: `ingest --mode claude-code` NAO computava embeddings → memorias com zero vec_memories
2. **v1.0.63 FINDING-1**: `edit` NAO regenerava embedding apos mudar body → recall retornava scores incorretos
3. **v1.0.56 C1 CRITICAL**: `force-merge` NAO sincronizava FTS5 (bug analogo para vetores possivel)
4. **Daemon crash**: se o daemon cai APOS salvar a memoria mas ANTES de persistir o vetor, a memoria existe sem vec_memories correspondente
5. **Database restore de backup**: backup feito entre memoria INSERT e vetor INSERT
6. **Upgrade de modelo**: se o modelo de embedding mudar (ex: multilingual-e5-small → e5-large), vetores antigos ficam incompativeis dimensionalmente

### Consequencias

- `recall` e `hybrid-search` NAO encontram memorias sem vetores — sao invisiveis para busca semantica
- `health --json` reporta `vec_memories_ok: true` quando counts batem, mas NAO verifica se os vetores estao CORRETOS (ex: dimensao errada, embedding de body antigo apos edit)
- O unico workaround e `edit --body "mesmo body"` por memoria individual — operacao de N passos para N memorias
- Nao existe `vec rebuild` analogo a `fts rebuild`
- Para bases com 1000+ memorias, reconstrucao manual e inviavel

### Causa Raiz

**Faceta 1 — Assimetria FTS5 vs vec:**

O FTS5 tem `fts rebuild` (adicionado v1.0.56 GAP-32) que reconstroi o indice completo a partir da tabela `memories`. Nao existe equivalente para vec — o vetor e gerado DURANTE o `remember`/`edit`/`ingest` e nunca regenerado depois.

**Faceta 2 — Health check verifica contagem, nao conteudo:**

`health --json` verifica `memories.count == vec_memories.count` (contagem igual) mas NAO verifica:
- Se cada memory_id tem um vec correspondente
- Se o vetor corresponde ao body ATUAL (apos edit)
- Se a dimensao do vetor e correta (384 para multilingual-e5-small)
- Se o vetor foi gerado com o modelo correto

**Faceta 3 — Sem transacao atomica memoria+vetor:**

O `remember` persiste a memoria, depois gera e persiste o vetor como operacoes separadas. Se o processo morre entre as duas, a memoria existe sem vetor. O `edit` com re-embedding (v1.0.63) tem o mesmo risco.

### Solucao

**S1 — Comando `vec rebuild` (RECOMENDADA, paridade com fts rebuild):**

```bash
sqlite-graphrag vec rebuild --json
```

O comando deveria:
1. Listar todas as memorias com body nao-vazio
2. Para cada memoria, regenerar embedding do body
3. Upsert em `vec_memories` (substituir vetor existente ou criar novo)
4. Regenerar `vec_chunks` para memorias multi-chunk
5. Regenerar `vec_entities` para todas entidades
6. Reportar NDJSON: `{name, status: "reindexed"|"skipped"|"failed", elapsed_ms}`
7. Summary: `{total, reindexed, skipped, failed, elapsed_ms}`

Trade-off: operacao LENTA (1-2s por memoria com daemon, 15-30min para 1000 memorias). Necessita `--confirm` ou `--yes` e `--max-concurrency`.

**S2 — Health check aprofundado (COMPLEMENTAR):**

Adicionar ao `health --json`:
```json
{
  "vec_memories_ok": true,
  "vec_memories_count_match": true,
  "vec_memories_orphaned": 0,
  "vec_memories_missing": 0,
  "vec_dimension_consistent": true,
  "vec_embedding_model": "multilingual-e5-small"
}
```

Verificar:
- `vec_memories_orphaned`: vetores sem memoria correspondente
- `vec_memories_missing`: memorias sem vetor correspondente
- `vec_dimension_consistent`: todos vetores tem 384 dimensoes

**S3 — `vec check` para diagnostico sem reconstrucao:**

```bash
sqlite-graphrag vec check --json
```

Analogo a `fts check`: verifica integridade sem modificar.

### Beneficios

- S1: recuperacao de desync em 1 comando (vs N edits manuais)
- S2: deteccao proativa de desync no health check
- S3: diagnostico sem risco de corrupao

### Como Solucionar

**v1.0.66 (S2 + S3):**
1. Adicionar verificacao de orphaned/missing vec na health (~20 linhas)
2. Adicionar `vec check --json` com diagnostico (~30 linhas)
3. Complexidade: media

**v1.1.0 (S1):**
4. Implementar `vec rebuild --json` com NDJSON progress (~150 linhas, reutiliza embedder + chunking)
5. Adicionar `--yes`, `--max-concurrency`, `--dry-run`
6. Complexidade: media-alta


## LOW-04 — Sem criterios de tipagem de memorias documentados (tipo atribuido por intuicao do LLM)

### Problema

Nao existem criterios formais para quando usar cada um dos 9 tipos de memoria. O LLM agent atribui tipos por intuicao, gerando inconsistencia:

Distribuicao atual (1000 memorias):
- note: 190 (19%) — tipo mais comum, usado como catch-all
- incident: 176 (17.6%)
- document: 172 (17.2%)
- reference: 163 (16.3%)
- decision: 163 (16.3%) — inclui ~20 sub-chunks de rules (ver HIGH-10)
- project: 100 (10%)
- feedback: 31 (3.1%)
- skill: 4 (0.4%) — severamente subutilizado
- user: 3 (0.3%) — severamente subutilizado

### Consequencias

- `skill` com apenas 4 memorias (0.4%) quando o banco tem dezenas de procedimentos e workflows que deveriam ser skill
- `decision` com 163 memorias inclui ~20 que nao sao decisoes (HIGH-10)
- `list --type skill` retorna quase nada — inutiliza o filtro para encontrar procedimentos
- Diferentes sessoes de LLM atribuem tipos diferentes para conteudo similar
- Recall por tipo (`list --type X`) tem signal-to-noise variavel entre tipos
- Nao existe validacao semantica: qualquer conteudo aceita qualquer tipo

### Causa Raiz

**Faceta 1 — Sem documentacao de criterios no CLAUDE.md:**

O CLAUDE.md lista os 9 tipos mas NAO define QUANDO usar cada um. O LLM precisa inferir da semantica do nome do tipo, que e ambigua (ex: quando algo e `note` vs `document` vs `reference`?).

**Faceta 2 — Tipos sem fronteira semantica clara:**

| Ambiguidade | Tipo A | Tipo B | Criterio faltante |
|---|---|---|---|
| Texto longo | document | reference | Extensao? Profundidade? |
| Procedimento | skill | reference | E executavel ou consultivo? |
| Observacao | note | feedback | E direcional (para alguem) ou reflexiva? |
| Bug report | incident | note | Tem impacto operacional? |
| Analise | document | decision | Conclui com escolha ou e descritiva? |

**Faceta 3 — CLI nao oferece sugestao de tipo:**

O `remember --type` aceita qualquer dos 9 valores sem feedback sobre adequacao. Nao existe `--suggest-type` que analisaria o body e recomendaria um tipo.

### Solucao

**S1 — Documentar criterios formais no CLAUDE.md (IMEDIATA, zero codigo):**

Adicionar secao "Criterios de Tipagem de Memorias" ao CLAUDE.md:

```
## Criterios de Tipagem de Memorias
- document: conteudo extenso, narrativo, transcricoes, analises profundas (>500 chars, prosa corrida)
- reference: frameworks, tabelas, definicoes, material de consulta rapida (formato estruturado, bullets, listas)
- skill: procedimentos, workflows, receitas de execucao, prompts operacionais (tem PASSOS, e EXECUTAVEL)
- decision: escolhas arquiteturais com alternativas avaliadas, justificativa e trade-offs (tem CONTEXTO + ESCOLHA + PORQUE)
- note: observacoes pontuais, insights, reflexoes, anotacoes de sessao (<500 chars, sem estrutura formal)
- project: escopo de trabalho ativo, estado de projetos, planejamentos (tem TIMELINE, STATUS, ENTREGAVEIS)
- user: perfil, preferencias, contexto pessoal do usuario (raramente criado por agente)
- feedback: correcoes de abordagem, validacoes, anti-patterns (direcional: "faca X, nao faca Y")
- incident: problemas, bugs, falhas, diagnosticos (tem CAUSA RAIZ, IMPACTO, FIX)
```

Regra de ouro: em caso de duvida, usar `note` (generico) em vez de tipo errado.

**S2 — Reclassificar memorias existentes via force-merge (IMEDIATA, zero codigo):**

Campanha de reclassificacao:
1. 20 decisions que sao sub-chunks → skill (HIGH-10, ja documentado)
2. Memorias de procedimento atualmente em `note` ou `reference` → skill
3. Memorias de bug report em `note` → incident

**S3 — --suggest-type que analisa body e recomenda (FUTURA):**

```bash
sqlite-graphrag remember --name X --suggest-type --body "..."
# → suggested_type: "skill" (body contains step-by-step instructions)
```

Heuristica baseada em:
- Presenca de "Passo 1", "Step 1", bullets numerados → skill
- Presenca de "Contexto:", "Decisao:", "Trade-off:" → decision
- Presenca de "Causa raiz:", "Impacto:", "Fix:" → incident
- Presenca de "Nao faca", "Evitar", "Preferir" → feedback
- Tamanho > 500 chars sem estrutura → document
- Tamanho < 500 chars sem estrutura → note

### Beneficios

- S1: LLM agents atribuem tipos consistentes entre sessoes
- S2: `list --type skill` retorna resultados uteis (de 4 para 20+)
- S3: automacao previne tipagem incorreta na fonte

### Como Solucionar

**Imediato (S1):** adicionar secao no CLAUDE.md (~20 linhas)
**Imediato (S2):** force-merge loop para reclassificar (~30 invocacoes, zero codigo)
**v1.1.0 (S3):** heuristica de sugestao de tipo (~50 linhas)


## MEDIUM-08 — graph --format json reportado com "100% edges source/target null" (NAO reproduzido em v1.0.65)

### Problema Reportado

Relato: `graph --format json` renderizaria 100% dos edges com `from: null, to: null`. Os comandos de query (related, graph traverse, hybrid-search --with-graph) funcionariam perfeitamente, impedindo exportacao do grafo para visualizacao externa.

### Investigacao (2026-05-29)

**NAO REPRODUZIDO** no banco real do usuario (2674 edges, 0 com null):
```json
{"total_edges": 2674, "null_from": 0, "null_to": 0, "both_null": 0}
```

Tambem nao reproduzido em banco de teste isolado. O codigo em `graph_export.rs:310-333` faz JOIN correto via `id_to_name` HashMap:
- Para cada edge, busca `from = id_to_name.get(&r.source_id)`
- Se nao encontrar, edge e **SKIPPED** com `tracing::warn!` (nao emitido com null)
- Campo `orphan_edges` contabiliza edges descartados

### Causa Raiz Possivel (nao confirmada)

O cenario que PODERIA causar edges com from/to null seria se:
1. `list_entities` e `list_relationships_by_namespace` usassem filtros de namespace DIFERENTES
2. Namespace filtrasse nos mas NAO edges → `id_to_name` vazio → todas edges skipped
3. Ou uma versao anterior do binario tivesse serialização diferente

### Status

**NAO REPRODUZIVEL** em v1.0.65. Classificado como INFORMATIVO.
Se voltar a ocorrer, investigar:
- Versao exata do binario
- Se `--namespace` foi passado
- Se `graph_export.rs` orphan_edges count e > 0 no stderr com `-vv`

### Impacto Real Verificado

- O export JSON funciona corretamente no v1.0.65
- 2674 edges com from/to populados
- NDJSON format tambem funciona corretamente
- Nenhuma acao necessaria a menos que reproduzido


## HIGH-11 — graph entities --json NAO expoe campo description (70% das entidades parecem sem descricao)

### Problema

O comando `graph entities --json` retorna campos `id`, `name`, `entity_type`, `namespace`, `created_at`, `degree` — mas NAO inclui `description` apesar de:
1. A tabela `entities` no SQLite TER coluna `description TEXT` (nullable)
2. O storage layer (`entities.rs:25`) TER `pub description: Option<String>` no struct
3. O comando `reclassify --description` PERMITIR editar descricoes de entidades
4. O comando `enrich --operation entity-descriptions` TER SIDO criado especificamente para gerar descricoes

Igualmente, `graph --format json` retorna nós com `id`, `name`, `namespace`, `kind`, `type` — sem `description`.

### Consequencias

- Auditoria de qualidade do grafo retornou "5902 entidades sem descricao" quando na realidade 1752 (29.8%) JA TEM descricoes (vindas da ingestao via Codex CLI) — diagnostico inflado em 42%
- LLM agents que consomem `graph entities --json` para montar contexto de prompt NAO recebem descricoes, perdendo informacao semantica valiosa
- O `enrich --operation entity-descriptions` gera descricoes que ficam invisiveis na API de leitura — write sem read correspondente
- Usuarios nao sabem quais entidades ja tem descricao e quais precisam de enrich, causando trabalho duplicado
- O schema `graph-entities.schema.json` NAO inclui `description` nos required/properties, reforçando a omissao

### Causa Raiz

O SELECT SQL em `graph entities` escolhe apenas um subconjunto de colunas da tabela:
```sql
SELECT id, name, type, namespace, created_at FROM entities WHERE ...
```

O campo `description` nao esta no SELECT nem no struct de serialização do JSON response. A omissao provavelmente foi intencional para reduzir payload (descricoes podem ser longas), mas cria uma lacuna de visibilidade critica para auditoria e contexto LLM.

### Solucao

**S1 — Incluir description por default (RECOMENDADA, simples):**

Adicionar `description` ao SELECT e ao struct serializado:
```rust
// Em graph_export.rs ou equivalente, no struct EntityOut
pub description: Option<String>,
```

Trade-off: aumenta payload; entidades com descricoes longas (100+ chars) inflam o JSON. Mitigavel com truncamento.

**S2 — Flag --with-descriptions (OPT-IN):**

Adicionar flag que inclui descricoes quando solicitado:
```bash
sqlite-graphrag graph entities --with-descriptions --json
```

Beneficio: payload default enxuto, opt-in para contexto completo. Consistente com `--with-bodies` no deep-research.

**S3 — Truncar descricoes no listing, full no read (COMPLEMENTAR):**

No `graph entities`, truncar descricao para ~100 chars com `...` no final. Para descricao completa, usar um futuro `read-entity --name X --json`.

### Beneficios

- S1/S2: auditorias de qualidade produzem numeros corretos (4120 vs 5902 sem descricao)
- S1/S2: LLM agents recebem contexto semantico das entidades, melhorando qualidade de traversals e prompts
- S1/S2: visibilidade do trabalho do `enrich` — usuarios veem resultado do investimento em descricoes
- S2: padrao consistente com `deep-research --with-bodies`

### Como Solucionar

**v1.0.66 (S1 ou S2):**
1. Adicionar `description: Option<String>` ao struct de entity listing
2. Adicionar `description` ao SELECT SQL em `src/commands/graph_export.rs` (para graph entities)
3. Atualizar `docs/schemas/graph-entities.schema.json` com campo `description` opcional
4. Se S2: adicionar flag `--with-descriptions` ao Clap args
5. Complexidade: baixa (~10 linhas para S1, ~20 para S2)


## HIGH-10 — 20 memorias tipo decision sao sub-chunks de rules + CLI nao oferece edit --type

### Problema

Duas facetas interligadas:

**Faceta 1 — Dados: 20 memorias com tipo errado:**

20 memorias tipo `decision` NAO sao decisoes arquiteturais — sao pedacos de arquivos de rules ingeridos com tipo errado. O tipo `decision` deveria ser reservado EXCLUSIVAMENTE para registros com: alternativas avaliadas, alternativa escolhida, justificativa e trade-offs.

Memorias afetadas (4 categorias):
- SCRAPING (7): rules-rust-scraping-* — regras operacionais de CSS selectors, proxy, paginacao
- NATIVE CRATES (4+ confirmadas): rules-rust-native-crates-* — listas de substituicao de CLIs
- IMAGENS (1): rules-rust-imagens-fixes-2026-05 — fixes pontuais
- SSH (1): rules-rust-ssh-expansion — registro de expansao

Diferenca estrutural:
- Decision REAL (ex: decisao-anyhow-binarios-thiserror-libs): contexto + alternativas + escolha + justificativa + trade-offs
- Sub-chunk (ex: rules-rust-scraping-css-selectors-xpath): lista imperativa de regras sem alternativas nem justificativa

**Faceta 2 — CLI: nao existe comando para mudar tipo de memoria:**

O comando `edit` aceita `--body`, `--description`, `--body-file`, `--body-stdin` mas NAO aceita `--type`. O comando `reclassify` opera em ENTIDADES, nao em memorias. Nao existe `reclassify-memory` nem `edit --type`.

### Consequencias

- `list --type decision` retorna ~163 memorias, ~20 (12%) NAO contem decisoes arquiteturais
- Signal-to-noise ratio do tipo decision cai para ~88%
- LLM agents que buscam "quais decisoes tecnicas foram tomadas" recebem regras operacionais misturadas
- Entidades-espelho inflam o grafo: rules-rust-native-crates (grau 31) e hub por ser fragmento de rules, nao por representar decisao real
- graph traverse passa por esses nos pensando que sao decisoes arquiteturais
- Agente futuro reconstruindo historico de decisoes infere que "proibir Command::new()" e decisao no mesmo nivel de "usar anyhow em binarios"

### Causa Raiz

**Faceta 1 — Ingestao com tipo errado:**
Os arquivos de rules foram ingeridos com `--type decision` quando o tipo correto seria `skill` ou `document`. A ingestao via `ingest --mode claude-code` ou `remember` nao valida se o conteudo corresponde semanticamente ao tipo declarado.

**Faceta 2 — Ausencia de edit --type no CLI:**
O storage layer (`memories.rs:215`) JA atualiza o campo `type` no SQL (`UPDATE SET type=?2`) durante force-merge. Porem o comando `edit` nao expoe essa capacidade — nao aceita `--type` como flag.

### Solucao

**S1 — Workaround IMEDIATO (funciona HOJE, verificado empiricamente):**

`remember --force-merge` JA muda o tipo quando `--type` e passado:

```bash
echo '{"body":"","entities":[],"relationships":[]}' | \
  sqlite-graphrag remember \
    --name rules-rust-scraping-css-selectors-xpath \
    --type skill \
    --description "mesma descricao" \
    --force-merge --graph-stdin --json
```

Verificado: `read --name type-test` mostra `memory_type: "skill"` apos force-merge com `--type skill`.

Para as 20 memorias, um loop:
```bash
for name in rules-rust-scraping-css-selectors-xpath rules-rust-native-crates-principle ...; do
  CURRENT=$(sqlite-graphrag read --name "$name" --json | jaq -r '{d: .description, b: .body}')
  DESC=$(echo "$CURRENT" | jaq -r '.d')
  echo "{\"body\":$(echo "$CURRENT" | jaq '.b'),\"entities\":[],\"relationships\":[]}" | \
    sqlite-graphrag remember --name "$name" --type skill --description "$DESC" --force-merge --graph-stdin --json
done
```

**S2 — Adicionar `edit --type` ao CLI (RECOMENDADA, retrocompativel):**

Adicionar flag `--type` ao comando `edit`:

```rust
// em DeepResearchArgs ou EditArgs
#[arg(long, value_enum, help = "Change memory type")]
pub memory_type: Option<MemoryType>,
```

O storage layer JA suporta — basta propagar o campo do CLI para `update_memory()`.

Beneficio: operacao atomica de 1 passo vs 3+ passos do workaround force-merge.

**S3 — Adicionar `reclassify-memory` em batch (FUTURA):**

Comando dedicado para reclassificacao em massa:
```bash
sqlite-graphrag reclassify-memory --from-type decision --to-type skill --filter-name "rules-rust-*" --batch --json
```

Seguindo padrao de `reclassify-relation` (batch + dry-run + filters).

**S4 — Validacao semantica de tipo na ingestao (PREVENTIVA):**

Na ingestao com `--type decision`, validar que o body contem marcadores de decisao (alternativas, justificativa, trade-offs). Se nao contem, emitir `tracing::warn!("body does not match decision pattern, consider using --type skill or --type document")`.

### Beneficios

- S1: resolve os 20 casos HOJE sem mudanca de codigo
- S2: operacao intuitiva para usuarios e LLM agents (`edit --type` e previsivel)
- S3: correcao em massa para futuros problemas de tipagem
- S4: previne recorrencia do problema na fonte

### Como Solucionar

**Imediato (dados, S1):**
1. Listar as 20 memorias afetadas via `list --type decision --json | jaq '.items[] | select(.name | startswith("rules-rust")) | .name'`
2. Para cada uma, executar `remember --force-merge --type skill` preservando body e description
3. Verificar com `list --type decision` que count diminuiu
4. Complexidade: zero codigo, ~20 invocacoes CLI

**v1.0.66 (CLI, S2):**
5. Adicionar `--type` opcional ao `EditArgs` struct em `src/commands/edit.rs`
6. Propagar para `update_memory()` em `src/storage/memories.rs` (ja aceita type no SQL)
7. Adicionar teste: edit --type muda tipo sem alterar body
8. Complexidade: baixa (~15 linhas)

**v1.1.0 (batch, S3):**
9. Novo comando `reclassify-memory` com padrao de reclassify-relation
10. Complexidade: media (~150 linhas, reutiliza padrao)


## MEDIUM-01 — deep-research evidence_chains vazio (PROMOVIDO para HIGH-01)

- Este gap foi promovido para HIGH-01 CRITICAL apos analise de causa raiz
- Ver HIGH-01 acima para descricao completa do seed entity flooding


## MEDIUM-01b — deep-research: sem contexto de grafo na saida JSON

### Problema

O deep-research retorna memorias nos `results[]` mas NAO retorna as entidades nem relacoes que conectam essas memorias — apenas nos `evidence_chains[]` (que sao sempre vazios por HIGH-01).

### Consequencias

- O LLM consumidor recebe resultados isolados sem saber COMO se relacionam
- A riqueza do grafo (entidades + relacoes tipadas) e completamente perdida na saida
- `graph traverse` retorna essa informacao, mas deep-research nao a inclui

### Solucao

Adicionar campo `graph_context` na resposta com:
- Top-k entidades por degree relevantes aos resultados
- Relacoes entre essas entidades
- Permite ao consumidor entender a topologia do conhecimento

### Como Solucionar

1. Apos RRF fusion, coletar entity_ids das memorias retornadas
2. Query relacoes entre esses entity_ids
3. Adicionar campo `graph_context: {entities: [...], relationships: [...]}` no JSON
4. Complexidade: baixa (~20 linhas)


## MEDIUM-02 — history --diff changes=null na primeira versao

- Primeira versao retorna `changes: null` em vez de `changes: {added_chars: N, removed_chars: 0}`
- Impacto: cosmetico — versao 1 nao tem versao anterior para comparar, null e semanticamente correto
- Sugestao: considerar retornar `{added_chars: body.len(), removed_chars: 0}` para versao 1 como baseline


## MEDIUM-03 — documentacao desatualizada sobre uppercase em nomes

- Comportamento real: nomes uppercase sao auto-normalizados para lowercase (v1.0.65 feature)
- Documentacao CLAUDE.md: implica rejeicao com exit 1
- Impacto: zero funcional — comportamento correto; documentacao enganosa para quem espera rejeicao


## MEDIUM-04 — documentacao source enum: "decomposed" vs "decomposition"

- Valor real no codigo: sub_queries[].source = "decomposed"
- Valor na documentacao CLAUDE.md: "decomposition"
- Fix: alinhar documentacao com implementacao


## MEDIUM-05 — performance remember 1.9s sem daemon

- Baseline sem daemon: remember avg 1904ms, recall avg 1480ms, hybrid-search avg 1487ms
- Com daemon ativo: ~200ms esperado (cold-start eliminado)
- Recomendacao: documentar que daemon e FORTEMENTE recomendado para uso interativo
- Nota: latencia aceitavel para scripts batch; inaceitavel para uso interativo de agente


## MEDIUM-06 — daemon ping exit 4 sem daemon ativo

- Comando: `sqlite-graphrag daemon --ping --json` sem daemon ativo retorna exit 4 (NotFound)
- Comportamento aceitavel mas poderia retornar exit code mais especifico ou JSON informativo
- Impacto: cosmético


## LOW-01 — sem fuzzing formal (cargo-fuzz) configurado

- rules_rust_testes.md linha 866: "APLICAR fuzzing a parsers, decoders e deserializacao"
- O projeto NAO tem diretorio `fuzz/` nem targets cargo-fuzz
- Alvos candidatos: graph-stdin JSON parsing, name validation, body processing, NDJSON ingest parsing
- Impacto: crash BUG-05 (bytes invalidos) teria sido encontrado mais cedo com fuzzing


## LOW-02 — sem mutation testing configurado

- rules_rust_testes.md linha 1091: "APLICAR cargo-mutants periodicamente em codigo critico"
- O projeto NAO tem cargo-mutants configurado
- Impacto: test suite pode ter cobertura alta mas baixa eficacia de deteccao


## LOW-03 — coverage threshold NAO enforced no CI

- ci.yml gera lcov.info mas NAO falha o build se cobertura cair abaixo de threshold
- rules_rust_testes.md linha 1005: "FALHAR build ao cair cobertura abaixo do threshold"
- Fix: adicionar `--fail-under-lines 80` ou equivalente no job coverage


## Resumo por Severidade

| Severidade | Count | IDs |
|---|---|---|
| CRITICAL | 2 | BUG-01, HIGH-01 (seed flooding) |
| BUG-HIGH | 1 | BUG-06 (link weight not updated + JSON lies) |
| HIGH | 16 | BUG-02, HIGH-01b, HIGH-02..HIGH-13, HIGH-10b, HIGH-10c |
| MEDIUM | 11 | BUG-03, BUG-04, MEDIUM-01b, MEDIUM-02..09 |
| LOW | 5 | BUG-05, LOW-01, LOW-02, LOW-03, LOW-04 |
| Total | 35 | |


## Testes Bloqueados (dependem de fix)

| Teste | Bloqueador |
|---|---|
| RC02 reclassify-relation execute | BUG-01 (updated_at) |
| RC06 collision merge | BUG-01 |


## Resultados por Categoria

| Categoria | Total | PASS | FAIL | BLOCKED | SKIP |
|---|---|---|---|---|---|
| Cat 1 Smoke | 52 | 50 | 2 | 0 | 0 |
| Cat 2 New Commands | 30 | 28 | 0 | 2 | 0 |
| Cat 3 GAPs | 30 | 21 | 7 | 0 | 2 |
| Cat 4 Regressions | 8 | 8 | 0 | 0 | 0 |
| Cat 5 Edge Cases | 15 | 12 | 2 | 0 | 1 |
| Cat 6 Lifecycle | 2 | 2 | 0 | 0 | 0 |
| Cat 7 Error Paths | 8 | 8 | 0 | 0 | 0 |
| Cat 8 Security | 6 | 6 | 0 | 0 | 0 |
| Cat 9 Performance | 5 | 5 | 0 | 0 | 0 |
| Cat 11 Explore | 3 | 2 | 1 | 0 | 0 |
| Cat 12 i18n | 4 | 4 | 0 | 0 | 0 |
| Cat 13 Fuzzing | 2 | 1 | 1 | 0 | 0 |
| Total | 165 | 147 | 13 | 2 | 3 |


## Performance Baselines (sem daemon)

| Operacao | Latencia Media | Threshold |
|---|---|---|
| remember | 1904ms | <2000ms PASS |
| recall | 1480ms | <2000ms PASS |
| hybrid-search | 1487ms | <2000ms PASS |
| deep-research | 1427ms | <15000ms PASS |
| list | 5ms | <1000ms PASS |


## Prioridade de Correcao Sugerida

- P0: BUG-01 (reclassify-relation inutilizavel), HIGH-01 (seed flooding → evidence chains sempre vazias, ~5 linhas fix), BUG-06 (link nao atualiza peso + JSON mente, ~10 linhas)
- P1: BUG-02 (link normalization), BUG-05 (crash UTF-8), HIGH-05 (jaq exit 5 cascata), HIGH-08 (source sempre "knn", ~10 linhas)
- P2: HIGH-07 (logging insuficiente, ~15 linhas), HIGH-01b (resultados vazios, threshold tuning), MEDIUM-01b (graph context na saida)
- P3: HIGH-06 (--mode claude-code/codex, ~400 linhas) + HIGH-09 (--max-cost-usd obrigatorio junto com HIGH-06), HIGH-04 (degree warning)
- P3b: HIGH-10 dados (reclassificar 20 memorias via force-merge, zero codigo, ~20 invocacoes CLI)
- P4: HIGH-10 CLI (edit --type, ~15 linhas), HIGH-03 (docs debug-schema), MEDIUM-02..06 (docs alignment)
- P5: LOW-01/02/03 (tooling: fuzzing, mutation, coverage gate)
