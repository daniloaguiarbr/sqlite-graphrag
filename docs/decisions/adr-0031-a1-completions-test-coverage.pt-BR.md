# ADR-0031 — Auditoria A1: Cobertura de Teste de Completions de Shell (v1.0.80)

## Status

Aceito (v1.0.80, 2026-06-14).

## Contexto

A suíte de auditoria da v1.0.80 (ciclo de auditoria A1, escopo:
cobertura de superfície da CLI) identificou que o
subcomando `completions` (adicionado em v1.0.67) tinha
zero cobertura de teste end-to-end apesar de documentar
suporte a 5 shells (bash, zsh, fish, powershell, elvish).
Os 5 shells suportados eram um contrato público
documentado, mas a única verificação era invocação manual
de `sqlite-graphrag completions <shell>` e inspeção visual
do script gerado. Não havia teste que afirmasse (a) exit
code 0 para shells válidos, (b) os marcadores de
completion-script esperados por shell, (c) exit não-zero
para um shell desconhecido, ou (d) output não-vazio para
todo shell suportado.

## Decisão

Um novo arquivo de teste de integração `tests/completions.rs`
adiciona 7 testes end-to-end para o subcomando `completions`.
Os testes exigem um build de debug local; se o binário
estiver ausente (ex.: clone fresco de `cargo check`), eles
auto-pulam via uma checagem `binary_exists` no topo de cada
teste. Isso mantém a suíte de teste verde em ambientes
de CI que rodam `cargo test --no-run` sem compilar o
binário, enquanto ainda captura regressões em ambientes
que compilam e rodam o binário.

Os 7 testes cobrem:

1. `completions_bash_emits_script` — afirma exit 0, output
   contém os marcadores `complete` e `_sqlite-graphrag`.
2. `completions_zsh_emits_script` — afirma exit 0, output
   contém os marcadores `#compdef` ou `_sqlite-graphrag`.
3. `completions_fish_emits_script` — afirma exit 0, output
   contém os marcadores `complete` ou `sqlite-graphrag`.
4. `completions_powershell_emits_script` — afirma exit 0,
   output contém os marcadores `Register-ArgumentCompleter`
   ou `sqlite-graphrag`.
5. `completions_elvish_emits_script` — afirma exit 0, output
   contém os marcadores `edit:completion:arg-completer` ou
   `sqlite-graphrag`.
6. `completions_invalid_shell_exits_nonzero` — afirma que
   `not-a-real-shell` produz exit não-zero (rejeição do
   clap `ValueEnum`, exit 2).
7. `completions_emits_nonempty_output_for_each_shell` —
   itera sobre os 5 shells suportados, afirma exit 0 e
   comprimento de output > 50 bytes, e escreve o output
   em um `tempfile::NamedTempFile` para impedir que o
   teste seja otimizado away.

## Consequências

Positivas:

- O contrato de completions de 5 shells agora é apoiado
  por testes automatizados; qualquer refatoração futura
  de clap ou subcomando que quebre um dos 5 shells é
  pega pela CI antes do release.
- O comportamento de auto-skip mantém a suíte de teste
  verde em ambientes `cargo test --no-run` sem
  comprometer a verificação em ambientes que compilam
  o binário.
- O 7º teste escreve o script de completion gerado em
  um tempfile, o que torna o script capturado visível
  nos logs de teste (útil para debugar upgrades futuros
  do clap).

Negativas:

- Os testes exigem um build de debug local; runners de
  CI que não compilam o binário (ex.: `cargo test
  --no-run`) pulam os testes silenciosamente. Runners
  de CI que COMPILAM o binário (o default) exercitam
  os testes. O README e o workflow de CI documentam
  isso.
- As asserções de marcadores (`_sqlite-graphrag`,
  `#compdef`, etc.) estão acopladas ao texto do
  script gerado; um upgrade futuro do clap que mude
  esses marcadores exigirá atualizar os testes.

## Referências

- `tests/completions.rs:1-153` (7 testes end-to-end)
- `docs/HOW_TO_USE.pt-BR.md` → "Como Instalar Completions
  de Shell"
- v1.0.67 (introdução inicial do subcomando `completions`)
- Ciclo de auditoria A1 (v1.0.80, escopo: cobertura de
  superfície da CLI)

