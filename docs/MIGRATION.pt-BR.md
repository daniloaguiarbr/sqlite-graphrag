# MIGRANDO PARA v1.0.80 — Política de Estabilidade, Infra Windows, Resiliência de SHUTDOWN

> Este guia é para operadores na v1.0.79 que querem atualizar para a v1.0.80 sem perder dados. Esta release é bump PATCH sem NENHUMA migração de banco.

## O Que Mudou na v1.0.80

- **Política de estabilidade declarada** (ADR-0032, G53): o contrato público é a CLI; a API da biblioteca é instável em v1.x.y. Consumidores da biblioteca devem fixar em `=1.0.80` e revisar CHANGELOG.md antes de bumpar
- **Job de CI `semver-checks`** adicionado em modo informativo (vira bloqueante em v1.0.81 quando as 9 violações MAJOR pendentes forem resolvidas)
- **G45 singleton de embedding cross-process** (follow-up do ADR-0032): `acquire_embedding_singleton` serializa chamadas de embedding LLM por par `(namespace, db)`; `--wait-embed-singleton SEGUNDOS` faz poll do lock; `AppError::EmbeddingSingletonLocked` é a nova variante estrutural (exit 75, retentável)
- **G55 S2 `MemoryNotFound` estrutural**: substitui o caminho legado `NotFound(String)` que mascarava qual alvo de lookup falhou; mensagens em pt-BR agora carregam nome e namespace explicitamente
- **G56 cache de entity-embed em processo**: `embed_entity_texts_cached` chaveado por `blake3(model || \0 || text)`; taxa de hit alta em `ingest`, modesta em `remember`/`remember-batch`
- **G58 fallback FTS5 de recall e hybrid-search**: `recall --fallback-fts-only` e `hybrid-search --fallback-fts-only` roteiam a query via FTS5 BM25 quando o subprocesso LLM falha; novos campos do envelope `vec_degraded`, `vec_error`, `warning` são preenchidos simetricamente
- **G53-WINDOWS-INFRA** (ADR-0033): os jobs da matrix windows-2025 ganharam steps de pre-warm e verify gateados em `if: matrix.os == windows-2025`. Os 2 modos históricos de falha de infra (download do rustup com erros transitórios de rede e `E0463 can't find crate for core` quando a stdlib do target está ausente) agora são recuperáveis na primeira re-run
- **Resiliência de SHUTDOWN** (ADR-0034): `src/signals.rs` é envolvido em uma barreira de captura de panic; o terceiro Ctrl-C consecutivo sai com código 130 e ZERO I/O, casando com a receita canônica de bypass SHUTDOWN em 3 camadas (`nohup` então `setsid` então `disown`)

## Quem É Afetado

- Todos os usuários da v1.0.79; as mudanças são todas aditivas no nível binário e de banco
- Consumidores da biblioteca (usuários do crate cargo, não da CLI) são FORTEMENTE aconselhados a fixar em `=1.0.80` porque a API da lib é instável dentro de v1.x.y
- Operadores multi-sessão (agentes concorrentes escrevendo no mesmo banco) se beneficiam do singleton G45 sem nenhuma ação

## Como Atualizar

```bash
cargo install sqlite-graphrag --version 1.0.80 --force
sqlite-graphrag --version   # deve reportar 1.0.80
```

NENHUMA migração de banco é necessária. O schema continua v13, a adoção de dim do G43 já roda em `open_rw` e `open_ro`, e as adições da API da biblioteca são todas ADITIVAS (nenhum re-export removido, nenhum campo renomeado, nenhuma assinatura alterada em 1.0.80).

## O Que Acontece Automaticamente

- Todos os comandos da v1.0.79 se comportam identicamente; as novas flags (`--wait-embed-singleton`, `--fallback-fts-only`, `--force-reembed` da v1.0.79) são opt-in
- Os steps de pre-warm do Windows são no-op em ubuntu e macos; só rodam em `matrix.os == windows-2025`
- O job de CI `semver-checks` é informativo na v1.0.80; ele reporta drift sem falhar o pipeline

## Pinning da API da Biblioteca

Se você depende da API da lib, fixe na versão EXATA em `Cargo.toml`:

```toml
[dependencies]
sqlite-graphrag = "=1.0.80"
```

O atalho `^1.0` te mantém na trilha de estabilidade da CLI. O atalho `^1.0.80` permite 1.0.80..<1.1.0, o que pode incluir uma futura 1.0.81 com mudanças quebrantes na lib. Para usuários da lib, o pin exato é mandatório.

## O Que Quebra

- **Consumidores da biblioteca que dependem de símbolos NÃO na superfície da lib 1.0.80**: nenhum adicionado além dos 6 documentados no CHANGELOG. Todos os 6 são aditivos
- **Workflows de CI que referenciam `windows-latest`**: esta release não altera a label do runner; a referência explícita `windows-2025` (adicionada na v1.0.73) continua sendo a escolha certa até a data de corte do redirect do VS2026 (2026-06-15)

## Rollback

Se a v1.0.80 não estiver funcionando para você:

```bash
cargo install sqlite-graphrag --version 1.0.79 --force
```

Seu banco está inalterado. A v1.0.80 não fez modificações de schema; a v1.0.79 lê o mesmo arquivo SQLite.


# MIGRAÇÃO PARA v1.0.78 — Correção do Registro Fantasma de V013 (G41)

## O Que Mudou

- `run_rehash` não insere mais linhas fantasma para migrações não aplicadas
- Novo helper `ensure_v013_tables_exist` repara bancos onde V013 foi registrada mas as tabelas nunca foram criadas
- Reparo automático roda incondicionalmente em `ensure_db_ready` — qualquer comando repara bancos corrompidos

## Quem É Afetado

- Usuários que rodaram `migrate --rehash` ou `migrate --to-llm-only --drop-vec-tables` na v1.0.76 ou v1.0.77
- Sintomas: `no such table: memory_embeddings` (exit 10) em `recall`, `hybrid-search`, `remember`

## Como Atualizar

```bash
cargo install sqlite-graphrag --version 1.0.78 --force
sqlite-graphrag migrate --rehash   # reparo explícito (opcional — qualquer comando repara automaticamente)
```

## O Que Acontece Automaticamente

- Qualquer comando CRUD (`remember`, `recall`, `hybrid-search`, etc.) detecta e repara o estado corrompido
- O helper `ensure_v013_tables_exist` verifica se V013 está em `refinery_schema_history` mas as tabelas BLOB-backed estão ausentes, e executa o SQL de V013 diretamente
- O SQL de V013 é idempotente (`CREATE TABLE IF NOT EXISTS`) — seguro para executar múltiplas vezes


# MIGRAÇÃO PARA v1.0.77 — Correção do G40

> Este guia é para operadores afetados pelo bug G40 da v1.0.76 onde `migrate --rehash` inseria linhas com `applied_on = NULL`

## O Que Mudou na v1.0.77

- Correção do INSERT em `run_rehash` que omitia o campo `applied_on`
- Sanitização automática de linhas com `applied_on = NULL` antes de rodar o migration runner
- Remoção de vec virtual tables via `PRAGMA writable_schema` quando o módulo `vec0` está ausente
- Correção do `debug-schema` que crashava em bancos com `applied_on = NULL`

## Quem É Afetado

- Operadores que rodaram `migrate --rehash` ou `migrate --to-llm-only` na v1.0.76
- Bancos que apresentam o erro `InvalidColumnType(Null at index: 2, name: applied_on)`
- Bancos v1.0.74 com vec virtual tables presentes

## Como Atualizar

```bash
cargo install sqlite-graphrag --version 1.0.77 --force
sqlite-graphrag migrate
```

- Nenhuma intervenção manual em SQL é necessária
- A v1.0.77 detecta e corrige automaticamente linhas com `applied_on = NULL`
- Vec virtual tables são removidas automaticamente via `writable_schema` se `vec0` estiver ausente


# MIGRAÇÃO PARA v1.0.76 — LLM-Only One-Shot

> Este guia é para operadores em v1.0.74 ou v1.0.75 que querem atualizar para v1.0.76 sem perder dados.

## O Que Mudou na v1.0.76

O build padrão agora é **apenas LLM e one-shot**:

- Geração de embedding: `claude code` (OAuth Anthropic) ou `codex` (OAuth OpenAI ChatGPT Pro), spawnado por chamada. Sem daemon. Sem runtime ONNX. Sem download de modelo.
- NER: o `LlmBackend` extrai entidades e relacionamentos via tool-use JSON. O `extract_graph_auto` padrão é apenas regex de URL; NER completo roda sob demanda com `--extraction-backend llm`.
- Busca vetorial: similaridade de cosseno em Rust puro sobre as tabelas BLOB-backed `memory_embeddings`, `entity_embeddings`, `chunk_embeddings`. A extensão C do `sqlite-vec` foi REMOVIDA.

## Pré-Requisitos

Você precisa de UMA destas no `PATH` depois do `cargo install`:

- `claude` — CLI do Claude Code 2.1.0+ ([docs](https://docs.claude.com/claude-code))
- `codex` — CLI do OpenAI Codex 0.130.0+
  ([repositório](https://github.com/openai/codex))

Ambas precisam estar logadas com o fluxo OAuth (assinatura Claude Pro/Max ou ChatGPT Pro). Chaves de API NÃO são suportadas e fazem o spawn ABORTAR com `AppError::Validation`.

Para verificar:

```bash
which claude || which codex
claude --version  # precisa reportar 2.1.0 ou superior
codex --version   # precisa reportar 0.130.0 ou superior
```

## Passo 1 — Instalar o Binário Atual (v1.0.79)

```bash
cargo install sqlite-graphrag --version 1.0.79 --force
```

Instale a v1.0.79 (não a 1.0.76): ela carrega os reparos de
migração G40/G41 e os fixes de embedding G42/G43 dos quais o
caminho de upgrade depende.

Isso instala o build padrão LLM-only (binário de ~6 MB, sem runtime ONNX, sem download de modelo). Se você quer o pipeline legado fastembed para a janela de transição:

```bash
cargo install sqlite-graphrag --version 1.0.76 --features embedding-legacy --force
```

A feature `embedding-legacy` foi REMOVIDA na v1.0.79 (antecipando o
cronograma da v1.1.0); o comando acima só funciona fixando 1.0.76-1.0.78.

## Passo 2 — Migrar o Banco Existente

A migração é automática no próximo `init`, `remember` ou `ingest`. A migração V013 dropa as virtual tables `vec_memories`, `vec_entities`, `vec_chunks` e cria as novas tabelas de embedding BLOB-backed. Memórias existentes são preservadas; seus embeddings são recomputados lazy na próxima escrita.

Para forçar uma migração explícita:

```bash
sqlite-graphrag init --force
```

A saída inclui `schema_version: 13` quando a migração completa. Bancos v1.0.74 ou v1.0.75 existentes reportarão `schema_version: 12` até `init` rodar.

### Comando Dedicado de Migração

A v1.0.76 introduz dois subcomandos novos para migração controlada:

```bash
# Recalcular checksums de migração para casar com o conteúdo atual
sqlite-graphrag migrate --rehash --json

# Upgrade one-shot para LLM-only (rehash + V013 + drop das vec tables)
sqlite-graphrag migrate --to-llm-only --drop-vec-tables --json
```

O `--drop-vec-tables` é uma guarda de segurança explícita: a CLI exige confirmação consciente antes de destruir dados. Use `--dry-run` antes para auditar.

## Passo 3 — Re-Embed (Opcional)

Se você tem um corpus grande, re-embede com o loop one-shot canônico (G42/S9, v1.0.79). Cada invocação processa um lote PEQUENO e ENCERRA, então o job sobrevive a qualquer janela de supervisor externo:

```bash
# Re-embedar memórias sem linha vetorial, 5 por invocação.
# Repita (loop externo) até o resumo reportar 0 itens completados.
sqlite-graphrag enrich --operation re-embed --limit 5 --resume --json
```

Para forçar UMA memória a re-embedar sem tocar no body, use `edit --force-reembed` (v1.0.79):

```bash
sqlite-graphrag edit --name minha-memoria --force-reembed
```

ATENÇÃO — a receita pré-v1.0.79 (`edit --description "rewarm embedding"`) estava ERRADA: edições somente de descrição pulam o re-embedding por design (v1.0.63) e deixam `memory_embeddings` intocada.

## Passo 4 — Verificar o Caminho LLM

Rode um único `remember` para confirmar que a LLM está cabeada corretamente:

```bash
sqlite-graphrag remember \
    --name smoke-test \
    --type note \
    --description "smoke test" \
    --body "se você consegue ler isso, a LLM está funcionando"
```

A primeira chamada leva 1-3 segundos (spawn de subprocesso LLM). Chamadas subsequentes no mesmo processo não são amortizadas (a CLI é one-shot), mas o lado da LLM pode fazer cache do modelo de embedding internamente.

## O Que Quebra em Bancos v1.0.74

| Comportamento v1.0.74 | Comportamento v1.0.76 |
| --- | --- |
| `sqlite-graphrag daemon` mantém o modelo de embedding em memória | `sqlite-graphrag daemon` foi totalmente removido na v1.0.76; cada chamada de embedding spawna um subprocesso LLM |
| `--enable-ner` dispara o loader GLiNER ONNX (~30s cold start, 1.1 GB de download de modelo) | `--enable-ner` dispara só regex de URL. Use `--extraction-backend llm` para obter NER completo via LLM. |
| `vec_memories`, `vec_entities`, `vec_chunks` são virtual tables sqlite-vec | `memory_embeddings`, `entity_embeddings`, `chunk_embeddings` são tabelas BLOB-backed regulares |
| Modelo fastembed: `multilingual-e5-small` (local, determinístico) | Modelo LLM: `claude-sonnet-4-6` (claude) ou `gpt-5.4` (codex) (round-trip de rede) |
| Primeiro `init` baixa 1.1 GB de pesos ONNX | Primeiro `init` faz um round-trip LLM de 1-3 s |
| Dimensionalidade de embedding fixa em 384 | Default 64 desde a v1.0.79, configurável via `SQLITE_GRAPHRAG_EMBEDDING_DIM` (faixa [8, 4096]); bancos migrados mantêm a 384 registrada em todo comando (G43) e continuam pesquisáveis; `enrich --operation re-embed` re-embeda na dim ativa |

## Rollback

Se a v1.0.76 não está funcionando para você, a escotilha de escape é:

```bash
cargo install sqlite-graphrag --version 1.0.75 --force
```

Seu banco v1.0.76 já foi migrado para o novo schema (a migração V013 rodou no primeiro `init`). Reverter para v1.0.75 vai exigir `init --force` para recriar as vec tables — você vai perder os embeddings que construiu na v1.0.76 a menos que faça dump antes.

Para dumpar os embeddings da v1.0.76 antes do rollback:

```bash
sqlite3 graphrag.sqlite "SELECT memory_id, embedding FROM memory_embeddings" > embeddings-v1076.json
```

Depois de reinstalar a v1.0.75, você pode reimportar os embeddings rodando `init --force` da v1.0.75 e depois um `ingest` em lote dos corpos de memória originais. O pipeline fastembed da v1.0.75 vai re-embutir tudo do zero.

## Features Removidas

| Feature | Removida em | Substituta |
| --- | --- | --- |
| `--enable-ner` (GLiNER ONNX) | padrão v1.0.76 | `--extraction-backend llm` |
| `vec_memories` / `vec_entities` / `vec_chunks` (sqlite-vec) | v1.0.76 | `memory_embeddings` / `entity_embeddings` / `chunk_embeddings` (BLOB) |
| `daemon` (infraestrutura totalmente removida) | v1.0.76 | Nenhuma — o subprocesso LLM é o novo "carregador de modelo" |
| Variáveis `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` | v1.0.69 (ainda aplicadas) | OAuth via `claude login` / `codex login` |

