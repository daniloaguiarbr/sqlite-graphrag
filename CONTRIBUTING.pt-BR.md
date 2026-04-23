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
timeout 300 cargo nextest run --all-features
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
- [ ] `cargo nextest run --all-features` executa todos os testes com sucesso
- [ ] `cargo llvm-cov --text` mantém cobertura no mínimo 80 por cento
- [ ] `cargo audit` reporta zero vulnerabilidades
- [ ] `cargo deny check advisories licenses bans sources` passa com zero violações


## Testes
- Execute a suíte completa com `cargo nextest run --all-features` para runner rápido com isolamento
- Meça cobertura com `cargo llvm-cov --text` e mantenha cobertura em 80 por cento ou acima
- Testes unitários vivem dentro de blocos `#[cfg(test)] mod tests` no próprio arquivo de implementação
- Testes de integração vivem em `tests/` e DEVEM usar `assert_cmd` mais `wiremock` para mocks HTTP
- A flag oculta `--skip-memory-guard` existe exclusivamente para testes que não alocam memória real
- Trate `init`, `remember`, `recall` e `hybrid-search` como comandos heavy-memory durante validação manual
- Inicie a validação de comandos pesados com `--max-concurrency 1` e só aumente após medir RSS e comportamento de swap
- JAMAIS emita requisições HTTP reais nem toque caminhos reais fora de um `TempDir` em testes


## Documentação
- Toda API pública DEVE ter doc comments `///` com pelo menos um exemplo testável quando razoável
- Rode `cargo doc --no-deps --all-features` com `RUSTDOCFLAGS="-D warnings"` localmente antes do push
- Regras de formatação de documentação estão em `docs_rules/rules_rust_documentacao.md`
- README, CONTRIBUTING, SECURITY e CODE_OF_CONDUCT bilíngues DEVEM permanecer sincronizados entre EN e pt-BR
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


## Reconhecimento
- Contribuidores são creditados no CHANGELOG ao lado da versão que entregou sua mudança
- Contribuidores também são listados em cada release note do GitHub quando a contribuição é visível
- JAMAIS adicione trailers `Co-authored-by` de agentes de IA em qualquer commit ou descrição de PR


## Dúvidas
- Abra uma GitHub Discussion para questões de design ou temas amplos não ligados a issue específica
- Use Security Advisories para qualquer coisa que se pareça com questão de segurança; veja SECURITY.md
