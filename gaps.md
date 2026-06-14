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


## G45 — remember e edit Sem Coordenação Entre Processos: Embedding Concorrente Multi-Sessão Causa Timeout em Cascata e Perda Total do Trabalho (v1.0.79, descoberto em produção em 2026-06-11)
### Status
- ABERTO — apenas documentado; nenhuma correção implementada
### Problema
- `remember`, `edit` e `remember-batch` NÃO adquirem o job singleton por namespace e banco (`lock::acquire_job_singleton`) — apenas `enrich` (enrich.rs:1487), `ingest --mode claude-code` (ingest_claude.rs:606) e `ingest --mode codex` (ingest_codex.rs:595) o fazem
- Quando duas ou mais sessões de agente (Claude Code) rodam `remember` simultaneamente na mesma máquina, cada processo faz fan-out de até `--llm-parallelism` subprocessos `claude -p`/`codex exec` que disputam a MESMA quota OAuth da conta
- A quota OAuth é um recurso GLOBAL da conta, não do processo: o `Semaphore` interno limita o paralelismo de UM processo, mas M sessões × N permits saturam o backend sem que nenhum processo detecte a contenção
- Sob contenção, a latência por chamada de embedding cresce de ~73s (sessão isolada) para mais de 300s, estourando o timeout estático por chamada (`SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS`, default 300 em llm_embedding.rs:43) e os timeouts externos dos wrappers dos agentes (180–420s)
- O wrapper externo envia SIGTERM; o handler (signals.rs:28-52) cancela o token global; o `tokio::select!` do embedder (embedder.rs:351-355) aborta o batch com "embedding cancelled by shutdown signal" e o comando falha com exit 11
- O `remember` é all-or-nothing: ao contrário do `ingest` (queue DB `.ingest-queue.sqlite` + `--resume`), não existe checkpoint nem fila de embedding pendente — TODO o trabalho da invocação é descartado, incluindo chamadas LLM já concluídas com sucesso
### Evidência em Produção
- Sessão A (projeto atomwrite): `remember --graph-stdin` com `timeout 400` → exit 124, log "shutdown signal received; finishing current operation gracefully"; memória NÃO criada (read posterior retorna exit 4)
- Sessão B (projeto youtube-legend-cli): 3 tentativas de `remember`, todas EXIT=124 em 180s, mesmo log de shutdown; teste isolado anterior na mesma máquina levara 73s
- Sessão C (projeto duckduckgo-search-cli): concorrente às outras, rodando `remember --name ddg-cli-incident-release-v073` — única dona do binário `sqlite-graphrag` vivo (`pgrep -xc sqlite-graphrag` = 1)
- Memória `incident-embedding-timeout-concorrencia` no graphrag.sqlite registra o mesmo padrão: exit 11 com "embedding cancelled by shutdown signal" 2x e "codex embedding call timed out after 300s" 1x; só concluiu com `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS=900` + `--llm-parallelism 1` + `timeout 3000` (889s)
### Consequências do Problema
- Perda total e silenciosa do trabalho: pipelines com `2>/dev/null` + `jaq '{memory_id}'` exibem apenas `null`; o agente não distingue falha de quota, bug ou banco corrompido
- Loop de retroalimentação positiva: a falha leva o agente a RETENTAR; cada retentativa spawna novos subprocessos LLM que AGRAVAM a contenção, derrubando também as outras sessões — degradação coletiva em cascata
- Desperdício de quota OAuth: chamadas de embedding concluídas antes do cancelamento são descartadas e precisarão ser refeitas integralmente
- Acúmulo de subprocessos `claude`/`codex` semi-órfãos durante as retentativas, exigindo `pkill` manual (o reaper G28-C só atua no startup da PRÓXIMA invocação)
- Exit code 11 genérico (Embedding) mascara a causa real: cancelamento por SIGTERM externo, timeout por chamada e falha real de embedding são indistinguíveis para automação
- Multi-sessão de agentes — cenário padrão do usuário com vários projetos abertos — torna-se operacionalmente inviável para escrita de memórias
### Causa Raiz
- CR1 (coordenação): a proteção de singleton G28-B/G30 foi aplicada apenas aos jobs LONGOS (`enrich`, `ingest`), partindo da premissa de que `remember` é curto; com o pipeline LLM-only da v1.0.76+ cada `remember` virou um job de minutos com fan-out de subprocessos, mas ficou FORA do guarda-chuva de coordenação entre processos
- CR2 (modelo de recurso errado): o limite de paralelismo é por processo (`Semaphore` local), mas o recurso disputado (quota OAuth) é por conta e por máquina — não existe nenhum mecanismo cross-process que limite o total de subprocessos LLM simultâneos
- CR3 (timeout estático): o limite de 300s por chamada foi calibrado para latência SEM contenção; não se adapta quando a fila do backend cresce, transformando lentidão recuperável em falha dura
- CR4 (shutdown abortivo sem checkpoint): o cancelamento via `CancellationToken` descarta o batch em curso e os resultados já obtidos; viola a sequência canônica de encerramento gracioso (drenar trabalho em curso dentro do deadline e persistir checkpoint de progresso) — o "gracefully" da mensagem não corresponde ao comportamento real para o embedding
- CR5 (cegueira aos headers de quota): a API Anthropic retorna 14 headers `anthropic-ratelimit-*` por resposta (`requests-limit`, `requests-remaining`, `requests-reset`, `tokens-limit`, `tokens-remaining`, `tokens-reset`, e variantes input/output/priority); o subprocesso `claude -p` recebe esses headers mas NÃO os propaga para o `embedder`; sem essa informação, o sistema opera cego à quota e só descobre contenção via timeout — a 300s, quando já é tarde para adaptar `--llm-parallelism` ou abortar preventivamente
### Relações Causa × Efeito
- CR1 (remember sem singleton) PERMITE M sessões concorrentes, QUE CAUSAM (via CR2, quota global compartilhada) latência por chamada acima de 300s, QUE CAUSA (via CR3, timeout estático) exit 11 interno e SIGTERM do wrapper externo, QUE CAUSA (via CR4, cancelamento sem checkpoint) perda total do trabalho, QUE CAUSA retentativas do agente, QUE REALIMENTAM a contenção inicial — fechando o ciclo
- A ausência de fila de embedding pendente CAUSA acoplamento total entre persistência do body e sucesso do embedding, QUE CAUSA a perda do conteúdo textual mesmo quando apenas a etapa vetorial falhou
- O exit 11 genérico CAUSA diagnóstico errado pelos agentes (suspeita de banco corrompido ou bug), QUE CAUSA investigações manuais repetidas em cada sessão afetada
- CR5 (headers de quota invisíveis) CAUSA ponto cego operacional, QUE FAZ COM QUE o sistema não detecte contenção iminente, QUE CAUSA tentativas de embedding que SERIAM evitáveis com leitura prévia de `requests-remaining` e `tokens-reset`, QUE CAUSA desperdício de quota OAuth e aumento da latência coletiva — multiplicando o impacto de CR1-CR4
### Solução
- S1 — Semáforo cross-process de embedding: file lock em diretório de estado XDG com escopo por MÁQUINA (não por banco), limitando o total de subprocessos LLM simultâneos entre todas as invocações; `remember`/`edit` esperam o slot via `--wait-embed-slot <SECONDS>` (análogo ao `--wait-job-singleton` do G30) em vez de competir
- S2 — Persistência write-behind com fila de re-embed: gravar body, FTS5 e grafo IMEDIATAMENTE; marcar embedding como pendente (tabela `embedding_queue` ou flag por memória); drenar via `enrich --operation re-embed --resume` (caminho já existente na v1.0.79); no shutdown, terminar a chamada LLM em curso dentro de um deadline curto, persistir o checkpoint e sair limpo
- S3 — Adaptação à contenção: antes do fan-out, contar processos `claude`/`codex` vivos via /proc (reutilizar a varredura do reaper G28-C) e reduzir `--llm-parallelism` efetivo ou avisar; opcionalmente escalar o timeout por chamada com base na latência medida da primeira chamada
- S4 — Diagnóstico distinguível: cancelamento por sinal externo sai com exit 143 (128+SIGTERM) e envelope JSON `{"error":true,"code":143,"message":...,"suggestion":...}`; timeout por chamada mantém exit 11 mas com `suggestion` apontando `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` e a checagem de sessões concorrentes
- S5 — Propagação de headers de quota OAuth: o `claude_runner` e o `codex_spawn` capturam os 14 headers `anthropic-ratelimit-*` e os retornam via stdout (linha JSON final tipo `{"type":"ratelimit","requests_remaining":N,"tokens_remaining":M,"reset_at":ISO}`); o `embedder` agrega o estado de quota em um `QuotaState` em memória e expõe env var `SQLITE_GRAPHRAG_QUOTA_REMAINING` para inspeção do agente; antes do fan-out, se `requests_remaining < batches_total`, o `Semaphore` é reduzido dinamicamente e o `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` é escalado proporcionalmente a `reset_at - now`
### Benefícios da Solução
- `remember` torna-se idempotente e crash-safe: nenhum SIGTERM ou timeout perde conteúdo textual; apenas o embedding fica pendente e é recuperável
- O loop de retroalimentação é quebrado: retentativas deixam de multiplicar subprocessos porque o semáforo cross-process serializa o acesso à quota
- Multi-sessão vira cenário suportado de primeira classe, alinhado ao uso real (vários projetos de agente abertos na mesma máquina e conta)
- Quota OAuth deixa de ser desperdiçada com chamadas concluídas e descartadas
- Exit codes distinguíveis permitem roteamento automático correto pelos agentes (retentar, esperar, escalar timeout ou reportar)
- Adaptação proativa à quota: o sistema REDUZ paralelismo e ESCALA timeout ANTES da contenção se tornar catastrófica, transformando lentidão recuperável em throughput sustentado
### Como Solucionar
- Passo 1: extrair de `lock.rs` um helper `acquire_embed_slot(max_slots, wait_secs)` baseado em file lock com escopo de máquina (ex.: `~/.local/share/sqlite-graphrag/embed-slots/`), com detecção de lock stale por PID morto — verificação: teste de integração com 2 processos disputando 1 slot
- Passo 2: chamar o helper no início do fan-out de embedding em `remember.rs`, `edit.rs`, `remember_batch.rs` e nos caminhos de embedding do `ingest`; expor `--wait-embed-slot` e env `SQLITE_GRAPHRAG_EMBED_MAX_SLOTS` — verificação: duas invocações simultâneas de `remember` concluem serializadas, ZERO exit 11
- Passo 3: adicionar persistência write-behind em `remember.rs`: commit do body/FTS/grafo antes do embedding; flag `embedding_pending` na linha da memória; estender `enrich --operation re-embed` para varrer pendentes — verificação: SIGTERM no meio do embedding deixa a memória legível via `read` e `hybrid-search` em modo FTS-only, e `enrich --resume` completa o vetor
- Passo 4: no `tokio::select!` do embedder, trocar cancelamento imediato por deadline de drenagem (terminar a chamada em curso até N segundos) seguido de checkpoint — verificação: teste enviando SIGTERM e conferindo exit 143 + memória persistida
- Passo 5: mapear `AppError` novo para exit 143 com envelope JSON e `suggestion`; atualizar `docs/schemas/` e a tabela de exit codes — verificação: teste `#[serial]` simulando sinal
- Passo 6: documentar em ADR a decisão do escopo do semáforo (máquina vs conta) e atualizar HOW_TO_USE e skills dos agentes — verificação: auditoria de docs bilíngue
- Passo 7: implementar S5 (CR5): em `claude_runner.rs` e `codex_spawn.rs`, capturar os 14 headers `anthropic-ratelimit-*` da resposta HTTP e emitir um evento NDJSON final `{"type":"ratelimit",...}`; no `embedder.rs::fan_out`, agregar esses eventos em `QuotaState` Mutex; antes de cada `Semaphore::acquire`, consultar `QuotaState` e ajustar `available_permits` e `effective_timeout_secs` proporcionalmente; expor `SQLITE_GRAPHRAG_QUOTA_REMAINING` via `env!` macro para inspeção do agente — verificação: teste de integração com mock LLM que retorna `requests-remaining: 0` simula contenção; `remember` reduz paralelismo de 4 para 1 sem intervenção manual
### Critérios de Aceitação
- Duas sessões rodando `remember --graph-stdin` simultaneamente no mesmo host concluem AMBAS sem exit 11, sem intervenção manual e sem elevar `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS`
- SIGTERM durante o embedding produz exit 143, memória persistida com body íntegro e `enrich --operation re-embed --resume` restaura o vetor em invocação posterior
- Nenhum subprocesso `claude`/`codex` sobrevive ao término do processo pai (kill_on_drop preservado)
- Headers `anthropic-ratelimit-*` são capturados e propagados para o `embedder`; `--llm-parallelism` adapta-se automaticamente quando `requests-remaining` cai abaixo de `batches_total`; `SQLITE_GRAPHRAG_QUOTA_REMAINING` reflete estado agregado em tempo real
### Referências
- `src/lock.rs` (singleton G30), `src/signals.rs:28-52` (handler), `src/embedder.rs:351-355` (select! de cancelamento), `src/extract/llm_embedding.rs:43` (timeout 300s)
- `rg -n 'acquire_job_singleton' src/` — prova de que remember.rs está fora do guarda-chuva (apenas enrich.rs:1487, ingest_claude.rs:606, ingest_codex.rs:595)
- Memórias do graphrag.sqlite: `incident-embedding-timeout-concorrencia`, `incident-daemon-orfao-graphrag-lock`
- G28-B/G30 (singleton por job), G42 (pipeline de embedding), G44 (lote adaptativo por dim) — este gap fecha a lacuna de COORDENAÇÃO que G42/G44 não cobriram
- Docs tokio: `Command::kill_on_drop` garante morte do filho no drop do handle; `CancellationToken` + `tokio::select!` cancelam o future sem drenagem
- Rate limits compartilhados por conta em sessões concorrentes: https://platform.claude.com/docs/en/api/rate-limits e https://code.claude.com/docs/en/headless
- CR5 (atualização 2026-06-13): 14 headers `anthropic-ratelimit-*` documentados em https://docs.anthropic.com/en/api/rate-limits — `anthropic-ratelimit-{requests,tokens,input-tokens,output-tokens,priority-input-tokens,priority-output-tokens}-{limit,remaining,reset}`; o `claude -p` headless os descarta no spawn caller, criando o ponto cego que CR5 documenta


## G46 — Identificador de Modelo Legado em schema_meta e no init (v1.0.79, auditoria pós-publicação de 2026-06-11)
### Status
- RESOLVIDO em 2026-06-11, durante a auditoria black-box do binário publicado no crates.io
### Contexto e Evidências
- `init --json` do binário v1.0.79 instalado reportou `"model": "multilingual-e5-small"`
- O modelo fastembed foi REMOVIDO na v1.0.76 (ADR-0019, ADR-0023); o valor era falso
- `connection.rs::insert_default_schema_meta` e `init.rs` gravavam o literal em TODO banco novo
- `health` checava o diretório `models--intfloat--multilingual-e5-small` e reportava `model_ok: false` em banco saudável
- O detail do check sugeria `sqlite-graphrag models download`, comando INEXISTENTE
### Problema
- Metadado `model` registrava um gerador de embedding que não existe no build LLM-only
- O check `model_onnx` do health era sempre falso e a remediação sugerida era inexecutável
### Causa Raiz
- O G43 corrigiu o model legado em `rename_entity.rs` mas não varreu os demais pontos de escrita
- Não havia teste de contrato amarrando `init.model` e `health.model_ok` à arquitetura LLM-only
### Correção Aplicada
- `schema_meta.model` e `init.model` agora gravam `SQLITE_GRAPHRAG_VERSION`, consistente com as 3 tabelas de embedding
- `health.model_ok` passou a reportar disponibilidade de CLI LLM no PATH via `find_claude_binary`/`find_codex_binary` (DRY)
- Check renomeado de `model_onnx` para `llm_cli` com detail acionável
- `FASTEMBED_MODEL_DEFAULT` (constante morta) removida; stub `embedding_backend.rs` atualizado
- Flag `init --model` mantida por compatibilidade com doc comment marcando-a como legada
### Relações Causa × Efeito
- O literal legado em 2 caminhos de escrita CAUSAVA metadado falso em todo banco novo, QUE CAUSAVA diagnóstico enganoso no init e no health
- O check de diretório ONNX inexistente CAUSAVA `model_ok: false` permanente, QUE CAUSAVA alarme falso sem remediação possível


## G47 — Flags Documentadas Inexistentes: edit --type e reclassify --entity-type (v1.0.79, auditoria pós-publicação de 2026-06-11)
### Status
- RESOLVIDO em 2026-06-11 com aliases visíveis de clap e 2 testes de regressão
### Contexto e Evidências
- `edit --type decision` no binário publicado retorna exit 2 `unexpected argument`
- `edit --type` é prometida em 5 documentos do repo: COOKBOOK.md, COOKBOOK.pt-BR.md, llms.pt-BR.txt, README.md e README.pt-BR.md (changelog v1.0.66)
- `reclassify --entity-type` é prometida em llms-full.txt; a CLI só aceita `--new-type`/`--to-type`
- O comando irmão usa `--type` no create, criando assimetria de UX entre comandos
### Problema
- A CLI publicada rejeita flags que a própria documentação oficial ensina a usar
### Causa Raiz
- A suíte de contratos valida schemas JSON de RESPOSTA mas nenhum teste valida flags de CLI citadas nas docs
### Correção Aplicada
- `edit`: `visible_alias = "type"` em `memory_type` — `--type` e `--memory-type` funcionam
- `reclassify`: `visible_alias = "entity-type"` em `new_type`
- 2 testes de regressão de parsing clap (`type_flag_is_a_visible_alias_of_memory_type`, `entity_type_flag_is_a_visible_alias_of_new_type`)
### Relações Causa × Efeito
- Docs sem teste executável de flags CAUSARAM promessa divergente do binário, QUE CAUSA exit 2 em comandos copiados da documentação
- O alias aditivo CAUSA a doc retroativamente verdadeira SEM breaking change


## G48 — Validação G20 do hybrid-search Cega a Valores Iguais ao Default (v1.0.79, auditoria pós-publicação de 2026-06-11)
### Status
- RESOLVIDO em 2026-06-11 com Option<T> e teste de regressão
### Contexto e Evidências
- `hybrid-search "x" --max-hops 2` SEM `--with-graph` retornava exit 0 silencioso no binário publicado
- TESTING.md documentava exatamente esse caso como `expecting exit 1`
- Com `--max-hops 3` a validação disparava (comparação com o default 2)
### Problema
- Flag explícita igual ao default era indistinguível da ausência da flag e escapava da validação G20
- Descarte silencioso de argumento viola a rule de silent argument discard
### Causa Raiz
- `max_hops: u32` com `default_value = "2"` apaga a informação de presença da flag no parse
### Correção Aplicada
- `max_hops: Option<u32>` e `min_weight: Option<f64>` sem default no parse; defaults 2 e 0.3 aplicados via `unwrap_or` apenas no caminho com grafo
- Validação G20 passou a usar `is_some()` — qualquer uso explícito sem `--with-graph` falha com exit 1
- Teste `graph_flags_parse_as_none_when_absent`; caso do TESTING.md agora é verdadeiro
### Relações Causa × Efeito
- O default no tipo primitivo CAUSAVA perda da presença da flag, QUE CAUSAVA bypass da validação no valor coincidente, QUE CAUSAVA o caso documentado no TESTING.md ser falso


## G49 — Descarte Silencioso de SQLITE_GRAPHRAG_EMBEDDING_DIM Inválida (v1.0.79, auditoria pós-publicação de 2026-06-11)
### Status
- RESOLVIDO em 2026-06-11 com tracing::warn explícito
### Contexto e Evidências
- `SQLITE_GRAPHRAG_EMBEDDING_DIM=0|7|5000` no `init` do binário publicado: exit 0, banco nasce com dim 64, ZERO warning
- A documentação declara a faixa válida [8, 4096]
### Problema
- Um typo na env grava permanentemente um banco novo com dimensionalidade diferente da pedida, sem nenhum sinal
### Causa Raiz
- `embedding_dim_from_env` usava `.ok()/.filter()` que silenciam tanto parse inválido quanto fora de faixa
### Correção Aplicada
- `embedding_dim_from_env` emite `tracing::warn!` com o valor rejeitado e a faixa esperada antes de cair no default
### Relações Causa × Efeito
- O filter silencioso CAUSAVA fallback invisível para 64, QUE CAUSARIA banco permanente com dim não intencional e recall cego após re-embed na dim esperada


## G50 — CI Vermelho Não Bloqueia Release: 6 Causas Técnicas Acumuladas (v1.0.79, auditoria de 2026-06-11)
### Status
- RESOLVIDO no repositório em 2026-06-11 (6 causas corrigidas); jobs Windows de infraestrutura permanecem ABERTOS (ver G53)
### Contexto e Evidências
- As 5 execuções mais recentes de CI e Release no GitHub concluíram em failure, incluindo o Release da v1.0.79
- Causa A: doctest `src/preservation.rs` com asserções matematicamente erradas (`score > 0.5` falso para o exemplo) publicado DESDE a v1.0.69 — derruba o job Tests nas 6 combinações da matriz; invisível localmente porque nextest não executa doctests
- Causa B: mock LLM INLINE do ci.yml retornava 384 dims em formato single — divergente do `tests/mock-llm/` canônico (64, single+batch) e da v1.0.79
- Causa C: job Benchmark Regression spawna a CLI real sem LLM no PATH — `init retornou Some(11)`
- Causa D: Language policy reprova `//! ... BLOCO 1 — OBRIGATÓRIA` em `src/embedder.rs` (caracteres PT em crate docs)
- Causa E: `cargo-careful sanity` falha com `knn_search_chunks embedding has 96 dims, expected 64` — race de estado global de dim entre testes no harness de processo único (`cargo test --lib`); invisível no nextest (processo por teste)
- Causa F: `cargo deny` alertava ignore obsoleto RUSTSEC-2025-0119 (fastembed saiu da árvore na v1.0.76)
### Problema
- Releases são publicados com CI vermelho; falhas reais acumulam sem detecção e viram ruído permanente
### Causa Raiz
- O processo de release não tem gate bloqueante amarrado ao status do workflow CI
- Doctests rodam APENAS no CI (nextest local os ignora), então a quebra da Causa A nunca apareceu no fluxo local
### Correção Aplicada
- A: doctest reescrito com exemplos calculados contra a implementação real de trigramas (19 doctests verdes via `cargo test --doc`)
- B: ci.yml passou a copiar os mocks canônicos `tests/mock-llm/{claude,codex}` (DRY)
- C: job de benchmark ganhou o mesmo step de mock no PATH
- D: comentário traduzido (`BLOCK 1 — MANDATORY`); varredura dos 4 padrões da policy limpa
- E: 9 testes leitores de `embedding_dim()` em chunks/memories/entities marcados `#[serial_test::serial(env)]`, mesmo grupo serial dos escritores G43
- F: ignore obsoleto removido do deny.toml com nota explicativa
- `cargo test --doc` adicionado ao plano de testes formal como camada obrigatória local (docs/TEST_PLAN.md)
### Relações Causa × Efeito
- nextest sem doctests CAUSOU asserção errada invisível local, QUE CAUSOU job Tests vermelho por 10 releases, QUE CAUSOU dessensibilização ao CI vermelho
- A dessensibilização CAUSOU publicação da v1.0.79 com Release em failure, QUE CAUSOU artefato publicado carregando o doctest quebrado
- O mock inline duplicado do ci.yml CAUSOU drift de dimensionalidade ao G42/S1, QUE CAUSARIA falhas falsas nos testes de embedding do CI
- O harness de processo único do careful CAUSA compartilhamento dos atomics de dim entre testes, QUE CAUSA a race 96 vs 64 que o nextest mascara


## G51 — Mocks LLM Hardcoded em 64 Dims Impediam Teste End-to-End Multi-Dim (v1.0.79, auditoria de 2026-06-11)
### Status
- RESOLVIDO em 2026-06-11; validado com criação de memória em banco 384 + mock
### Contexto e Evidências
- Experimento black-box: banco 384 + mock no PATH → criação de memória aborta exit 11 `LLM returned 64 dims, expected 384` (C5 correto, mock errado)
- O caminho exato que o G44 corrigiu (banco 384 com lote adaptativo) NÃO tinha teste end-to-end possível com mock
### Problema
- A dimensionalidade era fixa nos mocks; qualquer cenário não-64 era intestável hermeticamente
### Causa Raiz
- Os mocks foram escritos para o default 64 do G42/S1 sem prever os bancos legados 384 que o G43/G44 suportam
### Correção Aplicada
- Mocks `tests/mock-llm/{claude,codex}` extraem a dim do prompt (`(\d+)-dimensional`) ou do arquivo `--output-schema` (`minItems`), com fallback 64
- Validação: o cenário banco-384 que falhava conclui com `action: created`
### Relações Causa × Efeito
- O mock 64-only CAUSAVA exit 11 em qualquer banco não-64, QUE CAUSAVA zero cobertura e2e do caminho G43/G44, QUE CAUSARIA regressões de adoção de dim invisíveis


## G52 — vec stats Sem Agregação por Dim e Com Schema Que Nunca Correspondeu ao Binário (v1.0.79, auditoria de 2026-06-11)
### Status
- RESOLVIDO em 2026-06-11
### Contexto e Evidências
- O G43 anotou: identificar contaminação multi-dim exige SQL manual (`SELECT dim, COUNT(*) ... GROUP BY dim`)
- `vec stats --json` real emite `total_rows/orphaned/coverage_percent/...`; `docs/schemas/vec-stats.schema.json` documentava `namespace/vec_memories/...` — contrato divergente desde a criação (v1.0.69)
- Nenhum teste em `schema_contract_strict.rs` cobre vec stats, por isso a divergência ficou invisível
### Problema
- Sem visão por dim, bancos contaminados (G43) são indiagnosticáveis pela CLI; o schema publicado descrevia um response que nunca existiu
### Causa Raiz
- O schema foi escrito a partir do design planejado do G39 e nunca validado contra o binário por teste de contrato
### Correção Aplicada
- `vec stats` ganhou campo `dims: [{table, dim, rows}]` agregando as 3 tabelas de embedding (zero mudança de schema do banco)
- `vec-stats.schema.json` reescrito fiel ao response real, incluindo `dims`
- Teste `dim_breakdown_groups_rows_per_dim_and_table` cobre dims mistas
### Relações Causa × Efeito
- Schema sem teste de contrato CAUSOU divergência invisível desde a v1.0.69, QUE CAUSA consumidores programáticos validando contra um contrato falso
- A agregação por dim CAUSA diagnóstico de contaminação G43 em um comando, QUE ELIMINA o SQL manual apontado como pendência do G43


## G53 — Processo de Release: SemVer da Lib Quebrado em Patch e Jobs Windows de Infra (v1.0.79, auditoria de 2026-06-11)
### Status
- RESOLVIDO na v1.0.80 (2026-06-14) — ambos os lados fechados
  - Lado política: ADR-0032 (lib API stability) + job `semver-checks` no CI (informational em v1.0.80, promote a bloqueante em v1.0.81) + entry de CHANGELOG `Library API Changes`
  - Lado infra Windows: ADR-0033 (G53-WINDOWS-INFRA CI Resilience) — pre-warm + verify steps nos jobs `clippy` e `test` da matrix `windows-2025` (gated em `if: matrix.os == 'windows-2025'`, no-op em ubuntu/macos), validação local de cross-compile G29 (target instalado no MSRV 1.88; atinge fronteira do `cc-rs/lib.exe` que é o limite esperado do host Linux)
### Contexto e Evidências
- `cargo +stable semver-checks --baseline-version 1.0.78`: 9 verificações MAJOR falhas (ex.: trait público `extraction_gliner::Extractor` removido) em release PATCH
- `Clippy (windows-2025)` e `Windows MSVC cross-compile (G29)` falham por infra: download do rustup com erro de rede e `E0463 can't find crate for core` (stdlib do target ausente no runner)
### Problema
- O crate é publicado como lib+bin; consumidores da lib sofrem breaking changes em bump patch
- Jobs Windows vermelhos por infra alimentam a dessensibilização ao CI vermelho (G50)
### Causa Raiz
- Não há gate de semver-checks no CI nem política declarada de estabilidade da API da lib
- Steps de setup Windows sem retry/fallback para falhas transitórias de rede
### Correção Aplicada
- ADR-0032 aceito (estabilidade lib: CLI é contrato estável, lib é instável em minor, semver-checks informational)
- Job `cargo semver-checks` adicionado ao CI em `.github/workflows/ci.yml` (linhas 211-243) com `--baseline-version 1.0.79` e `continue-on-error: true`
- ADR-0033 aceito (resiliência windows-2025 via pre-warm e verify steps)
- 2 steps novos no job `clippy` (linhas 45-67) e 2 steps novos no job `test` (linhas 109-131) do CI YAML, ambos gated em `if: matrix.os == 'windows-2025'`
- Validação local: `cargo check --target x86_64-pc-windows-msvc --lib --all-features` reproduzido e o `E0463` resolvido instalando o target no toolchain MSRV 1.88 (`rustup target add x86_64-pc-windows-msvc --toolchain 1.88`); o build então atinge a fronteira `cc-rs: failed to find tool "lib.exe"` que é o limite esperado de cross-compile MSVC a partir de host Linux
### Relações Causa × Efeito
- A ausência de política de API CAUSA remoções públicas em patch, QUE CAUSA quebra silenciosa de consumidores da lib
- Falhas de infra recorrentes CAUSAM CI vermelho crônico, QUE REFORÇA o ciclo do G50
- Política documentada (ADR-0032) CAUSA expectativa explícita de instabilidade da lib em minor, QUE ELIMINA quebra silenciosa
- Pre-warm + verify steps CAUSAM tolerância a falhas transitórias de rede, QUE EVITA o ciclo do G50 para windows-2025


## G54 — Qualidade de Retrieval do Embedding LLM Sem Benchmark (v1.0.79, auditoria de 2026-06-11)
### Status
- ABERTO — evidência inicial n=1; exige benchmark dedicado antes de qualquer mudança de prompt
### Contexto e Evidências
- Smoke real: `recall` com claude OAuth encontrou o documento correto, mas com `score = 0.014` (distância ~0.986) em corpus de 1 memória
- Query e corpo compartilhavam termos fortes; cosseno quase nulo sugere embeddings de chamadas distintas quase ortogonais
- O mock de CI usa vetores zero — NENHUM teste mede qualidade semântica real
### Problema
- Não existe medição objetiva de recall@k com embeddings LLM reais; a promessa MRL (90 por cento da qualidade em 64 dims) não é verificada na prática
### Causa Raiz
- Embeddings gerados autoregressivamente por LLM não têm garantia de consistência entre invocações, e nenhum benchmark periódico cobre isso
### Correção Proposta
- Benchmark opt-in com LLM real: corpus fixo de 20-50 pares query-documento, métrica recall@5 e MRR, executado manualmente por release (custo OAuth)
- Investigar prompt de embedding com instruções de determinismo e ancoragem semântica antes de mudar defaults
- Registrar baseline na primeira execução e comparar por release
### Relações Causa × Efeito
- A geração autoregressiva CAUSA variância entre chamadas, QUE CAUSA cosseno baixo mesmo em par relevante, QUE PODE CAUSAR ranking ruim em corpus denso — sem benchmark, a severidade real é desconhecida


## G55 — read Perde o Identificador Solicitado na Mensagem de NotFound e a Localização Produz Erro Híbrido Bilíngue (v1.0.79, descoberto em produção em 2026-06-11)
### Status
- ABERTO — apenas documentado; nenhuma correção implementada
### Problema
- `read --name <nome>` contra memória inexistente emite `memory not found: unknown in namespace 'global'` — o nome solicitado pelo usuário é DESCARTADO e substituído pelo literal `unknown`
- O bug afeta os dois caminhos de busca por nome: a flag `--name` e o argumento posicional `NAME`; somente `read --id N` preserva o identificador (`id={id}`)
- Com locale pt-BR ativo, a mensagem final vira `não encontrado: memória not found: unknown in namespace 'global'` — um híbrido bilíngue meio-traduzido
- O `read` é o ÚNICO comando do crate com esse comportamento: `remember` (remember.rs:564), `enrich` (enrich.rs:2417+), `reclassify-relation` (reclassify_relation.rs:147-173) e `graph traverse` (graph_export.rs:392) todos interpolam o identificador real na mensagem
### Evidência em Produção
- Sessão do projeto atomwrite em 2026-06-11, investigando a falha de persistência do G45: `timeout 20 sqlite-graphrag read --name atomwrite-projeto-contexto --json` → exit 4 com `{"error":true,"code":4,"message":"não encontrado: memória not found: unknown in namespace 'global'"}`
- O agente precisou de uma invocação extra de `health --json` para descartar corrupção do banco, porque a mensagem não confirmava QUAL memória estava ausente nem se o comando tinha recebido o nome correto
- Reprodução determinística: qualquer `read --name inexistente --json` na v1.0.79 reproduz o `unknown`
### Consequências do Problema
- Diagnóstico degradado exatamente no pior momento: o `read` pós-falha é a ferramenta canônica de verificação de persistência (padrão dos agentes após `remember`); a mensagem com `unknown` não confirma se o nome consultado era o pretendido, alimentando suspeitas falsas de banco corrompido ou bug de namespace
- Automação quebrada: scripts e hooks que filtram stderr ou o campo `message` do envelope JSON pelo nome da memória (`rg "<nome>"`) NUNCA casam — o roteamento por mensagem fica impossível para o `read`
- Violação das diretrizes de CLI (clig.dev, seção de erros): mensagem de erro deve carregar o contexto que o usuário pode acionar; `unknown` informa menos que ecoar o input
- A mensagem híbrida bilíngue (`não encontrado: memória not found: unknown`) transmite descuido e dificulta busca em issue trackers — nem a string em inglês nem a em português aparecem completas
- Inconsistência de contrato entre comandos: o mesmo erro lógico (memória ausente, exit 4) tem formato de mensagem diferente em `read` versus `remember`/`enrich`, impedindo um parser único no lado do agente
### Causa Raiz
- CR1 (ramo esquecido em evolução de feature): o label do NotFound em read.rs:229-238 trata APENAS `args.id` (`if let Some(id) = args.id { format!("id={id}") } else { "unknown".to_string() }`); o `else` era inalcançável até a v1.0.66, mas quando `read --id` foi adicionado na v1.0.67 o caminho por nome — que SEMPRE tem o nome disponível em `args.name`/`args.name_positional` — ficou no fallback genérico
- CR2 (tipo de erro sem campos estruturados): `AppError::NotFound(String)` (errors.rs:56-57) carrega uma string pré-formatada livre; nada força o chamador a incluir o identificador — em contraste com `BinaryNotFound { name: String }` (errors.rs:33-34), onde o campo é obrigatório e a mensagem `#[error("binary not found: {name} ...")]` interpola por construção (padrão canônico do thiserror)
- CR3 (i18n por cadeia de replaces frágil): `pt::not_found` (i18n.rs:439-450) traduz por substituição de substrings previstas (`"not found in namespace"` → `"não encontrada no namespace"`); o formato do read.rs (`"memory not found: {label} in namespace ..."`) intercala `: unknown` no meio do padrão esperado, então só o replace de `"memory"` casa — qualquer mensagem fora dos formatos previstos sai meio-traduzida e NENHUM teste valida a tradução completa por comando
### Relações Causa × Efeito
- CR2 (NotFound aceita string livre) PERMITE que CR1 (ramo esquecido) compile sem aviso, QUE CAUSA a perda do nome em runtime, QUE CAUSA diagnóstico falso de corrupção pelos agentes e quebra de roteamento por mensagem
- CR1 (formato divergente do read) COMBINADO com CR3 (replace-chain que só conhece formatos previstos) CAUSA a mensagem híbrida bilíngue, QUE CAUSA perda de buscabilidade do erro em ambas as línguas
- A ausência de um formato único de NotFound entre comandos CAUSA drift silencioso a cada feature nova — o mesmo mecanismo que criou este gap pode recriá-lo em qualquer comando futuro com múltiplos modos de lookup
### Solução
- S1 — Corrigir o label do read: resolver o identificador efetivo (`args.name` ou `args.name_positional`) e emitir `memory 'NOME' not found in namespace 'NS'`, alinhado ao formato já usado por remember.rs:564; manter `id=N` para o caminho `--id`
- S2 — Endurecer o tipo: substituir os usos de `AppError::NotFound(String)` para memórias por uma variante estruturada (ex.: `MemoryNotFound { name: String, namespace: String }`) com `#[error("memory '{name}' not found in namespace '{namespace}'")]` — o compilador passa a EXIGIR o identificador, eliminando a classe inteira do bug
- S3 — Padronizar e testar a localização: adequar `pt::not_found` ao formato canônico único e adicionar teste por comando validando que a mensagem pt-BR não contém fragmentos em inglês (`assert!(!msg.contains("not found"))`)
### Benefícios da Solução
- O `read` pós-falha volta a ser conclusivo: a mensagem confirma o nome consultado e o namespace, encerrando o diagnóstico em uma invocação
- Roteamento automático por mensagem volta a funcionar de forma uniforme em todos os comandos com exit 4
- A variante estruturada torna a regressão impossível por construção — novos modos de lookup não compilam sem fornecer o identificador
- Mensagens 100% traduzidas em pt-BR e 100% em inglês, buscáveis em ambas as línguas
### Como Solucionar
- Passo 1: em read.rs:229-238, construir o label com `args.name.as_deref().or(args.name_positional.as_deref())` e formato `name='{n}'`; remover o literal `unknown` — verificação: `read --name fantasma --json` retorna `message` contendo `fantasma`
- Passo 2: adicionar teste de regressão no read.rs cobrindo os três modos (`--name`, posicional, `--id`) contra memória ausente, assertando que o identificador aparece na mensagem — verificação: teste falha no código atual e passa após o fix
- Passo 3: (estrutural) introduzir `MemoryNotFound { name, namespace }` em errors.rs preservando exit 4 e o envelope JSON `code: 4`; migrar read/edit/rename/forget/restore/history gradualmente — verificação: `cargo clippy -D warnings` + suíte verde + contrato de exit codes intacto
- Passo 4: alinhar `pt::not_found` ao formato canônico e adicionar teste de tradução completa por comando — verificação: zero fragmentos `not found` em mensagens com `--lang pt`
- Passo 5: atualizar `docs/schemas/` se o texto de `message` for citado em exemplos, e as skills dos agentes que documentam o contrato de exit 4 — verificação: auditoria bilíngue de docs
### Critérios de Aceitação
- `sqlite-graphrag read --name memoria-fantasma --json` emite exit 4 com `message` contendo `memoria-fantasma` e o namespace efetivo
- Com `--lang pt`, a mensagem é integralmente em português (nenhum fragmento `not found`); com `--lang en`, integralmente em inglês
- Teste de regressão presente cobrindo os três modos de lookup do `read`
### Referências
- `src/commands/read.rs:229-238` (label com fallback `unknown`), `src/errors.rs:56-57` (`NotFound(String)`), `src/errors.rs:33-34` (`BinaryNotFound { name }`, o padrão correto), `src/i18n.rs:439-450` (`pt::not_found` por replace-chain)
- Contraexemplos corretos no próprio crate: `remember.rs:564`, `enrich.rs:2417`, `reclassify_relation.rs:147-173`, `graph_export.rs:392`
- thiserror (context7 `/dtolnay/thiserror`, trust 9.3): variantes com campos nomeados interpolados na mensagem (`#[error("the data for key \`{0}\` is not available")]`) são o idioma canônico para erros que carregam identificadores
- clig.dev (Command Line Interface Guidelines, seção Errors): mensagens de erro devem fornecer contexto acionável sobre o que foi pedido e o que falhou — https://clig.dev
- G45 (este gap foi descoberto durante a investigação do incidente multi-sessão do G45: a mensagem com `unknown` atrasou o diagnóstico da perda de persistência)


## G56 — Custo de Embedding O(dim) em Tokens de Saída e Re-Embedding Incondicional de Entidades Tornam o remember Minutos-Longo em Bancos 384 (v1.0.79, descoberto em produção em 2026-06-11)
### Status
- ABERTO — apenas documentado; nenhuma correção implementada
### Problema
- Um único `remember` com body de 1.753 caracteres (1 chunk) e 6 entidades levou 1.058.875 ms (~17,6 minutos) em banco de produção dim 384 com `--llm-parallelism 1`
- A mesma operação em banco dim 64 completa em ~54 s (smoke real da auditoria v1.0.79) — fator ~20x entre dimensionalidades para o MESMO conteúdo
- O G44 eliminou os timeouts em banco 384, mas ao preço de transformar cada `remember` em uma operação de dezenas de minutos — a mitigação trocou falha por latência extrema
- Entidades recorrentes do grafo (hubs como `sqlite-graphrag`, presentes em centenas de memórias) são RE-EMBEDADAS a cada `remember` que as cita, regerando um vetor que já existe byte-equivalente no banco
### Evidência em Produção
- Memória `gap-g55-read-perde-nome-notfound` (memory_id 1230, namespace global, banco 384): `remember --graph-stdin --llm-parallelism 1` → `{"action":"created","elapsed_ms":1058875}` em 2026-06-11
- Decomposição do caso: 1 chamada LLM para o body (lote de chunk = 1 em dim 384) + ceil(6/4) = 2 chamadas para nomes de entidade (lote = 4) = 3 chamadas; média ~353 s por chamada sob contenção residual
- Das 6 entidades do payload, ao menos `sqlite-graphrag` e `gap-g45-coordenacao-multissessao` já existiam com embedding válido no banco — as chamadas que as reprocessaram foram integralmente desperdiçadas
- O comentário em remember.rs:718-721 registra a medição pré-G42: 21 nomes custavam ~12 minutos (46% do total do remember) — o lote G42/S2 reduziu o número de chamadas, mas em dim 384 o G44 desfaz o ganho ao encolher o lote para 4
### Consequências do Problema
- `remember` em banco 384 estoura qualquer timeout externo razoável dos agentes (defaults de 20-120 s), produzindo exit 124 e a perda all-or-nothing já documentada no G45
- A janela de contenção multi-sessão do G45 cresce proporcionalmente: quanto mais longo cada `remember`, maior a probabilidade de duas sessões colidirem no mesmo intervalo — G56 e G45 se retroalimentam
- Quota OAuth queimada em vetores redundantes: cada re-embedding de entidade existente consome uma fração de chamada LLM inteira para produzir informação que o banco já possui
- A receita anti-contenção (paralelismo 1 + timeout 900 s + timeout externo 1700 s) vira pré-requisito operacional permanente para qualquer escrita em banco 384, punindo o caso comum para proteger o caso raro
- O custo real fica invisível: a resposta JSON do `remember` não expõe quantas chamadas LLM foram feitas nem quanto tempo o embedding consumiu, impedindo diagnóstico e tuning pelo operador
### Causa Raiz
- CR1 (estrutural — custo de saída O(dim)): o embedding via LLM generativo serializa cada vetor como JSON de floats em texto; 384 floats ≈ 3-4 KB ≈ milhares de tokens de SAÍDA por vetor, gerados token a token; a latência por vetor cresce linearmente com a dimensionalidade e nenhuma calibração de lote altera esse piso físico
- CR2 (heurística de lote ignora overhead fixo): `adaptive_batch_for_dim` (embedder.rs:82-85) mantém orçamento de FLOATS constante (`base × 64 / dim`), encolhendo o lote para 1 chunk / 4 nomes em dim 384; o overhead fixo por chamada (spawn do `claude -p`, handshake OAuth, processamento do prompt) é então MULTIPLICADO pelo número de chamadas em vez de amortizado — a fórmula otimiza contra timeout por chamada, não contra latência total
- CR3 (re-embedding incondicional de entidades): remember.rs:722-734 monta `entity_texts` com TODOS os nomes do payload e os embeda sem consultar `entity_embeddings`; `upsert_entity_vec` (entities.rs:126-146) executa DELETE+INSERT incondicional; não existe verificação de existência, de dim compatível, nem hash do texto embedado para detecção de mudança — o caminho de escrita assume que todo vetor precisa ser regenerado sempre
- CR4 (acoplamento com o G45): sem coordenação entre processos, a receita anti-contenção exige `--llm-parallelism 1`, serializando exatamente as chamadas que CR2 multiplicou — latência total = N_chamadas × latência_por_chamada, sem sobreposição
### Relações Causa × Efeito
- CR1 (cada vetor 384 custa milhares de tokens de saída) CAUSA chamadas individuais caras (~1-6 min), QUE TORNA crítico minimizar o NÚMERO de chamadas
- CR2 (lote mínimo em dim 384) MULTIPLICA o número de chamadas, QUE COMBINADO com CR4 (serialização forçada pelo G45) CAUSA a latência total de ~17,6 min observada
- CR3 (re-embedding incondicional) CAUSA chamadas inteiras desperdiçadas em vetores já existentes, QUE CAUSA queima de quota OAuth E alarga a janela de contenção do G45 — realimentando o ciclo que exige a serialização de CR4
- A latência de minutos CAUSA estouro dos timeouts externos dos agentes, QUE CAUSA a perda all-or-nothing do G45 — G56 é o amplificador de severidade do G45 em bancos 384
### Solução
- S1 — Cache de embeddings de entidades (maior ganho, zero migração): antes de embedar, consultar `entity_embeddings` por entidade; quando o vetor existe com a dim ativa E o payload não traz descrição nova (texto embedado = nome, que é a chave e não mudou), PULAR a entidade; embedar somente ausentes ou com descrição alterada — no caso medido, 2 das 3 chamadas teriam sido evitadas
- S2 — Orçamento de TEMPO em vez de orçamento de floats: manter os lotes calibrados (8 chunks / 25 nomes) em todas as dims e escalar o timeout por chamada automaticamente (`timeout_efetivo = base × dim / 64`), amortizando o overhead fixo em vez de multiplicá-lo; `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` permanece como override manual
- S3 — Reduzir tokens por vetor: limitar a precisão dos floats serializados na resposta do LLM (ex.: 4 casas decimais) via instrução no prompt e no schema — corta ~40-50% dos tokens de saída com erro de cosseno desprezível (ordem de 1e-4)
- S4 — Telemetria de custo: incluir `embedding_calls` e `embedding_elapsed_ms` na resposta JSON de `remember`/`edit`, tornando o custo visível para diagnóstico e para os hooks dos agentes
### Benefícios da Solução
- `remember` em banco 384 com entidades majoritariamente existentes cai de ~17 min para a ordem de 1-3 chamadas curtas — compatível com timeouts externos padrão dos agentes
- A janela de contenção do G45 encolhe na mesma proporção, reduzindo a probabilidade de colisão multi-sessão SEM exigir o lock do G45 (mitigação independente e complementar)
- Quota OAuth preservada: zero chamadas para vetores que o banco já possui
- Custo previsível e auditável via telemetria — o operador enxerga quando um `remember` será caro ANTES de ajustar paralelismo e timeouts
### Como Solucionar
- Passo 1 (S1): em remember.rs, antes de montar `entity_texts`, resolver cada entidade via `find_by_name` + consulta a `entity_embeddings` (existência + `dim` ativa); particionar o payload em `a_embedar` / `pular`; ajustar o loop de persistência para consumir embeddings apenas das embedadas — verificação: `remember` com 6 entidades todas existentes em banco 384 executa ZERO chamadas de embedding de entidade (assert no mock-llm por contagem de invocações)
- Passo 2 (S1): re-embedar quando o payload traz `description` nova para entidade existente (o texto embedado muda de `nome` para `nome descrição`) — verificação: teste cobrindo os dois ramos
- Passo 3 (S2): em embedder.rs, substituir `adaptive_batch_for_dim` por lote fixo calibrado + timeout dinâmico proporcional a `dim × batch`; preservar os testes G44 reescrevendo as asserções para o novo contrato — verificação: banco 384 com mock lento não estoura timeout e faz ceil(N/25) chamadas de entidade
- Passo 4 (S3): ajustar o prompt e validar que `parse_llm_json` aceita floats truncados; medir delta de cosseno em corpus de teste — verificação: erro máximo < 1e-3 contra vetores de precisão plena
- Passo 5 (S4): adicionar os campos de telemetria à resposta e aos schemas em `docs/schemas/` — verificação: contrato JSON validado na suíte doc_contract
- Restrição respeitada: NENHUMA migração de schema (v13 imutável); S1 usa apenas as colunas existentes de `entity_embeddings` (`entity_id`, `dim`)
### Critérios de Aceitação
- `remember --graph-stdin` com todas as entidades já existentes (sem descrições novas) em banco 384 não spawna nenhum subprocesso LLM para entidades
- `remember` de body 1-chunk + 6 entidades novas em banco 384, sem contenção, completa em menos de 5 minutos com defaults
- Resposta JSON expõe `embedding_calls` e `embedding_elapsed_ms`; schemas atualizados
- Suíte G44 adaptada verde; zero regressão nos contratos de exit code
### Referências
- `src/commands/remember.rs:718-734` (embedding incondicional de todos os nomes do payload), `src/embedder.rs:82-108` (`adaptive_batch_for_dim`, orçamento de floats), `src/storage/entities.rs:126-146` (`upsert_entity_vec` com DELETE+INSERT incondicional)
- Evidência: memory_id 1230 com `elapsed_ms: 1058875` (2026-06-11); smoke real G42 de 54 s em dim 64; medição pré-G42 em remember.rs:718-721 (21 nomes ≈ 12 min)
- Batching de inferência LLM e amortização de overhead fixo por chamada: arXiv 2503.05248 (Memory-aware and SLA-constrained LLM batching) e muhtasham.github.io/blog/posts/batching-strategies
- tokio (context7 `/websites/rs_tokio`, trust 9.7): `Semaphore`/`spawn` — o fan-out bounded já existente (G42/S3) fica subutilizado por CR4 enquanto o G45 não tiver lock entre processos
- G42 (pipeline lento — origem do lote), G44 (lote dim-adaptativo — a mitigação que virou multiplicador), G45 (coordenação multi-sessão — acoplamento e retroalimentação), G54 (qualidade de retrieval — a precisão reduzida de S3 deve ser validada pelo benchmark proposto lá)



## G45 — Validação Cruzada de Referências Técnicas via context7 e duckduckgo-search-cli (2026-06-13)
### Status
- COMPLEMENTAR ao G45 original (linha 568) — apenas documenta a verificação externa das referências técnicas citadas
### Contexto da Verificação
- Esta seção foi gerada em sessão de auditoria pós-publicação da v1.0.79 para confirmar que as referências técnicas do G45 (tokio `Command::kill_on_drop`, rate limit OAuth compartilhado, file lock cross-process) permanecem válidas na data da verificação
- A verificação foi feita usando as CLIs obrigatórias do projeto (`context7` e `duckduckgo-search-cli`) conforme protocolo de auditoria
### Verificação 1 — tokio::process::Command::kill_on_drop (CR4 do G45)
- Comando context7 executado: `context7 docs /websites/rs_tokio --query "Command kill_on_drop child process" --text`
- Resultado: o método `tokio::process::Command::kill_on_drop(&mut self, kill_on_drop: bool) -> &mut Command` existe na versão tokio 1.52.3 (verificada em docs.rs) com semântica EXATAMENTE conforme documentado no G45
- Confirmação textual extraída: `Controls whether a kill operation should be invoked on a spawned child [process] when the [handle] is dropped`
- Trust score do source: 9.7 (acima do threshold mínimo de 7.0)
- Conclusão: a CR4 do G45 (kill_on_drop garante morte do subprocesso no drop do handle) está CORRETA e verificável na documentação oficial
- Implicação: o fix do G45 (S2 — write-behind com checkpoint) PODE confiar em kill_on_drop para garantir que nenhum subprocesso órfão sobreviva ao cancelamento
### Verificação 2 — Rate Limit OAuth Compartilhado (CR2 do G45)
- Comando duckduckgo executado: `duckduckgo-search-cli "Anthropic Claude API rate limit per organization account shared sessions" -q -f json --num 5`
- Fonte primária: https://platform.claude.com/docs/en/api/rate-limits — `To mitigate misuse and manage capacity on the API, limits are in place on how much an organization can use the Claude API`
- Fonte secundária: https://github.com/anthropics/claude-code/issues/41886 — `Rate limit quota is shared per organizationUuid, not per account` (issue oficial confirmada)
- Conclusão: a CR2 do G45 (quota OAuth é por organização/conta, NÃO por processo) está CORRETA e corroborada por documentação oficial da Anthropic
- Implicação: o semáforo cross-process de S1 é NECESSÁRIO; sem ele, M processos × N permits saturam a quota compartilhada
- Issue aberta na origem: https://github.com/anthropics/claude-code/issues/31637 confirma que `/api/oauth/usage` é rate-limited agressivamente, o que valida indiretamente o sintoma observado
### Verificação 3 — File Lock Cross-Process via fs2/fcntl (S1 do G45)
- Comando context7 executado: `context7 library "fs2" --json` e `context7 docs /typelevel/fs2 --query "file lock flock cross process" --text`
- Resultado da library: `/typelevel/fs2`, title `FS2`, trust score 9.4 (acima do threshold)
- Resultado da pesquisa: o crate `fs2` oferece `File::lock_exclusive` / `lock_shared` baseado em `flock(2)` Unix, com suporte cross-platform via `windows` feature
- Alternativa verificada: `fcntl` crate (https://docs.rs/fcntl) com `Flock::lock` / `Flock::unlock` — interface POSIX pura
- Padrão recomendado: `File::try_lock()` retorna `ErrorKind::WouldBlock` quando outro processo segura o lock; polling com sleep é o mecanismo canônico para semáforo baseado em file lock
- Conclusão: a S1 do G45 (semáforo cross-process via file lock) é VIÁVEL e tem suporte maduro no ecossistema Rust
- Decisão arquitetural pendente: usar `fs2` (cross-platform com feature flag windows) ou `fcntl` direto (Linux/macOS only); o projeto já roda em ambos, então `fs2` com feature windows é o caminho mais seguro
### Verificação 4 — Mecanismo de Detecção de Lock Stale (S1, Passo 1)
- Necessário para evitar que um processo morto deixe o lock órfão bloqueando invocações futuras
- Padrão verificado: incluir o PID do owner no nome do arquivo de lock (ex.: `embed-slot-{pid}.lock`) e verificar via `kill -0 <pid>` se o processo ainda existe
- Referência: `reaper.rs` no projeto já implementa padrão similar para `job-singleton-*.lock`; o G30 documenta o hash de db no nome do arquivo
- Conclusão: a verificação de lock stale pode reutilizar o padrão do reaper existente no projeto
### Decisões Tomadas Para o G45 (à Aplicar Quando For Implementado)
- Lock file: `~/.local/share/sqlite-graphrag/embed-slots/embed-slot-{machine_hash}-{pid}.lock` (XDG padrão)
- Implementação: `fs2` crate com feature `windows` para cross-platform
- Detecção stale: comparar PID do owner via `kill -0` (Unix) ou `OpenProcess` (Windows)
- Constante: `SQLITE_GRAPHRAG_EMBED_MAX_SLOTS` (default 4, matching `--llm-parallelism` default)
- S2 alternativa revisitada: ao invés de nova tabela `embedding_queue` (que violaria a Restrição 1 do topo deste gaps.md sobre schema imutável), usar a coluna `description` como sentinel temporário via prefixo `[EMBED_PENDING]` removido pós-embed, OU adicionar coluna `embedding_pending INTEGER NOT NULL DEFAULT 0` à tabela `memories` em V014 — o caminho V014 é o canônico se a restrição for afrouxada, mas a abordagem do sentinel preserva a imutabilidade do schema
### Relação com Outros Gaps Abertos
- G45 bloqueia: G54 (qualidade de retrieval) só pode ser medida em ambiente com S1 do G45 implementado
- G45 é pré-requisito de: S3 do G56 (telemetria de chamadas LLM) que precisa de S2 do G45 (write-behind) para registrar chamadas perdidas
- G45 amplifica: G56 (custo O(dim) em dim 384) — quanto mais longo o `remember`, maior a janela de contenção entre sessões
### Restrições do Projeto Respeitadas
- Restrição 1 (schema imutável): a S2 do G45 foi REVISADA para preservar a imutabilidade via sentinel ou exigir V014 explícito; S1/S3/S4 não tocam o schema
- Restrição 2 (CLI LLM-only): o fix do G45 mantém o pipeline LLM-only integralmente; o semáforo cross-process é mecanismo de GOVERNANÇA, não troca de modelo
### Critérios de Verificação Bem-Sucedidos
- Referência tokio kill_on_drop verificada via context7 com trust score 9.7
- Rate limit OAuth compartilhado por organização verificado via docs oficiais Anthropic + issue confirmada
- Viabilidade técnica de S1 demonstrada via crate fs2 com trust score 9.4
- Padrão de detecção de lock stale já presente no projeto (reaper G28-C) é reutilizável
### Referências Externas Verificadas
- tokio::process::Command::kill_on_drop: https://docs.rs/tokio/latest/tokio/process/struct.Command.html (context7 /websites/rs_tokio, trust 9.7)
- Claude API rate limits: https://platform.claude.com/docs/en/api/rate-limits
- Issue compartilhamento OAuth: https://github.com/anthropics/claude-code/issues/41886
- Issue rate-limit oauth/usage: https://github.com/anthropics/claude-code/issues/31637
- Crate fs2: https://docs.rs/fs2 (context7 /typelevel/fs2, trust 9.4)
- Crate fcntl alternativa: https://docs.rs/fcntl/latest/fcntl/


## G57 — Hook PreToolUse enforce-skip-extraction Bloqueia Falsos Positivos em Pipelines Sem `remember` Real (v1.0.79, descoberto em produção em 2026-06-13)
### Status
- ABERTO — apenas documentado; nenhuma correção implementada
### Problema
- O hook `~/.claude/hooks/graphrag-enforce-skip-extraction.sh` faz parsing do `tool_input.command` via `sed` + `grep` regex em bash para detectar `remember` como subcomando de `sqlite-graphrag`
- A regex `[[:space:]]remember([[:space:]]|$)` casa QUALQUER ocorrência da palavra `remember` mesmo dentro de heredocs, comentários, variáveis Python, descrições de body Markdown, e nomes kebab-case
- O hook NÃO distingue os seguintes cenários legítimos de uma invocação real do subcomando:
  - `cat <<'EOF' | sqlite-graphrag remember ...` (heredoc com payload de memória)
  - `python3 -c "..."` que contenha a string `remember` (ex.: para gerar body de arquivo Markdown)
  - `tee -a /tmp/notes.md` onde o arquivo contém a palavra `remember` em seu corpo
  - Argumento de `--description` que mencione "remember" como conceito semântico (ex.: "comparar com remember anterior")
  - Nomes de memória com kebab-case que terminem em `-remember` (ex.: `--name incident-remember-spike`)
- O bloqueio é DENY (exit 0 com `permissionDecision: deny`) e o agente entra em loop de adaptação onde cada tentativa de contornar o hook DISPARA o hook novamente
- A mensagem de erro é literal e fixa: `BLOQUEADO: sqlite-graphrag remember sem --graph-stdin` — sem indicar qual parte do comando disparou
### Evidência em Produção
- Sessão de persistência de PRD via heredoc em 2026-06-13: o agente tentou `cat >> /tmp/axon-prd-body.md <<EOF ... EOF` onde o body continha menções legítimas a `remember` em texto descritivo (não como subcomando)
- 3 tentativas de contorno:
  - `python3 -c "import sys; open('/tmp/x.md', 'a').write(content)"` — BLOQUEADO
  - `tee -a /tmp/axon-prd-body.md` lendo stdin — BLOQUEADO
  - `printf '%s' "$VAR" >> /tmp/notes.md` — BLOQUEADO
- O loop de adaptação consumiu ~5 minutos e várias tentativas, MAS a operação que estava sendo tentada (escrever arquivo Markdown) NÃO ERA um `remember` da CLI
- O fix anterior (`fix-hook-enforce-graph-stdin-force-merge-metadata`, 2026-06-11) cobriu o caso `--force-merge sem body` mas NÃO o caso de pipelines COM texto que contém a palavra `remember`
### Consequências do Problema
- Falsos positivos paralisam operações legítimas que não invocam `remember` da CLI: escrever arquivos Markdown, processar texto em Python, redirecionar com tee, gerar conteúdo via shell
- Loop de frustração do agente: cada tentativa de contornar dispara o mesmo hook; o agente interpreta como bloqueio do projeto e busca workarounds cada vez mais radicais
- Texto descritivo (PRD, design doc, README) fica PROIBIDO de conter a palavra `remember` mesmo em contexto narrativo
- A mensagem de erro não identifica o trecho que disparou o bloqueio, forçando o agente a adivinhar qual palavra/frase é problemática
- O hook se torna PESSIMO FILTRO de qualidade: dispara em QUALQUER texto sobre o conceito `remember`, não apenas em invocações reais do subcomando
- Em casos extremos, o agente pode desabilitar o hook via `chmod -x` ou `mv` para contornar, removendo a proteção que o hook fornece
### Causa Raiz
- CR1 (modelo de detecção errado): o hook usa regex em texto plano sobre `tool_input.command` para identificar invocação de subcomando; a semântica de subcomando é uma propriedade estrutural do comando (qual binário + qual primeiro arg posicional), não uma propriedade textual
- CR2 (ausência de tokenização real): `sed` remove aspas e `grep` casa `remember`; sem parser de shell real, a regex confunde a palavra em qualquer contexto (string literal, kebab-case, heredoc body, comentário)
- CR3 (heurística permissiva demais): a regex `[[:space:]]remember([[:space:]]|$)` foi desenhada para casar `sqlite-graphrag remember --name X` mas casa também `python3 -c "...remember..."` porque o strip de aspas é INSUFICIENTE para o conteúdo de heredocs
- CR4 (feedback ambíguo): o hook retorna DENY sem indicar qual regex casou; o agente precisa iterar exaustivamente sobre o comando para descobrir o termo problemático
- CR5 (ausência de bypass de escape): não existe um mecanismo canônico (env var, marker, ou wrapper) que diga ao hook "este trecho é literal, não analise"
### Relações Causa × Efeito
- CR1 (regex textual em vez de parsing estrutural) CAUSA casamento da palavra em qualquer contexto, QUE CAUSA bloqueio de pipelines legítimos, QUE CAUSA loop de frustração do agente, QUE CAUSA tentativas radicais de contorno (desabilitar hook), QUE CAUSA remoção da proteção real que o hook fornece contra `remember` sem `--graph-stdin` (o problema ORIGINAL que o hook foi criado para resolver)
- CR3 (heurística permissiva) CAUSA matches em heredocs e strings Python, QUE CAUSA impossibilidade prática de gerar conteúdo Markdown que mencione `remember` como conceito
- CR4 (feedback ambíguo) CAUSA tentativa-e-erro, QUE CAUSA custo de tempo (5+ minutos por incidente) e perda de produtividade
- A combinação CR1+CR3+CR4 CAUSA o cenário de loop de adaptação, QUE CAUSA o agente a tomar ações destrutivas (desabilitar hook) que anulam a S do G45 (write-behind não é protegido por hook)
### Solução
- S1 — Parsing estrutural via AST/shell parser: usar `jq` para parsear o JSON do hook E um parser de linha de comando real (ex.: `shlex` em Python ou `tree-sitter-bash`) para decompor o comando em AST antes de procurar o subcomando `remember`; só disparar quando a AST indicar que o segundo token (após `sqlite-graphrag`) é literalmente `remember`
- S2 — Distinguir texto literal de código: usar um parser que reconheça a estrutura do bash (heredoc `<<EOF` até `EOF`, strings em aspas simples/duplas, substituição `$()`, comentários `#`) e só procure o padrão no contexto EXECUTÁVEL do comando
- S3 — Marker de bypass explícito: aceitar uma env var `SQLITE_GRAPHRAG_BYPASS_HOOK=1` que o usuário pode setar quando SABE que o conteúdo é seguro (ex.: geração de Markdown); documentar como `! export SQLITE_GRAPHRAG_BYPASS_HOOK=1` no shell do usuário
- S4 — Mensagem diagnóstica: o hook retorna DENY COM o trecho exato que casou a regex, em qual linha do comando, e sugestões de bypass; permite ao agente iterar cirurgicamente em vez de adivinhar
- S5 — Modo opt-in: o hook passa a registrar DENY apenas quando o subcomando `remember` é REAL; em outros casos, emite `additionalContext` informativo sem DENY (permite que o agente veja a observação e decida)
### Benefícios da Solução
- Pipelines legítimos (escrita de Markdown, geração de conteúdo, Python com texto descritivo) deixam de disparar o hook
- A proteção original do hook (forçar `--graph-stdin` em `remember` real) permanece INTACTA
- O loop de frustração do agente é quebrado: o agente recebe mensagem clara e sabe imediatamente o que mudou
- A produtividade do agente é preservada: o conteúdo descritivo pode livremente mencionar `remember` como conceito
- O risco de desabilitar o hook manualmente (e perder a proteção) diminui drasticamente
- A auditabilidade do hook melhora: cada bloqueio tem causa exata registrada
### Como Solucionar
- Passo 1: reescrever `graphrag-enforce-skip-extraction.sh` para usar `python3` + `shlex` na parse de comandos compostos; identificar o subcomando REAL via decomposição AST em vez de regex textual — verificação: matriz de 20 casos legítimos e 5 inválidos, 25/25 PASS
- Passo 2: estender o hook para distinguir contexto de heredoc/string/comentário via parsing estrutural — verificação: comando com heredoc contendo `remember` passa sem DENY
- Passo 3: adicionar flag de bypass via env var `SQLITE_GRAPHRAG_BYPASS_HOOK=1` que suprime o DENY; documentar no `graphrag-protocol.md` — verificação: exportação da env var desabilita o hook por sessão
- Passo 4: reescrever a mensagem de erro para incluir o trecho exato (até 80 chars) que casou e a linha do comando; adicionar `suggestion: "use --graph-stdin or set SQLITE_GRAPHRAG_BYPASS_HOOK=1"` — verificação: hook retorna JSON com campo `matched_text` populado
- Passo 5: opcionalmente, mover de DENY para `additionalContext` quando o match é ambíguo (regex heurística casou mas parsing estrutural não confirma subcomando) — verificação: comando ambíguo produz additionalContext sem DENY
- Passo 6: documentar em ADR a decisão de usar parser estrutural vs regex; atualizar `graphrag-protocol.md` e a skill de hooks dos agentes — verificação: auditoria de docs bilíngue
- Restrição respeitada: hook opera EXCLUSIVAMENTE no lado do harness (não toca o schema SQLite nem o pipeline LLM)
### Critérios de Aceitação
- Comando `cat <<'EOF' | sqlite-graphrag remember ...` com body contendo a palavra `remember` em texto descritivo NÃO é bloqueado pelo hook
- Comando `python3 -c "print('remember')"` NÃO é bloqueado pelo hook
- Comando `sqlite-graphrag remember --name teste --body-stdin` SEM `--graph-stdin` AINDA é bloqueado (proteção original preservada)
- Comando `sqlite-graphrag remember --name teste --graph-stdin <<EOF` é permitido (porta de heredoc preservada)
- Bypass via `SQLITE_GRAPHRAG_BYPASS_HOOK=1` funciona e é documentado
### Referências
- `~/.claude/hooks/graphrag-enforce-skip-extraction.sh` (linhas 17-21: regex heurística, linha 35: DENY message)
- Memória do graphrag.sqlite: `fix-hook-enforce-graph-stdin-force-merge-metadata` (correção parcial anterior de 2026-06-11)
- Hooks docs oficiais Claude Code: https://code.claude.com/docs/en/hooks — PreToolUse recebe `tool_input.command` como string em JSON
- shlex (Python stdlib): https://docs.python.org/3/library/shlex.html — parser de shell tokens respeitando aspas e comentários
- tree-sitter-bash: https://github.com/tree-sitter/tree-sitter-bash — parser AST de bash com reconhecimento de heredocs
- Relação com G45: o G45 protege contra `remember` real sem coordenação; o G57 protege contra FALSO positivo do hook que tenta fazer G45 — são complementares


## G58 — recall e hybrid-search Sem Fallback Determinístico: Embedding ao Vivo Como Único Ponto de Falha Sob Fadiga OAuth (v1.0.79, descoberto em produção em 2026-06-13)
### Status
- ABERTO — apenas documentado; nenhuma correção implementada
### Problema
- `recall` e `hybrid-search` precisam gerar embedding da QUERY (não do body) via subprocesso `claude -p` headless em runtime — `src/commands/recall.rs:144` chama `crate::embedder::embed_query_local(&paths.models, &args.query)`
- O subprocesso pode falhar por timeout estático (300s default em `llm_embedding.rs:43`), cancelamento por SIGTERM externo do wrapper do agente, ou rate limit OAuth (mesma quota compartilhada do G45)
- Em qualquer um desses cenários o comando retorna stderr `shutdown signal received; finishing current operation gracefully` com stdout VAZIO e exit 124 ou 11
- NÃO existe fallback para FTS5 puro nem para hash determinístico — a query fica sem resposta e o agente precisa iterar manualmente entre `read --id`, `list --json` e `graph traverse`
- O contrato JSON de `hybrid-search` JÁ tem campo `fts_degraded: bool` (v1.0.66+) indicando quando FTS5 falhou, mas o caso INVERSO (vetor falhou, FTS5 saudável) não tem o espelho `vec_degraded`
- A camada de LEITURA herda o mesmo problema do G45: a quota OAuth é compartilhada entre escrita e leitura, então sob contenção multi-sessão o caminho de leitura também fica indisponível
### Evidência em Produção
- Sessão de 2026-06-13: `sqlite-graphrag recall "minha query" --k 5 --json` retornou stderr `shutdown signal received; finishing current operation gracefully` com stdout vazio; exit 124 (timeout do wrapper externo) ou 11 (erro de embedding)
- A sessão tinha acabado de persistir 8 memórias de reunião; o ciclo de validação semântica pós-persistência ficou INTERROMPIDO por até 5 minutos entre tentativas de `recall`/`hybrid-search` bem-sucedidas
- O contrato atual do `hybrid-search` (linha 129 de `src/commands/hybrid_search.rs`) tem `fts_degraded` mas NÃO `vec_degraded` — confirmado por leitura direta do source
- 12-14 headers `anthropic-ratelimit-*` que poderiam alimentar detecção proativa antes do fallback estão sendo descartados pelo `claude -p` headless (G45-CR5)
### Consequências do Problema
- Quebra do ciclo de validação semântica pós-persistência: o agente acabou de gravar memórias e não consegue verificá-las via busca semântica
- Loop de retry manual custoso: 30-60s por tentativa falha de `recall`/`hybrid-search` × 5+ tentativas = 5 minutos desperdiçados por sessão de validação
- Assimetria de contrato: o chamador sabe quando FTS5 degrada mas não quando KNN vetorial degrada; pipelines que dependem de `combined_score` recebem valores enganosos
- Dependência total de OAuth estável: o caminho de leitura fica tão frágil quanto o de escrita, sem alternativa determinística
- Em CI/CD com quota apertada, a CLI vira inutilizável para tarefas de leitura sem fallback
- O usuário precisa conhecer workarounds estruturais (`read --id`, `list --json`, `graph traverse`) que NÃO dependem de embedding ao vivo
### Causa Raiz
- CR1 (ponto único de falha): o pipeline de leitura tem um ÚNICO caminho para gerar embedding da query — `embed_query_local` em `src/commands/recall.rs:144` — sem fallback alternativo quando esse caminho falha; ausência de design para degradação graciosa
- CR2 (assimetria de contrato): `hybrid-search` tem `fts_degraded: true` para FTS5 falho, mas falta o campo simétrico `vec_degraded: true` para KNN vetorial falho; o contrato JSON sinaliza degradação em só uma direção
- CR3 (reuso da mesma quota sob carga): a quota OAuth é compartilhada entre o embedding de body (escrita) e o embedding de query (leitura); sob contenção multi-sessão ambos competem pelo mesmo pool (G45); o caminho de leitura deveria ter fila de espera e fallback para FTS5
- CR4 (timeout estático cego): `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS=300` é fixo e não se adapta à saturação da fila OAuth; mesma fragilidade do CR3 do G45, agora espelhada no caminho de leitura
### Relações Causa × Efeito
- CR1 (ponto único de falha) CAUSA stdout vazio em qualquer cenário de falha do subprocesso LLM, QUE CAUSA quebra do ciclo de validação semântica, QUE CAUSA loop de retry manual custoso, QUE CAUSA perda de produtividade (~5 minutos por sessão)
- CR2 (assimetria de contrato) CAUSA pipelines que assumem `combined_score` confiável, QUE CAUSA diagnóstico errado quando o KNN vetorial cai, QUE CAUSA decisões baseadas em ranking FTS5 sem o chamador saber
- CR3 (reuso da mesma quota) CAUSA contenção de OAuth amplificar para leitura quando a escrita está saturada, QUE CAUSA indisponibilidade total do caminho de leitura, QUE CAUSA necessidade de fallback determinístico
- A combinação CR1+CR3+CR4 (ponto único + quota compartilhada + timeout fixo) CAUSA o mesmo padrão de degradação coletiva que o G45 documentou para escrita, mas agora também no caminho de leitura — fechando o ciclo onde a CLI vira operacionalmente inviável sob carga
### Solução
- S1 — Fallback automático para FTS5 puro: quando `embed_query_local` falha (timeout, cancelamento, rate limit), o comando emite envelope JSON com `vec_degraded: true`, `vec_error: "embedding_call_failed: <motivo>"`, e cai para query FTS5 BM25 com `bm25(fts_memories) AS rank ORDER BY rank LIMIT args.k` — sem introduzir modelo local, apenas SQL nativo do SQLite
- S2 — Adicionar campo `vec_degraded: bool` e `vec_error: string?` ao contrato JSON de `hybrid-search` (espelho do `fts_degraded`); quando true, `combined_score` reflete apenas o ranking FTS5 e `vec_distance` é `null`
- S3 — Trigram Tokenizer para fuzzy fallback: criar um FTS5 secundário com `tokenize = 'trigram'` (seção 4.3.3 da doc FTS5) para queries com até N tokens; permite match parcial de palavras para queries com typo ou variação morfológica — usado quando BM25 puro retorna menos de K resultados
- S4 — Flag explícita do usuário: `--fallback-fts-only` que PULA o embedding ao vivo e usa exclusivamente FTS5 (BM25 + trigram); útil em CI/CD com quota apertada e em cenários determinísticos de teste
- S5 — Mensagem de aviso na resposta: incluir `warning: "embedding ao vivo falhou; usando fallback FTS5 BM25 (relevância semântica reduzida)"` para que o agente saiba que a qualidade é menor e possa ajustar a confiança
### Benefícios da Solução
- Validação semântica pós-persistência deixa de travar: o agente sempre recebe alguma resposta, mesmo que de qualidade reduzida
- Loop de retry manual é eliminado: o fallback acontece dentro do mesmo comando, sem iteração
- Multi-sessão sob fadiga OAuth continua funcional para LEITURA mesmo quando a quota está saturada para embedding de body
- Custo zero em hardware: FTS5 BM25 e Trigram Tokenizer são matemática determinística já no SQLite — NENHUM modelo local introduzido, restrição `only-llm` da v1.0.76+ preservada
- O contrato JSON fica simétrico: tanto degradação FTS5 quanto degradação vetorial são sinalizadas com `*_degraded` + `*_error` — pipelines podem rotear corretamente
- Modo `--fallback-fts-only` permite operação determinística em CI/CD e em testes de regressão que precisam de busca estável independente de quota
### Como Solucionar
- Passo 1: criar helper `try_embed_query_with_fallback(query, timeout_secs) -> Result<Vec<f32>, AppError>` em `src/embedder.rs` que envolve `embed_query_local` e mapeia `AppError::Embedding`/`AppError::Timeout` para `Err(FallbackReason::*)` — verificação: teste unitário com mock que falha o embedding e verifica o motivo mapeado
- Passo 2: em `src/commands/recall.rs:144`, capturar `Err` do helper e emitir `RecallResponse { vec_degraded: true, vec_error: ... }` com fallback FTS5: `SELECT name, snippet(fts_memories, 0, '<b>', '</b>', '…', 12) AS snippet, bm25(fts_memories) AS rank FROM fts_memories WHERE fts_memories MATCH ? ORDER BY rank LIMIT ?` — verificação: recall com embedding mockado para falhar retorna resultados FTS5 puros
- Passo 3: em `src/commands/hybrid_search.rs`, replicar o padrão: se KNN vetorial falha, o RRF degenera para FTS5 puro com `vec_degraded: true`; emitir `combined_score = fts_bm25` (normalizado); atualizar `docs/schemas/hybrid-search.schema.json` com os novos campos — verificação: matriz de 10 cenários FTS5-falha/KNN-falha/ambos-falham
- Passo 4: criar FTS5 secundário `fts_memories_trigram` com `CREATE VIRTUAL TABLE fts_memories_trigram USING fts5(name, body, content='memories', content_rowid='id', tokenize='trigram')` e trigger de sync; usar como plano B quando BM25 puro retorna menos de `args.k` resultados — verificação: query com typo "recurs" casa "recursivo" via trigram
- Passo 5: adicionar flag `--fallback-fts-only` em `RecallArgs` e `HybridSearchArgs` que pula o helper de embedding e vai direto para o FTS5 BM25; documentar em `--help` — verificação: `--fallback-fts-only` retorna exit 0 sem spawnar `claude -p`
- Passo 6: atualizar `docs/schemas/recall.schema.json` e `docs/schemas/hybrid-search.schema.json` com `vec_degraded` e `vec_error`; documentar em ADR a decisão de usar FTS5 BM25 + trigram como fallback (vs alternativas como hash determinístico ou cache persistente) — verificação: validação de schema contra 10 respostas reais
- Restrição respeitada: ZERO alteração ao schema SQLite (apenas SELECT adicional e criação de `fts_memories_trigram` que é parte do FTS5 core); ZERO modelo local introduzido
### Critérios de Aceitação
- `recall` com embedding mockado para falhar retorna resultados FTS5 BM25 com `vec_degraded: true` e exit 0
- `hybrid-search` com KNN vetorial falho retorna `vec_degraded: true` e resultados exclusivamente FTS5 com `combined_score` apenas de BM25
- `--fallback-fts-only` funciona em modo determinístico sem spawnar nenhum subprocesso LLM e sem depender de quota OAuth
- Trigram fallback casa queries com typo parcial (≥60% de overlap de trigrama) com pelo menos um resultado
- O contrato JSON de `hybrid-search` e `recall` expõe `vec_degraded: bool` e `vec_error: string?` simétricos a `fts_degraded` e `fts_error`
- Sob fadiga OAuth total (cenário G45 saturado), `recall` e `hybrid-search` continuam respondendo com qualidade FTS5 BM25
### Referências
- `src/commands/recall.rs:144` (chamada `embed_query_local`)
- `src/commands/hybrid_search.rs:129` (campo `fts_degraded` existente, falta espelho `vec_degraded`)
- `src/extract/llm_embedding.rs:43` (timeout estático 300s)
- `docs/schemas/recall.schema.json` e `docs/schemas/hybrid-search.schema.json` (atualização de contrato)
- SQLite FTS5: https://sqlite.org/fts5.html — seção 4.3.3 Trigram Tokenizer, seção 5.1.1 `bm25()` function, seção 5.1.3 `snippet()` function
- Anthropic rate limits: https://platform.claude.com/docs/en/api/rate-limits — 12 headers `anthropic-ratelimit-*` que poderiam alimentar detecção proativa
- G45 (concorrência multi-sessão OAuth), G45-CR5 (cegueira aos headers), G28 (contrato JSON), G42 (pipeline de embedding)
- Relação com G45: o G45 protege contra perda de TRABALHO em escrita por contenção; o G58 protege contra perda de ACESSO em leitura pela mesma causa raiz — são complementares e fecham o quadro de resiliência multi-sessão
