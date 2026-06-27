# Gap — Enrichment Não Usa Modelos de Texto OpenRouter via REST


## Problema
- O `enrich` só extrai entidades via CLI headless spawnada
- Os três modos são `claude-code`, `codex` e `opencode`
- TODOS exigem um subprocesso local instalado e autenticado
- O OpenRouter já fornece embedding, MAS nunca o enrichment
- Falta rotear o JUDGE do enrich para `/chat/completions` REST
- Modelos como `gpt-oss-120b` e `gemini-3.1-flash-lite` ficam inacessíveis
- A operação `re-embed` já usa OpenRouter, o que expõe a assimetria


## Consequências do Problema
- O usuário depende de instalar codex, claude ou opencode
- Sem CLI local, o enrichment fica TOTALMENTE indisponível
- Cold-start de processo adiciona latência a cada item
- O paralelismo fica limitado pelo custo de spawnar processos
- Não há escolha de modelo por preço, velocidade ou contexto
- A confiabilidade depende de parsear stdout do subprocesso
- O ecossistema de modelos OpenRouter fica desperdiçado no chat


## Causa Raiz do Problema
- O JUDGE foi implementado SOMENTE como spawn de subprocesso
- O enum `EnrichMode` em `src/commands/enrich.rs:331` lista só três modos
- As variantes são `ClaudeCode`, `Codex` e `Opencode`
- O dispatch em `src/commands/enrich.rs:673` chama `claude_runner::run_claude`
- Os caminhos seguintes usam `codex_spawn` e `opencode_runner`
- TODOS resolvem em `Command::new`, ou seja, processo externo
- O único cliente HTTP OpenRouter vive em `src/embedding_api.rs`
- A constante `OPENROUTER_EMBEDDINGS_URL` em `embedding_api.rs:14` é fixa
- Esse cliente fala APENAS com o endpoint `/embeddings`
- Não existe cliente REST de chat para `/chat/completions`
- Logo, o transporte REST do OpenRouter cobre só embedding


## Relações Causa e Efeito
- JUDGE só sabe spawnar processo CAUSA dependência de CLI local
- Ausência de cliente chat REST CAUSA bloqueio do OpenRouter no enrich
- `EnrichMode` sem variante OpenRouter CAUSA falta de rota REST
- Cold-start de subprocesso CAUSA latência alta por item
- Parsear stdout do processo CAUSA fragilidade na saída
- A assimetria embed-tem, chat-não-tem CAUSA confusão de capacidade


## Solução
- Adicionar um cliente REST de chat para o OpenRouter
- Espelhar a infra existente do `OpenRouterClient` de embedding
- Reusar `reqwest`, `secrecy` e o retry já presentes
- Enviar prompt e schema via `response_format` com `json_schema`
- Adicionar a variante `OpenRouter` ao enum `EnrichMode`
- Rotear o JUDGE para o novo cliente quando `--mode openrouter`
- A trait `ExtractionBackend` em `src/extract/mod.rs:99` é agnóstica
- A saída JSON cai nas mesmas structs de entidade e relação


## Benefícios da Solução
- Enrichment funciona SEM nenhuma CLI local instalada
- O usuário escolhe modelo por preço, velocidade e contexto
- Structured Outputs nativo é mais confiável que parsear stdout
- Sem cold-start de processo, a latência por item cai
- O paralelismo cresce sem o custo de spawnar processos
- A paridade com a operação `re-embed` fica restaurada
- A lista de modelos OpenRouter passa a servir o chat


## Como Solucionar
- Criar `OpenRouterChatClient` espelhando `embedding_api.rs`
- Definir constante `OPENROUTER_CHAT_URL` para `/chat/completions`
- Montar payload com `model`, `messages` e `response_format`
- Usar `response_format` do tipo `json_schema` com `strict` true
- Passar o schema fixo de `src/commands/enrich.rs` no `json_schema`
- Adicionar `provider.require_parameters` true para rotear correto
- DESABILITAR `reasoning` em `memory-bindings` e `entity-descriptions`
- Manter `reasoning` opcional apenas em `body-enrich`
- Adicionar variante `OpenRouter` ao enum `EnrichMode`
- Adicionar braço de dispatch para o novo modo no `enrich`
- Ler a chave via `OPENROUTER_API_KEY` com `secrecy` e zeroize
- NUNCA passar a chave como argumento de linha de comando
- Validar `response_format` por modelo antes de fixar em produção
- Apostas seguras iniciais — `gpt-oss-120b`, `gemini-3.1-flash-lite`, `deepseek-v4-flash`


## Risco e Calibração de Certeza
- Suporte a `json_schema` varia por provider no OpenRouter
- Modelos muito novos podem não honrar Structured Outputs
- A ÚNICA prova confiável é testar cada modelo contra a API
- `body-enrich` tolera mais modelos por ser texto livre
- `memory-bindings` exige suporte real a schema rígido
- Confirmado via doc oficial OpenRouter e crate `openrouter-rs`


## Trade-off Que Decide
- CLI headless usa OAuth do usuário, enrich SEM custo de token
- OpenRouter chat cobra tokens na `OPENROUTER_API_KEY`
- A troca é OAuth grátis por velocidade e confiabilidade
- A decisão final é econômica, não técnica


## Validação Obrigatória da Futura Correção
- `cargo fmt --all --check` — ZERO diffs
- `cargo clippy --all-targets --all-features -- -D warnings` — ZERO warnings
- `cargo test --all-features` — ZERO falhando
- `cargo doc --no-deps --all-features` — ZERO warnings
- Teste real contra `/chat/completions` com schema mínimo
- Cobertura mínima de 80% para o código novo
