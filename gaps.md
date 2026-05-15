# sqlite-graphrag v1.0.52 — CLI Gaps

- Levantamento: 2026-05-15
- Binário: sqlite-graphrag 1.0.52 (crates.io)
- DB: 448 memories, 2157 entities, 3997 relationships, schema v11, ~597 MB
- Acceptance testing v1.0.51: 72/72 PASS (100%)
- Somente gaps CLI — gaps de CI/GitHub/release workflow estão fora de escopo


## CRITICAL (2)

### C1 — RSS explosion no pipeline remember/ingest (MITIGADO, NÃO CORRIGIDO)
- Um único remember com arquivo grande cresceu para 52-55 GiB de RSS (incidente 2026-04-23)
- earlyoom enviou SIGTERM em 3 execuções distintas, desktop Fedora 44 travou por thrashing
- Causa raiz: leak de memória no ONNX Runtime durante embedding de chunks, NÃO depende de paralelismo
- MITIGAÇÃO v1.0.51: --max-rss-mb (default 8192) aborta entre chunks quando RSS excede o threshold
- Mitigação NÃO resolve o leak em si — apenas limita o dano
- Arena allocator desativado (embedder.rs:42), ORT_DISABLE_CPU_MEM_ARENA=1 (main.rs:53-59)
- Reproduzir SOMENTE com isolamento forte: systemd-run --scope com MemoryMax
- Bloqueado por D2 (ort 2.0.0 stable pode trazer fix upstream)

### C2 — mentions domina 86.3% do grafo (PARCIALMENTE CORRIGIDO)
- 3455 de 3997 relationships são do tipo "mentions"
- Grafo degrada para quase-monocromo — recall perde sinal por ruído
- NER automático (quando ativo) e agentes que não curam relações geram mentions como default
- prune-relations --relation mentions --dry-run confirma 3455 candidatos
- MITIGAÇÃO v1.0.52: health warning adicionado quando mentions > 80% do grafo
- Problema de disciplina de curadoria persiste — não é bug de código
- Agentes devem usar SOMENTE relações canônicas curadas: applies-to, uses, depends-on, causes, fixes, contradicts, supports, follows, related, replaces, tracked-in
- mentions deve ser reservado para citações explícitas, NUNCA como catch-all


## DEPENDENCIES — Bloqueados por upstream (2)

### D1 — rusqlite pinado em 0.37 (upstream: 0.39 disponível)
- refinery 0.9.1 constrains rusqlite a ">=0.23, <=0.38"
- Upgrade para 0.39 requer nova release de refinery que relaxe o constraint
- Impacto: sem acesso a melhorias de performance e segurança do rusqlite 0.38-0.39
- Rastreado desde v1.0.41

### D2 — ort pinado em =2.0.0-rc.12 (upstream: ort 2.0.0 stable não lançado)
- ort está em release candidate desde 2025, sem data para 2.0.0 stable
- Afeta C1: fix de leak de memória no ONNX Runtime pode vir com ort 2.0.0 stable
- Afeta targets: x86_64-apple-darwin e x86_64-unknown-linux-musl excluídos por falta de prebuilt ONNX
- Pin exato =2.0.0-rc.12 impede cargo update nessa dependência


## RESOLVIDOS na v1.0.52

| ID | Descrição | Status |
|----|-----------|--------|
| H1 | restore JSON não retorna campo "action" | CORRIGIDO — adicionado action: "restored" |
| H2 | i18n PT traduz prefixo mas corpo fica em inglês | CORRIGIDO — substituição completa da string em app_error_pt |
| H3 | ingest com espaços e caracteres especiais no nome | CORRIGIDO — campo original_filename no NDJSON |
| M1 | ingest exit 14 para diretório inexistente | CORRIGIDO — agora exit 1 (Validation) |
| M2 | forget retorna JSON E stderr para memória não encontrada | CORRIGIDO (BREAKING) — sem JSON no stdout em caso not-found |
| M3 | Vec::new() overallocation em 48 ocorrências | CORRIGIDO — 7 with_capacity() em hot paths |
| M5 | Nomes truncados no ingest sem aviso claro ao agente | CORRIGIDO — flag --dry-run adicionada ao ingest |
| L1 | Clap exit code 2 vs app exit code 2 para Duplicate | CORRIGIDO (BREAKING) — Duplicate movido para exit 9 |
| L2 | prune-relations sem opção de verbose para inspecionar arestas | CORRIGIDO — flag --show-entities adicionada (renomeada de --verbose por conflito com flag global do Clap) |
| L4 | Nenhum mecanismo de export além de sync-safe-copy | CORRIGIDO — novo subcomando export |

### Detalhes dos itens resolvidos

**H1 — restore JSON action field**
- Todos os outros comandos CRUD (edit, rename, forget, remember) retornam {"action": "..."} no JSON
- restore retornava {memory_id, name, version, restored_from, elapsed_ms} — sem campo "action"
- Agentes que esperavam action="restored" recebiam null
- Descoberto na v1.0.50, confirmado na v1.0.51 (acceptance testing F8 T086)
- Impacto: agentes orquestradores que parseavam .action para routing falhavam silenciosamente
- FIX: adicionado action: "restored" ao JSON response de restore

**H2 — i18n PT partial**
- --lang pt produzia: "Erro: não encontrado: memory 'X' not found in namespace 'Y'"
- O prefixo "Erro:" e "não encontrado:" eram traduzidos, mas o corpo "memory...not found..." ficava em inglês
- Inconsistente com a promessa de --lang pt
- Afetava stderr de: read, edit, forget, history, rename, restore
- FIX: substituição completa da string em app_error_pt — corpo agora integralmente em PT

**H3 — ingest spaces/special chars**
- Arquivos com espaços no nome impossibilitavam uso previsível do sqlite-graphrag ingest
- CLI converte basename para kebab-case: "file with spaces.md" → "file-with-spaces"
- "arquivo com acentuação.md" → "arquivo-com-acentuacao" (acentos removidos)
- "[draft] notes.md" → "draft-notes" (colchetes removidos)
- "file (1).md" → "file-1" (parênteses e espaços removidos)
- Problemas: perda de informação no nome, colisões silenciosas, impossível reverse-map
- NDJSON retornava campo "file" com path original, mas "name" já era kebab normalizado
- FIX: campo original_filename adicionado ao NDJSON por arquivo

**M1 — ingest exit 14**
- `sqlite-graphrag ingest /nonexistent --json` retornava exit 14 (I/O error)
- Documentação define exit 14 como "I/O error (arquivo inacessível, permissão, disco cheio)"
- Diretório inexistente é erro de VALIDAÇÃO de input, deveria ser exit 1
- FIX: agora retorna exit 1 (Validation) com mensagem "directory not found: /nonexistent"

**M2 — forget double output**
- `sqlite-graphrag forget nonexistent --json` retornava:
  - stdout: {"action":"not_found","forgotten":false,...} (exit 4)
  - stderr: "Erro: não encontrado: ..."
- Outros comandos com exit 4 retornam SOMENTE stderr (sem JSON)
- FIX (BREAKING): forget não-encontrado agora retorna apenas exit 4 + stderr, sem JSON em stdout

**M3 — Vec::new() overallocation**
- 48 ocorrências de Vec::new() no src/, maioria em código de teste
- Vec::new() sem Vec::with_capacity() desperdiça allocations quando tamanho é previsível
- FIX: 7 with_capacity() substituídos em hot paths de embedding e processamento de chunks

**M5 — ingest no dry-run**
- Nomes com mais de 60 caracteres são truncados automaticamente
- NDJSON inclui truncated:true e original_name, mas agentes precisam saber ANTES de ingerir
- Não havia flag --dry-run no ingest para preview de nomes
- Colisões possíveis: dois arquivos longos com mesmo prefixo de 60 chars → mesmo nome kebab
- FIX: flag --dry-run adicionada ao ingest para preview de nomes sem escrita

**L1 — exit code 2 collision**
- Clap retorna exit 2 para erros de parsing de argumentos CLI (padrão Clap)
- sqlite-graphrag usava exit 2 para AppError::Duplicate
- Colisão: agente não conseguia distinguir "argumento inválido" de "memória duplicada"
- Exemplo: `--entity-type bogus` → exit 2 (Clap), `remember --name existente` → exit 2 (app)
- FIX (BREAKING): AppError::Duplicate movido para exit 9

**L2 — prune no verbose**
- `prune-relations --relation mentions --yes` removeria 3455 edges de uma vez
- --dry-run mostrava count=3455 mas não listava quais entidades seriam afetadas
- Sem opção para inspecionar arestas individuais antes de confirmar
- FIX: flag --show-entities adicionada (renomeada de --verbose devido a conflito com flag global do Clap)

**L4 — no NDJSON export**
- sync-safe-copy cria cópia checkpointed do .sqlite, mas não havia export para JSON/NDJSON
- Para migrar entre máquinas, agente dependia de copiar arquivo binário SQLite
- FIX: novo subcomando export com --format ndjson para dump portável de todas as memórias

## Fechados como falsos positivos na v1.0.52

| ID | Descrição | Resolução |
|----|-----------|-----------|
| M4 | recall --k N retorna mais de N resultados | FECHADO — flag --max-graph-results já existe |
| L3 | graph entities não retorna entity_type no JSON | FECHADO — entity_type já é retornado |

### Detalhes dos itens fechados

**M4 — recall --k explosion**
- `sqlite-graphrag recall "test" --k 1 --json` retornava 393 results (1 direct + 392 graph)
- --k limitava apenas direct_matches, graph_matches era ilimitado
- FECHADO: flag --max-graph-results já existe e limita o total de resultados do grafo

**L3 — graph entities no type**
- `graph entities --json` parecia retornar entities[].name sem entity_type
- FECHADO: entity_type já é retornado no JSON response de graph entities; gap era baseado em versão desatualizada


## RESOLVIDOS na v1.0.51 (para referência)

| ID antigo | Descrição | Status |
|-----------|-----------|--------|
| M7 | remember on soft-deleted → exit 10 | CORRIGIDO — agora retorna exit 2 |
| M8 | namespace env var ignorada por 8 comandos | CORRIGIDO — todos respeitam SQLITE_GRAPHRAG_NAMESPACE |
| M6 | recipe_01_bootstrap timeout em debug | CORRIGIDO — slow-timeout 180s em nextest |
| M3-old | daemon.rs apenas 4 testes | CORRIGIDO — agora 10 testes |
| L1-old | 20/27 subcomandos sem after_long_help | CORRIGIDO — 27/27 têm EXAMPLES |
| L2-old | MIGRATION.md não existia | CORRIGIDO — criado com nota v1.0.51 |
| L3-old | README sem version highlights | CORRIGIDO — seção adicionada |
| L4-old | GLiNER int8 quality gap sem doc | CORRIGIDO — help text documenta trade-off |
| H1-old | warn_if_non_canonical em unlink | CORRIGIDO na v1.0.49 |
| H2-old | warn_if_non_canonical em related | CORRIGIDO na v1.0.49 |
| H6-old | related --help não listava canônicos | CORRIGIDO na v1.0.49 |
| H7-old | daemon auto-restart | CORRIGIDO na v1.0.50 |
| C2-old | graph_export descarte silencioso edges | CORRIGIDO na v1.0.50 — orphan edge warning |


## Resumo por severidade

| Severidade | Abertos | Descrição |
|------------|---------|-----------|
| CRITICAL | 2 | RSS leak (mitigado, bloqueado por D2), mentions 86.3% (parcialmente corrigido) |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 0 | — |
| DEPS | 2 | rusqlite 0.37, ort rc.12 |
| **Total abertos** | **4** | |
| Resolvidos v1.0.52 | 10 | H1, H2, H3, M1, M2, M3, M5, L1, L2, L4 |
| Fechados falso positivo | 2 | M4, L3 |
