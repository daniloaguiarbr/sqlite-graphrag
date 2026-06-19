# ADR-0048: Schema como Artefato Derivado via schemars + Must-Ignore (v1.0.89)

- **Status**: Aceito
- **Data**: 2026-06-19
- **Versão**: v1.0.89 (fecha GAP-E2E-007 P1)
- **Autores**: Danilo Aguiar <daniloaguiarbr@gmail.com>

## Contexto

`docs/schemas/health.schema.json` era um arquivo JSON Schema mantido manualmente. Declarava apenas as chaves conhecidas no momento da autoria mais algumas adições incrementais. Quando a v1.0.89 chegou, a struct `HealthResponse` em `src/commands/health.rs` já emitia 36 chaves (23 sempre presentes + 13 condicionais `Option<T>` via `skip_serializing_if`). O schema commitado cobria apenas 21 dessas chaves — um drift de 15 campos que nunca foram refletidos de volta no schema.

O drift era agravado por uma violação de política: o schema usava `additionalProperties: false` (Must-Validate), enquanto a regra do projeto `docs_rules/rules_rust_json_e_ndjson.md` linha 537 obriga `Must-Ignore` para APIs que evoluem ao longo do tempo com compatibilidade retroativa. O schema manual era simultaneamente incompleto E violador de política.

A causa raiz é estrutural: schemas escritos à mão não conseguem acompanhar structs Rust que ganham campos a cada release menor. Qualquer adição a `HealthResponse` (ex.: v1.0.65 adicionou 6 campos de qualidade de grafo; v1.0.67 adicionou `non_normalized_count` / `normalization_warning`; v1.0.67 também adicionou 4 campos de super-hub) silenciosamente alargou o contrato sem atualizar o arquivo de schema. Consumidores rodando validação estrita rejeitariam respostas que contivessem campos considerados desconhecidos.

## Decisão

Adotar `schemars = "0.8"` como dependência regular e gerar o schema a partir dos tipos Rust em tempo de build:

### 1. Adicionar `schemars = "0.8"` em `[dependencies]`

Fixado em 0.8 porque schemars 1.0 introduziu mudanças quebrantes de API (notadamente a assinatura da macro `schema_for!` e o enum `Schema` reformulado). A linha 0.8 é estável, amplamente implantada e casa com o exemplo documentado em `context7 docs /gresau/schemars` (ID `/gresau/schemars`, trustScore 8.8).

`schemars` vive em `[dependencies]` (não em `[dev-dependencies]`) porque a macro derive `JsonSchema` é aplicada à struct de produção `HealthResponse` em `src/commands/health.rs` — a crate lib precisa da macro em tempo de compilação, não apenas nos testes.

### 2. Derivar `JsonSchema` nos tipos de resposta do health

Três structs recebem o derive:

```rust
#[derive(Serialize, schemars::JsonSchema)]
pub struct HealthResponse { /* 36 campos */ }

#[derive(Serialize, schemars::JsonSchema)]
pub struct HealthCounts { /* 5 campos */ }

#[derive(Serialize, schemars::JsonSchema)]
pub struct HealthCheck { /* 3 campos */ }
```

`HealthCounts` e `HealthCheck` precisaram ser promovidas de privadas para `pub` porque o schema gerado referencia elas via `$ref` a partir de `HealthResponse.properties`, e `schema_for!` requer `JsonSchema` no escopo.

### 3. Criar `src/bin/dump_schema.rs` para regeneração idempotente

Um binário dedicado consome `schema_for!(HealthResponse)`, aplica duas transformações de pós-processamento e grava o resultado em `docs/schemas/health.schema.json`:

- Eleva `$schema` para `https://json-schema.org/draft/2020-12/schema` (conforme `docs_rules/rules_rust_json_e_ndjson.md` linha 555).
- Seta recursivamente `additionalProperties: true` em cada object com campo `properties` (Must-Ignore conforme linha 537).

O bin é **idempotente** — rodá-lo duas vezes produz saída byte-idêntica (checksum BLAKE3 `6230564bde8067dc3126e0c8c3027829c2eb0375b54fba76f8c09f51aa8c7c07` confere entre execuções).

### 4. Regenerar `docs/schemas/health.schema.json`

O schema regenerado agora contém 36 properties (casa com `HealthResponse` exatamente), `additionalProperties: true` na raiz e em cada object aninhado, e metadados Draft 2020-12.

### 5. Adicionar `tests/health_schema_drift_regression.rs` com 4 testes de regressão

- `assert_all_health_keys_in_schema` — verifica que 36 chaves conhecidas estão presentes.
- `assert_must_ignore_policy_active` — verifica `additionalProperties: true` na raiz.
- `assert_draft_2020_12` — verifica que o schema declara Draft 2020-12.
- `assert_dump_schema_is_idempotent` — roda o bin duas vezes e compara checksums BLAKE3.

## Consequências

### Positivas

- Schema regenerado automaticamente a partir dos tipos Rust — drift se torna estruturalmente impossível.
- Todas as 36 chaves sempre em sincronia com a struct `HealthResponse` (campos existentes mais adições futuras).
- Política Must-Ignore alinhada com `docs_rules/rules_rust_json_e_ndjson.md` linha 537.
- Alinhamento com Draft 2020-12 conforme linha 555.
- Consumidores que usam validação estrita (`additionalProperties: false`) no lado consumidor continuam funcionando porque o schema agora permite campos extras — forward-compatible.
- 4 testes de regressão impedem drift futuro via falha de CI.

### Negativas

- MUDANÇA QUEBRANTE para qualquer consumidor que dependia de `additionalProperties: false` para capturar typos em campos desconhecidos. O schema agora aceita campos extras, então consumidores devem migrar para Must-Ignore OU explicitamente optar pelo modo estrito no lado deles.
- Consumidores que usam validação estrita `jsonschema` não capturarão novos campos que não viram antes — este é o trade-off documentado do Must-Ignore.
- `schemars` adiciona aproximadamente 2 MB à árvore de dependências em tempo de build mas não afeta o tamanho do binário final porque `schemars` é usado apenas via macros derive (custo zero em runtime no CLI compilado).

## Alternativas Consideradas

1. **Manter schema manual, adicionar item de checklist no processo de release** — REJEITADO: o drift é sintoma de um fardo de manutenção insolúvel; checklists não impedem erro humano.
2. **Usar `schemars` mas manter Must-Validate (`additionalProperties: false`)** — REJEITADO: viola `docs_rules/rules_rust_json_e_ndjson.md` linha 537 diretamente.
3. **Usar `schemars` com auto-detecção de strictness por campo** — DIFERIDO: complexo, requer anotação por campo; fora do escopo da v1.0.89.
4. **Mudar para ferramenta diferente de geração de schema (ex.: `typify`, `schemars-derive`)** — REJEITADO: `schemars` é o padrão de fato no ecossistema Rust; trocar adiciona fricção sem resolver o problema central.

## Cross-referências

- `context7 docs /gresau/schemars` (ID `/gresau/schemars`, trustScore 8.8) — documentação da API schemars 0.8
- `docs_rules/rules_rust_json_e_ndjson.md` linhas 33, 537, 547, 555 — mandato Must-Ignore, mandato Draft 2020-12
- RFC 7493 (I-JSON) — definição de Must-Ignore (`additionalProperties` padrão true)
- `src/commands/health.rs::HealthResponse` — fonte da verdade
- `src/bin/dump_schema.rs` — binário de regeneração
- `tests/health_schema_drift_regression.rs` — cobertura de regressão
- ADR-0047 (deduplicação de stderr) — ortogonal mas adjacente: demonstra o valor de fonte única de verdade para questões transversais

## Não-objetivos (YAGNI)

- NÃO gerar schemas para os outros 48 arquivos de schema em `docs/schemas/` — essa é tarefa da v1.1.0 com ADR próprio.
- NÃO introduzir validador de schema em runtime em produção — `jsonschema` permanece dev-dependency apenas para o teste de regressão.
- NÃO remover os campos de metadados do schema manual (`$id`, `title`, `description`) — schemars já os popula.
- NÃO invocar `dump_schema` automaticamente no pipeline de build — o teste de regressão invoca o bin e verifica idempotência, o que é enforcement suficiente.

## Próximos passos

- v1.0.90: estender `dump_schema` para cobrir `codex-models.schema.json` (já bem definido, vitória fácil)
- v1.1.0: auditar todos os 49 schemas em `docs/schemas/` e migrar cada um para tipos deriváveis via schemars
- v1.1.0: integrar `dump_schema` em hook pre-commit para prevenir drift não-commitado
