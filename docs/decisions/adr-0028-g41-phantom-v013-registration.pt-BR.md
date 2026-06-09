# ADR-0028: G41 Correção do Registro Fantasma da V013


## Status
- Aceito (2026-06-09)
- Decisores: Danilo Aguiar
- Escopo: `src/commands/migrate.rs`, `src/storage/connection.rs`
- Cobre o bug de registro fantasma da V013 nas versões v1.0.76/v1.0.77


## Contexto
### Causa Raiz
- `run_rehash` em `migrate.rs` iterava TODAS as 13 migrações embarcadas
- Para qualquer migração NÃO presente em `refinery_schema_history`, inseria uma linha com `INSERT OR IGNORE`
- Isso registrava a V013 como "já aplicada" sem executar seu SQL
- `runner().run()` lia o histórico, via a V013 presente e a ignorava completamente
- As tabelas de embedding com backing BLOB nunca eram criadas
### Impacto do Incidente
- Toda operação de embedding falhava com exit 10: `no such table: memory_embeddings`
- Comandos afetados: `recall`, `hybrid-search`, `remember`, `edit`, `ingest`
- O banco de dados entrava em um ciclo sem saída onde nenhum comando conseguia executar o SQL da V013
### Fatores Agravantes
- `ensure_db_ready` em `connection.rs` só executa migrações quando `user_version < SCHEMA_USER_VERSION`
- Bancos corrompidos pelo G41 já tinham `user_version=50`
- O bloco de migração era ignorado completamente nos comandos CRUD
- Nenhum comando existente conseguia disparar o reparo


## Decisão
### Correção 1 — Remover Registro Fantasma
- Remover o branch `else` em `run_rehash` (linhas 272-281) que inseria migrações ausentes
- `run_rehash` agora APENAS reescreve checksums de migrações já presentes no histórico
- Migrações ausentes são deixadas para `runner().run()` aplicar com seu SQL
### Correção 2 — Helper `ensure_v013_tables_exist`
- Detecta o estado de registro fantasma
- V013 presente no histórico mas `memory_embeddings` ausente
- Executa o SQL da V013 diretamente quando o estado fantasma é detectado
- V013 usa `CREATE TABLE IF NOT EXISTS` e `INSERT OR REPLACE`
- A operação é idempotente por design
### Correção 3 — Helper Chamado em 4 Pontos de Entrada
- `run()` em migrate.rs
- `run_rehash` em migrate.rs
- `run_to_llm_only` em migrate.rs
- `ensure_db_ready` em connection.rs (incondicionalmente, fora da verificação de versão)


## Consequências
### Positivas
- Bancos corrompidos pelo G41 nas versões v1.0.76/v1.0.77 são auto-reparados por qualquer comando
- `run_rehash` agora é seguro e nunca registra migrações não aplicadas
- O ciclo sem saída é quebrado
- Compatível com 5 cenários de banco (novo, v1.0.74, corrompido, correto, apenas CRUD)
### Negativas
- Nenhuma
- O SQL da V013 é idempotente
- A verificação de reparo são dois SELECTs baratos em `sqlite_master` e `refinery_schema_history`


## Referências
- Arquivo: `src/commands/migrate.rs` (correção do registro fantasma)
- Arquivo: `src/storage/connection.rs` (chamada ao ensure_v013_tables_exist)
- Versão: v1.0.78
- Relacionado: ADR-0027 (G40 correção do applied_on NULL)
