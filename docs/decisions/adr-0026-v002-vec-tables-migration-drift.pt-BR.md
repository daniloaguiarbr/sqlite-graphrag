# ADR-0026: Drift da Migração V002 `vec_tables` Deve Ser Corrigido no Binário

- Status: Aceito (2026-06-09)
- Decisores: Danilo Aguiar
- Escopo: `migrations/V002__vec_tables.sql`, `src/commands/migrate.rs`, fluxo de instalação e rebuild do operador

## Contexto

O source verdadeiro da v1.0.76 embute `migrations/V002__vec_tables.sql`
como uma migração no-op de 721 bytes que termina em `SELECT 1;`. O
source histórico da v1.0.54 embute um V002 diferente: 834 bytes de DDL
do `sqlite-vec` que cria `vec_memories`, `vec_entities` e `vec_chunks`.

O Refinery 0.9.1 não compara arquivos de migração com SHA-256. Ele
verifica a migração aplicada contra o conteúdo embutido no binário usando
o checksum SipHasher13 sobre `(name, version, sql)`. Se o checksum
gravado em `refinery_schema_history` não bater com o SQL embutido no
binário em execução, o Refinery ABORTA com exit code 20 antes de
qualquer caminho de escrita (`remember`, `ingest`, `edit` e comandos
relacionados) continuar.

Em termos concretos, o mecanismo completo do SipHasher13 é:

```rust
let mut hasher = SipHasher13::new();   // chaves 0, 0
name.hash(&mut hasher);                // "vec_tables"
version.hash(&mut hasher);             // 2i32
sql.hash(&mut hasher);                 // conteúdo do .sql
let checksum = hasher.finish();        // u64
```

Para `name = "vec_tables"` e `version = 2i32`, o V002 de 834 bytes
embutido no binário histórico produz `10367736093436539632`. O V002
no-op de 721 bytes gravado pelo banco produz `16903500262185826246`.

Em 2026-06-09 o binário instalado em `~/.cargo/bin/sqlite-graphrag`
reportava versão `1.0.76`, mas a inspeção mostrou que ele havia sido
compilado do pacote source v1.0.54 e ainda embutia o V002 antigo de 834
bytes. O banco em si havia sido criado pelo build verdadeiro da v1.0.76,
então seu checksum gravado batia com o V002 no-op de 721 bytes. Logo, o
problema era drift de procedência do binário, não corrupção do banco.

## Decisão

Quando a falha for `applied migration V2__vec_tables is different than
filesystem one V2__vec_tables`, trate o binário como primeiro suspeito.
Não confie apenas em `sqlite-graphrag --version`.

O fix é recompilar o binário a partir do checkout correto e substituir o
executável instalado:

```bash
cargo build --release
cp target/release/sqlite-graphrag ~/.cargo/bin/sqlite-graphrag
```

Edições manuais em `refinery_schema_history` viram último recurso. Antes
de qualquer cirurgia de checksum:

- faça backup do banco
- confirme qual SQL de V002 o binário em execução embute
- calcule o checksum com SipHasher13 do Refinery, não com SHA-256

## Consequências

### Positivas

- A remediação é simples e preserva os dados existentes.
- O operador evita reescrever `refinery_schema_history` quando o banco já
  está correto.
- A triagem futura de drift começa pela procedência do binário, que é
  mais rápida e segura do que mutar linhas do histórico de migração.

### Negativas

- `--version` não é sinal suficiente de integridade para binários
  instalados.
- Um binário rotulado errado pode parecer atual e ainda embutir
  migrações obsoletas.
- O operador precisa manter um caminho de rebuild disponível ao depurar
  drift de migração.

## Verificação

- `cargo build --release`: verde — recompilou o checkout source local em
  `target/release/sqlite-graphrag`
- Binário instalado substituído em `~/.cargo/bin/sqlite-graphrag`: o
  novo executável caiu de ~37 MB para ~15 MB
- Validação com `remember`: `echo "teste" | timeout 60 sqlite-graphrag
  remember --name diagnose-final-006 --type note --description "fix"
  --body-stdin`
- Resultado de `remember`: `{"memory_id": 1207, "action": "created",
  "chunks_created": 1, "elapsed_ms": 34437}`
- O gap residual de 1138 memórias sem embedding não era a causa raiz do
  bloqueio de ~28 horas. O bloqueio real era o drift de migração.
- Checks pós-fix: `health`, `stats`, `hybrid-search`, `graph entities`,
  `read`, `list`, `remember` e `forget` passaram
- Após corrigir o drift, o pipeline local de embedding voltou a
  funcionar com sucesso no teste validado em ~34 s.
- Decisão persistida no grafo como
  `decisao-fix-migration-v2-vec-tables` com `type = decision` e
  descrição curta
- Estado pós-fix do banco: 1142 memórias preservadas antes de registrar
  esta decisão, 1143 depois; `schema_version = 13`; FTS5 e checks de
  integridade permaneceram OK

## Lições

- Nunca confie em `--version` sem checar o conteúdo embutido da migração
  quando o Refinery reportar drift.
- Rebuild do source local vem antes de análise profunda de drift em
  V002.
- Faça backup antes de tocar em `refinery_schema_history`.
- O Refinery 0.9.1 usa SipHasher13 com chaves padrão para checksums de
  migração.
