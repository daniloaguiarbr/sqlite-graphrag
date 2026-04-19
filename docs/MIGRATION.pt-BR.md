# Guia de Migração — neurographrag


## Migrando de v1.x para v2.x


### Mudanças Incompatíveis em v2.0.0
- Flag `--allow-parallel` foi removida sem substituto direto
- Concorrência agora é controlada exclusivamente via `--max-concurrency` (padrão: 4)
- Todo script que passa `--allow-parallel` deve remover essa flag antes de atualizar
- O teto de `--max-concurrency` é `2×nCPUs`; valores acima do teto retornam exit 2

### Mudanças Incompatíveis em v2.0.1
- Flag `--days` no subcomando `purge` foi substituída por `--retention-days` como nome canônico
- O alias `--days` permanece disponível em v2.0.1 e posteriores para compatibilidade
- Scripts usando `--days` continuam funcionando mas devem migrar para `--retention-days`
- Flag `--to` no subcomando `sync-safe-copy` foi substituída por `--dest` como nome canônico

### Novas Flags Globais em v2.0.1
- `--lang <en|pt>` seleciona o idioma de saída para mensagens legíveis por humanos no stderr
- `--tz <IANA>` aplica um fuso horário a todos os campos `*_iso` na saída JSON
- Ambas as flags são globais e podem ser colocadas antes de qualquer subcomando

### Mudanças de Versão de Schema
- v2.0.0 introduziu novas colunas; execute `neurographrag migrate` após atualizar o binário
- `migrate` é idempotente e seguro para executar múltiplas vezes no mesmo banco
- Execute `neurographrag health --json` para confirmar que `schema_version` corresponde ao valor esperado


## Passo a Passo — Atualização de v1.x

### Passo 1 — Instalar o novo binário
```bash
cargo install neurographrag --version 2.1.0
```

### Passo 2 — Aplicar migrações de schema
```bash
neurographrag migrate
```

### Passo 3 — Atualizar scripts que usam flags removidas
- Substituir `--allow-parallel` por `--max-concurrency <N>` (ex: `--max-concurrency 4`)
- Substituir `purge --days N` por `purge --retention-days N`
- Substituir `sync-safe-copy --to CAMINHO` por `sync-safe-copy --dest CAMINHO`

### Passo 4 — Verificar o banco de dados
```bash
neurographrag health --json
neurographrag stats --json
```

### Passo 5 — Confirmar formato de saída JSON
- `list --json` agora retorna `{"items": [...]}` (não um array puro)
- Atualizar pipelines `jaq` de `.[]` para `.items[]` para saída de lista
- `recall --json` e `hybrid-search --json` retornam `{"results": [...]}`
- Atualizar pipelines `jaq` de `.[]` para `.results[]` para saída de busca


## Mudanças em Códigos de Saída

| Código | Significado                                      | Desde  |
|--------|--------------------------------------------------|--------|
| 13     | Banco de dados ocupado (era 15 na v1.x)          | v2.0.0 |
| 75     | Semáforo de slots exaurido                       | v1.2.0 |
| 77     | Guarda de RAM baixa acionada                     | v1.2.0 |


## Instruções de Reversão

### Revertendo para v1.x
- O schema de v2.x é apenas de avanço; não existe downgrade automático
- Para reverter: restaure a partir de um snapshot de backup anterior à migração
- Use `sync-safe-copy` para criar um backup ANTES de executar `migrate`
```bash
neurographrag sync-safe-copy --dest ~/backup/neurographrag-pre-v2.sqlite
neurographrag migrate
```


## Veja Também
- `CHANGELOG.md` para a lista completa de mudanças por release
- `docs/HOW_TO_USE.md` para referência atual de flags
- `docs/COOKBOOK.md` para receitas de pipeline atualizadas
