# Gaps — sqlite-graphrag CLI


## G41 — `run_rehash` registra V013 como aplicada sem executar o SQL — tabelas BLOB-backed nunca são criadas
### Status: RESOLVIDO (v1.0.78)
### Severidade: CRÍTICA — bloqueia `recall`, `hybrid-search`, `remember` e qualquer operação que dependa de embeddings
### Versões afetadas: v1.0.76, v1.0.77
### Versão da correção: v1.0.78
### Data de identificação: 2026-06-09
### Arquivo raiz: `src/commands/migrate.rs:272-281`


### Problema
- O comando `migrate --rehash` (e `migrate --to-llm-only`) registra V013 em `refinery_schema_history` como "já aplicada" SEM executar o SQL de V013
- O `runner().run()` subsequente vê V013 no histórico e a PULA
- As tabelas `memory_embeddings`, `entity_embeddings` e `chunk_embeddings` NUNCA são criadas
- O banco fica em estado inconsistente: sem tabelas vec (removidas) E sem tabelas BLOB-backed (nunca criadas)
- Toda operação que toca embeddings falha com exit 10: `no such table: memory_embeddings`


### Consequências do Problema
- `hybrid-search` falha com `no such table: memory_embeddings` (exit 10)
- `recall` falha com o mesmo erro — busca vetorial impossível
- `remember` e `edit` falham ao tentar gravar embeddings — novas memórias não são indexáveis
- `ingest --mode claude-code` falha ao tentar inserir embeddings extraídos
- `health --json` reporta `vec_memories_ok: false`, `vec_entities_ok: false`, `vec_chunks_ok: false`
- `schema_meta` indica `schema_version: 13` mas as tabelas de V013 NÃO existem
- O operador acredita que a migração foi bem-sucedida (exit 0, `status: "ok"`) mas o banco está quebrado
- Rodar `migrate --rehash` novamente NÃO corrige — V013 já está no histórico (INSERT OR IGNORE é no-op)
- Rodar `migrate` sem flags NÃO corrige — runner vê V013 como aplicada e pula
- CICLO SEM SAÍDA: o operador não tem nenhum comando que execute o SQL de V013


### Causa Raiz
- `run_rehash` (migrate.rs:239) itera TODAS as 13 migrações via `crate::migrations::runner().get_migrations()`
- Para cada migração, verifica se existe em `refinery_schema_history` (linha 248-254)
- Se a migração NÃO existe no histórico (branch `else` na linha 272), INSERE a linha com checksum calculado
- Esse INSERT marca a migração como "já aplicada" para o refinery-core
- refinery-core 0.9.1 usa `get_applied_migrations()` que lê `refinery_schema_history`
- O `Runner::run()` compara migrações do filesystem com o histórico e PULA as que já têm entrada
- Resultado: V013 é registrada mas NUNCA executada — as 3 tabelas + 4 índices + 2 inserts de schema_meta do SQL nunca rodam


### Cadeia Causa-Efeito Completa
```
CAUSA RAIZ: run_rehash itera TODAS as migrações, incluindo as NÃO aplicadas
  ↓
EFEITO: INSERT OR IGNORE na linha 278 grava V013 em refinery_schema_history
  ↓
CAUSA: runner().run() chama get_applied_migrations() do refinery-core
  ↓
EFEITO: refinery-core vê V013 no histórico → considera "já aplicada"
  ↓
EFEITO: runner PULA o SQL de V013 inteiramente
  ↓
EFEITO: CREATE TABLE memory_embeddings / entity_embeddings / chunk_embeddings NUNCA executa
  ↓
EFEITO: CREATE INDEX idx_memory_embeddings_ns / idx_entity_embeddings_ns NUNCA executa
  ↓
EFEITO: INSERT schema_meta vec_engine='rust-cosine' NUNCA executa
  ↓
CONSEQUÊNCIA: qualquer query a memory_embeddings falha com "no such table"
  ↓
CONSEQUÊNCIA: recall, hybrid-search, remember, edit, ingest → exit 10
  ↓
CONSEQUÊNCIA: CLI inutilizável para TODA operação que envolva embeddings
  ↓
AGRAVANTE: nenhum comando existente executa o SQL de V013 — ciclo sem saída
```

### Evidência no Banco Real do Operador (2026-06-09)
- `refinery_schema_history` contém V013 com `applied_on = '2026-06-09T15:46:28.395524+00:00'`
- `sqlite_master` NÃO contém `memory_embeddings`, `entity_embeddings` nem `chunk_embeddings`
- `schema_meta` NÃO contém `vec_engine` nem `embedding_default_dim` (seriam inseridos por V013)
- `schema_meta.schema_version = 13` — o código assume V013 aplicada mas as tabelas não existem
- `hybrid-search` retorna `{"error": true, "code": 10, "message": "no such table: memory_embeddings"}`


### Diagnóstico Diferencial — Por Que Afeta Apenas Bancos Migrados
- Bancos NOVOS (criados na v1.0.76+): `runner().run()` aplica V013 normalmente — SEM BUG
- Bancos v1.0.74 migrados: `run_rehash` é chamado ANTES de `runner().run()` → V013 é "pré-registrada" → PULA
- Bancos v1.0.76 com G40: o mesmo fluxo — `run_rehash` pré-registra, runner pula


### Solução Proposta
- Separar a lógica de `run_rehash` em duas operações distintas:
  - REESCREVER checksums de migrações que JÁ estão no histórico (UPDATE)
  - NÃO INSERIR migrações que NÃO estão no histórico (remover o branch `else` das linhas 272-281)
- Adicionar helper `ensure_v013_tables_exist` que verifica se as 3 tabelas BLOB-backed existem
  - Se `refinery_schema_history` tem V013 MAS as tabelas NÃO existem: executar o SQL de V013 diretamente
  - Isso cobre bancos que já foram corrompidos por versões anteriores
- Chamar `ensure_v013_tables_exist` em 3 pontos: `run()`, `run_rehash`, `run_to_llm_only`
- Atualizar `health --json` para reportar estado das tabelas BLOB-backed separadamente dos nomes legados vec_*


### Benefícios da Solução
- Bancos migrados que perderam as tabelas BLOB-backed são reparados automaticamente
- `run_rehash` passa a ser idempotente e seguro — nunca registra migrações não executadas
- O ciclo sem saída é quebrado — o helper detecta e corrige a inconsistência
- `recall`, `hybrid-search`, `remember`, `edit` e `ingest` voltam a funcionar
- `health --json` reporta o estado real das tabelas de embedding
- Compatibilidade retroativa mantida — bancos novos e bancos corretamente migrados não são afetados


### Como Solucionar — Plano de Implementação

#### Passo 1 — Corrigir `run_rehash` (migrate.rs:272-281)
- REMOVER o branch `else` que faz INSERT de migrações ausentes
- `run_rehash` deve APENAS reescrever checksums de migrações JÁ registradas
- Migrações ausentes do histórico devem ser deixadas para o `runner().run()` aplicar

#### Passo 2 — Criar helper `ensure_v013_tables_exist` (migrate.rs)
- Verificar se `memory_embeddings` existe em `sqlite_master`
- Se NÃO existe MAS V013 está em `refinery_schema_history`:
  - Executar o SQL de V013 diretamente via `conn.execute_batch()`
  - Isso é seguro porque V013 usa `CREATE TABLE IF NOT EXISTS` e `DROP TABLE IF EXISTS`
- Retornar `bool` indicando se as tabelas foram criadas

#### Passo 3 — Chamar `ensure_v013_tables_exist` nos pontos de entrada
- Em `run()` (linha 175): após `sanitize_null_applied_on`, antes de `runner().run()`
- Em `run_rehash` (linha 234): após `sanitize_null_applied_on`
- Em `run_to_llm_only` (linha 323): após `sanitize_null_applied_on`

#### Passo 4 — Adicionar campos nos Reports
- `RehashReport`: adicionar `v013_tables_created: bool`
- `ToLlmOnlyReport`: adicionar `v013_tables_created: bool`
- `MigrateResponse`: adicionar `v013_tables_created: bool`

#### Passo 5 — Atualizar schemas JSON
- `docs/schemas/migrate-rehash.schema.json`: adicionar `v013_tables_created`
- `docs/schemas/migrate-to-llm-only.schema.json`: adicionar `v013_tables_created`

#### Passo 6 — Adicionar testes
- Teste unitário: simular banco com V013 no histórico mas SEM as tabelas → `ensure_v013_tables_exist` cria
- Teste unitário: `run_rehash` NÃO insere migrações ausentes (branch `else` removido)
- Teste de integração: `migrate --rehash` seguido de `migrate` aplica V013 normalmente
- Teste de integração: `migrate --to-llm-only` com banco corrompido repara as tabelas

#### Passo 7 — Criar ADR-0028
- Documentar decisão arquitetural da separação rehash/insert
- Documentar o helper de reparo `ensure_v013_tables_exist`

#### Passo 8 — Atualizar documentação
- `CHANGELOG.md` e `CHANGELOG.pt-BR.md`
- `docs/MIGRATION.md` e `docs/MIGRATION.pt-BR.md`
- `docs/AGENTS.md` e `docs/AGENTS.pt-BR.md`
- `docs/COOKBOOK.md` e `docs/COOKBOOK.pt-BR.md`
- `docs/TESTING.md` e `docs/TESTING.pt-BR.md`


### Arquivos Afetados
- `src/commands/migrate.rs` — arquivo principal (corrigir `run_rehash`, adicionar helper)
- `src/commands/health.rs` — melhorar diagnóstico de tabelas BLOB-backed
- `docs/schemas/migrate-rehash.schema.json` — novo campo
- `docs/schemas/migrate-to-llm-only.schema.json` — novo campo
- `docs/decisions/adr-0028-*.md` — nova decisão arquitetural
- `tests/schema_migration_integration.rs` — novos testes


### Notas Técnicas
- V013 SQL usa `CREATE TABLE IF NOT EXISTS` — execução direta é idempotente e segura
- V013 SQL usa `DROP TABLE IF EXISTS vec_*` — se vec tables já foram removidas, é no-op
- `schema_meta` inserts usam `INSERT OR REPLACE` — idempotente
- A correção deve ser retrocompatível com 4 cenários de banco:
  - Banco novo (v1.0.77+): sem V013 no histórico → runner aplica normalmente
  - Banco v1.0.74 não migrado: sem V013 → runner aplica normalmente (rehash corrigido não insere)
  - Banco v1.0.76/77 com bug: V013 no histórico mas SEM tabelas → helper cria
  - Banco v1.0.76/77 correto: V013 no histórico COM tabelas → helper é no-op
