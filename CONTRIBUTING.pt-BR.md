Leia este documento em [inglês (EN)](CONTRIBUTING.md).


# Contribuindo para o sqlite-graphrag


## Boas-vindas
- Obrigado por considerar uma contribuição: cada pull request fortalece a memória GraphRAG local
- Suas melhorias afetam diretamente desenvolvedores usando LLMs com memória durável em um único arquivo SQLite
- Código, documentação, testes, relatos de bug e ideias são contribuições igualmente valorizadas
- Este guia mantém seu onboarding em menos de 10 minutos do clone ao primeiro teste local


## Quick Start
- Use este repositório normalmente; o repositório público `sqlite-graphrag` já existe
- Os mesmos comandos de validação valem localmente e no workflow do repositório público
- Nenhum comando deve imprimir erros em um checkout limpo de `main`
```bash
timeout 120 cargo check --all-targets
timeout 300 cargo nextest run --profile ci
RUSTDOCFLAGS="-D warnings" timeout 120 cargo doc --no-deps --all-features
```


## Configuração de Desenvolvimento
### Requisitos de toolchain
- MSRV é Rust 1.88 declarado em `rust-version` dentro de `Cargo.toml`
- JAMAIS aumente o MSRV sem abrir uma issue estilo RFC para discussão antes
- Instale Rust via `rustup` e fixe a toolchain com `rustup default 1.88.0` ao reproduzir CI
### Pinagem de dependências
- Pin direto `constant_time_eq = "=0.4.2"` protege o MSRV 1.88 de drift transitivo via `blake3`
- JAMAIS rode `cargo update` indiscriminadamente; sempre abra PR explicando o bump de versão
- O lockfile `Cargo.lock` DEVE ser commitado porque este repositório entrega um binário CLI
### Requisitos de runtime
- SQLite 3.40 ou mais novo é exigido em runtime devido a `sqlite-vec` e FTS5 external-content
- No Linux você pode precisar de `libssl-dev` e `pkg-config` para algumas dev dependencies transitivas


## Estratégia de Branching
- A branch `main` é protegida e exige pipeline de CI verde para merge
- Branches de feature DEVEM usar o prefixo `feature/<descricao-curta-kebab-case>`
- Branches de correção DEVEM usar o prefixo `fix/<descricao-curta-kebab-case>`
- Branches apenas de documentação DEVEM usar o prefixo `docs/<descricao-curta-kebab-case>`
- Branches de manutenção DEVEM usar o prefixo `chore/<descricao-curta-kebab-case>`


## Convenção de Commits
- Siga a especificação Conventional Commits 1.0.0 em toda mensagem de commit em branches compartilhadas
- Use `feat` para novas funcionalidades visíveis ao usuário
- Use `fix` para correções de bug que entram em main
- Use `perf` para melhorias de performance sem mudança visível de comportamento
- Use `refactor` para reestruturação de código que não adiciona features nem corrige bugs
- Use `docs` para mudanças apenas de documentação
- Use `chore` para ferramentas, CI ou manutenção de repositório
- Use `test` para adicionar ou melhorar testes
- Use `ci` para mudanças no pipeline de CI
- JAMAIS adicione `Co-authored-by` de agentes de IA em mensagens de commit: regra aplicada pelo CI


## Processo de Pull Request
### Antes de abrir o PR
- Faça rebase sobre o `main` mais recente e resolva conflitos localmente
- Mantenha o escopo do PR focado em uma única mudança lógica quando possível
- Escreva uma descrição do PR explicando motivação, mudança e eventuais trade-offs
### Checklist de Validação do PR
- [ ] `cargo check --all-targets` passa com zero erros
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passa com zero warnings
- [ ] `cargo fmt --all --check` passa com zero diferenças
- [ ] `cargo doc --no-deps --all-features` com `RUSTDOCFLAGS="-D warnings"` executa limpo
- [ ] `cargo nextest run --profile ci` executa a suíte padrão com sucesso
- [ ] `cargo llvm-cov nextest --profile heavy --features slow-tests --summary-only` mantém cobertura no mínimo 80 por cento
- [ ] `cargo audit` reporta zero vulnerabilidades
- [ ] `cargo deny check advisories licenses bans sources` passa com zero violações


## Testes
- Execute a suíte padrão com `cargo nextest run --profile ci` para o runner rápido alinhado ao CI
- Execute a suíte lenta separadamente com `cargo nextest run --profile heavy --features slow-tests`
- Meça a cobertura de auditoria profunda com `cargo llvm-cov nextest --profile heavy --features slow-tests --summary-only`
- Mantenha o piso da cobertura de auditoria profunda em 80 por cento ou acima
- Testes unitários vivem dentro de blocos `#[cfg(test)] mod tests` no próprio arquivo de implementação
- Testes de integração vivem em `tests/` e DEVEM usar `assert_cmd` mais `wiremock` para mocks HTTP
- A flag oculta `--skip-memory-guard` existe exclusivamente para testes que não alocam memória real
- Trate `init`, `remember`, `recall` e `hybrid-search` como comandos heavy-memory durante validação manual
- Inicie a validação de comandos pesados com `--max-concurrency 1` e só aumente após medir RSS e comportamento de swap
- JAMAIS emita requisições HTTP reais nem toque caminhos reais fora de um `TempDir` em testes
- Execute `cargo test --lib lock::tests retry::circuit_breaker_tests` após modificar `lock.rs` ou `retry.rs` para exercitar os novos helpers de singleton e circuit breaker da v1.0.68
- Execute `cargo test --test terminal_compile_windows` após modificar `src/terminal.rs` para confirmar que a superfície pública continua chamável; o job dedicado de CI `windows-build-check` roda a checagem completa de tipos cross-platform
- Asserções de teste envolvendo timestamps DEVEM ser timezone-agnostic — parseie ISO via `chrono::DateTime::parse_from_rfc3339` e compare `timestamp()` contra `DateTime::UNIX_EPOCH` em vez de strings hardcoded `1970-01-01T00:00:00`; esta regra foi adicionada depois de um vazamento de `SQLITE_GRAPHRAG_DISPLAY_TZ` em v1.0.66/v1.0.67 que tornou três testes pré-existentes flaky


## Documentação
- Toda API pública DEVE ter doc comments `///` com pelo menos um exemplo testável quando razoável
- Rode `cargo doc --no-deps --all-features` com `RUSTDOCFLAGS="-D warnings"` localmente antes do push
- Regras de formatação de documentação estão em `docs_rules/rules_rust_documentacao.md`
- README, CONTRIBUTING, SECURITY e CODE_OF_CONDUCT bilíngues DEVEM permanecer sincronizados entre EN e pt-BR
- Ao adicionar ou modificar comandos CLI, atualize a documentação em AMBOS os arquivos em inglês e português (ex.: `README.md` e `README.pt-BR.md`, `docs/HOW_TO_USE.md` e `docs/HOW_TO_USE.pt-BR.md`)
- Atualize o CHANGELOG na seção Unreleased a cada mudança visível ao usuário


## Como Reportar Bugs
- Abra uma issue usando o template Bug Report no GitHub
- Inclua caso de reprodução mínimo, idealmente em menos de 20 linhas de invocação ou código
- Inclua o output de `cargo --version` e `rustc --version`
- Inclua seu SO, arquitetura, versão do SQLite e versão do sqlite-graphrag
- Inclua o comando exato rodado, o output observado e o output esperado


## Como Solicitar Funcionalidades
- Abra uma issue usando o template Feature Request no GitHub
- Descreva o caso de uso concreto e quem se beneficia; evite formato abstrato de lista de desejos
- Descreva pelo menos uma alternativa considerada e por que não atendeu
- Referencie qualquer seção do PRD upstream ou issue relacionada quando aplicável


## Processo de Release
- Mantenedores ajustam `version` em `Cargo.toml` seguindo Semantic Versioning 2.0.0
- Mantenedores atualizam o CHANGELOG movendo entradas Unreleased sob a nova versão com data ISO
- Mantenedores taggeiam o commit de release como `vX.Y.Z` usando `git tag -a vX.Y.Z -m "Release vX.Y.Z"`
- Empurrar a tag dispara `.github/workflows/release.yml` que constrói artefatos de release e assets do GitHub Release
- Publicação final no crates.io é feita manualmente com `cargo publish --locked`

## Releases Recentes
### v1.0.76 - 2026-06-07 — Apenas LLM One-Shot, Credencial LLM Apenas OAuth
- **MUDANÇA ARQUITETURAL QUEBRANTE**: o build padrão não embute mais nenhum modelo local. Toda geração de embedding, NER e busca vetorial delega para `claude -p` ou `codex exec` headless (OAuth, sem MCP, sem hooks). A CLI é one-shot. Binário cai de 39 MB para ~6 MB.
- **Crates removidos**: `fastembed 5.13.4`, `ort 2.0.0-rc.12`, `ndarray 0.16`, `tokenizers 0.22`, `huggingface-hub 0.4`, `sqlite-vec 0.1.9`
- **Features removidas**: `daemon` (como otimização de performance, mantido para compatibilidade de fonte até v1.1.0), caminho `--enable-ner` GLiNER ONNX (movido para feature `ner-legacy`)
- **Adicionado**: trait `ExtractionBackend` com `LlmBackend` / `EmbeddingBackend` / `NoneBackend` / `CompositeBackend`; trait `VersionAdapter` com `CodexAdapter` / `ClaudeAdapter` / `OpencodeAdapter`; `migrate --rehash` e `migrate --to-llm-only --drop-vec-tables`; tabelas BLOB-backed `memory_embeddings` / `entity_embeddings` / `chunk_embeddings`; cosseno em Rust puro em `src/similarity.rs`; fluxo de credencial LLM OAuth-only com aborto `AppError::Validation` quando `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estão no env
- **Migração V013** dropa as virtual tables `vec_memories` / `vec_entities` / `vec_chunks`; embeddings antigos são recomputados lazy na próxima escrita
- **Matriz CI**: `default` e `llm-only` desde a v1.0.79 (`embedding-legacy` removida); mock LLM CLI cabeada em 26 arquivos de teste; 107/115 testes previamente lentos corrigidos
- **7 novos ADRs**: `adr-0019-llm-only-one-shot`, `adr-0020-pure-rust-cosine`, `adr-0021-deprecate-daemon`, `adr-0022-blob-embeddings`, `adr-0023-remove-tokenizers`, `adr-0024-fts5-coarse-cosine-refine`, `adr-0025-oauth-only-embedding`; todos com traduções PT-BR
- **2 novos schemas JSON**: `migrate-rehash.schema.json`, `migrate-to-llm-only.schema.json`
- **3 novos docs**: `docs/HOW_TO_USE.md`, `docs/MIGRATION.md`, `docs/AGENTS.md` (e PT-BR) para a arquitetura v1.0.76 LLM-Only
- **1 novo doc**: `docs/HEADLESS_INVOCATION.md` (e PT-BR) cobrindo invocação headless OAuth-safe de Claude/Codex/OpenCode
- 745 testes de lib passam, 0 falham, 3 ignorados; `cargo clippy --all-targets --all-features -- -D warnings` zero warnings
- Veja `gaps.md` para o histórico completo de resolução e `CHANGELOG.pt-BR.md` para a entrada v1.0.76

### v1.0.68 - 2026-06-03 — Governança de Ciclo de Vida de Processos e Correção de Compilação Windows
- **G28-A** Isolamento de servidores MCP via `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` (subprocesso recebe `CLAUDE_CONFIG_DIR=<diretório vazio>`; `--strict-mcp-config` e `--mcp-config '{}'` são ignorados upstream conforme anthropics/claude-code#10787)
- **G28-B** `lock::acquire_job_singleton(JobType, namespace, wait_seconds)` mais `AppError::JobSingletonLocked { job_type, namespace }` (exit 75) integrado em `enrich`, `ingest --mode claude-code` e `ingest --mode codex` para prevenir proliferação de processos contra o mesmo banco
- **G28-D** Helper `retry::CircuitBreaker` com `AttemptOutcome::{Success, Transient, HardFailure}`; erros rate-limited e timeout são explicitamente excluídos da contagem de falhas; `enrich` emite `tracing::warn!` quando `--llm-parallelism > 4`
- **G29** `src/terminal.rs` reescrito com `!handle.is_null() && handle != INVALID_HANDLE_VALUE` para que `cargo install sqlite-graphrag` compile no Windows; `windows-sys` fixado em `=0.59.0` exato; novo job de CI `windows-build-check` roda `cargo check --target x86_64-pc-windows-msvc --lib --all-features` em todo push
- **Correções de Testes** três falhas pré-existentes de timezone-leak em `src/commands/{history,list,read}.rs` corrigidas via `chrono::DateTime::parse_from_rfc3339` + comparação com `DateTime::UNIX_EPOCH`
- **Documentação** novos ADRs `adr-008-process-lifecycle-singleton`, `adr-009-windows-sys-handle-pinning`, `adr-010-mcp-isolation-claude-config-dir`; `SKILL.md` EN+PT, `AGENTS.md` EN+PT, `llms.txt`, `llms.pt-BR.txt`, `llms-full.txt`, `INTEGRATIONS.md` EN+PT, `MIGRATION.md` EN+PT, `TESTING.md` EN+PT, `HOW_TO_USE.md` EN+PT, `CROSS_PLATFORM.md` EN+PT, `COOKBOOK.md` EN+PT atualizados com a seção v1.0.68; `docs/schemas/error-envelope.schema.json` atualizado para documentar o segundo template `code: 75`
- **CI** novo job `windows-build-check`; job `language-check` mantido do release anterior
- 692 testes de lib + 2 testes de integração passam; 0 warnings em `clippy -- -D warnings` e `cargo doc --no-deps --all-features` com `RUSTDOCFLAGS="-D warnings"`
- Veja `gaps.md` para o histórico de resolução completo e `CHANGELOG.pt-BR.md` para a entrada v1.0.68

## Checklist Obrigatório Pré-Push (desde v1.0.68)
- [ ] `cargo fmt --all --check` está limpo
- [ ] `cargo check --all-targets` passa
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` reporta zero warnings
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features` reporta zero warnings
- [ ] `cargo test --lib` reporta 692 passed, 0 failed
- [ ] `cargo test --test terminal_compile_windows` reporta 2 passed
- [ ] Título do PR está em inglês e segue Conventional Commits (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`, `ci:`, `build:`, `perf:`)
- [ ] Sem trailer `Co-authored-by: ...` para qualquer agente de IA (Claude, Codex, GPT, Copilot, Cursor, Gemini, Anthropic, OpenAI)
- [ ] Entradas de CHANGELOG adicionadas sob `[Sem Versão]` em AMBOS `CHANGELOG.md` e `CHANGELOG.pt-BR.md`
- [ ] Se tocar em `windows-sys` ou qualquer crate de FFI, rode `cargo check --target x86_64-pc-windows-msvc --lib --all-features` localmente
- [ ] Se tocar em `lock.rs` ou `retry.rs`, rode `cargo test --lib lock::tests retry::circuit_breaker_tests`


## Reconhecimento
- Contribuidores são creditados no CHANGELOG ao lado da versão que entregou sua mudança
- Contribuidores também são listados em cada release note do GitHub quando a contribuição é visível
- JAMAIS adicione trailers `Co-authored-by` de agentes de IA em qualquer commit ou descrição de PR


## Dúvidas
- Abra uma GitHub Discussion para questões de design ou temas amplos não ligados a issue específica
- Use Security Advisories para qualquer coisa que se pareça com questão de segurança; veja SECURITY.md
