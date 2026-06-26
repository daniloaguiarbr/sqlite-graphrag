# Embedding Alternativo para sqlite-graphrag — Análise de Viabilidade

é proibido ter modelo default hardcode na open router, o modelo deve ser selecionado pelo  usuário ao digitar o comando

## Problema
- O comando `remember` leva 20-60 segundos para salvar UMA memória
- O comando `remember` com `--llm-backend none` leva 38 milissegundos
- O embedding via subprocesso LLM é 3000x mais lento que o I/O SQLite puro
- O `remember` com body longo e entidades pode ultrapassar 120 segundos
- O `remember` atinge timeout e falha com exit 143 (SIGTERM)


## Consequências do Problema
- Hooks de memória do Claude Code travam por 1-2 minutos a CADA turno
- Sessões interativas sofrem latência inaceitável no salvamento
- Memórias falham silenciosamente quando o timeout é atingido
- O pipeline de `ingest` em massa fica proibitivamente lento
- O `enrich --operation re-embed` leva horas para re-embedar centenas de memórias
- A experiência do usuário degrada a CADA interação com o GraphRAG


## Causa Raiz
- CADA embedding spawna um processo Node.js completo (`codex exec` ou `claude -p`)
- O `codex exec` consome ~350 MB de RSS por instância
- O `claude -p` consome ~200-400 MB de RSS por instância
- CADA spawn paga cold-start: boot do Node.js (5-15 segundos)
- CADA spawn paga autenticação: validação OAuth (1-3 segundos)
- CADA spawn paga schema parsing: carregamento do `--output-schema` JSON
- O LLM generativo GERA floats token-a-token via autoregressive decoding
- Um encoder neural dedicado COMPUTA o vetor em UMA forward pass (~10-500ms)
- A arquitetura atual usa um LLM generativo como calculadora de embedding
- Usar LLM generativo para embedding é como usar um canhão para matar mosca


## Relação Causa e Efeito
- CAUSA: o embedding usa subprocess LLM generativo (Node.js headless)
- EFEITO: cold-start de 5-15 segundos POR chamada de embedding
- CAUSA: o cold-start de 5-15 segundos POR chamada
- EFEITO: `remember` simples (1 chunk + 5 entidades) leva 20-60 segundos
- CAUSA: `remember` leva 20-60 segundos
- EFEITO: hooks de memória travam a sessão interativa do Claude Code
- CAUSA: hooks de memória travam a sessão
- EFEITO: o usuário percebe lentidão extrema e abandona o uso do GraphRAG
- CAUSA: o LLM gera floats token-a-token (autoregressive)
- EFEITO: latência de 10-30 segundos para gerar 64 números decimais
- CAUSA: latência de 10-30 segundos para 64 floats
- EFEITO: o bottleneck NÃO é o SQLite, NÃO é o chunking, é EXCLUSIVAMENTE o LLM


## Solução
- Separar o embedding do enrichment em duas fases sequenciais distintas
- FASE 1 (embedding): usar modelo de embedding dedicado via API REST ou local
- FASE 2 (enrichment): manter codex/claude/opencode headless para raciocínio
- O embedding é uma operação matemática determinística que NÃO precisa de LLM generativo
- O enrichment é uma operação de raciocínio que PRECISA de LLM generativo
- São dois problemas fundamentalmente diferentes que NÃO devem compartilhar backend


## Como Solucionar
- Adicionar a dependência `reqwest` com `rustls-tls` ao `Cargo.toml` (~500 KB ao binário)
- Implementar novos backends de embedding: `Ollama`, `OpenRouter`, `LlamaCpp`
- Criar flag `--embedding-backend auto|openrouter|llm`
- Criar flag `--embedding-model <ID>` (OBRIGATORIO com `--embedding-backend openrouter`)
- Implementar adapter MRL que trunca o vetor retornado para 64 dimensões
- Manter flag `--llm-backend codex|claude|opencode` EXCLUSIVA para enrichment
- ZERO alteração no schema do banco de dados
- ZERO migração SQL
- ZERO ALTER TABLE
- Os BLOBs de `f32` no `memory_embeddings` aceitam QUALQUER fonte com 64 dimensões


## O Que é Embedding e Por Que 64 Dimensões
- Embedding transforma texto em um vetor de números decimais (floats)
- O tamanho do vetor é a dimensionalidade (dims)
- O sqlite-graphrag usa 64 dims por padrão (configurável via `--embedding-dim`)
- 64 dims com MRL (Matryoshka Representation Learning) oferece ~90% da qualidade de 768 dims
- MRL treina os dims em ordem de importância: os PRIMEIROS carregam mais informação
- Truncar de 1024 para 64 preserva a maioria do significado semântico
- TODOS os vetores no banco DEVEM ter a MESMA dimensionalidade (64)
- Misturar dimensionalidades QUEBRA a busca cosine silenciosamente



===

## Modelos Aprovados — API via OpenRouter

### O Que Fazer
- Usar a API REST do OpenRouter para gerar embeddings de 64 dimensões
- Fazer chamada HTTP POST para `https://openrouter.ai/api/v1/embeddings`
- Passar `"dimensions": 64` no body JSON quando o modelo suportar MRL
- Truncar para 64 em Rust (`embedding[..64].to_vec()`) quando o modelo NÃO suportar MRL nativo

### Por Que Fazer
- Latência de ~100-500ms por batch (vs 20-60 segundos com subprocess LLM)
- Speedup de 20-100x sobre o design atual
- API unificada compatível com o formato OpenAI
- Suporta batch (array de strings em uma chamada)
- Um endpoint para múltiplos providers (OpenAI, Qwen, NVIDIA, Google)

### Como Fazer
- Configurar variável de ambiente `OPENROUTER_API_KEY`
- Chamar via `reqwest::Client` async com `rustls-tls` (connection pooling, timeout 30s)
- Parsear o campo `data[0].embedding` da resposta JSON
- Truncar para 64 dims se necessário
- Salvar o `Vec<f32>` no `memory_embeddings` BLOB

### Gestão da API Key do OpenRouter (Padrão XDG)

- A `OPENROUTER_API_KEY` NUNCA fica em arquivo `.env` no CWD
- A chave é armazenada no XDG config do sqlite-graphrag com permissões restritas
- O binário funciona após `cargo install` sem preparo prévio do `.env`

### Hierarquia de Precedência da API Key
- Camada 1: variável de ambiente `OPENROUTER_API_KEY` (CI, containers, scripts)
- Camada 2: arquivo TOML em `~/.config/sqlite-graphrag/config.toml` (persistente)
- Camada 3: flag CLI `--openrouter-api-key <valor>` (pontual, evitar em histórico)
- Camada 1 vence Camada 2, Camada 2 vence Camada 3

### Subcomandos de Gestão da Chave
- `sqlite-graphrag config add-key --provider openrouter --from-stdin` (lê do stdin, evita histórico de shell)
- `sqlite-graphrag config list-keys` (lista chaves mascaradas com fingerprint)
- `sqlite-graphrag config remove-key <fingerprint>` (remove pela fingerprint)
- `sqlite-graphrag config doctor` (diagnostica qual camada venceu)

### Armazenamento no TOML XDG
- Caminho Linux: `~/.config/sqlite-graphrag/config.toml`
- Caminho macOS: `~/Library/Application Support/sqlite-graphrag/config.toml`
- Caminho Windows: `%APPDATA%\sqlite-graphrag\config\config.toml`
- Permissões do arquivo: `chmod 600` (somente o dono lê e escreve)
- Permissões do diretório: `chmod 700` (somente o dono acessa)
- Escrita atômica via `tempfile::NamedTempFile::persist` com `fsync`
- Verificação de symlink antes de ler ou escrever (defesa contra symlink attack)

### Formato do config.toml
- `schema_version = 1`
- `[[keys]]`
- `provider = "openrouter"`
- `value = "sk-or-v1-abc...xyz"` (chave completa, protegida por chmod 600)
- `added_at = "2026-06-25T16:00:00Z"` (timestamp RFC3339)
- `fingerprint = "a1b2c3d4e5f6g7h8"` (blake3 truncado, 16 hex)

### Segurança da Chave em Memória
- A chave em memória usa `secrecy::SecretString` (expõe via `ExposeSecret` apenas no ponto de uso)
- O buffer é zerado ao sair do escopo via `zeroize::ZeroizeOnDrop`
- A chave NUNCA aparece em logs, stderr ou output JSON
- Mascaramento em output: `sk-or...xyz8` (4 primeiros + 4 últimos caracteres)

### Chamada HTTP
- Endpoint: `POST https://openrouter.ai/api/v1/embeddings`
- Header: `Authorization: Bearer $OPENROUTER_API_KEY`
- Header: `Content-Type: application/json`
- Body (MRL nativo): `{"model": "qwen/qwen3-embedding-8b", "input": ["texto"], "dimensions": 64, "encoding_format": "float", "input_type": "search_document"}`
- Body (NVIDIA): `{"model": "nvidia/llama-nemotron-embed-vl-1b-v2:free", "input": ["texto"], "dimensions": 64, "encoding_format": "float", "input_type": "passage"}`
- Body (Mistral sem dims): `{"model": "mistralai/mistral-embed-2312", "input": ["texto"], "encoding_format": "float"}`
- Resposta: `{"data": [{"embedding": [0.23, -0.87, ...64 floats...]}]}`
- `encoding_format`: SEMPRE `"float"` — `"base64"` quebraria o parser f32
- `input_type`: varia por modelo — ver tabela de compatibilidade acima
- `dimensions`: omitido para modelos que rejeitam (Mistral, Perplexity com <128)

### Modelos Verificados via OpenRouter (E2E: 2026-06-25)

- PROIBIDO hardcodar modelo default — o usuário DEVE selecionar via `--embedding-model`
- PROIBIDO usar IDs que nao existem na API — consultar esta lista ANTES de documentar

### Ranking de Qualidade por Recall Score (dim=64 MRL, E2E 2026-06-25)
- google/gemini-embedding-001: 0.892 (MELHOR)
- google/gemini-embedding-2: 0.868
- mistralai/mistral-embed-2312: 0.832
- qwen/qwen3-embedding-8b: 0.814
- qwen/qwen3-embedding-4b: 0.754
- openai/text-embedding-3-small: 0.668
- nvidia/llama-nemotron-embed-vl-1b-v2:free: 0.662
- baai/bge-m3: 0.537
- openai/text-embedding-3-large: 0.449
- perplexity/pplx-embed-v1-0.6b: 0.415
- NOTA: OpenAI large (0.449) performa PIOR que small (0.668) em dim=64 — embeddings de alta dimensionalidade (3072) perdem mais informação ao truncar para 64 dims
- NOTA: TODOS os 10 modelos aceitaram dimensions: 64 nativamente na API, corrigindo informações anteriores sobre limitações de Perplexity e Mistral

### IDs que NAO EXISTEM na API OpenRouter (verificado 2026-06-25)
- `qwen/qwen3-embedding-0.6b` — "No endpoints found" (modelo registrado mas SEM providers ativos)
- `nvidia/llama-3.1-nemotron-embed-8b` — "does not exist"
- `nvidia/llama-nemotron-embed-8b` — "does not exist"
- `nvidia/nemotron-embed-8b` — "does not exist"
- `alibaba/qwen3-embedding-0.6b` — "does not exist"

### Compatibilidade de `input_type` por modelo (verificado E2E)
- Qwen3: aceita `search_document` e `search_query`
- OpenAI: aceita `search_document` e `search_query`
- NVIDIA Nemotron: REJEITA `search_document` — aceita APENAS `query` e `passage`
- Perplexity: aceita `search_document` e `search_query`
- Mistral: aceita `search_document` e `search_query`
- BAAI: aceita `search_document` e `search_query`
- Google Gemini: aceita `search_document` e `search_query`

### Compatibilidade de `dimensions` por modelo (verificado E2E)
- Qwen3 4B/8B: `dimensions: 64` nativo (MRL)
- OpenAI 3-small/3-large: `dimensions: 64` nativo (MRL)
- Google Gemini: `dimensions: 64` nativo (MRL)
- NVIDIA Nemotron VL 1B: `dimensions: 64` nativo (API retorna 64 floats)
- BAAI bge-m3: `dimensions: 64` nativo (API retorna 64 floats)
- Perplexity: dimensions: 64 aceito nativamente pela API (E2E verificado 2026-06-25) — truncamento em Rust NÃO foi necessário
- Mistral: dimensions: 64 aceito nativamente pela API (E2E verificado 2026-06-25) — truncamento em Rust NÃO foi necessário


### Modelos Aprovados (NAO BERT, NAO ONNX)

- Qwen3 Embedding 4B
  - ID API: `qwen/qwen3-embedding-4b`
  - E2E: VERIFICADO OK (2026-06-25) — remember + recall funcionando
  - Preço: $0.02 por milhão de tokens
  - Arquitetura: Qwen3 decoder-only (NAO BERT)
  - MRL: SIM, `dimensions: 64` nativo na API
  - Dimensão nativa: 2560
  - input_type: aceita `search_document` e `search_query`
  - Idiomas: 100+ incluindo português
  - Uso: equilíbrio entre qualidade e custo

- Qwen3 Embedding 8B
  - ID API: `qwen/qwen3-embedding-8b`
  - E2E: VERIFICADO OK (2026-06-25) — remember + recall funcionando
  - Preço: $0.01 por milhão de tokens
  - Arquitetura: Qwen3 decoder-only (NAO BERT)
  - MRL: SIM, `dimensions: 64` nativo na API
  - Dimensão nativa: 4096
  - input_type: aceita `search_document` e `search_query`
  - MTEB: 70.58 (primeiro lugar no leaderboard multilingual, junho 2025)
  - Idiomas: 100+ incluindo português
  - Uso: melhor qualidade disponível para embedding

- NVIDIA Llama Nemotron Embed VL 1B V2
  - ID API: `nvidia/llama-nemotron-embed-vl-1b-v2:free`
  - E2E: VERIFICADO OK (2026-06-25) — remember + recall funcionando
  - Preço: GRATUITO (zero custo)
  - Arquitetura: Llama 3.2 1B decoder-only com SigLip2 400M (NAO BERT)
  - MRL: SIM, `dimensions: 64` nativo na API (corrigido — antes dizia NAO)
  - Dimensão nativa: 2048
  - input_type: REJEITA `search_document` — aceita APENAS `query` e `passage`
  - Capacidade: multimodal (texto + imagem)
  - Uso: prototipação e testes sem custo

- OpenAI text-embedding-3-small
  - ID API: `openai/text-embedding-3-small`
  - E2E: VERIFICADO OK (2026-06-25) — remember + recall funcionando (score 0.24)
  - Preço: $0.02 por milhão de tokens
  - Arquitetura: proprietária OpenAI (NAO BERT)
  - MRL: SIM, `dimensions: 64` nativo na API (mínimo 1)
  - Dimensão nativa: 1536
  - input_type: aceita `search_document` e `search_query`
  - MTEB: 62.26 (referência da indústria)
  - Uso: compatibilidade máxima com ecossistema OpenAI

- OpenAI text-embedding-3-large
  - ID API: `openai/text-embedding-3-large`
  - E2E: VERIFICADO OK (2026-06-25) — remember + recall funcionando
  - Preço: $0.13 por milhão de tokens
  - Arquitetura: proprietária OpenAI (NAO BERT)
  - MRL: SIM, `dimensions: 64` nativo na API (mínimo 1)
  - Dimensão nativa: 3072
  - input_type: aceita `search_document` e `search_query`
  - Uso: máxima qualidade OpenAI quando custo não é restrição

- Perplexity Embed V1 0.6B
  - ID API: `perplexity/pplx-embed-v1-0.6b`
  - E2E: VERIFICADO OK (2026-06-25) — remember + recall funcionando
  - Preço: $0.004 por milhão de tokens (MAIS BARATO)
  - Arquitetura: decoder-only proprietária (NAO BERT)
  - MRL: SIM, dimensions: 64 aceito nativamente pela API (E2E verificado 2026-06-25)
  - Dimensão nativa: 1024
  - input_type: aceita `search_document` e `search_query`
  - Uso: menor custo por token disponível

- Mistral Embed 2312
  - ID API: `mistralai/mistral-embed-2312`
  - E2E: VERIFICADO OK (2026-06-25) — remember + recall funcionando
  - Preço: $0.10 por milhão de tokens
  - Arquitetura: Mistral decoder-only (NAO BERT)
  - MRL: SIM, dimensions: 64 aceito nativamente pela API (E2E verificado 2026-06-25)
  - Dimensão nativa: 1024
  - input_type: aceita `search_document` e `search_query`
  - Uso: ecossistema Mistral existente

- BAAI bge-m3
  - ID API: `baai/bge-m3`
  - E2E: VERIFICADO OK (2026-06-25) — remember + recall funcionando (score 0.16)
  - Preço: ~$0.01 por milhão de tokens
  - Arquitetura: encoder-only multilingual (NAO decoder-only)
  - MRL: SIM, `dimensions: 64` nativo na API
  - Dimensão nativa: 1024
  - input_type: aceita `search_document` e `search_query`
  - Uso: embedding multilingual de alta qualidade

- Google Gemini Embedding 001
  - ID API: `google/gemini-embedding-001`
  - E2E: VERIFICADO OK (2026-06-25) — remember + recall funcionando
  - Preço: ~$0.15 por milhão de tokens
  - Arquitetura: proprietária Google (NAO BERT)
  - MRL: SIM, `dimensions: 64` nativo na API
  - Dimensão nativa: 3072
  - input_type: aceita `search_document` e `search_query`
  - Uso: ecossistema Google

- Google Gemini Embedding 2
  - ID API: `google/gemini-embedding-2`
  - E2E: VERIFICADO OK (2026-06-25) — remember + recall funcionando
  - Preço: ~$0.12 por milhão de tokens
  - Arquitetura: proprietária Google (NAO BERT)
  - MRL: SIM, `dimensions: 64` nativo na API
  - Dimensão nativa: 3072
  - input_type: aceita `search_document` e `search_query`
  - Uso: versão mais recente do ecossistema Google


### Bugs Corrigidos na Auditoria E2E (2026-06-25)

- BUG-OR-1: `input_type="search_document"` hardcoded quebrava NVIDIA Nemotron
  - CAUSA: embedder.rs enviava `Some("search_document")` para TODOS os modelos
  - EFEITO: NVIDIA retornava erro "Unsupported input_type"
  - FIX: `model_default_input_type()` retorna `"passage"` para NVIDIA, `None` para Mistral, `"search_document"` para os demais
  - ARQUIVO: `src/embedding_api.rs` e `src/embedder.rs`

- BUG-OR-2: `model_supports_mrl()` retornava `false` para NVIDIA e BAAI
  - CAUSA: a função verificava apenas Qwen, OpenAI e Gemini
  - EFEITO: NVIDIA e BAAI nao recebiam `dimensions: 64` e a CLI truncava desnecessariamente em Rust
  - FIX: adicionado `llama-nemotron-embed` e `bge-m3` ao check de MRL
  - ARQUIVO: `src/embedding_api.rs`

- BUG-OR-3: `qwen/qwen3-embedding-0.6b` listado como modelo aprovado mas NAO existe na API
  - CAUSA: modelo registrado no OpenRouter mas sem endpoints ativos ("No endpoints found")
  - EFEITO: chamadas retornavam timeout ou 404
  - FIX: removido da lista de modelos aprovados, adicionado à lista de IDs inexistentes

- BUG-OR-4: `nvidia/llama-3.1-nemotron-embed-8b` listado mas NAO existe na API
  - CAUSA: ID incorreto — modelo nao registrado no OpenRouter
  - EFEITO: chamadas retornavam "Model does not exist"
  - FIX: removido da lista de modelos aprovados, adicionado à lista de IDs inexistentes

- BUG-OR-5: HTTP 200 com corpo malformado causava falha imediata sem retry
  - CAUSA: `execute_with_retry()` tratava HTTP 200 como sucesso incondicional e parse error abortava imediatamente
  - EFEITO: quando OpenRouter retornava HTTP 200 com corpo de erro (sem campo `data`), a CLI falhava com exit 11 sem retentar
  - EVIDENCIA: `qwen/qwen3-embedding-4b` falhou 1 de 2 vezes na auditoria E2E com "missing field `data`"
  - FIX: parse error em HTTP 200 agora é tratado como transitório e retentado com backoff exponencial
  - ARQUIVO: `src/embedding_api.rs` linhas 223-239


==


## GAP-OR-INGEST: RESOLVIDO em v1.0.93 — Comando `ingest` agora propaga `EmbeddingBackendChoice` e suporta `--enrich-after`

### Problema
- O comando `ingest` NÃO recebe a flag global `--embedding-backend` da CLI
- O comando `ingest` NÃO recebe a flag global `--embedding-model` da CLI
- O `IngestArgs` NÃO tem campos `embedding_backend` nem `embedding_model`
- O `main.rs` linha 430 passa APENAS `cli.llm_backend` para `ingest::run()`, ignorando `cli.embedding_backend` e `cli.embedding_model`
- O `stage_file()` recebe `LlmBackendChoice` (linha 511) e chama `embed_passage_with_choice()` que aceita APENAS `LlmBackendChoice`
- A função `embed_passage_with_embedding_choice()` (embedder.rs:404) já existe e aceita `EmbeddingBackendChoice`, mas o `ingest` NÃO a utiliza
- O `ingest` NÃO executa enrich sequencial após embedding — o usuário precisa rodar `enrich` manualmente em invocação separada
- PROIBIDO ter modelo default da OpenRouter hardcoded — o usuário DEVE selecionar o modelo de embedding ao digitar o comando

### Consequências do Problema
- O usuário NÃO consegue usar `ingest --embedding-backend openrouter --embedding-model "qwen/qwen3-embedding-8b"` para embedding rápido via API REST
- O `ingest` SEMPRE usa subprocess LLM headless (codex/claude/opencode) para embedding, com cold-start de 5-15s POR ARQUIVO
- Em um ingest de 100 arquivos, o embedding via subprocess consome ~25 minutos (100 × 15s) versus ~20 segundos via API REST OpenRouter (100 × 200ms)
- O grafo de conhecimento fica POBRE após ingest sem enrich — `deep-research` e `graph traverse` retornam poucos resultados
- O usuário precisa lembrar de executar `enrich` manualmente após `ingest` — nenhuma orientação automática é fornecida
- A flag `--embedding-backend openrouter` existe globalmente no CLI mas é silenciosamente IGNORADA pelo `ingest`

### Causa Raiz
- CAUSA 1: Lacuna de propagação no `main.rs` — `cli.embedding_backend` e `cli.embedding_model` NÃO são passados para `ingest::run()`
- CAUSA 2: `IngestArgs` foi definido ANTES da existência de `EmbeddingBackendChoice` (v1.0.79) e NUNCA foi atualizado para incluir as novas flags (v1.0.93)
- CAUSA 3: `stage_file()` recebe `LlmBackendChoice` (parâmetro posicional) e chama `embed_passage_with_choice()` em vez de `embed_passage_with_embedding_choice()`
- CAUSA 4: `embed_passages_parallel_local()` (embedder.rs:994) para chunks múltiplos NÃO tem variante que aceita `EmbeddingBackendChoice` — usa a chain LLM diretamente
- CAUSA 5: o fluxo `ingest` → `enrich` é desacoplado por design (operações independentes), mas NÃO existe mecanismo de enrich sequencial automático após embedding
- CAUSA 6: PROIBIDO ter modelo default da OpenRouter para impedir lock-in acidental — mas a ausência de validação obrigatória permite invocação sem `--embedding-model`

### Solução Proposta
- Propagar `EmbeddingBackendChoice` e `embedding_model` do `main.rs` para `ingest::run()`
- Adicionar parâmetros `embedding_backend` e `embedding_model` ao `stage_file()` 
- Substituir chamada a `embed_passage_with_choice()` por `embed_passage_with_embedding_choice()` no `stage_file()`
- Criar variante `embed_passages_parallel_with_embedding_choice()` para chunks múltiplos que aceita `EmbeddingBackendChoice`
- Adicionar flag `--enrich-after` ao `ingest` para disparar `enrich --operation memory-bindings` automaticamente após a fase de embedding
- VALIDAR que `--embedding-model` é OBRIGATÓRIO quando `--embedding-backend openrouter` — exit 78 se ausente
- PROIBIDO ter modelo default da OpenRouter — forçar o usuário a informar explicitamente

### Benefícios da Solução
- Embedding via API REST OpenRouter reduz tempo de ingest de ~25 minutos para ~20 segundos em 100 arquivos
- Enrich sequencial automático via `--enrich-after` elimina etapa manual esquecida pelo usuário
- Grafo de conhecimento fica RICO após ingest com enrich — `deep-research` e `graph traverse` funcionam plenamente
- Separação embedding (API REST ~200ms) de enrichment (LLM headless ~15s) permite paralelismo sem conflito de slots
- Sem modelo default previne lock-in acidental em modelo específico — o usuário mantém controle total

### Como Solucionar
- PASSO 1: Alterar `ingest::run()` em `src/commands/ingest.rs` para receber `EmbeddingBackendChoice` e `Option<String>` (embedding_model)
- PASSO 2: Alterar `main.rs` linha 430 para passar `cli.embedding_backend` e `cli.embedding_model` para `ingest::run()`
- PASSO 3: Alterar `stage_file()` para receber `EmbeddingBackendChoice` e `LlmBackendChoice` (separados)
- PASSO 4: Substituir `embed_passage_with_choice()` por `embed_passage_with_embedding_choice()` no `stage_file()` linha 647
- PASSO 5: Criar `embed_passages_parallel_with_embedding_choice()` no `embedder.rs` para chunks múltiplos com OpenRouter batch
- PASSO 6: Adicionar validação: se `embedding_backend == Openrouter` e `embedding_model.is_none()` → exit 78 com mensagem clara
- PASSO 7: Adicionar flag `--enrich-after` ao `IngestArgs` (default: false)
- PASSO 8: Quando `--enrich-after` ativo, após conclusão do embedding de TODOS os arquivos, invocar `enrich --operation memory-bindings` com o `--llm-backend` selecionado
- PASSO 9: Emitir evento NDJSON `{"event": "enrich_started"}` no stderr para feedback visual ao usuário
- PASSO 10: Testes — verificar que `ingest --embedding-backend openrouter --embedding-model "qwen/qwen3-embedding-8b" --enrich-after --llm-backend codex` completa embedding + enrich sequencialmente

### Relações Causa x Efeito
- CAUSA: `main.rs` NÃO propaga `embedding_backend` → EFEITO: `ingest` IGNORA flag global silenciosamente
- CAUSA: `IngestArgs` sem campo `embedding_backend` → EFEITO: `stage_file()` recebe APENAS `LlmBackendChoice`
- CAUSA: `stage_file()` chama `embed_passage_with_choice()` → EFEITO: OpenRouter NUNCA é usado para embedding no ingest
- CAUSA: sem `--enrich-after` → EFEITO: usuário ESQUECE de rodar `enrich` manualmente → grafo fica pobre
- CAUSA: sem validação de `--embedding-model` obrigatório → EFEITO: invocação sem modelo pode falhar com erro genérico em vez de mensagem clara
- CAUSA: embedding via subprocess LLM (cold-start 5-15s por arquivo) → EFEITO: ingest de 100 arquivos leva ~25 minutos
- CAUSA: embedding via API REST OpenRouter (~200ms por arquivo) → EFEITO: ingest de 100 arquivos leva ~20 segundos

### Arquivos Afetados
- `src/main.rs` — linha 430: propagar `cli.embedding_backend` e `cli.embedding_model`
- `src/commands/ingest.rs` — `run()` linha 1146: receber `EmbeddingBackendChoice`
- `src/commands/ingest.rs` — `stage_file()` linha 502: receber `EmbeddingBackendChoice`
- `src/commands/ingest.rs` — linhas 647 e 682: substituir funções de embedding
- `src/embedder.rs` — criar `embed_passages_parallel_with_embedding_choice()`
- `src/commands/ingest.rs` — `IngestArgs`: adicionar `--enrich-after`


### Resolução (v1.0.93)
- `main.rs`: propagado `cli.embedding_backend` para TODOS os 8 comandos que usam embedding
- `ingest.rs`: `stage_file()` agora recebe `EmbeddingBackendChoice` e chama `embed_passage_with_embedding_choice()` e `embed_passages_parallel_with_embedding_choice()`
- `embedder.rs`: criadas `embed_passages_parallel_with_embedding_choice()` e `try_embed_query_with_embedding_choice()`
- `IngestArgs`: adicionada flag `--enrich-after` que dispara `enrich --operation memory-bindings` sequencialmente
- Compilação ZERO erros, Clippy ZERO warnings, 986+ testes passando


===


## GAP-OR-PROPAGATION: RESOLVIDO em v1.0.93 — 5 comandos/operações IGNORAM `--embedding-backend openrouter` silenciosamente

### Problema
- O v1.0.93 propagou `EmbeddingBackendChoice` para 8 comandos (remember, remember-batch, ingest, recall, edit, restore, hybrid-search, deep-research)
- 5 comandos/operações que usam embedding CONTINUAM chamando a função OLD `embed_passage_with_choice()` que aceita APENAS `LlmBackendChoice`
- O usuário configura `--embedding-backend openrouter` na linha de comando, mas 5 paths de código IGNORAM esta flag silenciosamente
- NENHUM warning ou erro é emitido quando OpenRouter é ignorado — falha silenciosa

### Consequências do Problema
- `enrich --operation re-embed` com `--embedding-backend openrouter` gera embeddings via subprocess LLM (5-60s por memória) em vez de API REST (~200ms)
- Re-embedding de 500 memórias via `enrich` leva ~4 horas via subprocess versus ~100 segundos via OpenRouter
- `rename-entity` gera embedding de entidade via subprocess LLM mesmo com OpenRouter configurado — latência desnecessária de 5-15s por renomeação
- `init` probe de dimensão SEMPRE usa subprocess LLM — impossível probar dim com OpenRouter
- `ingest --mode claude-code` IGNORA `--embedding-backend openrouter` nos 4 call sites de embedding — todo o pipeline legado fica lento
- `remember` com body > 512KB que gera chunks paralelos usa `embed_passages_parallel_local()` que NÃO suporta OpenRouter batch — perde a vantagem de latência do batch API REST (32 textos por request)
- O usuário confia que `--embedding-backend openrouter` funciona globalmente, mas 5 paths silenciosamente degradam para subprocess LLM

### Causa Raiz
- CAUSA 1 (enrich): `main.rs:507` passa APENAS `cli.llm_backend` para `enrich::run()` — NÃO propaga `cli.embedding_backend`
- CAUSA 2 (enrich): `reembed_memory_vector()` em `enrich.rs:1446` chama `embed_passage_with_choice()` (OLD) que aceita APENAS `Option<LlmBackendChoice>` — a função `embed_passage_with_embedding_choice()` que aceita `EmbeddingBackendChoice` existe mas NÃO é utilizada
- CAUSA 3 (rename-entity): `main.rs:501` passa APENAS `cli.llm_backend` para `rename_entity::run()` — NÃO propaga `cli.embedding_backend`
- CAUSA 4 (rename-entity): `rename_entity.rs:98` chama `embed_passage_with_choice()` (OLD) para gerar embedding do novo nome da entidade
- CAUSA 5 (init): `main.rs:422` chama `init::run(args)` SEM qualquer backend — NÃO propaga `cli.embedding_backend` nem `cli.llm_backend`
- CAUSA 6 (init): `init.rs:132` chama `embed_passage_with_choice(&paths.models, "smoke test", None)` com `None` — SEMPRE usa subprocess LLM default
- CAUSA 7 (ingest_claude): `ingest_claude.rs` tem 4 call sites que chamam `embed_passage_with_choice()` (OLD) — nenhum aceita `EmbeddingBackendChoice`
- CAUSA 8 (remember chunks): `remember.rs:702` chama `embed_passages_parallel_local()` para chunks de body longo — esta função NÃO aceita `EmbeddingBackendChoice` e usa subprocess LLM diretamente
- CAUSA RAIZ COMUM: as 5 lacunas compartilham a mesma causa raiz — a propagação de `EmbeddingBackendChoice` na v1.0.93 cobriu apenas os 8 comandos mais frequentes mas NÃO atualizou os paths secundários que também geram embeddings

### Solução
- SUBGAP 1 (enrich): propagar `cli.embedding_backend` em `main.rs:507` para `enrich::run()` e alterar `reembed_memory_vector()` para chamar `embed_passage_with_embedding_choice()` em vez de `embed_passage_with_choice()`
- SUBGAP 2 (rename-entity): propagar `cli.embedding_backend` em `main.rs:501` para `rename_entity::run()` e alterar `rename_entity.rs:98` para chamar `embed_passage_with_embedding_choice()`
- SUBGAP 3 (init): propagar `cli.embedding_backend` em `main.rs:422` para `init::run()` e alterar `init.rs:132` para chamar `embed_passage_with_embedding_choice()` — probe usa OpenRouter quando disponível
- SUBGAP 4 (ingest_claude): propagar `cli.embedding_backend` para o pipeline `ingest_claude.rs` e substituir os 4 call sites de `embed_passage_with_choice()` por `embed_passage_with_embedding_choice()`
- SUBGAP 5 (remember chunks): alterar `remember.rs:702` para chamar `embed_passages_parallel_with_embedding_choice()` (já existe no `embedder.rs:1083`) em vez de `embed_passages_parallel_local()`

### Benefícios da Solução
- `enrich --operation re-embed` de 500 memórias cai de ~4 horas para ~100 segundos com OpenRouter
- Consistência total: `--embedding-backend openrouter` funciona em TODOS os paths de embedding sem exceção
- Zero falha silenciosa: o usuário confia que a flag global funciona universalmente
- `remember` com body longo usa OpenRouter batch (32 textos por request) — latência de chunks cai de ~30s (2 chunks × 15s subprocess) para ~400ms (1 request batch)
- `init` probe de dimensão funciona com OpenRouter — não requer subprocess LLM instalado para criar banco
- `ingest --mode claude-code` obtém a mesma vantagem de latência de embedding que o modo padrão

### Como Solucionar
- PASSO 1: Alterar `enrich::run()` em `src/commands/enrich.rs` para receber `EmbeddingBackendChoice` como parâmetro adicional
- PASSO 2: Alterar `reembed_memory_vector()` em `enrich.rs:1430` para receber e usar `EmbeddingBackendChoice`
- PASSO 3: Propagar `cli.embedding_backend` em `main.rs:507` para `enrich::run()`
- PASSO 4: Alterar `rename_entity::run()` em `src/commands/rename_entity.rs` para receber `EmbeddingBackendChoice`
- PASSO 5: Propagar `cli.embedding_backend` em `main.rs:501` para `rename_entity::run()`
- PASSO 6: Alterar `init::run()` em `src/commands/init.rs` para receber `EmbeddingBackendChoice` e `LlmBackendChoice`
- PASSO 7: Propagar `cli.embedding_backend` e `cli.llm_backend` em `main.rs:422` para `init::run()`
- PASSO 8: Alterar os 4 call sites em `ingest_claude.rs` para usar `embed_passage_with_embedding_choice()`
- PASSO 9: Alterar `remember.rs:702` para chamar `embed_passages_parallel_with_embedding_choice()` em vez de `embed_passages_parallel_local()`
- PASSO 10: Testes de integração verificando que `--embedding-backend openrouter` funciona em TODOS os 13 paths de embedding (8 existentes + 5 corrigidos)

### Relações Causa x Efeito
- CAUSA: `main.rs` NÃO propaga `embedding_backend` para `enrich::run()` → EFEITO: `enrich --operation re-embed` SEMPRE usa subprocess LLM
- CAUSA: `reembed_memory_vector()` chama função OLD → EFEITO: OpenRouter NUNCA é usado para re-embedding
- CAUSA: `main.rs` NÃO propaga `embedding_backend` para `rename_entity::run()` → EFEITO: renomeação de entidade SEMPRE usa subprocess LLM
- CAUSA: `init::run()` NÃO recebe backends → EFEITO: probe de dimensão SEMPRE usa subprocess LLM default
- CAUSA: `ingest_claude.rs` tem 4 call sites OLD → EFEITO: pipeline legado IGNORA OpenRouter
- CAUSA: `remember` chunks usa `embed_passages_parallel_local()` → EFEITO: body longo NÃO usa OpenRouter batch
- CAUSA RAIZ: propagação parcial de `EmbeddingBackendChoice` na v1.0.93 → EFEITO: 5 paths de embedding ficaram inconsistentes com os 8 paths corrigidos

### Arquivos Afetados
- `src/main.rs` — linhas 422, 501, 507: propagar `cli.embedding_backend` para init, rename-entity, enrich
- `src/commands/enrich.rs` — `run()` linha 1684 e `reembed_memory_vector()` linha 1430: receber e usar `EmbeddingBackendChoice`
- `src/commands/rename_entity.rs` — `run()` e linha 98: receber e usar `EmbeddingBackendChoice`
- `src/commands/init.rs` — `run()` e linha 132: receber e usar `EmbeddingBackendChoice`
- `src/commands/ingest_claude.rs` — 4 call sites de `embed_passage_with_choice()`: substituir por `embed_passage_with_embedding_choice()`
- `src/commands/remember.rs` — linha 702: substituir `embed_passages_parallel_local()` por `embed_passages_parallel_with_embedding_choice()`

### Severidade por Subgap
- SUBGAP 1 (enrich re-embed): ALTA — afeta re-embedding em massa de centenas de memórias
- SUBGAP 2 (rename-entity): BAIXA — operação rara, uma entidade por vez
- SUBGAP 3 (init probe): BAIXA — roda UMA VEZ na criação do banco
- SUBGAP 4 (ingest_claude): MEDIA — modo legado mas ainda em uso para extração de entidades via LLM
- SUBGAP 5 (remember chunks): MEDIA — afeta bodies longos (>512KB) com múltiplos chunks


### Resolução (v1.0.93)
- SUBGAP 1 (enrich): `enrich::run()` agora recebe `EmbeddingBackendChoice`; `reembed_memory_vector()` e `call_reembed()` propagam para `embed_passage_with_embedding_choice()`
- SUBGAP 2 (rename-entity): `rename_entity::run()` agora recebe `EmbeddingBackendChoice`; call site linha 98 migrado
- SUBGAP 3 (init): `init::run()` agora recebe `LlmBackendChoice` e `EmbeddingBackendChoice`; probe usa backend configurado
- SUBGAP 4 (ingest_claude): `run_claude_ingest()` recebe ambos backends; 4 call sites migrados para `embed_passage_with_embedding_choice()`
- SUBGAP 5 (remember chunks): `remember.rs:702` migrado de `embed_passages_parallel_local()` para `embed_passages_parallel_with_embedding_choice()`
- BUG-OR-EXIT-CODE: 3 validações OpenRouter em `main.rs` agora emitem exit 78 (EX_CONFIG) em vez de exit 1
- `main.rs`: propagação de `cli.embedding_backend` para TODOS os 13 comandos de embedding (8 originais + 5 corrigidos)
- Compilação ZERO erros, Clippy ZERO warnings, 1059 testes passando
- E2E: 10/10 modelos OpenRouter validados com TODAS as operações (init, remember, recall, hybrid-search, edit, ingest, enrich re-embed, rename-entity)


====

