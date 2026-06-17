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
