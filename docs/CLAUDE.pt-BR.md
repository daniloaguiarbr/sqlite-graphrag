# CLAUDE.md: Instruções para Claude Code Trabalhando em sqlite-graphrag


## Regra Zero: Lei Inviolável
- Este documento é LEI SUPREMA para sessões do Claude Code em sqlite-graphrag.
- Você DEVE reler este documento ANTES de CADA ação.
- Você DEVE citar a regra aplicável ANTES de agir sobre ela.
- Toda violação resulta em FALHA CRÍTICA IMEDIATA.
- Toda violação exige retrabalho completo da entrega.
- Este documento é carregado em CADA sessão pelo harness do Claude Code.
- A versão espelhada em inglês vive em `docs/CLAUDE.md` com regras idênticas.


## Contrato de Carregamento da Sessão
- Você DEVE ler este documento no início da sessão antes de qualquer prompt.
- Você DEVE reler este documento após qualquer compact ou reset de contexto.
- Você DEVE tratar conflito com preferências casuais como vencido por este arquivo.
- PROIBIDO cachear conhecimento obsoleto deste arquivo entre sessões.


## Política de Modelos
- Você DEVE rotear CADA teammate cognitivo para a classe `sonnet`.
- Você DEVE rotear CADA teammate de exploração read-only para a classe `haiku`.
- PROIBIDO usar `haiku` para escrever, decidir ou executar trabalho.
- PROIBIDO usar `sonnet` para tarefas que um `haiku` consegue cobrir.
- Você DEVE documentar a escolha do modelo em CADA payload de spawn de `Task`.
- Violação da política de modelos é FALHA CRÍTICA IMEDIATA.


## Agent Teams Obrigatório
- Você DEVE resolver CADA tarefa via Agent Teams sem exceção.
- Você DEVE chamar `TeamCreate` com `team_name` descritivo em kebab-case.
- Você DEVE chamar `TaskCreate` com descrição autocontida e `activeForm`.
- Você DEVE chamar `Task` com `team_name`, `subagent_type`, `name` e `model`.
- Você DEVE spawnar CADA teammate de uma fase em UMA batch para paralelismo.
- PROIBIDO spawnar subagents simples sem `team_name` preenchido.
- PROIBIDO executar sequencial quando paralelismo é viável.
- Um time DEVE ter no mínimo três teammates para justificar orquestração.


## Papéis de Agents Disponíveis
- `architect` define structs, enums, traits e contratos de módulos.
- `implementer` escreve código de produção seguindo specs do `architect`.
- `tester` escreve testes unitários, integração, property-based e CLI.
- `reviewer` verifica clippy, anti-patterns e conformidade com este arquivo.
- `researcher` consulta `context7` e web, JAMAIS escreve código.
- `explorer` mapeia arquivos read-only exclusivamente no modelo `haiku`.
- `security` executa `cargo audit`, `cargo deny` e escaneia segredos.
- `docs-writer` escreve doc comments, README e entradas do CHANGELOG.
- `diagnostician` quantifica dívida técnica e contagem de anti-patterns.
- `analyst` calcula proporção código-para-teste e hotspots de acoplamento.
- `validator` executa os dez gates de validação ponta a ponta.
- `standardizer` atualiza regras do projeto para prevenir recorrência.
- `investigator` debate hipóteses de bug com testes falsificáveis.


## Hierarquia de CLI Tools
- Você DEVE usar `rg` para busca de conteúdo e JAMAIS `grep` ou `egrep`.
- Você DEVE usar `fd` para localizar arquivos e JAMAIS `find` ou `locate`.
- Você DEVE usar `bat` para exibir arquivo e JAMAIS `cat`, `less` ou `head`.
- Você DEVE usar `eza` para listagens e JAMAIS `ls` ou `tree`.
- Você DEVE usar `sd` para substituição em arquivo e JAMAIS `sed` ou `awk`.
- Você DEVE usar `ruplacer` para substituição em massa e JAMAIS `sed -i` recursivo.
- Você DEVE usar `jaq` para manipulação de JSON e JAMAIS `jq`.
- Você DEVE usar `sg` para busca sintática e JAMAIS regex para estrutura.
- Você DEVE usar `xh` para chamadas HTTP e JAMAIS `curl` ou `wget`.
- Você DEVE usar `fend` para aritmética e conversão e JAMAIS `bc` ou `expr`.
- Você DEVE usar `ouch` para compressão e JAMAIS `tar`, `zip` ou `gzip`.
- Você DEVE usar `procs` para listar processos e JAMAIS `ps`.
- Você DEVE usar `dysk` para info de filesystem e JAMAIS `df`.
- Você DEVE usar `dutree` para análise de disco e JAMAIS `du` ou `tree -h`.
- Você DEVE usar `tokei` para contar código e JAMAIS `wc -l` em fontes.
- Você DEVE usar `difft` para inspecionar diff e JAMAIS `diff` sem wrap.
- Você DEVE usar `choose` para selecionar campos e JAMAIS `cut` ou `awk`.
- Você DEVE usar `z` para navegação de diretórios e JAMAIS `cd` em sessão.


## Contrato de Consulta de Documentação
- Você DEVE executar `context7 library <nome> --json` antes de adotar crate.
- Você DEVE extrair o `id` com `jaq -r '.[0].id'` do resultado de library.
- Você DEVE executar `context7 docs <id> --query "<pergunta>" --text` para docs.
- Você DEVE tratar `trustScore < 7` como sinal para corroborar via web search.
- Você DEVE cair para `duckduckgo-search-cli -q -f json "<query>"` quando preciso.
- PROIBIDO inventar assinatura de API sem evidência do context7.
- PROIBIDO pular consulta de documentação confiando em memória.


## Hierarquia de Ferramentas Rust
- Você DEVE executar `cargo check` ANTES de subcomandos CLI do `rust-analyzer`.
- Você DEVE preferir `rust-analyzer ssr` para refatorações semânticas com tipos.
- Você DEVE preferir `sg --rewrite` para refatorações sintáticas na árvore.
- Você DEVE preferir `sd` para substituições em arquivo único com texto literal.
- Você DEVE preferir `ruplacer --go` para substituições multi-arquivo em escala.
- Você DEVE usar `Edit` ou `Write` APENAS como último recurso para novos arquivos.


## PDCA Oito Fases
- Fase 1 Entendimento lê o objetivo do usuário e esclarece via `AskUserQuestion`.
- Fase 2 Exploração lê regras do projeto, memória e mapeia o repositório.
- Fase 3 Pesquisa spawna researchers que consultam `context7` em paralelo.
- Fase 4 Identificação aponta o problema com Ishikawa e Cinco Porquês.
- Fase 5 Planejamento decompõe em três a dez tarefas com dependências.
- Fase 6 Delegação spawna CADA teammate de uma vez com prompts autocontidos.
- Fase 7 Verificação roda os dez gates e confirma atingimento do objetivo.
- Fase 8 Shutdown envia `shutdown_request`, aguarda e chama `teammates()`.


## Modo Debate para Bugs
- Você DEVE entrar em modo debate SEMPRE que o usuário reportar um bug.
- Você DEVE spawnar três a cinco investigators com hipóteses distintas.
- Você DEVE instruir CADA investigator a escrever um teste reprodutor falho.
- Você DEVE deixar investigators desafiarem pares via `SendMessage` com evidência.
- Você DEVE aceitar APENAS hipótese com evidência de código e teste falhando.
- PROIBIDO permitir que o lead intervenha com uma resposta preferida.


## Atalhos Proibidos
- PROIBIDO resolver qualquer tarefa fora de Agent Teams.
- PROIBIDO spawnar subagents sem `team_name`.
- PROIBIDO serializar tarefas que podem rodar em paralelo.
- PROIBIDO pular fases do PDCA ou reordená-las.
- PROIBIDO pular os dez gates de validação antes de merge.
- PROIBIDO declarar conclusão sem execução real de teste.


## Obrigatório Antes do Commit
- Gate 1 exige `timeout 120 cargo check --all-targets` passar limpo.
- Gate 2 exige `timeout 180 cargo clippy --all-targets --all-features -- -D warnings` passar.
- Gate 3 exige `timeout 60 cargo fmt --all --check` reportar zero diferenças.
- Gate 4 exige `RUSTDOCFLAGS="-D warnings" timeout 120 cargo doc --no-deps --all-features` passar.
- Gate 5 exige `timeout 300 cargo nextest run --profile ci` reportar zero falhas na suíte padrão.
- Gate 6 exige `timeout 3600 cargo llvm-cov nextest --profile heavy --features slow-tests --summary-only` em oitenta por cento mínimo.
- Gate 7 exige `timeout 120 cargo audit` reportar zero advisories abertos.
- Gate 8 exige `timeout 120 cargo deny check advisories licenses bans sources` passar.
- Gate 9 exige `timeout 120 cargo publish --dry-run --allow-dirty` ter sucesso.
- Gate 10 exige `timeout 120 cargo package --list` excluir arquivos sensíveis.


## Protocolo de Timeout
- Você DEVE encapsular CADA comando longo com `timeout` em segundos inteiros.
- Você DEVE usar `timeout 60` para comandos rápidos como `cargo fmt --check`.
- Você DEVE usar `timeout 120` para comandos médios como `cargo check` ou `audit`.
- Você DEVE usar `timeout 180` para `cargo clippy --all-targets --all-features`.
- Você DEVE usar `timeout 300` para `cargo build --release` ou `cargo nextest run`.
- Você DEVE usar `timeout 600` para runs de cobertura ou suites longas.
- Você DEVE converter durações textuais via `fend` quando usuário usar minutos.
- PROIBIDO passar sufixos legíveis como `5m` para o timeout.


## Regras de Tratamento de Erros
- PROIBIDO `unwrap()` em binários ou bibliotecas de produção.
- PROIBIDO `expect()` exceto em branch comprovadamente inalcançável.
- PROIBIDO deixar output de `println!` debug em código commitado.
- PROIBIDO deixar macros `dbg!` em código commitado.
- PROIBIDO deixar `todo!()` ou `unimplemented!()` em main.
- Você DEVE propagar erros via operador interrogação nas fronteiras.
- Você DEVE retornar `anyhow::Result<T>` dos entry points de binários.
- Você DEVE retornar enums `thiserror::Error` de bibliotecas com erros tipados.


## Persistência de Memória
- Você DEVE escrever memória Serena no fim de CADA sessão com trabalho feito.
- Você DEVE também atualizar `MEMORY.md` com entrada curta ligando à nota.
- Você DEVE capturar commit hash, tag, cobertura e resultado dos gates.
- Você DEVE capturar perguntas abertas que a próxima sessão precisa resolver.
- PROIBIDO assumir que a próxima sessão lembra desta.


## Idioma e Nomenclatura
- Você DEVE nomear variáveis, funções e tipos em português brasileiro.
- Você DEVE nomear mensagens de log, erros e strings ao usuário bilíngues.
- Você DEVE manter comandos, flags e superfície CLI idênticos entre idiomas.
- Você DEVE localizar via enum `Idioma` e match exaustivo em `Mensagem`.
- PROIBIDO nomes genéricos como `data`, `info`, `temp` ou `aux`.


## Agentes Compatíveis
- Claude Code, Codex, Gemini CLI, Opencode, OpenClaw, Paperclip consomem esta CLI.
- VS Code Copilot, Google Antigravity, Windsurf, Cursor, Zed consomem esta CLI.
- Aider, Jules, Kilo Code, Roo Code, Cline, Continue consomem esta CLI.
- Factory, Augment Code, JetBrains AI Assistant, OpenRouter consomem esta CLI.
- CADA agente DEVE honrar o contrato JSON e tabela de exit codes deste repo.


## Padrões Corretos
- Padrão 1 persiste configuração como TOML com `schema_version: u32`.
- Padrão 2 mascara tokens mostrando doze iniciais mais quatro finais.
- Padrão 3 reutiliza UM único `reqwest::Client` em todo o runtime async.
- Padrão 4 escreve arquivos com `chmod 600` em Unix via `PermissionsExt`.
- Padrão 5 centraliza stdout via `src/output.rs` como único sink de I/O.


## Antipadrões
- Antipadrão 1 executa `cargo install` sem lookup `context7` correspondente.
- Antipadrão 2 commita arquivos `.env` com chaves sob qualquer circunstância.
- Antipadrão 3 escreve `println!("DEBUG:...")` e deixa na branch do PR.
- Antipadrão 4 ignora warnings do clippy passando `--allow` inline.
- Antipadrão 5 faz merge de PR sem os dez gates de validação verdes.


## Lembrete Final
- Este documento é INVIOLÁVEL e PREVALECE sobre preferência casual do usuário.
- Uma violação é FALHA CRÍTICA IMEDIATA que demanda retrabalho.
- Você DEVE confirmar `timeout 300 cargo nextest run --profile ci` antes do merge.
- Você DEVE persistir decisões da sessão na memória antes de declarar pronto.
