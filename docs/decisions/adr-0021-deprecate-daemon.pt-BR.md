# ADR-0021: Depreciação do Comando `daemon` (v1.0.76)

- Status: Aceito (2026-06-07)
- Atualização (v1.0.79): o código restante do daemon foi deletado antecipando o cronograma da v1.1.0; a janela de transição está fechada
- Decisores: Danilo Aguiar
- Escopo: src/daemon.rs, src/commands/daemon.rs, src/main.rs, src/cli.rs

## Contexto

O subcomando `daemon` (`sqlite-graphrag daemon`) foi introduzido na v1.0.21 para manter o modelo fastembed carregado em memória entre invocações da CLI. A carga do modelo era de ~30 s em cache ONNX fria; spawnar uma CLI nova por `remember` pagava esse custo a cada vez.

Na v1.0.76, o modelo fastembed se foi. O embedding é produzido por spawn de um subprocesso LLM headless por chamada. O tempo de vida do subprocesso é de ~1-3 s, e não há modelo para "manter carregado" — cada chamada é um round-trip LLM novo.

## Decisão

O subcomando `daemon` é **deprecado** mas mantido para compatibilidade de fonte durante a janela de transição v1.0.76 → v1.1.0. A CLI não o usa mais internamente:

- `embed_passage_or_local` e `embed_query_or_local` ainda consultam o daemon (se um estiver rodando), mas o handler `EmbedPassage` do daemon agora spawna um subprocesso LLM novo por chamada. O overhead do round-trip do socket do daemon é comparável ao spawn do LLM, então o daemon não oferece mais um speedup significativo.
- O caminho de autostart ainda tenta spawnar um daemon, mas o embedding do daemon é a mesma chamada LLM que o cliente faria diretamente, então o daemon é agora um intermediário desnecessário.
- `daemon --stop`, `daemon --ping` e `daemon` (autostart padrão) ainda funcionam, mas não oferecem mais nenhum benefício de performance.

O subcomando `daemon` é **REMOVIDO na v1.1.0**.

## Consequências

### Positivas

- A CLI agora é um verdadeiro one-shot: sem processo para manter vivo, sem socket para limpar, sem estado para inspecionar.
- O incidente de 60+ segundos de 2026-06-03 (load average 276 causado por 4 `enrich` × 2 workers × 10 servidores MCP = 192 processos) é estruturalmente impossível. Não há árvore de processos para proliferar.
- Novos usuários não precisam aprender sobre o daemon, o lock singleton, o auto-restart por mismatch de versão, ou qualquer outra complexidade de processo de longa duração. A CLI é apenas uma `cli` agora.

### Negativas

- Operadores com embeddings customizados ou inferência apenas local (sem CLI LLM disponível) perdem o daemon como fallback. A feature `embedding-legacy` restaura o comportamento da v1.0.74 para a janela de transição.
- O subcomando `daemon` e seu protocolo IPC permanecem no código-fonte (~600 linhas) até a v1.1.0. São no-ops para novos caminhos de código.

## Verificação

- `daemon --stop` ainda funciona.
- `daemon --ping` retorna uma resposta saudável quando o daemon está ativo.
- `daemon` (autostart padrão) ainda spawna e o round-trip da requisição de embedding funciona corretamente quando a CLI LLM está no PATH.
- `tests/v1044_features::related_entity_seed_via_link_succeeds` e os outros testes de round-trip do daemon passam quando uma CLI LLM está disponível no ambiente de teste. Eles falham com "no LLM CLI found on PATH" quando nem `claude` nem `codex` estão instalados; isso é documentado no CHANGELOG da v1.0.76.
