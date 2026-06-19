# ADR-0047: Deduplicação de Stderr — OAuth Linha Única + Portão de Tracing em slots.rs (v1.0.88)

- **Status**: Aceito
- **Data**: 2026-06-19
- **Versão**: v1.0.88 (fecha GAP-15 + followup do BUG-12)
- **Autores**: Danilo Aguiar <daniloaguiarbr@gmail.com>

## Contexto

Dois bugs distintos de amplificação de stderr apareceram após o release da v1.0.87:

### BUG-12 — Stderr de linha dupla por OAuth-only enforcement

`src/output.rs:141` (`output::emit_error`) roteava o envelope de erro através TANTO de `tracing::error!` (que renderiza para stderr via o tracing-subscriber) QUANTO de um `eprintln!` direto da mesma mensagem. O resultado eram 2 linhas de stderr por violação — a camada de tracing adicionava uma linha formatada com prefixo `[ERROR]`, e o `eprintln!` adicionava a linha de mensagem crua. Operadores rodando `sqlite-graphrag ... 2>err.log | tee out.json` viam linhas duplicadas. O teste de integração `oauth_stderr_emits_single_line_v1088` foi adicionado na v1.0.88 especificamente para falhar sob a saída duplicada.

### GAP-15 — Expansão de escopo de println! em slots.rs

`src/commands/slots.rs` usava `println!` diretamente para emitir diagnósticos de acquire/release de slot. Enquanto `println!` escreve em stdout (o stream NDJSON estruturado), os comandos deste arquivo rodam como parte do despachante global da CLI — `println!` aqui burla ambos:

- o envelope JSON que outros comandos emitem em stdout
- o portão do tracing-subscriber que filtra por `RUST_LOG` / `--log-level`

O resultado era que `sqlite-graphrag slots status` produzia texto de forma livre em stdout em vez do envelope JSON estruturado que o resto da CLI retorna para a mesma consulta.

## Decisão

Duas correções mínimas e cirúrgicas na v1.0.88:

### Correção 1 — BUG-12: remover o `eprintln!` redundante

Em `src/output.rs:141`, a chamada `eprintln!("{}", msg)` após `tracing::error!({msg})` é removida. A chamada `tracing::error!` sozinha é suficiente — o tracing-subscriber renderiza a linha formatada para stderr exatamente uma vez, e `RUST_LOG` / `--log-level` continuam governando se a linha aparece ou não.

```rust
// Antes (v1.0.87):
pub fn emit_error(code: u8, msg: &str) {
    tracing::error!(target: "output", code, msg);
    eprintln!("{}", msg);  // <-- removido na v1.0.88
}

// Depois (v1.0.88):
pub fn emit_error(code: u8, msg: &str) {
    tracing::error!(target: "output", code, msg);
}
```

### Correção 2 — GAP-15: substituir `println!` em slots.rs por `crate::output::emit_info`

Em `src/commands/slots.rs`, todas as 5 ocorrências de `println!` são substituídas por `crate::output::emit_info(msg)` (ou `tracing::info!` onde a mensagem é puramente diagnóstica e não se destina ao envelope de stdout). A divisão é:

- `println!("slot acquired: ...")` → `crate::output::emit_info("slot acquired: ...")` — roteia via tracing-subscriber para stderr
- `println!("slot released: ...")` → `crate::output::emit_info("slot released: ...")` — idem
- `println!("acquire timed out ...")` → `crate::output::emit_info("acquire timed out ...")` — idem
- `println!` no ramo de saída JSON de `slots status` → removido inteiramente (o envelope JSON é a única saída de stdout)
- `println!` na confirmação de release de slot → `tracing::info!` (puramente diagnóstico, sem envelope necessário)

Isso garante:

- `sqlite-graphrag slots status` agora emite o mesmo formato de envelope JSON que todo outro comando (consistente com a cadeia de fallback de captura de stderr da ADR-0040)
- Operadores que pipeam `... 2>err.log` veem diagnósticos de slot uma vez em stderr (pelo padrão da Correção 1)
- `RUST_LOG=warn` silencia diagnósticos de slot; `RUST_LOG=debug` os mantém visíveis

## Consequências

### Positivas

- stderr emite EXATAMENTE 1 linha por violação OAuth-only enforcement (validado pelo teste de integração `oauth_stderr_emits_single_line_v1088`)
- stderr emite EXATAMENTE 1 linha por evento de acquire/release de slot (validado pelo teste de integração `slots_no_println_integration`, `slot_status_emit_info_not_println`)
- `sqlite-graphrag slots status --json` retorna um envelope JSON parseável end-to-end (validado por `slots_status_returns_parseable_json`)
- `RUST_LOG` / `--log-level` governam consistentemente os diagnósticos de slot
- 1 syscall a menos por violação OAuth (`eprintln!` realiza uma syscall `write(2)` que agora é eliminada)

### Negativas

- Operadores que dependiam das linhas stderr duplicadas (parseando ambas as linhas de uma violação) precisam atualizar seus parsers de log para esperar 1 linha. Mitigação: a linha sobrevivente contém tanto o `code` estruturado quanto a `msg` legível por humanos, então nenhuma informação é perdida.
- Scripts que grepavam a saída de `slots.rs` por substrings `acquired` ou `released` continuam funcionando porque o texto diagnóstico é preservado verbatim em `emit_info`.

## Alternativas Consideradas

1. Usar apenas `tracing::error!`, não tocar em `slots.rs` — REJEITADO: deixa GAP-15 sem tratamento; slots.rs continuaria produzindo stdout de forma livre.
2. Rotear saída de `slots.rs` para um arquivo separado (`--slots-log`) — REJEITADO: adiciona uma nova flag e um novo conceito sem tratar a duplicação.
3. Usar `eprintln!` em slots.rs (casando com o padrão pré-fix do output.rs) — REJEITADO: dobra o ruído de stderr na direção oposta.
4. Substituir ambos `println!` E `eprintln!` globalmente por apenas `emit_info` / `emit_error` — DIFERIDO: fora de escopo para v1.0.88; rastreado como débito técnico para o audit pass da v1.0.89.

## Cross-references

- ADR-0011 (OAuth-only enforcement — a violação que BUG-12 duplicava em stderr)
- ADR-0040 (cadeia de fallback de captura de stderr — ortogonal mas adjacente: esta ADR garante 1 linha por erro, ADR-0040 garante que a linha sobreviva ao redirecionamento `2>`)
- ADR-0037 (envelope JSON de shutdown — define o formato de envelope JSON que slots.rs agora também emite)
- ADR-0045 (camada de validação preflight — falhas de preflight também fluem por `output::emit_error`, então a Correção 1 também deduplica linhas de erro de preflight)
- ADR-0046 (remediação de preflight — a correção BUG-12 está incluída nas consequências daquela ADR; esta ADR é a referência canônica para a decisão de deduplicação de stderr)
- `src/output.rs:141` (`emit_error` após a Correção 1)
- `src/commands/slots.rs` (5 sites de `println!` substituídos)
- `tests/oauth_stderr_emits_single_line_v1088.rs` (teste de regressão para BUG-12)
- `tests/slots_no_println_integration.rs` (teste de regressão para GAP-15)

## Não-objetivos (YAGNI)

- NÃO introduzir um formato estruturado de stderr (o formatador `tracing::error!` atual é suficiente)
- NÃO remover `eprintln!` de outros call sites fora de `output::emit_error` (cada um é revisado independentemente)
- NÃO mudar a semântica de `RUST_LOG`
- NÃO adicionar nova flag para verbosidade de stderr

## Próximos passos

- v1.0.89: audit pass sobre os sites restantes de `eprintln!` / `println!` para o mesmo padrão
- v1.0.89: ADR para formato de stderr estruturado (JSON-por-linha) se parsers de log downstream demandarem
