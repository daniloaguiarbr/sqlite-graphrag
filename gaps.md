# Gaps — Problemas Pendentes de Solução na CLI sqlite-graphrag


## GAP-ENRICH-BACKLOG-CONVERGE
- ID segue a convenção do projeto `GAP-<DOMINIO>-<DESCRITOR>`
- Domínio afetado `enrich --operation memory-bindings`
- Estado RESOLVIDO em v1.0.96 (ADR-0055; ver CHANGELOG [1.0.96])
- Prioridade ALTA
- Resumo o backlog de bindings não converge para zero
- Convergência quebra na presença de qualquer falha permanente


## Problema
- O comando `enrich --operation memory-bindings` vincula memórias a entidades
- O scan de pendências vive em `scan_unbound_memories` no arquivo `src/commands/enrich.rs`
- Esse scan retorna TODA memória sem aresta em `memory_entities`
- A drenagem completa exige um loop externo de N rounds
- Cada round dispara um novo `enrich` e re-escaneia as pendências
- Itens que falham continuam sem binding no grafo
- Itens falhos retornam intactos ao round seguinte
- NÃO existe estado terminal de falha para um item
- Um item cronicamente falho permanece pendente para sempre


## Consequências do Problema
- O scan NUNCA retorna zero quando há falha permanente
- O loop de drenagem esgota o teto de rounds sem FILA VAZIA
- Um núcleo de itens irreparáveis trava a conclusão do backlog
- O hook externo precisa de `timeout` para não pendurar
- O `timeout` externo corta a drenagem antes do fim
- Falhas transitórias e permanentes recebem o mesmo tratamento cego
- Um corpo vazio é re-tentado tão agressivamente quanto um `429`
- O custo de API cresce ao re-tentar itens sem chance de sucesso
- O operador não distingue pendência sadia de pendência morta


## Causa Raiz do Problema
- A pendência é derivada do ESTADO DO GRAFO desejado
- A definição é memória sem aresta em `memory_entities` linha 1005
- A pendência NÃO é derivada do ESTADO DA TENTATIVA
- A fila `.enrich-queue.sqlite` tem `attempt` linha 680
- A fila tem `status` mas sem valor terminal `dead`
- O scan ignora a fila e varre apenas a tabela `memories`
- O campo `attempt` nunca realimenta a elegibilidade do scan
- A flag `--retry-failed` reseta `attempt=0` linha 2037
- A fila é limpa entre execuções sem `--resume` linha 2044
- O worker incrementa `attempt` linha 2223 mas nunca o lê para desistir
- NÃO há classificação de erro transitório versus permanente persistida
- NÃO há campo `next_retry_at` para backoff temporal
- O resultado é a ausência de estado absorvente de falha


## Relações Causa e Efeito
- CAUSA pendência definida pelo grafo EFEITO falha permanente é re-selecionada eternamente
- CAUSA fila desacoplada do scan EFEITO `attempt` não exclui item do scan
- CAUSA `--retry-failed` zera `attempt` EFEITO histórico de tentativas é perdido
- CAUSA fila limpa sem `--resume` EFEITO cada round recomeça sem memória de falhas
- CAUSA sem classificação de erro EFEITO trata `429` igual a corpo inválido
- CAUSA sem `next_retry_at` EFEITO re-tenta de imediato sem espaçamento
- CAUSA sem estado `dead` EFEITO o scan nunca converge para zero
- CAUSA processamento REST serial EFEITO drenagem lenta de dezenas de minutos


## Solução
- Introduzir estado absorvente de falha `dead` na fila de enrich
- Persistir por item `attempt`, `last_error`, `error_class` e `next_retry_at`
- Reusar `AttemptOutcome` de `src/retry.rs` linha 207 para classificar a falha
- Reusar `compute_delay` de `src/retry.rs` linha 87 para o backoff
- Promover item a `dead` após N tentativas ou em falha permanente
- Redefinir pendência como sem binding E elegível por tentativa
- Elegível significa `attempt < N` E `now >= next_retry_at` E `status != dead`
- Expor modo `--until-empty` com checkpoint por item e `--max-runtime`
- Expor subcomando `--status` read-only para hook e timer
- Expor concorrência opcional `--llm-concurrency K` com escrita serializada


## Benefícios da Solução
- Convergência matemática garantida do backlog para zero
- O loop bash externo de N rounds deixa de ser necessário
- O `timeout` externo do hook torna-se irrelevante
- A CLI passa a garantir completude e término por conta própria
- O reuso de `AttemptOutcome` e `compute_delay` respeita DRY
- O dead-letter fica contabilizado à parte e revisável sob demanda
- O custo de API cai ao parar de re-tentar itens mortos
- O throughput melhora muito com concorrência REST controlada
- A observabilidade via `--status` evita disparos vazios de enrich


## Como Solucionar
- Passo 1 estender o schema da fila com `error_class` e `next_retry_at`
- Passo 1 adicionar o valor de status `dead` e índice associado
- Passo 2 no worker classificar o erro em `Transient` ou `HardFailure`
- Passo 2 mapear erro de rede `429` e `5xx` como `Transient`
- Passo 2 mapear corpo inválido e parse rejeitado como `HardFailure`
- Passo 3 em erro transitório agendar `next_retry_at = now + compute_delay(attempt)`
- Passo 4 em erro permanente ou `attempt >= cap` marcar status `dead`
- Passo 5 redefinir a elegibilidade do scan considerando fila e dead-letter
- Passo 6 adicionar `--until-empty` com checkpoint por item e `--max-runtime`
- Passo 7 adicionar `--status` read-only reportando pending dead e done
- Passo 8 adicionar `--llm-concurrency K` com escrita serializada pelo singleton
- Passo 9 escrever testes de convergência item permanente vira dead
- Passo 9 escrever testes de backoff e de elegibilidade do scan
- Validação build sem erros clippy sem warnings nextest verde
- Validação cobertura mínima de 80 por cento no código novo


## Evidências de Código
- `src/commands/enrich.rs:991` define `scan_unbound_memories`
- `src/commands/enrich.rs:1005` usa `NOT EXISTS` em `memory_entities`
- `src/commands/enrich.rs:669` cria a fila com `attempt` sem `dead`
- `src/commands/enrich.rs:2037` reseta `attempt=0` em `--retry-failed`
- `src/commands/enrich.rs:2044` limpa a fila sem `--resume`
- `src/commands/enrich.rs:2223` incrementa `attempt` sem ler para desistir
- `src/retry.rs:207` já define `AttemptOutcome` Transient e HardFailure
- `src/retry.rs:87` já define `compute_delay` com half-jitter


## Fontes Consultadas
- duckduckgo-search-cli systemoverflow classificação transitória versus permanente
- duckduckgo-search-cli AWS Prescriptive Guidance retry com backoff
- duckduckgo-search-cli effectum job queue baseada em SQLite em Rust
- docs-rs keen-retry retry como valor com outcomes diagnosticáveis
- docs-rs nexo-poller runtime de polling com retries e ack
- context7 `/cenkalti/backoff` trustScore 9.1
- context7 `/coveooss/exponential-backoff` trustScore 8.6


===




## GAP-OPENROUTER-REST-CONCURRENCY
- ID segue a convenção do projeto `GAP-<DOMINIO>-<DESCRITOR>`
- Domínio afetado embedding via OpenRouter E enrich via OpenRouter
- Estado RESOLVIDO em v1.0.96 (ADR-0055; ver CHANGELOG [1.0.96])
- Prioridade ALTA
- Resumo a vazão é serial onde a rede permitiria concorrência
- Restrição paralelismo SOMENTE via OpenRouter REST
- PROIBIDO codex headless claude-code headless opencode headless


## Problema
- O embedding via OpenRouter agrupa 32 textos por chamada REST
- Mas processa os lotes de forma SERIAL entre si
- O loop `for chunk in texts.chunks(MAX_BATCH_SIZE)` aguarda cada lote
- Não há concorrência entre lotes na camada REST
- O `Semaphore` paralelo de `embedder.rs` mira subprocesso local
- O caminho REST faz um único `block_on(embed_batch)` por vez
- O enrich via OpenRouter já tem worker pool por threads
- Mas o `--llm-parallelism` padrão é 1 ou seja serial
- O pool usa `std::thread::scope` com threads de SO pesadas
- Cada worker abre suas próprias conexões de banco
- Os guardrails de clamp avisam sobre fan-out de subprocesso CLI
- Esses avisos citam Codex OpenCode e Claude Code subprocess
- Esses avisos são código MORTO sob OpenRouter REST puro
- O singleton de job por namespace impede multiprocesso no mesmo banco
- A concorrência REST que o OpenRouter permite NUNCA é explorada


## Consequências do Problema
- Ingest de 100 markdowns embeda lote a lote sem sobreposição de rede
- O tempo de parede cresce linear com a contagem de lotes
- Enrich de 3000 descrições roda serial por padrão
- O operador precisa lembrar de passar `--llm-parallelism N` manualmente
- A latência de rede de cada chamada fica ociosa sem pipeline
- A CPU espera o socket em vez de preparar o próximo lote
- Threads de SO desperdiçam recursos para I/O de rede assíncrono
- O custo de oportunidade é vazão muito abaixo do teto do provedor
- O usuário percebe drenagem de dezenas de minutos evitável
- A escrita SQLite serial vira gargalo aparente sem ser o real
- O teto Cloudflare de 4 a 16 conexões fica subutilizado


## Causa Raiz do Problema
- O modelo de concorrência foi desenhado para subprocessos LLM locais
- A unidade de trabalho assumida é um fork de CLI caro
- Esse fork é limitado por CPU por RAM e por OAuth rate-limit
- A migração para OpenRouter mudou a estrutura de custo radicalmente
- Agora a unidade real é uma requisição HTTP assíncrona barata
- O código ainda trata paralelismo como fan-out de subprocesso
- O embedding herdou um caminho REST SERIAL por isso
- O `Semaphore` de `embedder.rs` só fazia sentido para subprocesso
- O comentário confirma paralelismo efetivo 1 no caminho REST
- O enrich herdou pool de threads de SO calibrado para subprocesso
- O padrão 1 reflete o medo de OAuth rate-limit de CLI local
- Esse medo não se aplica a modelos pagos via OpenRouter
- O código NUNCA separou três concorrências distintas
- Concorrência de rede é alta e barata
- Concorrência de subprocesso é baixa cara e agora morta
- Concorrência de escrita SQLite é obrigatoriamente unitária
- A fusão dessas três dimensões numa só trava a vazão


## Relações Causa e Efeito
- CAUSA modelo desenhado para subprocesso EFEITO REST serial herdado
- CAUSA unidade assumida ser fork caro EFEITO embedding sem pipeline de lotes
- CAUSA `Semaphore` mirar subprocesso EFEITO paralelismo efetivo 1 no REST
- CAUSA padrão `--llm-parallelism 1` EFEITO enrich serial sem o operador agir
- CAUSA guardrails citarem Codex OAuth EFEITO avisos mortos sob OpenRouter
- CAUSA pool por threads de SO EFEITO desperdício em I/O de rede assíncrono
- CAUSA três concorrências fundidas EFEITO escrita serial mascara o teto de rede
- CAUSA singleton por namespace EFEITO multiprocesso bloqueado no mesmo banco
- CAUSA teto Cloudflare ignorado EFEITO vazão muito abaixo do disponível


## Solução
- Separar EXPLICITAMENTE três concorrências em todo caminho OpenRouter
- Concorrência de rede via `buffer_unordered(K)` sobre stream de lotes
- Concorrência de escrita serializada por canal `mpsc` a um único writer
- Abolir o caminho de subprocesso nas operações OpenRouter REST
- No embedding pipelinar os lotes de 32 com `buffer_unordered(K)`
- Trocar o `Semaphore` de subprocesso por concorrência de tarefas tokio
- No enrich substituir `std::thread::scope` por tarefas tokio assíncronas
- Aplicar `buffer_unordered(K)` sobre o stream de itens da fila
- Calibrar o teto de `K` para a faixa segura Cloudflare 4 a 16
- Remover ou reescrever os guardrails que citam Codex e OpenCode
- Expor `--rest-concurrency K` distinto de `--llm-parallelism` legado
- Manter o singleton por namespace para a escrita serializada
- Permitir multiprocesso APENAS entre namespaces distintos


## Benefícios da Solução
- A vazão de embedding aproxima-se de K vezes a serial atual
- Ingest de 100 markdowns sobrepõe rede de K lotes ao mesmo tempo
- Enrich de 3000 descrições drena perto de K vezes mais rápido
- A latência de rede fica amortizada pelo pipeline de tarefas
- Tarefas tokio substituem threads de SO pesadas por I/O leve
- O uso de CPU e RAM cai ao abolir fork de subprocesso
- O teto Cloudflare passa a ser efetivamente utilizado
- A escrita SQLite serial deixa de ser gargalo aparente
- A separação de concorrências respeita o real custo REST
- O reuso de `buffer_unordered` segue idioms async do ecossistema
- A restrição OpenRouter somente fica garantida por construção


## Como Solucionar
- Passo 1 transformar `embed_batch` num stream de lotes de 32
- Passo 1 aplicar `StreamExt::buffer_unordered(K)` sobre esse stream
- Passo 2 coletar os vetores preservando a ordem por índice de lote
- Passo 3 no enrich modelar itens da fila como stream de tarefas
- Passo 3 aplicar `buffer_unordered(K)` sobre o dispatch de chat REST
- Passo 4 rotear toda escrita por canal `mpsc` a um único writer task
- Passo 5 o writer task aplica as mutações SQLite de forma serial
- Passo 6 adicionar flag `--rest-concurrency K` com clamp 1 a 16
- Passo 7 calibrar o padrão de `K` em faixa segura Cloudflare
- Passo 8 remover guardrails de subprocesso mortos sob OpenRouter
- Passo 9 preservar singleton por namespace para isolar bancos
- Passo 10 documentar multiprocesso só entre namespaces distintos
- Passo 11 escrever testes de vazão e de ordem preservada
- Passo 11 escrever testes de serialização de escrita sob concorrência
- Validação build sem erros clippy sem warnings nextest verde
- Validação cobertura mínima de 80 por cento no código novo


## Evidências de Código
- `src/embedding_api.rs:17` define `MAX_BATCH_SIZE` igual a 32
- `src/embedding_api.rs:142` itera `chunks(MAX_BATCH_SIZE)` com `await` serial
- `src/embedder.rs:1139` faz um único `block_on(embed_batch)` por vez
- `src/embedder.rs:1397` clamp de permits mira subprocesso e CPU e RAM
- `src/commands/enrich.rs:494` declara `llm_parallelism` u32 padrão 1
- `src/commands/enrich.rs:2067` faz `llm_parallelism.clamp(1, 32)`
- `src/commands/enrich.rs:2086` avisa sobre Claude Code subprocess fan-out
- `src/commands/enrich.rs:2097` avisa sobre OAuth rate-limit no Codex
- `src/commands/enrich.rs:2112` avisa sobre OAuth rate-limit no OpenCode
- `src/commands/enrich.rs:2169` usa `std::thread::scope` para o pool
- `src/commands/enrich.rs:1839` adquire singleton de job por namespace


## Fontes Consultadas
- docs-rs `futures::stream::BufferUnordered` poll de N futuros concorrentes
- docs-rs `futures::stream::StreamExt` método `buffer_unordered`
- context7 `tokio` concorrência limitada por stream
- duckduckgo-search-cli buffer_unordered bounded concurrency REST calls
- duckduckgo-search-cli Tokio FuturesUnordered thundering herd problem
- GraphRAG memória 1170 rust-structural-analysis-rules async patterns
- GraphRAG memória 1431 mapa de concorrência OpenRouter embedding serial enrich workerpool
