# Gaps — sqlite-graphrag


## GAP-OR-ENTITY-EMBED: RESOLVIDO em v1.0.94 — Entity embedding em `remember`/`remember-batch`/`ingest` IGNORA os backends e força codex (timeout de 120s)

Resíduo do GAP-OR-PROPAGATION (marcado RESOLVIDO em v1.0.93, presente no git/CHANGELOG)
A propagação de `EmbeddingBackendChoice` cobriu o caminho de CHUNKS, mas DEIXOU DE FORA o caminho de ENTIDADES


### Problema
- O `remember`, o `remember-batch` e o `ingest` com entidades via `--graph-stdin` levam cerca de 120 segundos
- A lentidão ocorre MESMO passando `--llm-backend none` e `--embedding-backend openrouter`
- O embedding das ENTIDADES NÃO respeita a flag de backend escolhida pelo usuário
- O caminho de entidades usa codex headless em vez de OpenRouter REST
- O comando atinge o timeout interno de 120 segundos e degrada ou falha
- Evidência real medida: o `remember` registrou `elapsed_ms: 119468` (cerca de 119 segundos)
- O bug só DISPARA quando há entidades NOVAS (cache miss); sem entidades novas não há entity embedding


### Consequências do Problema
- Cada `remember` com entidades trava a sessão por cerca de 2 minutos
- O hook Stop de memória bloqueia o encerramento do turno por cerca de 2 minutos
- Memórias com grafo de entidades ficam inviáveis de salvar interativamente
- O usuário percebe que o `remember` faz TUDO JUNTO, e não em fases separadas
- A separação embedding via OpenRouter versus enrichment via codex é violada na prática
- Sob fadiga ou limite de OAuth do codex, o entity embedding pode FALHAR por completo
- Toda fórmula que coloca `--llm-backend codex` em `remember`/`remember-batch`/`ingest`/`edit`/`restore` está ERRADA
- O codex pertence EXCLUSIVAMENTE ao `enrich`, NUNCA ao caminho de escrita
- O bug torna o `--llm-backend none` inócuo no caminho de entidades de qualquer forma
- Corrigir a fórmula para `none` é NECESSÁRIO mas NÃO SUFICIENTE nos 3 comandos afetados


### Auditoria de Cobertura das 8 Operações
- A CLI tem DOIS roteadores de embedding com comportamento divergente
- O roteador de PASSAGE e QUERY respeita os backends via `embed_passage_with_embedding_choice`
- O roteador de ENTIDADES ignora os backends via `embed_entity_texts_cached` que chama `get_embedder`
- AFETADO: `remember` chama `embed_entity_texts_cached` em `src/commands/remember.rs:771`
- AFETADO: `remember-batch` chama `embed_entity_texts_cached` em `src/commands/remember_batch.rs:415`
- AFETADO: `ingest` chama `embed_entity_texts_cached` em `src/commands/ingest.rs:727`
- NÃO AFETADO: `restore` re-embeda só o body via `embed_passage_with_embedding_choice` em `restore.rs:173`
- NÃO AFETADO: `edit` re-embeda só o body via caminho correto em `edit.rs:222`, zero entity calls
- NÃO AFETADO: `recall` embeda a query via `try_embed_query_with_embedding_choice` em `recall.rs:179`
- NÃO AFETADO: `hybrid-search` embeda a query via caminho correto em `hybrid_search.rs:233`
- NÃO AFETADO: `deep-research` embeda a query via caminho correto em `deep_research.rs:312`
- NÃO AFETADO: `rename-entity` embeda via `embed_passage_with_embedding_choice` em `rename_entity.rs:99`
- A lista correta e final de afetados é EXATAMENTE: `remember`, `remember-batch`, `ingest`
- Correção do diagnóstico anterior: `edit` e `restore` foram suspeitados por engano e NÃO sofrem o bug
- Correção do diagnóstico anterior: `ingest` é afetado e NÃO constava no diagnóstico inicial
- Distinção importante: o BUG de entidade atinge 3 comandos; a regra de NÃO usar codex na escrita vale para os 5 de escrita


### Causa Raiz
- `src/commands/remember.rs:771` chama `embed_entity_texts_cached(&paths.models, &entity_texts, parallelism)`
- A chamada NÃO passa `embedding_backend` nem `llm_backend`
- `src/embedder.rs:1154` define `embed_entity_texts_cached(models_dir, texts, parallelism)` SEM parâmetros de backend
- A função invoca `get_embedder()` em `src/embedder.rs:1162`
- A função invoca `embed_texts_parallel()` em `src/embedder.rs:1181`
- `get_embedder()` executa `LlmEmbedding::detect_available()` e instancia o embedder LLM (codex)
- O entity embedding SEMPRE roteia para codex, ignorando a escolha do usuário
- `src/extract/llm_embedding.rs:43` define `DEFAULT_EMBED_TIMEOUT_SECS: u64 = 120`
- Entidades novas geram cache miss e esperam o codex até o timeout de 120 segundos
- O caminho de CHUNKS já respeita os backends via `embed_passage_with_embedding_choice`
- A chamada de chunks fica em `src/commands/remember.rs:653-708` e passa ambos os backends
- O caminho de PASSAGE roteia por `embedding_backend.to_chain(llm_backend)` em `src/embedder.rs:411`
- O caminho de ENTIDADES é o código ANTIGO de G56 (v1.0.80), nunca migrado na v1.0.93
- O teste `src/embedder.rs:2045 none_backend_returns_empty_vector_without_calling_llm` prova que o backend `none` pula o LLM
- O entity path NÃO usa esse mecanismo de `none`, então a flag não tem efeito ali


### Solução Proposta
- Propagar `embedding_backend` e `llm_backend` para o embedding de entidades
- Rotear o entity embedding pelo MESMO mecanismo do caminho de chunks
- Com `openrouter`, reusar `OpenRouterEmbeddingClient::embed_batch` que JÁ existe em `src/embedding_api.rs:131`
- O `embed_batch` JÁ envia `input` como array e aplica `dimensions` MRL em lote
- Com `none`, pular o embedding e gravar linha em `pending_embeddings`, como o chunk path já faz
- Eliminar a divergência entre o caminho de chunks e o de entidades, aplicando DRY
- Remover qualquer fórmula com `--llm-backend codex` em `remember`/`remember-batch`/`ingest`/`edit`/`restore`
- Manter `--llm-backend codex --llm-model gpt-5.4-mini` EXCLUSIVAMENTE no `enrich`


### Benefícios da Solução
- O `remember` com entidades cai de cerca de 120 segundos para centenas de milissegundos
- O `remember-batch` e o `ingest` herdam o mesmo ganho de tempo no entity embedding
- Os hooks de memória deixam de travar a sessão a cada turno
- A separação embedding via OpenRouter versus enrichment via codex passa a valer de fato
- O `--llm-backend none` volta a ter efeito no caminho de entidades
- O caminho de escrita deixa de depender do OAuth do codex
- Os dois caminhos de embedding ficam unificados, reduzindo a superfície de bug
- A CLI, os hooks e o CLAUDE.md ficam consistentes sobre operações separadas


### Como Solucionar
- Alterar a assinatura de `embed_entity_texts_cached` para receber `embedding_backend` e `llm_backend`
- Despachar internamente para o mecanismo de `embed_*_with_embedding_choice` já existente
- Reusar `OpenRouterEmbeddingClient::embed_batch` em `src/embedding_api.rs:131` para o lote REST de entidades
- Atualizar o chamador em `src/commands/remember.rs:771` para passar os dois backends
- Atualizar o chamador em `src/commands/remember_batch.rs:415` para passar os dois backends
- Atualizar o chamador em `src/commands/ingest.rs:727` para passar os dois backends
- NÃO tocar em `edit` nem `restore`: eles não re-embedam entidades e já estão corretos
- Honrar `--skip-embedding-on-failure` no novo caminho, como já ocorre no chunk path
- Consultar context7 sobre `reqwest` antes de tocar na chamada REST
- Adicionar teste objetivo: `remember --graph-stdin --llm-backend none` completa em menos de 2 segundos
- Adicionar teste de regressão: entity embedding com `none` grava `pending_embeddings` sem chamar LLM
- NUNCA reintroduzir codex no caminho de escrita


### Relações Causa x Efeito
- CAUSA: `embed_entity_texts_cached` não recebe `embedding_backend` nem `llm_backend`
- EFEITO: o entity embedding ignora a escolha de backend do usuário
- CAUSA: o entity embedding ignora a escolha e cai em `get_embedder()`
- EFEITO: `get_embedder()` instancia o embedder LLM codex
- CAUSA: o codex é invocado para embedar entidades novas em cache miss
- EFEITO: o comando espera o timeout interno de 120 segundos
- CAUSA: o timeout de 120 segundos é atingido a cada escrita com entidades novas
- EFEITO: o hook de memória trava a sessão por cerca de 2 minutos por turno
- CAUSA: a v1.0.93 migrou o chunk path mas não o entity path
- EFEITO: a separação embedding versus enrichment vale só para chunks
- CAUSA: o `--llm-backend none` não alcança o caminho de entidades
- EFEITO: a flag de separação fica inócua e o usuário observa tudo junto
- CAUSA: apenas `remember`, `remember-batch` e `ingest` chamam o entity path
- EFEITO: somente esses 3 comandos sofrem o timeout, os outros 5 não


### Arquivos Afetados
- `src/embedder.rs:1154` — `embed_entity_texts_cached` com assinatura sem backends
- `src/embedder.rs:1162` — `get_embedder` invocado dentro do entity path
- `src/embedder.rs:1181` — `embed_texts_parallel` que opera sobre LlmEmbedding codex
- `src/embedder.rs:141` — `get_embedder` que instancia LlmEmbedding via `detect_available`
- `src/commands/remember.rs:771` — chamador afetado que não passa os backends
- `src/commands/remember_batch.rs:415` — chamador afetado que não passa os backends
- `src/commands/ingest.rs:727` — chamador afetado que não passa os backends
- `src/extract/llm_embedding.rs:43` — `DEFAULT_EMBED_TIMEOUT_SECS = 120`
- `src/embedding_api.rs:131` — `embed_batch` REST OpenRouter a ser reusado
- `src/commands/edit.rs:222` — NÃO afetado, re-embeda só o body pelo caminho correto
- `src/commands/restore.rs:173` — NÃO afetado, re-embeda só o body pelo caminho correto


### Evidências e Verificação
- Evidência de execução: o `remember` retornou `elapsed_ms: 119468`
- Evidência de código: a assinatura de `embed_entity_texts_cached` não tem parâmetros de backend
- Evidência de auditoria: só 3 dos 8 comandos chamam `embed_entity_texts_cached`
- Evidência de auditoria: `edit` e `restore` têm zero chamadas ao entity path
- Evidência de reuso: `embed_batch` em `src/embedding_api.rs:131` já faz lote REST com `input` array e `dimensions`
- Evidência de documentação: a OpenRouter Embeddings API aceita `input` em array e `dimensions`
- Confiança alta porque a análise se baseia em fatos do código, não em inferência
- Limite declarado: o bug não foi reproduzido de propósito para evitar o timeout de 120 segundos


### Pesquisa de Referência
- context7 `/seanmonstar/reqwest` trustScore 9.7 confirma POST JSON com bearer para REST
- duckduckgo confirma OpenRouter Embeddings API com `input` array e `dimensions` MRL
- achado de reuso: `OpenRouterEmbeddingClient::embed_batch` já implementa o lote REST necessário
- rules rust graphrag `rules-rust-cli-one-shot` exigem CLI que nasce, executa e morre, sem daemon
- rules graphrag DRY e KISS orientam reusar o chunk path e eliminar o caminho duplicado
- rules graphrag tratam erro como contrato de tipo, modelando o backend no sistema de tipos


## GAP-EMBED-TIMEOUT-300: RESOLVIDO em v1.0.94 — Timeout de embedding LLM é de 120s, curto e inconsistente; subir para 300s

O `DEFAULT_EMBED_TIMEOUT_SECS` ficou para trás quando ingest, enrich e opencode adotaram 300s
O embedding LLM é o ÚNICO subprocesso headless que ainda usa 120s na CLI inteira


### Problema
- O timeout por chamada de embedding LLM é de apenas 120 segundos
- Esse valor é o ÚNICO subprocesso LLM da CLI que não usa 300 segundos
- O `ingest`, o `enrich`, o `opencode` e o `llm_backend` já usam 300 segundos
- O comentário do código afirma ser consistente com os defaults do `ingest`
- O comentário está factualmente ERRADO porque o `ingest` usa 300 e não 120
- O codex headless tem cold start e latência variável por chamada
- Lotes de entidades novas geram mais vetores e estouram os 120 segundos
- O usuário pediu subir o default de 120 para 300 segundos


### Consequências do Problema
- Embeddings de lotes grandes abortam por timeout antes de terminar
- O entity embedding de `remember`, `remember-batch` e `ingest` falha mais cedo
- A operação retorna exit 11 de falha de embedding sob lote pesado
- O cold start do codex consome parte dos 120 segundos sem produzir vetor
- A inconsistência de 120 versus 300 confunde quem lê o código
- O comentário desatualizado induz o leitor a um pressuposto falso
- Sob carga, o caminho de escrita degrada de forma evitável
- O limite curto interage com o GAP-OR-ENTITY-EMBED e agrava a lentidão


### Causa Raiz
- `src/extract/llm_embedding.rs:43` define `DEFAULT_EMBED_TIMEOUT_SECS: u64 = 120`
- O comentário em `src/extract/llm_embedding.rs:40-42` diz ser consistente com o `ingest`
- O `ingest` define `DEFAULT_TIMEOUT: u64 = 300` em `src/commands/ingest.rs:967`
- O `enrich` define `DEFAULT_TIMEOUT: u64 = 300` em `src/commands/enrich.rs:1618`
- O `opencode_runner` define `DEFAULT_OPENCODE_TIMEOUT_SECS: u64 = 300` em `opencode_runner.rs:13`
- O `llm_backend` define `timeout_secs: Some(300)` em `src/extract/llm_backend.rs:40` e `:71`
- A constante de embedding nunca foi migrada quando os demais subiram para 300
- Houve um drift histórico que deixou só o embedding em 120 segundos
- A função `embed_timeout()` em `llm_embedding.rs:45` lê o default 120 como fallback
- A precedência é `timeout_override` da instância depois env var depois o default
- O override de instância existe via `LlmEmbeddingBuilder::override_timeout` em `:190`
- O env var `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` existe mas o default é a causa
- O clamp aceita o intervalo de 10 a 3600 segundos, então 300 é válido


### Solução Proposta
- Alterar `DEFAULT_EMBED_TIMEOUT_SECS` de 120 para 300 em `llm_embedding.rs:43`
- Corrigir o comentário desatualizado para refletir o alinhamento real com 300
- Manter o env var `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` como override do default
- Manter o clamp de 10 a 3600 segundos que já comporta 300 sem mudança
- Manter o escalonamento por lote de 15 segundos por item adicional intacto
- NÃO criar flag nova porque o mecanismo de override já existe e basta


### Benefícios da Solução
- Lotes pesados de entidades passam a caber dentro do limite de tempo
- O embedding LLM deixa de abortar cedo sob cold start do codex
- A CLI fica consistente com 300 segundos em todos os subprocessos LLM
- O comentário do código passa a descrever a realidade do `ingest`
- A confusão entre 120 e 300 some do código e da leitura
- O caminho de escrita degrada menos sob carga real de produção
- A correção é mínima e cirúrgica em uma única constante


### Como Solucionar
- Trocar o literal 120 por 300 em `src/extract/llm_embedding.rs:43`
- Reescrever o comentário das linhas 40 a 42 para citar 300 corretamente
- Atualizar o teste `assert_eq!(DEFAULT_EMBED_TIMEOUT_SECS, 120)` em `:1244` para 300
- Revisar o teste de escalonamento por lote em `:1516` que depende da base
- Validar com build, clippy, fmt e a suíte de testes sem falhas
- Confirmar que o clamp de 10 a 3600 segue aceitando o novo default 300
- Consultar context7 sobre `tokio::time::timeout` antes de tocar no consumo
- Registrar a mudança no CHANGELOG do projeto na próxima release


### Relações Causa x Efeito
- CAUSA: o default de embedding ficou em 120 quando o resto subiu para 300
- EFEITO: o embedding LLM é o único subprocesso com limite mais curto
- CAUSA: o codex headless tem cold start e latência variável por chamada
- EFEITO: parte dos 120 segundos é gasta antes de gerar qualquer vetor
- CAUSA: lotes de entidades novas geram muitos vetores em uma chamada
- EFEITO: a chamada estoura os 120 segundos e aborta com exit 11
- CAUSA: o comentário cita o `ingest` mas o `ingest` usa 300, não 120
- EFEITO: o leitor do código assume consistência que não existe
- CAUSA: o limite curto soma com o GAP-OR-ENTITY-EMBED que força codex
- EFEITO: a escrita com entidades novas fica lenta e frágil ao mesmo tempo


### Arquivos Afetados
- `src/extract/llm_embedding.rs:43` — `DEFAULT_EMBED_TIMEOUT_SECS = 120` a virar 300
- `src/extract/llm_embedding.rs:40-42` — comentário desatualizado que cita o `ingest`
- `src/extract/llm_embedding.rs:45` — `embed_timeout()` que usa o default como fallback
- `src/extract/llm_embedding.rs:409-414` — `instance_embed_timeout` com a precedência de 3 níveis
- `src/extract/llm_embedding.rs:776` — spawn single-call que aplica o timeout
- `src/extract/llm_embedding.rs:920` — spawn batch via stdin que aplica o timeout
- `src/extract/llm_embedding.rs:1002` — spawn batch via arg que aplica o timeout
- `src/extract/llm_embedding.rs:1244` — teste que afirma o default igual a 120
- `src/extract/llm_embedding.rs:1516` — teste de escalonamento por lote sobre a base
- `src/commands/ingest.rs:967` — referência de 300 que prova a inconsistência
- `src/commands/enrich.rs:1618` — referência de 300 que prova a inconsistência
- `src/commands/opencode_runner.rs:13` — referência de 300 que prova a inconsistência
- `src/extract/llm_backend.rs:40` — referência de 300 que prova a inconsistência


### Operações da CLI Que Usam Esse Timeout
- O timeout governa TODO embedding roteado para LLM headless codex, claude ou opencode
- AFETADO: `remember` dispara entity embedding LLM e usa o timeout de 120
- AFETADO: `remember-batch` dispara entity embedding LLM e usa o timeout de 120
- AFETADO: `ingest` dispara entity embedding LLM e usa o timeout de 120
- CONDICIONAL: `recall` usa o timeout só se `--llm-backend` for codex, claude ou opencode
- CONDICIONAL: `hybrid-search` usa o timeout só com backend LLM no embedding da query
- CONDICIONAL: `deep-research` usa o timeout só com backend LLM no embedding da query
- CONDICIONAL: `edit --body` e `restore` usam o timeout só com backend LLM no re-embed
- NÃO AFETADO com openrouter: o caminho REST usa o timeout de 30s em `embedding_api.rs:15`
- Distinção: o timeout de 120 vale para o subprocesso LLM, não para o REST OpenRouter


### Evidências e Verificação
- Evidência de código: só `llm_embedding.rs:43` usa 120 entre todos os subprocessos LLM
- Evidência de inconsistência: `ingest`, `enrich`, `opencode` e `llm_backend` usam 300
- Evidência de comentário falso: o texto cita o `ingest` que na verdade usa 300
- Evidência de mecanismo: `tokio::time::timeout` envolve `cmd.output()` em 3 pontos de spawn
- Evidência de override pronto: builder, env var e clamp de 10 a 3600 já existem
- Confiança alta porque a auditoria varreu todas as constantes de timeout do `src`
- Limite declarado: o estouro real de lote não foi reproduzido para evitar o timeout


### Pesquisa de Referência
- context7 `/websites/rs_tokio` trustScore 9.7 confirma `tokio::time::timeout` com `Elapsed`
- duckduckgo confirma que `codex exec` headless tem cold start e latência variável
- GraphRAG `headless-comparacao-patterns` mostra os patterns headless usando `timeout 300`
- rules rust graphrag exigem evidência de código e validação completa antes de fechar
- rules graphrag DRY orientam reusar o override existente sem criar flag nova


## GAP-HEADLESS-DEFAULT: RESOLVIDO em v1.0.94 — CLI headless definida como padrão; `enrich --mode` tem default `claude-code` que spawna `claude -p`

A flag `--mode` do `enrich` escolhe a CLI headless de extração sozinha, sem o usuário pedir
O default `claude-code` spawna `claude -p`, que herda o `.mcp.json` do CWD e falha


### Problema
- O `enrich` tem DOIS seletores de backend independentes e separados
- O `--llm-backend` controla o embedding de entidades do `enrich`
- O `--mode` controla a extração de entidades e relações do `enrich`
- A flag `--mode` tem default embutido igual a `claude-code`
- Omitir `--mode` faz o `enrich` spawnar `claude -p` por conta própria
- Passar `--llm-backend codex` NÃO altera o `--mode`, que segue em `claude-code`
- O usuário não escolhe a CLI headless, mas ela já vem escolhida
- A política exigida é proibir CLI headless definida como padrão
- O usuário deve informar a CLI headless e o modelo em TODO comando


### Consequências do Problema
- O `enrich --operation memory-bindings` falhou em 58 de 58 itens
- 51 itens falharam com `claude -p exited with code Some(1)`
- 7 itens falharam com `Argument list too long (os error 7)`
- O subprocesso `claude -p` herda o CWD com `.mcp.json` do projeto
- O `.mcp.json` herdado quebra o `claude -p` mesmo com preflight desligado
- Corpos de memória maiores que 150 KB estouram o ARG_MAX do Linux
- A fase `validate` mostrou `binary_path: claude` mesmo passando `--llm-backend codex`
- O usuário acredita usar codex, mas o `enrich` usa claude por baixo
- O enrichment pós-escrita não gera bindings para nenhuma memória do namespace


### Causa Raiz
- `src/commands/enrich.rs:379` declara `#[arg(long, value_enum, default_value = "claude-code")]`
- O campo `mode: EnrichMode` em `enrich.rs:380` recebe esse default headless
- O default faz o clap preencher `--mode` com `claude-code` quando ausente
- O enum `EnrichMode` em `enrich.rs:330-339` tem `ClaudeCode`, `Codex` e `Opencode`
- O enum NÃO tem variante neutra que force escolha explícita do usuário
- O comentário em `enrich.rs:378` declara o default como `claude-code (OAuth-first)`
- O exemplo em `enrich.rs:357` ensina `--mode claude-code` como uso padrão
- O `claude-code` resolve o binário `claude` e o invoca como `claude -p`
- O `claude -p` é um subprocesso que herda o CWD e o `.mcp.json` do projeto
- O `--llm-backend` e o `--mode` são caminhos de código distintos e independentes
- Mudar `--llm-backend` para codex não troca o provider de extração do `--mode`
- O default headless mascara a escolha real e induz erro de uso silencioso


### Solução Proposta
- Remover o `default_value = "claude-code"` da flag `--mode` em `enrich.rs:379`
- Tornar a flag `--mode` OBRIGATÓRIA, sem default embutido
- Forçar o usuário a informar a CLI headless e o modelo em todo comando
- Aplicar a mesma política a qualquer flag que selecione CLI headless
- Reavaliar o `--llm-backend` com default `auto` sob a mesma regra
- Seguir o padrão do campo `operation` que já é obrigatório sem default
- NÃO criar nova variante de enum porque a obrigatoriedade do clap basta


### Benefícios da Solução
- O `enrich` deixa de spawnar `claude -p` por conta própria
- O usuário escolhe codex e modelo de forma explícita e consciente
- O clap rejeita o comando sem `--mode` com erro claro e fail-fast
- O erro de uso aparece antes da execução, não após 58 falhas em runtime
- A herança do `.mcp.json` pelo `claude -p` deixa de ocorrer por acidente
- A CLI passa a refletir a política de provider e modelo sempre explícitos
- A confusão entre `--llm-backend` e `--mode` perde o efeito silencioso


### Como Solucionar
- Trocar `#[arg(long, value_enum, default_value = "claude-code")]` por `#[arg(long, value_enum)]` em `enrich.rs:379`
- Manter o tipo `mode: EnrichMode` como campo obrigatório, não `Option`
- Corrigir o comentário em `enrich.rs:378` para remover a menção a default
- Atualizar o exemplo em `enrich.rs:357` para `--mode codex --codex-model gpt-5.4-mini`
- Revisar o log em `enrich.rs:1946` que assume `mode = "claude-code"`
- Avaliar se o `--llm-backend` default `auto` deve virar obrigatório também
- Atualizar testes que dependem do default `claude-code` do `--mode`
- Consultar context7 sobre clap antes de tocar no atributo do argumento
- Validar com build, clippy, fmt e a suíte de testes sem falhas
- Registrar a mudança no CHANGELOG do projeto na próxima release


### Relações Causa x Efeito
- CAUSA: a flag `--mode` tem default `claude-code` embutido
- EFEITO: omitir `--mode` spawna `claude -p` sem o usuário pedir
- CAUSA: o `claude -p` herda o CWD com o `.mcp.json` do projeto
- EFEITO: 51 itens falham com `claude -p exited with code Some(1)`
- CAUSA: corpos grandes são passados como argumento ao `claude -p`
- EFEITO: 7 itens falham com `Argument list too long (os error 7)`
- CAUSA: `--llm-backend` e `--mode` são seletores independentes
- EFEITO: passar `--llm-backend codex` não troca o provider de extração
- CAUSA: o default headless mascara a escolha real do provider
- EFEITO: o usuário pensa usar codex, mas o `enrich` usa claude
- CAUSA: o default só falha em runtime, item a item
- EFEITO: o erro aparece tarde, após 58 de 58 falhas


### Arquivos Afetados
- `src/commands/enrich.rs:379` — `default_value = "claude-code"` a ser removido
- `src/commands/enrich.rs:380` — campo `mode: EnrichMode` que recebe o default
- `src/commands/enrich.rs:378` — comentário que declara o default headless
- `src/commands/enrich.rs:330-339` — enum `EnrichMode` sem variante neutra
- `src/commands/enrich.rs:357` — exemplo que ensina `--mode claude-code`
- `src/commands/enrich.rs:375` — campo `operation` obrigatório que serve de modelo correto
- `src/commands/enrich.rs:1946` — log que assume `mode = "claude-code"`
- `src/commands/enrich.rs:1615-1632` — validação de conflito de flags por `--mode`
- `src/commands/ingest.rs:222` — NÃO afetado, default é `IngestMode::None` local sem LLM


### Operações da CLI Afetadas
- AFETADO: `enrich --operation memory-bindings` usa o `--mode` default headless
- AFETADO: `enrich --operation entity-descriptions` usa o `--mode` default headless
- AFETADO: `enrich --operation body-enrich` usa o `--mode` default headless
- CONDICIONAL: `enrich --operation re-embed` usa o `--mode` só na parte de extração LLM
- NÃO AFETADO: `ingest --mode` tem default `IngestMode::None`, local e sem LLM
- Distinção: o default headless é exclusivo do `enrich`, não do `ingest`
- Distinção: o `--llm-backend` global é seletor separado do `--mode` do `enrich`


### Evidências e Verificação
- Evidência de código: `enrich.rs:379` é o único arg de `--mode` com default headless
- Evidência empírica: 58 de 58 itens falharam com o `--mode` default `claude-code`
- Evidência de erro 1: 51 itens com `claude -p exited with code Some(1)`
- Evidência de erro 2: 7 itens com `Argument list too long (os error 7)`
- Evidência de correção: `--mode codex --codex-model gpt-5.4-mini` resolveu `binary_path` para codex
- Evidência de padrão: o campo `operation` sem default já é obrigatório no clap
- Confiança alta porque a falha foi observada e a correção foi verificada na sessão
- Limite declarado: a correção do código não foi aplicada, apenas documentada


### Pesquisa de Referência
- context7 `/websites/rs_clap` trustScore 9.7 confirma argumento obrigatório sem `default_value`
- duckduckgo confirma `claude -p` headless e herança de configuração via CWD do projeto
- duckduckgo retornou a doc oficial de Claude Code headless e a issue de `.mcp.json` por CWD
- GraphRAG não tem memória duplicada deste gap, scores de hybrid-search abaixo de 0,02
- rules rust graphrag exigem evidência de código e validação completa antes de fechar
- rules graphrag tratam erro de uso como contrato, preferindo fail-fast no parser


## GAP-EMBED-DIM-64: RESOLVIDO em v1.0.94 — Default de embedding é 64; DEVE ser 384 para criar banco, embedar e operar

O `DEFAULT_EMBEDDING_DIM` é 64, incompatível com o corpus de produção indexado em 384
O cliente OpenRouter congela o dim no startup com `unwrap_or(64)`, antes de abrir o banco


### Problema
- O default de dimensionalidade de embedding da CLI é 64
- O banco de produção `graphrag.sqlite` está indexado em 384 dimensões
- Toda operação SEM `--embedding-dim 384` gera vetor de 64 dimensões
- O vetor de 64 colide com os vetores de 384 já gravados no banco
- A busca knn aborta com exit 11 informando 64 dims, esperava 384
- A flag `--embedding-dim 384` virou obrigatória em TODO comando de embedding
- A env var `SQLITE_GRAPHRAG_EMBEDDING_DIM` NÃO cobre o caminho OpenRouter
- A política exigida é o default ser 384 para banco, embedding e operações


### Consequências do Problema
- Cada `recall`, `hybrid-search` e `deep-research` falha sem a flag explícita
- Cada `remember`, `remember-batch` e `ingest` grava vetor de 64 sem a flag
- Um banco novo via `init` sem a flag nasce com dim 64 gravado
- Esse banco fica incompatível com qualquer corpus 384 de produção
- O usuário precisa repetir `--embedding-dim 384` em toda invocação
- Esquecer a flag uma vez corrompe a consistência dimensional do índice
- A env var sozinha não conserta porque o caminho OpenRouter a ignora
- Os hooks e fórmulas tiveram que cravar a flag manualmente para não quebrar
- A confusão entre env e flag induz erro de uso silencioso e tardio


### Causa Raiz
- `src/constants.rs:28` define `DEFAULT_EMBEDDING_DIM: usize = 64`
- O comentário em `constants.rs:22-27` rebaixou de 384 para 64 na v1.0.79
- O motivo declarado foi custo de tokens autoregressivos no backend LLM-only
- `src/constants.rs:43` resolve a precedência env, depois banco, depois default 64
- `src/main.rs:371` inicializa o cliente OpenRouter com `cli.embedding_dim.unwrap_or(64)`
- Esse `unwrap_or(64)` é um literal cravado que NÃO chama `constants::embedding_dim()`
- O bloco `main.rs:348-380` roda no startup global, antes do dispatch do subcomando
- A conexão do banco que popula o dim ativo só abre DEPOIS, no subcomando
- O cliente OpenRouter já nasceu fixo em 64 antes de o banco informar 384
- `src/embedder.rs:208` passa o dim a `OpenRouterClient::new(api_key, model, dim)`
- `src/embedding_api.rs:92` congela `self.dim` na construção do cliente
- `src/embedding_api.rs:110-111` envia `dimensions: Some(self.dim)` na requisição REST
- O OpenRouter aplica truncamento MRL ao dim pedido e devolve vetor de 64
- `src/embedding_api.rs:177-187` valida e trunca o vetor retornado contra `self.dim`
- A env var só é lida por `constants::embedding_dim()`, NÃO pelo init eager OpenRouter
- A flag preenche `cli.embedding_dim` E vira env em `main.rs:188`, cobrindo os dois caminhos
- `src/commands/init.rs:108-109` grava o dim ativo no `schema_meta` ao criar o banco
- Sem a flag, `init` grava 64 e estampa o banco novo com dimensionalidade errada
- `src/commands/init.rs:230-235` tem teste que trava o default em 64


### Solução Proposta
- Elevar `DEFAULT_EMBEDDING_DIM` de 64 para 384 em `constants.rs:28`
- Trocar o `unwrap_or(64)` por `constants::embedding_dim()` em `main.rs:371`
- Fazer o init eager do OpenRouter consultar a precedência env, banco e default
- Garantir que o cliente OpenRouter herde o dim do banco aberto quando existir
- Manter a flag `--embedding-dim` como override consciente para migração de corpus
- Manter o clamp de 8 a 4096 que já comporta 384 sem mudança
- Atualizar o teste de init que afirma o default 64 para 384
- NÃO criar flag nova porque o mecanismo de precedência já existe


### Benefícios da Solução
- O `recall`, o `hybrid-search` e o `deep-research` funcionam sem flag manual
- O `remember`, o `remember-batch` e o `ingest` gravam vetor de 384 por default
- Um banco novo via `init` nasce em 384, compatível com produção
- A env var e o banco passam a alimentar também o cliente OpenRouter
- O usuário deixa de repetir `--embedding-dim 384` em todo comando
- O erro de mismatch exit 11 some do uso padrão da CLI
- A consistência dimensional do índice fica protegida por default seguro
- A flag passa a ser exceção de migração, não obrigação diária


### Como Solucionar
- Trocar `pub const DEFAULT_EMBEDDING_DIM: usize = 64;` por `= 384;` em `constants.rs:28`
- Reescrever o comentário `constants.rs:22-27` para refletir o default 384
- Trocar `cli.embedding_dim.unwrap_or(64)` por `crate::constants::embedding_dim()` em `main.rs:371`
- Avaliar reordenar o init OpenRouter para após a abertura do banco
- Atualizar `assert` do default em `src/commands/init.rs:230-235` para 384
- Auditar testes que dependem do default 64 em `constants.rs` e `init.rs`
- Confirmar que o clamp de 8 a 4096 segue aceitando 384
- Consultar context7 sobre clap antes de tocar no parser do argumento
- Validar com build, clippy, fmt e a suíte de testes sem falhas
- Registrar a mudança e o impacto de re-embed no CHANGELOG da release


### Relações Causa x Efeito
- CAUSA: `DEFAULT_EMBEDDING_DIM` é 64 e o corpus é 384
- EFEITO: vetor gerado sem flag colide com o índice e dá exit 11
- CAUSA: `main.rs:371` usa `unwrap_or(64)` cravado, não a precedência
- EFEITO: o cliente OpenRouter nasce em 64 ignorando o banco
- CAUSA: o init do OpenRouter é eager no startup, antes da conexão
- EFEITO: o dim do banco 384 chega tarde demais para o cliente
- CAUSA: a env var só é lida por `constants::embedding_dim()`
- EFEITO: a env sozinha não corrige o caminho OpenRouter
- CAUSA: a flag preenche `cli.embedding_dim` e também vira env
- EFEITO: só a flag cobre os dois caminhos e por isso virou obrigatória
- CAUSA: `init` grava o dim ativo no `schema_meta` sem flag
- EFEITO: banco novo nasce em 64 e fica incompatível com produção


### Arquivos Afetados
- `src/constants.rs:28` — `DEFAULT_EMBEDDING_DIM = 64` a virar 384
- `src/constants.rs:22-27` — comentário que justifica o rebaixamento a 64
- `src/constants.rs:43-51` — `embedding_dim()` com precedência env, banco, default
- `src/main.rs:371` — `cli.embedding_dim.unwrap_or(64)` cravado no init OpenRouter
- `src/main.rs:185-188` — flag global que materializa a env var de dim
- `src/cli.rs:227-235` — flag `embedding_dim: Option<u64>` documentada como default 64
- `src/embedder.rs:208` — `get_openrouter_embedder` que repassa o dim ao cliente
- `src/embedding_api.rs:77-92` — `OpenRouterClient::new` que congela `self.dim`
- `src/embedding_api.rs:110-111` — envio de `dimensions: Some(self.dim)` na requisição
- `src/embedding_api.rs:177-187` — validação e truncamento contra `self.dim`
- `src/commands/init.rs:108-109` — grava o dim ativo no `schema_meta` do banco novo
- `src/commands/init.rs:230-235` — teste que trava o default em 64
- `src/storage/connection.rs` — popula o dim ativo lendo o `schema_meta` ao abrir


### Operações da CLI Afetadas
- AFETADO: `init` estampa o banco novo com o default 64 sem flag
- AFETADO: `remember` e `remember-batch` geram vetor de 64 sem flag
- AFETADO: `ingest` grava chunks e entidades em 64 sem flag
- AFETADO: `recall`, `hybrid-search` e `deep-research` embedam a query em 64
- AFETADO: `edit --body` e `restore` re-embedam o body em 64 sem flag
- AFETADO: `rename-entity` re-embeda a entidade em 64 sem flag
- AFETADO: `enrich --operation re-embed` regenera vetores no dim ativo
- Distinção: o caminho OpenRouter só respeita a FLAG, não a env isolada
- Distinção: o caminho LLM local respeita a env via `constants::embedding_dim()`


### Evidências e Verificação
- Evidência de código: `constants.rs:28` define o default 64 explicitamente
- Evidência de código: `main.rs:371` usa `unwrap_or(64)` cravado no init OpenRouter
- Evidência de ordem: o bloco `main.rs:348-380` roda no startup antes do subcomando
- Evidência de congelamento: `embedding_api.rs:92` fixa `self.dim` na construção
- Evidência de requisição: `embedding_api.rs:110-111` envia `dimensions: Some(self.dim)`
- Evidência empírica: o banco real responde exit 11 com 64 dims, esperava 384
- Evidência de init: `init.rs:108-109` grava o dim ativo no `schema_meta`
- Confiança alta porque a auditoria varreu constants, main, embedder e init
- Limite declarado: a correção do código não foi aplicada, apenas documentada


### Pesquisa de Referência
- context7 `/clap-rs/clap` trustScore 7.1 confirma flag opcional com override de default
- duckduckgo confirma MRL em arXiv 2205.13147, base do truncamento de dimensões
- duckduckgo mostra que truncar dimensões reduz precisão semântica do embedding
- referência histórica: 384 casava o modelo `multilingual-e5-small` do corpus
- GraphRAG `skill-embedding-dim-384-flag-obrigatoria-2026-06-26` registra a flag obrigatória
- rules rust graphrag exigem evidência de código e validação completa antes de fechar
- rules graphrag tratam default inseguro como contrato quebrado e preferem default correto
