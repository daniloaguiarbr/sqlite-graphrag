# Integrações


> 21 agentes e 20+ plataformas em um único contrato de CLI

- Leia a versão em inglês em [INTEGRATIONS.md](INTEGRATIONS.md)
- Cada receita abaixo está pronta para copiar e custa zero para executar


## Aliases de Flags CLI (desde v1.0.35)
- `recall` e `hybrid-search` aceitam `--limit` como alias de `-k`/`--k`. Os exemplos abaixo usam `--k` e continuam válidos.
- `rename` aceita `--from`/`--to` como aliases de `--name`/`--new-name` (aliases legados `--old`/`--new` continuam suportados).
- Todos os campos JSON `schema_version` (`init`, `stats`, `migrate`, `health`) são emitidos como números JSON (eram string em `init`/`stats`/`migrate` antes da v1.0.35).
- Auto-init via `remember`/`ingest`/etc. agora ativa `journal_mode = wal` corretamente (correção de regressão).


## Tabela Resumo
### Catálogo — Toda Integração Suportada
| Nome | Tipo | Versão Mínima | Exemplo | Docs Oficiais |
| --- | --- | --- | --- | --- |
| Claude Code | Agente IA | 1.0+ | `sqlite-graphrag recall "query" --json` | https://docs.anthropic.com/claude-code |
| Codex CLI | Agente IA | 0.5+ | `sqlite-graphrag remember --name X --type user --body "..."` | https://github.com/openai/codex |
| Gemini CLI | Agente IA | recente | `sqlite-graphrag hybrid-search "query" --k 5 --json` | https://github.com/google-gemini/gemini-cli |
| Opencode | Agente IA | recente | `sqlite-graphrag recall "auth flow" --json` | https://github.com/opencode-ai/opencode |
| OpenClaw | Agente IA | recente | `sqlite-graphrag list --type user --json` | projeto comunitário |
| Paperclip | Agente IA | recente | `sqlite-graphrag read --name note --json` | projeto comunitário |
| VS Code Copilot | Agente IA | 1.90+ | tasks.json | https://code.visualstudio.com/docs/copilot |
| Google Antigravity | Agente IA | recente | `sqlite-graphrag hybrid-search "prompt" --json` | docs do Antigravity |
| Windsurf | Agente IA | recente | `sqlite-graphrag recall "plano refactor" --json` | https://windsurf.com/docs |
| Cursor | Agente IA | 0.40+ | `sqlite-graphrag remember --name cursor-ctx --type project --body "..."` | https://cursor.com/docs |
| Zed | Agente IA | recente | `sqlite-graphrag recall "abas abertas" --json` | https://zed.dev/docs |
| Aider | Agente IA | 0.60+ | `sqlite-graphrag recall "refactor" --k 5 --json` | https://aider.chat |
| Jules | Agente IA | preview | `sqlite-graphrag stats --json` | https://jules.google |
| Kilo Code | Agente IA | recente | `sqlite-graphrag recall "tarefas" --json` | projeto comunitário |
| Roo Code | Agente IA | recente | `sqlite-graphrag hybrid-search "contexto repo" --json` | projeto comunitário |
| Cline | Agente IA | extensão VS Code | `sqlite-graphrag list --limit 20 --json` | https://cline.bot |
| Continue | Agente IA | VS Code ou JetBrains | `sqlite-graphrag recall "docstring" --json` | https://docs.continue.dev |
| Factory | Agente IA | recente | `sqlite-graphrag recall "contexto pr" --json` | https://factory.ai |
| Augment Code | Agente IA | recente | `sqlite-graphrag hybrid-search "review" --json` | https://docs.augmentcode.com |
| JetBrains AI Assistant | Agente IA | 2024.2+ | `sqlite-graphrag recall "stacktrace" --json` | https://www.jetbrains.com/ai |
| OpenRouter | Roteador IA | qualquer | `sqlite-graphrag recall "regra" --json` | https://openrouter.ai/docs |
| Shells POSIX | Shell | qualquer | `sqlite-graphrag recall "$query" --json` | https://www.gnu.org/software/bash |
| Nushell | Shell | 0.90+ | `^sqlite-graphrag recall "query" --k 5 --json \| from json \| get results` | https://www.nushell.sh/book |
| GitHub Actions | CI/CD | qualquer | workflow YAML | https://docs.github.com/actions |
| GitLab CI | CI/CD | qualquer | `.gitlab-ci.yml` | https://docs.gitlab.com/ee/ci |
| CircleCI | CI/CD | qualquer | `.circleci/config.yml` | https://circleci.com/docs |
| Jenkins | CI/CD | 2.400+ | Jenkinsfile | https://www.jenkins.io/doc |
| Docker e Podman Alpine | Container | qualquer | Dockerfile | https://docs.docker.com |
| Kubernetes | Orquestrador | 1.25+ | Job ou CronJob | https://kubernetes.io/docs |
| Homebrew | Gerenciador Pacote | macOS e Linux | `brew install sqlite-graphrag` (planejado) | https://brew.sh |
| Scoop e Chocolatey | Gerenciador Pacote | Windows | `scoop install sqlite-graphrag` (planejado) | https://scoop.sh e https://chocolatey.org |
| Nix e Flakes | Gerenciador Pacote | qualquer | `nix run .#sqlite-graphrag` | https://nixos.org |


## Claude Code
### Agente Anthropic — Integração Subprocess
- Receita pronta para copiar em `.claude/hooks/`, zero custo, memória permanece na sua máquina
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é persistir contexto entre sessões do Claude Code sem serviços externos de memória
- Use `sqlite-graphrag recall "$USER_PROMPT" --k 5 --json` em um hook pre-task para injetar contexto
- Versão mínima exige Claude Code 1.0 ou posterior para suporte estável ao diretório `.claude/hooks/`
- Docs oficiais em https://docs.anthropic.com/claude-code descrevendo o ciclo de vida dos hooks
- Dica de ouro é capturar exit code `75` como retry-later mantendo o agente vivo graciosamente


## Codex CLI
### Agente OpenAI — Subprocess Dirigido Por AGENTS.md
- Receita pronta para colar no `AGENTS.md` da raiz do repo, zero custo para ativar
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é expor o contrato de memória via convenção nativa do `AGENTS.md` da própria OpenAI
- Use `sqlite-graphrag recall "<query>" --k 5 --json` documentado dentro do `AGENTS.md` na raiz do repo
- Versão mínima exige Codex CLI 0.5 ou posterior para regras determinísticas de parsing do AGENTS.md
- Docs oficiais em https://github.com/openai/codex cobrindo a ordem de descoberta do AGENTS.md
- Dica de ouro é incluir um exemplo de invocação funcional sob cada comando listado para Codex


## Gemini CLI
### Agente Google — Subprocess Com Contrato JSON
- Receita pronta para copiar na config do Gemini CLI, zero custo, roda completamente local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é injetar memória em prompts do Gemini 2.5 Pro durante sessões longas de código
- Use `sqlite-graphrag hybrid-search "query" --k 5 --json` para recall com intenção mista de keyword
- Versão mínima suporta qualquer release recente do Gemini CLI com invocação subprocess habilitada
- Docs oficiais em https://github.com/google-gemini/gemini-cli sobre padrões de integração de tool
- Dica de ouro é definir `SQLITE_GRAPHRAG_LANG=pt` ao prompt-ar Gemini em contextos em português


## Opencode
### Agente Comunitário — Integração Subprocess
- Receita pronta para copiar no hook plugin do Opencode, zero custo, roda como subprocess
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é persistir contexto multi-turno no loop open source de orquestração do Opencode
- Use `sqlite-graphrag recall "$query" --json` como parte do pipeline pre-generation do Opencode
- Versão mínima suporta qualquer release recente do Opencode expondo hook subprocess via plugin
- Projeto oficial em https://github.com/opencode-ai/opencode com issue tracker comunitário
- Dica de ouro é definir o namespace pelo slug do repo para evitar vazamento entre projetos


## OpenClaw
### Agente Comunitário — Driver Subprocess
- Receita pronta para adicionar no startup do OpenClaw, zero custo, memória é totalmente local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é injetar memória persistente em loops do agente OpenClaw sem rebuild de plugin
- Use `sqlite-graphrag list --type user --json` para buscar contexto inicial no começo de uma run
- Versão mínima suporta qualquer release recente do OpenClaw capaz de shell out para binários CLI
- Docs oficiais dentro do README GitHub do OpenClaw explicando regras de integração subprocess
- Dica de ouro é executar o binário dentro da pasta alvo e manter o default `graphrag.sqlite`


## Paperclip
### Agente Comunitário — Cliente Subprocess
- Receita pronta para colar na config de hook do Paperclip, zero custo, memória fica local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é persistir memória cross-session no agente autônomo de desenvolvimento Paperclip
- Use `sqlite-graphrag read --name onboarding-note --json` para semear a sessão com notas prévias
- Versão mínima suporta qualquer release recente do Paperclip que possa spawnar subprocess filho
- Docs oficiais no repositório comunitário do Paperclip descrevendo o contrato de hook subprocess
- Dica de ouro é rodar `health --json` no startup e abortar quando integridade reporta dano algum


## VS Code Copilot
### Agente Microsoft — Integração tasks.json
- Receita pronta para colar no tasks.json, zero custo, recall dispara de dentro do editor
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é expor memória relevante de uma seleção dentro dos painéis de chat do VS Code Copilot
- Use a entrada de exemplo em tasks.json que chama `sqlite-graphrag recall "$selection" --json`
- Versão mínima exige VS Code 1.90 ou posterior para as substituições mais recentes de tasks.json
- Docs oficiais em https://code.visualstudio.com/docs/copilot cobrindo registro de tool no chat
- Dica de ouro é mapear a task em `Cmd+Shift+M` para invocação de recall com uma única tecla


## Google Antigravity
### Agente Google — Integração Runner
- Receita pronta para registrar como runner Antigravity, zero custo, binário é autocontido
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é rodar sqlite-graphrag como runner de primeira classe em pipelines Antigravity em escala
- Use `sqlite-graphrag hybrid-search "$PROMPT" --json --k 10` como passo de retrieval em um runner
- Versão mínima suporta qualquer release recente do Antigravity que aceite runners binários arbitrários
- Docs oficiais na página do produto Google Antigravity descrevendo formato de config de runner
- Dica de ouro é rodar `sync-safe-copy` antes de cada pipeline para proteger o artefato compartilhado


## Windsurf
### Agente Codeium — Integração Terminal
- Receita pronta para colar em um binding Run task do Windsurf, zero custo para ativar recall
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é expor recall de memória para painéis assistentes do Windsurf via invocação de terminal
- Use `sqlite-graphrag recall "$EDITOR_CONTEXT" --json` mapeado para um binding Run task no Windsurf
- Versão mínima suporta qualquer release recente do Windsurf com execução de task de terminal ativa
- Docs oficiais em https://windsurf.com/docs descrevendo a sintaxe de binding de task de terminal
- Dica de ouro é persistir resultados em `/tmp/ng.json` para templates de prompt Windsurf lerem


## Cursor
### Agente Cursor — Integração Terminal
- Receita pronta para adicionar em `.cursorrules` ou binding de terminal, zero custo, memória é local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é parear Cursor AI com um backend de memória local que sobrevive restarts do editor
- Use `sqlite-graphrag remember --name cursor-ctx --type project --body "$SELECTION"` por atalho
- Versão mínima exige Cursor 0.40 ou posterior para regras AI estáveis e override de env de terminal
- Docs oficiais em https://cursor.com/docs cobrindo padrões de regras AI e integração de terminal
- Dica de ouro é definir `SQLITE_GRAPHRAG_NAMESPACE=${workspaceFolderBasename}` por workspace


## Zed
### Agente Zed Industries — Integração Assistant Panel
- Receita pronta para adicionar como task profile do Zed, zero custo, roda do terminal integrado
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é cablear recall de memória no painel assistente do Zed sem extensões customizadas
- Use `sqlite-graphrag recall "abas abertas" --json --k 5` como comando de terminal disponível ao Zed
- Versão mínima suporta qualquer release recente do Zed com painel assistente e tasks de terminal
- Docs oficiais em https://zed.dev/docs descrevendo painel assistente e integração de terminal
- Dica de ouro é definir um profile de task Zed compartilhando memória entre múltiplos workspaces


## Aider
### Agente Open Source — Integração Shell
- Receita pronta para colar no alias shell antes do `aider`, zero custo, zero servidor de config
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é aumentar pair programming do Aider com memória durável entre repositórios git
- Use `sqlite-graphrag recall "refactor target" --k 5 --json` invocado antes de cada prompt Aider
- Versão mínima exige Aider 0.60 ou posterior para invocação subprocess e hook estáveis e suportadas
- Docs oficiais em https://aider.chat descrevendo configuração e comandos shell customizados
- Dica de ouro é escopar memória por repositório via `SQLITE_GRAPHRAG_NAMESPACE=$(basename $(pwd))`


## Jules
### Agente Google Labs — Automação CI
- Receita pronta para adicionar como passo CI do Jules, zero custo, binário instala em segundos
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é rodar manutenção de memória dentro dos pipelines de automação preview do Jules
- Use `sqlite-graphrag stats --json` como passo CI para monitorar crescimento de memória semanal
- Versão mínima é a release preview corrente do Jules disponível via early access do Google Labs
- Docs oficiais em https://jules.google explicando configuração de job CI e autenticação necessária
- Dica de ouro é falhar o pipeline quando `stats.memories` excede o limite combinado para um projeto


## Kilo Code
### Agente Comunitário — Integração Subprocess
- Receita pronta para colar no hook de startup do Kilo Code, zero custo, memória é arquivo local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é expor camada de memória persistente ao agente autônomo de engenharia Kilo Code
- Use `sqlite-graphrag recall "tarefas recentes" --json` no começo de toda run do agente Kilo Code
- Versão mínima suporta qualquer release recente do Kilo Code capaz de spawnar processos filhos
- Docs oficiais no repositório comunitário do Kilo Code descrevendo o contrato de subprocess
- Dica de ouro é logar exit code `75` como retryable em vez de fatal quando orquestrador está ocupado


## Roo Code
### Agente Comunitário — Integração Subprocess
- Receita pronta para cablear no ciclo de hook do Roo Code, zero custo, dados em SQLite local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é injetar memória em prompts do agente Roo Code para entendimento profundo do repo
- Use `sqlite-graphrag hybrid-search "contexto repo" --json` para recall entre tipos mistos de query
- Versão mínima suporta qualquer release recente do Roo Code com capacidade de hook subprocess
- Docs oficiais no repositório comunitário do Roo Code explicando convenções de ciclo de hook
- Dica de ouro é encadear `related <name> --hops 2` após recall para expansão multi-hop no grafo


## Cline
### Extensão Comunitária VS Code — Integração Terminal
- Receita pronta para registrar como tool de terminal do Cline, zero custo, memória persiste local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é dar ao Cline memória persistente entre sessões VS Code sem serviços em cloud
- Use `sqlite-graphrag list --limit 20 --json` como passo inicial no startup da conversa do Cline
- Versão mínima suporta a release atual da extensão VS Code do Cline no marketplace
- Docs oficiais em https://cline.bot cobrindo registro de tool de terminal e padrões de uso
- Dica de ouro é mapear o comando como tool Cline com nome descritivo e explicação de uso


## Continue
### Agente Open Source — Integração Terminal IDE
- Receita pronta para colar nos custom commands do Continue, zero custo, sem servidor necessário
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é expor memória sqlite-graphrag nos painéis de chat Continue em VS Code ou JetBrains
- Use `sqlite-graphrag recall "docstring" --json` de um registro de custom command do Continue
- Versão mínima suporta qualquer release recente da extensão Continue em VS Code ou JetBrains
- Docs oficiais em https://docs.continue.dev descrevendo comandos customizados e integração de tool
- Dica de ouro é documentar cada comando no config do Continue para o LLM embutido detectar


## Factory
### Agente Factory — API Ou Subprocess
- Receita pronta para adicionar na config de tool do droid Factory, zero custo, binário autocontido
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é integrar sqlite-graphrag com droids autônomos de desenvolvimento Factory em produção
- Use `sqlite-graphrag recall "contexto pr" --json` durante preparação do plano do droid Factory
- Versão mínima suporta qualquer release recente do Factory com integração subprocess ou API
- Docs oficiais em https://factory.ai explicando configuração de tool do droid e execução do plano
- Dica de ouro é definir `--wait-lock` longo para droids Factory rodando sob concorrência pesada


## Augment Code
### Agente Augment — Integração IDE
- Receita pronta para cablear no registro de tool da IDE Augment, zero custo, roda como subprocess
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é alimentar agentes de review Augment Code com memória persistente entre repositórios
- Use `sqlite-graphrag hybrid-search "code review" --json` na preparação de review da IDE Augment
- Versão mínima suporta qualquer release recente do Augment Code com hooks de terminal e subprocess
- Docs oficiais em https://docs.augmentcode.com descrevendo registro de tool e agentes suportados
- Dica de ouro é ativar `--lang en` explicitamente para linguagem de review consistente entre times


## JetBrains AI Assistant
### Agente JetBrains — Integração IDE
- Receita pronta para registrar como external tool do JetBrains, zero custo, recall leva milissegundos
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é adicionar memória sqlite-graphrag ao JetBrains AI Assistant em IntelliJ PyCharm WebStorm
- Use `sqlite-graphrag recall "$SELECTION" --json` registrado como runner de external tool JetBrains
- Versão mínima exige JetBrains AI Assistant 2024.2 ou posterior para registro moderno de tool
- Docs oficiais em https://www.jetbrains.com/ai explicando registro de tool e external runner
- Dica de ouro é mapear o tool a um atalho de teclado para invocar recall com uma mão no teclado


## OpenRouter
### Roteador Multi-LLM — Qualquer Versão Suportada
- Receita pronta para adicionar como preâmbulo de qualquer pipeline OpenRouter, zero custo local
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é compartilhar backend comum de memória entre todo LLM hospedado via OpenRouter
- Use `sqlite-graphrag recall "regra roteamento" --json` como preâmbulo antes de request roteado
- Versão mínima suporta qualquer release da API OpenRouter já que memória fica local e independente
- Docs oficiais em https://openrouter.ai/docs explicando regras de roteamento e integração da API
- Dica de ouro é reusar o mesmo namespace entre todos os modelos roteados para contexto coeso


## Shells POSIX
### Bash Zsh Fish PowerShell — Qualquer Versão
- Receita pronta para colar em alias ou script de shell, zero custo, pipes funcionam imediatamente
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess sem processo extra
- Propósito é compor sqlite-graphrag com pipelines clássicos Unix e Windows shell sem atrito
- Use `sqlite-graphrag recall "$query" --json | jaq '.hits[].name'` em qualquer shell POSIX
- Versão mínima suporta qualquer Bash Zsh Fish ou PowerShell 7 recente
- Docs oficiais em https://www.gnu.org/software/bash e homepages dos respectivos projetos shell
- Dica de ouro é colocar variáveis entre aspas para evitar word splitting em queries com espaços


## Nushell
### Nushell — Integração Pipeline de Dados Estruturados
- Receita pronta para colar em script Nushell, zero custo, saída vira tabela Nu nativa
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como subprocess via sigil `^` no Nu
- Propósito é compor saída do sqlite-graphrag com pipelines de dados estruturados do Nushell nativamente
- Use `^sqlite-graphrag recall "query" --k 5 --json | from json | get results` para consultar memória
- Versão mínima suporta Nushell 0.90 ou posterior para comando externo estável e pipeline `from json`
- Docs oficiais em https://www.nushell.sh/book descrevendo comandos externos e parsing de JSON
- Dica de ouro é encadear `| select name score` para exibir tabela de memória ranqueada no Nu


## GitHub Actions
### CI/CD — Qualquer Runner Recente
- Receita pronta para copiar em `.github/workflows/`, zero custo, roda em qualquer runner GitHub
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag instala em segundos via cargo em qualquer runner
- Propósito é rodar manutenção de memória e backups em workflows agendados do GitHub Actions
- Use workflow cron que executa `sqlite-graphrag purge --days 30 --yes` e `vacuum` agendados
- Versão mínima funciona em qualquer runner `ubuntu-latest` `macos-latest` ou `windows-latest`
- Docs oficiais em https://docs.github.com/actions descrevendo sintaxe de workflows agendados
- Dica de ouro é fazer upload do sync-safe-copy como artifact do build para capacidade de rollback


## GitLab CI
### CI/CD — Runner Recente
- Receita pronta para copiar em `.gitlab-ci.yml`, zero custo, roda em qualquer runner GitLab
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag instala em segundos via cargo em qualquer runner
- Propósito é rodar manutenção sqlite-graphrag em pipelines agendados do GitLab CI rotineiramente
- Use stage `.gitlab-ci.yml` agendado que invoca `cargo install --path .` primeiro
- Versão mínima suporta runner recente do GitLab com toolchain Rust disponível para instalação
- Docs oficiais em https://docs.gitlab.com/ee/ci descrevendo configuração de pipelines agendados
- Dica de ouro é cachear o diretório cargo install entre runs para startup de job mais rápido


## CircleCI
### CI/CD — Executor Recente
- Receita pronta para copiar na config CircleCI, zero custo, binário instala via cargo em segundos
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag instala em segundos via cargo em qualquer executor
- Propósito é rodar manutenção e backups sqlite-graphrag em workflows agendados do CircleCI
- Use workflow agendado com `cargo install --path .` seguido dos passos do job
- Versão mínima suporta executor Linux ou macOS recente do CircleCI com toolchain Rust
- Docs oficiais em https://circleci.com/docs descrevendo pipelines agendados e workflows suportados
- Dica de ouro é persistir o DB no workspace para jobs downstream auditarem o snapshot gerado


## Jenkins
### CI/CD — Jenkins 2.400+
- Receita pronta para colar em stage de Jenkinsfile, zero custo, funciona em ambientes air-gapped
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag instala via cargo e pode rodar só como subprocesso ou ativar `sqlite-graphrag daemon` para menor latência
- Propósito é integrar backups sqlite-graphrag em pipelines Jenkins self-hosted para ambientes regulados
- Use stage em Jenkinsfile rodando `cargo install --path .` e comandos operacionais
- Versão mínima exige Jenkins 2.400 ou posterior para pipeline declarative e gerência de agent estáveis
- Docs oficiais em https://www.jenkins.io/doc cobrindo sintaxe de pipeline declarative a fundo
- Dica de ouro é arquivar a saída do sync-safe-copy como artifact para retenção de longo prazo


## Docker e Podman Alpine
### Container — Qualquer Versão Recente
- Receita pronta para copiar em Dockerfile, zero custo, imagem final cabe em menos de 25 MB Alpine
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag é binário estático sem dependência de runtime
- Propósito é empacotar sqlite-graphrag em imagens Alpine mínimas para deploys reproduzíveis em produção
- Use Dockerfile multi-stage com stage builder Rust e runtime Alpine copiando o binário único
- Versão mínima suporta qualquer Docker ou Podman com sintaxe multi-stage compatível ativada
- Docs oficiais em https://docs.docker.com cobrindo multi-stage build e minimização de imagem
- Dica de ouro é montar o arquivo SQLite como named volume para persistir memória entre restarts


## Kubernetes Jobs E CronJobs
### Kubernetes — 1.25+
- Receita pronta para copiar em manifesto CronJob, zero custo, roda no seu cluster existente
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como Job one-shot sem sidecar necessário
- Propósito é rodar manutenção sqlite-graphrag como Kubernetes CronJobs em clusters gerenciados
- Use manifesto CronJob referenciando a imagem Alpine e invocando purge mais vacuum agendados
- Versão mínima exige Kubernetes 1.25 ou posterior para Job CronJob e concurrency policy estáveis
- Docs oficiais em https://kubernetes.io/docs descrevendo Job CronJob e PersistentVolumeClaim
- Dica de ouro é montar o DB de um PVC com access mode `ReadWriteOnce` para segurança de dados


## Homebrew
### Gerenciador Pacote — macOS E Linux
- Receita pronta para executar assim que a fórmula entrar, zero custo, instala o mesmo binário do cargo
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag é binário único sem dependência de runtime
- Propósito é instalar sqlite-graphrag em macOS e Linux com o familiar gerenciador Homebrew
- Use `brew install sqlite-graphrag` assim que a fórmula oficial aparecer nos taps Homebrew core
- Versão mínima suporta qualquer Homebrew 4.0 ou posterior em macOS ou distros Linuxbrew
- Docs oficiais em https://brew.sh explicando descoberta de fórmulas e convenções de instalação
- Dica de ouro é fixar a release via `brew install sqlite-graphrag@1.2.1` assim que taps versionados surjam


## Scoop E Chocolatey
### Gerenciador Pacote — Windows
- Receita pronta para executar assim que o manifesto entrar, zero custo, instala o mesmo binário do cargo
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag é único exe sem dependência de runtime
- Propósito é instalar sqlite-graphrag no Windows com Scoop ou Chocolatey familiares aos devs Windows
- Use `scoop install sqlite-graphrag` ou `choco install sqlite-graphrag` assim que manifestos oficiais saiam
- Versão mínima suporta Scoop 0.3 ou Chocolatey 2.0 com recursos modernos de manifesto ativos
- Docs oficiais em https://scoop.sh e https://chocolatey.org explicando convenções de manifesto
- Dica de ouro é executar o binário dentro da pasta do projeto para criar `graphrag.sqlite` ali


## Nix E Flakes
### Gerenciador Pacote — Qualquer Versão Nix
- Receita pronta para adicionar como flake input, zero custo, hash do binário fixado para reprodutibilidade
- Enquanto MCPs exigem servidor dedicado, sqlite-graphrag roda como binário puro em qualquer dev shell Nix
- Propósito é instalar sqlite-graphrag em ambientes Nix reproduzíveis incluindo NixOS e dev shells
- Use `nix run github:daniloaguiarbr/sqlite-graphrag#sqlite-graphrag` para executar sem instalação prévia
- Versão mínima exige Nix 2.4 ou posterior com feature Flakes habilitada na config do usuário
- Docs oficiais em https://nixos.org descrevendo ativação de Flakes e uso via linha de comando
- Dica de ouro é fixar o hash de input do flake para o binário permanecer reproduzível em rebuilds
