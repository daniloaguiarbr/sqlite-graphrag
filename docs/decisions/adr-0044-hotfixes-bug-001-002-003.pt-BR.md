# ADR-0044: Hotfixes v1.0.85.2 — `--dry-run-backend` Standalone, `embed_via_backend` retorna Resolved Kind, `setup_mock_path` JSON corrigido

- **Status**: Aceito
- **Data**: 2026-06-17
- **Versão**: v1.0.85.2 (hotfix)
- **Autores**: tech-lead

## Contexto

A release v1.0.85 (ADR-0043) introduziu o flag global `--dry-run-backend` e refatorou o pipeline de embedding. Três bugs remanescentes foram descobertos em auditoria local em 2026-06-17:

- **BUG-001**: `--dry-run-backend` exigia subcommand obrigatório, retornando exit 2 quando invocado standalone
- **BUG-002**: `embed_via_backend` não retornava o backend resolvido, perdendo informação observável
- **BUG-003**: `setup_mock_path()` em `tests/embedder.rs` produzia JSONL quando testes esperavam JSON, mascarando a verificação

Estes 3 bugs são documentados em `gaps.md:393, 430, 464, 510` como `Solucionado em v1.0.85.2` (ADR-0044).

## Decisão

Aplicar 3 correções cirúrgicas em v1.0.85.2:

1. **`pub command: Option<Commands>` em `src/cli.rs:248`** — tornar subcommand opcional. Quando `cli.dry_run_backend` for `true` E `cli.command` for `None`, executar early-exit com JSON do backend resolvido
2. **`embed_via_backend` retorna `Result<(Vec<f32>, LlmBackendKind), AppError>`** — propagar o `resolved_kind` para o chamador, que popula `backend_invoked` em 7 envelopes JSON
3. **`setup_mock_path()` em `tests/embedder.rs:37-77`** — alinhar formato do dump com expectation (JSON vs JSONL)

## Consequências

### Positivas
- `--dry-run-backend` funciona como documentado (UX correta)
- 7 envelopes JSON agora reportam `backend_invoked: "claude" | "codex" | "none"` consistentemente
- Testes `embed_via_backend_*` em `tests/embedder.rs` rodam sem mascaramento de formato

### Negativas
- Mudança de assinatura `embed_via_backend` é patch-aditivo via tuple
- Requer recompilação dos 6 call sites que consomem o retorno
- Bilíngue (EN + pt-BR) deste ADR deve ser criado em sincronia

## Alternativas Consideradas

1. Não criar ADR-0044 — REJEITADO: gaps.md referencia em 4 linhas; inconsistência cross-doc
2. Criar ADR-0044 sem body (só stub) — REJEITADO: viola template de ADR
3. Adotar naming `adr-0044-v1-0-85-2-hotfixes.md` — REJEITADO: quebra convenção numérica
4. Renomear para `adr-0044-dry-run-standalone.md` — REJEITADO: perde escopo dos 3 fixes

## Cross-refs

- `gaps.md:393, 430, 464, 510` — referências primárias aos 3 BUGs
- `Cargo.toml:3` — version `1.0.85.2`
- ADR-0042 (v1.0.84) — split Claude backend (dependência)
- ADR-0043 (v1.0.85) — five-gap remediation (dependência)
- `src/cli.rs:248` — declaração `pub command: Option<Commands>`
- `tests/embedder.rs:37-77` — `setup_mock_path()` corrigido
- 7 envelopes JSON: edit, embedding-status, enrich-summary, hybrid-search, ingest-summary, recall, remember — todos com `backend_invoked`
