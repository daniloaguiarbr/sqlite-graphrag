# Guia de Testes


- Leia a versão em inglês em [TESTING.md](TESTING.md)


## Por Que Categorizar os Testes
### O Incidente de Livelock Térmico — 2026-04-19
- Em 2026-04-19 às 11:37:40, o Intel i9-14900KF do desenvolvedor atingiu Tjmax 100°C
- A temperatura do VRM chegou a 99°C e o sistema exigiu reset forçado após 3 minutos e 11 segundos
- Causa raiz: `tests/loom_lock_slots.rs` executava sem gate `#[cfg(loom)]`
- O agendador do loom é intensivo por design — ele explora todas as permutações de threads
- Executar modelos loom sem isolamento causa runaway térmico em CPUs de alto núcleo
- Foi o terceiro incidente em sete dias causado pelo mesmo arquivo de testes sem proteção
- TODOS os testes loom DEVEM ter gate `#[cfg(loom)]` e ser serializados com `#[serial(loom_model)]`
- NUNCA execute testes loom dentro da invocação padrão `cargo nextest run`


## Categorias de Testes
### Testes Unitários — Inline com o Código-Fonte
- Localização: blocos `#[cfg(test)] mod tests` dentro de cada módulo em `src/`
- Executar com: `/usr/bin/timeout 300 cargo nextest run --profile default`
- Escopo: funções puras, variantes de erro, mascaramento, parsing, validação
- Isolamento: sem I/O, sem filesystem, sem chamadas HTTP
- Gate: sempre compilado, sempre executado no profile default
### Testes de Integração — Arquivos Separados
- Localização: diretório `tests/`
- Executar com: `/usr/bin/timeout 300 cargo nextest run --profile default`
- Escopo: subcomandos CLI, contratos de schema JSON, conformidade PRD, CRUD de storage
- Isolamento: `TempDir` por teste, `env_clear()`, wiremock para HTTP
- Gate: sempre compilado, sempre executado no profile default
### Testes de Concorrência Loom — Opt-in Explícito
- Localização: `tests/loom_lock_slots.rs`
- Executar com: `/usr/bin/timeout 3900 bash scripts/test-loom.sh` ou o job CI `loom`
- Escopo: teste de permutação do semáforo de lock slots
- Isolamento: NUNCA executar em paralelo com outros testes — um modelo por vez
- Gate: `#[cfg(loom)]` obrigatório em CADA função de teste e bloco de imports
- Risco térmico: testes loom sem proteção causaram travamento do sistema em 2026-04-19
### Testes End-to-End Lentos e Stress — Opt-in via Feature Flag
- Localização: arquivos em `tests/` protegidos por `#[cfg(feature = "slow-tests")]`
- Executar com: `/usr/bin/timeout 1800 cargo nextest run --profile heavy --features slow-tests`
- Escopo: suítes end-to-end longas, contratos, reuse de daemon, paridade i18n, roteamento de exit code, alta concorrência e loops de retry estendidos
- Gate: excluído dos profiles nextest `default` e `ci`
- Suítes críticas de release: `/usr/bin/timeout 1200 cargo test --features slow-tests --test doc_contract_integration -- --nocapture`
- Suítes críticas de release: `/usr/bin/timeout 1200 cargo test --features slow-tests --test prd_compliance -- --nocapture`
- O CI executa essas duas suítes em um job dedicado `slow-contracts` em `ubuntu-latest`
### Benchmarks — Criterion
- Localização: `benches/`
- Executar com: `/usr/bin/timeout 1800 cargo bench` ou `/usr/bin/timeout 1800 cargo criterion`
- Escopo: baselines de latência para remember, recall, hybrid-search, stats, graph
- Gate: nunca incluído em `cargo nextest run`


## Como Executar
### Default — Desenvolvimento Local
- Executar todos os testes unitários e de integração: `/usr/bin/timeout 300 cargo nextest run --profile default`
- Executar com saída em caso de falha: `/usr/bin/timeout 300 cargo nextest run --profile default --no-capture`
- Executar um teste específico pelo nome: `/usr/bin/timeout 300 cargo nextest run --profile default fragmento_do_nome`
- Executar um arquivo específico: `/usr/bin/timeout 300 cargo nextest run --profile default -E 'test(schema_contract)'`
### CI — Paralelismo Controlado
- Executar todos os testes como o CI faria: `/usr/bin/timeout 600 cargo nextest run --profile ci`
- O profile `ci` define `test-threads = 2` e `RUST_TEST_THREADS=2`
- O profile `ci` habilita retentativas em testes instáveis
- O workflow também executa `doc_contract_integration` e `prd_compliance` separadamente com `--features slow-tests`
### Heavy — Testes de Stress e Lentos
- Executar testes de stress e lentos: `/usr/bin/timeout 1800 cargo nextest run --profile heavy --features slow-tests`
- O profile `heavy` define `test-threads = 1` para isolamento máximo
- NUNCA execute o profile `heavy` em máquina com throttling térmico ativo
- Para validação de release, prefira os comandos explícitos de contrato acima antes de uma rodada heavy mais ampla


## Auditoria Segura do Remember
### Reproduza o Comportamento da Binária Instalada com Limites de cgroup
- Use `/usr/bin/timeout 3900 bash scripts/audit-remember-safely.sh <diretorio-do-corpus>` para auditar o `remember` com segurança contra um corpus real
- O script usa por padrão o `sqlite-graphrag` instalado no `PATH`
- Sobrescreva a binária com `BIN=./target/debug/sqlite-graphrag` para comparar mudanças locais com a build publicada
- O script usa `systemd-run --user --scope -p MemoryMax=4G -p MemorySwapMax=0`
- O script inicializa um banco temporário isolado e um cache de modelo isolado para o daemon
- O script tenta executar `daemon --stop` ao sair para evitar deixar um processo residente com o modelo carregado
- O script executa casos conhecidos de sucesso, limiar, falha e caso sintético


## Testes do Daemon
### Valide Explicitamente O Reuso Do Processo Persistente
- Execute `/usr/bin/timeout 900 cargo test --all-features --test daemon_integration -- --nocapture` para validar o daemon ponta a ponta
- A suíte do daemon prova `ping`, `shutdown`, auto-start, restart após stop e incrementos de contador em `init`, `remember`, `recall` e `hybrid-search`
- Use `SQLITE_GRAPHRAG_CACHE_DIR=/tmp/test-cache` para isolar o socket do daemon e o cache do modelo por execução
- Se um teste do daemon travar, execute `sqlite-graphrag daemon --stop` com o mesmo cache dir antes de tentar de novo
- A flag oculta de testes `--skip-memory-guard` agora desabilita auto-start do daemon por padrão, exceto quando `SQLITE_GRAPHRAG_DAEMON_FORCE_AUTOSTART=1` estiver definido


## Testes de Concorrência Loom
### Como o Loom Funciona
- O loom executa cada teste múltiplas vezes permutando os entrelaçamentos de threads
- Usa redução de estados para evitar explosão combinatória
- Cada modelo deve terminar dentro de um limite de preempção definido
- O uso de CPU é extremamente alto — um núcleo satura completamente por modelo
- NUNCA execute testes loom junto com outros testes no mesmo processo
### Executar Testes Loom Localmente
- Use o script canônico: `/usr/bin/timeout 3900 bash scripts/test-loom.sh`
- O script define `RUSTFLAGS="--cfg loom"` e `RUST_TEST_THREADS=1`
- O script define `LOOM_MAX_PREEMPTIONS=2` para iteração local mais rápida
- Execute somente no modo release: `--release` é obrigatório para velocidade aceitável
- Monitore a temperatura da CPU antes e durante a execução
### Executar Testes Loom Individualmente
- Compilar primeiro: `/usr/bin/timeout 600 env RUSTFLAGS="--cfg loom" cargo build --release --tests`
- Executar modelo único: `/usr/bin/timeout 3600 env RUSTFLAGS="--cfg loom" RUST_TEST_THREADS=1 cargo nextest run --release -E 'test(lock_slot)'`
- Limite menor para iteração local: `LOOM_MAX_PREEMPTIONS=2`
- Limite maior para rigor no CI: `LOOM_MAX_PREEMPTIONS=3`
### Checkpoint e Retomada
- Defina `LOOM_CHECKPOINT_FILE=/tmp/loom-checkpoint.json` para retomar execuções interrompidas
- O arquivo de checkpoint registra as permutações já exploradas
- Delete o arquivo de checkpoint para iniciar uma exploração nova


## Variáveis de Ambiente
### Variáveis do Loom — Definir Antes de Executar `scripts/test-loom.sh`
- `RUSTFLAGS="--cfg loom"` — habilita o gate de feature do loom, OBRIGATÓRIO para todos os testes loom
- `LOOM_MAX_PREEMPTIONS=2` — limita a profundidade de preempção por modelo (local: 2, CI: 2)
- `LOOM_MAX_BRANCHES=500` — limita o fator de ramificação por execução (padrão CI: 500)
- `LOOM_LOG=1` — habilita rastreamento detalhado de execução do loom no stderr
- `LOOM_CHECKPOINT_FILE=/tmp/loom.json` — caminho para arquivo de checkpoint para retomar execuções
- `RUST_TEST_THREADS=1` — OBRIGATÓRIO, proíbe execução paralela de modelos loom
### Variáveis do Cargo e Nextest
- `RUST_TEST_THREADS=N` — controla o paralelismo do nextest em nível de processo
- `CARGO_TERM_COLOR=always` — preserva cores nos logs do CI
- `NEXTEST_PROFILE=ci` — sobrescreve o profile ativo do nextest via ambiente
### Variáveis Específicas do sqlite-graphrag
- `SQLITE_GRAPHRAG_DB_PATH=/tmp/test/graphrag.sqlite` — isola o caminho do banco por teste
- `SQLITE_GRAPHRAG_CACHE_DIR=/tmp/test-cache` — isola cache do modelo e lock files por teste
- `SQLITE_GRAPHRAG_LOG_FORMAT=json` — muda a saída de log para JSON estruturado
- `SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo` — sobrescreve o timezone dos timestamps


## Profiles do CI
### Profile — default
- Ativa: sempre, a menos que seja sobrescrito
- `test-threads`: número de CPUs lógicas
- `RUST_TEST_THREADS`: não definido, herda o padrão do sistema
- Tentativas: 0
- Timeout por teste: 60 segundos
- Exclui: testes loom, feature slow-tests
### Profile — ci
- Ativa: `/usr/bin/timeout 600 cargo nextest run --profile ci`
- `test-threads`: 2
- `RUST_TEST_THREADS`: 2 (explícito, previne sobrecarga térmica em runners compartilhados)
- Tentativas: 2 para testes instáveis
- Timeout por teste: 120 segundos
- Exclui: testes loom, feature slow-tests
- Job dedicado `slow-contracts` cobre `doc_contract_integration` e `prd_compliance` com `/usr/bin/timeout 1200 cargo test --features slow-tests`
### Profile — heavy
- Ativa: `/usr/bin/timeout 1800 cargo nextest run --profile heavy --features slow-tests`
- `test-threads`: 1
- `RUST_TEST_THREADS`: 1
- Tentativas: 0
- Timeout por teste: 600 segundos
- Inclui: testes com gate da feature slow-tests
- Exclui: testes loom (sempre separados)
### Job CI Loom — Etapa Separada no Workflow
- Ativa: job chamado `loom` em `ci.yml`
- Ambiente: `RUSTFLAGS="--cfg loom"`, `RUST_TEST_THREADS=1`, `LOOM_MAX_PREEMPTIONS=2`, `LOOM_MAX_BRANCHES=500`
- Executa: `/usr/bin/timeout 3600 cargo nextest run --release -E 'test(loom)'`
- NUNCA deve ser mesclado com as execuções dos profiles default ou ci


## Solução de Problemas
### Throttling Térmico Durante os Testes
- Sintoma: a suíte de testes desacelera progressivamente, CPU reporta temperatura alta
- Causa: testes loom ou de stress rodando sem limites de thread adequados
- Correção: interrompa a execução imediatamente, deixe a CPU esfriar por 5 minutos
- Prevenção: NUNCA execute `cargo test` sem os profiles do nextest configurados
- Prevenção: SEMPRE use `scripts/test-loom.sh` para testes loom
### Travamento do Sistema Durante Testes Loom
- Sintoma: máquina fica sem resposta, exige reset forçado
- Causa: modelos loom executando em paralelo (RUST_TEST_THREADS > 1) em CPU de alto TDP
- Correção: reset forçado, depois defina `RUST_TEST_THREADS=1` antes de qualquer execução loom
- Caso histórico: 2026-04-19 11:37:40 — i9-14900KF travou por 3 minutos e 11 segundos
- Prevenção: atributo `#[serial(loom_model)]` DEVE estar presente em todo teste loom
### Teste Loom Não Termina
- Sintoma: modelo loom não termina após vários minutos
- Causa: `LOOM_MAX_PREEMPTIONS` não definido, exploração sem limite padrão
- Correção: defina `LOOM_MAX_PREEMPTIONS=2` para iteração local
- Trade-off: valores menores perdem entrelaçamentos raros, CI usa 2 por velocidade
### Testes Instáveis no CI
- Sintoma: teste passa localmente mas falha de forma intermitente no CI
- Causa: ausência de `#[serial]` em testes que compartilham estado global ou variáveis de ambiente
- Correção: adicione `#[serial]` da crate `serial_test` nos testes afetados
- Diagnóstico: execute `/usr/bin/timeout 600 cargo nextest run --profile ci --retries 0` para ver todas as falhas


## Referências
- Documentação da crate loom: `https://docs.rs/loom/latest/loom/`
- Repositório GitHub do loom: `https://github.com/tokio-rs/loom`
- Documentação do cargo-nextest: `https://nexte.st/`
- Referência de configuração do cargo-nextest: `https://nexte.st/docs/configuration/`
- Crate serial_test: `https://docs.rs/serial_test/latest/serial_test/`
