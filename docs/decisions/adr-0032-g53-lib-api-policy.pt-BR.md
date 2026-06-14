# ADR-0032: Política de Estabilidade da API da Biblioteca (G53)

## Status
- Aceito (2026-06-13)
- Decisores: Danilo Aguiar
- Escopo: `Cargo.toml`, `README.md`, `README.pt-BR.md`, `.github/workflows/ci.yml` (job semver-checks)
- v1.0.80 — este ADR formaliza a decisão que a auditoria G53 sinalizou como ABERTO.


## Contexto
- `sqlite-graphrag` é publicado como crate dual lib+bin no crates.io.
- A biblioteca é consumida por um conjunto pequeno de casos de uso embarcados (ex.: servidores MCP customizados que envolvem o binário, serviços de longa duração que importam `storage::memories` diretamente).
- Até v1.0.79, a superfície publicada da lib tem 9 mudanças quebrantes de nível MAJOR vs v1.0.78 segundo `cargo semver-checks --baseline-version 1.0.78`:
  - 7 remoções de trait (família `extraction_gliner::Extractor`)
  - 2 remoções de re-export de tipo
- Essas mudanças saíram em bump **patch** (1.0.78 -> 1.0.79) porque nenhuma release publicada aplica um gate `cargo semver-checks` em CI.
- Consumidores que fixaram em `^1.0.78` tiveram seus builds quebrando no `cargo update`.


## Decisão
- A **CLI é o contrato público estável**. Os envelopes `--json` documentados em `docs/schemas/*.schema.json` e as variáveis de ambiente listadas em `llms.txt` e `llms-full.txt` permanecem estáveis em todas as versões v1.x.y. Bumps em qualquer direção (1.0.78 -> 1.1.0 ou 1.1.0 -> 2.0.0) DEVEM preservar o contrato da CLI ou migrá-lo através de um ciclo de deprecação documentado.
- A **API da biblioteca é instável** dentro de v1.x.y. Re-exports, campos públicos de struct e assinaturas de função podem mudar em qualquer release v1.x.y.
- Mudanças quebrantes na API da biblioteca saem como bump **minor** (ex.: 1.0.79 -> 1.1.0), nunca patch. Bumps de patch (1.0.79 -> 1.0.80) são limitados a mudanças aditivas sem quebra na superfície da lib.
- Um job `cargo semver-checks` é adicionado ao CI como **INFORMATIVO** em v1.0.80 (`continue-on-error: true`) para que PRs existentes não sejam bloqueados antes das 9 violações MAJOR atuais serem resolvidas. O job é promovido a **BLOQUEANTE** em v1.0.81 uma vez que uma baseline limpa seja estabelecida.


## Consequências
### Positivas
- Consumidores da CLI (caso de uso dominante) ganham contrato estável e previsível em todas as releases v1.x.y.
- Consumidores da biblioteca são explicitamente informados de que a superfície da lib é instável, removendo a falsa expectativa de garantias SemVer.
- `cargo semver-checks` está no CI a partir de v1.0.80, fornecendo visão estruturada do drift da API da lib ao longo do tempo.
- As 9 violações MAJOR atuais viram dívida rastreada e visível, em vez de fonte invisível de regressão.

### Negativas
- Consumidores da biblioteca devem fixar versão exata e ler CHANGELOG.md em todo bump minor. Workflow de fricção maior que a garantia implícita `^1.0.80`.
- A regra de bump minor para remoções significa que uma trilha v1.0.x pode acumular dívida de API da lib que só se resolve em v2.0.0. Aceitamos este trade-off: um cgroup 1.x.y de releases é tratado como ciclo "lib API pode mudar", com v2.0.0 como única quebra dura.

### Mitigação
- `Cargo.toml` expõe o shorthand SemVer padrão `^1.0`, então `cargo add sqlite-graphrag` adota "seguir estabilidade da CLI" — exatamente a intenção.
- Consumidores da biblioteca que precisam de fixação estão documentados para usar sintaxe `sqlite-graphrag = "=1.0.80"`.
- `CHANGELOG.md` e `CHANGELOG.pt-BR.md` são atualizados em toda release com seção "Mudanças na API da Biblioteca" listando re-exports removidos, mudanças em campos públicos de struct e mudanças de assinatura explicitamente.


## Alternativas Consideradas
1. **Adotar v2.0.0 para remoções, manter patch estrito para v1.x.y.** Rejeitado porque exigiria publicar v1.0.80, v1.0.81 ... v1.0.89 com zero mudanças na API da lib, acumulando dívida técnica sem nunca publicar. O caminho de bump minor permite movimento para frente.
2. **Bumpar para 2.0.0 imediatamente na transição v1.0.79 -> v1.0.80.** Rejeitado porque v1.0.79 é a release de produção atual e as mudanças quebrantes lá já foram publicadas sob bump patch; bumpar retroativamente para 2.0.0 seria mentira documental.
3. **Manter comportamento atual (sem política, sem gate em CI).** Rejeitado porque G50 documentou que 6 das últimas 7 execuções de CI terminaram em failure incluindo a release v1.0.79; continuar sem gate é vetor de regressão conhecido.


## Relacionado
- G50: CI Vermelho Não Bloqueia Release (motivação para o gate)
- G53: Processo de Release (gap pai)
- ADR-0011: OAuth-only enforcement (precedente para postura CLI-as-contract)
- `docs/decisions/adr-0028-g41-phantom-v013-registration.md` (precedente para escopo de ADR = um gap ou uma decisão)


## Implementação
- `Cargo.toml`: sem mudança. SemVer padrão já está correto para a política.
- `README.md` e `README.pt-BR.md`: nova seção "Política de Estabilidade" adicionada entre "Por que sqlite-graphrag?" e "Superpoderes para Agentes de IA" em v1.0.80.
- `.github/workflows/ci.yml`: novo job `semver-checks` com `continue-on-error: true` contra `--baseline-version 1.0.79`. Promovido a bloqueante em v1.0.81.
- `llms.txt` e `llms-full.txt`: sem mudança necessária (doc da CLI inalterado).
