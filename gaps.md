# Gaps — sqlite-graphrag CLI


## G40 — `migrate --rehash` insere linha com `applied_on = NULL` que bloqueia toda migração subsequente
### Status: RESOLVIDO na v1.0.77
### Severidade: CRITICAL
### Versão afetada: v1.0.76
### Data de identificação: 2026-06-09
### Data de resolução: 2026-06-09

### Problema
- O comando `migrate --rehash` insere linhas novas em `refinery_schema_history` SEM preencher o campo `applied_on`
- Quando a migração V013 NUNCA foi aplicada no banco, o `run_rehash` detecta a ausência da linha 13 e cria um registro placeholder
- Esse registro contém APENAS `version`, `name` e `checksum` — o campo `applied_on` fica NULL
- Na execução SEGUINTE de `runner().run()`, o refinery-core 0.9.1 chama `get_applied_migrations` que lê TODAS as linhas da tabela
- O driver rusqlite do refinery faz `let applied_on: String = row.get(2)?` (tipo `String`, NÃO `Option<String>`)
- Quando `applied_on` é NULL, `row.get::<_, String>(2)` falha com `InvalidColumnType(Null at index: 2, name: applied_on)`
- A partir desse ponto, QUALQUER comando que invoque o migration runner aborta com exit code 20

### Consequências do Problema
- `migrate --to-llm-only --drop-vec-tables` falha imediatamente após o rehash interno
- `migrate` sem flags falha com o mesmo erro
- TODOS os comandos que dependem do runner de migração ficam bloqueados
- O operador entra em loop destrutivo: tenta `--rehash`, que reinsere a linha fantasma, que bloqueia o runner de novo
- Remoção manual da linha via `sqlite3` funciona, mas o próximo `migrate` reinsere a linha com NULL
- Cenário real: bloqueio de ~28 horas na máquina do operador (incidente de 2026-06-09)
- O banco inteiro fica inacessível para operações de escrita que passam pelo runner

### Causa Raiz do Problema
- Localização exata: `src/commands/migrate.rs:263-266`
- O código:
```rust
conn.execute(
    "INSERT OR IGNORE INTO refinery_schema_history (version, name, checksum) VALUES (?1, ?2, ?3)",
    rusqlite::params![version, name, new_checksum.to_string()],
)?;
```
- O INSERT omite o campo `applied_on`
- O schema da tabela (definido pelo refinery-core em `traits/mod.rs:108-112`) declara `applied_on VARCHAR(255)` SEM constraint `NOT NULL`
- O SQLite aceita o INSERT com `applied_on = NULL` sem erro
- PORÉM o driver rusqlite do refinery (`drivers/rusqlite.rs:16`) EXIGE `String` (NOT NULL) ao ler o campo
- Essa incompatibilidade entre escrita (permite NULL) e leitura (exige NOT NULL) é o defeito estrutural

### Cadeia Causa-Efeito Completa
```
CAUSA: run_rehash insere linha sem applied_on
  ↓
EFEITO: linha 13 gravada com applied_on = NULL
  ↓
CAUSA: runner().run() chama get_applied_migrations()
  ↓
EFEITO: query SELECT lê TODAS as linhas incluindo a com NULL
  ↓
CAUSA: driver rusqlite faz row.get::<_, String>(2) na linha com NULL
  ↓
EFEITO: InvalidColumnType(Null at index: 2) → AppError::Internal → exit 20
  ↓
CAUSA: operador tenta migrate --rehash para "consertar"
  ↓
EFEITO: rehash detecta que linha 13 já existe → não reescreve → runner falha de novo
  ↓
CAUSA: operador remove linha 13 via sqlite3 e tenta migrate de novo
  ↓
EFEITO: rehash reinsere linha 13 SEM applied_on → ciclo infinito
```

### Agravante — V013 e vec0
- O problema é AGRAVADO pela interação com a migração V013
- V013 executa `DROP TABLE IF EXISTS vec_memories` que requer o módulo `vec0`
- O build LLM-only da v1.0.76 NÃO inclui `vec0`
- O SQLite aborta com `no such module: vec0` ao tentar DROP em virtual tables
- O refinery-core NÃO faz rollback da transação de INSERT do histórico quando o SQL falha no modo NÃO-batched (linhas 84-99 de `sync.rs`)
- Na prática o INSERT do histórico (índice ímpar) e o SQL da migração (índice par) rodam em transações SEPARADAS por iteração
- Se o SQL falha, o INSERT da PRÓXIMA iteração NUNCA roda, mas o estado da tabela pode ficar inconsistente com linhas fantasmas de tentativas anteriores

### Solução Proposta
- Alterar `run_rehash` em `src/commands/migrate.rs:263-266` para SEMPRE incluir `applied_on` com timestamp válido
- Usar o mesmo formato RFC3339 que o refinery-core usa nativamente
- Alternativa: usar `CURRENT_TIMESTAMP` do SQLite como valor default
- O INSERT corrigido deve ser:
```rust
conn.execute(
    "INSERT OR IGNORE INTO refinery_schema_history (version, name, applied_on, checksum) VALUES (?1, ?2, ?3, ?4)",
    rusqlite::params![
        version,
        name,
        time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap(),
        new_checksum.to_string()
    ],
)?;
```

### Benefícios da Solução
- Elimina o ciclo infinito de reinserção de linha fantasma
- `migrate --to-llm-only --drop-vec-tables` passa a funcionar para bancos v1.0.74
- O fluxo `rehash → runner` fica atômico sem estado intermediário inválido
- Compatibilidade total com o contrato do refinery-core 0.9.1 (campo `applied_on` sempre preenchido)
- Operadores não precisam de intervenção manual no banco

### Como Solucionar — Passo a Passo
- Passo 1: Adicionar `time` como dependência (já presente no projeto via refinery-core)
- Passo 2: Alterar o INSERT na linha 263-266 de `src/commands/migrate.rs` para incluir `applied_on` com timestamp RFC3339
- Passo 3: Adicionar lógica de SANITIZAÇÃO no início de `run_rehash` e `run_to_llm_only` para detectar e corrigir linhas existentes com `applied_on = NULL`
- Passo 4: A sanitização deve ser:
```sql
UPDATE refinery_schema_history
SET applied_on = datetime('now')
WHERE applied_on IS NULL;
```
- Passo 5: Adicionar teste unitário que reproduz o cenário (inserir linha sem `applied_on`, rodar rehash, confirmar que o runner não falha)
- Passo 6: Adicionar teste de integração para o fluxo completo `rehash → runner` em banco com histórico parcial
- Passo 7: Documentar em `MIGRATION.pt-BR.md` que operadores com bancos afetados podem rodar a sanitização manual via `sqlite3` antes de atualizar
- Passo 8: Considerar adicionar portão de segurança no `run_to_llm_only` que verifica `SELECT COUNT(*) FROM refinery_schema_history WHERE applied_on IS NULL` e corrige ANTES de chamar `runner().run()`

### Agravante Secundário — vec0 e DROP TABLE em Virtual Tables
- O problema de `applied_on = NULL` é o bloqueio PRIMÁRIO
- Mas mesmo após corrigir o `applied_on`, a V013 AINDA falhará em bancos v1.0.74 com vec tables presentes
- `DROP TABLE IF EXISTS vec_memories` exige que o módulo `vec0` esteja carregado no SQLite
- O build LLM-only da v1.0.76 remove `sqlite-vec` das dependências
- O SQLite do sistema (ex: macOS `/usr/bin/sqlite3`) NÃO tem `vec0`
- Nem `DROP TABLE`, nem `ALTER TABLE RENAME`, nem `DELETE FROM sqlite_master` funcionam sem `vec0`
- A ÚNICA forma de remover vec tables sem `vec0` é manipulação direta de `sqlite_master`:
```sql
PRAGMA writable_schema = ON;
DELETE FROM sqlite_master WHERE type = 'table' AND name IN ('vec_memories', 'vec_entities', 'vec_chunks');
DELETE FROM sqlite_master WHERE type = 'table' AND name LIKE 'vec_memories_%';
DELETE FROM sqlite_master WHERE type = 'table' AND name LIKE 'vec_entities_%';
DELETE FROM sqlite_master WHERE type = 'table' AND name LIKE 'vec_chunks_%';
PRAGMA writable_schema = OFF;
PRAGMA integrity_check;
VACUUM;
```
- Essa lógica DEVE ser incorporada no `run_to_llm_only` ANTES de chamar `runner().run()`
- O `run_to_llm_only` deve dropar as vec tables via `writable_schema` quando `vec0` não está disponível
- Depois, a V013 faz `DROP TABLE IF EXISTS` que será no-op (tabelas já removidas)
- E o `CREATE TABLE IF NOT EXISTS` das BLOB tables executará normalmente

### Referências
- `src/commands/migrate.rs:263-266` — INSERT sem `applied_on`
- `refinery-core-0.9.1/src/drivers/rusqlite.rs:16` — `row.get::<_, String>(2)` exige NOT NULL
- `refinery-core-0.9.1/src/traits/mod.rs:95-104` — `insert_migration_query` com `applied_on` preenchido
- `refinery-core-0.9.1/src/traits/mod.rs:108-112` — schema da tabela sem `NOT NULL` em `applied_on`
- `refinery-core-0.9.1/src/traits/sync.rs:84-99` — loop alternado SQL/INSERT sem rollback cruzado
- `migrations/V013__drop_vec_use_blob_embeddings.sql:14-16` — DROP das vec tables
- `migrations/V002__vec_tables.sql` — no-op `SELECT 1;` na v1.0.76
- `docs/decisions/adr-0026-v002-vec-tables-migration-drift.pt-BR.md` — contexto do drift V002
- Stack Overflow: "Corrupted Database Table cannot DROP — No such module FTS5" — mesmo padrão com virtual tables

### Resolução Aplicada na v1.0.77
- Helper `sanitize_null_applied_on` adicionado em `src/commands/migrate.rs`
- Chamado em 3 pontos de entrada: `run()`, `run_rehash`, `run_to_llm_only`
- INSERT corrigido na linha 263 para incluir `applied_on` com `chrono::Utc::now().to_rfc3339()`
- Helper `remove_vec_virtual_tables_without_module` adicionado para limpeza via `PRAGMA writable_schema`
- `debug_schema.rs:36` corrigido: `applied_on: String` para `Option<String>`
- Campos `null_rows_fixed` e `vec_tables_removed_via_writable_schema` adicionados nos reports JSON
- 4 testes unitários novos cobrindo sanitização, INSERT e remoção de vec tables
- Validação: 723 testes passaram, ZERO falhas, ZERO warnings do clippy
- ADR: `docs/decisions/adr-0027-g40-applied-on-null-fix.pt-BR.md`
