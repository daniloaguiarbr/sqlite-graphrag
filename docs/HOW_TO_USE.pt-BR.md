# COMO USAR sqlite-graphrag (v1.0.79 — Apenas LLM)

> Entregue memória persistente a qualquer agente de IA com um binário local, um único arquivo SQLite, e a CLI de LLM que você já confia.

- Versão em inglês: [HOW_TO_USE.md](HOW_TO_USE.md)
- Voltar ao [README.md](../README.md) para referência de comandos


## O Que Mudou na v1.0.79 (G42 + G43)

O trabalho do G42 tornou o pipeline de embedding rápido, paralelo e em lote; o G43 tornou universal a adoção da dimensionalidade:

- A dimensionalidade default de embedding caiu de 384 para 64 (configurável via `SQLITE_GRAPHRAG_EMBEDDING_DIM`, faixa [8, 4096]); bancos pré-existentes mantêm a `schema_meta.dim` registrada em todo comando (adoção em `open_rw`/`open_ro`, G43).
- Chamadas de embedding são em lote (`{items:[{i,v}]}`; chunks em 8, nomes de entidade em 25 em dim 64; adaptativos à dim — G44) e rodam em paralelo sob semáforo bounded: `--llm-parallelism` em `remember` (default 4), `ingest` (default 2) e `edit` (default 4), clamp [1, 32].
- `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` seleciona o modelo de embedding do claude; `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` (default 300) limita cada chamada LLM.
- `enrich --operation re-embed` e `edit --force-reembed` são os caminhos canônicos de re-embedding.
- O código restante do daemon foi deletado; as features `embedding-legacy` e `ner-legacy` foram removidas; `--enable-ner` é somente URL-regex e as flags da era GLiNER avisam como no-ops.


## O Que Mudou na v1.0.76

O build padrão agora é **apenas LLM e one-shot**. Não há modelo local de embedding, não há NER GLiNER, não há runtime ONNX, não há extensão C do `sqlite-vec`. Cada `remember`, `ingest`, `edit` spawna um subprocesso headless de LLM (CLI do claude code ou codex) que devolve o embedding e, opcionalmente, as entidades extraídas.

A CLI é one-shot: não há daemon, não há modelo a manter em memória, não há socket a limpar. O binário de release tem ~6 MB (era 39 MB) e o cold start é 1-3 s (era 30 s com a carga do modelo ONNX).


## Pré-Requisitos

Você precisa de UMA destas CLIs instalada e no `PATH`:

- `claude` — CLI do Claude Code 2.1.0+
  ([instalação](https://docs.claude.com/claude-code))
- `codex` — CLI do OpenAI Codex 0.130.0+
  ([repositório](https://github.com/openai/codex))

Ambas precisam estar logadas com o **fluxo OAuth** (assinatura Claude Pro/Max ou ChatGPT Pro). Chaves de API NÃO são suportadas — veja a seção "Validação OAuth" abaixo.

Para verificar:

```bash
which claude || which codex
claude --version
codex --version
```


## Validação OAuth

A v1.0.76 herda o mandato OAuth-only da v1.0.69. Se `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estiverem definidas no ambiente, o spawn da LLM ABORTA com `AppError::Validation` e a CLI sai com código 1.

Para remover:

```bash
unset ANTHROPIC_API_KEY
unset OPENAI_API_KEY
```

As duas variáveis de chave de API também são excluídas da whitelist de env-clear, então não conseguem burlar a checagem mesmo quando definidas em um processo pai.


## Instalação

```bash
cargo install sqlite-graphrag --version 1.0.79 --force
```

Isso instala o build padrão LLM-only. Verifique:

```bash
sqlite-graphrag --version
# sqlite-graphrag 1.0.79
```

Para o pipeline legado fastembed (REMOVIDO na v1.0.79):

```bash
# REMOVIDO na v1.0.79: a feature embedding-legacy não existe mais.
# As versões 1.0.76-1.0.78 a aceitavam; fixe uma dessas versões se
# precisar do pipeline fastembed legado (sem suporte).
```


## Inicializar um Banco

```bash
sqlite-graphrag init --namespace meu-projeto
```

O comando `init`:

1. Cria `graphrag.sqlite` no diretório atual.
2. Roda todas as migrações incluindo V013 (dropa vec tables, cria `memory_embeddings`, `entity_embeddings`, `chunk_embeddings`).
3. Spawna a LLM uma vez para confirmar que a sessão OAuth é válida.
4. Reporta `schema_version: 13` no sucesso.

O primeiro `init` é lento (1-3 s de round-trip LLM). Chamadas subsequentes são no-ops (o schema já está na versão alvo).


## Persistir Sua Primeira Memória

```bash
sqlite-graphrag remember \
    --name decisao-auth-2026-06 \
    --type decision \
    --description "Estratégia de rotação de token JWT com expiração de 15 min" \
    --body "Escolhemos JWT com access token de 15 minutos e
    refresh token de 7 dias. O fluxo de refresh usa cookies HttpOnly.
    Veja https://auth0.com/docs/refresh-tokens para a especificação." \
    --entities-file entidades.json
```

Onde `entidades.json` é:

```json
[
  {"name": "JWT", "entity_type": "concept"},
  {"name": "Auth0", "entity_type": "tool"}
]
```

O comando `remember`:

1. Chama a LLM para embutir o corpo — em lote e em paralelo desde a v1.0.79 (`--llm-parallelism`, default 4; 1-3 s por chamada).
2. Armazena a memória em `memories` (indexada por FTS5).
3. Armazena o embedding como BLOB em `memory_embeddings`.
4. Liga as entidades via tabela `entities`.
5. Retorna JSON com `memory_id`, `version`, `elapsed_ms`.


## Buscar Memórias

Os dois comandos principais de busca são:

```bash
# Busca por token exato + semântica, fundida via RRF
sqlite-graphrag hybrid-search "design auth jwt" --k 10 --json

# Apenas semântica (sem componente FTS5)
sqlite-graphrag recall "design auth jwt" --k 5 --no-graph --json
```

Para o tamanho padrão de namespace (10k memórias ou menos), o refinamento por cosseno sobre o BLOB de embedding é rápido o suficiente (ms de dígito único). Para namespaces maiores, prefira `hybrid-search` para que o FTS5 faça a filtragem grossa.


## Extrair Entidades via LLM

O `remember` padrão faz apenas extração de URL. Para NER completo (entidades + relacionamentos tipados), use o backend LLM:

```bash
sqlite-graphrag remember \
    --name revisao-design-t2 \
    --type note \
    --description "Notas da revisão de design do T2" \
    --body "$(cat revisao-design.md)" \
    --extraction-backend llm
```

A LLM devolve JSON estruturado com entidades e relacionamentos no mesmo prompt que produz o embedding. O round-trip total é 3-8 s (mais longo que o caminho de só embedding porque o prompt inclui o schema e a resposta é maior).


## Ferramentas de Qualidade LLM (herdadas da v1.0.69)
### `enrich` — Qualidade do Grafo Aumentada por LLM
- O subcomando `enrich` executa operações de qualidade do grafo curadas por LLM. Três estão totalmente implementadas: `memory-bindings` (extrai entidades de memórias órfãs), `entity-descriptions` (preenche descrições de entidade NULL ou vazias) e `body-enrich` (expande corpos curtos de memória em conteúdo mais rico).
- Duas operações adicionais são apenas de varredura e exibem listas candidatas sem reescrever: `weight-calibrate`, `relation-reclassify`, `entity-connect`, `entity-type-validate`, `description-enrich`, `cross-domain-bridges`, `domain-classify`, `graph-audit`, `deep-research-synth`, `body-extract`.
- `--mode claude-code` ou `--mode codex` seleciona o provedor LLM. O padrão é `claude-code`. Ambos os provedores são OAuth-only desde a v1.0.69.
- `--preflight-check` emite um ping de 1 turno ANTES de varrer o conjunto candidato. Em rate limit OAuth do Claude, a sondagem aborta com erro claro (ou troca para `--fallback-mode` quando fornecido). Padrão desligado para manter `--dry-run` e fluxos de CI com custo zero.
- `--fallback-mode <claude-code|codex>` troca automaticamente de provedor quando a sondagem de preflight ou uma chamada em voo atinge rate limit. Ignorado quando `--mode` já é `codex`.
- `--rate-limit-buffer <SEGUNDOS>` padrão 300. Quando a sondagem detecta que o reset do rate limit OAuth está a menos do que o buffer de distância, aborta com sugestão para esperar.
- `--names <a,b,c>` e `--names-file <CAMINHO>` selecionam um subconjunto específico de nomes de memória em vez de varrer todos os candidatos. `--names-file` aceita comentários `#` e linhas em branco. As duas flags se combinam como união quando ambas estão setadas.
- `--preserve-threshold <FLOAT>` (padrão 0.7) controla o portão de similaridade trigrama Jaccard para `body-enrich`. Quando a reescrita do LLM pontua abaixo do threshold, o corpo enriquecido é REJEITADO e emitido como `EnrichItemResult::PreservationFailed`. Protege contra invenção do LLM.
- `--llm-parallelism <N>` spawna N threads de worker LLM em paralelo (padrão 1, máximo 32). Codex tolera até 16 em produção; Claude avisa acima de 4 por causa da fan-out OAuth-MCP. Desde a v1.0.79 a mesma flag também existe em `remember` (default 4), `ingest` (default 2) e `edit` (default 4) para o fan-out de embedding.
- `--max-load-check` recusa iniciar quando o load average de 1 minuto excede `2 × ncpus`. Defina como false em runners de CI disputados.
- `--circuit-breaker-threshold <N>` (padrão 5) aborta o job após N resultados `HardFailure` consecutivos. Erros transient de rate limit e timeout não contam.
- `--codex-model-validate` (padrão true) verifica `--codex-model` contra a lista de modelos aceitos pelo ChatGPT Pro OAuth ANTES de o subprocesso ser spawnado. Use `--codex-model-fallback <MODELO>` para auto-substituir um modelo conhecido em vez de abortar.
- `--dry-run` faz preview do conjunto candidato sem spawnar nenhum LLM. A saída é NDJSON com um evento por memória e um resumo final.
- `--resume` continua um batch interrompido anteriormente a partir do queue DB. `--retry-failed` retenta apenas os itens que falharam.
### `vec` — Manutenção do Índice Vetorial (G39)
- `vec orphan-list --json` lista linhas de embedding de memória cujo `memory_id` não existe mais na tabela `memories`. Cada linha reporta o `vector_hash` (BLAKE3 do blob de embedding) para rastreabilidade.
- `vec purge-orphan --yes --dry-run --json` faz preview da contagem de deleção sem remover nada.
- `vec purge-orphan --yes --json` purga as TRÊS vec tables (`vec_memories`, `vec_entities`, `vec_chunks`) em uma única transação implícita. A resposta reporta `deleted`, `deleted_entities`, `deleted_chunks` e `elapsed_ms`.
- `vec stats --json` expõe `vec_memories_rows`, `vec_entities_rows`, `vec_chunks_rows`, `orphans` e o timestamp do último vacuum. Use para auditar a saúde das vec tables após ciclos de `forget` em massa.
- O subcomando `forget` agora chama `memories::delete_vec` ANTES do soft-delete, prevenindo novos órfãos em estado estável.
### `codex-models` — Descobrir Modelos ChatGPT Pro OAuth (G33)
- `codex-models --json` retorna a lista de modelos aceitos, a contagem e o padrão. Atualmente: `codex-auto-review`, `gpt-5.3-codex-spark`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.5`.
- `codex-models --suggest <substring> --json` retorna a correspondência mais próxima via busca por substring com fallback Levenshtein. Útil quando um operador digita `o4-mini` e quer saber a alternativa aceita mais próxima.
### Endurecimento de `optimize` e `backup` (G36 + G38)
- `optimize` agora faz pré-verificação da saúde do FTS5 via `check_fts_functional` ANTES de reconstruir. Um índice saudável não é mais reconstruído (economiza ~10 minutos em um banco de 4.3 GB). Force a reconstrução com `--no-fts-skip-when-functional`.
- `optimize --fts-dry-run --json` sai com código 1 se o índice FTS5 precisar de reconstrução, 0 caso contrário. Amigável para CI.
- `optimize --fts-progress <N>` (padrão 30) emite uma linha de progresso a cada N segundos durante a reconstrução. Defina como 0 para desabilitar.
- `optimize --yes` pula o prompt de confirmação. Obrigatório para CI não interativo.
- `backup` usa por padrão `run_to_completion(1000, Duration::from_millis(5), None)` (era 100/50ms). Para um banco de 4.3 GB isso é um speedup de 25x (~21s vs ~9 min).
- `backup --backup-step-size <PAGES>` e `--backup-step-sleep-ms <MS>` ajustam a granularidade de cópia de páginas. `--backup-no-sleep` remove o sleep entre steps totalmente para máximo throughput. `--backup-progress <PAGES>` (padrão 100) emite uma linha de progresso a cada N páginas.
### Família de Subcomandos `migrate` (v1.0.76, atualizado v1.0.77 e v1.0.78)
- `migrate --rehash --json` reescreve os checksums registrados de migração para casar com o conteúdo atual do arquivo. Idempotente. Obrigatório para upgrades v1.0.74 → v1.0.76 onde a migração V002 foi intencionalmente esvaziada para um no-op.
- `migrate --to-llm-only --drop-vec-tables --json` é o upgrade one-shot para bancos v1.0.74 / v1.0.75. Combina `--rehash` com o descarte da V013 das vec tables. A flag `--drop-vec-tables` é OBRIGATÓRIA como rede de segurança explícita. As tabelas com backing BLOB `memory_embeddings` / `entity_embeddings` / `chunk_embeddings` permanecem e são a fonte de verdade daqui em diante; embeddings são recomputados preguiçosamente no próximo `remember` / `edit` / `ingest`.
- Correção v1.0.77 (G40): a resposta JSON de ambos os comandos agora inclui `null_rows_fixed` (inteiro) e `vec_tables_removed_via_writable_schema` (inteiro). Bancos com linhas `applied_on = NULL` são sanitizados automaticamente antes do migration runner executar.
- Correção v1.0.78 (G41): a resposta JSON de ambos os comandos agora inclui `v013_tables_created` (boolean). Bancos onde V013 foi registrada em `refinery_schema_history` mas as tabelas BLOB-backed de embedding nunca foram criadas são reparados automaticamente. Qualquer comando CRUD também dispara esse reparo incondicionalmente via `ensure_db_ready`.


## Migração da v1.0.74 ou v1.0.75

Veja [MIGRATION.md](MIGRATION.md) para o passo a passo completo. A versão curta:

1. Instale a v1.0.76 (LLM-only).
2. Rode `sqlite-graphrag init` — a migração V013 roda automaticamente.
3. As vec tables antigas são dropadas; a nova `memory_embeddings` começa vazia.
4. As memórias são re-embutidas lazy no próximo `edit` ou `ingest`.

Para um corpus grande, use o loop one-shot canônico de re-embed (G42/S9, v1.0.79) — cada invocação processa um lote pequeno e encerra:

```bash
sqlite-graphrag enrich --operation re-embed --limit 5 --resume --json
```

Nota: a receita antiga `edit --description "<mesmo>"` nunca re-embedou nada (edições somente de descrição são no-op para embeddings); use `edit --force-reembed` para uma única memória.


## Ambiente de Teste em CI

Se você quer rodar a suíte completa de testes em CI, precisa de uma CLI de LLM no `PATH`. O build da v1.0.76 não embute via fastembed na configuração padrão, então `v1044_features`, `signal_handling_integration` e `v2_breaking_integration` vão falhar com `no LLM CLI found on PATH` quando nem `claude` nem `codex` estiverem instalados.

Soluções alternativas:

1. Instale `claude` na imagem de CI e autentique via OAuth (requere guardar tokens OAuth em segredos de CI).
2. Use uma CLI de LLM mock que devolve uma resposta JSON fixa para o prompt de embedding (usada internamente pelos testes unitários em `src/extract/llm_embedding.rs`).


## Veja Também

- [COOKBOOK.md](COOKBOOK.md) para receitas comuns
- [MIGRATION.md](MIGRATION.md) para upgrade v1.0.74 → v1.0.76
- [CROSS_PLATFORM.md](CROSS_PLATFORM.md) para Windows e macOS
- [AGENTS.md](AGENTS.md) para integração com agentes
- [HEADLESS_INVOCATION.md](HEADLESS_INVOCATION.md) para invocação headless OAuth-safe de Claude/Codex/OpenCode
- [decisions/](decisions/) para os 26 ADRs

