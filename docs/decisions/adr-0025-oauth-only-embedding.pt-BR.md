# ADR-0025: Fluxo de Credencial LLM Apenas OAuth (v1.0.76 — herdado da v1.0.69)

- Status: Aceito (reafirmado em 2026-06-07)
- Atualização (v1.0.79): a válvula de escape `embedding-legacy` mencionada abaixo foi removida antecipando o cronograma da v1.1.0; a janela de transição está fechada
- Decisores: Danilo Aguiar
- Escopo: src/extract/llm_embedding.rs, src/commands/claude_runner.rs, src/commands/codex_spawn.rs

## Contexto

O ADR-0011 (v1.0.69) estabeleceu fluxo de credencial apenas OAuth para as CLIs claude / codex. A v1.0.76 é o primeiro release onde OAuth é o ÚNICO fluxo suportado também para o cliente de embedding, porque o cliente de embedding agora spawna claude / codex diretamente (sem fallback fastembed).

As flags de endurecimento da v1.0.69 são preservadas:

Para `claude code` (7 flags):

```
--strict-mcp-config
--mcp-config '{}'
--settings '{"hooks":{}}'
--dangerously-skip-permissions
--output-schema '{"type":"object","properties":{"embedding":{...}},"required":["embedding"],"additionalProperties":false}'
--model claude-sonnet-4-6
-p <prompt>
```

Para `codex` (7 flags + whitelist do ChatGPT Pro OAuth):

```
--json
--output-schema '{"type":"object",...}'
--ephemeral
--skip-git-repo-check
--sandbox read-only
--ignore-user-config
--ignore-rules
-c mcp_servers='{}'
--ask-for-approval never
--model gpt-5.4
```

OAuth é imposto pelo check `oauth_only_enforce()` em `src/extract/llm_embedding.rs`, que ABORTA com `AppError::Validation` se `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estiver no ambiente. As duas env vars de API key também são excluídas da whitelist de env-clear em `claude_runner::build_claude_command` e `codex_spawn::build_codex_command`, então mesmo um processo pai que as exporta não pode burlar a checagem.

## Consequências

### Positivas

- A CLI não pode ser enganada a enviar embeddings ou conteúdo extraído para um endpoint de terceiros por um atacante que controla a env var `ANTHROPIC_API_KEY`. Credenciais OAuth são amarradas à assinatura do próprio usuário (Claude Pro/Max ou ChatGPT Pro).
- O fluxo OAuth fornece visibilidade de billing por requisição (a página da conta do usuário mostra cada round-trip LLM), então operadores podem auditar seu próprio gasto com LLM.
- As flags de endurecimento tornam o subprocesso LLM determinístico (sem servidores MCP, sem hooks, sem config de usuário) então as respostas de embedding são reproduzíveis entre hosts.

### Negativas

- Operadores que querem usar um provedor de LLM diferente (Azure OpenAI, Bedrock, Ollama local) não podem fazê-lo sem modificar `src/extract/llm_embedding.rs` para adicionar um novo caminho de spawn. Isso é intencional; o build v1.0.76 está comprometido com claude e codex apenas.
- Operadores sem acesso à internet no host que roda sqlite-graphrag não podem usar o backend LLM. A feature `embedding-legacy` restaura o pipeline local fastembed para uso offline durante a janela de transição.

## Verificação

- `cargo test --lib extract::llm_embedding::tests::oauth_only_enforce_blocks_api_keys`: verde — o check de env var dispara quando qualquer chave está setada.
- `cargo test --lib extract::llm_embedding::tests::flavour_as_str_is_stable`: verde — o enum EmbeddingFlavour serializa corretamente.
- `claude_runner.rs::tests::*` e `codex_spawn.rs::tests::*`: o conjunto canônico de 7 flags é preservado em todos os caminhos de spawn.
