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
