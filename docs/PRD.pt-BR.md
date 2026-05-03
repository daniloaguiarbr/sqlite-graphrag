# PRD — sqlite-graphrag v2.1.0

> **Fonte de verdade.** Este documento define os contratos que qualquer versão DEVE satisfazer.
> A conformidade comportamental é verificada por `tests/prd_compliance.rs` (habilitado via `--features slow-tests`).

---

## 1. Regras de Nomenclatura

### 1.1 Nomes de memória com prefixo `__` DEVEM ser rejeitados (saída 1)
- Justificativa: o prefixo `__` é um namespace sentinela reservado; armazenar conteúdo do usuário sob ele colide com chaves internas.
- Teste: `prd_name_double_underscore_rejected`
- Validação: `sqlite-graphrag remember --name __reserved --type user --description "x" --body "y"` → saída 1

---

## 2. Operações entre Namespaces

### 2.1 `link` entre namespaces DEVE falhar com saída 4 (entidade não encontrada)
- Justificativa: entidades são escopadas por namespace; referenciar um alvo inexistente em outro namespace deve resultar em erro de entidade não encontrada, não em corrupção silenciosa.
- Teste: `prd_cross_namespace_link_rejected`
- Validação: `sqlite-graphrag link --from <existente> --to <inexistente> --relation related --namespace global` → saída 4

---

## 3. Soft-delete e Restauração

### 3.1 Memórias esquecidas NÃO DEVEM aparecer em consultas ativas
- Justificativa: o soft-delete preenche `deleted_at`; qualquer consulta que filtre `WHERE deleted_at IS NULL` deve excluir registros soft-deletados.
- Teste: `prd_soft_delete_recall_does_not_return_forgotten`
- Validação: após `forget`, `SELECT COUNT(*) FROM memories WHERE name=<nome> AND deleted_at IS NULL` → 0; `deleted_at IS NOT NULL` → 1

### 3.2 O trigger FTS `trg_fts_ad` DEVE ser idempotente em dupla exclusão
- Justificativa: uma segunda tentativa de exclusão em uma linha FTS já removida não deve corromper o índice FTS nem gerar erro.
- Teste: `prd_trg_fts_ad_idempotent_double_delete`
- Validação: após soft-delete manual + `DELETE FROM fts_memories WHERE rowid=<id>`, `INSERT INTO fts_memories(fts_memories) VALUES('integrity-check')` → sucesso (sem erro)

### 3.3 `restore` DEVE reverter uma memória soft-deletada para estado ativo
- Justificativa: `restore` é o caminho de recuperação para um `forget` acidental; a memória deve ter `deleted_at = NULL` após um restore bem-sucedido.
- Teste: `prd_restore_reverte_soft_delete`
- Validação: `sqlite-graphrag forget --name <nome> --namespace global` e depois `sqlite-graphrag restore --name <nome> --namespace global --version <v>` → `deleted_at IS NULL`

### 3.4 `purge --retention-days N` DEVE remover permanentemente memórias soft-deletadas com mais de N dias
- Justificativa: purge é o mecanismo de exclusão permanente; registros com `deleted_at` mais antigo que a janela de retenção devem ser removidos completamente da tabela `memories`.
- Teste: `prd_purge_retention_removes_old_soft_deleted`
- Validação: `sqlite-graphrag purge --retention-days 1 --yes` com memória soft-deletada há 2 dias → `SELECT COUNT(*) FROM memories WHERE name=<nome>` → 0

---

## 4. Versionamento e Histórico

### 4.1 `remember` com `--force-merge` DEVE incluir `merged_into_memory_id` no JSON de saída
- Justificativa: os chamadores precisam detectar se um novo registro foi criado ou mesclado em um existente; o campo sinaliza a semântica de merge.
- Teste: `prd_remember_duplicate_returns_merged_into_memory_id`
- Validação: `sqlite-graphrag remember --name <existente> ... --force-merge` → JSON na saída padrão contém a chave `merged_into_memory_id`

### 4.2 O JSON de saída do `remember` DEVE incluir `entities_persisted` e `relationships_persisted`
- Justificativa: esses campos permitem que os chamadores confirmem que a extração NER produziu artefatos de grafo sem consultar o banco de dados diretamente.
- Teste: `prd_remember_json_contains_entities_and_relationships_persisted`
- Validação: `sqlite-graphrag remember ... --skip-extraction` → JSON na saída padrão contém as chaves `entities_persisted` e `relationships_persisted`

### 4.3 A saída de `history` DEVE incluir `created_at_iso` em cada entrada de versão
- Justificativa: timestamps ISO 8601 em entradas de histórico permitem que os chamadores correlacionem versões com eventos em tempo real sem conversão de epoch.
- Teste: `prd_history_includes_created_at_iso`
- Validação: `sqlite-graphrag history --name <nome> --namespace global` → `json["versions"][0]["created_at_iso"]` presente

### 4.4 `rename` DEVE atualizar a versão da memória em `memory_versions`
- Justificativa: um rename é uma mudança semântica; a tabela de versionamento deve registrá-lo para que o histórico reflita a transição.
- Teste: `prd_rename_updates_version`
- Validação: `sqlite-graphrag rename --name <orig> --new-name <novo> --namespace global` → `SELECT COUNT(*) FROM memory_versions WHERE name=<novo> OR name=<orig>` ≥ 1; memória existe com o novo nome

---

## 5. Operações de Grafo

### 5.1 `link` DEVE criar uma entrada em `memory_relationships` ou `relationships`
- Justificativa: um link bem-sucedido deve ser persistido para que o grafo seja consultável; nenhuma operação silenciosa sem efeito é permitida.
- Teste: `prd_link_creates_memory_relationships`
- Validação: `sqlite-graphrag link --from <src> --to <dst> --relation related --namespace global` (caminho de sucesso) → `COUNT(*) FROM memory_relationships` > 0 OU `COUNT(*) FROM relationships` > 0

### 5.2 `unlink` DEVE remover apenas a relação especificada, preservando as demais
- Justificativa: unlink é uma operação direcionada; remover uma aresta não deve cascatear para outras arestas da mesma origem.
- Teste: `prd_unlink_removes_only_specific_relation`
- Validação: com arestas A→B e A→C, `sqlite-graphrag unlink --from ent-a --to ent-b --relation related --namespace global` → `COUNT(*) FROM relationships WHERE source_id=A` = 1

### 5.3 A saída de `graph --format json` DEVE conter os campos `nodes` e `edges`
- Justificativa: consumidores downstream (visualizadores, agentes) dependem de um esquema JSON estável com essas duas chaves de nível superior.
- Teste: `prd_graph_json_contains_nodes_and_edges`
- Validação: `sqlite-graphrag graph --format json --namespace global` → JSON contém `nodes` e `edges`

### 5.4 A saída de `graph --format dot` DEVE começar com `digraph sqlite-graphrag {`
- Justificativa: o cabeçalho do formato DOT é necessário para qualquer consumidor Graphviz compatível; desviar quebra integrações de ferramentas.
- Teste: `prd_graph_dot_is_valid_digraph`
- Validação: `sqlite-graphrag graph --format dot --namespace global` → saída padrão contém `digraph sqlite-graphrag {`

### 5.5 A saída de `graph --format mermaid` DEVE conter `graph LR`
- Justificativa: a diretiva Mermaid `graph LR` sinaliza um grafo direcionado da esquerda para a direita; os renderizadores exigem exatamente esse token.
- Teste: `prd_graph_mermaid_starts_with_graph_lr`
- Validação: `sqlite-graphrag graph --format mermaid --namespace global` → saída padrão contém `graph LR`

### 5.6 `cleanup-orphans` DEVE remover entidades que não possuem memórias associadas
- Justificativa: entidades órfãs inflam o grafo sem contribuir para o recall; cleanup-orphans deve removê-las e reportar a contagem.
- Teste: `prd_cleanup_orphans_removes_entities_without_memories`
- Validação: `sqlite-graphrag cleanup-orphans --yes` com uma entidade órfã → `json["deleted"]` ≥ 1; entidade ausente da tabela `entities`

---

## 6. Busca e Recall

### 6.1 O índice FTS5 DEVE usar o tokenizador `unicode61 remove_diacritics`
- Justificativa: busca insensível a diacríticos (ex.: "nao" correspondendo a "não") é uma funcionalidade documentada; o tokenizador deve ser configurado adequadamente.
- Teste: `prd_fts5_unicode61_remove_diacritics`
- Validação: `SELECT sql FROM sqlite_master WHERE name='fts_memories'` → contém `unicode61` ou `remove_diacritics`

### 6.2 `hybrid-search` DEVE aceitar o argumento `--rrf-k` (padrão 60)
- Justificativa: k=60 do RRF é o padrão documentado para Reciprocal Rank Fusion; o argumento deve ser aceito sem erro.
- Teste: `prd_hybrid_search_rrf_k_default_60`
- Validação: `sqlite-graphrag hybrid-search "query" --rrf-k 60 --namespace global` → saída 0

---

## 7. Armazenamento Vetorial

### 7.1 A tabela `vec_memories` DEVE declarar `distance_metric=cosine`
- Justificativa: a distância de cosseno é a métrica correta para similaridade de embeddings semânticos; usar um padrão diferente degradaria silenciosamente a qualidade do recall.
- Teste: `prd_vec_memories_distance_metric_cosine`
- Validação: `SELECT sql FROM sqlite_master WHERE name='vec_memories'` → contém `cosine`

---

## 8. Otimização e Manutenção

### 8.1 `edit` com `--expected-updated-at` desatualizado DEVE retornar saída 3 (Conflito)
- Justificativa: o controle de concorrência otimista exige que edições baseadas em um snapshot desatualizado sejam rejeitadas; saída 3 é o código de conflito designado.
- Teste: `prd_edit_expected_updated_at_stale_returns_exit_3`
- Validação: `sqlite-graphrag edit --name <nome> --namespace global --body "x" --expected-updated-at 0` → saída 3

### 8.2 `optimize` DEVE finalizar com sucesso e retornar `{"status": "ok"}`
- Justificativa: o comando optimize executa manutenção de FTS e vetorial; os chamadores devem ser capazes de detectar a conclusão via o campo status.
- Teste: `prd_optimize_runs_and_returns_status_ok`
- Validação: `sqlite-graphrag optimize` → saída 0; `json["status"]` = `"ok"`

### 8.3 `vacuum` DEVE retornar `size_before_bytes` e `size_after_bytes`
- Justificativa: os chamadores precisam medir o espaço recuperado; ambos os campos são necessários para comparação antes/depois.
- Teste: `prd_vacuum_returns_size_before_and_size_after`
- Validação: `sqlite-graphrag vacuum` → saída 0; JSON contém `size_before_bytes` e `size_after_bytes`

---

## 9. Configuração e Segurança

### 9.1 No máximo 4 instâncias simultâneas DEVEM ser permitidas; a 5ª DEVE retornar saída 75
- Justificativa: concorrência limitada previne contenção no banco de dados; saída 75 (`EX_TEMPFAIL`) sinaliza ao chamador para tentar novamente mais tarde.
- Teste: `prd_five_instances_fifth_returns_exit_75`
- Validação: com 4 slots de lock ocupados, `sqlite-graphrag --max-concurrency 4 --wait-lock 0 namespace-detect` → saída 75

### 9.2 O tamanho do corpo NÃO DEVE exceder 512.000 bytes; violações DEVEM retornar saída 6
- Justificativa: um corpo sem limite esgotaria a memória de FTS e embedding; o limite rígido protege a estabilidade do runtime.
- Teste: `prd_max_body_len_exceeded_returns_exit_6`
- Validação: `sqlite-graphrag remember ... --body-file <arquivo-de-512001-bytes>` → saída 6

### 9.3 A variável de ambiente `SQLITE_GRAPHRAG_NAMESPACE` DEVE ser aceita como namespace padrão
- Justificativa: pipelines de scripts definem o namespace uma vez via ambiente; a CLI deve respeitá-lo sem exigir flag por comando.
- Teste: `prd_sqlite_graphrag_namespace_env_works`
- Validação: `remember --namespace ns-from-env` → `SELECT namespace FROM memories WHERE name=<nome>` = `ns-from-env`

### 9.4 O arquivo de banco de dados DEVE ter permissões `600` (somente leitura/escrita do proprietário) após `init` no Unix
- Justificativa: o banco de dados armazena conteúdo de memória sensível; permissões legíveis por todos seriam uma violação de confidencialidade.
- Teste: `prd_chmod_600_aplicado_apos_init` (`#[cfg(unix)]`)
- Validação: `sqlite-graphrag init` → `stat <db>` → `mode & 0o777` = `0o600`

### 9.5 Path traversal (`..`) em `SQLITE_GRAPHRAG_DB_PATH` DEVE ser rejeitado
- Justificativa: aceitar caminhos contendo `..` permitiria que a CLI escrevesse fora do diretório pretendido, uma vulnerabilidade de path traversal.
- Teste: `prd_path_traversal_rejected_in_db_path`
- Validação: `SQLITE_GRAPHRAG_DB_PATH=../../../etc/passwd sqlite-graphrag init` → saída não-zero

---

## 10. Saída e Diagnósticos

### 10.1 `health` DEVE retornar JSON com os campos `integrity_ok` e `schema_ok`
- Justificativa: verificações de saúde são consumidas por agentes de monitoramento; esses dois campos booleanos são o contrato documentado para liveness e conformidade de esquema.
- Teste: `prd_health_emits_integrity_ok_and_schema_ok`
- Validação: `sqlite-graphrag health` → saída 0; JSON contém `integrity_ok` e `schema_ok`

### 10.2 `stats` DEVE retornar JSON com os campos `memories`, `entities` e `relationships`
- Justificativa: stats é o principal comando de observabilidade; os três contadores fornecem uma visão geral do estado do grafo.
- Teste: `prd_stats_inclui_memories_entities_relationships`
- Validação: `sqlite-graphrag stats` → saída 0; JSON contém `memories`, `entities`, `relationships` (e opcionalmente `memories_total`)

### 10.3 `list` DEVE respeitar o argumento `--limit`
- Justificativa: a paginação exige que o limite do lado servidor seja aplicado; retornar mais itens do que o solicitado quebra consumidores downstream.
- Teste: `prd_list_respeita_limit`
- Validação: com 5 memórias armazenadas, `sqlite-graphrag list --namespace global --limit 2` → `json["items"].length` = 2

### 10.4 `sync-safe-copy` DEVE produzir um snapshot coerente com `bytes_copied > 0` e `status = "ok"`
- Justificativa: a integridade do backup depende de o snapshot ser uma cópia consistente; `bytes_copied` confirma que os dados foram gravados e `status` confirma a ausência de erros.
- Teste: `prd_sync_safe_copy_generates_coherent_snapshot`
- Validação: `sqlite-graphrag sync-safe-copy --dest <caminho>` → saída 0; JSON contém `bytes_copied` > 0 e `status = "ok"`; arquivo de destino existe
