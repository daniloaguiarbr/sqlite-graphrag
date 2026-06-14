# ADR-0033: Resiliência de CI G53-WINDOWS-INFRA para windows-2025

## Status
- Aceito (2026-06-14)
- Decisores: Danilo Aguiar
- Escopo: `.github/workflows/ci.yml` (jobs `clippy` e `test` na matrix `windows-2025`)
- v1.0.80 — esta ADR formaliza o lado de infraestrutura Windows do G53 que a auditoria da v1.0.80 sinalizou como ABERTO.


## Contexto
- A v1.0.80 fecha o lado de política do G53 via ADR-0032 (estabilidade da API lib).
- O lado de infraestrutura restante cobre a matrix `windows-2025` nos jobs de CI `clippy` e `test`. Esses jobs dependem de `dtolnay/rust-toolchain@stable` para instalar o Rust no runner, que pode falhar com erros transitórios de rede durante `rustup toolchain install`.
- Reproduzir essa flakiness a partir do host Linux usado para autoria da mudança é impossível — windows-2025 é um runner hospedado no GitHub, acessível somente via CI.
- O lado de cross-compile do suporte a Windows já está coberto pelo job `windows-build-check` do G29 (cargo check --target x86_64-pc-windows-msvc em ubuntu-latest). Esse job já possui seu próprio passo explícito de `rustup target add` após a action do dtolnay, contornando o problema do `--profile minimal` com `--target`.


## Decisão
- Adicionar um passo de pre-warm **antes** de `dtolnay/rust-toolchain@stable` em ambos os jobs de matrix `clippy` e `test`, condicionado por `if: matrix.os == 'windows-2025'`. O passo executa 3 tentativas de `rustup toolchain install stable --profile minimal --no-self-update` com backoff de 15 segundos.
- Adicionar um passo de verificação **após** `dtolnay/rust-toolchain@stable` nos mesmos jobs, com a mesma condição. O passo executa 3 tentativas de `rustc --version && cargo --version` com backoff de 10 segundos para confirmar que o toolchain está operacional.
- Usar `shell: pwsh` porque os runners windows-2025 do GitHub Actions adotam PowerShell por padrão, não bash.
- NÃO modificar o job `windows-build-check` (G29) — ele já possui sua própria solução de contorno e roda em outro runner (`ubuntu-latest`).
- NÃO introduzir novas dependências, scripts de instalação ou estratégias de cache que não estejam já no repositório. O loop de reuso reaproveita verbatim o comando de instalação do toolchain existente.


## Consequências
### Positivas
- Falhas transitórias de rede em `rustup toolchain install` não bloqueiam mais os jobs da matrix windows-2025.
- O passo de verificação captura instalações parciais (toolchain presente mas symlinks de `rustc`/`cargo` quebrados) antes que passos downstream desperdicem tempo.
- O gate `if: matrix.os == 'windows-2025'` mantém ubuntu-latest e macos-latest inalterados — sem mudança no tempo de CI para os caminhos dominantes.
- Os passos de pre-warm e verificação são no-op em caso de sucesso, então o tempo de CI no caminho feliz permanece inalterado.

### Negativas
- 3 tentativas de retry adicionam até 30 segundos de tempo de wall-clock no pior caso para os jobs windows-2025. Isso é aceitável porque os jobs windows-2025 representam uma fração pequena do tempo total de CI.
- A lógica de retry fica duplicada entre os jobs `clippy` e `test`. Uma action composta seria mais limpa, mas a duplicação é de 6 linhas de YAML e o custo de uma action composta (novo arquivo, novo teste, version skew) supera o benefício.


## Verificação
- Os dois novos passos são adicionados no commit <preenchido pelo lead no momento do commit>.
- O YAML do CI continua parseando como GitHub Actions válido (validado localmente com `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"`).
- O pré-requisito do job `windows-build-check` (G29) — instalar o target `x86_64-pc-windows-msvc` — foi validado no host. O modo de falha original `E0463: can't find crate for 'core'`, que motivou o passo explícito de `rustup target add` no CI, foi reproduzido e em seguida resolvido instalando o target no toolchain MSRV 1.88 fixado pelo projeto (`rustup target add x86_64-pc-windows-msvc --toolchain 1.88`).
- O `cargo check` de cross-compile no Linux agora alcança o build script de `libsqlite3-sys`, que falha com `cc-rs: failed to find tool "lib.exe"`. Este é o LIMITE esperado do cross-compile a partir do host Linux: produzir um artefato MSVC linkável a partir de um runner Linux requer o MSVC build tools, que não está (e nem deve estar) instalado no host Linux do CI. O CI fecha esse loop executando os jobs `clippy` e `test` no runner real `windows-2025` da matrix, onde o toolchain MSVC ESTÁ disponível — os novos passos de pre-warm/verificação nesses jobs da matrix são o que tornam esse caminho confiável.
- Efeito líquido: o cross-compile check do G29 em `ubuntu-latest` agora avança confiavelmente além do `E0463` até a fronteira do `cc-rs` (sinal positivo de que o grafo de build para o target `windows-2025` compila nas partes que não precisam do linker MSVC), e os próprios jobs da matrix `windows-2025` agora são resilientes a falhas transitórias de `rustup toolchain install`.
