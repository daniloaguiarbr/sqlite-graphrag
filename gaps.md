# Gaps — Problemas Documentados da CLI sqlite-graphrag


## Restrições Invioláveis do Projeto
### Restrição 1 — Estrutura do Banco SQLite Imutável
- PROIBIDO alterar a estrutura do banco de dados SQLite
- PROIBIDO criar migração nova, executar `ALTER TABLE` ou adicionar tabela, coluna ou índice
- Toda solução DEVE caber no schema v13 atual
- Evidência de viabilidade para o G42: a tabela `schema_meta` key-value existe desde `V001__init.sql:3-6`
- A chave `dim` JÁ é gravada em `schema_meta` no bootstrap (`connection.rs:109-127`)
- As 3 tabelas de embedding JÁ têm coluna `dim INTEGER NOT NULL DEFAULT 384` (`V013__drop_vec_use_blob_embeddings.sql:18-52`)
- CONCLUSÃO: dimensão configurável é um UPDATE de chave existente, ZERO mudança estrutural
### Restrição 2 — CLI Obrigatoriamente LLM-Only
- PROIBIDO usar modelos locais de embedding ou NER em qualquer solução
- PROIBIDO reintroduzir ONNX, fastembed, ort, ndarray, tokenizers, sqlite-vec, GLiNER ou qualquer modelo embarcado
- Esses crates foram removidos na v1.0.76 (ADR-0019, ADR-0023, ADR-0025) e a remoção é DEFINITIVA
- Todo embedding nasce OBRIGATORIAMENTE de `claude -p` ou `codex exec` headless OAuth
- PROIBIDO "resolver" lentidão regredindo a modelo local
- Toda otimização DEVE atuar no caminho LLM: dimensão, batching, paralelismo e configuração
- PROIBIDO reintroduzir daemon — a arquitetura one-shot permanece (ADR-0021)
### Elos Causa × Efeito das Restrições
- PORQUE modelos locais são proibidos, o ganho de performance do G42 DEVE vir exclusivamente de S1 (dimensão), S2 (batching), S3 (paralelismo) e S6 (configuração)
- PORQUE o schema é imutável, S1 usa a chave `dim` EXISTENTE em `schema_meta` e a coluna `dim` EXISTENTE das tabelas de embedding
- Violar a restrição de schema CAUSARIA migração V014, QUE CAUSARIA incompatibilidade com bancos v1.0.76+ em produção


## G42 — Pipeline de Embedding LLM One-Shot Lento, Serializado e Frágil (v1.0.78)
### Status
- RESOLVIDO na v1.0.79 — S1 a S9 e C5 implementados antes da publicação; ver CHANGELOG `[1.0.79]` e os gaps derivados G43 e G44
### Contexto e Evidências Medidas
- Um `remember` com body de 8.6 KB gerou 18 chunks e 21 entidades, totalizando 39 itens para embeddar
- A medição real foi 1.566.756 ms, ou seja, 26 MINUTOS para uma única memória
- Cada item levou ~40 segundos em média no caminho one-shot
- O re-embed de 1023 memórias faltantes foi estimado em ~12 HORAS
- A flag `--llm-parallelism 8` resultou em paralelismo efetivo 1, medido via `procs`
- Cada `claude -p` com config completo carregou 223.139 tokens de cache_creation com 6 MCPs e 13 plugins
- O mesmo `claude -p` com `CLAUDE_CONFIG_DIR` vazio carregou 0 tokens de input e respondeu em ~10 segundos
- O cold start medido foi ~1.7s por subprocesso codex e ~2.3s por subprocesso claude
- Um job de re-embed em background morreu com exit 144 após ~80 minutos, processando 112 de 1135 memórias por sessão
- O crash report registrou SIGABRT na thread "ctrl-c" com `parentPid: 1`, indicando processo orfanado
- O fluxo do `remember` tem 4 etapas: parse e validação (~50ms), chunking (~10ms), embedding (26 MINUTOS) e gravação (~100ms)
- A etapa de embedding é o gargalo com 99.99% do tempo total
### Problema
- O pipeline de embedding LLM trata cada texto como uma operação atômica isolada
- Cada texto spawna um subprocesso LLM completo que nasce, processa 1 item e morre
- Todos os embeddings passam por um Mutex global que serializa o processo inteiro
- O LLM gera 384 floats como texto JSON autoregressivo, token por token
- Entidades de 6 bytes pagam o mesmo custo que chunks de 500 bytes
- O caminho claude carrega 223K tokens de configuração desnecessária por invocação
- O handler de sinais aborta o processo quando o stderr é um pipe fechado
- A recomendação documentada de warm-up de embeddings não funciona
### Consequências do Problema
- Consequências diretas de tempo: 26 minutos por `remember` e ~12 horas por re-embed de corpus
- Consequências diretas de custo: ~3072 tokens de output por vetor, e output custa mais que input nas APIs
- Consequências diretas de hardware: a CPU fica ociosa 95% do tempo esperando I/O de rede
- Consequência de segunda ordem: jobs de horas colidem com a janela de ~80 minutos do harness do Claude Code
- A colisão com o harness causa exit 144 e re-embed parcial garantido de 9.9% por sessão
- O orfanamento do shell pai fecha os pipes de stdout e stderr
- O pipe fechado dispara o bug do handler de sinais e mata o processo com SIGABRT
- Re-embeds parciais deixam memórias sem embedding no banco
- Memórias sem embedding degradam `recall` e `hybrid-search` silenciosamente
- O usuário é forçado a criar workarounds externos: `one-shot-loop.sh`, `nohup`, `disown` e round-robin de contas
- O custo por entidade desencoraja o uso de grafo rico, que é a proposta de valor central da ferramenta
### Causa Raiz do Problema
- CAUSA PRIMÁRIA ARQUITETURAL: o design "1 texto = 1 chamada síncrona sob Mutex" foi herdado da era do modelo ONNX local
- Com ONNX local cada vetor custava ~10ms e o Mutex global era inócuo
- A v1.0.76 trocou o gerador por LLM autoregressivo de 30-60s por vetor SEM redesenhar o pipeline
- O perfil de custo mudou 3000× mas a arquitetura de chamada permaneceu idêntica
### Causa Raiz — Grupo A: Lentidão do Pipeline
- A1 — Dimensão 384 hardcoded em DUAS fontes de verdade: `src/extract/llm_embedding.rs:26` e `src/constants.rs:21` (violação DRY)
- A1 — O schema JSON `"minItems":384,"maxItems":384` está hardcoded 2 vezes: `llm_embedding.rs:238` (claude) e `llm_embedding.rs:287` (codex)
- A1 — 384 floats × ~8 tokens por float ≈ 3072 tokens de output autoregressivo a 50-100 tokens/s = 30-60s por vetor
- A2 — 1 subprocesso LLM por item: `invoke_with_prefix` (`llm_embedding.rs:192`) spawna 1 processo por texto
- A2 — Cada spawn paga fork, runtime init, leitura de OAuth do disco e handshake TLS: 39 cold starts = 66-90s de overhead morto
- A2 — Um runtime tokio current-thread é criado POR CHAMADA quando fora de runtime (`llm_embedding.rs:207-211`)
- A3 — Mutex global serializa TUDO: `static EMBEDDER: OnceLock<Mutex<LlmEmbedding>>` em `src/embedder.rs:26`
- A3 — `flush_group` (`embedder.rs:88-100`) SEGURA o lock DURANTE o loop sequencial que faz I/O de rede de 30-60s por item
- A3 — Esse lock segurado durante I/O é a razão exata do paralelismo efetivo 1 com `--llm-parallelism 8`
- A3 — Viola a rule rust: NUNCA segurar lock durante await ou I/O
- A4 — Entidades curtas usam o mesmo pipeline de chunks: `remember.rs:627,664,710` chamam `embed_passage_local` item a item
- A4 — 21 nomes de 3-15 caracteres consumiram ~12 minutos, ou seja, 46% do tempo total
- A4 — `embed_passages_controlled` (`embedder.rs:59`) faz batch APENAS contábil: agrupa para orçamento de tokens mas executa item a item sob o mesmo lock
- A4 — `REMEMBER_MAX_CONTROLLED_BATCH_CHUNKS = 4` (`constants.rs:159`) limita ainda mais o agrupamento
- A5 — Schema temporário recriado a cada chamada: `llm_embedding.rs:288-293` escreve e `llm_embedding.rs:307` deleta o arquivo POR item
- A5 — O path `sqlite-graphrag-embed-schema-{pid}.json` é por PID e COLIDE sob paralelismo futuro: worker A deleta enquanto o codex do worker B lê
- A5 — `codex-home-{pid}` (`llm_embedding.rs:363-377`) é criado por processo e NUNCA limpo, acumulando em `~/.local/share/sqlite-graphrag/`
### Causa Raiz — Grupo B: Custo e Configuração dos Subprocessos
- B1 — Modelo claude hardcoded: `llm_embedding.rs:125,154` fixam `claude-sonnet-4-6` SEM env override (PROIBIDO MODELO hardcoded)
- B1 — O caminho codex já tem `SQLITE_GRAPHRAG_CODEX_EMBED_MODEL` desde a v1.0.78, criando assimetria entre backends
- B2 — `CLAUDE_CONFIG_DIR` vazio NÃO é injetado no caminho de embedding: `invoke_claude` (`llm_embedding.rs:239-261`) faz `env_clear()` e injeta só PATH e HOME
- B2 — Sem a env var, o claude cai no `~/.claude` padrão e carrega ~223.139 tokens de configuração POR invocação
- B2 — As flags `--strict-mcp-config --mcp-config '{}'` são SILENCIOSAMENTE IGNORADAS pelo Claude Code (issue anthropics/claude-code#10787)
- B2 — O mecanismo G28-A (`SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR`) existe SÓ em `claude_runner.rs` e `enrich.rs`, mas o caminho de embedding não o honra
- B2 — REQUISITO: o diretório de configuração vazio DEVE ser o PADRÃO no caminho de embedding
- B3 — O codex falha com `request_user_input is unavailable in Default mode` e exit 11 em contexto não-TTY
- B3 — O spawn de embedding precisa garantir modo headless completo em TODOS os caminhos
- B3 — O contorno atual (`--extraction-backend none`) salva a memória SEM embedding, degradando a busca vetorial
### Causa Raiz — Grupo C: Fragilidade Operacional
- C1 — O harness do Claude Code mata jobs background em ~80 minutos com exit 144 (128+16, sinal externo)
- C1 — Com o pipeline lento, QUALQUER re-embed de corpus real excede a janela e o término parcial é garantido
- C1 — A causa não é a CLI receber o sinal, e sim a lentidão do Grupo A tornar obrigatórios jobs de horas
- C2 — SIGABRT por `eprintln!` em handler de sinal com pipe quebrado: `src/signals.rs:24`
- C2 — Quando o processo é orfanado, o stderr é um pipe FECHADO e o `eprintln!` panica com BrokenPipe
- C2 — Com `panic = "abort"` no perfil release, o panic vira `abort()` na thread "ctrl-c" — exatamente o stack `__pthread_kill → abort` do crash report
- C2 — `tracing::warn!` (`signals.rs:19`) também escreve em stderr no mesmo cenário vulnerável
- C2 — Viola as rules rust: TRATAR BrokenPipe como exit gracioso 141, NUNCA usar `eprintln!` em produção, NUNCA panicar ao receber sinal esperado
- C3 — Warm-up documentado quebrado: as docs v1.0.76 recomendam loop `edit --description "<mesmo>"` para pré-aquecer embeddings
- C3 — Mas `edit` com descrição idêntica é no-op e NÃO gera embedding (comportamento v1.0.63: edições somente de descrição não re-embedam)
- C3 — `vec_memories` permanece inalterado e o usuário precisa de `remember --force-merge` (26 minutos) ou mudança artificial de body
- C4 — O modo one-shot não é o padrão documentado do `enrich`: `--limit` (`enrich.rs:377`) e `--resume` (`enrich.rs:421`) EXISTEM mas não são recomendados como padrão operacional
- C4 — O usuário precisou escrever `one-shot-loop.sh` EXTERNO com round-robin de backends
- C4 — REQUISITO: cada invocação do `enrich` deve ser one-shot — abre, processa um lote pequeno e fecha (conformidade com rules_rust_cli_one_shot)
- C5 — Descarte silencioso de dimensão errada: `normalise_dim` (`embedder.rs:143-153`) trunca ou preenche silenciosamente vetores com dimensão divergente
- C5 — Isso mascara respostas LLM malformadas e viola a rule: prefira erro claro a comportamento silencioso
### Relações Causa × Efeito
- A1 (384 dims) CAUSA 3072 tokens de output, QUE CAUSA 30-60s de geração por vetor
- A2 (1 subprocesso por item) CAUSA 39 cold starts, QUE CAUSAM 66-90s de overhead morto sem nenhum token gerado
- A3 (Mutex global segurado no loop de I/O) CAUSA paralelismo efetivo 1, QUE CAUSA tempo total = SOMA dos itens em vez de máximo
- A4 (entidades sem batch dedicado) CAUSA 21 chamadas para nomes curtos, QUE CAUSAM 12 minutos desperdiçados (46% do total)
- A1 + A2 + A3 + A4 juntas CAUSAM os 26 minutos por `remember` e as ~12 horas por re-embed de corpus
- B2 (223K tokens de configuração) CAUSA ~40-50s por `claude -p`, QUE CAUSA preferência forçada pelo codex
- A lentidão dos grupos A e B CAUSA jobs de horas em background
- Jobs de horas CAUSAM colisão com a janela de ~80 minutos do harness, QUE CAUSA exit 144 e re-embed parcial de 9.9% por sessão
- O orfanamento do shell pai CAUSA stderr e stdout virarem pipes fechados
- Pipe fechado + `eprintln!` no handler (C2) CAUSA panic BrokenPipe, QUE CAUSA SIGABRT com `panic = "abort"`
- O re-embed parcial CAUSA memórias sem embedding, QUE CAUSA `recall` e `hybrid-search` degradados
- C3 (warm-up quebrado) CAUSA impossibilidade de regenerar embedding sem reescrever body, QUE CAUSA uso de `remember --force-merge` de 26 minutos como contorno
- Todo o conjunto CAUSA workarounds manuais externos (one-shot-loop.sh, nohup, disown, round-robin de contas), QUE CAUSAM complexidade operacional fora da CLI
- A restrição LLM-only (Restrição 2) CAUSA que toda otimização atue no caminho LLM, QUE CAUSA S1+S2+S3+S6 serem as ÚNICAS alavancas de performance disponíveis
- A restrição de schema imutável (Restrição 1) CAUSA o uso da chave `dim` EXISTENTE em `schema_meta` e da coluna `dim` EXISTENTE, QUE CAUSA retrocompatibilidade total sem migração V014
- Paralelismo SEM bounded concurrency CAUSARIA rajada de subprocessos simultâneos, QUE CAUSARIA rate limit HTTP 429 do provedor LLM e RAM esgotada (N × 200-400 MB por subprocesso node)
- O Semaphore com permit RAII CAUSA teto rígido de subprocessos, QUE CAUSA uso de RAM e taxa de requisições previsíveis
- Persistência incremental via canal bounded CAUSA progresso preservado item a item, QUE CAUSA resiliência ao exit 144 — o kill perde no máximo o lote em voo
- `kill_on_drop` + `wait` obrigatório CAUSAM zero subprocessos zumbis, QUE CAUSA eliminação dos órfãos `claude` e `codex` com `PPID=1` que o reaper precisa matar hoje
- O cancel via token nas tasks CAUSA shutdown limpo no primeiro sinal, QUE ELIMINA a janela em que o segundo sinal encontrava o processo no meio de I/O (reforça S8)
### Solução
- S1 (resolve A1): dimensão de embedding configurável com default 64
- S1: flag global `--embedding-dim N` + env `SQLITE_GRAPHRAG_EMBEDDING_DIM`
- S1: fonte ÚNICA de verdade para a constante de dimensão, eliminando a duplicação entre `constants.rs` e `llm_embedding.rs`
- S1: schema JSON gerado dinamicamente a partir da dimensão configurada
- S1: dimensão escolhida persistida na chave `dim` EXISTENTE de `schema_meta` (V001) e na coluna `dim` EXISTENTE das tabelas de embedding (V013) — ZERO mudança de schema, conforme a Restrição 1
- S1: cosseno dinâmico exigindo apenas `a.len() == b.len()`
- S1: base científica MRL (arXiv 2205.13147) — 64 dimensões retém 90%+ da qualidade de retrieval para corpus < 100K memórias
- S2 (resolve A2 e A4): batching de N textos por chamada LLM com schema `{items:[{i,v}]}`
- S2: chunks em lotes de 8 por chamada (textos longos); desde o G44 o 8 é a BASE para dim 64, adaptada por clamp(base×64/dim, 1, base)
- S2: entidades em lotes de 25 por chamada (textos curtos), em grupo separado por perfil de tamanho; desde o G44 o 25 é a BASE para dim 64, adaptada pela mesma fórmula
- S2: prompt numerado listando os textos, resposta com índice e vetor por item
- S2: 39 chamadas LLM viram 4-5 chamadas
- S3 (resolve A3): paralelismo real removendo o lock-durante-I/O de `flush_group`
- S3: o Mutex passa a proteger apenas a clonagem da configuração do embedder
- S3: `JoinSet` + `Semaphore` REUTILIZANDO o padrão async já existente em `deep_research.rs:314-391` (`Semaphore::new` + `acquire_owned` + `join_next` + `is_panic`)
- S3: o clamp `llm_parallelism.clamp(1,32)` segue o precedente de `enrich.rs:1685`, que usa `std::thread::scope` como modelo síncrono alternativo
- S3: toda solução respeita a seção Restrições Invioláveis do Projeto no topo deste documento
- S3: flag `--llm-parallelism` adicionada a `remember`, `edit` e `ingest`
- S4 (resolve A5): schema file criado UMA vez por processo via `tempfile::NamedTempFile` com nome único randomizado e cleanup RAII
- S4: elimina o I/O redundante, a race do path por PID e os arquivos órfãos no tmpdir
- S4: estender o reaper de órfãos existente para limpar `codex-home-{pid}` antigos no startup
- S5 (resolve B1): env `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` para o caminho claude, simétrico ao codex
- S5: ZERO modelo hardcoded em qualquer caminho de embedding
- S6 (resolve B2): `CLAUDE_CONFIG_DIR` vazio POR PADRÃO no caminho de embedding
- S6: honrar `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` quando definida
- S6: quando ausente, criar e usar diretório vazio gerenciado em `~/.local/state/sqlite-graphrag/claude-empty-config/` com mode 0700
- S6: ganho medido — 223K tokens viram 0 e ~40-50s viram ~10-15s por chamada
- S7 (resolve B3): garantir modo headless completo no spawn codex de embedding em TODOS os caminhos
- S7: tratar o erro `request_user_input` com mensagem acionável em vez de exit 11 opaco
- S8 (resolve C2): handler de sinais seguro sem panic possível
- S8: substituir `eprintln!` e `tracing::warn!` no handler por escrita best-effort que IGNORA erro de I/O
- S8: tratar BrokenPipe globalmente com exit 141 conforme convenção UNIX
- S8: segundo sinal termina com exit 130 SEM nenhuma operação de I/O
- S9 (resolve C3 e C4): caminho canônico de re-embed documentado como one-shot
- S9: documentar `enrich --operation re-embed --limit N --resume` como padrão oficial — cada invocação processa N itens e ENCERRA
- S9: corrigir as docs v1.0.76 removendo a recomendação quebrada do loop `edit --description`
- S9: adicionar flag `edit --force-reembed` para regenerar embedding sem alterar o body
### Operações de Paralelismo e Multiprocessamento
- Esta seção detalha o desenho concorrente de S3 conforme as rules rust de paralelismo (graphrag: `rules-rust-paralelismo-e-multiprocessamento`)
- BLOCO 1 — Classificação de workload (OBRIGATÓRIA):
- O embedding é I/O-bound + subprocess-bound: 30-60s de espera de rede por chamada com CPU local ociosa
- Paralelizar com tokio (concorrência de I/O), NUNCA com rayon (reservado a CPU-bound)
- A classificação DEVE ficar documentada em comentário no topo do módulo
- BLOCO 2 — Bounded concurrency (lei central):
- `Arc<tokio::sync::Semaphore>` com `acquire_owned().await` ANTES de cada `Command::spawn` de LLM
- O permit é RAII: devolvido no fim da task INCLUSIVE em panic
- Fórmula de permits: `permits = min(flag --llm-parallelism, available_parallelism(), ram_livre × 0.5 / rss_por_subprocesso, teto de rate limit do provedor)`
- Subprocessos `claude -p` são node com RSS típico de 200-400 MB — o limite por RAM é PERTINENTE
- Medição de RSS OBRIGATÓRIA antes de calibrar: `/usr/bin/time -v claude -p ...` no Linux ou `/usr/bin/time -l` no macOS, lendo Maximum resident set size
- O valor medido e a fórmula DEVEM ser documentados em comentário no código
- Clamp final `clamp(1, 32)` consistente com `enrich --llm-parallelism`
- Flag `--llm-parallelism` exposta em `remember`, `edit` e `ingest` (hoje só `--max-concurrency` global existe em `cli.rs:35-36`)
- BLOCO 3 — Padrão de implementação (reutilizar código interno):
- O padrão canônico async JÁ EXISTE em `deep_research.rs:314-391`: `Semaphore::new(permits)` + `JoinSet` + `acquire_owned().await` + coleta incremental `join_next()` + `JoinError::is_panic()` tratado
- `enrich.rs:1766` usa `std::thread::scope` com conexões WAL separadas — modelo síncrono alternativo válido, mas o de `deep_research.rs` é o canônico a seguir
- Lock NUNCA cruza I/O: o Mutex global passa a proteger apenas a clonagem da config (`LlmEmbedding` = flavour + paths + model, Clone barato)
- Isso elimina o anti-pattern de `flush_group` (`embedder.rs:88-100`) que segura o lock durante 30-60s de rede
- Runtime tokio multi_thread criado UMA vez por comando (como `deep_research.rs:252-257`), nunca por chamada (corrige `llm_embedding.rs:204-211`)
- BLOCO 4 — Higiene de subprocessos (multiprocessamento):
- `kill_on_drop(true)` em todo `tokio::process::Command` de embedding — cancel não deixa processo vivo
- `wait` SEMPRE aguardado: zero zumbis `<defunct>` após qualquer caminho de saída
- `tokio::time::timeout` por chamada LLM, consistente com `--claude-timeout` e `--codex-timeout` existentes
- Linux opcional: encapsular subprocessos com `systemd-run --scope -p MemoryMax` quando disponível, VALIDANDO disponibilidade antes de assumir, com fallback transparente no macOS
- O reaper de órfãos existente (`src/reaper.rs`) cobre o cenário de crash — estender o escopo para limpar `codex-home-{pid}` órfãos (já previsto em S4)
- BLOCO 5 — Backpressure e persistência incremental:
- Resultados fluem por `tokio::sync::mpsc::channel(N)` BOUNDED do estágio de embedding para o estágio de gravação SQLite (single-writer WAL)
- Gravar cada embedding ASSIM QUE chega, sem acumular o corpus inteiro em memória
- Persistência incremental CAUSA progresso preservado mesmo sob exit 144 do harness — itens já gravados sobrevivem ao kill, mitigando C1 antes mesmo do speedup
- PROIBIDO `unbounded_channel` entre estágios
- BLOCO 6 — Cancelamento e graceful shutdown:
- Propagar o `CancellationToken` JÁ EXISTENTE (`lib.rs:56`, `cancel_token()`) a cada task via `tokio::select!` entre o trabalho e `token.cancelled()`
- Cada branch do `select!` DEVE ser cancel-safe
- O handler de sinal (S8) cancela o token, as tasks drenam com timeout e o segundo sinal sai com exit 130 SEM I/O
- `JoinSet::shutdown().await` no caminho de abort para encerrar tasks pendentes
- BLOCO 7 — Proibições (antipadrões):
- NUNCA `tokio::spawn` em loop sem permit — fork bomb de subprocessos node de 200-400 MB
- NUNCA `futures::join_all`, `FuturesUnordered` ou `buffer_unordered` sem bound sobre coleção de tamanho desconhecido
- NUNCA segurar `MutexGuard` através de `.await`
- NUNCA descartar `JoinHandle` de task crítica sem await
- NUNCA criar runtime dentro de runtime existente
- BLOCO 8 — Observabilidade de saturação:
- Emitir `available_permits()` e contadores completed, failed e cancelled via `tracing` no NDJSON de progresso
- Span por lote com `#[tracing::instrument]` incluindo índice do lote e backend usado
- Logar o tempo de espera por permit SEPARADO do tempo de trabalho útil
- BLOCO 9 — Testes obrigatórios de concorrência:
- Teste 1: disparar 10× mais lotes que permits e assertar via `AtomicUsize` que o pico de concorrência NUNCA excede N
- Teste 2: panic dentro de task devolve o permit via RAII — validar `available_permits()` antes e depois
- Teste 3: cancel durante embedding não deixa subprocesso zumbi (verificar via `procs` após o teste)
- Teste 4: graceful shutdown com tasks em andamento termina dentro do timeout
- Usar `loom` se primitivas de sincronização customizadas forem introduzidas; rodar com `--test-threads=1` onde o determinismo exigir
- BLOCO 10 — Documentação obrigatória no código:
- Classificação de workload em comentário no topo do módulo
- Fórmula de permits e valor de RSS medido documentados
- Cancel safety de cada future pública documentada
### Benefícios da Solução
- A1→S1: output por vetor cai de ~3072 para ~512 tokens (speedup 6×)
- A2→S2: cold starts caem de 39 para 4-5 (speedup 9× no overhead)
- A3→S3: tempo total vira o máximo do lote em vez da soma (speedup 4× com parallelism 4)
- A4→S2: entidades caem de 21 chamadas para 1 chamada (speedup 24× só nas entidades)
- A5→S4: 39 escritas de schema viram 1 e a race condition é eliminada
- B2→S6: 223K tokens de contexto viram 0 por `claude -p` (speedup 3-4× no caminho claude)
- COMBINADO: `remember` de 39 itens cai de ~26 minutos para ~30s-1min (speedup ~25-50×)
- Efeito de segunda ordem: re-embed de 1023 memórias cai de ~12 horas para ~30-60 minutos
- Jobs curtos cabem na janela do harness e eliminam a classe inteira de exit 144 derivados de duração
- O fim do SIGABRT no handler elimina crashes em processos orfanados
- Conformidade com as rules rust: DRY (fonte única de dimensão), proibição de hardcode, fail-fast, zero descarte silencioso, BrokenPipe = exit 141, one-shot por invocação
- O custo baixo por entidade volta a viabilizar grafos ricos, que são a proposta de valor da ferramenta
- Bounded concurrency torna RAM e taxa de requisições previsíveis sob qualquer carga
- Persistência incremental elimina a perda de progresso em kills externos, mitigando C1 independente do speedup
- Zero zumbis e shutdown limpo eliminam a classe de órfãos `claude`/`codex` que o reaper caça hoje
- Conformidade com o checklist de 28 itens das rules de paralelismo (subset aplicável a CLI one-shot)
- As soluções respeitam as duas Restrições Invioláveis: ZERO mudança de schema e ZERO modelo local
### Como Solucionar
- Passo 1 — Dimensão configurável (`src/constants.rs`, `src/extract/llm_embedding.rs`, `src/similarity.rs`):

```rust
// ANTES: duas fontes de verdade hardcoded
// constants.rs:21        pub const EMBEDDING_DIM: usize = 384;
// llm_embedding.rs:26    pub const EMBEDDING_DIM: usize = 384;

// DEPOIS: fonte única com default 64 e override por env/flag
pub const DEFAULT_EMBEDDING_DIM: usize = 64;

pub fn embedding_dim() -> usize {
    std::env::var("SQLITE_GRAPHRAG_EMBEDDING_DIM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_EMBEDDING_DIM)
}

// Schema gerado dinamicamente em vez de string const duplicada
fn build_embedding_schema(dim: usize) -> String {
    format!(
        r#"{{"type":"object","properties":{{"embedding":{{"type":"array","items":{{"type":"number"}},"minItems":{dim},"maxItems":{dim}}}}},"required":["embedding"],"additionalProperties":false}}"#
    )
}
```

- Passo 2 — Cosseno dinâmico (`src/similarity.rs`):

```rust
// ANTES: dimensão fixa implícita
// DEPOIS: exigir apenas que os dois vetores tenham a mesma dimensão
pub fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None; // mixed-dim retorna None; caller emite warning estruturado
    }
    Some(dot(a, b) / (norm(a) * norm(b)))
}
```

- Passo 3 — Schema de batch e prompt numerado (`src/extract/llm_embedding.rs`):

```json
{
  "type": "object",
  "properties": {
    "items": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "i": {"type": "integer"},
          "v": {"type": "array", "items": {"type": "number"}, "minItems": 64, "maxItems": 64}
        },
        "required": ["i", "v"],
        "additionalProperties": false
      }
    }
  },
  "required": ["items"],
  "additionalProperties": false
}
```

```text
Generate 64-dimensional semantic embedding vectors for each numbered text below.
Return a JSON object with an "items" array.
Each item has "i" (the index) and "v" (the 64-float vector, values between -1 and 1).

1: farmácia gestão planejamento planejar dia
2: financeiro pix confirmar personal farma
3: danilo
4: eliandra
```

- Passo 4 — Paralelismo real com JoinSet + Semaphore (`src/embedder.rs`, reutilizando o padrão de `deep_research.rs:314-391`):

```rust
// Workload: I/O-bound + subprocess-bound (espera de rede 30-60s por chamada LLM).
// Permits: min(--llm-parallelism, available_parallelism, ram_livre*0.5/RSS, rate limit).
// RSS por subprocesso claude -p medido via /usr/bin/time: ~200-400 MB (node).
use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;

// ANTES (embedder.rs:88-100): lock segurado DURANTE o loop de I/O
// fn flush_group(...) { let mut guard = embedder.lock(); for text in group { guard.embed_passage(text)?; } }

// DEPOIS: lock apenas para clonar a config; I/O fora do lock; N workers
// limitados por semáforo; cancel propagado; resultados fluem por canal
// bounded para gravação INCREMENTAL no SQLite (single-writer WAL)
async fn embed_with_parallelism(
    batches: Vec<Vec<(usize, String)>>,
    parallelism: usize,
    dim: usize,
    client: LlmEmbedding, // Clone barato: flavour + paths + model
    persist_tx: mpsc::Sender<(usize, Vec<f32>)>, // canal BOUNDED para o estágio de gravação
) -> Result<(), AppError> {
    let semaphore = Arc::new(Semaphore::new(parallelism));
    let token = crate::cancel_token().clone(); // CancellationToken existente (lib.rs:56)
    let mut set = JoinSet::new();
    for batch in batches {
        let sem = semaphore.clone();
        let client = client.clone();
        let token = token.clone();
        let tx = persist_tx.clone();
        set.spawn(async move {
            // acquire_owned: permit RAII move-ável para a task; devolvido até em panic
            let _permit = sem
                .acquire_owned()
                .await
                .map_err(|e| AppError::Embedding(format!("semáforo fechado: {e}")))?;
            // cancel-safe: o branch cancelled() aborta sem corromper estado;
            // o subprocesso LLM usa kill_on_drop(true) e morre junto
            tokio::select! {
                res = client.generate_batch_embedding(&batch, dim) => {
                    for item in res? {
                        // send().await bloqueia se o gravador estiver atrás: backpressure
                        tx.send(item).await
                            .map_err(|_| AppError::Embedding("gravador encerrou".into()))?;
                    }
                    Ok(())
                }
                _ = token.cancelled() => Err(AppError::Embedding("cancelado por sinal".into())),
            }
        });
    }
    drop(persist_tx); // sinaliza fim do canal quando todas as tasks terminarem
    while let Some(result) = set.join_next().await {
        match result {
            Ok(task_result) => task_result?,
            Err(join_err) if join_err.is_panic() => {
                return Err(AppError::Embedding(format!("task panicou: {join_err}")));
            }
            Err(join_err) => {
                return Err(AppError::Embedding(format!("task cancelada: {join_err}")));
            }
        }
    }
    Ok(())
}
```

- Passo 5 — Separação chunks × entidades por perfil de tamanho:

```rust
// Entidades (curtas): lotes de 25; chunks (longos): lotes de 8; tudo em paralelo (G44: 25 e 8 são bases para dim 64, adaptadas em runtime)
let entity_batches = batch_texts(&entity_names, 25);
let chunk_batches = batch_texts(&chunks, 8);
let all_batches: Vec<_> = entity_batches.into_iter().chain(chunk_batches).collect();
```

- Passo 6 — Schema file único com RAII (`src/extract/llm_embedding.rs:288-307`):

```rust
// ANTES: write + remove_file POR chamada com path colidível por PID
// DEPOIS: NamedTempFile criado UMA vez por processo; Drop limpa automaticamente
let schema_file = tempfile::Builder::new()
    .prefix("sqlite-graphrag-embed-schema-")
    .suffix(".json")
    .tempfile()
    .map_err(|e| AppError::Embedding(format!("schema tempfile: {e}")))?;
std::fs::write(schema_file.path(), build_batch_schema(dim))?;
// reutilizar schema_file.path() em TODAS as chamadas; Drop remove no fim
```

- Passo 7 — Modelo e config dir sem hardcode (`src/extract/llm_embedding.rs:125,154,239-261`):

```rust
// Modelo claude com env override, simétrico ao codex
let model = std::env::var("SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL")
    .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

// CLAUDE_CONFIG_DIR vazio POR PADRÃO no embedding (issue #10787 torna as flags inúteis)
let empty_config = std::env::var("SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR")
    .map(std::path::PathBuf::from)
    .unwrap_or_else(|_| default_empty_config_dir()); // ~/.local/state/sqlite-graphrag/claude-empty-config, mode 0700
cmd.env("CLAUDE_CONFIG_DIR", &empty_config);
```

- Passo 8 — Handler de sinais sem panic (`src/signals.rs:13-29`):

```rust
// ANTES (signals.rs:24): eprintln! panica com BrokenPipe quando stderr é pipe fechado
// DEPOIS: escrita best-effort que ignora erro de I/O; segundo sinal sai sem I/O
use std::io::Write;
if prev == 0 {
    crate::SHUTDOWN.store(true, Ordering::Release);
    crate::cancel_token().cancel();
    let _ = writeln!(
        std::io::stderr(),
        "shutdown signal received; finishing current operation gracefully"
    ); // erro de pipe fechado é IGNORADO, jamais panica
} else {
    std::process::exit(130); // sem I/O algum no caminho de saída forçada
}
```

- Passo 9 — Documentação do padrão one-shot e correção do warm-up (docs):
- Documentar `enrich --operation re-embed --limit 5 --resume --json` como caminho canônico de re-embed em lote pequeno
- Cada invocação processa 5 itens em ~40-60s e ENCERRA, imune à janela do harness
- O loop externo é responsabilidade do chamador, conforme a filosofia one-shot
- Remover das docs v1.0.76 a recomendação do loop `edit --description "<mesmo>"` que não re-embeda
- Adicionar `edit --force-reembed` como caminho explícito para regenerar embedding sem alterar body
- Passo 10 — Retrocompatibilidade dos embeddings existentes:
- Embeddings de 384 dims permanecem válidos e pesquisáveis entre si até o re-embed
- Comparação entre dimensões diferentes retorna score 0 COM campo `warnings` no JSON de resposta
- `similarity.rs:16-19` JÁ é dinâmico: retorna 0.0 quando `a.len() != b.len()`, sem assumir 384
- `memories.rs:542-544` JÁ faz skip de linhas com dimensão divergente na busca KNN
- O ÚNICO ponto hardcoded restante é `memories.rs:495`, que valida o query embedding contra `EMBEDDING_DIM` — deve passar a usar a dimensão ativa
- A dimensão ativa fica registrada na chave `dim` EXISTENTE de `schema_meta` para diagnóstico via `health --json` — ZERO mudança de schema
### Critérios de Aceitação
- `remember` com 39 itens completa em menos de 90 segundos com `--llm-parallelism 4`
- ZERO lock segurado durante I/O de rede em qualquer caminho de embedding
- ZERO constante de dimensão duplicada no código (fonte única verificável por `rg 'EMBEDDING_DIM'`)
- ZERO modelo hardcoded sem env override em qualquer caminho de embedding
- `claude -p` de embedding roda com 0 tokens de configuração por padrão
- Teste que FALHA se vetor com dimensão divergente for normalizado silenciosamente
- Teste de SIGPIPE confirma exit 141 sem abort em processo com stderr fechado
- `enrich --operation re-embed --limit 5 --resume` documentado como padrão one-shot oficial
- Re-embed de 1000+ memórias completa em menos de 60 minutos com parallelism 4
- ZERO migração nova e ZERO `ALTER TABLE` (verificável: diretório `migrations/` inalterado e `schema_version` continua 13)
- ZERO dependência de modelo local reintroduzida (verificável: `rg 'fastembed|onnx|tokenizers|sqlite-vec' Cargo.toml` vazio)
- Teste de pico de concorrência: com permits 4, o máximo de subprocessos LLM simultâneos medido é 4
- `JoinError::is_panic` tratado em todo `join_next` do caminho de embedding
- Zero subprocessos zumbis após cancel (verificável por `procs` após o teste)
- Embeddings gravados incrementalmente: kill no meio do job preserva os itens já processados
- Métrica de `available_permits` e contadores completed/failed/cancelled presentes no NDJSON de progresso
- Classificação de workload e fórmula de permits documentadas em comentário no módulo
### Referências
- arXiv 2205.13147 — Matryoshka Representation Learning: 64 dims retém a maior parte da qualidade de retrieval (verificado via duckduckgo-search-cli)
- anthropics/claude-code#10787 — `--strict-mcp-config` e `--mcp-config '{}'` silenciosamente ignorados pelo Claude Code
- openai/codex#18113 — bug de propagação de `--sandbox` no `codex exec`
- docs.rs/tokio — APIs `JoinSet` e `Semaphore` validadas via context7 (/websites/rs_tokio, trustScore 9.7)
- ADR-0019 a ADR-0026 — decisões da arquitetura LLM-only v1.0.76
- rules rust aplicáveis: cli one-shot, tratamento de erros (BrokenPipe = 141), processos externos, proibição de hardcode, silent argument discard
- docs.rs/tokio/sync/struct.Semaphore.html — `Arc<Semaphore>` + `acquire_owned` como padrão canônico de bounded concurrency (fonte primária do deep-research via duckduckgo-search-cli, 4 sub-queries, agregação RRF)
- context7 `/websites/rs_tokio` (trustScore 9.7) — APIs `Semaphore`, `JoinSet` e `spawn` validadas
- rule graphrag `rules-rust-paralelismo-e-multiprocessamento` — 12 sub-memórias e checklist de 28 itens (bounded concurrency, cancel safety, panic handling, testes)
- `src/commands/deep_research.rs:314-391` — implementação interna de referência do padrão bounded com `acquire_owned` e `is_panic`
- ADR-0019, ADR-0021, ADR-0023 e ADR-0025 — base das Restrições Invioláveis LLM-only e one-shot
- `migrations/V001__init.sql:3-6` e `V013__drop_vec_use_blob_embeddings.sql:18-52` — evidência de que o schema atual já suporta dimensão configurável


## G43 — Adoção da Dimensionalidade Não Cobria os Comandos Principais (v1.0.79, descoberto na auditoria de docs de 2026-06-11)
### Status
- RESOLVIDO na própria v1.0.79, antes da publicação
- Descoberto por experimento controlado durante a auditoria da pasta docs
### Problema
- O sync do G42/S1 (`schema_meta.dim` → dim ativa do processo) rodava SOMENTE dentro de `ensure_db_ready`
- `remember`, `edit`, `recall` e `hybrid-search` abrem conexão via `open_rw`/`open_ro` e NUNCA chamam `ensure_db_ready`
- Esses comandos usavam silenciosamente o default compilado (64) contra bancos 384 pré-v1.0.79
- Embeddings de dimensões misturadas pontuam cosseno 0.0 entre si (`similarity.rs` retorna 0.0 em mismatch)
- O recall vetorial ficava CEGO ao corpus antigo sem nenhum erro visível
- `init.rs:100` gravava `INSERT OR REPLACE ... ('dim', '384')` hardcoded — banco NOVO nascia marcado 384 com embeddings reais de 64
- `rename_entity.rs:107` gravava coluna `dim` 384 fixa e o nome de modelo removido `multilingual-e5-small`
### Evidência em Produção
- Banco real `graphrag.sqlite` desta sessão: `schema_meta.dim = 384`, `memory_embeddings` com 1159 vetores de 384 + 1 de 64, `entity_embeddings` com 57 de 384 + 21 de 64
- Experimento controlado: `init` com mock LLM falhava exit 11; `remember` em banco 384 pedia 64 ao LLM
### Causa Raiz
- A promessa S1 "bancos 384 continuam funcionando" dependia de um caminho de código (`ensure_db_ready`) que os comandos de embedding não percorrem
- Os testes de integração que detectariam o problema ficam atrás do gate `--features slow-tests`, que NÃO roda no CI
- Os mocks `tests/mock-llm/{claude,codex}` devolviam 384 dims no formato single — a suíte slow inteira falhava desde o S1+S2 sem ninguém ver
### Correção Aplicada
- `connection.rs`: nova `adopt_embedding_dim()` (leitura best-effort de `schema_meta.dim` + `set_active_embedding_dim`) chamada em `open_rw` E `open_ro`; env override continua vencendo; banco virgem é no-op
- `init.rs`: `INSERT OR IGNORE` com `embedding_dim()` no lugar do literal `'384'` — preserva a dim registrada em re-init
- `rename_entity.rs`: INSERT duplicado substituído pelo writer canônico `upsert_entity_vec` (DRY; tamanho real do vetor; versão da CLI como model)
- Mocks de teste reescritos: 64 dims e bilíngues nos dois formatos (single `{"embedding":[...]}` e batch `{items:[{i,v}]}` via detecção de "EXACTLY N items" em argv+stdin)
- 2 testes obsoletos de daemon viraram guardas de regressão da remoção; 2 testes de `--autostart-daemon` atualizados; `smoke_26` canonicaliza paths (symlink /var → /private/var no macOS)
- `.config/nextest.toml`: filtro removido do binário deletado `daemon_integration`
- 4 testes de regressão novos em `connection.rs` (adoção rw/ro, precedência env, banco virgem)
### Verificação
- `cargo test --lib`: 741 passed, 0 failed
- Suíte completa `--features slow-tests`: 1210 testes, 0 failed (estava 15 failed antes do fix)
- Experimento end-to-end: banco novo opera em 64 (init+remember+recall exit 0); banco 384 pré-existente opera em 384 (remember grava embedding 384)
### Relações Causa × Efeito
- O sync restrito a `ensure_db_ready` CAUSOU dim ativa errada nos comandos principais, QUE CAUSOU embeddings misturados em bancos migrados, QUE CAUSOU recall vetorial cego ao corpus antigo
- O gate `slow-tests` fora do CI CAUSOU mocks desatualizados invisíveis, QUE CAUSOU a suíte de integração quebrada sem detecção desde o G42
- A adoção em `open_rw`/`open_ro` CAUSA dim correta em TODO comando, QUE CAUSA a promessa S1 cumprida de fato
### Ação Operacional Pendente em Bancos Contaminados
- Re-embedar itens gravados na dim errada ANTES do fix: `edit --force-reembed` por memória afetada
- Entidades contaminadas: sem comando dedicado de re-embed de entidade; `rename-entity` ida-e-volta re-embeda na dim ativa
- Identificar contaminados: `SELECT dim, COUNT(*) FROM memory_embeddings GROUP BY dim` via comandos da CLI (`health`/`vec stats` não agregam por dim hoje — oportunidade futura)


## G44 — Tamanho de Lote de Embedding Não Escala Com a Dimensionalidade Ativa (v1.0.79, descoberto em produção em 2026-06-11)
### Status
- RESOLVIDO em 2026-06-11 — Caminho B aprovado pelo usuário e implementado: lote adaptativo por dim
### Correção Aplicada
- `src/embedder.rs::adaptive_batch_for_dim(base, dim)`: `clamp(base × EMBED_BATCH_CALIBRATION_DIM / dim, 1, base)` com calibração em dim 64
- Wrappers públicos `chunk_embed_batch_size()` e `entity_embed_batch_size()` leem a dim ativa (`constants::embedding_dim()`, garantida pelo G43) e emitem `tracing::debug!` com dim, base e lote calculado
- Call sites atualizados: `remember.rs` (chunks e entidades), `ingest.rs` (chunks e entidades), `embedder.rs::embed_passages_controlled`
- Resultado: banco 64 → lotes 8/25 (idêntico ao G42); banco 384 → lotes 1/4 (~orçamento de floats constante por chamada)
- `base.max(1)` torna a função total — `clamp` entraria em panic com limite superior abaixo do inferior
- 6 testes de regressão: fórmula pura em dims 64/128/256/384/4096 e bordas degeneradas (dim 0, base 0) + wrapper end-to-end com env 384 (`serial_test::serial(env)`)
- O workaround `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS=900` deixa de ser necessário em bancos 384; a env permanece como válvula para corpos extremos
- Retry-split em coverage mismatch NÃO foi implementado — o lote adaptativo remove a causa raiz (resposta longa demais); retry-split trataria só o sintoma (YAGNI)
### Problema
- O batching do G42/S2 usa lotes FIXOS: 8 chunks ou 25 nomes de entidade por chamada LLM
- O orçamento foi calibrado para a dim default 64: 8 × 64 = 512 floats de saída por chamada
- Em bancos LEGADOS registrados com 384 dims, o mesmo lote de 8 pede 8 × 384 = 3072 floats por chamada
- Observado em produção no graphrag.sqlite real (banco 384): claude devolveu 3 itens de 8 ("LLM batch returned 3 items, expected 8, G42/S2 coverage check"); codex estourou o timeout de 300s
- Com SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS=900 o mesmo remember completou em 381s (6,4 min) — confirma lentidão, não travamento
- Até a chamada SINGLE de query em 384 dims é pesada: recall medido em 72s no banco real
- O C5 rejeita corretamente a resposta incompleta, mas o comando inteiro falha com exit 11 sem retry
### Causa Raiz
- LLMs degradam fidelidade e latência proporcionalmente ao tamanho da SAÍDA pedida
- O lote fixo ignora que a saída por item é proporcional à dim ativa
### Workaround Imediato (histórico — dispensado pela correção)
- `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS=900` (ou maior) para corpos longos em bancos 384
- Retentar a invocação: a falha de cobertura do claude é não determinística
### Correção Proposta (histórico — implementada como descrito em Correção Aplicada)
- Tamanho de lote adaptativo: `batch = clamp(TOKEN_BUDGET_FLOATS / dim, 1, 8)` com `TOKEN_BUDGET_FLOATS = 512` — bancos 64 mantêm 8; bancos 384 caem para 1-2 itens por chamada
- Alternativa complementar: em coverage mismatch (C5), dividir o lote ao meio e retentar (retry-split) em vez de falhar o comando inteiro
- Critério de aceitação: `remember --force-merge` de corpo com 20+ chunks em banco 384 conclui sem elevar o timeout
### Relações Causa × Efeito
- O lote fixo calibrado para 64 CAUSA saída de ~3072 floats em bancos 384, QUE CAUSA respostas incompletas (claude) e timeouts (codex), QUE CAUSA exit 11 em remember/edit de corpos longos
- O batch adaptativo pela dim CAUSARIA saída constante por chamada, QUE CAUSARIA confiabilidade independente da dimensionalidade do banco
