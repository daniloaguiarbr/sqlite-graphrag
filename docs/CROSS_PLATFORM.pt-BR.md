# SUPORTE CROSS PLATFORM

> Um binário de 6 MB, cinco targets, zero download de modelo em todo sistema operacional moderno (v1.0.76 Apenas LLM)

- Leia este guia em inglês em [CROSS_PLATFORM.md](CROSS_PLATFORM.md)
- Volte ao [README.md](../README.md) principal para referência completa de comandos


## Nota Arquitetural da v1.0.76
- O build padrão é apenas LLM e one-shot. Não há runtime ONNX para distribuir, não há `libonnxruntime.so` para empacotar, e não há modelo `multilingual-e5-small` para baixar. A geração de embedding delega para um subprocesso headless `claude code` ou `codex` (OAuth) spawnado por chamada.
- A feature `embedding-legacy` foi REMOVIDA na v1.0.79 (antecipando o cronograma da v1.1.0). Todo build é LLM-only; o pipeline fastembed + ort + tokenizers e o contrato ONNX ARM64 GNU não se aplicam mais.
- A tabela cross-platform abaixo descreve o build LLM-only, que agora é o único build.



## A Dor Que Você Já Conhece
### Antes — Inferno de Dependências Que Custa Duas Horas
- Instalar stack RAG em Python custa duas horas entre pip, venv e extensões C
- Containers Alpine quebram com símbolos glibc ausentes em wheels Python constantemente
- Gatekeeper do macOS coloca binários não assinados em quarentena bloqueando primeira execução
- Separadores de caminho do Windows quebram scripts shell copiados direto de tutoriais Linux
- Shells diferentes interpretam regras de quoting diferentes entre Bash Zsh Fish e PowerShell


### Depois — Binário Único Que Simplesmente Roda
- Um `cargo install --locked` entrega o binário em qualquer target suportado oficialmente
- Sem runtime Python, sem runtime Node, sem JVM, e com apenas um contrato de biblioteca compartilhada no ARM64 GNU
- Startup do binário fica abaixo de oitenta milissegundos em todo target suportado
- Códigos de saída permanecem idênticos nos cinco targets publicados garantindo orquestração confiável
- Formato JSON de saída fica byte a byte idêntico em todo sistema operacional testado


### Ponte — O Comando Que Te Leva Até Lá
```bash
cargo install --path .
```


## Matriz de Suporte
### Targets — Cinco Combinações Que Publicamos e Testamos
| Target | Sistema Operacional | Arquitetura | Tamanho do Binário | Startup |
| --- | --- | --- | --- | --- |
| x86_64-unknown-linux-gnu | Linux glibc | x86_64 | ~25 MB | <50ms |
| aarch64-unknown-linux-gnu | Linux glibc | aarch64 | ~24 MB | <60ms |
| aarch64-apple-darwin | macOS | Apple Silicon | ~22 MB | <30ms |
| x86_64-pc-windows-msvc | Windows | x86_64 | ~28 MB | <80ms |
| aarch64-pc-windows-msvc | Windows | ARM64 | ~27 MB | <80ms |

- Cada linha acima recebe asset de release vinculado a cada tag publicada no GitHub
- Cada linha acima recebe smoke tests automatizados em CI a cada commit empurrado
- Manifesto SHA256SUMS acompanha cada binário para verificação de integridade imediata
- Símbolos de debug são entregues como artefatos `.dSYM` ou `.pdb` separados sob demanda
- Cross-compilação usa `cross` em hosts Linux para a célula `aarch64-unknown-linux-gnu` da matriz

### Targets de Release Não Suportados — Por Que Foram Excluídos
- `x86_64-apple-darwin` foi excluído porque o build da v1.0.76 não exige mais um caminho de ONNX Runtime pré-compilado (e macOS Intel tem sido um target de macOS deprecado há muito tempo desde 2024)
- `x86_64-unknown-linux-musl` foi excluído porque nenhuma dependência nativa glibc-only permanece no build padrão, mas um build musl não faz parte da matriz de release
- Reintroduzir qualquer um desses targets é uma tarefa rotineira de cross-compile na v1.0.76 porque nenhuma extensão C precisa ser linkada

### ARM64 GNU — Sem Mais Contrato de ONNX Runtime Compartilhado
- A v1.0.76 NÃO tem dependência de ONNX runtime no build padrão. O contrato anterior do `aarch64-unknown-linux-gnu` (`libonnxruntime.so` ao lado da binária, env var `ORT_DYLIB_PATH`) está REMOVIDO.
- Nota histórica: builds com a feature removida `embedding-legacy` (v1.0.76-v1.0.78) distribuíam `libonnxruntime.so` no `aarch64-unknown-linux-gnu`. Desde a v1.0.79 nenhuma configuração precisa do contrato.
- O contrato de dynamic loader era um artefato do pipeline fastembed da v1.0.74. Com o subprocesso LLM como modelo, a binária precisa de zero bibliotecas C compartilhadas além da libc


## Notas Para Linux
### glibc Primeiro — Caminho Oficial de Release no Linux
- Binário glibc roda em Ubuntu 20.04, Debian 11, Fedora 36 e distros mainstream
- `x86_64-unknown-linux-gnu` e `aarch64-unknown-linux-gnu` são os únicos assets Linux publicados agora
- `x86_64-unknown-linux-musl` não faz parte da matriz oficial de release desde `v1.0.16`
- Com o build LLM-only, nenhuma restrição de versão de glibc existe além do que a binária do subprocesso LLM precisa


## Notas Para macOS
### Gatekeeper — Assinatura e Notarização
- Binários não assinados baixados via navegador disparam quarentena na primeira execução
- Remova a quarentena com `xattr -d com.apple.quarantine /usr/local/bin/sqlite-graphrag`
- Binários instalados via `cargo install` ignoram Gatekeeper por virem do rustc local
- Os assets oficiais de macOS atualmente cobrem apenas Apple Silicon


### Apple Silicon — Performance Nativa em M1 M2 M3 M4
- Binário aarch64 nativo roda trinta por cento mais rápido que x86_64 via Rosetta
- macOS Intel está atualmente fora da matriz oficial de release nesta configuração do projeto
- O subprocesso LLM (`claude` ou `codex`) é o modelo; a binária Rust em si não carrega nenhum modelo
- A única latência do lado LLM é o spawn de 1-3 s do subprocesso (claude / codex) por `remember` / `recall`
- Cold start mede vinte e oito milissegundos no M2 graças ao preditor de branches melhorado


## Notas Para Windows
### Shell — PowerShell 7 e Windows Terminal
- PowerShell 7 ou posterior executa cada exemplo do README sem modificação alguma
- Windows Terminal renderiza saída colorida e barras de progresso identicamente aos shells Unix
- CMD.EXE legado funciona mas remove cores ANSI exceto se `SQLITE_GRAPHRAG_FORCE_COLOR=1`
- Usuários WSL2 devem preferir o binário Linux glibc para paridade completa com Unix
- PowerShell ISE NÃO suporta prompts interativos usados durante a confirmação do `init`


### Console UTF-8 — O Único Ajuste Necessário
```powershell
chcp 65001
$env:PYTHONIOENCODING = "utf-8"
sqlite-graphrag remember --name "memória-acentuada" --body "caracteres unicode funcionam"
```
- Code page 65001 troca o console para codificação UTF-8 renderizando caracteres corretamente
- Sem UTF-8 o binário ainda funciona mas stdout exibe caracteres de substituição nos acentos
- Windows Terminal moderno usa UTF-8 por padrão eliminando a necessidade do comando `chcp`
- Line endings permanecem LF dentro do banco SQLite independente da configuração do console
- Scripts persistem corretamente entre Windows, Linux e macOS quando salvos em UTF-8

### Tipo HANDLE e o Limite do windows-sys 0.59 (G29, v1.0.68)
- O crate `windows-sys` mudou o tipo de `HANDLE` entre 0.48/0.52 (`isize`) e 0.59+ (`*mut c_void`); a quebra foi feita pela Microsoft em [windows-rs#171]
- `cargo install sqlite-graphrag` no Windows quebrou em v1.0.67 com `error[E0308]: mismatched types` em `src/terminal.rs:29:26` porque a comparação `handle != 0 && handle as isize != -1` só era válida para o tipo antigo
- v1.0.68 substitui a comparação pelo idiom type-safe `!handle.is_null() && handle != INVALID_HANDLE_VALUE`, que funciona para ambas as eras de tipo e também captura o sentinela `INVALID_HANDLE_VALUE` (`(HANDLE)-1`) que é diferente de NULL
- `windows-sys` está fixado em `=0.59.0` exato em `Cargo.toml:111` para evitar resolução silenciosa para um futuro 0.59.x que possa quebrar o contrato de tipo novamente
- Novo job de CI `windows-build-check` em `.github/workflows/ci.yml` roda `cargo check --target x86_64-pc-windows-msvc --lib --all-features` em todo push e PR para capturar regressões futuras antes do publish
- Workaround manual para v1.0.66/v1.0.67 (apenas se você precisa ficar nessas versões): edite `~/.cargo/registry/src/index.crates.io-*/sqlite-graphrag-*/src/terminal.rs`, substitua a linha 29 por `if !handle.is_null() && handle != INVALID_HANDLE_VALUE`, e adicione `INVALID_HANDLE_VALUE` ao `use windows_sys::Win32::Foundation::{...}`.  Depois rode `cargo install --path .` a partir do source corrigido.
- Referência: `https://docs.rs/windows-sys/0.59.0/windows_sys/Win32/Foundation/type.HANDLE.html` (atual) e `https://docs.rs/windows-sys/0.52.0/windows_sys/Win32/Foundation/type.HANDLE.html` (legado)

### Resiliência de Infra Windows no CI (G53-WINDOWS-INFRA, ADR-0033, v1.0.80)
- Os jobs da matrix windows-2025 (`clippy` e `test`) ganharam 2 steps novos cada, gateados em `if: matrix.os == 'windows-2025'` (no-op em ubuntu e macos): um step de pre-warm que baixa o toolchain rustup no cache do runner antes do build, e um step de verify que re-checa `rustup show active-toolchain` após install
- Os 2 modos históricos de falha de infra agora são recuperáveis na primeira re-run em vez de acumularem como CI vermelho: (a) download do rustup com erros transitórios de rede, (b) `E0463 can't find crate for core` quando a stdlib do target está ausente
- Validação local de cross-compile: `cargo check --target x86_64-pc-windows-msvc --lib --all-features` reproduzido e o `E0463` resolvido via `rustup target add x86_64-pc-windows-msvc --toolchain 1.88`; o build então atinge a fronteira `cc-rs: failed to find tool "lib.exe"`, que é o limite esperado de cross-compile MSVC a partir de host Linux
- A label explícita `windows-2025` do runner (substituindo `windows-latest` desde a v1.0.73) continua sendo a escolha certa até a data de corte do redirect do VS2026 (2026-06-15); veja ADR-0033 para a justificativa completa e as condições de fronteira


## Containers
### Imagens glibc — Caminho Oficial Hoje
- Prefira imagens base Debian ou Ubuntu para os assets Linux oficiais atuais
- Alpine e imagens puramente musl não fazem parte da matriz suportada desde `v1.0.16`
- O caminho de container musl exige uma decisão de backend antes de voltar a ser suportado


## Suporte A Shells
### Bash Zsh Fish PowerShell Nushell — Todos Primeira Classe
```bash
# Bash e Zsh compartilham sintaxe idêntica para cada pipeline desta documentação
sqlite-graphrag recall "query" --json | jaq '.results[].name'
```
```fish
# Fish usa a mesma invocação do binário com sintaxe ligeiramente diferente para variáveis
sqlite-graphrag recall "query" --json | jaq '.results[].name'
```
```powershell
# PowerShell canaliza objetos nativamente mas jaq ainda aceita JSON puro em stdin
sqlite-graphrag recall "query" --json | jaq '.results[].name'
```
```nu
# Nushell consome JSON diretamente em tabelas estruturadas sem ferramentas externas
sqlite-graphrag recall "query" --json | from json | get results | select name
```
- Cada shell acima lê os mesmos códigos de saída garantindo semântica de orquestração idêntica
- Formato JSON de saída fica byte idêntico nos cinco shells simplificando pipelines automatizados
- Scripts de completion são suportados pela CLI atual via `sqlite-graphrag completion <shell>`
- Precedência de variáveis de ambiente permanece idêntica em todos os shells testados em CI
- Sinais SIGINT e SIGTERM funcionam identicamente habilitando shutdown gracioso universalmente


## Caminhos E XDG
### Caminhos — Crate Directories Resolve Cada Sistema Operacional
- Caminho padrão do banco resolve para `./graphrag.sqlite` no diretório da invocação
- Caminhos no macOS resolvem para `~/Library/Application Support/sqlite-graphrag/` conforme HIG
- Caminhos no Windows resolvem para `%APPDATA%\sqlite-graphrag\` e `%LOCALAPPDATA%\sqlite-graphrag\`
- Override via `SQLITE_GRAPHRAG_DB_PATH` tem prioridade absoluta em todo sistema operacional


### Variáveis De Ambiente — Overrides Em Runtime
```bash
export SQLITE_GRAPHRAG_DB_PATH="/var/lib/graphrag.sqlite"
export SQLITE_GRAPHRAG_CACHE_DIR="/tmp/sqlite-graphrag-cache"
export SQLITE_GRAPHRAG_LANG="pt"
export SQLITE_GRAPHRAG_LOG_LEVEL="debug"
```
- `SQLITE_GRAPHRAG_DB_PATH` sobrescreve o caminho padrão `./graphrag.sqlite`
- `SQLITE_GRAPHRAG_CACHE_DIR` isola cache do modelo e lock files para cenários de container e teste
- `SQLITE_GRAPHRAG_LANG` alterna saída da CLI entre inglês e português brasileiro imediatamente
- `SQLITE_GRAPHRAG_LOG_LEVEL` controla verbosidade do tracing expondo cada query SQL em `debug`


## Performance Por Target
### Benchmarks — Targets Suportados Selecionados
| Target | Cold Start | Warm Recall | RSS Após Modelo | Throughput Embedding |
| --- | --- | --- | --- | --- |
| x86_64-linux-gnu (i7-13700) | 48 ms | 4 ms | 820 MB | 1500 tok/s |
| aarch64-linux-gnu (Graviton3) | 58 ms | 5 ms | 810 MB | 1400 tok/s |
| aarch64-apple-darwin (M3 Pro) | 28 ms | 3 ms | 790 MB | 2000 tok/s |
| x86_64-windows-msvc (i7-12700) | 75 ms | 6 ms | 860 MB | 1300 tok/s |

- Cold start mede tempo desde o spawn do processo até a primeira query SQL completa com sucesso
- Warm recall mede segunda invocação com o cache de páginas do banco já quente em memória
- RSS após modelo reporta memória residente de pico após carregar `multilingual-e5-small` completo
- Throughput de embedding mede tokens por segundo durante operações sustentadas de `remember`
- Cada número acima fica dentro de dez por cento de variância entre dez rodadas de benchmark locais


## Agentes Validados Por Plataforma
### Vinte E Um Agentes — Verificados Em Cada Target
- Claude Code da Anthropic roda identicamente em Linux, macOS e Windows em shells nativos
- Codex da OpenAI usa o mesmo binário em containers Linux e laptops macOS de desenvolvedores
- Gemini CLI do Google invoca o binário pelo caminho padrão de execução em subprocesso
- Opencode como harness open source integra via stdin e stdout em todo sistema operacional suportado
- OpenClaw framework de agentes visa containers Linux primordialmente mas funciona em macOS também
- Paperclip assistente de pesquisa roda em ambientes desktop macOS e Linux simultaneamente
- VS Code Copilot da Microsoft executa via tasks no terminal integrado entre sistemas operacionais
- Google Antigravity plataforma roda o binário Linux glibc dentro de seu runtime sandbox
- Windsurf da Codeium visa predominantemente instalações de editor em macOS e Windows
- Cursor editor invoca o binário via seu terminal em macOS, Linux e Windows sem distinção
- Zed editor roda sqlite-graphrag como ferramenta externa em macOS e Linux nativamente
- Aider agente de código foca em terminais Linux e macOS para fluxos git-aware diários
- Jules do Google Labs roda o binário Linux glibc em pipelines de CI predominantemente
- Kilo Code agente autônomo foca em fluxos macOS para desenvolvedores com bindings nativos
- Roo Code orquestrador executa em servidores Linux e workstations macOS intercambialmente
- Cline agente autônomo integra via VS Code em todo sistema operacional que o editor suporta
- Continue assistente open source executa onde seu editor host rodar com suporte nativo
- Factory framework de agentes prefere containers Linux para cenários multi-agente reproduzíveis
- Augment Code assistente foca em ambientes de engenharia macOS e Linux predominantemente
- JetBrains AI Assistant roda sqlite-graphrag ao lado do IntelliJ IDEA nos três desktops suportados
- OpenRouter camada proxy executa o binário Linux em clusters Kubernetes e hosts Docker


### Codex CLI (v1.0.62)
- Codex CLI (`codex exec`) está disponível em macOS, Linux e Windows
- Descoberta do binário segue: flag `--codex-binary`, variável de ambiente `SQLITE_GRAPHRAG_CODEX_BINARY`, depois busca no PATH
- No Windows, busca `codex.exe` no PATH com resolução de extensões via `PATHEXT`
- Subprocesso usa `env_clear()` com whitelist de variáveis específica por plataforma incluindo vars do Windows via `#[cfg(windows)]`


## Autenticação Somente OAuth em Todas as Plataformas (v1.0.69)
### Mudança Comportamental Aplica-se Identicamente em Todo SO
- O spawn de `claude -p` e `codex exec` ABORTA com `AppError::Validation` (código de saída 1) quando `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estão definidas no ambiente, em alvos Linux glibc, aarch64 GNU, macOS e Windows
- OAuth é o ÚNICO mecanismo de credencial aceito em todo target publicado
- A flag `--bare` foi REMOVIDA de todo caminho executável em toda variante de build
- Migração: execute `claude login` (Claude Pro/Max) ou `codex login` (ChatGPT Pro) uma vez em cada host e remova a env var do shell rc
- Defesa em profundidade: `ANTHROPIC_API_KEY` e `OPENAI_API_KEY` estão INTENCIONALMENTE AUSENTES das whitelists `env_clear` em toda plataforma; mesmo se um refactor futuro mover o guard OAuth-only, a variável nunca alcança o filho
- Veja `docs/decisions/adr-0011-oauth-only-enforcement.md` para a justificativa completa e `src/commands/claude_runner.rs:574-666` e `src/commands/codex_spawn.rs:684-758` para os quatro testes de conformidade OAuth-only em cada binário
