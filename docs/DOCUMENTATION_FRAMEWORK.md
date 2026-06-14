# Documentation Framework — Prompt Rules for Replication

> Regras imperativas invioláveis para replicar o framework de documentação deste projeto em qualquer outro projeto Rust CLI ou software open-source


## Visão Geral do Framework

- Este framework define 3 camadas de documentação: RAIZ, DOCS e SKILL
- CADA camada tem arquivos obrigatórios, estrutura definida e objetivo específico
- TODOS os arquivos de documentação seguem o padrão bilíngue EN/PT-BR
- A camada RAIZ comunica com humanos (desenvolvedores, contribuidores, usuários)
- A camada DOCS comunica com humanos avançados (integradores, operadores, testadores)
- A camada SKILL comunica com máquinas (agentes de IA, LLMs, pipelines de automação)


## Princípio Bilíngue Inviolável

### OBRIGATÓRIO — Espelhamento 1:1
- CADA arquivo `.md` na raiz DEVE ter seu par `.pt-BR.md` espelhado
- CADA arquivo `.md` na pasta `docs/` DEVE ter seu par `.pt-BR.md` espelhado
- CADA arquivo `.txt` de LLM DEVE ter seu par `.pt-BR.txt` espelhado
- CADA pasta em `skill/` DEVE ter variante `-en` e variante `-pt`
- NUNCA publique arquivo de documentação sem seu par bilíngue
- NUNCA misture idiomas dentro do mesmo arquivo
- NUNCA traduza automaticamente sem revisão humana

### OBRIGATÓRIO — Cross-Reference Entre Idiomas
- CADA arquivo EN DEVE conter link para versão PT-BR na primeira linha útil
- CADA arquivo PT-BR DEVE conter link para versão EN na primeira linha útil
- Formato EN: `Read this document in [Portuguese (pt-BR)](ARQUIVO.pt-BR.md).`
- Formato PT-BR: `Leia este documento em [inglês (EN)](ARQUIVO.md).`
- POSICIONE o link ANTES de qualquer conteúdo substantivo

### OBRIGATÓRIO — Convenção de Nomes
- Versão inglês: `NOME.md` (nome canônico sem sufixo)
- Versão português: `NOME.pt-BR.md` (sufixo `.pt-BR` antes da extensão)
- Versão inglês TXT: `nome.txt`
- Versão português TXT: `nome.pt-BR.txt`
- NUNCA use `NOME-en.md` ou `NOME_EN.md` para a versão inglês
- NUNCA use `NOME-pt.md` sem o `-BR` completo


## Camada 1 — Pasta Raiz (18 arquivos MD + 2 pares de templates + 3 licenças + 4 configs)

### OBRIGATÓRIO — Inventário Completo da Raiz — Documentação Bilíngue
- `README.md` + `README.pt-BR.md` — Porta de entrada do projeto
- `CHANGELOG.md` + `CHANGELOG.pt-BR.md` — Histórico de mudanças por versão
- `CONTRIBUTING.md` + `CONTRIBUTING.pt-BR.md` — Guia de contribuição
- `CODE_OF_CONDUCT.md` + `CODE_OF_CONDUCT.pt-BR.md` — Código de conduta
- `SECURITY.md` + `SECURITY.pt-BR.md` — Política de segurança e vulnerabilidades
- `INTEGRATIONS.md` + `INTEGRATIONS.pt-BR.md` — Catálogo de integrações externas
- `llms.txt` + `llms.pt-BR.txt` — Resumo compacto para agentes de IA (llms.txt standard)
- `llms-full.txt` — Versão expandida do llms.txt com documentação completa inline (EN-only)
- `gaps.md` — Relatório de acceptance testing com gaps identificados (EN-only)

### OBRIGATÓRIO — Arquivos de Licença na Raiz
- `LICENSE` — Arquivo de licença principal (symlink ou dual-license notice)
- `LICENSE-MIT` — Texto completo da licença MIT
- `LICENSE-APACHE` — Texto completo da licença Apache 2.0
- DEVE usar licença dual `MIT OR Apache-2.0` como padrão Rust community
- DEVE incluir AMBOS os textos de licença como arquivos separados
- NUNCA omita arquivos de licença — crates.io e GitHub dependem deles

### OBRIGATÓRIO — Arquivos de Configuração Documentais na Raiz
- `Cargo.toml` — Manifesto do projeto com metadados em inglês
- `Cross.toml` — Configuração de cross-compilation
- `deny.toml` — Política de supply chain e licenças
- `rust-toolchain.toml` — Pinning de toolchain Rust

### Objetivo de Cada Arquivo da Raiz

#### README.md + README.pt-BR.md
- OBJETIVO: primeira impressão do projeto para qualquer visitante
- DEVE conter badge cluster com 5 badges: crates.io, docs.rs, CI, licença, Contributor Covenant
- DEVE conter hero tagline em blockquote com 15 palavras ou menos
- DEVE conter seção "What is it?" com 6 bullets técnicos
- DEVE conter seção "Why?" com diferencial em 3-4 bullets
- DEVE conter seção "Quick Start" com 4 comandos ou menos
- DEVE conter tabelas de comandos agrupadas por família
- DEVE conter tabela de variáveis de ambiente
- DEVE conter seção "Integration Patterns" com exemplos pipeable
- DEVE conter seção "Exit Codes" com tabela numérica
- DEVE conter seção "Troubleshooting FAQ" com 3-5 problemas
- DEVE conter link para CHANGELOG, nunca changelog inline
- DEVE conter seção "Contributing" apontando para CONTRIBUTING.md
- DEVE conter seção "Security" apontando para SECURITY.md
- DEVE conter seção "License" com identificador SPDX
- NUNCA exceda 900 linhas por versão de idioma
- ESTRUTURA README segue modelo AIDA (Atenção, Interesse, Desejo, Ação)

#### CHANGELOG.md + CHANGELOG.pt-BR.md
- OBJETIVO: registro cronológico reverso de todas as mudanças por versão
- DEVE seguir formato Keep a Changelog (https://keepachangelog.com/en/1.1.0/)
- DEVE agrupar por: Added, Changed, Fixed, Removed, Security, Deprecated
- DEVE incluir data de release em formato ISO 8601
- DEVE incluir número de arquivos alterados por release
- DEVE incluir contagem de bugs corrigidos e features novas no heading
- NUNCA omita uma versão publicada do changelog
- NUNCA registre mudanças internas invisíveis ao usuário

#### CONTRIBUTING.md + CONTRIBUTING.pt-BR.md
- OBJETIVO: onboarding de novos contribuidores com fluxo completo
- DEVE conter seção "Welcome" com tom inclusivo
- DEVE conter seção "Quick Start" com passos de setup
- DEVE conter seção "Development Setup" com requisitos de toolchain
- DEVE conter seção "Branching Strategy" com convenção de branches
- DEVE conter seção "Commit Convention" com formato de mensagens
- DEVE conter seção "Pull Request Process" com checklist de validação
- DEVE conter seção "Testing" com comandos de teste
- DEVE conter seção "Documentation" com política de docs
- DEVE conter seção "How to Report Bugs" com template
- DEVE conter seção "How to Request Features" com template
- DEVE conter seção "Release Process" com fluxo de publicação
- NUNCA exceda 150 linhas por versão de idioma

#### CODE_OF_CONDUCT.md + CODE_OF_CONDUCT.pt-BR.md
- OBJETIVO: estabelecer padrões de comportamento da comunidade
- DEVE adotar Contributor Covenant 2.1 como base
- DEVE conter badge do Contributor Covenant
- DEVE conter informações de contato para reportar violações
- DEVE conter seções de escopo, enforcement e atribuição
- NUNCA modifique o texto padrão do Contributor Covenant sem justificativa

#### SECURITY.md + SECURITY.pt-BR.md
- OBJETIVO: canal de comunicação para vulnerabilidades de segurança
- DEVE conter seção "Supported Versions" com tabela de versões ativas
- DEVE conter seção "Reporting a Vulnerability" com instruções claras
- DEVE conter seção "Response SLA" com tempos de resposta
- DEVE conter seção "Fix SLA by CVSS Severity" com prazos por gravidade
- DEVE conter seção "Disclosure Policy" com política de divulgação
- DEVE conter seção "Best Practices for Users" com orientações
- NUNCA exceda 80 linhas por versão de idioma

#### INTEGRATIONS.md + INTEGRATIONS.pt-BR.md
- OBJETIVO: catálogo completo de plataformas, agentes e ferramentas compatíveis
- DEVE conter tabela sumária com todas as integrações
- DEVE conter seção dedicada por integração com: nome, tipo de agente, método de integração
- DEVE conter exemplos de configuração para cada integração
- DEVE cobrir: agentes de IA, IDEs, CI/CD, containers, package managers, shells
- DEVE agrupar integrações por categoria (agentes, IDEs, CI/CD, etc.)
- NUNCA liste integração sem exemplo funcional

#### llms.txt + llms.pt-BR.txt
- OBJETIVO: resumo compacto otimizado para descoberta por agentes de IA
- SEGUE o padrão llms.txt (https://llmstxt.org/)
- DEVE conter título H1 com nome do projeto
- DEVE conter blockquote hero com proposta de valor em uma frase
- DEVE conter parágrafo de abertura com números concretos (agentes, tamanho, latência)
- DEVE conter seção "Primary Documentation" com links para docs principais
- DEVE conter seção "Core Commands" com lista completa de subcomandos
- DEVE conter seção "Environment Variables" com todas as variáveis
- DEVE conter seção "Exit Codes" com tabela numérica
- DEVE conter seção "Stable Facts" com fatos verificáveis e estáveis
- NUNCA exceda 150 linhas
- NUNCA inclua detalhes de implementação interna
- TRATE este arquivo como cartão de visita do projeto para LLMs

#### llms-full.txt
- OBJETIVO: documentação completa inline para contexto expandido de LLMs
- DEVE conter TODA a informação do README + HOW_TO_USE + COOKBOOK condensados
- DEVE ser autocontido — um LLM DEVE conseguir operar o projeto lendo APENAS este arquivo
- DEVE incluir Quick Start, todos os comandos, variáveis de ambiente, padrões de integração
- DEVE incluir exemplos de uso para cada comando principal
- PODE exceder 500 linhas quando necessário para completude
- NUNCA exija leitura de arquivo externo para operar o projeto
- VERSÃO única em inglês (sem par PT-BR) — inglês é lingua franca de LLMs

#### gaps.md
- OBJETIVO: relatório de acceptance testing com gaps identificados por versão
- DEVE conter resultado agregado (X/Y PASS + N FINDINGs)
- DEVE conter versão do binário e estado do banco de produção
- DEVE conter cada gap com: classificação de severidade (HIGH, MEDIUM, LOW)
- CADA gap DEVE conter seções: Problem, Consequences, Root Cause, Solution, Benefits, How to Resolve
- DEVE ser atualizado a cada release com nova rodada de acceptance testing
- VERSÃO única em inglês — documento técnico interno


## Camada 2 — Pasta docs/ (14 arquivos MD + subpasta schemas/)

### OBRIGATÓRIO — Inventário Completo da Pasta docs/
- `docs/AGENTS.md` + `docs/AGENTS.pt-BR.md` — Guia completo para integração com agentes de IA
- `docs/COOKBOOK.md` + `docs/COOKBOOK.pt-BR.md` — Receitas práticas de produção
- `docs/CROSS_PLATFORM.md` + `docs/CROSS_PLATFORM.pt-BR.md` — Suporte cross-platform
- `docs/HOW_TO_USE.md` + `docs/HOW_TO_USE.pt-BR.md` — Guia de uso completo
- `docs/MIGRATION.md` + `docs/MIGRATION.pt-BR.md` — Guia de migração entre versões
- `docs/TESTING.md` + `docs/TESTING.pt-BR.md` — Guia de testes e estratégia de QA
- `docs/HEADLESS_INVOCATION.md` + `docs/HEADLESS_INVOCATION.pt-BR.md` — Referência canônica de invocação headless OAuth-safe (adicionado na v1.0.76)
- `docs/DOCUMENTATION_FRAMEWORK.md` — Este próprio framework (versão única EN, referencia regras de PT-BR indiretamente)
- `docs/schemas/README.md` — Índice e documentação dos JSON Schemas (bilíngue inline)
- `docs/schemas/*.schema.json` — Um schema JSON Draft 2020-12 por subcomando
- `docs/decisions/adr-NNNN-*.md` — Architectural Decision Records (ADRs) documentando decisões de design v1.0.x

### Mudanças na Camada 2 a Partir da v1.0.76
- Adicionados `docs/HEADLESS_INVOCATION.md` + versão PT-BR (promovidos do gaps.md)
- 2 novos schemas JSON para `migrate --rehash` e `migrate --to-llm-only`
- `docs/AGENTS.md` ganhou seção "v1.0.76 Architecture (LLM-Only)" e "OAuth Enforcement"
- `docs/TESTING.md` ganhou seção "v1.0.76 Test Infrastructure — 3-Feature CI Matrix"
- `docs/COOKBOOK.md` ganhou receita "How To Upgrade From v1.0.74 Or v1.0.75 To v1.0.76"
- `docs/MIGRATION.md` reescrito do zero para a breaking change v1.0.76
- `docs/HOW_TO_USE.md` reescrito do zero para LLM-Only One-Shot
- 7 novos ADRs (0019-0025) cobrindo a arquitetura v1.0.76, todos com versão PT-BR
- ADR 0026 documenta o drift de migração V002 (PT-BR incluso)

### Mudanças na Camada 2 a Partir da v1.0.77
- ADR-0027 documenta a correção do G40 (`applied_on = NULL` bloqueava migrações), com versão PT-BR
- `docs/schemas/migrate-rehash.schema.json` atualizado com campo `null_rows_fixed`
- `docs/schemas/migrate-to-llm-only.schema.json` atualizado com campos `null_rows_fixed` e `vec_tables_removed_via_writable_schema`
- `docs/schemas/debug-schema.schema.json` atualizado: `applied_on` agora aceita `null` (tipo `["string", "null"]`)
- `docs/AGENTS.md` ganhou seção "New in v1.0.77" cobrindo o G40 fix
- `docs/TESTING.md` ganhou seção "v1.0.77 Test Additions — G40 Fix Coverage"
- `docs/COOKBOOK.md` ganhou subseção "v1.0.77 Fix" na receita de upgrade
- `docs/MIGRATION.md` ganhou seção "MIGRATING TO v1.0.77 — G40 Fix" no topo

### Mudanças na Camada 2 a Partir da v1.0.78
- ADR-0028 documenta a correção do G41 (`run_rehash` registrava V013 sem executar SQL), com versão PT-BR
- `docs/schemas/migrate-rehash.schema.json` atualizado com campo `v013_tables_created`
- `docs/schemas/migrate-to-llm-only.schema.json` atualizado com campo `v013_tables_created`
- `docs/AGENTS.md` ganhou seção "New in v1.0.78" cobrindo o G41 fix
- `docs/AGENTS.pt-BR.md` ganhou seção "Novidades na v1.0.78" cobrindo o G41 fix
- `docs/TESTING.md` ganhou seção "v1.0.78 Test Additions — G41 Fix Coverage"
- `docs/TESTING.pt-BR.md` ganhou seção correspondente em português
- `docs/COOKBOOK.md` ganhou subseção "v1.0.78 Fix" na receita de upgrade
- `docs/COOKBOOK.pt-BR.md` ganhou subseção correspondente em português
- `docs/MIGRATION.md` ganhou seção "MIGRATING TO v1.0.78 — G41 Phantom V013 Registration Fix" no topo
- `docs/MIGRATION.pt-BR.md` ganhou seção correspondente em português
- `README.md` e `README.pt-BR.md` atualizados para "Current release: v1.0.78"
- `docs/MIGRATION.pt-BR.md` ganhou seção correspondente em português
- `README.md` e `README.pt-BR.md` atualizados para "Current release: v1.0.78"

### Mudanças na Camada 2 a Partir da v1.0.79
- ADR-0019-0026 cobertos na seção anterior (v1.0.76/v1.0.77/v1.0.78)
- Pipeline de embedding LLM fechado pelo G42 (S1 dim configurável, S2 batching, S3 bounded parallelism, S4 tempfile RAII, S5 modelo env, S6 empty CLAUDE_CONFIG_DIR, S7 codex headless actionable, S8 panic-free signal handler, S9 canonical re-embed)
- G43 dim-adoption em `open_rw` e `open_ro`; mocks de teste reescritos para 64 dims + batch schema
- G44 dim-adaptive batch size via `clamp(base×64/dim, 1, base)`
- G50 CI vermelho fechado: 6 causas (doctest, mock inline, benchmark LLM, language policy, race de dim, deny obsoleto)
- G51 mocks LLM extraem dim do prompt para testes multi-dim
- G52 `vec stats` ganhou `dims: [{table, dim, rows}]`; schema fiel ao binário
- G47 flags documentadas inexistentes: aliases visíveis para `--type` em `edit` e `--entity-type` em `reclassify`
- G48 G20 não cegava `--max-hops` igual ao default (Option<T>)
- G49 `SQLITE_GRAPHRAG_EMBEDDING_DIM` inválido emite `tracing::warn!`
- Daemon infrastructure e features legadas (`embedding-legacy`, `ner-legacy`, `full`) totalmente removidas
- `docs/AGENTS.md` e `docs/AGENTS.pt-BR.md` ganharam seções "v1.0.79" cobrindo G42-G52 e a remoção do daemon
- `docs/TESTING.md` e `docs/TESTING.pt-BR.md` ganharam seções "v1.0.79 Test Additions"
- `docs/COOKBOOK.md` e `docs/COOKBOOK.pt-BR.md` ganharam subseções "v1.0.79 Fix" nas receitas de upgrade
- `docs/MIGRATION.md` ganhou receita de re-embed com `enrich --operation re-embed --limit N --resume` (substituindo a receita quebrada `edit --description`)
- `README.md` e `README.pt-BR.md` atualizados para "Current release: v1.0.79"

### Mudanças na Camada 2 a Partir da v1.0.80
- **ADR-0032 (G53, v1.0.80) — Library API Stability Policy**: CLI é contrato estável; API da biblioteca é instável em v1.x.y. Consumidores da lib devem fixar `=1.0.80`; bump de patch é estritamente aditivo na superfície da lib. Documentado em `docs/decisions/adr-0032-g53-lib-api-policy.md` e em `docs/decisions/adr-0032-g53-lib-api-policy.pt-BR.md`
- **ADR-0033 (G53-WINDOWS-INFRA, v1.0.80) — Windows CI Resilience**: jobs `clippy` e `test` da matrix windows-2025 ganharam steps de pre-warm e verify gateados em `if: matrix.os == 'windows-2025'`. Os 2 modos históricos de falha de infra (rustup download transitório e `E0463` por stdlib ausente) agora são recuperáveis na primeira re-run. Documentado em `docs/decisions/adr-0033-g53-windows-infra-resilience.md` e versão PT-BR
- **ADR-0034 (SHUTDOWN Resilience, v1.0.80) — Panic-Free Third-Signal Exit**: `src/signals.rs` é envolvido em uma barreira de captura de panic; o terceiro Ctrl-C consecutivo sai com código 130 e ZERO I/O. Receita canônica de bypass SHUTDOWN em 3 camadas (`nohup` → `setsid` → `disown`) documentada em `docs/HEADLESS_INVOCATION.md` e `docs/COOKBOOK.md`. Documentado em `docs/decisions/adr-0034-shutdown-resilience.md` e versão PT-BR
- `docs/MIGRATION.md` e `docs/MIGRATION.pt-BR.md` ganharam seção "MIGRATING TO v1.0.80" no topo (sem migração de banco, apenas bump de versão e nota sobre pin de lib)
- `docs/CROSS_PLATFORM.md` e `docs/CROSS_PLATFORM.pt-BR.md` ganharam subseção "CI Windows Infra Resilience (G53-WINDOWS-INFRA, ADR-0033, v1.0.80)" após a seção HANDLE
- `README.md` e `README.pt-BR.md` ganharam bullet "Upgrading from v1.0.79 to v1.0.80?" e badge "Current release: v1.0.80"
- `CHANGELOG.md` e `CHANGELOG.pt-BR.md` ganharam entradas para G45 (cross-process embedding singleton), G53 (stability policy + semver-checks CI), G55 S2 (MemoryNotFound estrutural), G56 (entity-embed cache), G58 (FTS5 fallback), G53-WINDOWS-INFRA e SHUTDOWN resilience



### Objetivo e Entrega de Cada Arquivo da Pasta docs/

#### docs/AGENTS.md + docs/AGENTS.pt-BR.md
- OBJETIVO: guia exaustivo para agentes de IA consumirem o projeto como ferramenta
- DEVE conter hero tagline idêntica ao README
- DEVE conter seção "Why Agents Love This CLI" com benefícios de máquina
- DEVE conter seção "Compatible Agents and Orchestrators" com lista completa
- DEVE conter seção "Agent Integration Details" com exemplos por agente
- DEVE conter TODA a referência de CRUD (Create, Read, Update, Delete)
- DEVE conter TODA a referência de pesquisa (recall, hybrid-search, related, graph traverse, deep-research)
- DEVE conter referência de grafo (link, unlink, entities, stats, traverse)
- DEVE conter referência de manutenção (comandos `cache` e `daemon` removidos na v1.0.76; código restante do daemon deletado na v1.0.79)
- DEVE conter contrato JSON completo com campos por comando
- DEVE conter exit codes com estratégia de retry
- DEVE conter seção de concorrência e recursos
- DEVE ser AUTOCONTIDO — um agente DEVE operar o projeto lendo APENAS este arquivo
- NUNCA exija leitura de outro arquivo para completude operacional
- ENTREGA: um agente de IA CONSEGUE usar o projeto end-to-end lendo apenas AGENTS.md

#### docs/COOKBOOK.md + docs/COOKBOOK.pt-BR.md
- OBJETIVO: receitas práticas prontas para copiar e executar
- DEVE conter seção "CLI Flag Aliases" com tabela de aliases
- DEVE conter seção "Default Values Reference" com valores padrão
- CADA receita DEVE seguir formato "How To [Verbo] [Objeto] [Contexto]"
- CADA receita DEVE conter bloco de código executável copiar-colar
- CADA receita DEVE ser independente das demais
- DEVE cobrir: bootstrap, ingest, search, graph, integração com agentes, backup, export, debug
- DEVE incluir receitas de integração para cada agente suportado
- DEVE incluir receitas de operações avançadas (merge, rename, reclassify, prune)
- ENTREGA: um operador RESOLVE qualquer tarefa comum copiando uma receita

#### docs/CROSS_PLATFORM.md + docs/CROSS_PLATFORM.pt-BR.md
- OBJETIVO: documentar suporte e particularidades de cada plataforma
- DEVE conter tabela de targets suportados com status
- DEVE conter instruções de instalação por plataforma
- DEVE conter particularidades de runtime por OS (subprocesso LLM, musl, ARM64)
- DEVE conter seção de CI/CD com matrix de targets
- ENTREGA: um desenvolvedor CONFIGURA build e CI para qualquer target lendo este arquivo

#### docs/HOW_TO_USE.md + docs/HOW_TO_USE.pt-BR.md
- OBJETIVO: guia narrativo de uso do início ao domínio completo
- DEVE conter hero tagline com proposta de valor
- DEVE conter links de navegação para README e outros docs
- DEVE cobrir: instalação, inicialização, operações CRUD, busca, grafo
- DEVE seguir progressão de complexidade crescente
- DEVE incluir exemplos com saída esperada
- ENTREGA: um novo usuário SAI operando o projeto após ler este arquivo

#### docs/MIGRATION.md + docs/MIGRATION.pt-BR.md
- OBJETIVO: guia de migração entre versões ou nomes do projeto
- DEVE conter tabela "What Changes" com antes/depois
- DEVE conter instruções passo-a-passo de migração
- DEVE conter seção de rollback para caso de problemas
- DEVE conter breaking changes com impacto e solução
- ENTREGA: um usuário MIGRA entre versões sem perda de dados lendo este arquivo

#### docs/TESTING.md + docs/TESTING.pt-BR.md
- OBJETIVO: guia de estratégia de testes e como executar a suíte
- DEVE conter motivação para categorização de testes
- DEVE conter categorias de teste (unitário, integração, contrato, E2E)
- DEVE conter comandos exatos para executar cada categoria
- DEVE conter política de cobertura mínima
- DEVE conter instruções para adicionar novos testes
- DEVE conter seção "Test Matrix" com a matriz CI de features vigente (`default` e `llm-only` desde a v1.0.79; `embedding-legacy` removida)
- DEVE conter o contrato da Mock LLM CLI para rodar testes sem credenciais OAuth reais
- ENTREGA: um contribuidor ESCREVE e EXECUTA testes seguindo este guia

#### docs/HEADLESS_INVOCATION.md + docs/HEADLESS_INVOCATION.pt-BR.md
- OBJETIVO: referência canônica de invocação headless OAuth-safe de Claude Code, Codex CLI e OpenCode
- DEVE conter tabela comparativa dos interruptores de MCP e Hooks por CLI
- DEVE conter os comandos exatos de hardening flags para cada CLI
- DEVE conter seção de "Por Que NÃO Usar `--bare`" para Claude
- DEVE conter ressalvas de bugs conhecidos (issue #14490 do Claude, issue #3441 do Codex)
- DEVE ser referenciado em `docs/HOW_TO_USE.md` e `docs/AGENTS.md` para usuários finais
- ENTREGA: um operador invoca LLM headless sem herdar MCPs ou hooks lendo este arquivo

#### docs/schemas/README.md
- OBJETIVO: índice e documentação de todos os JSON Schemas do projeto
- DEVE ser bilíngue inline (seção EN seguida de seção PT-BR no mesmo arquivo)
- DEVE conter tabela mapeando subcomando para arquivo de schema
- DEVE conter seção de seleção de schema por modo de ingestão
- DEVE conter seção de schemas de input (payloads de entrada)
- DEVE conter seção de uso com exemplos de validação
- DEVE conter seção de comportamento de flags
- DEVE conter garantia de estabilidade (política SemVer de schemas)
- ENTREGA: um integrador VALIDA qualquer output do CLI contra o schema correto

#### docs/schemas/*.schema.json
- OBJETIVO: contrato formal de cada subcomando em JSON Schema Draft 2020-12
- DEVE haver exatamente UM arquivo `.schema.json` por subcomando ou evento NDJSON
- DEVE usar `"additionalProperties": false` em todos os schemas
- DEVE documentar TODOS os campos obrigatórios e opcionais
- NOME do arquivo DEVE ser kebab-case do nome do subcomando: `nome-comando.schema.json`
- SUBCOMANDOS com modos DEVEM ter schemas separados por modo: `ingest-file-event.schema.json` vs `ingest-claude-file-event.schema.json`
- DEVE incluir schema de error envelope: `error-envelope.schema.json`
- DEVE incluir schemas de input: `entities-input.schema.json`, `relationships-input.schema.json`
- ENTREGA: qualquer parser ou agente VALIDA output do CLI programaticamente


## Camada 3 — Pasta skill/ (2 pastas, 2 arquivos SKILL.md)

### OBRIGATÓRIO — Inventário Completo da Pasta skill/
- `skill/<nome-projeto>-en/SKILL.md` — Skill de instrução para agentes de IA em inglês
- `skill/<nome-projeto>-pt/SKILL.md` — Skill de instrução para agentes de IA em português

### OBRIGATÓRIO — Estrutura de Diretório da Skill
- CADA idioma em pasta separada com sufixo `-en` ou `-pt`
- DENTRO de cada pasta, exatamente UM arquivo chamado `SKILL.md`
- NOME da pasta segue padrão: `<nome-do-projeto>-<idioma>`
- NUNCA misture idiomas na mesma pasta
- NUNCA nomeie o arquivo diferente de `SKILL.md`

### OBRIGATÓRIO — Estrutura do Arquivo SKILL.md
- DEVE iniciar com YAML frontmatter delimitado por `---`
- Frontmatter DEVE conter campo `name:` com nome do projeto
- Frontmatter DEVE conter campo `description:` com texto de trigger para agentes de IA
- O campo `description` DEVE ser otimizado para matching por LLMs — incluir sinônimos, keywords, nomes de agentes, cenários de uso
- O campo `description` DEVE incluir condições de auto-invocação mesmo sem menção explícita
- Após o frontmatter, o corpo DEVE conter TODA a referência operacional do projeto
- O corpo DEVE usar estrutura de headings H2/H3 com labels imperativas (REQUIRED, FORBIDDEN, Correct Pattern)

### OBRIGATÓRIO — Conteúdo do SKILL.md
- DEVE conter seção "Fundamental Principles" com filosofia de uso
- DEVE conter seção "Initialization and Health Check" com bootstrap
- DEVE conter seção "Global Configuration" com todas as variáveis e flags
- DEVE conter TODAS as operações CRUD documentadas individualmente
- DEVE conter TODAS as operações de pesquisa (search, recall, traverse)
- DEVE conter referência de grafo (link, unlink, entities, stats)
- DEVE conter gerenciamento de entidades (delete, rename, reclassify, merge)
- DEVE conter contrato JSON completo com campos críticos por comando
- DEVE conter exit codes com estratégia de retry
- DEVE conter seção de concorrência e recursos
- DEVE conter seção de manutenção e backup
- DEVE ser AUTOCONTIDO — injetado como system prompt, o agente DEVE operar sem ler mais nada

### OBRIGATÓRIO — Linguagem Imperativa do SKILL.md
- USE headings H3 com prefixo de categoria: `### REQUIRED —`, `### FORBIDDEN —`, `### Correct Pattern —`
- USE bullets iniciando com VERBO IMPERATIVO em MAIÚSCULAS: `USAR`, `NUNCA`, `EXECUTAR`, `TRATAR`
- USE negações absolutas: `NUNCA`, `JAMAIS`, `PROIBIDO`
- USE afirmações absolutas: `SEMPRE`, `OBRIGATÓRIO`, `DEVE`
- NUNCA use linguagem sugestiva ("considere", "talvez", "recomendado")
- NUNCA use voz passiva
- CADA bullet DEVE ser uma regra independente e atômica

### Objetivo e Entrega do SKILL.md
- OBJETIVO: transformar qualquer agente de IA em operador competente do projeto
- PÚBLICO: LLMs e agentes de IA (Claude Code, Codex, Cursor, Windsurf, etc.)
- FORMATO: markdown com YAML frontmatter, otimizado para injeção em system prompts
- ENTREGA: um agente de IA que recebe SKILL.md como contexto OPERA o projeto end-to-end sem assistência humana


## Relação Entre as 3 Camadas

### OBRIGATÓRIO — Hierarquia de Completude
- Camada 1 (RAIZ): informações de alto nível, onboarding, governança do projeto
- Camada 2 (DOCS): documentação técnica profunda, receitas, guias operacionais
- Camada 3 (SKILL): instrução máquina-para-máquina, autocontida e imperativa

### OBRIGATÓRIO — Progressão de Audiência
- README.md → qualquer visitante (30 segundos para entender o projeto)
- AGENTS.md → integrador técnico (opera o projeto via agente de IA)
- COOKBOOK.md → operador avançado (resolve tarefas específicas via receitas)
- SKILL.md → agente de IA (opera o projeto autonomamente sem humano)

### OBRIGATÓRIO — Regra de Autocontenção
- README.md DEVE ser suficiente para decidir se o projeto é relevante
- AGENTS.md DEVE ser suficiente para integrar o projeto com qualquer agente
- COOKBOOK.md DEVE ser suficiente para resolver qualquer tarefa operacional
- HOW_TO_USE.md DEVE ser suficiente para um novo usuário operar o projeto
- SKILL.md DEVE ser suficiente para um agente de IA operar o projeto
- llms-full.txt DEVE ser suficiente para um LLM entender o projeto completamente
- NENHUM arquivo DEVE exigir leitura de outro para cumprir seu objetivo primário

### OBRIGATÓRIO — Sobreposição Intencional
- README.md, AGENTS.md, COOKBOOK.md, SKILL.md e llms-full.txt PODEM repetir informação
- A repetição É INTENCIONAL — cada arquivo serve audiência diferente em contexto diferente
- NUNCA substitua conteúdo por "veja arquivo X" quando a audiência-alvo pode não ter acesso ao arquivo X
- SEMPRE prefira redundância sobre referência cruzada em documentos autocontidos


## Convenções de Formatação

### OBRIGATÓRIO — Headings
- H1 (`#`) SOMENTE para título do documento (uma vez por arquivo)
- H2 (`##`) para seções principais
- H3 (`###`) para subseções com prefixo de categoria
- NUNCA use H4 ou inferior — reestruture a hierarquia
- NUNCA use heading sem conteúdo abaixo

### OBRIGATÓRIO — Hero Tagline
- CADA documento DEVE ter blockquote hero após H1
- Formato: `> proposta de valor em 15 palavras ou menos`
- POSICIONE imediatamente após badges (se houver) e antes de qualquer conteúdo

### OBRIGATÓRIO — Badges (apenas README)
- MÍNIMO 5 badges: crates.io, docs.rs, CI, licença, Contributor Covenant
- POSICIONE imediatamente após H1
- USE formato shields.io para uniformidade
- ORDEM: registry, docs, CI, licença, código de conduta

### PROIBIDO — Formatação
- NUNCA use emojis em documentação técnica
- NUNCA use negrito com asteriscos duplos para ênfase
- NUNCA use separador horizontal de três hífens (`---`) exceto em frontmatter
- NUNCA use HTML inline em markdown
- NUNCA use imagens sem alt-text descritivo

### OBRIGATÓRIO — Estilo de Escrita
- CADA bullet DEVE ter entre 8 e 15 palavras
- USE verbos no imperativo
- ELIMINE advérbios e conectores parasitas
- SUBSTITUA "pode" por "entrega", "garante", "elimina"
- SUBSTITUA "é recomendado" por DEVE ou SEMPRE
- SUBSTITUA "evite" por PROIBIDO ou JAMAIS
- USE números concretos em vez de qualificadores vagos


## Omissões Detectadas no Projeto Modelo — Gaps Estruturais

### STATUS LEGADO — Gaps identificados e corrigidos em versões anteriores
- As três omissões abaixo foram DETECTADAS e CORRIGIDAS antes do v1.0.68
- Mantidas aqui como referência histórica do que o framework exige
- Projetos novos DEVEM satisfazer as três regras desde o primeiro release
- Esta seção NÃO descreve o estado atual do projeto; o estado atual está em `gaps.md`

### STATUS LEGADO — README.md e README.pt-BR.md NÃO continham cross-reference bilíngue
- O README.md NÃO continha link para README.pt-BR.md na primeira linha útil
- O README.pt-BR.md NÃO continha link para README.md na primeira linha útil
- TODOS os outros pares bilíngues (CONTRIBUTING, SECURITY, etc.) já continham o cross-reference
- REGRA: README.md DEVE conter `Read this document in [Portuguese (pt-BR)](README.pt-BR.md).` após badges
- REGRA: README.pt-BR.md DEVE conter `Leia este documento em [inglês (EN)](README.md).` após badges
- CORREÇÃO aplicada no projeto modelo antes do v1.0.68

### STATUS LEGADO — INTEGRATIONS.md e INTEGRATIONS.pt-BR.md NÃO continham cross-reference bilíngue
- O INTEGRATIONS.md NÃO continha link para INTEGRATIONS.pt-BR.md
- O INTEGRATIONS.pt-BR.md NÃO continha link para INTEGRATIONS.md
- REGRA: INTEGRATIONS.md DEVE conter `Read this document in [Portuguese (pt-BR)](INTEGRATIONS.pt-BR.md).`
- REGRA: INTEGRATIONS.pt-BR.md DEVE conter `Leia este documento em [inglês (EN)](INTEGRATIONS.md).`
- CORREÇÃO aplicada no projeto modelo antes do v1.0.68

### STATUS LEGADO — Ausência de GitHub Issue e PR Templates
- O projeto NÃO continha `.github/ISSUE_TEMPLATE/` com templates de bug report e feature request
- O projeto NÃO continha `.github/PULL_REQUEST_TEMPLATE.md` com checklist de PR
- REGRA: TODO projeto open-source DEVE conter templates de issue e PR no GitHub
- CORREÇÃO aplicada no projeto modelo antes do v1.0.68 — ver `gaps.md` entrada de resolução v1.0.68


## Camada Auxiliar — CI/CD Workflows (.github/workflows/)

### OBRIGATÓRIO — Inventário de Workflows
- `.github/workflows/ci.yml` — Pipeline de validação em push e PR
- `.github/workflows/release.yml` — Pipeline de build e publicação em tags `v*`
- NUNCA publique release sem workflow de CI passando
- NUNCA publique sem workflow de release automatizado

### OBRIGATÓRIO — ci.yml
- DEVE executar: fmt, clippy, test, doc, audit, deny em matrix multi-OS
- DEVE incluir job `msrv` para validar MSRV declarado
- DEVE incluir job `language-check` para auditoria de idioma no código
- DEVE incluir job `commit-check` para bloquear Co-authored-by de agentes

### OBRIGATÓRIO — release.yml
- DEVE triggerar em tags `v*`
- DEVE incluir: validate, build-matrix, publish-github-release, publish-crates-io
- DEVE gerar binários para: linux-gnu, linux-musl, macos-arm64, macos-x86, windows-msvc
- DEVE gerar SHA256SUMS.txt para verificação de integridade


## Camada Auxiliar — Pastas de Suporte

### OBRIGATÓRIO — Pasta migrations/
- DEVE conter migrações SQL versionadas para projetos com banco de dados
- FORMATO de nome: `V<NNN>__<descricao_snake_case>.sql`
- NUMERAÇÃO sequencial sem gaps
- CADA migração DEVE ser idempotente ou com rollback documentado

### OBRIGATÓRIO — Pasta scripts/
- DEVE conter scripts auxiliares de desenvolvimento e auditoria
- NOMEIE scripts em inglês com kebab-case ou snake_case
- DOCUMENTE propósito de cada script no primeiro comentário

### OBRIGATÓRIO — Pasta benches/
- DEVE conter benchmarks com `criterion` para projetos com requisitos de performance
- NOMEIE benchmarks em inglês com snake_case
- INCLUA benchmark de regressão como baseline


## Padrões de Cross-Reference Entre Arquivos

### OBRIGATÓRIO — README Aponta para Docs
- README.md DEVE conter links para: CONTRIBUTING.md, SECURITY.md, CHANGELOG.md
- README.md DEVE conter seção "JSON Schemas" apontando para docs/schemas/README.md
- README.md DEVE conter seção "Contributing" apontando para CONTRIBUTING.md
- README.md DEVE conter seção "Security" apontando para SECURITY.md

### OBRIGATÓRIO — Docs Apontam para README
- CADA arquivo em docs/ DEVE conter link de volta ao README.md principal
- Formato: `Return to the main [README.md](../README.md) for command reference`
- POSICIONE após hero tagline e cross-reference de idioma

### OBRIGATÓRIO — CHANGELOG Formato de Heading por Release
- Formato: `## [X.Y.Z] - YYYY-MM-DD`
- DEVE incluir seção `[Unreleased]` no topo para mudanças em progresso
- Subseções: `### Added`, `### Changed`, `### Fixed`, `### Removed`, `### Security`, `### Deprecated`
- NUNCA altere heading de release já publicada

### OBRIGATÓRIO — llms.txt Aponta para Docs Primários
- DEVE conter seção "Primary Documentation" com links para:
  - README.md no repositório GitHub
  - docs/AGENTS.md no repositório GitHub
  - docs/COOKBOOK.md no repositório GitHub
  - docs/HOW_TO_USE.md no repositório GitHub
- DEVE usar URLs absolutas do GitHub, não caminhos relativos


## Checklist de Conformidade para Novos Projetos

### OBRIGATÓRIO — Antes do Primeiro Release
- [x] LICENSE + LICENSE-MIT + LICENSE-APACHE criados com textos completos
- [x] README.md + README.pt-BR.md criados com todas as seções obrigatórias e 5 badges
- [x] CHANGELOG.md + CHANGELOG.pt-BR.md criados com formato Keep a Changelog
- [x] CONTRIBUTING.md + CONTRIBUTING.pt-BR.md criados com fluxo completo
- [x] CODE_OF_CONDUCT.md + CODE_OF_CONDUCT.pt-BR.md criados com Contributor Covenant 2.1
- [x] SECURITY.md + SECURITY.pt-BR.md criados com SLAs definidas
- [x] INTEGRATIONS.md + INTEGRATIONS.pt-BR.md criados com catálogo inicial
- [x] llms.txt + llms.pt-BR.txt criados com resumo compacto
- [x] llms-full.txt criado com documentação inline completa
- [x] gaps.md criado com primeira rodada de acceptance testing
- [x] docs/AGENTS.md + docs/AGENTS.pt-BR.md criados com referência autocontida
- [x] docs/COOKBOOK.md + docs/COOKBOOK.pt-BR.md criados com receitas iniciais
- [x] docs/CROSS_PLATFORM.md + docs/CROSS_PLATFORM.pt-BR.md criados com targets
- [x] docs/HOW_TO_USE.md + docs/HOW_TO_USE.pt-BR.md criados com guia narrativo
- [x] docs/MIGRATION.md + docs/MIGRATION.pt-BR.md criados (mesmo que vazio para v1)
- [x] docs/TESTING.md + docs/TESTING.pt-BR.md criados com estratégia de testes
- [x] docs/schemas/README.md criado bilíngue inline com índice de schemas
- [x] docs/schemas/*.schema.json criados para cada subcomando com saída JSON
- [x] skill/<projeto>-en/SKILL.md criado com referência operacional completa
- [x] skill/<projeto>-pt/SKILL.md criado espelhando versão EN
- [x] .github/workflows/ci.yml criado com pipeline de validação multi-OS
- [x] .github/workflows/release.yml criado com pipeline de publicação em tags
- [x] .github/ISSUE_TEMPLATE/ criado com templates de bug e feature request
- [x] .github/PULL_REQUEST_TEMPLATE.md criado com checklist de validação
- [x] TODOS os cross-references entre idiomas verificados em TODOS os pares
- [x] NENHUM arquivo de documentação sem par bilíngue
- [x] NENHUM README ou INTEGRATIONS sem link para versão no outro idioma

### OBRIGATÓRIO — Quando o Checklist Está 100% Concluído
- MARQUE cada item como `[x]` no checklist acima
- A remoção de qualquer item só é permitida quando ele vira legado documentado em `gaps.md`
- Projetos que herdam o template DEVEM copiar o checklist já marcado como ponto de partida
- ADICIONE novos itens quando o framework ganhar regras; nunca remova itens marcados como concluídos

### OBRIGATÓRIO — A Cada Release
- [ ] CHANGELOG.md + CHANGELOG.pt-BR.md atualizados com mudanças da versão
- [ ] README.md + README.pt-BR.md atualizados se houver novos comandos ou variáveis
- [ ] docs/AGENTS.md + docs/AGENTS.pt-BR.md atualizados se houver mudanças de contrato JSON
- [ ] docs/COOKBOOK.md + docs/COOKBOOK.pt-BR.md atualizados se houver novas receitas
- [ ] docs/HOW_TO_USE.md + docs/HOW_TO_USE.pt-BR.md atualizados com novas flags e subcomandos
- [ ] docs/MIGRATION.md + docs/MIGRATION.pt-BR.md atualizados com breaking changes e guia de upgrade
- [ ] docs/TESTING.md + docs/TESTING.pt-BR.md atualizados com novos testes adicionados
- [ ] docs/CROSS_PLATFORM.md + docs/CROSS_PLATFORM.pt-BR.md atualizados se houver mudanças multiplataforma
- [ ] docs/schemas/*.schema.json atualizados se houver mudanças de output JSON
- [ ] docs/schemas/README.md atualizado se houver novos schemas
- [ ] docs/decisions/adr-NNNN-*.md criado para cada decisão arquitetural nova
- [ ] skill/*/SKILL.md atualizados se houver mudanças operacionais
- [ ] llms.txt + llms.pt-BR.txt atualizados se houver mudanças na proposta de valor
- [ ] llms-full.txt atualizado para refletir estado atual completo
- [ ] gaps.md atualizado com nova rodada de acceptance testing
- [ ] INTEGRATIONS.md + INTEGRATIONS.pt-BR.md atualizados se houver novas integrações
- [ ] TODAS as seções "Authentication" e "API keys" revisadas para refletir a OAuth-only enforcement (v1.0.69+)


## Contagem de Referência — Métricas do Projeto Modelo

### Referência de Tamanho por Arquivo (linhas aproximadas)
- README.md: 800-900 linhas
- CHANGELOG.md: cresce a cada release (~100 linhas por release)
- CONTRIBUTING.md: 120-150 linhas
- CODE_OF_CONDUCT.md: 80-100 linhas
- SECURITY.md: 60-80 linhas
- INTEGRATIONS.md: 400-500 linhas (cresce com integrações)
- llms.txt: 120-150 linhas
- llms-full.txt: 500-600 linhas
- gaps.md: variável por release
- docs/AGENTS.md: 1200-1300 linhas
- docs/COOKBOOK.md: 1700-1800 linhas
- docs/HOW_TO_USE.md: 700-750 linhas
- docs/CROSS_PLATFORM.md: 200-210 linhas
- docs/MIGRATION.md: 250-300 linhas
- docs/TESTING.md: 220-240 linhas
- docs/schemas/README.md: 120-130 linhas
- skill/*/SKILL.md: 800-850 linhas

### Referência de Contagem de Schemas
- UM schema `.json` por subcomando que emite JSON no stdout
- UM schema `.json` por tipo de evento NDJSON (file-event, summary, phase)
- UM schema `error-envelope.schema.json` universal
- Schemas de input para payloads de entrada (entities-input, relationships-input)
- Total típico: 40-60 schemas para um CLI com 30+ subcomandos
