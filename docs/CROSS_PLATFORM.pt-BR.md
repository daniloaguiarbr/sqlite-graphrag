# SUPORTE CROSS PLATFORM

> Um binário, cinco targets, zero drama de configuração em todo sistema operacional moderno


- Leia este guia em inglês em [CROSS_PLATFORM.md](CROSS_PLATFORM.md)
- Volte ao [README.md](../README.md) principal para referência completa de comandos


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
- `x86_64-apple-darwin` foi excluído porque as releases atuais de `ort` não fornecem um caminho compatível de ONNX Runtime pré-compilado para macOS Intel nesta configuração do projeto
- `x86_64-unknown-linux-musl` foi excluído porque as releases atuais de `ort` não fornecem um caminho suportado de ONNX Runtime pré-compilado para musl nesta configuração do projeto
- Reintroduzir qualquer um desses targets agora EXIGE build custom do ONNX Runtime ou outra estratégia de backend

### ARM64 GNU — Contrato da Biblioteca Compartilhada do ONNX Runtime
- `aarch64-unknown-linux-gnu` usa carregamento dinâmico do ONNX Runtime em vez de bundling por linkedição
- Distribua `libonnxruntime.so` ao lado da binária, em `./lib/`, ou configure `ORT_DYLIB_PATH` explicitamente
- Isso evita falhas de link específicas do target ao usar arquivos pré-compilados do ONNX Runtime durante a cross-compilação


## Notas Para Linux
### glibc Primeiro — Caminho Oficial de Release no Linux
- Binário glibc roda em Ubuntu 20.04, Debian 11, Fedora 36 e distros mainstream
- `x86_64-unknown-linux-gnu` e `aarch64-unknown-linux-gnu` são os únicos assets Linux publicados agora
- `x86_64-unknown-linux-musl` não faz parte da matriz oficial de release desde `v1.0.16`
- Reintroduzir musl agora exige build custom do ONNX Runtime ou outra estratégia de backend
- Prefira glibc para workstations, runners de CI e imagens de container até esse gap fechar


## Notas Para macOS
### Gatekeeper — Assinatura e Notarização
- Binários não assinados baixados via navegador disparam quarentena na primeira execução
- Remova a quarentena com `xattr -d com.apple.quarantine /usr/local/bin/sqlite-graphrag`
- Binários instalados via `cargo install` ignoram Gatekeeper por virem do rustc local
- Distribuição via Homebrew continua planejada sobre a linha pública atual de release
- Os assets oficiais de macOS atualmente cobrem apenas Apple Silicon


### Apple Silicon — Performance Nativa em M1 M2 M3 M4
- Binário aarch64 nativo roda trinta por cento mais rápido que x86_64 via Rosetta
- macOS Intel está atualmente fora da matriz oficial de release nesta configuração do projeto
- O carregamento de modelo segue a mesma stack `fastembed` mais `ort` usada nos outros targets publicados
- Geração de embeddings atinge 2000 tokens por segundo no M3 Pro contra 800 via Rosetta
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
