Leia este documento em [inglês (EN)](SECURITY.md).


# Política de Segurança


## Versões Suportadas
- A tabela abaixo lista quais versões do sqlite-graphrag recebem correções de segurança atualmente
- Usuários em linhas descontinuadas são FORTEMENTE encorajados a atualizar para uma versão suportada
- Atualizar cedo reduz janela de exposição e alinha com a política de divulgação coordenada

| Versão  | Status        | Correções de Segurança     |
| ------- | ------------- | -------------------------- |
| 1.0.x   | Suportada     | Sim, recebe correções      |
| 0.x     | Sem suporte   | Sem correções fornecidas   |


## Reportando uma Vulnerabilidade
- OBRIGATÓRIO reportar questões de segurança via GitHub Security Advisories no repositório público `sqlite-graphrag` como canal privado preferencial
- Use o email daniloaguiarbr@gmail.com apenas como fallback quando o reporte privado do GitHub estiver indisponível
- JAMAIS abra issue pública, pull request ou discussão no GitHub para relatos de segurança
- Inclua reprodução mínima, versões afetadas e comportamento esperado versus observado
- Inclua detalhes do ambiente como sistema operacional, arquitetura e versão do rustc
- Inclua estimativa de severidade CVSS 3.1 quando possível para acelerar triagem


## SLA de Resposta
- A triagem de cada advisory tem início comprometido em até 72 horas úteis após envio
- Email de reconhecimento inicial será enviado dentro dessa mesma janela de 72 horas
- Você receberá um identificador de caso e contato do mantenedor designado
- Atualizações de progresso são compartilhadas no mínimo a cada 7 dias até resolução ou divulgação


## SLA de Correção por Severidade CVSS
- Severidade crítica (CVSS 9.0 a 10.0) recebe patch em até 7 dias corridos após triagem validada
- Severidade alta (CVSS 7.0 a 8.9) recebe patch em até 14 dias corridos após triagem validada
- Severidade média (CVSS 4.0 a 6.9) recebe patch em até 30 dias corridos após triagem validada
- Severidade baixa (CVSS 0.1 a 3.9) recebe patch em até 90 dias corridos após triagem validada
- Correções liberadas seguem imediatamente com entrada no CHANGELOG e GitHub Security Advisory quando a linha afetada ainda estiver suportada


## Política de Divulgação
- Seguimos divulgação coordenada com janela padrão de 90 dias de embargo a partir do relato inicial
- O embargo pode ser encurtado quando correção é liberada antes de 90 dias
- O embargo pode ser estendido quando correção demanda mais tempo e o autor do relato concorda
- Divulgação pública inclui identificador CVE quando o impacto justificar
- Divulgação pública inclui o GitHub Security Advisory com versões afetadas e versão corrigida
- Crédito é atribuído ao autor do relato exceto se anonimato for explicitamente solicitado


## Política de Atualização de Segurança
- Patches para versões suportadas são entregues como nova release patch no crates.io e GitHub Releases
- Toda release é validada com o pipeline completo de 10 comandos descrito em CONTRIBUTING
- CI executa `cargo audit` e `cargo deny check advisories licenses bans sources` em cada push
- Supply chain é protegida via pinagem `constant_time_eq = "=0.4.2"` para proteger MSRV 1.88
- Drift de MSRV de dependência transitiva é monitorado proativamente conforme política do PRD

## v1.0.76 Aplicação OAuth-Only de Credencial LLM
- O build padrão é apenas LLM e one-shot. Cada chamada de embedding spawna um subprocesso headless `claude code` ou `codex`.
- O spawn ABORTA com `AppError::Validation` e código de saída 1 quando `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` são detectadas no ambiente.
- O fluxo OAuth (assinatura Claude Pro/Max ou ChatGPT Pro) é o ÚNICO mecanismo de credencial aceito.
- Ambas as variáveis de chave de API estão INTENCIONALMENTE AUSENTES da whitelist de env-clear em `claude_runner.rs`, `codex_spawn.rs` e `ingest_claude.rs`. Defesa em profundidade: mesmo se um refactor futuro mover a guarda OAuth-only, a variável nunca chega ao filho.
- A flag `--bare` (que também exigiria uma chave de API) foi REMOVIDA de todo caminho executável desde a v1.0.69.
- Quatro testes `#[serial_test::serial(env)]` validam o conjunto canônico de flags e o comportamento de aborto.
- Veja `docs/decisions/adr-0011-oauth-only-enforcement.md` para a justificativa completa e `docs/decisions/adr-0025-oauth-only-embedding.md` para a aplicação específica em embedding da v1.0.76.
- Migração: qualquer operador que dependa de `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` precisa migrar para OAuth antes de atualizar.

## v1.0.83 Preservação de Credenciais de Provider Customizado (ADR-0041)
- O build padrão agora PRESERVA sete variáveis de ambiente de provider customizado ao spawnar subprocessos `claude -p` ou `codex exec`, habilitando providers Anthropic-compatíveis (MiniMax/api.minimax.io, OpenRouter, AWS Bedrock, gateways corporativos)
- As variáveis preservadas são `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CODEX_ACCESS_TOKEN`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY` e `OTEL_EXPORTER_OTLP_ENDPOINT`
- Essas variáveis são SEMANTICAMENTE DISTINTAS das rejeitadas pelo OAuth-only `ANTHROPIC_API_KEY` e `OPENAI_API_KEY`; a guarda OAuth-only em `claude_runner.rs`, `codex_spawn.rs` e `ingest_claude.rs` continua rejeitando as chaves de API com exit 1 (defesa em profundidade preservada)
- A whitelist agora vive em um helper compartilhado único `src/spawn/env_whitelist.rs` expondo `apply_env_whitelist(cmd, strict)` e `is_strict_env_clear()`; os três spawners delegam em vez de duplicar o array inline
- Para ambientes de compliance que proíbem encaminhamento de credenciais via env vars (PCI-DSS, SOC2, HIPAA), operadores podem definir `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` ou passar `--strict-env-clear`; o modo estrito preserva apenas `PATH` e descarta todas as outras env vars
- Cinco testes de regressão `#[serial_test::serial(env)]` vivem em `tests/claude_runner_env.rs` cobrindo propagação de provider customizado, preservação do aborto OAuth-only, herança de base-URL pelo codex, descarte de credenciais no modo estrito e auditoria de no-leak que varre o stderr do subprocesso procurando o valor literal do token com `RUST_LOG=trace`
- Nenhuma telemetria é emitida; a correção é silenciosa exceto quando a guarda OAuth-only dispara (que emite um arg de marcador orientativo apontando para `ANTHROPIC_AUTH_TOKEN` ou `~/.codex/auth.json` como resoluções legítimas)
- Modelo de ameaça: valores de credencial para providers customizados fluem do processo orquestrador para o subprocesso LLM pela fronteira de processo. O teste de auditoria de no-leak previne regressões futuras onde uma macro `tracing` possa imprimir o token bruto no stderr. Operadores em hosts compartilhados devem preferir `--strict-env-clear` para evitar encaminhar segredos
- Veja `docs/decisions/adr-0041-preserve-custom-provider-env.md` (PT-BR) e `.md` (EN) para a decisão arquitetural completa e alternativas consideradas

## v1.0.87+ Camada de Validação Pre-flight (ADR-0045)
- Todo spawn de subprocesso LLM passa por src/spawn/preflight.rs (15 testes unitários, 7 guardas) ANTES do fork. Falhas retornam AppError::PreFlightFailed (exit code 16, EX_CONFIG).
- 7 guardas: check_argv_size, check_binary_exists, check_mcp_config_inline (substitui o literal --mcp-config '{}' por tempfile, corrige BUG-2), check_mcp_config_path, check_walkup_mcp_json (valida o walk-up de .mcp.json, corrige BUG-5), check_output_buffer (corrige BUG-4), check_claude_config_dir (evita vazamento de MCP).
- Bypass: SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1 desabilita todas as 7 guardas. Opt-out de último recurso; o bypass reverte para Command::spawn() direto e herda todas as 5 classes de BUG.
- Hotfixes da v1.0.88: BUG-11 (falha de preflight em extract/llm_embedding.rs não propagava para remember; corrigido com embed_via_backend_strict), BUG-12 (OAuth-only emitia 2 linhas idênticas no stderr; corrigido com stderr de linha única), BUG-13 (link --create-missing burlava a validação de nome de entidade; corrigido validando ANTES de normalizar em entity_validation_integration.rs, 8 testes, fronteira de 4 caracteres).
- Veja docs/decisions/adr-0045-preflight-validation-layer.md e adr-0046-preflight-remediation.md para a decisão arquitetural completa.

## v1.0.89 Remediação do Pipeline de Embedding e Correções de Segurança (ADR-0050)
- BUG-YES-FLAG-IGNORED: três comandos destrutivos (slots release, purge, cleanup-orphans) declaravam --yes mas executavam deleções sem ele. Todos agora abortam com AppError::Validation quando --yes está ausente, alinhando com os 5 outros comandos destrutivos que já aplicavam isso
- BUG-BOOLISH-ENV: quatro flags booleanas de CLI (--skip-embedding-on-failure, --strict-env-clear, --dry-run-backend, --llm-slot-no-wait) rejeitavam valores Unix padrão de env (1, yes, on) com exit 2. Corrigido via BoolishValueParser. Scripts que definem SQLITE_GRAPHRAG_SKIP_EMBEDDING_ON_FAILURE=1 agora funcionam corretamente
- BUG-STRICT-ENV-PROPAGATION: a flag de CLI --strict-env-clear era silenciosamente ignorada porque main.rs não a propagava para a env var. Corrigido: agora propagada via set_var antes do dispatch do comando
- GAP-FLAGS-MORTAS: 7 flags globais de LLM eram aceitas pelo clap mas silenciosamente ignoradas porque módulos internos liam env vars diretamente. Corrigido: main.rs agora faz a ponte das flags de CLI para env vars via set_var
- GAP-RECALL-001: deadlock de embedding causado por slots de subprocesso LLM obsoletos resolvido via drop(stdin) explícito, timeout reduzido (300s para 30s), reaper de slots obsoletos e limpeza de processos órfãos do sqlite-graphrag
- Veja docs/decisions/adr-0050-embedding-deadlock-remediation.md para a decisão arquitetural completa

## v1.0.93 Tratamento de Chave API OpenRouter (ADR-0052)
- v1.0.93 introduz `--embedding-backend openrouter` que usa uma chave de API real (NÃO OAuth) para chamadas REST diretas ao OpenRouter
- A chave é fornecida via flag `--openrouter-api-key` ou variável `OPENROUTER_API_KEY`
- A chave é encapsulada em `secrecy::SecretString` e zeroizada no drop — JAMAIS mantida como String plana na memória após inicialização
- A chave JAMAIS é logada no stderr mesmo em nível `RUST_LOG=trace`
- A chave JAMAIS é persistida no `graphrag.sqlite` ou em qualquer arquivo de cache
- A chave JAMAIS é encaminhada para subprocessos LLM (claude, codex, opencode) — flui apenas para chamadas HTTPS `reqwest` para `api.openrouter.ai`
- Isto é SEMANTICAMENTE DISTINTO do enforço OAuth-only nos backends LLM: `ANTHROPIC_API_KEY` e `OPENAI_API_KEY` continuam ABORTANDO com exit 1
- A variável `OPENROUTER_API_KEY` NÃO está na whitelist de env-clear — permanece apenas no processo pai
- Operadores em hosts compartilhados DEVEM preferir a flag `--openrouter-api-key` ao invés da variável para minimizar janela de exposição
- Veja `docs/decisions/adr-0052-openrouter-embedding-backend.md` para a decisão arquitetural completa

## Hall da Fama
- Reconhecemos publicamente pesquisadores que reportam vulnerabilidades de forma responsável
- Esta seção está aberta a contribuições: seu nome será adicionado após divulgação coordenada
- Se preferir anonimato, respeitamos essa preferência sem exceção


## Melhores Práticas para Usuários
- SEMPRE instale releases publicadas com `cargo install sqlite-graphrag --locked`
- Use `cargo install --path .` apenas quando estiver testando intencionalmente um checkout local não publicado
- SEMPRE rotacione seus tokens de API do `crates.io` em intervalo regular
- SEMPRE mantenha sua toolchain rustc atualizada na última release estável compatível com MSRV 1.88
- SEMPRE revise entradas do CHANGELOG antes de atualizar entre versões majors
- JAMAIS commite segredos ou tokens no repositório ou em forks derivados
- JAMAIS desabilite o memory guard em produção via flags não documentadas
- JAMAIS eleve concorrência de comandos pesados cegamente em hosts com memória restrita; prefira execução serial em auditorias
- JAMAIS ignore warnings do `cargo audit` sem abrir um advisory de segurança rastreado
- JAMAIS defina `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` no ambiente; o spawn abortará com exit 1
- JAMAIS dependa do encaminhamento de `ANTHROPIC_AUTH_TOKEN` quando o host é compartilhado com processos não confiáveis; prefira `--strict-env-clear` para que credenciais permaneçam apenas no processo pai
- JAMAIS faça commit de valores `OPENROUTER_API_KEY` no repositório ou em forks derivados
- SEMPRE use a flag `--openrouter-api-key` em vez da variável de ambiente em hosts compartilhados


## v1.0.94 Hardening do Modo Headless (ADR-0053)
- A v1.0.94 torna `enrich --mode` OBRIGATÓRIO (removido o default `claude-code`); omitir é rejeitado pelo clap com exit 2.
- Isso evita um spawn acidental de `claude -p` que herdaria o `.mcp.json` do projeto do chamador e executaria servidores MCP não confiáveis em contexto headless.
- Nenhum novo exit code e nenhuma nova variável de ambiente são introduzidos; a mudança é apenas uma superfície de default mais segura.
- Modos válidos são `claude-code`, `codex`, `opencode`; escolha o que casa com seu `--llm-backend`.


## v1.0.95 Tratamento de Chave de Chat OpenRouter (ADR-0054)
- A v1.0.95 adiciona `enrich --mode openrouter`, que roteia a etapa JUDGE ao `/chat/completions` do OpenRouter via HTTPS (`src/chat_api.rs`) em vez de spawnar uma CLI local.
- Ele reutiliza a MESMA `OPENROUTER_API_KEY` já documentada para o backend de embedding, com o MESMO tratamento: envolvida em `secrecy::SecretBox`, zeroizada no drop, JAMAIS logada, JAMAIS passada a qualquer subprocesso.
- A chave flui apenas para o cliente HTTPS `reqwest` que aponta para `openrouter.ai`; não está na whitelist de env-clear e permanece apenas no processo pai.
- Nenhuma nova superfície de credencial é introduzida além da já documentada para o backend de embedding OpenRouter.
