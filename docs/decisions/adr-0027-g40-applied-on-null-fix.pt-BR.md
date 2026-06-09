# ADR-0027: Correção do G40 — `applied_on = NULL` Bloqueia Migrações


## Status
- Aceito (2026-06-09)
- Decisores: Danilo Aguiar
- Escopo: `src/commands/migrate.rs`, `src/commands/debug_schema.rs`
- Abrange o fluxo de migração v1.0.74 para v1.0.77


## Contexto
### Causa Raiz
- `run_rehash` em `migrate.rs:263` inseria linhas sem `applied_on`
- O campo ficava NULL no SQLite após a operação
- O `refinery-core` 0.9.1 lê `applied_on` como `String` (NOT NULL)
- `row.get::<_, String>(2)` falha com `InvalidColumnType`
- Todas as migrações subsequentes ficam bloqueadas (exit 20)
### Impacto do Incidente
- Incidente real: aproximadamente 28 horas de bloqueio em 2026-06-09
- Nenhuma migração executava após o rehash com campo NULL
### Agravantes
- V013 DROP requer módulo `vec0` ausente no build LLM-only
- `debug_schema.rs` também crasha ao ler `applied_on` NULL
- O operador não tinha ferramenta de diagnóstico funcional


## Decisão
### Correção 1 — Helper `sanitize_null_applied_on`
- UPDATE em linhas com `applied_on` NULL antes do runner
- Preenche com timestamp RFC3339 atual via `chrono`
- Executa antes de qualquer operação do refinery
### Correção 2 — INSERT com `applied_on` Explícito
- Todo INSERT agora inclui `applied_on` com timestamp RFC3339
- Usa `chrono::Utc::now().to_rfc3339()` como valor padrão
- Previne criação de novas linhas com campo NULL
### Correção 3 — `remove_vec_virtual_tables_without_module`
- Limpa virtual tables órfãs via `writable_schema`
- Remove referências ao módulo `vec0` ausente no build LLM-only
- Desbloqueia a migração V013 sem dependência do sqlite-vec
### Correção 4 — `debug_schema.rs` Tolerante a NULL
- Campo `applied_on` alterado de `String` para `Option<String>`
- Diagnóstico acessível mesmo em bancos com campo NULL
- Sem crash ao inspecionar bancos afetados pelo bug


## Consequências
### Positivas
- 4 cenários retrocompatíveis cobertos pela correção
- Cenário fresh install funciona sem intervenção
- Cenário v1.0.74 migra corretamente para v1.0.77
- Cenário v1.0.76 com bug corrigido automaticamente
- Cenário v1.0.74 com bug acumulado também resolvido
- Sem intervenção manual do operador em nenhum caso
- `debug-schema` acessível em bancos afetados pelo NULL
- Idempotência garantida em todas as operações de sanitização
### Negativas
- VACUUM após `writable_schema` pode ser lento em bancos grandes
- `chrono` RFC3339 emite `+00:00` em vez de `Z` como sufixo
- Formato `+00:00` é compatível mas difere do crate `time`
- Overhead de sanitização no startup do migrate (desprezível)


## Verificação
- `cargo build`: ZERO erros de compilação
- `cargo test --lib`: 723 testes executados (4 novos)
- ZERO falhas na suite de testes completa
- `cargo clippy`: ZERO warnings reportados
- 4 testes novos cobrem os cenários de retrocompatibilidade


## Referências
- Arquivo: `src/commands/migrate.rs` (helper de sanitização)
- Arquivo: `src/commands/debug_schema.rs` (tolerância a NULL)
- Versão: v1.0.77
- Data do incidente: 2026-06-09
