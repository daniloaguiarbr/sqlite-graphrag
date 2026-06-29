# Gaps da CLI sqlite-graphrag — Análise de Causa Raiz


## Objetivo, Fontes e Convenção
- Este documento cataloga os gaps da CLI `sqlite-graphrag` observados em produção
- O objetivo é APENAS documentar, NUNCA corrigir a CLI nesta entrega
- A versão de referência é `sqlite-graphrag` 1.0.96 confirmada em crates.io e docs.rs
- Cada gap segue o padrão `GAP-SG-NN` e os eixos PROBLEMA, CONSEQUÊNCIAS, CAUSA RAIZ, RELAÇÃO CAUSA, SOLUÇÃO, BENEFÍCIOS, COMO SOLUCIONAR e STATUS
- O campo STATUS cruza cada gap com o `CHANGELOG.md` para distinguir aberto de resolvido
- A fonte primária é a execução real do ingest e do enrichment de 92 arquivos rules-rust
- As fontes externas foram consultadas via context7, duckduckgo-search-cli e mcp docs-rs
- O ledger bruto de evidências vive em `erros_graphrag.md` na raiz do projeto


## Correções de Premissa contra o CHANGELOG
- Esta seção corrige premissas do backlog que o CHANGELOG refuta
- CORREÇÃO 1: o backlog afirma que o enrich NÃO usa structured outputs e isso é FALSO
- O CHANGELOG linha 33 confirma `response_format` `json_schema` `strict:true` mais `provider.require_parameters:true` JÁ enviados
- CORREÇÃO 2: o backlog afirma timeout REST fixo sem flag e isso está DESATUALIZADO
- O CHANGELOG linha 32 confirma a flag `--openrouter-timeout` JÁ existente na CLI
- CORREÇÃO 3: o backlog trata a disciplina de dead-letter como ausente e isso está DESATUALIZADO
- O CHANGELOG linhas 11 a 17 confirmam `error_class`, `next_retry_at`, status `dead`, `--until-empty`, `--max-attempts` e `--status` JÁ existentes
- A feature responsável é `GAP-ENRICH-BACKLOG-CONVERGE` registrada em v1.0.96
- Essas correções viram notas de STATUS nos gaps afetados, não gaps abertos


## Causa Raiz Unificadora — GAP-SG-01 Camada HTTP ingênua do cliente OpenRouter
- PROBLEMA: a resiliência da CLI na camada OpenRouter é orientada a STATUS e confia no provider
- PROBLEMA: ela não inspeciona o corpo da resposta nem protege contra schema ignorado
- CONSEQUÊNCIAS: erro de embedding com mensagem enganosa e enrich em dead-letter terminal
- CONSEQUÊNCIAS: corpos densos falham e itens válidos morrem na primeira tentativa
- CAUSA RAIZ: o cliente confia que erro chega como STATUS HTTP e que o provider honra o schema
- CAUSA RAIZ: quando o provider devolve 200 com erro ou non-JSON, não há rede de segurança client-side
- RELAÇÃO CAUSA -> EFEITO: estouro de tokens causa 200 com `{error}` que causa parse `missing field data`
- RELAÇÃO CAUSA -> EFEITO: non-JSON do provider causa HardFailure que causa `dead` irreversível
- SOLUÇÃO: ramificar a resposta entre `{data}` e `{error}` e adicionar reparo e revival client-side
- BENEFÍCIOS: erro acionável, queda drástica de dead-letter e corpos grandes previsíveis
- COMO SOLUCIONAR: usar `match` sobre um enum de resposta com `serde` em vez de parse otimista
- COMO SOLUCIONAR: integrar reparo `llm_json` ou `jsonrepair` antes do parse estrito
- COMO SOLUCIONAR: medir tokens antes do request e expor revival de `dead`
- STATUS: resolvido na v1.0.97, commit aaeebcc (Fase A) — raiz HTTP unificadora resolvida; cascata GAP-SG-02..17 endereçada nas Fases B-G


## Gaps de Embedding e Limites de Corpo
- GAP-SG-02 Limite de embedding em bytes ignora restrição real em tokens
- PROBLEMA: o guard de corpo é medido em bytes mas a restrição real é em tokens
- CONSEQUÊNCIAS: corpos de cerca de 113 a 127 KB estouram o contexto e falham o embedding
- CAUSA RAIZ: o `qwen/qwen3-embedding-8b` aceita cerca de 32K tokens e o limite nominal é 512000 bytes
- RELAÇÃO CAUSA -> EFEITO: unidade de medida errada causa guard inócuo que causa estouro silencioso
- SOLUÇÃO: estimar tokens do corpo antes do request e validar contra o limite real do modelo
- BENEFÍCIOS: rejeição previsível e mensagem correta antes da chamada de rede
- COMO SOLUCIONAR: contar tokens com um tokenizer compatível e rejeitar acima do teto efetivo
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase B) — guard EMBEDDING_REQUEST_MAX_TOKENS=30000 antes do request via count_tokens

- GAP-SG-03 Mensagem missing field data enganosa esconde estouro de tokens
- PROBLEMA: o erro `missing field data` aparece quando o corpo excede o contexto do modelo
- CONSEQUÊNCIAS: o operador interpreta erro de schema interno em vez de excesso de tokens
- CAUSA RAIZ: o OpenRouter responde 200 com `{error}` e a CLI parseia como resposta de sucesso
- RELAÇÃO CAUSA -> EFEITO: 200 com erro causa parse de `data` ausente que causa mensagem enganosa
- SOLUÇÃO: detectar o envelope `{error}` e propagar `code` e `message` reais ao chamador
- BENEFÍCIOS: diagnóstico imediato da causa verdadeira sem tentativa e erro
- COMO SOLUCIONAR: checar a presença de `error` antes de desserializar `data`
- STATUS: resolvido na v1.0.97, commit aaeebcc (Fase A) — envelope {error} detectado, code e message reais propagados

- GAP-SG-04 Sem chunking nem validação de tamanho antes do request
- PROBLEMA: a CLI não valida o tamanho do corpo nem fragmenta antes de enviar o embedding
- CONSEQUÊNCIAS: oito arquivos grandes ficaram sem vetor e exigiram divisão manual
- CAUSA RAIZ: o caminho de embedding envia o corpo inteiro sem checagem prévia de capacidade
- RELAÇÃO CAUSA -> EFEITO: corpo grande causa request inválido que causa falha tardia de rede
- SOLUÇÃO: validar o tamanho e oferecer fragmentação automática abaixo do limite efetivo
- BENEFÍCIOS: ingestão de qualquer tamanho sem perda de conteúdo
- COMO SOLUCIONAR: cortar em fronteira de seção markdown com folga abaixo de 127 KB
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase B) — assess_body_budget + auto-split nativo lossless por seção markdown

- GAP-SG-05 Limite de 512 chunks mais restritivo que bytes e não previsível
- PROBLEMA: documentos acima de 512 chunks falham mesmo abaixo de 512000 bytes
- CONSEQUÊNCIAS: uma parte de 229968 bytes gerou 515 chunks e falhou por três unidades
- CAUSA RAIZ: o limite de chunks é mais apertado que o de bytes e não deriva do tamanho do arquivo
- RELAÇÃO CAUSA -> EFEITO: corpo denso causa muitos chunks que causam rejeição imprevisível
- SOLUÇÃO: expor o limite de chunks e estimá-lo antes da escrita
- BENEFÍCIOS: previsibilidade e menos retrabalho de split manual
- COMO SOLUCIONAR: calcular a contagem de chunks no pré-voo e avisar com sugestão de divisão
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase B) — estimate_chunk_count; dry-run reporta chunk/token/partition

- GAP-SG-06 dry-run não detecta estouro de chunks
- PROBLEMA: o `--dry-run` do ingest valida o input mas não detecta estouro de chunks
- CONSEQUÊNCIAS: o estouro só aparece na escrita real e aborta o arquivo no meio do lote
- CAUSA RAIZ: a pré-visualização não executa a contagem de chunks do corpo
- RELAÇÃO CAUSA -> EFEITO: dry-run incompleto causa falsa validação que causa falha tardia
- SOLUÇÃO: incluir a contagem de chunks na rota de `--dry-run`
- BENEFÍCIOS: detecção antecipada antes de qualquer escrita
- COMO SOLUCIONAR: reusar o chunker no caminho de dry-run e reportar o total
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase B) — dry-run reporta contagem de chunks e tokens antes da escrita

- GAP-SG-07 Sem auto-split nativo de corpos grandes
- PROBLEMA: a CLI não particiona corpos grandes automaticamente no ingest
- CONSEQUÊNCIAS: quatro arquivos grandes viraram doze partes por divisão manual com `split`
- CAUSA RAIZ: não existe rotina nativa que respeite os limites de bytes e de chunks de forma transparente
- RELAÇÃO CAUSA -> EFEITO: ausência de auto-split causa trabalho manual que causa naming divergente
- SOLUÇÃO: auto-particionar em sub-memórias respeitando ambos os limites
- BENEFÍCIOS: ingestão sem intervenção e sem tentativa e erro
- COMO SOLUCIONAR: dividir por fronteira de linha com margem segura abaixo de 512 chunks
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase B) — auto-split nativo por seção markdown sob bytes/chunks/tokens


## Gaps de Enrich, Dead-letter e Convergência
- GAP-SG-08 non-JSON do nitro apesar do schema enviado
- PROBLEMA: o `deepseek/deepseek-v4-flash:nitro` devolve non-JSON intermitente apesar do schema
- CONSEQUÊNCIAS: cerca de treze por cento das memórias falham por execução
- CAUSA RAIZ: aceitar o parâmetro via `require_parameters` não garante honrá-lo no provider
- CAUSA RAIZ: o DeepSeek tem bugs de structured output confirmados nas issues `#1069` e `#302`
- RELAÇÃO CAUSA -> EFEITO: roteamento `:nitro` por throughput causa schema ignorado que causa non-JSON
- SOLUÇÃO: reparar a saída antes do parse e considerar fallback de modelo
- BENEFÍCIOS: a maioria das respostas non-JSON passa a ser recuperada na origem
- COMO SOLUCIONAR: aplicar reparo `llm_json` e, em falha persistente, rotear a outro modelo de schema estável
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — non-JSON reparado na origem (json_repair) e reclassificado Transient; premissa de schema confirmada

- GAP-SG-09 parse-fail classificado HardFailure terminal na primeira falha
- PROBLEMA: a CLI marca parse-fail como `HardFailure` terminal já na primeira ocorrência
- CONSEQUÊNCIAS: itens válidos morrem sem retry mesmo quando o modelo quase acertou
- CAUSA RAIZ: a classificação trata falha PROBABILÍSTICA de schema como PERMANENTE
- RELAÇÃO CAUSA -> EFEITO: HardFailure imediato causa `dead` que causa perda sem nova tentativa
- SOLUÇÃO: classificar non-JSON como Transient com orçamento de tentativas
- BENEFÍCIOS: o enrichment converge sem descartar itens recuperáveis
- COMO SOLUCIONAR: mover o caso non-JSON de HardFailure para Transient em `record_item_failure`
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — non-JSON reclassificado de HardFailure para Transient em record_item_failure

- GAP-SG-10 Sem reparo de JSON nem fallback de modelo no enrich
- PROBLEMA: o enrich valida o JSON em modo estrito sem tolerância nem fallback
- CONSEQUÊNCIAS: conteúdo denso falha com `expected value at line 1 column 1`
- CAUSA RAIZ: não há extração de bloco markdown nem auto-reparo de chaves e aspas
- RELAÇÃO CAUSA -> EFEITO: parse estrito sem reparo causa aborto que causa item perdido
- SOLUÇÃO: integrar uma camada de reparo antes do `serde_json`
- BENEFÍCIOS: queda drástica de dead-letter sem trocar o modelo
- COMO SOLUCIONAR: aplicar `llm_json` 1.0.3 ou `jsonrepair` 0.1.0 ao corpo retornado
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — reparo via json_repair::repair_to_value + guard de shape antes do parse estrito

- GAP-SG-11 dead terminal sem requeue nem reset-dead
- PROBLEMA: itens em `dead` não retornam ao processamento por nenhuma flag
- CONSEQUÊNCIAS: doze memórias ficaram travadas sem entidades de forma persistente
- CAUSA RAIZ: não existe subcomando de revival e `dead` é estado final
- RELAÇÃO CAUSA -> EFEITO: ausência de reset causa permanência indefinida no dead-letter
- SOLUÇÃO: expor um comando que mova `dead` para `pending`
- BENEFÍCIOS: recuperação de itens sem reescrever memórias nem editar o sidecar
- COMO SOLUCIONAR: adicionar `enrich --requeue-dead` que zera `attempts` e `next_retry_at`
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — enrich --requeue-dead move dead para pending zerando attempts/next_retry_at

- GAP-SG-12 Fila sidecar indexada por nome não memory_id
- PROBLEMA: a fila `.enrich-queue.sqlite` usa `item_key` igual ao NOME da memória
- CONSEQUÊNCIAS: recriar a memória com novo `memory_id` não reativa o item
- CAUSA RAIZ: a chave da fila é o nome e não o identificador estável da memória
- RELAÇÃO CAUSA -> EFEITO: chave por nome causa dessincronia que causa reprocessamento bloqueado
- SOLUÇÃO: vincular a fila ao `memory_id` e propagar o ciclo de vida
- BENEFÍCIOS: a fila acompanha a memória em todas as operações
- COMO SOLUCIONAR: migrar a coluna de chave para `memory_id` com `ALTER TABLE` idempotente
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — coluna operation migrada idempotente + memory_id no enqueue

- GAP-SG-13 forget purge e force-merge não limpam entrada dead
- PROBLEMA: operações no banco principal não resetam a entrada `dead` da fila
- CONSEQUÊNCIAS: `queue_dead` infla com nomes já purgados e confunde o diagnóstico
- CAUSA RAIZ: `forget`, `purge` e `force-merge` atuam em `memories` e não no sidecar
- RELAÇÃO CAUSA -> EFEITO: ausência de cascata causa registro órfão que causa métrica fantasma
- SOLUÇÃO: cascatear a limpeza do sidecar nas operações de delete e purge
- BENEFÍCIOS: métricas confiáveis e fila enxuta
- COMO SOLUCIONAR: aplicar delete em cascata por `memory_id` na remoção
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — cleanup_queue_entry em cascata em forget/purge/remember force-merge

- GAP-SG-14 retry-failed cobre só failed e ignora dead
- PROBLEMA: o `--retry-failed` reativa apenas `failed` e nunca `dead`
- CONSEQUÊNCIAS: com `queue_failed:0` e `queue_dead:8` o comando não tem efeito
- CAUSA RAIZ: o escopo da flag exclui o estado terminal por design
- RELAÇÃO CAUSA -> EFEITO: escopo restrito causa no-op que causa backlog travado
- SOLUÇÃO: oferecer uma flag dedicada ao estado `dead`
- BENEFÍCIOS: recuperação explícita sem ambiguidade de escopo
- COMO SOLUCIONAR: separar `--retry-failed` de `--requeue-dead` com semântica clara
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — --requeue-dead dedicado a dead; --retry-failed mantém escopo failed

- GAP-SG-15 until-empty sai com completed zero quando tudo em cooldown ou dead
- PROBLEMA: o `--until-empty` encerra com `completed:0` quando nada está elegível
- CONSEQUÊNCIAS: o operador conclui falsamente que o backlog terminou
- CAUSA RAIZ: o drain só processa itens com `eligible_now` maior que zero
- RELAÇÃO CAUSA -> EFEITO: cooldown e dead causam elegibilidade zero que causa drain vazio
- SOLUÇÃO: distinguir backlog em cooldown de backlog realmente vazio no relato
- BENEFÍCIOS: convergência honesta sem falsa sensação de término
- COMO SOLUCIONAR: reportar `waiting` e `dead` ao lado de `completed` no fim do loop
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — status reporta waiting+dead ao lado de completed, distinguindo cooldown de backlog vazio

- GAP-SG-16 Backoff opaco sem next_retry_at por item nem ignore-backoff
- PROBLEMA: o `next_retry_at` por item não é exposto nem ajustável
- CONSEQUÊNCIAS: o operador não sabe quando o item volta a ser elegível
- CAUSA RAIZ: o agendamento de backoff é interno e sem controle do chamador
- RELAÇÃO CAUSA -> EFEITO: backoff invisível causa interpretação errada que causa abandono prematuro
- SOLUÇÃO: expor o `next_retry_at` por item e oferecer `--ignore-backoff`
- BENEFÍCIOS: diagnóstico claro e controle de convergência sob janela longa
- COMO SOLUCIONAR: incluir o campo por item no `--status` e uma flag de bypass de cooldown
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — waiting_items[] expõe next_retry_at por item + flag --ignore-backoff

- GAP-SG-17 Timeout por item alto ainda estoura corpos densos
- PROBLEMA: o default de `--openrouter-timeout` ainda estoura em corpos densos
- CONSEQUÊNCIAS: respostas lentas batem 300 segundos e viram `failed`
- CAUSA RAIZ: o teto por item é alto mas insuficiente para geração densa sob carga
- RELAÇÃO CAUSA -> EFEITO: geração lenta causa timeout que causa item perdido
- SOLUÇÃO: ajustar o default e documentar o ajuste por corpo
- BENEFÍCIOS: corpos densos ganham tempo suficiente para completar
- COMO SOLUCIONAR: elevar o valor padrão e orientar `--openrouter-timeout` por tamanho
- STATUS: resolvido na v1.0.97, commit dc6b974 (Fase G) — default --openrouter-timeout elevado de 300 para 600 (DEFAULT_TIMEOUT_SECS 600) para corpos densos

- GAP-SG-18 names destrava cooldown mas é pouco evidente
- PROBLEMA: a flag `--names` destrava o cooldown mas é pouco evidente e não documentada
- CONSEQUÊNCIAS: foi o ÚNICO método que processou os itens travados na sessão
- CAUSA RAIZ: o scan direcionado por nomes ignora o cooldown sem destaque na ajuda
- RELAÇÃO CAUSA -> EFEITO: descoberta tardia causa diagnóstico longo que causa retrabalho
- SOLUÇÃO: documentar `--names` como remédio canônico de cooldown
- BENEFÍCIOS: convergência rápida de subconjuntos travados
- COMO SOLUCIONAR: descrever `--names` e `--names-file` na ajuda e no README
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — --names/--names-file documentados como remédio de cooldown

- GAP-SG-19 Auto-enrich do hook só drena e não escaneia
- PROBLEMA: o enrich automático do hook reportou `items_total:0` sem escanear o backlog
- CONSEQUÊNCIAS: 94 memórias ficaram sem entidades até intervenção manual
- CAUSA RAIZ: o disparo automático apenas drena a fila já enfileirada
- RELAÇÃO CAUSA -> EFEITO: drain sem scan causa fila vazia que causa enrichment inócuo
- SOLUÇÃO: o caminho automático deve executar o ciclo de scan e drain
- BENEFÍCIOS: o backlog é enfileirado e processado sem intervenção
- COMO SOLUCIONAR: o hook deve invocar `--until-empty` que faz scan e drain
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — ciclo scan+drain via --until-empty documentado para o caminho automático

- GAP-SG-20 enrich singleton exit 75 sem multiprocessamento entre memórias
- PROBLEMA: o `enrich` é singleton por namespace e retorna `exit 75` em concorrência
- CONSEQUÊNCIAS: não há multiprocessamento entre memórias, só fan-out interno por batch
- CAUSA RAIZ: o lock singleton impede execuções paralelas no mesmo banco
- RELAÇÃO CAUSA -> EFEITO: singleton causa serialização entre runs que causa vazão limitada
- SOLUÇÃO: manter o singleton e maximizar o paralelismo interno via `--rest-concurrency`
- BENEFÍCIOS: vazão alta sem corromper a escrita serial do SQLite
- COMO SOLUCIONAR: elevar `--rest-concurrency` dentro do batch único e documentar o teto
- STATUS: por design na v1.0.97 (commit a67b863) — singleton mantido; --rest-concurrency intra-batch é o caminho de vazão; documentado na Fase O

- GAP-SG-21 Teto de max-attempts baixo para modelo instável
- PROBLEMA: o teto de `--max-attempts` é baixo para um modelo estocástico
- CONSEQUÊNCIAS: itens recuperáveis esgotam tentativas e viram `dead`
- CAUSA RAIZ: o default cinco não absorve a variância do `:nitro`
- RELAÇÃO CAUSA -> EFEITO: orçamento baixo causa esgotamento que causa dead-letter desnecessário
- SOLUÇÃO: elevar o orçamento de tentativas para modelos instáveis
- BENEFÍCIOS: menos itens em dead-letter por mera variância
- COMO SOLUCIONAR: aumentar `--max-attempts` em conjunto com reparo de JSON
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — default --max-attempts elevado de 5 para 8

- GAP-SG-22 Sidecar não documentado na ajuda nem no README
- PROBLEMA: a fila `.enrich-queue.sqlite` não é documentada na ajuda nem no README
- CONSEQUÊNCIAS: a única inspeção possível foi abrir o sidecar com `sqlite3` read-only
- CAUSA RAIZ: a existência e o schema do sidecar não constam na documentação
- RELAÇÃO CAUSA -> EFEITO: sidecar oculto causa diagnóstico manual que causa risco de erro
- SOLUÇÃO: documentar o sidecar, seu schema e seu ciclo de vida
- BENEFÍCIOS: inspeção segura sem abrir o banco diretamente
- COMO SOLUCIONAR: descrever o sidecar no README e em `enrich --help`
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — sidecar .enrich-queue.sqlite documentado na ajuda do enrich

- GAP-SG-23 Sem comando para listar dead com erro e classe
- PROBLEMA: não existe comando para listar itens `dead` com o erro e a classe
- CONSEQUÊNCIAS: o operador não consegue auditar a causa de cada item morto
- CAUSA RAIZ: o `--status` reporta contagens mas não detalha itens
- RELAÇÃO CAUSA -> EFEITO: ausência de listagem causa cegueira que causa recuperação às cegas
- SOLUÇÃO: oferecer uma listagem detalhada do dead-letter
- BENEFÍCIOS: auditoria item a item da causa terminal
- COMO SOLUCIONAR: adicionar `enrich --list-dead` com `error_class` e `message`
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — enrich --list-dead lista itens dead com error_class e message


## Gaps de memory-bindings e body-extract
- GAP-SG-24 memory-bindings no-op para memórias já vinculadas
- PROBLEMA: a operação retorna `eligible_now:0` para memórias que já têm vínculos
- CONSEQUÊNCIAS: o enrichment do hook processa zero itens no fluxo padrão do projeto
- CAUSA RAIZ: a elegibilidade exige ausência de bindings, nunca satisfeita com `--graph-stdin` curado
- RELAÇÃO CAUSA -> EFEITO: bindings na escrita causam elegibilidade zero que causa grafo raso
- SOLUÇÃO: oferecer um modo de aumento que processe memórias já vinculadas
- BENEFÍCIOS: o grafo ganha entidades finas como `tokio`, `sqlx` e `jwt`
- COMO SOLUCIONAR: adicionar uma operação que ignore o filtro e faça merge aditivo
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — operação augment-bindings processa memórias já vinculadas via --names

- GAP-SG-25 Semântica enganosa de memory-bindings
- PROBLEMA: o nome sugere criar vínculos mas a operação só liga a entidades existentes
- CONSEQUÊNCIAS: o operador espera extração de entidades novas e não a obtém
- CAUSA RAIZ: a operação não extrai entidades do corpo, apenas vincula às já presentes
- RELAÇÃO CAUSA -> EFEITO: nome ambíguo causa expectativa errada que causa diagnóstico confuso
- SOLUÇÃO: renomear ou documentar a operação com precisão semântica
- BENEFÍCIOS: expectativa correta sobre o que cada operação faz
- COMO SOLUCIONAR: descrever a operação como vínculo, não como extração, na ajuda
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — semântica de memory-bindings documentada; augment-bindings cobre a extração aditiva

- GAP-SG-26 Sem operação de aumento aditivo de entidades por nome
- PROBLEMA: não existe operação que adicione entidades finas preservando o corpo e filtrando por nome
- CONSEQUÊNCIAS: o re-enriquecimento de uma memória vinculada é impossível sem efeitos colaterais
- CAUSA RAIZ: o conjunto de operações não cobre o caso de aumento aditivo seletivo
- RELAÇÃO CAUSA -> EFEITO: lacuna de operação causa impossibilidade que causa grafo estagnado
- SOLUÇÃO: introduzir uma operação de aumento aditivo por nome
- BENEFÍCIOS: enriquecimento incremental sem reescrever corpo nem remover vínculos
- COMO SOLUCIONAR: criar uma operação que mescle entidades novas filtrando por `--names`
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — operação augment-bindings faz merge aditivo filtrando por --names

- GAP-SG-27 body-extract ignora names e varre o banco inteiro
- PROBLEMA: a flag `--names` é desconsiderada e a operação varre o banco por id ascendente
- CONSEQUÊNCIAS: ao nomear uma memória, o log reportou `total:4524` varrendo tudo
- CAUSA RAIZ: o seletor de candidatos não aplica o filtro de nomes antes do scan global
- RELAÇÃO CAUSA -> EFEITO: filtro ignorado causa scan total que causa risco de dano amplo
- SOLUÇÃO: respeitar `--names` e `--names-file` como filtro obrigatório
- BENEFÍCIOS: enrichment cirúrgico por subconjunto sem efeitos colaterais
- COMO SOLUCIONAR: aplicar o filtro na consulta de candidatos antes do laço
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — body-extract respeita --names na seleção de candidatos

- GAP-SG-28 body-extract trunca o corpo de forma destrutiva
- PROBLEMA: a operação reescreve o corpo via LLM e reduz drasticamente o conteúdo
- CONSEQUÊNCIAS: `mantos-2021-12-18-amor-lirio` caiu de 120650 para 8563 caracteres
- CAUSA RAIZ: a operação sumariza e substitui o corpo sem modo somente leitura
- RELAÇÃO CAUSA -> EFEITO: extração com reescrita causa truncamento que causa perda de conteúdo
- SOLUÇÃO: separar extração de entidades da reescrita do corpo
- BENEFÍCIOS: extração segura sobre o texto sem alterá-lo
- COMO SOLUCIONAR: introduzir um modo read-only que escreva apenas no grafo
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — flag --body-extract-graph-only escreve só no grafo sem reescrever o corpo


## Gaps de Flags e Parsing
- GAP-SG-29 resume incompatível com mode none e validação só em runtime
- PROBLEMA: `ingest --mode none --resume` falha com `exit 1` e o erro G20 só surge em runtime
- CONSEQUÊNCIAS: a combinação inválida só é detectada após iniciar a execução
- CAUSA RAIZ: a validação do conflito ocorre tarde e não no parser
- RELAÇÃO CAUSA -> EFEITO: validação tardia causa exit inesperado que causa fluxo quebrado
- SOLUÇÃO: declarar o conflito no parser para falha previsível
- BENEFÍCIOS: erro imediato em tempo de parsing
- COMO SOLUCIONAR: usar `conflicts_with` do `clap` para a combinação proibida
- STATUS: resolvido na v1.0.97, commit dc6b974 (Fase G) — ingest --mode none --resume falha fail-fast antes de qualquer IO

- GAP-SG-30 body-file incompatível com graph-stdin pelo canal stdin único
- PROBLEMA: `remember --body-file --graph-stdin` falha com `exit 2`
- CONSEQUÊNCIAS: o corpo precisa ir embutido no JSON, forçando contorno via `jaq --rawfile`
- CAUSA RAIZ: o canal stdin é único e disputado pelos dois modos
- RELAÇÃO CAUSA -> EFEITO: canal único causa exclusão mútua que causa contorno manual
- SOLUÇÃO: ler o grafo de um descritor separado do corpo
- BENEFÍCIOS: corpos grandes com grafo curado sem gambiarra
- COMO SOLUCIONAR: aceitar `--body-file` junto de `--graph-stdin` lendo o grafo de outro fd
- STATUS: resolvido na v1.0.97, commit dc6b974 (Fase G) — remember --graph-file combinável com --body-file (fd separado)

- GAP-SG-31 enrich status exige operation e mode mesmo read-only
- PROBLEMA: o `enrich --status` exige `--operation` e `--mode` mesmo sendo read-only
- CONSEQUÊNCIAS: inspecionar o backlog cobra argumentos de mutação
- CAUSA RAIZ: o subcomando compartilha o grupo de argumentos obrigatórios para todas as flags
- RELAÇÃO CAUSA -> EFEITO: grupo único causa exigência indevida que causa atrito de inspeção
- SOLUÇÃO: tornar os argumentos opcionais quando `--status` está presente
- BENEFÍCIOS: inspeção sem fricção e sem efeitos colaterais
- COMO SOLUCIONAR: condicionar a obrigatoriedade com `required_unless_present`
- STATUS: resolvido na v1.0.97, commit dc6b974 (Fase G) — --status/--list-dead/--requeue-dead dispensam --operation/--mode (required_unless_present_any)

- GAP-SG-32 db não é global e é rejeitado antes do subcomando
- PROBLEMA: a flag `--db` é rejeitada quando posicionada antes do subcomando
- CONSEQUÊNCIAS: comandos quebram com `unexpected argument` de forma intermitente
- CAUSA RAIZ: o parsing posicional da flag de banco é inconsistente entre subcomandos
- RELAÇÃO CAUSA -> EFEITO: posição sensível causa rejeição que causa falha do comando
- SOLUÇÃO: tornar `--db` global ou documentar a posição exigida
- BENEFÍCIOS: caminho do banco estável e independente de ordem
- COMO SOLUCIONAR: usar `SQLITE_GRAPHRAG_DB_PATH` ou promover a flag a global
- STATUS: resolvido na v1.0.97, commit dc6b974 (Fase G) — funcionalmente ok (--db após subcomando + SQLITE_GRAPHRAG_DB_PATH); nota de doc na Fase O

- GAP-SG-33 description com hífen quebra o clap
- PROBLEMA: `--description "- texto"` é interpretado como flag e quebra o comando
- CONSEQUÊNCIAS: dezesseis reescritas falharam em lote com `try '--help'`
- CAUSA RAIZ: o parser trata o valor iniciado por hífen como nova flag quando passado com espaço
- RELAÇÃO CAUSA -> EFEITO: hífen inicial causa erro de parsing que causa falha de toda a operação
- SOLUÇÃO: aceitar valores iniciados por hífen via separador explícito ou sanitização
- BENEFÍCIOS: robustez ao conteúdo real que começa com marcadores de lista
- COMO SOLUCIONAR: exigir `--description=...` ou usar `allow_hyphen_values` no `clap`
- STATUS: resolvido na v1.0.97, commit dc6b974 (Fase G) — allow_hyphen_values em --description/--body

- GAP-SG-34 config doctor json rejeita a própria flag json
- PROBLEMA: o `config doctor --json` rejeita a própria flag `--json`
- CONSEQUÊNCIAS: o diagnóstico de configuração não tem saída estruturada
- CAUSA RAIZ: o subcomando não declara a flag global `--json` no seu escopo
- RELAÇÃO CAUSA -> EFEITO: flag ausente no escopo causa rejeição que causa parsing manual
- SOLUÇÃO: aceitar `--json` no `config doctor`
- BENEFÍCIOS: diagnóstico legível por máquina
- COMO SOLUCIONAR: propagar a flag global ao subcomando de configuração
- STATUS: resolvido na v1.0.97, commit dc6b974 (Fase G) — --json aceito em todas as variantes de config (config doctor --json)

- GAP-SG-35 llm-parallelism rejeitado no remember-batch
- PROBLEMA: a flag `--llm-parallelism` é rejeitada no `remember-batch` com `exit 2`
- CONSEQUÊNCIAS: o usuário não controla o paralelismo na escrita em lote
- CAUSA RAIZ: a flag não está declarada no escopo do `remember-batch`
- RELAÇÃO CAUSA -> EFEITO: flag ausente causa rejeição que causa paralelismo fixo
- SOLUÇÃO: aceitar a flag no `remember-batch` ou documentar a ausência
- BENEFÍCIOS: controle explícito do paralelismo de escrita
- COMO SOLUCIONAR: declarar `--llm-parallelism` no subcomando ou expor alternativa equivalente
- STATUS: resolvido na v1.0.97, commit dc6b974 (Fase G) — --llm-parallelism declarado em remember-batch (resolve rejeição exit 2)

- GAP-SG-36 remember help bloqueado pelo hook PreToolUse
- PROBLEMA: o `remember --help` foi interceptado e bloqueado pelo hook PreToolUse
- CONSEQUÊNCIAS: a inspeção da própria interface da CLI ficou impedida
- CAUSA RAIZ: o hook bloqueia o subcomando sem distinguir `--help` de execução real
- RELAÇÃO CAUSA -> EFEITO: bloqueio indiscriminado causa cegueira de interface que causa tentativa e erro
- SOLUÇÃO: permitir `--help` no hook como caso seguro
- BENEFÍCIOS: inspeção da interface sem violar a política
- COMO SOLUCIONAR: o hook deve liberar invocações que contenham `--help`
- STATUS: resolvido na v1.0.97, commit dc6b974 (Fase M) — verificado: o hook efetivo já libera --help (premissa desatualizada)


## Gaps de Nomes e Escrita Silenciosa
- GAP-SG-37 Normalização kebab silenciosa sem preservar literal
- PROBLEMA: todo nome é forçado a kebab-case com apenas um `WARN`
- CONSEQUÊNCIAS: nomes como `Rules Rust Axum` são impossíveis como chave literal
- CAUSA RAIZ: a normalização não oferece opção de preservar a forma original
- RELAÇÃO CAUSA -> EFEITO: normalização silenciosa causa nome divergente que causa surpresa do operador
- SOLUÇÃO: avisar de forma explícita e oferecer modo de preservação
- BENEFÍCIOS: previsibilidade do nome final
- COMO SOLUCIONAR: emitir aviso claro e expor uma flag de preservação opcional
- STATUS: resolvido na v1.0.97, commit f418957 (Fase H) — remember --strict-name rejeita nome não-kebab devolvendo o canônico

- GAP-SG-38 Truncamento de nome em 60 caracteres sem aviso
- PROBLEMA: nomes derivados são cortados em 60 caracteres sem aviso
- CONSEQUÊNCIAS: nomes longos colidem e dependem de sufixo de desambiguação
- CAUSA RAIZ: o truncamento ocorre sem sinalização ao operador
- RELAÇÃO CAUSA -> EFEITO: corte silencioso causa colisão que causa duplicação inesperada
- SOLUÇÃO: avisar do truncamento e do sufixo aplicado
- BENEFÍCIOS: rastreabilidade entre arquivo de origem e nome final
- COMO SOLUCIONAR: emitir aviso quando o nome exceder o limite e for cortado
- STATUS: resolvido na v1.0.97, commit f418957 (Fase H) — aviso de truncamento promovido debug->warn; truncated/original_name no NDJSON

- GAP-SG-39 Escrita silenciosa sem persistir e sem mensagem
- PROBLEMA: o `remember` retornou exit não-zero sem persistir e sem mensagem clara
- CONSEQUÊNCIAS: a falha ocorreu com nomes contendo datas hifenizadas ou a palavra `GraphRAG`
- CAUSA RAIZ: o tratamento de erro é pouco verboso no stdout
- RELAÇÃO CAUSA -> EFEITO: erro silencioso causa falsa sensação que causa estado inconsistente
- SOLUÇÃO: padronizar envelopes de erro acionáveis
- BENEFÍCIOS: observabilidade real das operações de escrita
- COMO SOLUCIONAR: emitir um envelope JSON de erro com causa e remediação
- STATUS: resolvido na v1.0.97, commit f418957 (Fase H) — AppError::suggestion() emite {error,code,message,suggestion} em qualquer escrita não-zero


## Gaps de Métricas e Observabilidade
- GAP-SG-40 chunks_persisted zero em memórias pesquisáveis
- PROBLEMA: o `remember` reportou `chunks_persisted:0` em memórias pesquisáveis
- CONSEQUÊNCIAS: a métrica não reflete o resultado efetivo da escrita
- CAUSA RAIZ: o contador não captura o estado real de persistência
- RELAÇÃO CAUSA -> EFEITO: métrica incoerente causa diagnóstico errado que causa retrabalho
- SOLUÇÃO: tornar a métrica coerente com o resultado observável
- BENEFÍCIOS: confiança na contagem reportada
- COMO SOLUCIONAR: contar os chunks realmente persistidos após o commit
- STATUS: resolvido na v1.0.97, commit f418957 (Fase I) — chunks_persisted lê COUNT real pós-commit (storage_chunks::count_for_memory)

- GAP-SG-41 embedding status zerado com embeddings presentes
- PROBLEMA: o `embedding status` reporta zero mesmo com embeddings presentes
- CONSEQUÊNCIAS: a prova real de embedding precisou vir do `vec_rank` em `hybrid-search`
- CAUSA RAIZ: a métrica reflete só a fila assíncrona, sempre vazia no caminho REST síncrono
- RELAÇÃO CAUSA -> EFEITO: métrica de fila causa contagem zero enganosa que causa auditoria falsa
- SOLUÇÃO: reportar a cobertura real de vetores presentes
- BENEFÍCIOS: auditoria confiável sem inferência indireta
- COMO SOLUCIONAR: contar vetores na tabela em vez de itens na fila
- STATUS: resolvido na v1.0.97, commit f418957 (Fase I) — embedding status com objeto coverage de vetores reais nas tabelas

- GAP-SG-42 enrich status eligible_now idêntico entre operações
- PROBLEMA: o `enrich --status` reporta `eligible_now` idêntico entre operações distintas
- CONSEQUÊNCIAS: é impossível distinguir o backlog real de cada operação
- CAUSA RAIZ: o contador de fila é compartilhado e não segmentado por operação
- RELAÇÃO CAUSA -> EFEITO: contador único causa ambiguidade que causa diagnóstico incorreto
- SOLUÇÃO: segmentar o backlog por operação
- BENEFÍCIOS: visão precisa do trabalho pendente por operação
- COMO SOLUCIONAR: contar itens elegíveis por operação no relatório de status
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — status reporta counts segmentados por operação

- GAP-SG-43 stats json total_memories null
- PROBLEMA: o `stats --json` retornou `total_memories:null`
- CONSEQUÊNCIAS: a contagem exigiu fallback manual via `list`
- CAUSA RAIZ: o agregado não preenche o campo de total
- RELAÇÃO CAUSA -> EFEITO: agregado incompleto causa null que causa contagem manual
- SOLUÇÃO: preencher o total no agregado de estatísticas
- BENEFÍCIOS: estatística confiável em uma chamada
- COMO SOLUCIONAR: popular `total_memories` na consulta de `stats`
- STATUS: resolvido na v1.0.97, commit f418957 (Fase I) — total_memories preenchido no stats --json

- GAP-SG-44 vec_memories_missing silencioso após re-remember
- PROBLEMA: o `health` acusou `vec_memories_missing:2` após muitos re-remember
- CONSEQUÊNCIAS: embeddings ficaram nulos sem erro visível e exigiram `re-embed` manual
- CAUSA RAIZ: a falha de embedding não foi sinalizada no caminho de escrita
- RELAÇÃO CAUSA -> EFEITO: falha silenciosa causa vetor ausente que causa busca degradada
- SOLUÇÃO: sinalizar embedding ausente na escrita e oferecer reparo automático
- BENEFÍCIOS: integridade de embedding sem auditoria manual
- COMO SOLUCIONAR: validar o vetor após o commit e disparar `re-embed` quando ausente
- STATUS: resolvido na v1.0.97, commit f418957 (Fase I) — remember checa vetor pós-commit e recomenda re-embed se ausente

- GAP-SG-45 llm_parallelism um no evento de scan mesmo com paralelismo
- PROBLEMA: o evento de scan mostra `llm_parallelism:1` mesmo com paralelismo ativo
- CONSEQUÊNCIAS: o relato sugere serialização que não corresponde ao drain real
- CAUSA RAIZ: o campo reflete a fase de scan serial e não o fan-out do drain
- RELAÇÃO CAUSA -> EFEITO: campo ambíguo causa leitura errada que causa diagnóstico confuso
- SOLUÇÃO: separar a métrica de scan da métrica de drain
- BENEFÍCIOS: relato fiel do paralelismo efetivo
- COMO SOLUCIONAR: emitir o paralelismo do drain em campo distinto
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — status separa a métrica de scan da métrica de drain

- GAP-SG-46 eligible_now zero logo após created
- PROBLEMA: o `eligible_now` fica em zero logo após `created`
- CONSEQUÊNCIAS: o diagnóstico imediato confunde criação com backlog vazio
- CAUSA RAIZ: o scan de elegibilidade é assíncrono e ainda não rodou
- RELAÇÃO CAUSA -> EFEITO: scan assíncrono causa elegibilidade zero que causa interpretação errada
- SOLUÇÃO: sinalizar que a elegibilidade ainda não foi materializada
- BENEFÍCIOS: leitura honesta do estado recém-criado
- COMO SOLUCIONAR: marcar o estado como pendente de scan no relatório
- STATUS: resolvido na v1.0.97, commit a67b863 (Fase C-F) — status marca state pending-scan logo após created


## Gaps de Vocabulário Canônico do Grafo
- GAP-SG-47 Tipos de entidade não-canônicos descartados silenciosamente
- PROBLEMA: tipos de entidade não-canônicos são descartados com um `WARN`
- CONSEQUÊNCIAS: entidades legítimas como `platform`, `language` e `feature` são perdidas
- CAUSA RAIZ: o prompt de extração não restringe o LLM ao conjunto canônico
- RELAÇÃO CAUSA -> EFEITO: prompt sem restrição causa tipos inválidos que causam descarte de dados
- SOLUÇÃO: instruir o LLM com o conjunto canônico e mapear em vez de descartar
- BENEFÍCIOS: zero perda de entidades legítimas
- COMO SOLUCIONAR: injetar a lista canônica no prompt e aplicar o mapa `platform -> concept`
- STATUS: resolvido na v1.0.97, commit dc6b974 (Fase J) — EntityType::map_to_canonical mapeia tipos não-canônicos em vez de descartar; lista canônica no prompt

- GAP-SG-48 Relações não-canônicas aceitas com warn
- PROBLEMA: relações não-canônicas são aceitas com `WARN` enquanto entidades são rejeitadas
- CONSEQUÊNCIAS: o grafo acumula relações fora do vocabulário canônico
- CAUSA RAIZ: as políticas do parser de entidades e do parser de relações divergem
- RELAÇÃO CAUSA -> EFEITO: política divergente causa inconsistência que causa grafo heterogêneo
- SOLUÇÃO: unificar a política e mapear relações não-canônicas
- BENEFÍCIOS: grafo consistente com vocabulário único
- COMO SOLUCIONAR: aplicar o mapa `part_of -> applies-to` em vez de aceitar cru
- STATUS: resolvido na v1.0.97, commit dc6b974 (Fase J) — map_to_canonical_relation unifica relações da extração LLM

- GAP-SG-49 entity degree cap apenas consultivo
- PROBLEMA: o `entity degree cap` apenas avisa e não previne hubs gigantes
- CONSEQUÊNCIAS: o `max_degree` chegou a 152 e travessias retornam ruído
- CAUSA RAIZ: o cap de 50 é consultivo e sem ação de mitigação
- RELAÇÃO CAUSA -> EFEITO: cap consultivo causa hub gigante que causa travessia ruidosa
- SOLUÇÃO: dar ação ao cap em vez de só avisar
- BENEFÍCIOS: travessias limpas sem ruído de hub
- COMO SOLUCIONAR: rejeitar a aresta de menor peso ou dividir o hub ao exceder o cap
- STATUS: resolvido na v1.0.97, commit dc6b974 (Fase J) — graph::enforce_degree_cap acionável (poda aresta de menor peso) em link.rs/remember.rs


## Gaps de Leitura, Merge e Prune
- GAP-SG-50 read format raw retorna zero bytes
- PROBLEMA: a flag `read --format raw` retorna 0 bytes
- CONSEQUÊNCIAS: o corpo só é recuperável via `read --name X --json` e `jaq -r .body`
- CAUSA RAIZ: a flag é aceita pelo parser mas não produz saída
- RELAÇÃO CAUSA -> EFEITO: flag aceita mas vazia causa pipeline quebrada em silêncio
- SOLUÇÃO: corrigir a saída de raw ou remover a flag
- BENEFÍCIOS: contrato de saída previsível
- COMO SOLUCIONAR: emitir o campo `body` puro quando raw for solicitado
- STATUS: resolvido na v1.0.97, commit f418957 (Fase K) — read --format raw emite body puro sem envelope

- GAP-SG-51 force-merge com entities vazio mescla em vez de substituir
- PROBLEMA: o `force-merge --graph-stdin` com `entities:[]` mescla os bindings antigos
- CONSEQUÊNCIAS: é impossível zerar vínculos sem `forget`
- CAUSA RAIZ: a operação aplica merge aditivo e nunca substituição
- RELAÇÃO CAUSA -> EFEITO: merge aditivo causa retenção de vínculos que causa grafo imutável
- SOLUÇÃO: oferecer um modo de substituição explícita de vínculos
- BENEFÍCIOS: controle total do conjunto de vínculos da memória
- COMO SOLUCIONAR: aceitar uma flag que substitua em vez de mesclar quando `entities` está vazio
- STATUS: resolvido na v1.0.97, commit f418957 (Fase K) — remember --replace-graph zera vínculos antes de re-vincular (entities:[] limpa sem forget)

- GAP-SG-52 prune-ner aborta em bindings curados
- PROBLEMA: o `prune-ner` remove só bindings NER e aborta em bindings curados
- CONSEQUÊNCIAS: não há comando para desvincular memória e entidade de forma cirúrgica
- CAUSA RAIZ: o escopo do `prune-ner` exclui vínculos curados via `--graph-stdin`
- RELAÇÃO CAUSA -> EFEITO: escopo restrito causa aborto que causa desvínculo impossível
- SOLUÇÃO: oferecer um comando de desvínculo seletivo por par memória e entidade
- BENEFÍCIOS: curadoria cirúrgica do grafo
- COMO SOLUCIONAR: estender `unlink` para cobrir vínculos curados por nome
- STATUS: resolvido na v1.0.97, commit f418957 (Fase K) — unlink --memory --entity remove binding curado que prune-ner não atinge


## Gaps de Inventário e Ingest
- GAP-SG-53 list limit subestima inventário
- PROBLEMA: o `list --limit 2000` não retornou todas as memórias existentes
- CONSEQUÊNCIAS: a contagem previu três colisões mas o ingest revelou 46 duplicatas reais
- CAUSA RAIZ: a paginação subestima o inventário sem aviso
- RELAÇÃO CAUSA -> EFEITO: listagem incompleta causa contagem errada que causa decisão errada
- SOLUÇÃO: usar `export` como inventário confiável em vez de `list`
- BENEFÍCIOS: contagem exata para decisões de dedup
- COMO SOLUCIONAR: preferir `export --namespace` que emite NDJSON completo
- STATUS: resolvido na v1.0.97, commit f418957 (Fase L) — list --json emite truncation_warning recomendando export como inventário

- GAP-SG-54 ingest sem force-merge para atualizar duplicatas
- PROBLEMA: o `ingest` não possui `--force-merge` para atualizar duplicatas existentes
- CONSEQUÊNCIAS: 46 arquivos já existentes foram marcados `skipped` sem atualização
- CAUSA RAIZ: o `--force-merge` é exclusivo do `remember` e ausente no `ingest`
- RELAÇÃO CAUSA -> EFEITO: ausência da flag causa skip que causa conteúdo desatualizado
- SOLUÇÃO: oferecer atualização de duplicatas no ingest
- BENEFÍCIOS: reingestão idempotente que atualiza o existente
- COMO SOLUCIONAR: usar `remember-batch --force-merge` para o caminho de atualização em massa
- STATUS: resolvido na v1.0.97, commit f418957 (Fase L) — ingest --force-merge atualiza duplicatas in-place

- GAP-SG-55 ingest deduplica só por nome exato
- PROBLEMA: o `ingest` deduplica apenas por nome exato
- CONSEQUÊNCIAS: o naming divergente `parte-1` versus `part-01` gerou 32 memórias para 4 arquivos
- CAUSA RAIZ: a derivação de nome difere entre execuções e quebra o dedup
- RELAÇÃO CAUSA -> EFEITO: naming divergente causa dedup falho que causa duplicação silenciosa
- SOLUÇÃO: deduplicar também por hash de conteúdo
- BENEFÍCIOS: conteúdo idêntico detectado independente do nome derivado
- COMO SOLUCIONAR: comparar `body_hash` antes de criar nova memória
- STATUS: resolvido na v1.0.97, commit f418957 (Fase L) — ingest deduplica por body_hash (naming divergente não duplica)

- GAP-SG-56 429 Retry-After consumido mas não exposto ao chamador
- PROBLEMA: o `429 Retry-After` é consumido internamente mas não exposto ao chamador
- CONSEQUÊNCIAS: o backoff empurra o `next_retry_at` sem informar o tempo de espera
- CAUSA RAIZ: o cliente aplica o header internamente sem propagá-lo ao operador
- RELAÇÃO CAUSA -> EFEITO: espera oculta causa diagnóstico cego que causa interpretação errada
- SOLUÇÃO: expor o tempo de espera ao chamador
- BENEFÍCIOS: visibilidade do cooldown imposto pelo provider
- COMO SOLUCIONAR: incluir o `retry-after` lido no envelope de erro retornado
- STATUS: resolvido na v1.0.97, commit aaeebcc (Fase A) — retry-after do servidor exposto ao chamador via RateLimited


## Fontes Consultadas
- OpenRouter Structured Outputs em `openrouter.ai/docs/guides/features/structured-outputs`
- OpenRouter Provider Routing e require_parameters em `openrouter.ai/docs/guides/routing/provider-selection`
- OpenRouter variante nitro em `openrouter.ai/docs/guides/routing/model-variants/nitro`
- Issue `simonw/llm-openrouter#28` sobre habilitar schema só em modelos compatíveis
- DeepSeek structured output nas issues `#1069` e `#302` e DeepSeek JSON mode docs
- Qwen3-Embedding-8B com contexto de 32K tokens em Hugging Face, SiliconFlow e OpenRouter
- Crates docs.rs `llm_json` 1.0.3 e `jsonrepair` 0.1.0 para reparo de JSON
- APIs Rust `clap` `conflicts_with` e `reqwest` `ClientBuilder::timeout` para as correções de parser e timeout
- CHANGELOG.md linhas 11 a 17 e 31 a 33 para GAP-ENRICH-BACKLOG-CONVERGE, structured outputs e openrouter-timeout
- GraphRAG memórias `ingest-rules-rust-embedding-limit-113kb`, `enrich-rules-rust-cooldown-names-fix` e `auditoria-auto-enrichment-causa-raiz-openrouter-200`


## Dívida Técnica v1.0.97 — Auditoria Pós-Selagem (incremental)
- GAP-SG-57 enrich.rs monólito de 6013 linhas sem modularização
- PROBLEMA: `src/commands/enrich.rs` cresceu para 6013 linhas com TODO de modularização desde v1.0.89 (ADR-0046 registrava 4116)
- CONSEQUÊNCIAS: navegação difícil, fila/scan/persist/operações misturados num único arquivo
- CAUSA RAIZ: crescimento incremental sem split; 14+ handlers call_* mais fila mais scan mais persist coabitando
- RELAÇÃO CAUSA -> EFEITO: arquivo gigante causa baixa navegabilidade que causa manutenção lenta
- SOLUÇÃO: converter em diretório `src/commands/enrich/` com submódulos coesos
- BENEFÍCIOS: cada submódulo abaixo de 1700 linhas, testes localizados, fecha ADR-0046
- COMO SOLUCIONAR: `git mv enrich.rs enrich/mod.rs` e extrair queue.rs, scan.rs, postprocess.rs, extraction.rs preservando os 6 símbolos públicos
- STATUS: resolvido na v1.0.97 (working tree, pendente de commit) — mod.rs 2355 mais queue 639 mais scan 859 mais postprocess 352 mais extraction 1636; 36 testes preservados; ADR-0056

- GAP-SG-58 unwrap()/expect() de produção sem auditoria nem lint gate
- PROBLEMA: panics potenciais via unwrap()/expect() em código de produção, sem portão automático
- CONSEQUÊNCIAS: falha em runtime (panic) em vez de erro tratável; estimativas anteriores imprecisas (423, depois 41/2)
- CAUSA RAIZ: atalhos de unwrap/expect e ausência dos lints clippy::unwrap_used/expect_used
- RELAÇÃO CAUSA -> EFEITO: unwrap não auditado causa panic que causa crash em vez de envelope de erro
- SOLUÇÃO: auditar e converter os sítios reais de produção e ativar o lint gate
- BENEFÍCIOS: zero panics de produção rastreáveis; regressão futura travada pelo lint
- COMO SOLUCIONAR: 41 sítios reais (não 423) — embedder OnceLock para ok_or_else, signals para inspect_err.ok, system_load para into_inner, enrich 1409 para let-else, 24 provider_binary para unwrap_or_else(Path::new vazio), config_cmd serde para `?`; `#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]` em lib.rs; 2 invariantes const com `#[allow]` justificado
- STATUS: resolvido na v1.0.97 (working tree, pendente de commit) — clippy --all-targets -D warnings verde

- GAP-SG-59 parse_claude_output duplicado entre enrich e ingest_claude
- PROBLEMA: a lógica de parse do output do claude estava duplicada, com divergência semântica oculta de max_turns
- CONSEQUÊNCIAS: manutenção em dobro e risco de drift; ingest sem detecção G03 max_turns nem auth warn
- CAUSA RAIZ: ingest_claude::parse_claude_output divergiu de claude_runner::parse_claude_output (max_turns fatal versus tolerado)
- RELAÇÃO CAUSA -> EFEITO: duplicação divergente causa correção parcial que causa comportamento inconsistente
- SOLUÇÃO: unificar via parâmetro preservando ambas as semânticas
- BENEFÍCIOS: fonte única de verdade; ingest ganha G03 mais auth warn; cerca de 40 linhas removidas
- COMO SOLUCIONAR: `claude_runner::parse_claude_output_opts(stdout, tolerate_max_turns)`; enrich usa false, ingest usa true; extract_with_claude e open_queue_db NÃO unificados (divergência legítima)
- STATUS: resolvido na v1.0.97 (working tree, pendente de commit) — teste test_terminal_reason_max_turns_detected verde

- GAP-SG-60 config_cmd.rs com 5 unwrap em serde_json::to_string não auditados
- PROBLEMA: 5 `serde_json::to_string(&output).unwrap()` em println! escaparam da auditoria por heurística de boundary cfg(test)
- CONSEQUÊNCIAS: panic potencial na serialização da saída de config; lacuna na auditoria manual
- CAUSA RAIZ: config_cmd.rs sem `#[cfg(test)]` inline; a heurística do primeiro cfg(test) não os classificou; só o lint os revelou
- RELAÇÃO CAUSA -> EFEITO: auditoria manual incompleta causa sítios omitidos que o lint automático expõe
- SOLUÇÃO: converter para propagação de erro via From
- BENEFÍCIOS: zero panic na serialização; prova de que o lint gate supera a busca manual
- COMO SOLUCIONAR: trocar `.unwrap()` por `?` (AppError::Json com #[from] serde_json::Error já existe)
- STATUS: resolvido na v1.0.97 (working tree, pendente de commit) — descoberto e fechado pelo lint do GAP-SG-58

- GAP-SG-61 teste flaky concurrency_peak_never_exceeds_permits
- PROBLEMA: embedder::tests::concurrency_peak_never_exceeds_permits falha sob a suíte completa (cerca de 50 por cento), passa isolado
- CONSEQUÊNCIAS: a suíte default reporta falha esporádica não-determinística
- CAUSA RAIZ REAL (diagnóstico anterior estava ERRADO): NÃO é timing nem run_bounded; o teste lê o global de processo crate::constants::embedding_dim() em DOIS instantes — uma vez para o dim esperado (linha 1745) e outra dentro de work para o vetor produzido (linha 1761); testes-irmãos mutam a env var SQLITE_GRAPHRAG_EMBEDDING_DIM em paralelo (llm_embedding=64, connection=96), injetando dim divergente entre as duas leituras; run_bounded então rejeita 64 != 384 (G42/C5) e o .expect panica
- RELAÇÃO CAUSA -> EFEITO: leitura dupla de global mutável de processo causa dim divergente entre esperado e produzido que causa erro de fan-out que causa panic
- SOLUÇÃO: ler a dimensão UMA vez e usar o MESMO valor capturado nos dois pontos (esperado e produzido), tornando o teste hermético ao global que ele não possui
- BENEFÍCIOS: teste determinístico independente de mutações de env por irmãos; fecha a classe (corrige tambem panicking_task e cancellation que tinham o mesmo padrão)
- COMO SOLUCIONAR: trocar dummy_vec(crate::constants::embedding_dim()) por dummy_vec(dim) nos 3 sítios (1761/1807/1846); dim é Copy, capturado por cópia no closure move
- STATUS: resolvido na v1.0.97 (working tree, pendente de commit) — 0/10 falhas sob contenção apos o fix (era cerca de 5/10); 970 lib testes verdes

- GAP-SG-62 installed_binary_smoke falha por version mismatch do binário global (AMBIENTE)
- PROBLEMA: tests/installed_binary_smoke.rs (26 testes) falha porque ~/.cargo/bin/sqlite-graphrag é v1.0.96 e o workspace é v1.0.97
- CONSEQUÊNCIAS: cargo test --features slow-tests reporta 26 falhas de ambiente, não de lógica
- CAUSA RAIZ: o teste valida o binário INSTALADO globalmente, não o compilado do workspace; o instalado está desatualizado
- RELAÇÃO CAUSA -> EFEITO: binário global desatualizado causa mismatch que causa falha de setup (não de código)
- SOLUÇÃO: reinstalar o binário do workspace ou usar o bypass declarado
- BENEFÍCIOS: smoke do binário instalado alinhado ao workspace
- COMO SOLUCIONAR: `cargo install --path . --locked --force` OU `SQLITE_GRAPHRAG_ALLOW_INSTALLED_VERSION_MISMATCH=1`
- STATUS: resolvido na v1.0.97 — `cargo install --path . --locked --force` executado (exit 0); binário global alinhado ao workspace; installed_binary_smoke valida sem bypass

- GAP-SG-63 cluster flaky llm_slots::tests sob a suíte completa (descoberto ao reproduzir GAP-SG-61)
- PROBLEMA: read_status_reflects_active_slots, slot_enforces_max_concurrency, slot_releases_on_drop e concurrent_acquires_with_2_threads_serialize falham até 8 em 10 rodadas da suíte completa
- CONSEQUÊNCIAS: a suíte default reporta de 1 a 4 falhas esporádicas; mascarava o estado verde real
- CAUSA RAIZ: defeito composto nos testes (produção intocada) — slots_dir() resolve XDG_RUNTIME_DIR ANTES de SQLITE_GRAPHRAG_CACHE_DIR; em máquina com XDG_RUNTIME_DIR setado, read_status e concurrent só setavam CACHE_DIR sem remover XDG, então o isolamento virava no-op e todos gravavam no mesmo dir real; além disso std::env::set_var é global de processo e os testes em paralelo clobberavam XDG/CACHE_DIR mutuamente
- RELAÇÃO CAUSA -> EFEITO: precedência de XDG mais mutação de env global em paralelo causa colisão no mesmo slots-dir que causa contagem de slots não-determinística que causa asserção falha
- SOLUÇÃO: serializar os 4 testes que tocam o slots-dir com um static Mutex (poison-recovery via into_inner) e fazer os dois testes quebrados usarem isolate_slots_env() real (que remove XDG e seta CACHE_DIR único)
- BENEFÍCIOS: cluster determinístico; precedência de produção XDG > CACHE_DIR preservada intacta
- COMO SOLUCIONAR: static SLOT_TEST_LOCK em src/llm_slots.rs; let _serial = SLOT_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner()) no topo dos 4; trocar o set manual de CACHE_DIR por isolate_slots_env()/restore_slots_env() em read_status e concurrent
- STATUS: resolvido na v1.0.97 (working tree, pendente de commit) — 0/10 falhas sob contenção apos o fix (era 8/10); clippy -D warnings, fmt e 970 lib testes verdes

- GAP-SG-64 fila do enrich (.enrich-queue.sqlite) resolvida pelo CWD, não pelo --db (achado de auditoria e2e v1.0.97)
- PROBLEMA: enrich --status --db X reporta o backlog da fila do CWD, não do banco apontado por --db
- CONSEQUÊNCIAS: número de backlog enganoso quando --db diverge do CWD; ao drenar, a fila do CWD (memory_id de outro banco) seria processada contra --db, gerando dead-letter ou cross-processing por colisão de id
- CAUSA RAIZ: open_queue_db(DEFAULT_QUEUE_DB) usa a constante relativa ".enrich-queue.sqlite" (src/commands/enrich/mod.rs:62, chamadas 1245/1313/1627/1838); o caminho NÃO deriva do diretório do --db; a tabela queue não tem coluna namespace nem qualificador de banco (indexa por memory_id/entity_id inteiros)
- RELAÇÃO CAUSA -> EFEITO: fila CWD-fixa mais ausência de qualificador de banco causa divergência entre o scan (respeita --db, unbound_backlog correto) e a fila (CWD) que causa --status enganoso e risco de processamento cruzado
- EVIDÊNCIA: mesmo --db e mesmo --namespace, queue_pending=111 a partir do CWD do projeto e queue_pending=0 a partir de /tmp/e2e-cwd-test; a única variável é o CWD; irmão de design do GAP-SG-63 (slots_dir CWD/XDG)
- IMPACTO: nulo no uso canônico (rodar do diretório do projeto contra ./graphrag.sqlite, fila e banco coincidem); real quando --db aponta para outro banco a partir do mesmo CWD
- SOLUÇÃO: derivar o caminho da fila do diretório do --db (sidecar ao lado do banco) em vez do CWD; preservar fallback legado quando --db é default
- BENEFÍCIOS: --status reflete o banco apontado; fila isolada por banco; elimina cross-processing por id entre bancos no mesmo CWD
- COMO SOLUCIONAR: trocar DEFAULT_QUEUE_DB relativo por path derivado de Path::new(db).parent().join(".enrich-queue.sqlite"); avaliar migração ou aviso para filas legadas no CWD
- STATUS: resolvido na v1.0.97 — fila derivada de `crate::paths::sidecar_path(&paths.db, ".enrich-queue.sqlite")` nos 4 ramos de run mais cleanup_queue_entry(db_path,..); ADR-0057; teste tests/enrich_queue_db_isolation.rs verde; sem migração legada (canônico coincide)


- GAP-SG-65 fila do ingest (.ingest-queue.sqlite) com default CWD-relativo, não derivado do --db (achado da auditoria e2e v1.0.97)
- PROBLEMA: o default `#[arg(long, default_value = ".ingest-queue.sqlite")]` (IngestArgs.queue_db) resolve contra o CWD, não o diretório do --db
- CONSEQUÊNCIAS: --resume e --retry-failed perdem a fila quando o CWD muda entre execuções; cross-processing por memory_id ao drenar contra outro banco
- CAUSA RAIZ: clap default_value é estático em parse-time e não deriva de outro argumento; consumido por ingest_claude.rs e ingest_codex.rs
- RELAÇÃO CAUSA -> EFEITO: default CWD-relativo causa fila desalinhada do --db que causa resume quebrado e risco de processamento cruzado
- EVIDÊNCIA: irmão direto do GAP-SG-64; mesma classe, confirmada por varredura exaustiva (enrich e ingest eram os únicos membros vivos; slots_dir/lock.rs/schema_path já seguros)
- SOLUÇÃO: queue_db vira Option<String> sem default; em runtime deriva crate::paths::sidecar_path(&early_paths.db, ".ingest-queue.sqlite") quando None; --queue-db explícito sobrepõe
- BENEFÍCIOS: fila do ingest segue o --db; --resume reencontra a fila independente do CWD; helper sidecar_path compartilhado com o enrich (DRY)
- COMO SOLUCIONAR: trocar String para Option<String>, remover default_value, derivar via sidecar_path; padrão Option mais runtime confirmado por docs clap e StackOverflow 78053303
- STATUS: resolvido na v1.0.97 — ADR-0057; clippy -D, fmt, 973 lib mais schema_contract_strict 38/0 mais ingest_integration 10/0 verdes
- LIMPEZA v1.0.97: removida a constante morta `constants::CLI_LOCK_FILE = "cli.lock"` (zero usos em src; o lock real usa lock.rs com cache_dir, XDG-derivado)


- GAP-SG-66 dead-letter ÓRFÃO sem comando de limpeza (achado da auditoria de hooks v1.0.97)
- PROBLEMA: linhas `status='dead'` cujo `item_key` (nome da memória) não existe mais no banco principal não têm caminho de purga; `--requeue-dead` só as re-falha
- CONSEQUÊNCIAS: `queue_dead` infla de forma monotônica (110 no banco real, todas "not found"); os avisos de dead-letter dos hooks viram ruído; recuperação impossível
- CAUSA RAIZ: a fila indexa por `item_key`/`memory_id`; quando a memória é renomeada ou purgada APÓS enfileirar, a linha dead vira órfã, e `cleanup_queue_entry` (GAP-SG-13) só dispara em forget/purge de memória EXISTENTE
- RELAÇÃO CAUSA -> EFEITO: memória ausente mais dead permanente mais ausência de comando de purga causa `queue_dead` que nunca decresce que causa ruído de métrica e recover quebrado
- EVIDÊNCIA: 110 dead todos `error_class=permanent` "not found: memory 'X' not found"; a auditoria de hooks expôs a lacuna porque `recover-dead.sh` usava `pending list --filter-status dead` (exit 2 na v1.0.97) e nunca via o dead-letter real
- SOLUÇÃO: `enrich --prune-dead-orphans` — inspetor read-only que deleta só linhas dead `item_type='memory'` cujo `item_key` não existe mais no banco principal (`namespace+name+deleted_at IS NULL`); linhas `entity` intocadas
- BENEFÍCIOS: `queue_dead` honesto; `recover-dead.sh` poda órfão e recupera o resto (corpo presente); o loop de convergência fecha sem ruído permanente
- COMO SOLUCIONAR: flag no grupo `required_unless_present_any` (dispensa `--operation`/`--mode`); `queue::prune_dead_orphans(queue_conn, main_conn, op, ns)` reusando o `SELECT id FROM memories WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL` do `enqueue_candidate`; campo `DeadSummary.pruned`; teste `prune_dead_orphans_removes_only_orphan_memory_rows`
- STATUS: resolvido na v1.0.97 — implementado em `src/commands/enrich/{queue.rs,mod.rs}`; smoke real podou 110 órfãos (`dead_total` 110->0, `pruned:110`); hooks `lib/graphrag-recover-dead.sh` (GAP-A, usava `pending list` inválido) e `lib/graphrag-enrich-worker.sh` (GAP-B, residual passa a emitir `total_dead` db-scoped) reconectados; ADR-0058
