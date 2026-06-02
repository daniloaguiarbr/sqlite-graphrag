# Gaps — sqlite-graphrag v1.0.67 (G/E/EP/LT/P/M/MP/TR/PE)
## Resumo: 24 CORRIGIDOS, 2 PARCIAIS (G20, G21), 1 ADIADO (G14)


## G01 HIGH (CORRIGIDO v1.0.67) — Hooks do Claude Code corrompem output do enrich --mode claude-code
### Status: CORRIGIDO — enrich.rs:580 passa max_turns=7 via claude_runner; hooks desabilitados via --settings '{"hooks":{}}'
### Problema
- `enrich --mode claude-code` spawna `claude -p` internamente para cada entidade/memória
- O `memory-guardian.sh` (Stop hook em `~/.claude/settings.json`) dispara dentro de CADA subprocesso `claude -p`
- O hook emite `decision: "block"` pedindo "SALVAMENTO PROATIVO" ao final do turno
- Em modo headless NÃO existe humano para responder ao bloqueio
- O bloqueio consome turns extras até atingir `--max-turns 3` → exit 1
- O `parse_claude_json_output()` recebe output malformado ou truncado → falha de parsing
### Consequências
- Taxa de falha de 64% em operações enrich (540 de 842 entidades falharam em teste real)
- Cada falha desperdiça tokens de LLM pagos sem produzir resultado
- O output JSON esperado pelo parser fica corrompido pelo conteúdo do hook
- Falhas em cascata: cada arquivo/entidade processado sofre a mesma exaustão
- `terminal_reason: max_turns` no JSON indica exaustão, não erro real de extração
- Diagnóstico falso: sem inspecionar `terminal_reason`, parece "unknown error"
- Entidades sem descrição permanecem sem enriquecimento
### Causa Raiz — 5 Porquês
- POR QUE falha? → `claude -p` retorna exit 1 com `terminal_reason: max_turns`
- POR QUE max_turns? → hook Stop (`memory-guardian.sh`) consome turns 2 e 3 de `--max-turns 3`
- POR QUE hook dispara? → hooks e permissões são sistemas INDEPENDENTES no Claude Code
- POR QUE não foi suprimido? → `--settings '{"hooks":{}}'` é passado (enrich.rs:545-547) MAS o Claude Code pode não aplicar override de forma confiável em todos os cenários
- POR QUE override falha? → possível deep merge do objeto hooks em vez de replace, ou SessionStart hooks carregam antes do override tomar efeito
### Evidência no Código
- `src/commands/enrich.rs:542-548` — lógica de detecção OAuth e passagem de `--settings '{"hooks":{}}'`
- `src/commands/enrich.rs:539` — `--max-turns 3` limita turns
- `src/commands/enrich.rs:579-592` — tratamento de erro captura exit code mas não distingue max_turns de erro real
- `src/commands/enrich.rs:617` — `parse_claude_json_output()` falha ao parsear output corrompido
- `src/commands/ingest_claude.rs:323-328` — implementação IDÊNTICA no ingest (mesma vulnerabilidade)
- `~/.claude/settings.json:88-98` — hook Stop com `memory-guardian.sh`
### Solução Proposta
- Opção A (RECOMENDADA) — Aumentar `--max-turns` de 3 para 7
  - Absorve turns consumidos por hooks sem alterar lógica
  - Risco: custo maior por invocação se hooks consumirem turns extras
  - Implementação: alterar `enrich.rs:539` de `.arg("3")` para `.arg("7")`
- Opção B — Detectar `terminal_reason: max_turns` e retry
  - Parsear `terminal_reason` do JSON de saída do `claude -p`
  - Se `max_turns`, retentar com `--max-turns` dobrado
  - Implementação: adicionar lógica de retry em `enrich.rs:568-607`
- Opção C — Usar `CLAUDE_CONFIG_DIR` com config limpa
  - Criar diretório temporário com `settings.json` contendo `{"hooks":{}}`
  - Passar `CLAUDE_CONFIG_DIR=/tmp/claude-clean-XX` no env do subprocesso
  - O `env_clear()` (enrich.rs:493) já preserva `CLAUDE_CONFIG_DIR` (enrich.rs:505)
  - Bypass completo dos hooks do usuário
  - Risco: pode perder configurações legítimas do usuário
- Opção D — Desabilitar hooks temporariamente no arquivo real
  - Renomear `~/.claude/settings.json` → `~/.claude/settings.json.bak` antes do enrich
  - Restaurar após conclusão
  - Workaround manual, não escalável
### Benefícios da Solução
- Taxa de sucesso sobe de 36% para próximo de 100%
- Zero turns desperdiçados com bloqueios de hooks em headless
- Custo de LLM proporcional ao trabalho real executado
- Enriquecimento completo do grafo em uma única invocação
- Diagnóstico claro quando falha por motivo real (não por hook)
### Complexidade
- Opção A: trivial (uma linha)
- Opção B: baixa (30-50 linhas, parsing de terminal_reason)
- Opção C: média (criar/limpar tmpdir, gerenciar lifecycle)
- Opção D: manual (não requer código)
### Arquivos Afetados
- `src/commands/enrich.rs` — spawn do `claude -p` e parsing de output
- `src/commands/ingest_claude.rs` — mesma vulnerabilidade (mesma lógica)


## G02 MEDIUM (CORRIGIDO v1.0.67) — enrich.rs e ingest_claude.rs duplicam lógica de spawn do claude -p
### Status: CORRIGIDO — src/commands/claude_runner.rs criado como módulo DRY compartilhado
### Problema
- `enrich.rs:472-607` (função `run_claude_extraction`) e `ingest_claude.rs:260-450` (função `run_single_extraction`) contêm lógica DUPLICADA
- Ambos implementam: validação de versão, env_clear, whitelist de env vars, detecção OAuth, passagem de flags, parsing de output
- O próprio código reconhece: `enrich.rs:613-615` tem comentário "DRY note: Mirrors parse_claude_output in ingest_claude.rs. Should be unified."
### Consequências
- Correções aplicadas em um arquivo podem ser esquecidas no outro
- G01 afeta AMBOS os arquivos pelo mesmo motivo (mesma lógica duplicada)
- Divergências futuras criam bugs sutis difíceis de diagnosticar
- Manutenção dobrada para cada mudança na interface com Claude Code
### Causa Raiz
- `enrich` foi adicionado na v1.0.65 copiando a lógica do `ingest_claude` (v1.0.62)
- Nenhum refactor para extrair módulo compartilhado foi executado após estabilização
### Solução Proposta
- Extrair módulo `src/commands/claude_runner.rs` com funções compartilhadas
- `find_claude_binary()`, `validate_claude_version()`, `build_claude_command()`, `run_claude_extraction()`, `parse_claude_json_output()`
- Ambos `enrich.rs` e `ingest_claude.rs` chamam o módulo compartilhado
### Arquivos Afetados
- `src/commands/enrich.rs`
- `src/commands/ingest_claude.rs`
- `src/commands/mod.rs` (novo módulo)


## G03 LOW (CORRIGIDO v1.0.67) — enrich.rs não detecta terminal_reason: max_turns
### Status: CORRIGIDO — claude_runner.rs:279 detecta terminal_reason: "max_turns" e retorna erro específico
### Problema
- `enrich.rs:579-592` trata exit code não-zero como erro genérico
- Não distingue `terminal_reason: max_turns` (hooks consumindo turns) de erro real de extração
- O stderr é inspecionado apenas para "auth" e "login" (linhas 581-586)
### Consequências
- Todas as falhas por exaustão de turns são reportadas como "claude extraction failed"
- Impossível distinguir automaticamente entre hooks interferindo vs erro real do LLM
- Retry cego sem aumentar max_turns repete a mesma falha
### Causa Raiz
- O campo `terminal_reason` está no JSON do stdout, não no stderr
- Quando exit code é não-zero, o código lê stderr e ignora stdout (linha 587-591)
- O stdout com `terminal_reason: max_turns` é descartado antes do parsing
### Solução Proposta
- Parsear stdout MESMO quando exit code é não-zero
- Detectar `terminal_reason: max_turns` no JSON
- Quando detectado, emitir evento NDJSON com `status: "failed"` e `reason: "max_turns_exhausted"`
- Opcionalmente retry com `--max-turns` maior
### Arquivos Afetados
- `src/commands/enrich.rs:568-607`
- `src/commands/ingest_claude.rs` (mesma lógica)


## G04 HIGH (CORRIGIDO v1.0.67) — hybrid-search retorna body em vez de snippet quebrando pipelines jaq
### Status: CORRIGIDO — campo snippet adicionado ao HybridSearchItem; body preservado para backward compat
### Problema
- `hybrid-search` results retornam campo `body` (texto completo) sem campo `snippet`
- TODOS os outros comandos de busca retornam campo `snippet` (truncado em 300 chars)
- `recall`, `deep-research`, `list` usam `snippet` consistentemente
- Até `hybrid-search.graph_matches[]` usa `snippet` (via RecallItem)
- Somente `hybrid-search.results[]` (via HybridSearchItem) diverge com `body`
- Pipelines `jaq` escritos para `recall`/`deep-research` falham silenciosamente no `hybrid-search`
- `.snippet[:200]` em campo inexistente (null) causa `jaq` exit code 5 (runtime error)
- Mensagem de erro `cannot use null as rangeable (array or string)` NÃO indica o campo correto
### Consequências
- Pipelines portáteis entre comandos de busca QUEBRAM ao trocar recall por hybrid-search
- O exit code 5 do `jaq` é erroneamente interpretado como "zero resultados" do hybrid-search
- Usuários gastam tempo debugando "por que a busca não retorna nada" quando o problema é o nome do campo
- O campo `body` retorna texto COMPLETO (pode ter megabytes) sem truncamento, saturando terminais e contextos de LLM
- Inconsistência no contrato JSON dificulta automação e scripts reutilizáveis
- A documentação (CLAUDE.md) lista `body` como campo do hybrid-search mas exemplos de pipeline usam `.snippet`
### Causa Raiz — 5 Porquês
- POR QUE `jaq` falha? → `.snippet` é `null` porque `HybridSearchItem` NÃO tem campo `snippet`
- POR QUE não tem snippet? → a struct `HybridSearchItem` (hybrid_search.rs:74) define `body: String` (linha 81) em vez de `snippet`
- POR QUE usa body? → decisão de design para retornar texto completo em vez de truncado
- POR QUE recall usa snippet? → `RecallItem` (output.rs:273) trunca com `body.chars().take(300)` para preview
- POR QUE a inconsistência? → `HybridSearchItem` foi projetado independentemente de `RecallItem`, sem alinhar campo de preview
### Evidência no Código
- `src/commands/hybrid_search.rs:74-106` — struct `HybridSearchItem` com `body: String` (linha 81), ZERO campo `snippet`
- `src/commands/hybrid_search.rs:281` — `body: row.body` atribui texto COMPLETO sem truncamento
- `src/output.rs:266-291` — struct `RecallItem` com `snippet: String` (linha 273)
- `src/commands/recall.rs:170` — `row.body.chars().take(300).collect()` gera snippet truncado
- `src/commands/hybrid_search.rs:327` — graph_matches USA snippet via RecallItem (inconsistência interna)
- `src/commands/deep_research.rs` — usa RecallItem com snippet, consistente com recall
### Reprodução
- Comando: `sqlite-graphrag hybrid-search "query" --k 10 --json | jaq -r '.results[]  | .snippet[:200]'`
- Resultado: `Error: cannot use null as rangeable (array or string)` — exit code 5 (jaq runtime error)
- Comando correto (workaround): `jaq -r '.results[] | .body[:200]'` — funciona, mas diverge do padrão recall/deep-research
### Solução Proposta
- Opção A (RECOMENDADA) — Adicionar campo `snippet` ao `HybridSearchItem`
  - Truncar `row.body.chars().take(300).collect()` para `snippet`
  - MANTER campo `body` para backward compatibility
  - Resultado: ambos `snippet` e `body` disponíveis no JSON
  - Implementação: adicionar campo `pub snippet: String` na struct (hybrid_search.rs:81) e popular em linha 275-292
- Opção B — Substituir `body` por `snippet`
  - Renomear e truncar como em RecallItem
  - Adicionar `--with-bodies` flag (como deep-research) para opt-in de texto completo
  - BREAKING CHANGE para consumidores existentes do campo `body`
- Opção C — Alias via serde
  - Adicionar `#[serde(alias = "snippet")]` ou campo separado com `#[serde(rename)]`
  - Menor impacto mas complexidade de manutenção
### Benefícios da Solução
- Pipelines `jaq` portáteis entre recall, hybrid-search, deep-research e list
- Zero quebras silenciosas ao trocar comando de busca
- Contrato JSON consistente em TODOS os comandos de pesquisa
- Preview de 300 chars evita saturação de terminal e contexto LLM
- Backward compatible (campo `body` preservado na Opção A)
### Complexidade
- Opção A: trivial (3 linhas — campo na struct + atribuição + truncamento)
- Opção B: baixa-média (renomear campo + adicionar flag --with-bodies)
- Opção C: trivial (1-2 linhas de anotação serde)
### Arquivos Afetados
- `src/commands/hybrid_search.rs:74-106` — struct `HybridSearchItem` (adicionar `snippet`)
- `src/commands/hybrid_search.rs:267-292` — build de resultados (popular `snippet`)


## G05 MEDIUM (CORRIGIDO v1.0.67) — Clap rejeita queries de busca iniciando com hífens como flags CLI
### Status: CORRIGIDO — allow_hyphen_values = true nos 3 comandos de busca
### Problema
- Os 3 comandos de busca (`hybrid-search`, `recall`, `deep-research`) definem `QUERY` como argumento posicional
- Nenhum deles configura `allow_hyphen_values = true` no atributo `#[arg]`
- Queries que INICIAM com `-` ou `--` são interpretadas pelo Clap como flags CLI
- Exemplo: `sqlite-graphrag hybrid-search "--bare --settings"` → exit code 2 (Clap parsing error)
- Exemplo: `sqlite-graphrag recall "-p"` → exit code 2
- Queries com hífens EMBUTIDOS (ex: `"claude -p headless"`) funcionam porque o primeiro caractere não é `-`
### Consequências
- Impossível buscar diretamente por flags CLI ou argumentos de linha de comando
- Usuários pesquisando documentação de CLIs (como `--bare`, `--settings`) encontram barreira
- Mensagem de erro `unexpected argument '--bare' found` não indica solução imediata
- Clap sugere `-- --bare` como workaround mas requer reordenar flags (`--k`, `--json`) antes do `--`
- Queries sobre segurança (`--no-verify`, `--force`), configuração (`--config`) ou debugging (`-v`, `-vvv`) falham
### Causa Raiz — 5 Porquês
- POR QUE Clap rejeita? → string começa com `-`, Clap interpreta como flag/opção
- POR QUE interpreta como flag? → `allow_hyphen_values` NÃO está configurado no argumento QUERY
- POR QUE não está configurado? → padrão do Clap é rejeitar valores com hífens em posicionais
- POR QUE é padrão? → evita ambiguidade entre flags e valores, mas QUERY é semântico e nunca é flag
- POR QUE afeta 3 comandos? → hybrid-search, recall e deep-research definem `pub query: String` sem o atributo
### Evidência no Código
- `src/commands/hybrid_search.rs:35-36` — `#[arg(help = "...")] pub query: String` sem `allow_hyphen_values`
- `src/commands/recall.rs` — mesma definição sem `allow_hyphen_values`
- `src/commands/deep_research.rs` — mesma definição sem `allow_hyphen_values`
- ZERO arquivos no projeto contêm `allow_hyphen_values` (verificado com `rg`)
### Reprodução
- `sqlite-graphrag hybrid-search "--bare" --k 5 --json` → exit 2: `unexpected argument '--bare' found`
- `sqlite-graphrag recall "-p" --k 5 --json` → exit 2: `unexpected argument '-p' found`
- `sqlite-graphrag hybrid-search "claude -p headless" --k 5 --json` → exit 0 (funciona, não começa com `-`)
### Solução Proposta
- Opção A (RECOMENDADA) — Adicionar `allow_hyphen_values = true` ao argumento QUERY
  - `#[arg(allow_hyphen_values = true, help = "...")]`
  - Aplicar em hybrid-search, recall E deep-research
  - Sem breaking change, sem efeito colateral
- Opção B — Documentar uso do separador `--`
  - Instruir: `sqlite-graphrag hybrid-search --k 10 --json -- "--bare --settings"`
  - Mitiga mas não resolve — exige reordenação de argumentos
### Benefícios da Solução
- Qualquer string é aceita como query de busca, incluindo flags CLI e argumentos
- Busca por documentação de CLIs funciona sem workarounds
- Consistente com expectativa de que QUERY é texto livre semântico
- Zero breaking changes
### Complexidade
- Opção A: trivial (1 atributo em 3 arquivos)
- Opção B: documentação apenas
### Arquivos Afetados
- `src/commands/hybrid_search.rs:35-36` — arg QUERY
- `src/commands/recall.rs` — arg QUERY
- `src/commands/deep_research.rs` — arg QUERY


## G06 HIGH (CORRIGIDO v1.0.67) — reclassify exige --new-type ao atualizar apenas --description
### Status: CORRIGIDO — validação relaxada aceita --new-type OU --description (reclassify.rs:120-124)
### Problema
- `reclassify --name <entidade> --description "texto" --json` falha com exit 1
- Mensagem: `"erro de validação: --new-type is required in single mode"`
- A validação trata `--new-type` como OBRIGATÓRIO em single mode, bloqueando `--description` sozinho
- A documentação (CLAUDE.md:3988, v1.0.58) PROMETE que `--description` sozinho funciona
- O help text (`--help`) também afirma: "Single mode requires --name and --new-type"
- A lógica de update de descrição (linhas 135-141) EXISTE e funciona, mas NUNCA é alcançada sem `--new-type`
### Consequências
- Impossível atualizar descrição de entidade sem alterar o tipo
- Usuários forçados a passar `--new-type` redundante com o tipo ATUAL para chegar ao update de descrição
- Workaround: `reclassify --name X --new-type concept --description "texto"` — exige conhecer o tipo atual primeiro
- Fluxo de enriquecimento de descrições de entidades (quality workflow documentado) fica 2x mais complexo
- Inconsistência entre documentação e implementação gera frustração e confusão
- Bug secundário: CLAUDE.md:3518 documenta flag inexistente `--entity-type` (correto: `--new-type`)
### Causa Raiz — 5 Porquês
- POR QUE falha? → `args.new_type.ok_or_else(...)` retorna `Err` quando `--new-type` é `None` (reclassify.rs:120-122)
- POR QUE é obrigatório? → a validação trata single mode como "reclassificar tipo", ignorando que `--description` é caso de uso independente
- POR QUE não foi corrigido? → o campo `--description` foi adicionado em v1.0.58 MAS a validação pré-existente (v1.0.56) não foi relaxada
- POR QUE a validação bloqueia? → a lógica condicional de description (linhas 135-141) está DENTRO do bloco que requer `new_type` (linhas 120-143)
- POR QUE o help text está errado? → o `after_long_help` (linha 29) foi escrito na v1.0.56 e nunca atualizado para refletir a adição de `--description` na v1.0.58
### Evidência no Código
- `src/commands/reclassify.rs:120-122` — `args.new_type.ok_or_else(|| AppError::Validation("--new-type is required in single mode"))` bloqueia ANTES de checar description
- `src/commands/reclassify.rs:135-141` — lógica de update de descrição EXISTE e funciona, mas é inalcançável sem `--new-type`
- `src/commands/reclassify.rs:29` — help text: `"Single mode requires --name and --new-type."` (incorreto desde v1.0.58)
- `src/commands/reclassify.rs:44` — `pub description: Option<String>` definido como OPCIONAL no Clap (correto)
- `src/commands/reclassify.rs:148-155` — `description_updated: Some(true)` no response JÁ está preparado para o caso
- `docs/schemas/reclassify.schema.json:12` — schema JÁ documenta `description_updated` como campo válido
- CLAUDE.md:3988 — documentação promete: `"USAR reclassify --name <entidade> --description "texto" --json"`
- CLAUDE.md:3518 — bug secundário: usa `--entity-type` (flag inexistente no CLI, correto é `--new-type`)
### Reprodução
- `sqlite-graphrag reclassify --name sqlite-graphrag --description "CLI tool for GraphRAG" --json` → exit 1: `--new-type is required`
- `sqlite-graphrag reclassify --name 12-factor-app --new-type concept --json` → exit 0 (funciona)
- `sqlite-graphrag reclassify --name 12-factor-app --new-type concept --description "texto" --json` → exit 0 (combinado funciona)
### Solução Proposta
- Opção A (RECOMENDADA) — Relaxar validação para aceitar `--new-type` OU `--description`
  - Substituir `args.new_type.ok_or_else(...)` (linha 120-122) por validação condicional
  - Exigir pelo menos UM de `--new-type` ou `--description` (não ambos obrigatórios)
  - Tornar o `UPDATE entities SET type` condicional (somente quando `--new-type` presente)
  - Manter `UPDATE entities SET description` condicional (somente quando `--description` presente)
  - Atualizar `after_long_help` linha 29 para: `"Single mode requires --name and at least one of --new-type or --description."`
  - Corrigir CLAUDE.md:3518: `--entity-type` → `--new-type`
- Opção B — Clap `required_unless_present` no `--new-type`
  - Adicionar `#[arg(long, required_unless_present = "description")]` no campo `new_type`
  - Validação em nível Clap em vez de lógica manual
  - Mais idiomático mas requer cuidado com interação batch mode
### Benefícios da Solução
- `reclassify --name X --description "texto"` funciona conforme documentado desde v1.0.58
- Workflow de enriquecimento de entidades simplificado (sem necessidade de conhecer tipo atual)
- Consistência entre documentação e implementação
- Zero breaking changes (quem já usa `--new-type` continua funcionando)
- Menor fricção para agentes LLM que seguem a documentação literalmente
### Complexidade
- Opção A: baixa (10-15 linhas de refactor na validação + 1 linha no help text + 1 linha no CLAUDE.md)
- Opção B: trivial (1 atributo Clap) mas requer teste de interação com batch mode
### Arquivos Afetados
- `src/commands/reclassify.rs:120-143` — lógica de validação e update no single mode
- `src/commands/reclassify.rs:29` — help text `after_long_help`
- `CLAUDE.md:3518` — corrigir `--entity-type` → `--new-type`


## G07 HIGH (CORRIGIDO v1.0.67) — graph export DOT/Mermaid sem styling visual produz blocos escuros em PDF
### Status: CORRIGIDO — diretivas Apple HIG e Mermaid theme neutral adicionados em graph_export.rs:773-806
### Problema
- `graph --format dot` gera DOT com ZERO atributos visuais: sem `bgcolor`, sem `fillcolor`, sem `fontname`, sem `style`
- `graph --format mermaid` gera Mermaid com ZERO diretivas de estilo: sem `classDef`, sem `style`, sem `%%{init: {}}%%`
- Quando convertidos a PDF via `dot -Tpdf` ou renderers Mermaid, a aparência depende 100% dos defaults do renderer
- Blocos `pre` e tabelas ASCII (labels longas com `\n`) renderizam com fundo escuro/preto na maioria dos renderers PDF
- Usuário rejeitou explicitamente cards com fundo preto nos blocos pre/ASCII do PDF
- Preferência declarada: light card seguindo Apple HIG (`secondarySystemBackground`)
- O output DOT atual consiste APENAS em `node_id [label="text"];` sem NENHUM atributo de nó ou grafo global
- O output Mermaid atual consiste APENAS em `id["text"]` sem NENHUM `classDef` ou tema
### Consequências
- PDFs gerados a partir do DOT/Mermaid são visualmente pobres e ilegíveis em impressão
- Blocos de código e tabelas ASCII em labels longas ficam com fundo preto contrastando com texto branco
- Usuários que exportam grafos para documentação ou apresentação precisam editar manualmente o DOT/Mermaid
- A falta de fonte monospace em labels com ASCII art desalinha colunas e bordas de tabela
- Nodes sem `style=filled` ficam transparentes ou com borda fina, dificultando a leitura
- O contraste escuro viola Apple HIG que define backgrounds claros hierárquicos para cards
- O DOT não define `rankdir`, `nodesep`, `ranksep` — o layout é apertado e confuso em grafos densos
- Impossível diferenciar visualmente tipos de entidade (person, concept, tool) sem cores por tipo
- Edge labels sem `fontsize` ficam ilegíveis em grafos com muitas relações
- O Mermaid não define tema (`%%{init: {'theme': 'neutral'}}%%`) — cada renderer aplica seu padrão
### Causa Raiz — 5 Porquês
- POR QUE blocos pre ficam escuros? → o renderer PDF aplica tema escuro quando NÃO há diretivas de estilo explícitas no DOT/Mermaid
- POR QUE não há diretivas? → `render_dot()` (graph_export.rs:768-784) emite APENAS `label` sem atributos visuais
- POR QUE não foram adicionadas? → o `render_dot` foi implementado para produzir DOT funcional mínimo, sem preocupação com renderização visual
- POR QUE mínimo? → a funcionalidade foi adicionada como export de dados para ferramentas externas, não como gerador de documentação visual
- POR QUE não evoluiu? → nenhum feedback de usuário havia sinalizado a necessidade de styling visual até agora
### Evidência no Código
- `src/commands/graph_export.rs:768-784` — `render_dot()` gera APENAS `node [label="..."];` e `from -> to [label="..."];` sem NENHUM atributo visual
- `src/commands/graph_export.rs:770` — `out.push_str("digraph sqlite-graphrag {\n");` sem `graph [bgcolor=...]`, sem `node [style=...]`, sem `edge [fontsize=...]`
- `src/commands/graph_export.rs:773` — `format!("  {node_id} [label=\"{escaped}\"];\n")` — ZERO atributos além de label
- `src/commands/graph_export.rs:780` — `format!("  {from} -> {to} [label=\"{label}\"];\n")` — ZERO atributos de aresta
- `src/commands/graph_export.rs:798-813` — `render_mermaid()` gera APENAS `id["label"]` e `from -->|label| to` sem `classDef` nem `style`
- `src/commands/graph_export.rs:800` — `out.push_str("graph LR\n");` sem `%%{init: {'theme': 'neutral'}}%%`
- `src/cli.rs` — enum `GraphExportFormat` com `Json`, `Dot`, `Mermaid`, `Ndjson` — sem variante `Pdf`
### Reprodução
- `sqlite-graphrag graph --format dot --output graph.dot && dot -Tpdf graph.dot -o graph.pdf` → PDF sem styling, blocos escuros
- `sqlite-graphrag graph --format dot 2>/dev/null | bat -P -r 1:5` → confirma ZERO atributos visuais no DOT
- `sqlite-graphrag graph --format mermaid 2>/dev/null | bat -P -r 1:5` → confirma ZERO diretivas de estilo no Mermaid
### Solução Proposta
- Opção A (RECOMENDADA) — Adicionar atributos visuais Apple HIG light card ao DOT e Mermaid
  - DOT: adicionar bloco global de estilo no `digraph`:
    - `graph [bgcolor="white", fontname="Helvetica Neue", fontsize=12, rankdir=LR, nodesep=0.8, ranksep=1.2];`
    - `node [shape=box, style="filled,rounded", fillcolor="#F2F2F7", fontname="Helvetica Neue", fontsize=11, color="#C7C7CC"];`
    - `edge [fontname="Helvetica Neue", fontsize=9, color="#8E8E93"];`
  - Mermaid: adicionar init theme e classDef:
    - `%%{init: {'theme': 'neutral', 'themeVariables': {'primaryColor': '#F2F2F7', 'primaryTextColor': '#1C1C1E', 'primaryBorderColor': '#C7C7CC', 'lineColor': '#8E8E93'}}}%%`
  - Cores baseadas em Apple HIG iOS Light Mode:
    - `#F2F2F7` — systemGray6 (card background, secondarySystemBackground)
    - `#C7C7CC` — systemGray4 (borders)
    - `#8E8E93` — systemGray (edge labels, secondary text)
    - `#1C1C1E` — label (primary text)
    - `#FFFFFF` — systemBackground (graph background)
  - Implementação: adicionar 3 linhas de diretivas globais em `render_dot()` antes do loop de nós
  - Implementação: adicionar 1 linha de `%%{init: ...}%%` em `render_mermaid()` antes do `graph LR`
- Opção B — Adicionar flag `--theme light|dark|none` ao graph export
  - `--theme light` aplica palette Apple HIG light (default)
  - `--theme dark` aplica palette Apple HIG dark
  - `--theme none` preserva comportamento atual sem styling (backward compat)
  - Mais flexível mas maior complexidade de implementação
- Opção C — Adicionar cores por tipo de entidade
  - Mapear cada `entity_type` para uma cor distinta do sistema Apple HIG:
    - `person` → `#007AFF` (systemBlue) fill light
    - `tool` → `#34C759` (systemGreen) fill light
    - `concept` → `#F2F2F7` (systemGray6) fill light
    - `decision` → `#FF9500` (systemOrange) fill light
    - `project` → `#5856D6` (systemIndigo) fill light
    - `incident` → `#FF3B30` (systemRed) fill light
  - Combinar com Opção A para styling global + diferenciação por tipo
### Benefícios da Solução
- PDFs gerados a partir do DOT/Mermaid são visualmente profissionais e legíveis
- Blocos pre/ASCII em labels renderizam com fundo claro `#F2F2F7` em vez de preto
- Fonte `Helvetica Neue` (Apple system font) garante alinhamento correto de ASCII art
- Cores Apple HIG criam hierarquia visual consistente e familiar
- Grafos densos ficam legíveis com `nodesep`/`ranksep` adequados
- Nodes com `style=filled,rounded` criam visual de card moderno
- Diferenciação por tipo de entidade (Opção C) permite leitura rápida da topologia do grafo
- Mermaid com tema `neutral` renderiza consistentemente em GitHub, GitLab, VS Code e renderers web
- Zero breaking change para consumidores que processam o DOT/Mermaid programaticamente
- Usuário não precisa editar manualmente o output para obter PDF apresentável
### Complexidade
- Opção A: baixa (3-5 linhas de diretivas globais em `render_dot` + 1 linha de init theme em `render_mermaid`)
- Opção B: média (novo enum `Theme`, 3 variantes, lógica condicional em ambas funções)
- Opção C: média (lookup de cor por `entity_type`, 13 mapeamentos, integração com NodeOut)
- Opção A + C combinadas: média (melhor resultado visual com esforço moderado)
### Arquivos Afetados
- `src/commands/graph_export.rs:768-784` — `render_dot()` (adicionar diretivas globais de estilo)
- `src/commands/graph_export.rs:798-813` — `render_mermaid()` (adicionar init theme e classDef)
- `src/commands/graph_export.rs:300-309` — construção de `NodeOut` (adicionar `entity_type` se Opção C)
- `src/cli.rs` — enum `GraphExportFormat` (adicionar variante `Pdf` se evolução futura)


## G08 HIGH (CORRIGIDO v1.0.67) — remember single-shot força N processos para N memórias causando contention e cancelamento em cascata
### Status: CORRIGIDO — subcomando remember-batch implementado com NDJSON stdin, --transaction e --force-merge
### Problema
- `remember` cria UMA memória por invocação CLI (struct `RememberArgs` com `--name: String` singular)
- Agentes LLM (Claude Code, Codex) precisam salvar N memórias e DEVEM spawnar N processos separados
- Cada invocação paga: spawn de processo, aquisição de slot no semáforo (4 slots), conexão com SQLite, handshake com daemon, verificação de schema
- Executar N invocações em paralelo compete pelo write lock exclusivo do SQLite WAL
- Se QUALQUER invocação paralela falha (hook, validação, SQLITE_BUSY), Claude Code CANCELA todas as invocações irmãs
- O `ingest` resolve batch para ARQUIVOS de diretório, mas NÃO existe batch para memórias programáticas
- O único caminho para N memórias é N × `remember` — sequencial (lento) ou paralelo (frágil)
### Consequências
- Cancelamento em cascata: uma invocação errando por hook ou validação cancela TODAS as irmãs paralelas no Claude Code
- Overhead multiplicado: N processos × (spawn ~50ms + slot ~500ms poll + connection + daemon handshake + schema check)
- Slot exhaustion: com N > 4, invocações excedentes aguardam `CLI_LOCK_DEFAULT_WAIT_SECS=300s` ou falham com exit 75
- SQLITE_BUSY contention: WAL permite apenas UM escritor; N escritores simultâneos competem pelo lock com `busy_timeout=5s` + 5 retries ≈ 9.3s
- Sem atomicidade: se 3 de 5 invocações paralelas sucedem e 2 falham, o grafo fica em estado parcial inconsistente
- Desperdício de embedding: cada invocação conecta ao daemon ou carrega modelo ONNX separadamente
- Latência sequencial: N memórias × ~1.5s por invocação (com daemon ativo) = 15s para 10 memórias
- Impossível transacionar: não há como agrupar N memórias em uma transação atômica all-or-nothing
- Agentes conservadores serializam invocações para evitar contention, sacrificando throughput
- Logs e tracing ficam intercalados de N processos simultâneos, dificultando diagnóstico
### Causa Raiz — 5 Porquês
- POR QUE o agente precisa spawnar N processos? → `remember` aceita APENAS UM `--name` por invocação (remember.rs:63-64)
- POR QUE apenas um por vez? → `RememberArgs` define `pub name: String` como campo singular, sem modo batch ou NDJSON stdin
- POR QUE não existe batch? → o `ingest` foi projetado para bulk import de ARQUIVOS de diretório; memórias programáticas (geradas por agentes LLM em runtime) não têm representação em disco
- POR QUE memórias programáticas não usam ingest? → `ingest` requer diretório com arquivos físicos; agentes LLM geram memórias dinamicamente sem escrever arquivos intermediários
- POR QUE não foi adicionado batch ao remember? → o padrão original era invocação humana interativa (1 memória por vez); workflows multi-agente paralelos com Claude Code/Codex surgiram após o design do comando
### Evidência no Código
- `src/commands/remember.rs:60-64` — struct `RememberArgs` com `pub name: String` (singular, sem alternativa batch)
- `src/commands/remember.rs:76-82` — flags `--body`, `--body-file`, `--body-stdin`, `--graph-stdin` mutuamente exclusivas (design single-shot)
- `src/cli.rs:250-252` — `uses_cli_slot()` retorna `true` para TODOS os comandos exceto `Daemon`; cada `remember` consome 1 dos 4 slots
- `src/constants.rs:331` — `MAX_CONCURRENT_CLI_INSTANCES = 4`; 5+ invocações paralelas causam slot exhaustion
- `src/constants.rs:129` — `BUSY_TIMEOUT_MILLIS = 5_000`; SQLite espera 5s antes de retornar SQLITE_BUSY
- `src/constants.rs:49-55` — `MAX_SQLITE_BUSY_RETRIES = 5` com backoff exponencial: 300→600→1200→2400→4800ms ≈ 9.3s total
- `src/storage/utils.rs:37-61` — `with_busy_retry()` implementa retry com half-jitter mas escopo é POR PROCESSO, não cross-process
- `src/lock.rs:75-108` — `acquire_cli_slot()` implementa semáforo via file-lock; 4 slots padrão com polling de 500ms
- `src/commands/ingest.rs` — modelo de NDJSON output com evento por arquivo + summary final JÁ EXISTE e serve como padrão para batch
- `src/storage/memories.rs:270-273` — `with_busy_retry()` em memória write confirma que contention paralela É um cenário conhecido
### Reprodução
- `sqlite-graphrag remember --name mem-a --type note --description "A" --body "a" --json & sqlite-graphrag remember --name mem-b --type note --description "B" --body "b" --json & wait` → possível exit 15 (SQLITE_BUSY) em uma das invocações
- Executar 5+ invocações paralelas → exit 75 (AllSlotsFull) na 5a invocação se `--wait-lock` for curto
- No Claude Code: spawnar 2+ `remember` como parallel tool calls → se uma erra por hook/validação, TODAS são canceladas
### Solução Proposta
- Opção A (RECOMENDADA) — Adicionar subcomando `remember-batch` com NDJSON stdin
  - Aceita NDJSON via stdin: cada linha é um objeto JSON com `name`, `type`, `description`, `body`, `entities`, `relationships`
  - Output: NDJSON, uma linha por memória processada + linha summary final (padrão idêntico ao `ingest`)
  - UMA invocação CLI, UM slot, UMA conexão DB, UMA sessão daemon
  - Flag `--transaction` para atomicidade all-or-nothing (SAVEPOINT + ROLLBACK em falha)
  - Flag `--fail-fast` para parar na primeira falha
  - Flag `--force-merge` aplicado a TODAS as memórias do batch
  - Eventos NDJSON por memória: `name`, `status` ("indexed"/"failed"/"skipped"), `memory_id`, `elapsed_ms`
  - Linha summary: `total`, `succeeded`, `failed`, `skipped`, `elapsed_ms`
  - Schema NDJSON de entrada: `{"name":"x","type":"note","description":"y","body":"z","entities":[{"name":"e","entity_type":"concept"}],"relationships":[{"source":"a","target":"b","relation":"depends-on","strength":0.7}]}`
- Opção B — Adicionar flag `--batch` ao `remember` existente
  - `remember --batch` lê NDJSON do stdin em vez de `--name`/`--body`
  - Reutiliza `RememberArgs` com override de comportamento
  - Menor surface de API mas conflito semântico com flags existentes
- Opção C — Arquivo JSON array como input
  - `remember --memories-file memories.json` aceita array JSON de memórias
  - Mais simples de implementar mas NDJSON é preferível para streaming
### Benefícios da Solução
- ZERO cancelamento em cascata: uma invocação processa todas as memórias
- Overhead de slot e conexão pago UMA VEZ em vez de N vezes
- ZERO contention SQLITE_BUSY: escrita sequencial in-process dentro do mesmo slot
- Atomicidade via --transaction: N memórias sucedem ou falham juntas
- Throughput ~10x: N memórias em ~1.5s (1 invocação) vs N × 1.5s (N invocações)
- Logs e tracing coesos em um único processo
- Padrão NDJSON consistente com `ingest` (mesmo contrato de output)
- Agentes LLM podem gerar NDJSON programaticamente sem arquivos intermediários
- Compatível com pipes: gerar NDJSON e pipar direto ao remember-batch
- Daemon session reusada: embeddings de N memórias em 1 conexão vs N handshakes
### Complexidade
- Opção A: média (novo subcomando ~150-200 linhas, reutiliza `memories::upsert_memory` e `entities::persist_graph`)
- Opção B: baixa-média (flag condicional em `remember.rs`, ~100 linhas de lógica batch)
- Opção C: baixa (parsing de JSON array, ~80 linhas)
### Arquivos Afetados
- `src/commands/remember_batch.rs` (NOVO) — handler do subcomando remember-batch
- `src/commands/mod.rs` — registrar novo módulo
- `src/cli.rs:256+` — adicionar variante `RememberBatch` ao enum `Commands`
- `src/cli.rs:239-248` — marcar `RememberBatch` como `is_embedding_heavy`
- `src/main.rs:268+` — dispatch do novo subcomando


## G09 CRITICAL (CORRIGIDO v1.0.66) — reclassify-relation --batch crash por coluna inexistente updated_at
### Problema
- O comando `reclassify-relation --batch` falha com exit code 10 em toda invocação na v1.0.65
- Mensagem: `database error: no such column: updated_at`
- O erro ocorre em 3 queries SQL dentro de `run_single()` e `run_batch()`
- O modo `--dry-run` mascara o bug porque executa apenas `SELECT COUNT(*)` sem o `UPDATE`
- O modo single (sem `--batch`) também falha na mesma causa
- O comando é 100% inutilizável em qualquer invocação que tente persistir mudanças
- Corrigido na v1.0.66 (commit 453ec50), mas a causa raiz arquitetural permanece
### Consequências
- Impossível renomear tipos de relação em massa de forma nativa na v1.0.65
- Força workaround manual: `unlink --from A --to B --relation old` seguido de `link --from A --to B --relation new`
- O workaround perde atomicidade: se falhar no meio, arestas ficam parcialmente removidas
- O workaround é O(N) invocações para N arestas, com overhead de slot e SQLite WAL por invocação
- `--dry-run` retorna contagem correta mas a execução real falha, confundindo o diagnóstico
- Agentes LLM que usam `reclassify-relation` em pipelines automatizados falham silenciosamente
- A documentação CLAUDE.md prometia o comando funcionando desde v1.0.65
- O teste unitário em `reclassify_relation.rs` cobria apenas serialização de resposta, NÃO execução SQL
- Cadeias de qualidade de grafo (normalize → reclassify-relation → cleanup-orphans) quebram no meio
- Impossível padronizar vocabulário de relações em grafos com centenas de arestas
### Causa Raiz — 5 Porquês
- POR QUE falha? → o SQLite retorna `SQLITE_ERROR` porque a coluna `updated_at` não existe na tabela `relationships`
- POR QUE a query referencia `updated_at`? → o handler de `reclassify_relation.rs` v1.0.65 inclui `SET relation = ?1, updated_at = unixepoch()` em 3 queries UPDATE
- POR QUE a query foi escrita assim? → copy-paste do handler de `entities` ou `memories`, que TEM `updated_at`
- POR QUE o copy-paste não foi detectado? → os testes unitários testavam apenas serialização JSON, NUNCA executavam SQL contra um banco real
- POR QUE a tabela `relationships` não tem `updated_at`? → inconsistência arquitetural: `memories` e `entities` têm timestamps `created_at`/`updated_at`, mas `relationships` não tem nenhum
### Evidência no Código
- v1.0.65 `src/commands/reclassify_relation.rs:194` — `SET relation = ?1, updated_at = unixepoch()` em `run_single()`
- v1.0.65 `src/commands/reclassify_relation.rs:304` — `SET relation = ?1, updated_at = unixepoch()` em `run_batch()` modo filtrado
- v1.0.65 `src/commands/reclassify_relation.rs:314` — `SET relation = ?1, updated_at = unixepoch()` em `run_batch()` modo sem filtro
- v1.0.66 diff: removeu `, updated_at = unixepoch()` das 3 queries (commit 453ec50)
- Schema real `relationships`: colunas `id, namespace, source_id, target_id, relation, weight, description, metadata` — SEM `created_at`, SEM `updated_at`
- Schema `entities`: TEM `created_at INTEGER NOT NULL DEFAULT (unixepoch())` E `updated_at INTEGER NOT NULL DEFAULT (unixepoch())`
- Schema `memories`: TEM `created_at` E `updated_at` com trigger `trg_memories_updated_at`
- Testes em `reclassify_relation.rs:383-474`: 7 testes, TODOS sobre serialização de `ReclassifyRelationResponse`, NENHUM executa SQL contra DB
### Reprodução (v1.0.65)
- `sqlite-graphrag reclassify-relation --from-relation mentions --to-relation related --batch --json`
- Exit code: 10 (database error)
- `sqlite-graphrag reclassify-relation --from-relation mentions --to-relation related --batch --dry-run --json`
- Exit code: 0 (sucesso enganoso, mascarando o bug)
### Solução Aplicada (v1.0.66)
- Remoção da referência a `updated_at` das 3 queries UPDATE em `reclassify_relation.rs`
- O fix é correto e suficiente para desbloquear o comando
### Solução Remanescente — Lacuna Arquitetural
- Opção A: adicionar `created_at INTEGER NOT NULL DEFAULT (unixepoch())` à tabela `relationships` via migração
- Opção B: adicionar `created_at` E `updated_at` à tabela `relationships` para paridade com `entities` e `memories`
- Opção C: manter sem timestamps se o custo de migração em bancos grandes for proibitivo
- Independente da opção: adicionar testes de integração que executam SQL contra DB real para reclassify-relation
### Benefícios da Solução Remanescente
- Paridade de schema entre as 3 tabelas principais: `memories`, `entities`, `relationships`
- Auditabilidade de quando cada aresta foi criada e modificada
- Permite queries como "arestas criadas/modificadas nos últimos N dias"
- Elimina a armadilha de copy-paste futuro: se timestamps existem, referenciá-los é válido
- Testes de integração SQL previnem regressão de toda query que referencia colunas
- O padrão `unixepoch()` como DEFAULT é zero-cost para writes existentes
- Consistência com expectativa dos agentes LLM que consultam documentação mencionando timestamps
### Complexidade
- Fix aplicado (v1.0.66): trivial (remoção de 3 fragmentos SQL)
- Migração de timestamps: baixa (ALTER TABLE + DEFAULT, ~20 linhas de migração)
- Testes de integração: média (~50-80 linhas, requer setup de DB in-memory com schema completo)
### Arquivos Afetados
- `src/commands/reclassify_relation.rs:192-196` — query single mode (CORRIGIDO v1.0.66)
- `src/commands/reclassify_relation.rs:302-305` — query batch filtrado (CORRIGIDO v1.0.66)
- `src/commands/reclassify_relation.rs:312-315` — query batch sem filtro (CORRIGIDO v1.0.66)
- `src/storage/schema.rs` ou migração v12+ — adicionar timestamps à tabela `relationships`
- `tests/reclassify_relation_integration.rs` (NOVO) — testes SQL contra DB real
---


## G10 — BUG HIGH (CORRIGIDO v1.0.67): normalize-entities --dry-run Subnotifica Merges (merged_count Sempre Zero)
### Status: CORRIGIDO — normalize_entities.rs:136 calcula merge_count_preview real com detecção de colisão
### Severidade: HIGH — dry-run não é confiável para prever impacto real da normalização
### Problema
- `normalize-entities --dry-run` reporta `merged_count: 0` independente do estado do banco
- A execução real (`--yes`) mescla entidades que colidem após normalização
- Exemplo real: dry-run reportou 0 merges, execução mesclou 33 entidades
- O operador não consegue prever quantas entidades serão fundidas antes de gravar
- A premissa fundamental do dry-run (simular com segurança antes de aplicar) está quebrada
- O campo `merged_count` na resposta do dry-run é hardcoded como `0` na linha 106
- O campo `normalized_count` no dry-run conta TODAS as entidades que mudariam de nome, sem distinguir renomeações de merges
### Consequências
- O operador toma decisão de aplicar baseado em dados incompletos
- Em grafos com muitas variantes de caixa, dezenas de merges silenciosos ocorrem sem aviso prévio
- Merges reestruturam relacionamentos (UPDATE OR IGNORE + DELETE) e removem entidades fonte
- Efeitos colaterais de merge (retarget de relationships, remoção de memory_entities, eliminação de self-loops) são invisíveis no preview
- O dry-run infla `normalized_count` ao incluir entidades que na verdade seriam mergeadas (não renomeadas)
- Pipelines automatizados que confiam no dry-run para decidir se aplicam podem executar merges destrutivos inesperados
- A documentação do CLAUDE.md afirma que dry-run "faz preview de quais entidades seriam renomeadas ou mescladas" — mas merged_count é sempre 0
- Não há como estimar o impacto real no grafo sem aplicar a operação destrutivamente
- Agentes LLM que consultam o dry-run antes de aplicar tomam decisão com dados falsos
- Regressão de confiança no padrão dry-run/apply usado em toda a CLI
### Causa Raiz — 5 Porquês
- POR QUE o dry-run reporta merged_count 0? Porque o valor é hardcoded na linha 106: `merged_count: 0`
- POR QUE é hardcoded? Porque o ramo dry-run (linhas 102-120) retorna antes de entrar no loop de aplicação (linhas 128-203) onde a detecção de colisão ocorre
- POR QUE a detecção de colisão não roda no dry-run? Porque a lógica de colisão (`SELECT id FROM entities WHERE name = ?2`, linha 132) está encapsulada dentro do loop que só executa na transação real
- POR QUE não foi extraída para ser reutilizada? Porque o design original tratou dry-run como contagem simples de nomes que mudariam, sem considerar que normalizar pode gerar colisões entre nomes distintos que convergem para o mesmo kebab-case
- POR QUE o teste não detectou? Porque o teste `dry_run_returns_count_without_changes` (linha 266) verifica apenas que o dry-run não modifica o banco, mas não verifica se o merged_count prediz corretamente as colisões que ocorreriam
### Evidência no Código
- `src/commands/normalize_entities.rs:88-98` — `to_change` computa TODAS as entidades que precisam normalizar, sem distinguir rename de merge
- `src/commands/normalize_entities.rs:100` — `normalized_count_preview = to_change.len()` conta tudo como renomeação
- `src/commands/normalize_entities.rs:102-119` — ramo dry-run retorna ANTES do loop de aplicação, com `merged_count: 0` hardcoded
- `src/commands/normalize_entities.rs:106` — `merged_count: 0` hardcoded na resposta do dry-run
- `src/commands/normalize_entities.rs:128-203` — loop de aplicação com detecção de colisão EXCLUSIVA da transação real
- `src/commands/normalize_entities.rs:130-138` — query de colisão `SELECT id FROM entities WHERE name = ?2` ausente do dry-run
- `src/commands/normalize_entities.rs:140-188` — lógica de merge (retarget relationships, delete source) inacessível ao dry-run
- `src/commands/normalize_entities.rs:266-287` — teste `dry_run_returns_count_without_changes` não testa merged_count com colisões
### Reprodução
- `sqlite-graphrag normalize-entities --dry-run --json` — retorna `merged_count: 0` mesmo com colisões pendentes
- Inserir manualmente duas entidades: "Hello World" e "hello-world" no mesmo namespace
- Executar dry-run: reportará `normalized_count: 1, merged_count: 0`
- Executar com --yes: reportará `normalized_count: 0, merged_count: 1`
### Solução Proposta
- Extrair a lógica de detecção de colisão do loop de aplicação (linhas 128-138) para uma função reutilizável
- Executar detecção de colisão no ramo dry-run iterando `to_change` e verificando se o nome normalizado já existe no banco OU se dois nomes distintos em `to_change` convergem para o mesmo normalizado
- Classificar cada entrada de `to_change` como `rename` ou `merge`
- Retornar `normalized_count` (apenas renomeações sem colisão) e `merged_count` (colisões com nomes existentes ou entre normalizados) separadamente
- O dry-run NÃO precisa simular a transação inteira — apenas contar colisões via SELECT read-only
### Benefícios da Solução
- Dry-run fiel ao que a execução fará: paridade entre preview e aplicação
- O operador enxerga quantas entidades serão fundidas ANTES de gravar
- Decisão informada: merged_count permite avaliar impacto destrutivo no grafo
- Pipelines automatizados podem confiar no dry-run para decidir se aplicam
- Consistência com o padrão dry-run/apply de toda a CLI (ingest, replace, transform, scope)
- Agentes LLM recebem dados precisos para tomada de decisão
- A documentação do CLAUDE.md passa a ser verdadeira sobre o comportamento do dry-run
### Como Solucionar
- Passo 1: criar função `fn classify_changes(conn, namespace, to_change) -> (Vec<Rename>, Vec<Merge>)` que itera `to_change`, consulta o banco para cada nome normalizado, e classifica como rename (sem colisão) ou merge (colisão com existente)
- Passo 2: adicionar detecção de colisão intra-batch — quando dois nomes distintos em `to_change` convergem para o mesmo normalizado, o segundo é merge mesmo que o nome não exista ainda no banco
- Passo 3: no ramo dry-run (linhas 102-119), chamar `classify_changes` e preencher `normalized_count` e `merged_count` com os valores reais
- Passo 4: no ramo de aplicação (linhas 122-206), reutilizar `classify_changes` para manter paridade
- Passo 5: adicionar teste `dry_run_predicts_merge_count_on_collision` que insere "Hello World" + "hello-world", executa dry-run e asserta `merged_count: 1`
- Passo 6: adicionar teste `dry_run_detects_intra_batch_collision` que insere "Hello World" + "HELLO_WORLD" (sem existente normalizado), executa dry-run e asserta `merged_count: 1` (um será merge do outro)
### Complexidade
- Detecção de colisão com existentes: baixa (~15 linhas, SELECT read-only no dry-run)
- Detecção de colisão intra-batch: média (~20 linhas, HashMap para rastrear nomes normalizados já vistos)
- Testes: baixa (~40 linhas, requer setup_db existente)
### Arquivos Afetados
- `src/commands/normalize_entities.rs:88-119` — extrair classificação de changes e preencher merged_count no dry-run
- `src/commands/normalize_entities.rs:128-203` — refatorar loop de aplicação para reutilizar classificação
- `src/commands/normalize_entities.rs:266+` — adicionar 2 testes de integração cobrindo colisão no dry-run
---


## G12 MEDIUM (CORRIGIDO v1.0.67) — NewRelationship rejeita campo type como alias de relation (assimetria com NewEntity)
### Status: CORRIGIDO — entities.rs:39 serde(alias = "type") adicionado ao campo relation
### Problema
- remember --relationships-file e --graph-stdin rejeitam campo type no JSON de relacionamentos
- O erro eh exit 20: unknown field type, expected one of from, source, target, to, relation, strength, description
- NewEntity aceita type como alias de entity_type via serde alias
- NewRelationship NAO aceita type como alias de relation — assimetria de API
- Agentes LLM e humanos usam type por analogia com entidades e falham
- O erro interrompe TODA a operacao remember — grafo inteiro rejeitado por um campo
- serde deny_unknown_fields transforma campo desconhecido em ERRO FATAL, nao warning
### Consequencias
- Agentes Claude Code que geram JSON com type em relacionamentos falham com exit 20
- O erro nao sugere a correcao: diz expected one of from, source... sem mencionar que relation eh o campo correto
- Usuarios que copiam padrao de entidades (type aceito) e aplicam em relacoes sao surpreendidos
- Toda operacao remember com grafo falha atomicamente — entidades validas NAO sao persistidas
- --entities-file funciona com type, --relationships-file falha com type — inconsistencia na mesma invocacao
- Confusao entre type (campo do JSON) e relation (campo canonico) eh recorrente em agentes
- Diagnostico dificil: exit 20 eh internal/serialization error — obscurece a causa real
- Pipelines automatizados que geram JSON de grafo precisam conhecer esta assimetria
- Zero testes de deserializacao em src/storage/entities.rs cobrem o caminho type em relacoes
- Documentacao no CLAUDE.md diz usar relation mas nao adverte contra type
### Causa Raiz — 5 Porques
- POR QUE falha? serde rejeita campo type em NewRelationship como unknown field
- POR QUE rejeita? struct usa serde deny_unknown_fields sem alias para type
- POR QUE nao tem alias? NewEntity recebeu serde alias type mas NewRelationship nao
- POR QUE a assimetria? ao adicionar alias em entidades, relacionamentos foram esquecidos
- POR QUE nao foi detectado? ZERO testes de deserializacao cobrem NewRelationship com campo type
### Evidencia no Codigo
- src/storage/entities.rs:20-26 — NewEntity com deny_unknown_fields E alias type no campo entity_type
- src/storage/entities.rs:32-42 — NewRelationship com deny_unknown_fields MAS SEM alias type no campo relation
- src/storage/entities.rs:39 — campo relation: String sem serde alias type
- src/commands/remember.rs:193-202 — GraphInput reutiliza NewRelationship sem adapter
- src/commands/ingest_claude.rs:48-63 — EXTRACTION_SCHEMA usa relation (NAO type) — LLM extraction funciona
- Zero testes de deserializacao em src/storage/entities.rs — nenhum cfg(test) no modulo
### Reproducao
- echo '[{"source":"a","target":"b","type":"depends-on","strength":0.9}]' > /tmp/rels.json
- sqlite-graphrag remember --name test --type note --description test --body x --relationships-file /tmp/rels.json --json
- Resultado: exit 20, unknown field type
- Esperado: aceitar type como alias de relation (analogia com entidades)
### Solucao Proposta
- Adicionar serde alias type ao campo relation em NewRelationship
- Manter relation como nome canonico — type apenas como alias de entrada
### Beneficios
- Paridade de API: entidades e relacoes aceitam type como alias — contrato consistente
- Agentes LLM param de falhar ao usar type em relacoes por analogia com entidades
- Zero breaking change: relation continua como campo canonico
- Menos suporte: elimina erro recorrente de agentes que geram JSON com type
- Pipelines automatizados nao precisam conhecer a assimetria entre entidades e relacoes
- Principio de menor surpresa: se entidades aceitam type, relacoes tambem devem
- Consistencia com filosofia de aliases ja aplicada em from/source e to/target
### Como Solucionar
- Passo 1: em src/storage/entities.rs:39, adicionar serde alias type acima de pub relation: String
- Passo 2: adicionar testes de deserializacao em src/storage/entities.rs cobrindo NewRelationship com campo type
- Passo 3: adicionar teste de deserializacao confirmando que NewEntity com type continua funcionando (regressao)
- Passo 4: adicionar teste de deserializacao para GraphInput via --graph-stdin com type em relacoes
- Passo 5: atualizar doc comment de NewRelationship (linha 28-31) mencionando alias type
- Passo 6: atualizar CLAUDE.md secao Anexar Grafo no remember mencionando que type eh aceito como alias de relation
### Complexidade
- Mudanca no struct: MINIMA (1 linha, adicionar atributo serde)
- Testes novos: BAIXA (30 linhas, 3-4 testes de deserializacao)
- Documentacao: BAIXA (2 linhas, doc comment + CLAUDE.md)
### Arquivos Afetados
- src/storage/entities.rs:39 — adicionar serde alias type ao campo relation
- src/storage/entities.rs:28-31 — atualizar doc comment de NewRelationship
- src/storage/entities.rs (final) — adicionar modulo cfg(test) com testes de deserializacao


## G11 — BUG MEDIUM (CORRIGIDO v1.0.67): normalize_entity_name Ignora Ponto, Barra e Outros Pontuadores
### Status: CORRIGIDO — parsers/mod.rs:203 mapeia ALL [^a-z0-9] para hifen via is_ascii_alphanumeric
### Severidade: MEDIUM — canonicalização incompleta gera quase-duplicatas silenciosas
### Problema
- `normalize_entity_name` converte APENAS espaço e underscore em hífen
- Caracteres `.` `/` `@` `#` `:` `\` passam intactos pelo pipeline
- Nomes como `lei-14.478/2022` permanecem `lei-14.478/2022` em vez de `lei-14-478-2022`
- Nomes como `agents.md` permanecem `agents.md` em vez de `agents-md`
- Nomes como `src/main.rs` permanecem `src/main.rs` em vez de `src-main-rs`
- O contrato da função promete kebab-case ASCII mas entrega caracteres não alfanuméricos
- Banco real já contém entidades com `.` (ex: `agents.md`, `agents.pt-br.md`) que escaparam à normalização
### Consequências do Problema
- Entidades com pontuação exigem `rename-entity` manual para cada ocorrência
- `normalize-entities --yes` não captura essas quase-duplicatas
- Fragmentação silenciosa: `agents.md` e `agents-md` coexistem como nós distintos
- `hybrid-search` e `recall` perdem sinal semântico por entidades fragmentadas
- `graph traverse` não conecta nós que deveriam ser o mesmo conceito
- `merge-entities` precisa ser invocado manualmente para cada par
- Ingestão via `--mode claude-code` ou `--enable-ner` gera nomes com pontuação que ficam permanentemente não canônicos
- A promessa de idempotência (normalizar já normalizado retorna igual) é falsa para nomes com pontuação
- Custo operacional cresce linearmente com o número de entidades com pontuação no grafo
- O campo `entity_type: file` é o mais afetado — caminhos como `src/main.rs` mantêm barras
### Causa Raiz — 5 Porquês
- POR QUE nomes com ponto e barra não são normalizados?
  - Porque `normalize_entity_name` usa `.replace([' ', '_'], "-")` que lista APENAS espaço e underscore
- POR QUE o replace lista apenas espaço e underscore?
  - Porque o pipeline original foi desenhado para converter `snake_case` e `Title Case` em `kebab-case`
- POR QUE o pipeline não considerou outros separadores?
  - Porque os casos de teste iniciais cobriram apenas nomes de pessoas e identificadores de código
- POR QUE os testes não cobrem ponto e barra?
  - Porque os 9 testes em `entity_name_tests` testam acentos, espaços, underscores, hífens e strings vazias — nenhum testa pontuação
- POR QUE a raiz é o conjunto de separadores na linha 200?
  - Porque a regra `[' ', '_']` é uma ALLOWLIST de separadores quando deveria ser uma DENYLIST de caracteres permitidos (`[a-z0-9-]`)
### Evidência no Código — Linhas Exatas
- `src/parsers/mod.rs:195` — assinatura `pub fn normalize_entity_name(s: &str) -> String`
- `src/parsers/mod.rs:198` — NFKD: `s.nfkd().filter(|c| c.is_ascii()).collect()` — filtra não-ASCII mas mantém pontuação ASCII
- `src/parsers/mod.rs:200` — CAUSA RAIZ: `ascii.to_lowercase().replace([' ', '_'], "-")` — lista fixa de separadores
- `src/parsers/mod.rs:201-214` — colapso de hífens consecutivos e trim — funciona corretamente MAS só recebe hífens de espaço/underscore
- `src/parsers/mod.rs:320-384` — 9 testes: ZERO cobrem `.` `/` `@` `#` `:` `\`
- `src/parsers/mod.rs:185-193` — doc examples mostram APENAS espaço e underscore como separadores
### Reprodução — Evidência Direta
- `sqlite-graphrag link --from "lei-14.478/2022" --to "regulacao" --relation applies-to --create-missing --json`
- Resultado: entidade criada com nome `lei-14.478/2022` em vez de `lei-14-478-2022`
- `sqlite-graphrag link --from "lei-14-478-2022" --to "regulacao" --relation applies-to --create-missing --json`
- Resultado: SEGUNDA entidade criada — duplicata semântica que `normalize-entities` não captura
### Solução Proposta
- Trocar a regra de ALLOWLIST de separadores por DENYLIST de caracteres permitidos
- Substituir `.replace([' ', '_'], "-")` por mapeamento de `[^a-z0-9]` para `-`
- Manter o colapso de hífens consecutivos e trim das bordas (já funciona)
- Resultado: qualquer caractere que não seja letra minúscula ou dígito vira hífen
### Benefícios da Solução
- Canonicalização COMPLETA: todo caractere não alfanumérico é convertido
- Zero pontuação residual em nomes de entidade
- Eliminação de quase-duplicatas por formatação diferente
- `normalize-entities --yes` captura TODOS os casos, não apenas espaço/underscore
- Redução de `rename-entity` e `merge-entities` manuais
- Idempotência real: `normalize(normalize(x)) == normalize(x)` para QUALQUER entrada
- Consistência com o contrato declarado de kebab-case ASCII
### Como Solucionar
- Passo 1: em `src/parsers/mod.rs:200`, substituir `.replace([' ', '_'], "-")` por mapeamento que converta QUALQUER `[^a-z0-9]` em `-`
- Passo 2: a implementação pode usar `.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()` após o `to_lowercase()`
- Passo 3: o colapso de hífens consecutivos (linhas 202-214) já trata múltiplos hífens — nenhuma mudança necessária
- Passo 4: atualizar doc comments (linhas 185-193) com exemplos de ponto e barra
- Passo 5: adicionar testes para pontuação em `entity_name_tests`:
  - `assert_eq!(normalize_entity_name("lei-14.478/2022"), "lei-14-478-2022")`
  - `assert_eq!(normalize_entity_name("src/main.rs"), "src-main-rs")`
  - `assert_eq!(normalize_entity_name("user@domain.com"), "user-domain-com")`
  - `assert_eq!(normalize_entity_name("v1.0.66"), "v1-0-66")`
  - `assert_eq!(normalize_entity_name("key:value"), "key-value")`
- Passo 6: executar `normalize-entities --dry-run` no banco real para verificar impacto antes de aplicar
### Complexidade
- Mudança na função: BAIXA (~1 linha alterada, lógica de colapso já existe)
- Testes novos: BAIXA (~10 linhas, 5 asserts adicionais)
- Impacto em dados existentes: MÉDIO — `normalize-entities --yes` vai renomear entidades com pontuação existentes
### Arquivos Afetados
- `src/parsers/mod.rs:200` — substituir `.replace([' ', '_'], "-")` por mapeamento `[^a-z0-9] → -`
- `src/parsers/mod.rs:185-193` — atualizar doc examples com casos de pontuação
- `src/parsers/mod.rs:320-384` — adicionar 5 testes de pontuação em `entity_name_tests`


## G13 MEDIUM (CORRIGIDO v1.0.67) — Comandos de busca rejeitam --top-k (convencao padrao de vector search) e deep-research nao aceita --limit
### Status: CORRIGIDO — aliases = ["limit", "top-k"] adicionados nos 3 comandos de busca
### Problema
- hybrid-search, recall e deep-research rejeitam --top-k com exit 2 (Clap parsing error)
- --top-k eh a convencao padrao em ferramentas de vector search (FAISS, Milvus, Qdrant, Chroma, Pinecone)
- O flag --k como nome longo eh incomum — CLIs usam flags longas descritivas
- deep-research NAO aceita --limit como alias de --k (recall e hybrid-search aceitam)
- Quando --top-k falha e o output eh piped para jaq, o exit code real eh mascarado
- Clap emite texto de erro, jaq recebe non-JSON e falha — o exit code reportado eh do jaq, nao do sqlite-graphrag
- Agentes LLM usam --top-k por padrao porque eh o termo canonico em KNN search
- A mensagem de erro do Clap nao sugere --k como alternativa correta
### Consequencias
- Agentes Claude Code que geram comandos com --top-k falham silenciosamente em pipelines
- O exit code mascarado (5 do jaq vs 2 do Clap) dificulta diagnostico automatizado
- Usuarios vindos de FAISS, Qdrant, Milvus, Chroma esperam --top-k e sao surpreendidos
- deep-research aceita --k mas NAO --limit — inconsistencia com recall e hybrid-search
- Tres convencoes para o mesmo conceito: --k (canonico), --limit (alias parcial), --top-k (inexistente)
- Pipeline agente + jaq quebra porque jaq recebe texto Clap em vez de JSON
- Documentacao do CLAUDE.md usa --k consistentemente mas agentes externos nao conhecem a convencao
- O flag --k como long option viola principio de CLIs onde flags longas sao descritivas
### Causa Raiz — 5 Porques
- POR QUE falha com --top-k? Clap nao tem alias "top-k" definido no campo k
- POR QUE nao tem alias? O atributo #[arg] define apenas alias = "limit" (e mesmo esse falta no deep-research)
- POR QUE nao foi adicionado? A CLI adotou --k como convencao propria sem considerar convencoes de vector search
- POR QUE deep-research nao tem --limit? Ao implementar deep-research (v1.0.64), o alias "limit" foi esquecido
- POR QUE a inconsistencia nao foi detectada? ZERO testes de integracao que validem aliases de flags entre comandos
### Evidencia no Codigo
- src/commands/hybrid_search.rs:41 — #[arg(short = 'k', long, alias = "limit")] com alias "limit" mas SEM "top-k"
- src/commands/recall.rs:45 — #[arg(short = 'k', long, alias = "limit")] com alias "limit" mas SEM "top-k"
- src/commands/deep_research.rs:42-48 — #[arg(long, short)] SEM alias "limit" NEM "top-k"
- Nenhum dos tres comandos define visible_alias = "top-k" para exibicao no --help
- Inconsistencia: recall e hybrid-search aceitam --limit, deep-research NAO
### Reproducao
- sqlite-graphrag hybrid-search "query" --top-k 3 --json → exit 2: unexpected argument '--top-k'
- sqlite-graphrag recall "query" --top-k 3 --json → exit 2: unexpected argument '--top-k'
- sqlite-graphrag deep-research "query" --top-k 3 --json → exit 2: unexpected argument '--top-k'
- sqlite-graphrag deep-research "query" --limit 3 --json → exit 2: unexpected argument '--limit'
- sqlite-graphrag recall "query" --limit 3 --json → exit 0 (funciona — inconsistencia com deep-research)
- Pipeline mascarado: sqlite-graphrag hybrid-search "q" --top-k 3 --json 2>&1 | jaq '...' → exit code do jaq, NAO do sqlite-graphrag
### Solucao Proposta
- Adicionar visible_alias = "top-k" ao campo k em recall, hybrid-search e deep-research
- Adicionar alias = "limit" ao campo k em deep-research (paridade com recall e hybrid-search)
- Usar visible_alias para que --top-k apareca no --help como alternativa documentada
### Beneficios
- Agentes LLM param de falhar ao usar --top-k — convencao universalmente reconhecida em vector search
- Paridade de aliases: os tres comandos aceitam --k, --limit e --top-k
- Zero breaking change: --k continua como nome canonico
- Diagnostico mais facil: pipelines param de mascarar exit codes por falha de flag
- Principio de menor surpresa: usuarios de FAISS, Milvus, Qdrant, Chroma, Pinecone encontram --top-k no --help
- Consistencia interna: deep-research passa a aceitar --limit como recall e hybrid-search
### Como Solucionar
- Passo 1: em src/commands/hybrid_search.rs:41, alterar alias = "limit" para aliases = ["limit", "top-k"]
- Passo 2: em src/commands/recall.rs:45, alterar alias = "limit" para aliases = ["limit", "top-k"]
- Passo 3: em src/commands/deep_research.rs:42-48, adicionar aliases = ["limit", "top-k"] ao campo k
- Passo 4: considerar usar visible_aliases em vez de aliases para que --top-k e --limit aparecam no --help
- Passo 5: adicionar testes de integracao que invoquem cada comando com --top-k e --limit verificando exit 0
- Passo 6: atualizar doc comments dos tres campos k mencionando aliases --limit e --top-k
- Passo 7: atualizar CLAUDE.md secao de campos criticos mencionando que --top-k eh aceito como alias
### Complexidade
- Mudanca nos structs: MINIMA (3 linhas alteradas, um atributo por comando)
- Testes novos: BAIXA (~20 linhas, 6 testes de integracao)
- Documentacao: BAIXA (3 linhas de doc comment + CLAUDE.md)
### Arquivos Afetados
- src/commands/hybrid_search.rs:41 — adicionar alias "top-k" ao campo k
- src/commands/recall.rs:45 — adicionar alias "top-k" ao campo k
- src/commands/deep_research.rs:42-48 — adicionar aliases "limit" e "top-k" ao campo k



## G14 HIGH (ADIADO — 3-5 sprints) — Arquitetura acoplada a SQLite local impede uso multi-maquina e causa corrupcao via Dropbox sync
### Status: ADIADO — requer redesign de 46 arquivos e 135 callsites; src/storage/backend.rs existe como fase 1 (trait placeholder)
### Problema
- sqlite-graphrag opera EXCLUSIVAMENTE com arquivo SQLite local (graphrag.sqlite)
- Sincronizacao via Dropbox/Google Drive entre maquinas causa corrupcao do banco
- SQLite NAO suporta escritas concorrentes via rede ou filesystem distribuido
- WAL mode depende de shared memory (SHM) que cloud sync NAO preserva
- Edicao simultanea de duas maquinas corrompe WAL, FTS5 e sqlite-vec indexes
- Cenario real: PC no sitio rodando automacao + MacBook na farmacia para uso diario
- WhatsApp CLI precisa de banco online (roda continuamente, acessivel de qualquer maquina)
- Daemon de embedding em uma maquina NAO eh acessivel de outra
- 1014+ memorias em risco de corrupcao a cada sync do Dropbox
### Consequencias
- Corrupcao silenciosa do banco quando duas maquinas escrevem via Dropbox sync
- Perda de memorias, entidades e relacionamentos sem possibilidade de recovery
- health --json retorna integrity_ok: false apos sync conflitante
- FTS5 index corrompe primeiro (256 referencias no codigo) — hybrid-search degrada
- sqlite-vec virtual tables (28 referencias) corrompem — recall retorna zero resultados
- Usuario FORÇADO a usar uma unica maquina por vez para evitar corrupcao
- Impossivel rodar WhatsApp CLI (sempre-ligado) e CLI interativa na mesma base
- Backup via sqlite-graphrag backup eh pontual, NAO resolve acesso concorrente
- Zero replicacao: perda da maquina = perda de TODAS as memorias
- Modelo de embedding (multilingual-e5-small) precisa ser baixado em cada maquina separadamente
### Causa Raiz — 5 Porques
- POR QUE corrompe via Dropbox? SQLite depende de file locking (fcntl/flock) que cloud sync NAO respeita
- POR QUE depende de file locking? WAL mode usa shared memory (-shm file) que requer acesso local ao mesmo filesystem
- POR QUE NAO ha alternativa? A arquitetura inteira esta acoplada a rusqlite::Connection sem abstracão de storage
- POR QUE nao ha abstracão? O projeto nasceu como CLI local — 46 arquivos referenciam rusqlite diretamente
- POR QUE nao foi planejado? Decisao de design original priorizou simplicidade (SQLite = zero config) sobre portabilidade multi-maquina
### Evidencia no Codigo — Acoplamento Profundo
- 46 arquivos em src/ referenciam rusqlite diretamente
- 135 chamadas a open_rw/open_ro/ensure_db_ready (pontos de entrada do banco)
- src/storage/connection.rs:40-44 — open_rw() retorna rusqlite::Connection concreto, sem trait
- src/storage/connection.rs:71-122 — ensure_db_ready() opera sobre paths locais via std::path::Path
- src/storage/memories.rs — 38 chamadas a conn. (metodos diretos em Connection)
- src/pragmas.rs:41-58 — PRAGMAs SQLite-especificos hardcoded (WAL, busy_timeout, mmap, cache_size)
- 256 referencias a FTS5 (SQLite-especifico, sem equivalente em PostgreSQL/libSQL remoto)
- 28 referencias a sqlite-vec extension (C extension carregada via sqlite3_auto_extension)
- Cargo.toml — rusqlite 0.37 com feature bundled (compila SQLite embutido)
- Cargo.toml — refinery 0.9 com feature rusqlite (migracoes acopladas a rusqlite)
- ZERO traits de abstracão: nenhum pub trait Storage, Repository ou Backend em todo src/
- ZERO uso de dyn ou generics para backend de dados
### Reproducao — Cenario de Corrupcao
- Maquina A: sqlite-graphrag remember --name test-a --type note --description "A" --body "from A"
- Maquina B (mesmo Dropbox): sqlite-graphrag remember --name test-b --type note --description "B" --body "from B"
- Dropbox sincroniza graphrag.sqlite, graphrag.sqlite-wal e graphrag.sqlite-shm em momentos diferentes
- Resultado: sqlite-graphrag health --json retorna integrity_ok: false
- sqlite-graphrag recall "test" --json pode retornar exit 10 (database error) ou resultados inconsistentes
### Opcoes de Solucao Pesquisadas
- Opcao 1 (RECOMENDADA): Turso/libSQL com embedded replicas
- libSQL eh fork do SQLite com replicacao built-in
- Embedded replicas: arquivo SQLite local que sincroniza com servidor remoto
- Leitura local (latencia ~0ms) + escritas replicadas para nuvem
- Crate libsql-client 0.34 com API sincrona similar a rusqlite (SyncClient)
- Turso oferece free tier e self-hosted options
- Escrito em Rust (afinidade natural com sqlite-graphrag)
- trustScore 8.9 no context7
- Opcao 2: Supabase + pgvector
- PostgreSQL com extensao pgvector para busca vetorial
- Transacoes ACID, concorrencia total, zero corrupcao
- Requer reescrita COMPLETA da camada de storage
- Busca hibrida via BM25 + HNSW (equivalente funcional a FTS5 + sqlite-vec)
- Free tier generoso, self-hosted possivel
- Complexidade: MUITO ALTA — 46 arquivos + SQL incompativel
- Opcao 3: Litestream (replicacao unidirecional)
- Replica WAL para S3/Backblaze como backup continuo
- NAO resolve acesso concorrente (read-only replicas)
- Resolve apenas backup, NAO portabilidade
- Opcao DESCARTADA: Pinecone
- NAO permite self-hosting
- Custo mais alto (350 USD/mes vs 150-200 USD)
- Sem controle dos dados — violaria principio de soberania
### Solucao Proposta — Turso/libSQL Embedded Replicas (Faseada)
- Fase 1 (FUNDACAO): Criar trait StorageBackend abstraindo rusqlite::Connection
- Definir trait com metodos para CRUD de memorias, entidades, relacoes, chunks
- Implementar SqliteBackend como wrapper do codigo atual (ZERO mudanca funcional)
- Migrar handlers de conn: &Connection para backend: &dyn StorageBackend
- Fase 2 (MIGRAÇÃO): Substituir rusqlite por libsql-client internamente no SqliteBackend
- libsql-client SyncClient tem API similar a rusqlite (execute, query, batch)
- Manter compatibilidade total com banco local existente
- Verificar suporte a sqlite-vec e FTS5 no libSQL
- Fase 3 (REPLICACAO): Adicionar modo --sync-url para embedded replicas
- Flag --sync-url <TURSO_URL> no CLI para ativar replicacao
- Leitura local + sync automatico com servidor remoto
- Migracao de banco existente: export NDJSON -> import no novo backend
- Fase 4 (MULTI-MAQUINA): Resolver daemon e embedding distribuido
- Daemon de embedding por maquina com sync de vetores via libSQL
- Health check remoto via --sync-url
### Beneficios
- Acesso seguro ao banco de QUALQUER maquina sem corrupcao
- WhatsApp CLI e CLI interativa podem operar simultaneamente
- Backup continuo automatico via replicacao (zero risco de perda)
- Latencia de leitura identica a local (embedded replicas)
- Compatibilidade retroativa: modo local continua funcionando sem --sync-url
- Evolucao futura para web UI, mobile access, API HTTP sobre o mesmo banco
- Fim da dependencia exclusiva de uma unica maquina
### Como Solucionar
- Passo 1: definir trait StorageBackend em src/storage/mod.rs com metodos CRUD
- Passo 2: implementar struct SqliteBackend que encapsula rusqlite::Connection
- Passo 3: migrar src/storage/memories.rs (38 conn. calls) para usar trait
- Passo 4: migrar src/storage/entities.rs (10 rusqlite refs) para usar trait
- Passo 5: migrar src/storage/chunks.rs, versions.rs, fusion.rs, urls.rs, utils.rs
- Passo 6: migrar 135 callsites de open_rw/open_ro/ensure_db_ready para factory do backend
- Passo 7: adicionar libsql-client como dependencia opcional via feature flag
- Passo 8: implementar LibsqlBackend com SyncClient para modo embedded-replica
- Passo 9: adicionar --sync-url ao CLI (Clap arg global) para ativar replicacao
- Passo 10: criar comando migrate-to-remote para exportar banco local para Turso
- Passo 11: atualizar CLAUDE.md com documentacao do modo online
- Passo 12: adicionar testes de integracao para ambos backends
### Complexidade
- Trait de abstracão: ALTA (46 arquivos afetados, 135 pontos de entrada)
- Migracão interna: ALTA (28.208 LOC, 256 refs FTS5, 28 refs sqlite-vec)
- Novo backend libSQL: MEDIA (API similar a rusqlite, SyncClient 1:1)
- Testes: ALTA (cobertura existente precisa funcionar em ambos backends)
- Documentacão: MEDIA (CLAUDE.md, README, after_long_help)
- Estimativa total: 3-5 sprints de trabalho focado
### Arquivos Afetados (amostra — 46 arquivos no total)
- src/storage/connection.rs — refatorar open_rw/open_ro para factory do backend
- src/storage/memories.rs — 38 chamadas conn. para trait methods
- src/storage/entities.rs — 10 refs rusqlite para trait methods
- src/storage/chunks.rs — embedding storage para trait methods
- src/storage/fusion.rs — hybrid search internals para trait methods
- src/storage/utils.rs — helper queries para trait methods
- src/storage/versions.rs — version history para trait methods
- src/storage/urls.rs — URL storage para trait methods
- src/pragmas.rs — PRAGMAs SQLite-especificos para backend-aware config
- src/commands/*.rs — TODOS os 30+ handlers usam Connection diretamente
- Cargo.toml — adicionar libsql-client como feature opcional


## G15 HIGH (CORRIGIDO v1.0.67) — remember --force-merge e edit re-embeddam body inteiro incondicionalmente mesmo quando conteudo nao mudou e edit ignora chunks
### Status: CORRIGIDO — remember.rs:720-727 compara body_hash (skip re-embed se inalterado); edit.rs:143 mesma lógica adicionada
### Problema
- remember --force-merge SEMPRE deleta todos os chunks e re-embeda o body inteiro, mesmo que o conteudo NAO tenha mudado
- edit NAO gerencia chunks — computa UM unico embedding para o body inteiro independente do tamanho
- Nenhum dos dois compara body_hash antigo com novo ANTES de disparar o pipeline de embedding
- Para memorias grandes (~53KB, ~50+ chunks), remember --force-merge gasta ~10-15 segundos re-processando tudo
- edit em body de 53KB gera UM embedding agregado perdendo a granularidade de busca por chunks
- Agentes LLM que usam --force-merge em loops idempotentes re-processam 100% dos embeddings a cada iteracao
- O daemon de embedding processa cada chunk serialmente (loop no remember.rs:600-618) sem paralelismo
- Memorias grandes atualizadas frequentemente (ex: rules documents ~53KB) sofrem overhead desproporcional
### Consequencias
- Desperdicio computacional: 50+ embeddings recalculados quando ZERO bytes mudaram no body
- Latencia desnecessaria: ~10-15s para remember --force-merge em body 53KB com daemon ativo
- Sem daemon: ~100s+ por operacao (carrega modelo ONNX para cada embedding)
- edit perde granularidade: body de 53KB com 181 headings gera UM embedding — recall e hybrid-search encontram a memoria como bloco unico em vez de chunks semanticos
- Agentes automatizados que rodam remember --force-merge periodicamente consomem CPU e RAM desnecessariamente
- Risco de timeout em pipelines automatizados quando body eh grande
- Pipeline de ingest com --mode claude-code multiplica o problema: cada arquivo re-processado integralmente
- Sessoes longas de agente acumulam operacoes redundantes de embedding
- Pressao de memoria RSS acumulativa em loops com memorias grandes pode triggerar exit 77
### Causa Raiz — 5 Porques
- POR QUE re-embeda tudo? remember.rs:684 executa delete_chunks incondicionalmente antes de re-inserir
- POR QUE nao compara antes? O body_hash (blake3 na linha 376) eh calculado mas NUNCA comparado com o hash existente no banco
- POR QUE edit nao faz chunks? edit.rs:176 chama embed_passage_or_local com o body inteiro sem chunking
- POR QUE edit e remember divergem? edit foi implementado como operacao leve (update metadata + body) sem chunking; remember foi implementado como operacao completa (full re-index)
- POR QUE nao ha skip? O fluxo assume que toda chamada ao remember ou edit com body requer re-indexacao total — nao existe curto-circuito por hash
### Evidencia no Codigo — Tres Lacunas Distintas
- LACUNA 1: remember.rs:684 — delete_chunks(&tx, existing_id) INCONDICIONAL no path --force-merge
- remember.rs:376 — body_hash calculado via blake3 MAS comparado apenas com OTHER memorias via find_by_hash (linha 536)
- O hash da memoria EXISTENTE nunca eh lido para comparacao — existente eh identificado por (namespace, name), nao por hash
- LACUNA 2: edit.rs:176 — embed_passage_or_local(&paths.models, &new_body) computa UM embedding para body inteiro
- edit.rs NAO importa chunking module — ZERO referencias a chunk em todo o arquivo
- edit.rs NAO chama delete_chunks nem insert_chunk_slices — chunks anteriores ficam ORFAOS no banco
- LACUNA 3: edit.rs:131 — body_changed = raw_body.is_some() testa se body FOI PASSADO, NAO se body MUDOU
- edit.rs:134 — new_hash calculado mas NUNCA comparado com hash existente na row (row.body_hash)
- Re-embedding dispara quando body eh passado, mesmo identico ao existente
### Reproducao
- Criar memoria grande: sqlite-graphrag remember --name large-doc --type document --description "test" --body-file big-53k.md
- Verificar chunks: sqlite-graphrag stats --json (observar chunks_total incrementado)
- Force-merge sem mudanca: sqlite-graphrag remember --name large-doc --type document --description "test" --body-file big-53k.md --force-merge
- Resultado: TODOS os chunks deletados e re-criados, TODOS os embeddings recalculados (~10-15s)
- Esperado: comparar body_hash, detectar body identico, skip de re-embedding (< 1s)
- Edit perdendo chunks: sqlite-graphrag edit --name large-doc --body-file big-53k.md
- Resultado: UM embedding para 53KB, chunks anteriores ficam orfaos, recall perde granularidade
### Solucao Proposta — Tres Correcoes Independentes
- Correcao 1 (SKIP por hash): comparar body_hash do input com body_hash da memoria existente
- Se hashes iguais: skip de delete_chunks, skip de re-embedding, manter chunks e vetores existentes
- Se hashes diferentes: executar pipeline completo (comportamento atual)
- Implementar em remember.rs ANTES da linha 549 (tokenizer) e em edit.rs ANTES da linha 171
- Correcao 2 (CHUNKS no edit): adicionar chunking ao edit.rs para bodies que produzem 2+ chunks
- Importar chunking module em edit.rs
- Chamar split_into_chunks_hierarchical quando body muda
- Deletar chunks antigos e inserir novos (mesmo pipeline de remember.rs)
- Correcao 3 (DIFF de chunks): para bodies que mudaram parcialmente, computar diff por chunk
- Comparar hash de cada chunk novo com chunk existente na mesma posicao
- Re-embedar apenas chunks que mudaram de fato
- Otimizacao avancada: requer refatoracao da tabela memory_chunks para armazenar chunk_hash
### Beneficios
- Skip por hash elimina ~100% do overhead em loops idempotentes (caso mais comum de --force-merge)
- Latencia de --force-merge com body identico cai de ~10-15s para < 1s
- edit passa a manter granularidade de busca por chunks em bodies grandes
- Agentes LLM podem usar --force-merge liberalmente sem penalidade de performance
- Reducao de carga no daemon de embedding em sessoes longas
- Pipeline ingest com --mode claude-code beneficia-se do skip por hash em re-ingestoes
- Chunks orfaos eliminados: edit passa a gerenciar ciclo de vida completo dos chunks
- Diff por chunk (fase avancada) reduziria re-embedding a ~5-10% do body em atualizacoes tipicas
### Como Solucionar
- Passo 1: em remember.rs, ANTES da linha 549, ler body_hash da memoria existente via read_by_name
- Passo 2: comparar body_hash do input com body_hash existente — se iguais, setar flag skip_reindex = true
- Passo 3: quando skip_reindex, pular tokenizer, chunking, embedding, delete_chunks e insert_chunks
- Passo 4: ainda permitir update de metadata (type, description) mesmo com skip_reindex
- Passo 5: em edit.rs, importar chunking module e storage_chunks
- Passo 6: em edit.rs, apos linha 131, comparar new_hash com row.body_hash existente
- Passo 7: se hashes iguais em edit.rs, skip re-embedding (body nao mudou de fato)
- Passo 8: se hashes diferentes em edit.rs, executar chunking e insert_chunk_slices (paridade com remember)
- Passo 9: adicionar campo chunk_hash a tabela memory_chunks para diff futuro por chunk
- Passo 10: adicionar testes unitarios para skip por hash em remember e edit
- Passo 11: adicionar teste de integracao confirmando chunks preservados apos --force-merge sem mudanca
- Passo 12: emitir campo body_unchanged: true no JSON response quando skip_reindex aplicado
### Complexidade
- Correcao 1 (skip por hash): BAIXA (~20 linhas em remember.rs + ~10 linhas em edit.rs)
- Correcao 2 (chunks no edit): MEDIA (~40 linhas, importar chunking + pipeline de chunks)
- Correcao 3 (diff por chunk): ALTA (migracao de schema + logica de comparacao por chunk)
- Testes: BAIXA (~30 linhas, 3-4 testes unitarios + 2 integracao)
- Documentacao: BAIXA (CLAUDE.md mencionar body_unchanged no JSON)
### Arquivos Afetados
- src/commands/remember.rs:376-684 — adicionar comparacao de body_hash e flag skip_reindex
- src/commands/edit.rs:131-187 — adicionar comparacao de hash, importar chunking, gerenciar chunks
- src/storage/chunks.rs — adicionar campo chunk_hash na struct Chunk (fase avancada)
- src/storage/memories.rs — expor body_hash no resultado de find_by_name ou read_by_name
- migrations/ — migracao para adicionar chunk_hash a memory_chunks (fase avancada)



## G16 HIGH (CORRIGIDO v1.0.67) — rename falha com exit 10 (UNIQUE constraint) quando memória soft-deleted ocupa o nome alvo e purge destrói colaterais
### Status: CORRIGIDO — rename.rs auto-purga ghost soft-deleted antes do UPDATE; emite ghost_purged: true no JSON
### Problema
- `rename --from A --to B` falha com exit 10 quando uma memória soft-deleted já ocupa o nome B
- O UNIQUE(namespace, name) na tabela memories (V001__init.sql:22) cobre TODAS as linhas incluindo soft-deleted
- O comando rename NÃO verifica se uma memória soft-deleted bloqueia o nome alvo antes de executar UPDATE
- O workaround exige `purge --retention-days 0 --yes` que destrói TODAS as memórias soft-deleted do namespace
- No caso real reportado, purge destruiu 7 memórias (71.911 bytes) para desbloquear 1 único nome
- O `ingest` deriva nomes via `derive_kebab_name` que difere da nomeação manual do usuário
- Arquivo `rules-serde-rust-serialization.md` gera nome `rust-serde-serialization-rules` (ordem diferente dos segmentos)
- Consolidação de nomes exige: forget antigo → purge (colateral) → rename novo → edit descrição
- O erro `UNIQUE constraint failed: memories.namespace, memories.name` é reportado como exit 10 (database error) em vez de exit 9 (duplicate) ou mensagem explicativa
### Consequências
- Perda colateral de memórias: purge sem --name destrói TODAS as soft-deleted do namespace
- No caso real, 7 memórias (71.911 bytes) foram destruídas para desbloquear 1 nome
- Impossível reverter: memórias purgadas são permanentemente destruídas (DELETE físico)
- O exit 10 (database error) engana o agente LLM que não associa UNIQUE constraint a soft-delete
- Pipeline de consolidação pós-ingest requer 4 passos manuais (forget → purge → rename → edit)
- Agentes automatizados que renomeiam memórias falham sem diagnóstico claro
- O purge --name existe mas não é sugerido pelo erro — agente usa purge global por padrão
- Nomes divergentes entre ingest (derivado) e manual (escolhido) criam duplicatas semânticas
- Re-ingestão com `--mode claude-code` ou `--mode codex` gera nomes diferentes do canônico existente
### Causa Raiz — 5 Porquês
- POR QUE rename falha? O UPDATE na linha 170 de rename.rs viola UNIQUE(namespace, name) quando memória soft-deleted ocupa o alvo
- POR QUE não detecta antes? rename.rs NÃO chama `find_by_name_any_state` para o nome ALVO — só verifica o nome FONTE (linha 142)
- POR QUE o UNIQUE inclui soft-deleted? A constraint `UNIQUE(namespace, name)` em V001__init.sql:22 é table-level sem condição WHERE
- POR QUE não usa partial index? SQLite suporta `CREATE UNIQUE INDEX WHERE deleted_at IS NULL` (partial unique index) desde 3.8.0 (2013), mas o schema original não utilizou
- POR QUE purge destrói colaterais? `purge --retention-days 0 --yes` sem `--name` opera em TODAS as memórias soft-deleted do namespace, não apenas na que bloqueia o alvo
### Evidência no Código — Quatro Lacunas Distintas
- LACUNA 1 (rename não verifica alvo): rename.rs:142-143 chama `find_by_name` apenas para o nome FONTE
- rename.rs:170 executa `UPDATE memories SET name=?2 WHERE id=?1 AND deleted_at IS NULL` sem verificar o ALVO
- O código de remember.rs:462-473 JÁ implementa a verificação correta via `find_by_name_any_state` + `clear_deleted_at` para --force-merge
- rename.rs simplesmente NÃO reproduz esse padrão para o nome de destino
- LACUNA 2 (UNIQUE não é parcial): V001__init.sql:22 define `UNIQUE(namespace, name)` como table constraint
- Essa constraint NÃO pode ser condicional — SQLite não suporta WHERE em table-level UNIQUE
- Necessário: dropar a constraint e criar `CREATE UNIQUE INDEX idx_mem_ns_name_live ON memories(namespace, name) WHERE deleted_at IS NULL`
- LACUNA 3 (exit code incorreto): O erro é reportado como exit 10 (database error genérico)
- Deveria ser exit 9 (duplicate) com mensagem explicando que memória soft-deleted bloqueia o nome
- O agente LLM interpreta exit 10 como corrupção do banco em vez de conflito de nomes
- LACUNA 4 (ingest gera nomes divergentes): derive_kebab_name (ingest.rs:1339-1375) normaliza basename via NFD + filtro ASCII
- Arquivo `rules-serde-rust-serialization.md` → stem `rules-serde-rust-serialization` → nome idêntico
- Mas arquivo nomeado diferentemente gera segmentos em ordem diferente do nome canônico
- ingest NÃO verifica soft-deleted via `find_by_name_any_state` no path de persist (ingest.rs:615-621)
- INSERT falha silenciosamente com UNIQUE se soft-deleted ocupa o nome derivado
### Reprodução
- Criar memória: `sqlite-graphrag remember --name regra-x --type document --description "teste" --body "conteúdo"`
- Soft-delete: `sqlite-graphrag forget --name regra-x`
- Criar segunda memória: `sqlite-graphrag remember --name regra-y --type document --description "teste" --body "conteúdo"`
- Tentar rename: `sqlite-graphrag rename --from regra-y --to regra-x --json`
- Resultado: exit 10 — `UNIQUE constraint failed: memories.namespace, memories.name`
- Esperado: rename detecta soft-deleted, auto-purga o fantasma, e completa o rename com sucesso
- Workaround atual: `purge --retention-days 0 --name regra-x --yes --json` seguido de rename
- Workaround perigoso (usado no caso real): `purge --retention-days 0 --yes --json` SEM --name — destrói TODAS soft-deleted
### Solução Proposta — Três Correções Independentes
- Correção 1 (RENAME detecta e auto-purga fantasma): Antes do UPDATE em rename.rs:170, chamar `find_by_name_any_state` para o nome ALVO
- Se soft-deleted ocupa o alvo: executar DELETE permanente APENAS daquela memória (não purge global)
- Emitir campo `ghost_purged: true` no JSON response para rastreabilidade
- Preservar versions e chunks da memória purgada? NÃO — o rename é uma operação de consolidação
- Correção 2 (UNIQUE parcial): Migrar schema para usar partial unique index
- `DROP INDEX` da constraint atual (requer ALTER TABLE para dropar UNIQUE table-level)
- `CREATE UNIQUE INDEX idx_memories_ns_name_live ON memories(namespace, name) WHERE deleted_at IS NULL`
- Memórias soft-deleted deixam de bloquear nomes — qualquer operação pode reutilizar nomes de memórias deletadas
- Correção 3 (ingest verifica soft-deleted): Em persist_staged (ingest.rs:615-621), substituir `find_by_name` por `find_by_name_any_state`
- Se soft-deleted com mesmo nome existe: auto-purgar e prosseguir com INSERT
- Emitir campo `ghost_purged: true` no evento NDJSON do arquivo
### Benefícios
- rename para de falhar com exit 10 quando memória soft-deleted ocupa o alvo
- Zero perda colateral: apenas a memória-fantasma específica é destruída, não todas as soft-deleted
- Exit code correto (9 em vez de 10) quando conflito de nomes é detectado sem auto-purge
- Partial unique index elimina a classe inteira de problemas de nomes-fantasma
- Agentes LLM podem renomear memórias sem workflow manual de 4 passos
- ingest pode re-ingerir diretórios sem falhar por nomes de memórias previamente deletadas
- Consolidação pós-ingest de nomes (derivado → canônico) reduz de 4 passos para 1
### Como Solucionar
- Passo 1: em rename.rs, ANTES da linha 162 (início da transação), chamar `find_by_name_any_state(&conn, &namespace, &normalized_new_name)`
- Passo 2: se resultado for `Some((ghost_id, true))`, executar `DELETE FROM memories WHERE id = ?1` dentro da transação
- Passo 3: também deletar chunks, versions e memory_entities do ghost_id via CASCADE (já configurado no schema)
- Passo 4: emitir `ghost_purged: true` e `ghost_purged_id: ghost_id` no RenameResponse
- Passo 5: criar migração V0XX para substituir UNIQUE table constraint por partial unique index
- Passo 6: `CREATE UNIQUE INDEX idx_memories_ns_name_live ON memories(namespace, name) WHERE deleted_at IS NULL`
- Passo 7: verificar que find_by_name (usado em 15+ comandos) continua funcionando com partial index
- Passo 8: em ingest.rs:615, substituir `find_by_name` por `find_by_name_any_state`
- Passo 9: se soft-deleted, deletar permanentemente antes do INSERT
- Passo 10: atualizar error handling em rename.rs para mapear UNIQUE constraint → exit 9 com mensagem explicativa
- Passo 11: adicionar teste unitário: rename sobre nome ocupado por soft-deleted deve suceder
- Passo 12: adicionar teste de integração: ingest sobre nome de memória previamente deletada deve suceder
### Complexidade
- Correção 1 (rename auto-purge): BAIXA (~15 linhas em rename.rs)
- Correção 2 (partial unique index): MÉDIA (migração de schema + verificação de compatibilidade com 15+ comandos)
- Correção 3 (ingest verifica soft-deleted): BAIXA (~10 linhas em ingest.rs)
- Testes: BAIXA (~30 linhas, 3-4 testes unitários + 2 integração)
- Documentação: BAIXA (CLAUDE.md mencionar ghost_purged no JSON de rename)
### Arquivos Afetados
- src/commands/rename.rs:142-174 — adicionar verificação de fantasma soft-deleted no nome alvo e auto-purge
- src/commands/ingest.rs:615-621 — substituir find_by_name por find_by_name_any_state com auto-purge
- src/storage/memories.rs — nenhuma mudança necessária (find_by_name_any_state já existe)
- migrations/V0XX__partial_unique_index.sql — migração para substituir UNIQUE table constraint por partial unique index
- src/errors.rs — mapear UNIQUE constraint de memories para exit 9 em vez de exit 10



## G17 MEDIUM (CORRIGIDO v1.0.67) — Nenhum comando CLI aceita memory_id como input apesar de 20 comandos retornarem memory_id no JSON
### Status: CORRIGIDO — read.rs aceita --id para lookup direto por memory_id via memories::n()
### Problema
- `recall`, `hybrid-search`, `list`, `read`, `remember`, `edit`, `rename`, `forget` e mais 12 comandos retornam campo `memory_id` no JSON de resposta
- NENHUM desses comandos aceita `memory_id` ou `--id` como argumento de entrada
- O único identificador de entrada aceito é `--name` (string kebab-case)
- `recall 141 --json` trata "141" como query semântica, gerando embedding vetorial para o texto "141"
- O modelo e5-small computa vetor para string numérica e retorna resultados com distância ~0.20 (quase aleatórios)
- O agente LLM ou operador humano que recebe `memory_id: 141` de um comando anterior NÃO consegue usar esse ID para lookup direto
- `read` aceita apenas `--name <nome>` ou argumento posicional de nome — NUNCA `--id <N>`
- A função `memories::n()` (renomeada de `read_full`) em memories.rs:600 JÁ implementa lookup por `i64` ID, mas não é exposta por nenhum comando CLI
### Consequências
- Ciclo de referência quebrado: comandos retornam `memory_id` como identificador estável, mas o consumidor não pode usá-lo para referenciar a memória
- Agentes LLM que recebem `memory_id: 141` de um `list` ou `recall` precisam memorizar o `name` correspondente para fazer `read`
- Pipeline programático com `jaq` extrai `memory_id` mas precisa VOLTAR a extrair `name` para alimentar o próximo comando
- `recall "141"` gasta ciclo de embedding (~200ms com daemon, ~1.9s sem) para produzir resultados irrelevantes
- Não há detecção de que a query é numérica e poderia ser interpretada como ID
- Inconsistência de contrato: 20 comandos EMITEM `memory_id` no JSON mas 0 comandos CONSOMEM `memory_id` como input
- Em pipelines automatizados, forçar uso de `name` em vez de `id` requer JOIN extra via `list --json | jaq`
- Scripts que processam NDJSON do `ingest` recebem `memory_id` por evento mas precisam de `name` para qualquer operação subsequente
### Causa Raiz — 5 Porquês
- POR QUE `recall 141` retorna resultados irrelevantes? O argumento posicional é SEMPRE tratado como query semântica e embedado como vetor pelo modelo e5-small
- POR QUE não detecta que "141" é numérico? recall.rs:36 define `pub query: String` sem nenhuma validação ou detecção de padrão numérico
- POR QUE read não aceita --id? ReadArgs (read.rs:19-39) define apenas `name_positional` e `--name` sem campo `--id`
- POR QUE a função read_full/n existe mas não é exposta? A função `memories::n()` (memories.rs:600) é usada INTERNAMENTE por recall, hybrid-search e deep-research para hydrating resultados de KNN, mas nenhum handler CLI a expõe como argumento
- POR QUE memory_id é retornado se não pode ser consumido? O campo foi adicionado para compatibilidade com o contrato documentado, mas o caminho inverso (input por ID) nunca foi implementado
### Evidência no Código — Três Lacunas Distintas
- LACUNA 1 (recall não detecta query numérica): recall.rs:36-37 define `pub query: String` sem parser ou validação
- recall.rs:126-130 embeda QUALQUER string via `embed_query_or_local` sem verificar se é numérico puro
- Um número como "141" gera vetor semântico para o texto "141" com distância ~0.20 para resultados aleatórios
- recall NÃO implementa short-circuit: se a query for inteiro puro, poderia chamar `memories::n(id)` diretamente
- LACUNA 2 (read não aceita --id): read.rs:19-39 define ReadArgs com `name_positional: Option<String>` e `name: Option<String>`
- Não existe campo `#[arg(long)] pub id: Option<i64>` em ReadArgs
- A função `memories::n()` (memories.rs:600) já implementa `SELECT ... WHERE id=?1 AND deleted_at IS NULL`
- Bastaria adicionar `--id <N>` com `conflicts_with = "name"` e `conflicts_with = "name_positional"` para expor
- LACUNA 3 (memory_id emitido mas não consumido): 20 arquivos em src/commands/ referenciaram `memory_id` no JSON de saída
- list.rs:61 inclui `memory_id: i64` no ListItem
- recall retorna `memory_id` em RecallItem (output.rs)
- remember, edit, rename, forget, restore retornam `memory_id` na resposta
- NENHUM desses 20 comandos aceita `memory_id` ou `--id` como argumento de entrada
### Reprodução
- Criar memória: `sqlite-graphrag remember --name teste-id --type note --description "teste" --body "conteúdo"`
- Listar e obter ID: `sqlite-graphrag list --limit 1 --json | jaq '.items[0].memory_id'` retorna ex: 1080
- Tentar recall por ID: `sqlite-graphrag recall 1080 --json` retorna resultados IRRELEVANTES (distância ~0.20)
- Tentar read por ID: `sqlite-graphrag read --id 1080 --json` FALHA com erro de argumento desconhecido
- Workaround: `sqlite-graphrag list --limit 1 --json | jaq -r '.items[0].name'` e depois `sqlite-graphrag read --name <nome> --json`
- Esperado: `sqlite-graphrag read --id 1080 --json` retorna a memória diretamente
### Solução Proposta — Duas Correções Independentes
- Correção 1 (read aceita --id): Adicionar campo `#[arg(long, conflicts_with_all = ["name", "name_positional"])] pub id: Option<i64>` em ReadArgs
- No handler `run()`, priorizar `--id` sobre `--name`: se `id` presente, chamar `memories::n(&conn, id)` diretamente
- Se memória com aquele ID não existir no namespace, retornar exit 4 (NotFound) como já faz para `--name`
- Preservar compatibilidade: `--name` e argumento posicional continuam funcionando exatamente como antes
- Correção 2 (recall detecta query numérica pura): Em recall.rs, ANTES de embedar, verificar se a query é inteiro puro via `query.trim().parse::<i64>()`
- Se parse suceder: chamar `memories::n(&conn, id)` diretamente e retornar como único resultado com `distance: 0.0` e `source: "id_lookup"`
- Se parse falhar: prosseguir com fluxo semântico normal (embedding + KNN)
- Emitir campo `lookup_mode: "id"` ou `lookup_mode: "semantic"` no RecallResponse para transparência
### Benefícios
- Ciclo de referência fechado: `memory_id` retornado por qualquer comando pode ser usado como input para `read --id`
- Zero desperdício de embedding: query numérica pura não gasta ciclo de computação vetorial
- Pipelines programáticos podem usar `jaq` para extrair `memory_id` e alimentar `read --id` diretamente
- Agentes LLM podem referenciar memórias por ID sem precisar memorizar ou carregar nomes
- Consistência de contrato: 20 comandos emitem `memory_id` e pelo menos 1 comando consome `memory_id`
- Compatibilidade total: nenhuma mudança em comandos existentes, apenas adição de `--id` em `read`
### Como Solucionar
- Passo 1: em read.rs:19-39, adicionar campo `#[arg(long, conflicts_with_all = ["name", "name_positional"], help = "Memory ID (integer) for direct lookup")] pub id: Option<i64>`
- Passo 2: em read.rs:75-80, alterar resolução de nome para verificar `args.id` primeiro
- Passo 3: se `args.id` presente, chamar `memories::n(&conn, id)` em vez de `memories::read_by_name`
- Passo 4: preservar validação de namespace — verificar que a memória retornada pertence ao namespace ativo
- Passo 5: em recall.rs, ANTES da linha 126, adicionar detecção de query numérica
- Passo 6: se `args.query.trim().parse::<i64>()` suceder, fazer lookup direto e retornar early
- Passo 7: adicionar campo `lookup_mode` em RecallResponse para distinguir id_lookup de semantic
- Passo 8: adicionar testes: `read --id <N>` retorna memória correta; `recall "141"` retorna memória com ID 141
- Passo 9: atualizar `after_long_help` de `read` com exemplo: `sqlite-graphrag read --id 42 --json`
- Passo 10: considerar adicionar `--id` também em `edit`, `forget`, `rename`, `history` e `restore` para consistência completa
### Complexidade
- Correção 1 (read --id): BAIXA (~15 linhas em read.rs, 1 campo Clap + 1 branch no handler)
- Correção 2 (recall detecta numérico): BAIXA (~10 linhas em recall.rs, parse + early return)
- Extensão (--id em outros comandos): MÉDIA (~5 linhas por comando x 5 comandos = ~25 linhas)
- Testes: BAIXA (~20 linhas, 2-3 testes unitários + 1 integração)
- Documentação: BAIXA (atualizar after_long_help e CLAUDE.md)
### Arquivos Afetados
- src/commands/read.rs:19-39 — adicionar campo `--id` em ReadArgs e branch de lookup no handler
- src/commands/recall.rs:126-130 — adicionar detecção de query numérica antes do embedding
- src/output.rs — adicionar campo `lookup_mode` em RecallResponse (opcional)
- src/storage/memories.rs — nenhuma mudança necessária (memories::n já existe e aceita i64)



## G18 HIGH (CORRIGIDO v1.0.67) — Semáforo de concorrência do daemon preso em 4 slots por 3 bugs sobrepostos: métrica de memória com margem excessiva, custo-por-slot superestimado e teto rígido hardcoded
### Status: CORRIGIDO — margem /2 removida (memory_guard.rs:60); ceiling dinâmico 2*nCPUs com env override (lock.rs:80-88)
### Problema
- O semáforo global que limita invocações concorrentes da CLI fica preso em 4 slots mesmo em máquina de 64 GB com ~57 GB realmente disponíveis
- `--max-concurrency 12` (ou 16) NÃO tem efeito: o efetivo permanece 4
- Um 5o worker de enrich fica parado em `--wait-lock`; comandos de gestão (`remember`, `stats`) falham com exit 75 ("all 4 concurrency slots occupied")
- O `daemon --max-concurrency N` é tratado como "sugestão" e clampado silenciosamente para baixo sem informar o motivo
- A fórmula `min(cpus, available_mb / 1100) * 0.5` divide por 2 o resultado como "margem de segurança"
- Numa máquina de 8 CPUs e 57 GB: `min(8, 51) * 0.5 = 4` — resultado coincide com a constante hardcoded
- Mesmo que a fórmula calculasse um valor maior, `lock.rs:79` faz `clamp(1, MAX_CONCURRENT_CLI_INSTANCES)` onde a constante é 4
- O teto rígido de 4 em `lock.rs` anula qualquer cálculo dinâmico ou pedido explícito do usuário
### Consequências
- Em pipelines `enrich --mode claude-code` com muitos arquivos, apenas 4 workers processam simultaneamente
- Workers 5+ ficam bloqueados em `--wait-lock` desperdiçando tempo de sessão e tokens
- Comandos leves de gestão (`remember`, `stats`, `read`) falham com exit 75 por competir pelos mesmos 4 slots
- O paralelismo real é ~25% do que a máquina comporta (4 de ~16 possíveis)
- O usuário que passa `--max-concurrency 12` não recebe feedback sobre o motivo da redução
- A mensagem de log "Reducing requested concurrency" só aparece em modo verbose
- O custo-por-slot de 1100 MB assume que CADA worker carrega o modelo ONNX, mas com daemon ativo o modelo é carregado UMA vez
- Workers de `enrich --mode claude-code` são leves (spawn de `claude -p`), mas pagam o "preço" de embedding que não executam
### Causa Raiz — 5 Porquês
- POR QUE `--max-concurrency 12` resulta em 4? Porque `calculate_safe_concurrency()` calcula 4 e `main.rs:211` faz `requested.min(safe)` = 4
- POR QUE `calculate_safe_concurrency` calcula 4 com 57 GB? Porque a fórmula `min(cpus, available_mb / 1100) * 0.5` com 8 CPUs dá `min(8, 51) / 2 = 4`
- POR QUE divide por 2? O fator `0.5` em `memory_guard.rs:60` (`resource_bound / 2`) é uma "margem de segurança" conservadora que halva o resultado
- POR QUE 1100 MB por slot? A constante `EMBEDDING_LOAD_EXPECTED_RSS_MB = 1100` (constants.rs:359) foi calibrada para carregar o modelo ONNX POR PROCESSO, mas com daemon o modelo é compartilhado
- POR QUE mesmo calculando mais, o resultado continua 4? Porque `lock.rs:79` faz `max_concurrency.clamp(1, MAX_CONCURRENT_CLI_INSTANCES)` onde `MAX_CONCURRENT_CLI_INSTANCES = 4` é constante hardcoded que anula qualquer cálculo
### Evidência no Código — Quatro Lacunas Sobrepostas
- LACUNA 1 (fórmula com margem excessiva): `memory_guard.rs:60` aplica `resource_bound / 2` como margem, halvando o resultado
- Com 8 CPUs e 57 GB: `min(8, 57000/1100) = min(8, 51) = 8`, depois `8 / 2 = 4`
- O fator `0.5` é defensivo demais para cenários com daemon ativo (modelo compartilhado)
- LACUNA 2 (custo-por-slot superestimado): `constants.rs:359` define `EMBEDDING_LOAD_EXPECTED_RSS_MB = 1100`
- Calibrado em 2026-04-23 para `remember`, `recall`, `hybrid-search` que carregam modelo ONNX per-process
- Com daemon ativo, o modelo é carregado UMA vez; workers de CLI consomem centenas de MB, não 1.1 GB
- Workers de `enrich --mode claude-code` apenas fazem spawn de `claude -p` — custo marginal é ~200-500 MB
- O semáforo está medindo o recurso errado: a pressão real de RAM vem dos processos `claude` externos que o semáforo nem contabiliza
- LACUNA 3 (teto rígido hardcoded em lock.rs): `lock.rs:79` faz `max_concurrency.clamp(1, MAX_CONCURRENT_CLI_INSTANCES)` onde `MAX_CONCURRENT_CLI_INSTANCES = 4`
- Esta constante é INDEPENDENTE de qualquer cálculo dinâmico de memória ou CPU
- Mesmo que `calculate_safe_concurrency` retornasse 16, o lock.rs clamparia para 4
- Mesmo que o usuário passasse `--max-concurrency 999`, o efetivo seria 4
- LACUNA 4 (clamp silencioso sem override): `main.rs:223-233` loga "Reducing requested concurrency" apenas via tracing (stderr)
- NÃO emite em nível info nem no JSON
- Não existe env var de escape hatch como `SQLITE_GRAPHRAG_FORCE_MAX_CONCURRENCY`
- O usuário não recebe feedback sobre POR QUE seu `--max-concurrency 12` virou 4
- `daemon --ping --json` NÃO inclui `max_concurrency_configured` nem `max_concurrency_effective`
### Reprodução
- Verificar CPUs: `nproc` retorna 8 (ou `sysctl -n hw.ncpu` no macOS)
- Verificar memória: `memory_guard.rs:20` usa `sys.available_memory()` via sysinfo 0.32
- Iniciar daemon: `sqlite-graphrag daemon --max-concurrency 12`
- Tentar 5 workers: o 5o fica parado em `--wait-lock` até timeout
- Verificar lock files: `fd -g 'cli-slot-*' $(sqlite-graphrag config path 2>/dev/null || echo ~/.cache/sqlite-graphrag/)` — apenas 4 arquivos existem
- Rodar `sqlite-graphrag stats --json` enquanto 4 workers rodam: exit 75 ("all 4 concurrency slots occupied")
- O efetivo permanece 4 independente do `--max-concurrency` passado
### Solução Proposta — Quatro Correções Independentes
- Correção 1 (eliminar margem de 0.5 quando daemon ativo): Em `memory_guard.rs:60`, remover ou condicionar o `/ 2`
- Se daemon está ativo (modelo compartilhado), usar `resource_bound` sem halvar
- Se daemon está inativo (modelo carregado per-process), manter `/ 2`
- Verificar presença do daemon via `daemon --ping` antes de calcular
- Correção 2 (recalibrar custo-por-slot): Reduzir `EMBEDDING_LOAD_EXPECTED_RSS_MB` de 1100 para ~300 quando daemon ativo
- Tornar configurável via env `SQLITE_GRAPHRAG_MEM_PER_SLOT_MB` (default 256-512 MB com daemon, 1100 sem daemon)
- Documentar que o custo real de RAM do `enrich --mode claude-code` vem dos subprocessos `claude` externos
- Correção 3 (eliminar teto rígido hardcoded): Em `lock.rs:79`, substituir `clamp(1, MAX_CONCURRENT_CLI_INSTANCES)` por `clamp(1, 2 * cpu_count)`
- O teto dinâmico `2 * nCPUs` é seguro e escalável
- Remover `MAX_CONCURRENT_CLI_INSTANCES` como constante ou usá-la apenas como DEFAULT (não como teto)
- Correção 4 (honrar --max-concurrency explícito + observabilidade): Distinguir se `--max-concurrency` veio do default ou foi explícito pelo usuário
- Se explícito: usar exatamente o valor, clampado apenas a `[1, 2*nCPUs]` — sem redução por heurística de memória
- Se default: aplicar heurística de memória normalmente
- Adicionar env `SQLITE_GRAPHRAG_FORCE_MAX_CONCURRENCY=1` como escape hatch
- Emitir bloco JSON de decisão: `{ requested, effective, reason, available_mem_mb, per_slot_mb, ncpus, ceiling }`
- `daemon --ping --json` DEVE incluir `max_concurrency_configured` e `max_concurrency_effective`
### Benefícios
- Utilização real de ~16 slots em máquina de 8 CPUs com 64 GB (em vez de 4)
- `enrich --mode claude-code` com 8+ workers paralelos reduz tempo total em ~50-75%
- Comandos leves de gestão não competem por slots escassos
- Override explícito garante que operador experiente controla o paralelismo
- Transparência total: o JSON explica POR QUE o semáforo foi dimensionado daquela forma
- `daemon --ping` expõe configuração efetiva para diagnóstico
- Compatibilidade: default continua seguro para quem não configura nada
### Como Solucionar
- Passo 1: em `memory_guard.rs:48-63`, aceitar parâmetro `daemon_active: bool`
- Passo 2: se `daemon_active`, usar `resource_bound` sem dividir por 2 e usar custo-por-slot reduzido (~300 MB)
- Passo 3: se NÃO `daemon_active`, manter fórmula atual com `/ 2` e 1100 MB/slot
- Passo 4: em `constants.rs:331`, mudar `MAX_CONCURRENT_CLI_INSTANCES` de 4 para `2 * nCPUs` (calculado em runtime) ou remover como teto
- Passo 5: em `lock.rs:79`, substituir `clamp(1, MAX_CONCURRENT_CLI_INSTANCES)` por `clamp(1, max_concurrency)` passando o teto real
- Passo 6: em `main.rs:188`, distinguir se `--max-concurrency` veio do default via `cli.max_concurrency.is_some()`
- Passo 7: se explícito, bypassar heurística de memória, clampando apenas a `[1, 2*nCPUs]`
- Passo 8: adicionar env `SQLITE_GRAPHRAG_MEM_PER_SLOT_MB` com default 300 (daemon) ou 1100 (sem daemon)
- Passo 9: adicionar env `SQLITE_GRAPHRAG_FORCE_MAX_CONCURRENCY` como escape hatch
- Passo 10: em `main.rs:213-233`, emitir bloco JSON de decisão em nível info com campos: requested, effective, reason, available_mem_mb, per_slot_mb, ncpus, ceiling
- Passo 11: em `daemon --ping`, adicionar campos `max_concurrency_configured` e `max_concurrency_effective` na resposta JSON
- Passo 12: adicionar testes: 57 GB available + 8 CPUs + daemon ativo + `--max-concurrency 12` = efetivo 12; `--max-concurrency 999` clampado a `2*nCPUs` com reason="ceiling"
### Complexidade
- Correção 1 (margem condicional): BAIXA (~5 linhas em memory_guard.rs, 1 branch condicional)
- Correção 2 (custo-por-slot configurável): MÉDIA (~15 linhas, env var + lógica de resolução)
- Correção 3 (eliminar teto hardcoded): BAIXA (~3 linhas em lock.rs + remover constante)
- Correção 4 (override explícito + observabilidade): MÉDIA (~30 linhas, detecção de flag explícito + JSON de decisão + daemon ping)
- Testes: MÉDIA (~40 linhas, mock de vm_stats + cenários com/sem daemon)
- Documentação: BAIXA (atualizar CLAUDE.md e after_long_help do daemon)
### Arquivos Afetados
- src/memory_guard.rs:48-63 — condicionar margem `/ 2` e custo-por-slot à presença do daemon
- src/constants.rs:331 — mudar ou remover `MAX_CONCURRENT_CLI_INSTANCES = 4` como teto rígido
- src/constants.rs:359 — documentar que `EMBEDDING_LOAD_EXPECTED_RSS_MB = 1100` é para cenário sem daemon
- src/lock.rs:79 — substituir clamp hardcoded por teto dinâmico `2 * nCPUs`
- src/main.rs:188-233 — distinguir flag explícito vs default, emitir JSON de decisão
- src/commands/daemon.rs — adicionar campos `max_concurrency_configured` e `max_concurrency_effective` no ping


## G19 HIGH (CORRIGIDO v1.0.67) — enrich e ingest --mode claude-code processam chamadas LLM em série pura desperdiçando 75% do tempo em I/O wait de subprocessos
### Status: CORRIGIDO — flag --llm-parallelism adicionado ao enrich com thread pool via std::thread::scope

### Problema
- O comando `enrich --operation entity-descriptions --mode claude-code` processa 1 entidade por vez em loop serial
- Cada chamada `claude -p` (headless) leva ~12,5s por item: ~2s de cold-start + ~10s de inferência LLM
- Com 2.321 entidades sem descrição, o tempo total é ~8 horas em série
- O mesmo padrão serial existe em `enrich --operation memory-bindings`, `enrich --operation body-enrich` e `ingest --mode claude-code`
- O flag `--max-concurrency` controla slots CLI via flock (semáforo do G18), NÃO o paralelismo interno de chamadas LLM
- O usuário que passa `--max-concurrency 4` espera 4 chamadas `claude -p` paralelas, mas obtém 1 por vez
- A fila SQLite (`.enrich-queue.sqlite`) já usa `UPDATE...RETURNING` atômico para claim — design projetado para multi-worker, mas o código nunca spawna mais de 1 worker

### Consequências
- Pipeline `enrich -o entity-descriptions` com 2.321 itens leva ~8 horas em vez de ~2 horas (com 4 workers)
- Pipeline `enrich -o memory-bindings` com 1.000 memórias leva ~3,5 horas em vez de ~50 minutos
- Pipeline `ingest --mode claude-code` com 500 arquivos leva ~1,7 horas em vez de ~25 minutos
- A máquina fica ~90% idle durante o processamento: cada `claude -p` consome CPU apenas durante inferência (~2-3s), ficando ~10s em I/O wait de rede OAuth
- O processo pai (`sqlite-graphrag`) gasta a maior parte do tempo bloqueado em `child.wait_timeout()` — I/O bound puro
- O usuário precisa orquestrar manualmente N terminais com `sqlite-graphrag enrich --resume` para obter paralelismo — workaround frágil e não documentado
- Sessões longas de `enrich` ficam vulneráveis a interrupções: 8 horas de processamento serial versus 2 horas paralelas reduz a janela de exposição a falhas

### Causa Raiz — 5 Porquês
- POR QUE o enrich leva 8 horas para 2.321 itens? Porque processa 1 item por vez em loop serial
- POR QUE processa 1 por vez? Porque o loop `run()` em `enrich.rs:1158` faz dequeue+call_claude+persist sequencialmente, sem spawnar threads ou tasks concorrentes
- POR QUE não spawna workers paralelos? Porque foi implementado como cópia do padrão de `ingest_claude.rs:714` que também é serial
- POR QUE ingest_claude.rs é serial? Porque o design original priorizou simplicidade e segurança de acesso ao SQLite, ignorando que chamadas LLM são I/O bound e o gargalo é espera de rede, não CPU
- POR QUE a fila foi projetada com claim atômico se só há 1 worker? Porque o pattern `UPDATE...RETURNING` foi incluído antecipando multi-worker via `--resume` manual, mas a paralelização interna nunca foi implementada

### Evidência no Código — Três Lacunas Independentes
- LACUNA 1 (loop serial em enrich.rs): `enrich.rs:1158-1366` faz `loop { dequeue → call_claude → persist → emit_json }` bloqueando no `child.wait_timeout()` a cada item
- `call_claude()` em `enrich.rs:478-607` spawna `std::process::Command` síncrono e aguarda com `child.wait_timeout(timeout)` na linha 566
- ZERO uso de `std::thread`, `tokio::spawn`, `rayon::par_iter` ou qualquer primitiva de concorrência
- O único "paralelismo" possível é o workaround manual: múltiplos processos `sqlite-graphrag enrich --resume` competindo pela mesma fila
- LACUNA 2 (loop serial em ingest_claude.rs): `ingest_claude.rs:714` segue padrão idêntico — 1 arquivo por vez
- Mesmo pattern serial: `loop { dequeue → extract_with_claude → persist → emit_json }`
- LACUNA 3 (ausência de flag --llm-parallelism): Não existe flag CLI para controlar quantos subprocessos `claude -p` ou `codex exec` rodam em paralelo
- `--max-concurrency` (semáforo G18) limita invocações CLI, não chamadas LLM internas
- O usuário não tem como expressar "quero 4 claude -p simultâneos dentro de um único enrich"
- A documentação não explica a diferença entre concorrência de CLI slots e paralelismo de chamadas LLM

### Reprodução
- Iniciar enrich serial: `sqlite-graphrag enrich -o entity-descriptions --mode claude-code --json`
- Observar no NDJSON: cada item leva ~12,5s; itens processados sequencialmente (index 0, 1, 2...)
- Verificar processos: `procs claude` — apenas 1 processo `claude -p` ativo por vez
- Calcular ETA: `fend "2321 * 12.5 / 3600"` = ~8,1 horas
- Workaround manual: abrir 4 terminais, cada um com `sqlite-graphrag enrich -o entity-descriptions --mode claude-code --resume --json`
- Verificar claim atômico: `sqlite3 .enrich-queue.sqlite "SELECT status, COUNT(*) FROM queue GROUP BY status"` — mostra 1 processing, N-1 pending

### Solução Proposta — Três Correções Independentes
- Correção 1 (paralelismo interno com bounded thread pool): Adicionar flag `--llm-parallelism <N>` (default 1 para compatibilidade, recomendado 4-8)
- Spawnar N threads (ou tasks tokio), cada uma executando o loop dequeue-call-persist independente
- A fila SQLite com `UPDATE...RETURNING` já garante claim atômico sem race condition
- Cada thread mantém seu próprio `Connection` ao DB principal para persistência (SQLite WAL suporta múltiplos writers com retry)
- Usar `std::thread::scope` para paralelismo sem overhead de runtime async (chamadas LLM são `Command::spawn` síncrono)
- Alternativa: `tokio::task::spawn_blocking` com `Semaphore::new(N)` se o runtime async já estiver disponível
- Correção 2 (aplicar mesmo padrão a ingest_claude.rs): Extrair o loop paralelo em módulo compartilhado `src/commands/llm_runner.rs`
- `enrich.rs` e `ingest_claude.rs` reutilizam o mesmo pool de workers
- O módulo aceita um closure `Fn(item_key) -> Result<ItemResult>` como estratégia de processamento
- Correção 3 (flag --llm-parallelism na CLI com documentação): Adicionar flag ao `EnrichArgs` e `IngestClaudeArgs`
- Documentar no `after_long_help` que `--llm-parallelism` controla subprocessos LLM paralelos
- Documentar que `--max-concurrency` controla slots CLI (flock), não chamadas LLM
- Emitir no NDJSON de fase: `{"phase":"scan","llm_parallelism":4,"items_total":2321}`
- `daemon --ping` DEVE incluir campo `llm_workers_active` para observabilidade

### Benefícios
- Pipeline `enrich -o entity-descriptions` de 2.321 itens reduz de ~8 horas para ~2 horas (4 workers) ou ~1 hora (8 workers)
- Pipeline `ingest --mode claude-code` de 500 arquivos reduz de ~1,7 horas para ~25 minutos
- Utilização de CPU sobe de ~10% (1 worker I/O bound) para ~40-60% (4-8 workers com overlap de cold-start e inferência)
- Sem necessidade de orquestração manual em múltiplos terminais
- Flag explícito dá controle ao operador: `--llm-parallelism 1` para máquinas restritas, `8` para hardware potente
- Sessões mais curtas reduzem risco de interrupção e corrupção por timeout
- Compatibilidade: default `--llm-parallelism 1` mantém comportamento atual

### Como Solucionar
- Passo 1: criar módulo `src/commands/llm_runner.rs` com struct `LlmWorkerPool` que aceita N workers e um closure de processamento
- Passo 2: `LlmWorkerPool::run()` usa `std::thread::scope` para spawnar N threads; cada thread faz `loop { dequeue_from_queue → call_closure → persist → emit_json }`
- Passo 3: a fila SQLite (`.enrich-queue.sqlite`) já suporta claim atômico via `UPDATE...RETURNING`; nenhuma mudança necessária na tabela queue
- Passo 4: cada thread abre sua própria `Connection` ao DB principal (`graphrag.sqlite`) — WAL mode suporta concurrent writers com busy_timeout
- Passo 5: adicionar `--llm-parallelism <N>` ao `EnrichArgs` (default 1) com validação `clamp(1, 2*nCPUs)`
- Passo 6: em `enrich.rs:run()`, substituir o loop serial por `LlmWorkerPool::new(llm_parallelism).run(queue_conn, |item_key| { ... })`
- Passo 7: aplicar o mesmo refactor a `ingest_claude.rs` substituindo seu loop serial
- Passo 8: adicionar contadores atômicos (`AtomicUsize`) para `completed`, `failed`, `skipped`, `cost_total` compartilhados entre threads
- Passo 9: serializar emissão NDJSON via `Mutex<Stdout>` para evitar linhas intercaladas
- Passo 10: emitir `llm_parallelism` no PhaseEvent de scan para observabilidade
- Passo 11: documentar no `after_long_help` a diferença entre `--max-concurrency` (slots CLI) e `--llm-parallelism` (subprocessos LLM)
- Passo 12: adicionar testes: mock de `call_claude` com sleep de 100ms; 4 workers devem completar 8 itens em ~200ms (2 batches), não ~800ms (serial)

### Complexidade
- Correção 1 (thread pool com bounded workers): MÉDIA (~60 linhas em llm_runner.rs, thread::scope + dequeue loop)
- Correção 2 (refactor de enrich.rs e ingest_claude.rs): MÉDIA (~40 linhas cada, extrair closure + integrar LlmWorkerPool)
- Correção 3 (flag CLI + documentação): BAIXA (~15 linhas, Clap arg + PhaseEvent field + after_long_help)
- Testes: MÉDIA (~30 linhas, mock com sleep + assertion de tempo)
- Documentação: BAIXA (atualizar CLAUDE.md com --llm-parallelism e sua distinção de --max-concurrency)

### Arquivos Afetados
- src/commands/llm_runner.rs — NOVO módulo com `LlmWorkerPool` (bounded thread pool + dequeue loop)
- src/commands/enrich.rs:1158-1366 — substituir loop serial por `LlmWorkerPool::run()`
- src/commands/enrich.rs:203-289 — adicionar `--llm-parallelism` ao `EnrichArgs`
- src/commands/ingest_claude.rs:714-900 — substituir loop serial por `LlmWorkerPool::run()`
- src/commands/mod.rs — adicionar `pub mod llm_runner;`
- CLAUDE.md — documentar `--llm-parallelism` e sua diferença de `--max-concurrency`

### Relação com Outros Gaps
- G02 (duplicação enrich/ingest_claude): a criação de `llm_runner.rs` resolve G02 simultaneamente — o módulo compartilhado elimina a duplicação de `call_claude`, `parse_claude_output` e o loop de processamento
- G08 (remember single-shot): G08 trata da contention de N processos `remember` competindo por slots; G19 trata da serialização INTERNA de chamadas LLM dentro de um único processo
- G18 (semáforo preso em 4): G18 trata do teto rígido de slots CLI; G19 trata da falta de paralelismo DENTRO de cada slot — são complementares e independentes



## G20 MEDIUM (PARCIALMENTE CORRIGIDO v1.0.67) — 30 flags mode-específicas aceitas e silenciosamente descartadas por 4 comandos sem validação condicional nem feedback ao usuário
### Status: PARCIAL — hybrid_search e recall validam --max-hops/--min-weight; ingest e enrich com TODO(G20) para validação completa
### Problema
- Os comandos `ingest`, `enrich`, `hybrid-search` e `recall` aceitam flags condicionais sem validar se o modo/contexto ativo permite seu uso
- 30 flags são silenciosamente descartadas quando o modo ativo não as processa
- O parser Clap aceita todas as flags na fase de parsing, mas o runtime ignora as flags do modo inativo sem emitir erro, warning ou tracing
- O usuário acredita que TODAS as flags passadas foram processadas
- Nenhum dos 4 comandos afetados possui `conflicts_with`, `requires` ou validação pós-parse para flags mode-específicas
### Evidência no Código — Instâncias por Comando
- INSTÂNCIA 1 (ingest com 16 flags descartáveis): `ingest.rs:746-752` faz `if args.mode == IngestMode::ClaudeCode { return }` antes de ler flags NER/parallelism
- `ingest --mode claude-code --enable-ner --gliner-variant int8 --ingest-parallelism 8 --low-memory --max-rss-mb 4096` → 5 flags aceitas e descartadas
- `ingest --mode none --claude-binary /usr/bin/claude --claude-timeout 600 --max-cost-usd 10 --resume --retry-failed --keep-queue --rate-limit-wait 120` → 8 flags aceitas e descartadas
- `ingest --mode none --codex-binary /usr/bin/codex --codex-model o4-mini --codex-timeout 600` → 3 flags aceitas e descartadas
- `IngestArgs` struct (linhas 88-264) contém ZERO declarações `conflicts_with` para flags mode-específicas
- INSTÂNCIA 2 (enrich com 10 flags descartáveis): `EnrichArgs` struct (linhas 203-289) contém ZERO declarações `conflicts_with`
- `enrich -o entity-descriptions --min-output-chars 500 --max-output-chars 2000 --prompt-template foo.txt` → 3 flags body-enrich aceitas e descartadas
- `enrich -o memory-bindings --mode codex --claude-binary /usr/bin/claude --claude-timeout 600` → 3 flags claude aceitas e descartadas com modo codex
- `enrich -o entity-descriptions --mode claude-code --codex-binary /usr/bin/codex --codex-model o4-mini --codex-timeout 600` → 3 flags codex aceitas e descartadas com modo claude
- Flags body-enrich-only (4): `--min-output-chars`, `--max-output-chars`, `--preserve-check`, `--prompt-template`
- INSTÂNCIA 3 (hybrid-search com 2 flags descartáveis): `hybrid_search.rs:297` usa `--max-hops` e `--min-weight` APENAS quando `--with-graph` ativo
- `hybrid-search "query" --max-hops 5 --min-weight 0.1` → 2 flags aceitas e descartadas sem `--with-graph`
- INSTÂNCIA 4 (recall com 2 flags descartáveis): `recall.rs:189` usa `--max-hops` e `--min-weight` APENAS quando `--no-graph` está ausente
- `recall "query" --max-hops 5 --min-weight 0.1 --no-graph` → 2 flags aceitas e descartadas com `--no-graph` ativo
### Consequências
- O usuário passa `--claude-timeout 600` com `--mode none` e acredita que o timeout foi configurado — mas a flag foi descartada
- Pipelines automatizados de agentes LLM passam flags baseados na documentação sem receber feedback de que o contexto não as suporta
- Debug de problemas de performance é dificultado: o operador ajusta `--ingest-parallelism 8` com `--mode claude-code` sem saber que a flag nunca foi lida
- `--max-cost-usd 5.00` com `--mode none` dá falsa sensação de controle orçamentário
- `--max-hops 5` sem `--with-graph` dá falsa sensação de travessia profunda quando apenas busca vetorial pura foi executada
- Erro silencioso viola o princípio "prefira erro claro a comportamento silencioso" declarado no CLAUDE.md do projeto
- Operações parciais criam estado inconsistente sem feedback: o operador acredita que configurou 6 parâmetros, mas apenas 2 foram efetivamente aplicados
### Causa Raiz — 5 Porquês
- POR QUE flags são descartadas silenciosamente? Porque o runtime resolve o modo (linhas 746-752 em ingest.rs) e retorna antes de ler as flags do modo inativo
- POR QUE o runtime não valida? Porque ZERO validação pós-parse existe para detectar flags mode-específicas em modo incompatível
- POR QUE não existe validação pós-parse? Porque as structs `IngestArgs` e `EnrichArgs` declaram TODAS as flags como campos independentes sem `conflicts_with` ou `requires`
- POR QUE as flags não têm `conflicts_with`? Porque Clap `conflicts_with` não suporta condição "flag X só é válida quando flag Y tem valor Z" — seria necessário validação pós-parse via `CommandFactory::command().error()`
- POR QUE não usam validação pós-parse? Porque o pattern cresceu incrementalmente: cada modo novo adicionou flags na struct sem voltar a validar compatibilidade cruzada entre os modos existentes
### Inventário Completo — 30 Flags Afetadas
- `ingest` modo `none`/`gliner` (5 flags que são descartadas quando `--mode claude-code` ou `--mode codex`): `--enable-ner`, `--gliner-variant`, `--ingest-parallelism`, `--low-memory`, `--max-rss-mb`
- `ingest` modo `claude-code` (8 flags descartadas quando `--mode none`): `--claude-binary`, `--claude-model`, `--claude-timeout`, `--max-cost-usd`, `--resume`, `--retry-failed`, `--keep-queue`, `--rate-limit-wait`
- `ingest` modo `codex` (3 flags descartadas quando `--mode none` ou `--mode claude-code`): `--codex-binary`, `--codex-model`, `--codex-timeout`
- `enrich` modo `claude-code` (3 flags descartadas quando `--mode codex`): `--claude-binary`, `--claude-model`, `--claude-timeout`
- `enrich` modo `codex` (3 flags descartadas quando `--mode claude-code`): `--codex-binary`, `--codex-model`, `--codex-timeout`
- `enrich` operação `body-enrich` (4 flags descartadas com outra operação): `--min-output-chars`, `--max-output-chars`, `--preserve-check`, `--prompt-template`
- `hybrid-search` flags de grafo (2 flags descartadas sem `--with-graph`): `--max-hops`, `--min-weight`
- `recall` flags de grafo (2 flags descartadas com `--no-graph`): `--max-hops`, `--min-weight`
### Solução Proposta — Validação Pós-Parse com Exit 2
- ABORDAGEM: adicionar função `validate_mode_flags()` em cada comando, chamada ANTES da lógica de negócio
- Clap `conflicts_with` NÃO suporta condição "flag X só quando value_enum Y = Z" — precisa de validação pós-parse
- Usar `clap::CommandFactory::command().error(ErrorKind::ArgumentConflict, msg).exit()` para exit code 2 (usage error padrão Clap)
- Cada flag mode-específica que foi explicitamente passada pelo usuário (não é o default) E cujo modo não está ativo → emitir erro
- Detectar "flag explicitamente passada" via `args.contains_id("flag_name")` no `ArgMatches` ou via wrapper `Option<T>` com `None` como default
### Benefícios
- Erro claro impede que o usuário acredite que configuração foi aplicada quando não foi
- Agentes LLM automatizados recebem exit 2 imediato em vez de resultado silenciosamente incompleto
- Debug instantâneo: mensagem como "--claude-timeout requires --mode claude-code (active mode: none)" identifica a causa em 1 segundo
- Conformidade com princípio "prefira erro claro a comportamento silencioso" do CLAUDE.md
- Compatibilidade preservada: flags com valor default continuam aceitas sem erro — apenas flags explicitamente passadas pelo usuário em modo incompatível geram erro
### Como Solucionar
- Passo 1: em `ingest.rs`, criar função `validate_ingest_mode_flags(args: &IngestArgs, matches: &ArgMatches) -> Result<()>`
- Passo 2: se `mode == None`, verificar que nenhuma flag claude/codex/queue foi explicitamente passada via `matches.contains_id()`
- Passo 3: se `mode == ClaudeCode`, verificar que nenhuma flag NER/parallelism foi explicitamente passada
- Passo 4: se `mode == Codex`, verificar que nenhuma flag claude NER/parallelism foi explicitamente passada
- Passo 5: em `enrich.rs`, criar função `validate_enrich_flags(args: &EnrichArgs, matches: &ArgMatches) -> Result<()>`
- Passo 6: se `mode == ClaudeCode`, verificar que nenhuma flag codex foi passada e vice-versa
- Passo 7: se `operation != BodyEnrich`, verificar que nenhuma flag body-enrich-only foi passada
- Passo 8: em `hybrid_search.rs`, se `!args.with_graph` e (`matches.contains_id("max_hops")` ou `matches.contains_id("min_weight")`), emitir erro
- Passo 9: em `recall.rs`, se `args.no_graph` e (`matches.contains_id("max_hops")` ou `matches.contains_id("min_weight")`), emitir erro
- Passo 10: emitir erro via `AppError::Validation(format!("--{flag} requires --mode {mode} (active mode: {active})"))`
- Passo 11: testes unitários para cada combinação inválida verificando exit code 1 (validação)
- Passo 12: testes unitários para cada combinação VÁLIDA verificando que flags default não geram falso positivo
### Complexidade
- Validação em `ingest.rs`: MÉDIA (~40 linhas, 3 branches por modo, ~16 flags a verificar)
- Validação em `enrich.rs`: MÉDIA (~30 linhas, 2 branches por modo + 1 por operação, ~10 flags)
- Validação em `hybrid_search.rs`: BAIXA (~5 linhas, 1 branch, 2 flags)
- Validação em `recall.rs`: BAIXA (~5 linhas, 1 branch, 2 flags)
- Testes: MÉDIA (~60 linhas, combinações modo x flag)
- Total estimado: ~140 linhas de código novo
### Arquivos Afetados
- `src/commands/ingest.rs:746` — adicionar `validate_ingest_mode_flags()` antes de `run()`
- `src/commands/enrich.rs:984` — adicionar `validate_enrich_flags()` antes do loop principal
- `src/commands/hybrid_search.rs:297` — adicionar validação de `--max-hops`/`--min-weight` sem `--with-graph`
- `src/commands/recall.rs:189` — adicionar validação de `--max-hops`/`--min-weight` com `--no-graph`
### Relação com Outros Gaps
- G05 (Clap rejeita queries com hífens): G05 trata da confusão hífens-como-flags; G20 trata de flags ACEITAS mas silenciosamente ignoradas — são anti-patterns Clap complementares
- G06 (reclassify exige --new-type): G06 é um caso de flag OBRIGATÓRIA que deveria ser OPCIONAL; G20 é o inverso — flags OPCIONAIS que deveriam ser REJEITADAS em contexto incompatível




## G21 MEDIUM (PARCIALMENTE CORRIGIDO v1.0.67) — 7 instâncias de tracing::warn!/debug! com exit 0 mascaram descarte de argumentos do usuário como sucesso para chamadores automatizados
### Status: PARCIAL — instâncias 1,2,5 convertidas para rejeição com exit 1; instâncias 3,4,6 pendentes (requerem warnings JSON)
### Problema
- 7 instâncias nos comandos `remember`, `ingest`, `ingest_claude` e `merge_entities` aceitam flags contraditórias ou inválidas e descartam o argumento do usuário silenciosamente
- O descarte é sinalizado apenas via `tracing::warn!` ou `tracing::debug!` no stderr — NÃO no JSON do stdout
- O exit code permanece 0 (sucesso) em TODOS os 7 casos
- Chamadores automatizados (agentes LLM, pipelines CI, scripts) leem APENAS exit code e stdout JSON — NUNCA stderr
- O chamador acredita que TODOS os argumentos passados foram processados quando na verdade parte foi descartada
- Viola o princípio "prefira erro claro a comportamento silencioso" declarado no CLAUDE.md do projeto
- Viola a Rule of Repair de Eric Raymond: "When you must fail, fail noisily and as soon as possible"
### Diferença entre G20 e G21
- G20 documenta 30 flags aceitas e COMPLETAMENTE ignoradas pelo runtime sem NENHUM feedback (zero `tracing::warn!`, zero log)
- G21 documenta 7 instâncias onde EXISTE feedback via `tracing::warn!` ou `tracing::debug!` no stderr, MAS o exit code é 0
- G20 = silêncio total sem rastro; G21 = warning ineficaz que mente para o chamador via exit 0
- Ambos são instâncias do mesmo anti-pattern "Silent Argument Discard", mas com mecanismos e soluções diferentes
### Evidência no Código — 7 Instâncias Confirmadas
- INSTÂNCIA 1 (remember.rs:385-388): `--enable-ner` e `--skip-extraction` são contradizentes
- O handler emite `tracing::warn!("--enable-ner and --skip-extraction are contradictory; --enable-ner takes precedence")`
- `--enable-ner` vence silenciosamente; o chamador que passou `--skip-extraction` não recebe erro
- `sqlite-graphrag remember --name x --type note --description "y" --body "z" --enable-ner --skip-extraction` → exit 0
- INSTÂNCIA 2 (remember.rs:390-391): `--skip-extraction` é deprecado e não tem efeito
- O handler emite `tracing::warn!("--skip-extraction is deprecated and has no effect")`
- A flag é ACEITA pelo Clap, ACEITA pelo handler, e DESCARTADA silenciosamente com exit 0
- A documentação diz "deprecated since v1.0.45" mas o Clap não marca como `hide = true` nem emite erro
- INSTÂNCIA 3 (remember.rs:336-343): relationships acima do cap são truncadas silenciosamente
- O handler emite `tracing::warn!(count, cap, "truncating relationships to cap")`
- O chamador que enviou 50 relações pode receber confirmação de memória salva sem saber que apenas 30 foram persistidas
- O JSON de resposta NÃO inclui campo `relationships_truncated` explícito no nível top-level
- INSTÂNCIA 4 (remember.rs:518-529): body vazio com `--force-merge` sem `--clear-body` preserva body antigo
- O handler emite `tracing::debug!("GAP-08: empty body with --force-merge and no --clear-body; preserving existing body")`
- Nível `debug!` é AINDA MAIS invisível que `warn!` — requer `-vv` para aparecer
- O chamador que passou body vazio intencionalmente (para limpar) recebe exit 0 sem saber que o body antigo foi preservado
- INSTÂNCIA 5 (ingest.rs:314-321): `--low-memory` sobrescreve `--ingest-parallelism N>1`
- O handler emite `tracing::warn!("--ingest-parallelism overridden by --low-memory; using 1")`
- O chamador que passou `--ingest-parallelism 8 --low-memory` recebe exit 0 com parallelism=1
- A flag `--ingest-parallelism 8` foi aceita e descartada sem erro
- Duplicado em `ingest.rs:986-989` com a mesma lógica para `--enable-ner` + `--skip-extraction`
- INSTÂNCIA 6 (ingest_claude.rs:1205-1207): `--max-cost-usd` ignorado quando OAuth detectado
- O handler emite `tracing::debug!("--max-cost-usd ignored: OAuth subscription detected")`
- O chamador que passou `--max-cost-usd 5.00` para controlar gastos recebe exit 0 sem saber que o budget NÃO está ativo
- Nível `debug!` requer `-vv` — o pipeline automatizado NUNCA vê esta mensagem
- INSTÂNCIA 7 (merge_entities.rs:82-85): source == target em merge é skipado silenciosamente
- O handler faz `if name == &args.into { continue; }` sem `tracing::warn!` nem log
- `merge-entities --names "a,b,a" --into a` → "a" é skipado silenciosamente da lista de fontes
- ZERO feedback no JSON, ZERO log, exit 0 se restam fontes válidas
### Consequências
- Agentes LLM automatizados leem exit 0 e stdout JSON — descarte de argumentos é INVISÍVEL para eles
- O chamador que passa `--max-cost-usd 5.00` com OAuth acredita que tem controle orçamentário ativo
- O chamador que passa `--skip-extraction` acredita que NER está desabilitado quando na verdade o flag não tem efeito
- O chamador que passa `--ingest-parallelism 8 --low-memory` acredita que 8 workers estão ativos
- O chamador que envia 50 relationships acredita que todas foram persistidas
- Debug de pipelines é dificultado: o operador precisa saber que deve ativar `-vv` E ler stderr para descobrir que flags foram descartadas
- Operações de merge com entidade source == target passam silenciosamente sem feedback
- O pattern `tracing::warn! + exit 0` cria uma "zona cinza" entre erro e sucesso que não existe no contrato JSON
### Causa Raiz — 5 Porquês
- POR QUE o descarte usa `tracing::warn!` em vez de erro? Porque o handler trata a contradição como "aviso informativo" em vez de "rejeição obrigatória"
- POR QUE o handler não rejeita? Porque o design prioriza "degradação graciosa" (graceful degradation) sobre "falha ruidosa" (fail fast)
- POR QUE o design escolheu degradação graciosa? Porque a lógica foi escrita antes de agentes LLM automatizados serem o caso de uso principal
- POR QUE agentes automatizados não veem o warning? Porque `tracing::warn!` vai para stderr e agentes leem APENAS exit code + stdout JSON
- POR QUE o contrato JSON não inclui warnings? Porque o schema de resposta (JSON) foi desenhado com campos de sucesso apenas, sem campo `warnings[]` para sinalizar descarte parcial
### Solução Proposta — Dois Caminhos Complementares
- CAMINHO A (rejeição estrita): converter `tracing::warn! + exit 0` em `return Err(AppError::Validation(msg))`
- ADEQUADO para instâncias 1, 2, 5: flags contraditórias ou deprecadas DEVEM ser rejeitadas
- ADEQUADO para instância 7: source == target DEVE ser rejeitado em vez de skipado
- Exit code 1 (validação) sinaliza ao chamador que a operação foi recusada
- CAMINHO B (warning no JSON): adicionar campo `warnings: Vec<String>` ao schema de resposta JSON
- ADEQUADO para instâncias 3, 4, 6: o descarte é uma degradação legítima que NÃO deve impedir a operação
- Truncamento de relationships (3) é uma limitação de design, não erro do chamador
- Preservação de body (4) é proteção contra destruição acidental
- Ignorar budget com OAuth (6) é limitação da plataforma, não erro do chamador
- O campo `warnings[]` no stdout JSON é visível para agentes automatizados
### Benefícios
- Instâncias rejeitadas (caminho A): o chamador recebe exit 1 imediato com mensagem clara sobre a contradição
- Instâncias com warning no JSON (caminho B): o chamador pode processar `warnings[]` programaticamente
- Agentes LLM automatizados passam a detectar descarte parcial via JSON em vez de depender de stderr
- Debug instantâneo: não é mais necessário ativar `-vv` e ler stderr para descobrir que flags foram descartadas
- Conformidade com Rule of Repair: contradições falham ruidosamente; degradações legítimas aparecem no JSON
- O contrato JSON evolui de "sucesso binário" para "sucesso com avisos" — modelo mais expressivo
### Como Solucionar
- Passo 1: definir enum `WarningKind { DeprecatedFlag, ConflictingFlags, DataTruncated, FeatureIgnored }`
- Passo 2: adicionar campo `warnings: Vec<Warning>` ao schema de resposta JSON para `remember`, `ingest`, `merge-entities`
- Passo 3: em `remember.rs:385-388`, converter `tracing::warn!` de `--enable-ner` + `--skip-extraction` para `return Err(AppError::Validation("--enable-ner and --skip-extraction are mutually exclusive"))`
- Passo 4: em `remember.rs:390-391`, converter `tracing::warn!` de `--skip-extraction` deprecado para `return Err(AppError::Validation("--skip-extraction is deprecated since v1.0.45; remove this flag"))`
- Passo 5: em `remember.rs:336-343`, manter truncamento mas adicionar `warnings.push(Warning::DataTruncated(...))` ao JSON de resposta
- Passo 6: em `remember.rs:518-529`, manter preservação de body mas adicionar `warnings.push(Warning::FeatureIgnored(...))` ao JSON
- Passo 7: em `ingest.rs:314-321`, converter `tracing::warn!` de `--low-memory` + `--ingest-parallelism` para `return Err(AppError::Validation("--ingest-parallelism N>1 conflicts with --low-memory"))`
- Passo 8: em `ingest_claude.rs:1205-1207`, manter comportamento com OAuth mas adicionar `warnings.push(Warning::FeatureIgnored("--max-cost-usd ignored: OAuth subscription detected"))` ao JSON de resposta
- Passo 9: em `merge_entities.rs:82-85`, converter skip silencioso em `return Err(AppError::Validation("source entity cannot equal target entity"))` quando source == target E é a ÚNICA fonte
- Passo 10: duplicar os passos 3 e 4 em `ingest.rs:986-992` que tem a mesma lógica de NER
- Passo 11: testes unitários para CADA instância verificando exit code 1 (caminho A) ou presença de `warnings[]` no JSON (caminho B)
### Complexidade
- Enum `WarningKind` + struct `Warning`: BAIXA (~15 linhas em `errors.rs` ou `output.rs`)
- Campo `warnings` no schema JSON: MÉDIA (~20 linhas por comando, 3 comandos = ~60 linhas)
- Conversão de 4 instâncias para `AppError::Validation`: BAIXA (~4 linhas por instância = ~16 linhas)
- Adição de `warnings.push()` em 3 instâncias: BAIXA (~3 linhas por instância = ~9 linhas)
- Testes: MÉDIA (~50 linhas, 7 instâncias x cenários válido + inválido)
- Total estimado: ~150 linhas de código novo
### Arquivos Afetados
- `src/commands/remember.rs:385-391` — converter warn de NER flags contradizentes em rejeição
- `src/commands/remember.rs:336-343` — adicionar `warnings[]` no JSON para truncamento
- `src/commands/remember.rs:518-529` — adicionar `warnings[]` no JSON para preservação de body
- `src/commands/ingest.rs:314-321` — converter warn de `--low-memory` override em rejeição
- `src/commands/ingest.rs:986-992` — converter warn de NER flags contradizentes em rejeição
- `src/commands/ingest_claude.rs:1205-1207` — adicionar `warnings[]` no JSON para OAuth budget
- `src/commands/merge_entities.rs:82-85` — converter skip silencioso em rejeição ou warning
- `src/errors.rs` ou `src/output.rs` — definir `WarningKind` e `Warning`
### Relação com Outros Gaps
- G20 (30 flags silenciosamente descartadas): G20 trata de flags COMPLETAMENTE ignoradas sem nenhum feedback; G21 trata de flags onde EXISTE feedback via `tracing::warn!` mas o exit code mente para o chamador — são duas faces do mesmo anti-pattern "Silent Argument Discard"
- G05 (Clap rejeita queries com hífens): G05 é rejeição EXCESSIVA pelo Clap; G21 é rejeição INSUFICIENTE pelo runtime — são anti-patterns opostos na camada de validação
- G08 (body vazio com --force-merge): G21 instância 4 É a documentação comportamental do G08 — a preservação silenciosa de body é exatamente o que G08 descreve; resolver G21 passo 6 complementa a solução do G08




## G22 LOW (CORRIGIDO v1.0.67) — Comando `read` não inclui contexto de grafo (entidades e relacionamentos) no JSON de resposta, forçando 3 chamadas sequenciais para montar contexto completo de uma memória
### Status: CORRIGIDO — flag --with-graph adicionado ao read com entities e relationships no JSON
### Problema
- O comando `read --name <nome> --json` retorna 18 campos (body, description, timestamps, version, metadata) mas ZERO informação do grafo de conhecimento
- Para obter o contexto completo de uma memória, o chamador precisa orquestrar 3 comandos sequenciais:
- Chamada 1: `read --name <nome> --json` → body, description, timestamps
- Chamada 2: `memory-entities --name <nome> --json` → entidades vinculadas (entity_id, name, entity_type)
- Chamada 3: `related <nome> --hops 1 --json` → memórias relacionadas via grafo (name, hop_distance, relation)
- O JSON do `read` não contém campo `entities[]` nem `relationships[]` — acessar esses campos causa erro no pipeline jaq
- `jaq '.entities[]'` no output de `read` retorna `cannot use null as iterable` com exit code 5
- O chamador NÃO tem como saber, a partir do `read`, se a memória possui entidades ou relacionamentos
- O `read` é o ÚNICO comando de consulta individual que NÃO oferece flag opt-in para enriquecer a resposta com contexto de grafo
### Evidência no Código
- `src/commands/read.rs:42-69`: struct `ReadResponse` com 18 campos, ZERO campos de grafo
- `src/commands/read.rs:86-118`: handler faz UMA query SQL (`memories::read_by_name`) + 1 query de versão — ZERO JOINs com `memory_entities` ou `relationships`
- `src/commands/memory_entities.rs:132-148`: query para entidades já existe isolada — `SELECT e.id, e.name, e.type FROM memory_entities me JOIN entities e ON e.id = me.entity_id WHERE me.memory_id = ?1`
- `src/commands/hybrid_search.rs`: implementa flag `--with-graph` (campo `with_n`) que adiciona `graph_matches[]` quando ativada — precedente de flag opt-in para enriquecimento
- `src/commands/deep_research.rs`: implementa flag `--with-bodies` (campo `with_n`) que inclui corpos completos nos resultados — precedente de flag opt-in para enriquecimento
- `deep-research` retorna campo `graph_context` com `entities[]` e `relationships[]` nativamente — o `read` NÃO
### Inconsistência no Contrato JSON
- `deep-research` retorna `graph_context` com `entities[{name, entity_type, degree}]` e `relationships[{from, to, relation, weight}]`
- `hybrid-search --with-graph` retorna `graph_matches[]` com memórias descobertas via travessia de grafo
- `read` retorna ZERO contexto de grafo — é o único comando de consulta sem acesso ao grafo
- `list` também retorna ZERO contexto de grafo por item — mas `list` é enumeração, não consulta individual
- O chamador que usa `read` após `hybrid-search` ou `deep-research` PERDE o contexto de grafo que a busca tinha
### Consequências
- Agentes LLM automatizados que precisam de contexto completo devem orquestrar 3 chamadas sequenciais em vez de 1
- Pipelines jaq falham com erro críptico `cannot use null as iterable` ao tentar acessar `.entities[]` no output de `read`
- O padrão de 3 chamadas precisa ser reimplementado por CADA chamador — não existe abstração no CLI
- O chamador que faz `read` após `deep-research` precisa re-executar queries de grafo que o sistema já conhece
- Documentação do CLAUDE.md descreve o pipeline de 3 camadas canônico (hybrid-search → read → related) como workflow ESPERADO — naturalizando a complexidade em vez de simplificar o contrato
- Descoberta de contexto é impossível: o `read` não indica se a memória possui 0 ou 50 entidades vinculadas
### Causa Raiz — 5 Porquês
- POR QUE o `read` não inclui entidades e relacionamentos? Porque o handler foi escrito como lookup direto da tabela `memories` sem JOINs com tabelas de grafo
- POR QUE o handler não faz JOINs? Porque `memory_entities` e `relationships` foram adicionadas ao schema DEPOIS do `read` — o handler nunca foi atualizado para incorporar os novos dados
- POR QUE nunca foi atualizado? Porque o pipeline de 3 camadas canônico (hybrid-search → read → related) foi documentado como a forma "correta" de obter contexto completo, mascarando a ausência de uma solução integrada
- POR QUE o pipeline de 3 camadas foi aceito como normal? Porque a prioridade foi adicionar novos comandos (`memory-entities`, `related`, `deep-research`) em vez de enriquecer comandos existentes
- POR QUE novos comandos foram priorizados? Porque cada comando novo resolve um caso de uso específico (listar entidades, travessia multi-hop) enquanto enriquecer o `read` exige alteração do contrato JSON existente com risco de breaking change
### Solução Proposta — Flag `--with-graph` Opt-In no `read`
- ABORDAGEM: adicionar flag `--with-graph` ao `read` que, quando ativa, inclui `entities[]` e `relationships[]` na resposta
- SEM `--with-graph`: comportamento idêntico ao atual — ZERO breaking change
- COM `--with-graph`: adiciona 2 campos ao JSON de resposta
- Campo `entities: [{entity_id, name, entity_type}]` — reutiliza query de `memory-entities`
- Campo `relationships: [{source, target, relation, weight, direction}]` — reutiliza queries de travessia do grafo
- PRECEDENTE: `hybrid-search --with-graph` e `deep-research --with-bodies` já usam exatamente este padrão
### Benefícios
- Chamador obtém contexto completo em 1 chamada em vez de 3
- Pipelines jaq funcionam com `read --with-graph | jaq '.entities[]'` sem erro
- Agentes LLM reduzem de 3 roundtrips para 1, simplificando orquestração
- Consistência com `hybrid-search --with-graph` e `deep-research --with-bodies`
- ZERO breaking change — flag é opt-in com default off
- A query SQL adicional (JOIN com `memory_entities` e busca de relationships) adiciona ~2ms ao read com daemon ativo
### Como Solucionar
- Passo 1: adicionar campo `with_graph: bool` ao `ReadArgs` com `#[arg(long, help = "Include entities and relationships in response")]`
- Passo 2: definir struct `ReadGraphContext` com campos `entities: Vec<EntityBinding>` e `relationships: Vec<RelationshipBinding>`
- Passo 3: reutilizar tipo `EntityBinding` de `memory_entities.rs` (entity_id, name, entity_type)
- Passo 4: definir `RelationshipBinding` com campos `source`, `target`, `relation`, `weight`, `direction`
- Passo 5: em `read.rs:86-118`, após obter `memory_id`, executar query de entidades via JOIN com `memory_entities` (mesma query de `memory_entities.rs:132-148`)
- Passo 6: executar query de relacionamentos via `relationships` table filtrando por entidades vinculadas à memória
- Passo 7: adicionar campos `entities` e `relationships` ao `ReadResponse` com `#[serde(skip_serializing_if = "Option::is_none")]`
- Passo 8: quando `--with-graph` é false, campos são `None` e não aparecem no JSON — ZERO breaking change
- Passo 9: testes unitários verificando que sem `--with-graph` o JSON não contém `entities` nem `relationships`
- Passo 10: testes unitários verificando que com `--with-graph` o JSON contém `entities[]` e `relationships[]` corretos
### Complexidade
- Flag `--with-graph` no `ReadArgs`: BAIXA (~3 linhas)
- Structs `ReadGraphContext`, `EntityBinding`, `RelationshipBinding`: BAIXA (~20 linhas, reutiliza tipos existentes)
- Query de entidades via JOIN: BAIXA (~10 linhas, reutiliza SQL de `memory_entities.rs`)
- Query de relacionamentos: BAIXA (~15 linhas, SQL similar ao `graph traverse`)
- Campos condicionais em `ReadResponse` com `skip_serializing_if`: BAIXA (~5 linhas)
- Testes: BAIXA (~30 linhas, 2 cenários: com e sem --with-graph)
- Total estimado: ~83 linhas de código novo
### Arquivos Afetados
- `src/commands/read.rs:19-39` — adicionar `with_graph: bool` ao `ReadArgs`
- `src/commands/read.rs:42-69` — adicionar `entities: Option<Vec<EntityBinding>>` e `relationships: Option<Vec<RelationshipBinding>>` ao `ReadResponse`
- `src/commands/read.rs:86-118` — adicionar queries condicionais de entidades e relacionamentos quando `--with-graph` é true
- `src/commands/memory_entities.rs:132-148` — extrair query SQL como função reutilizável (ou copiar)
### Relação com Outros Gaps
- G19 (enrich/ingest LLM serial): G19 trata de performance de pipeline LLM; G22 trata de ergonomia de API para consulta de contexto — são complementares quando o agente precisa ler memórias enriquecidas
- G20 (30 flags silenciosamente descartadas): G20 e G22 são ambos problemas de contrato CLI — G20 é sobre flags ACEITAS e ignoradas; G22 é sobre dados EXISTENTES mas não expostos
- G21 (tracing::warn! com exit 0): G21 trata de feedback ENGANOSO; G22 trata de dados AUSENTES — ambos impactam agentes automatizados que dependem do JSON




## G23 LOW (CORRIGIDO v1.0.67) — Inconsistência de naming nos campos JSON entre comandos: `results[]` vs domínio semântico, `source_entity`/`target_entity` vs `from`/`to` vs `from_name`/`to_name`, e `weight` (saída) vs `strength` (entrada)
### Status: CORRIGIDO — aliases related_memories, from/to em related.rs; weight como alias de strength em entities.rs:41
### Problema
- O contrato JSON da CLI usa nomes DIFERENTES para o MESMO conceito semântico em comandos diferentes
- O chamador que aprende o campo de um comando e tenta reutilizar no outro recebe `cannot use null as iterable` (exit 5 do jaq)
- O erro da sessão: `sqlite-graphrag related --json | jaq '.relationships[]'` falha porque o campo chama `results[]`, não `relationships[]`
- O chamador naturalmente espera `.relationships[]` em um comando chamado `related` que retorna dados de relacionamentos
- A inconsistência afeta 3 eixos independentes: container de coleção, endpoints de aresta e peso de relação
### Eixo 1 — Container de Coleção (nome do array principal no JSON)
- `related` → `results[]` — array genérico para dados que são especificamente memórias relacionadas via grafo
- `recall` → `results[]`, `direct_matches[]`, `graph_matches[]` — 3 arrays com naming semântico para matches diretos e via grafo
- `hybrid-search` → `results[]`, `graph_matches[]` — 2 arrays com naming semântico
- `deep-research` → `results[]`, `evidence_chains[]`, `graph_context.entities[]`, `graph_context.relationships[]` — 4 containers semânticos
- `list` → `items[]` E `memories[]` (alias semântico adicionado na v1.0.66)
- `graph --format json` → `nodes[]` E `entities[]` (alias semântico adicionado na v1.0.66)
- `graph entities` → `entities[]` (naming semântico desde a origem)
- `graph traverse` → `hops[]` (naming semântico desde a origem)
- PADRÃO: v1.0.66 adicionou aliases semânticos em `list` e `graph`, mas `related` ficou de fora sem alias
### Eixo 2 — Endpoints de Aresta (nomes dos campos source/target no JSON de relação)
- `related` → `source_entity`, `target_entity` (com sufixo `_entity`)
- `deep-research` graph_context → `from`, `to` (sem sufixo)
- `deep-research` evidence_chains → `from`, `to` (sem sufixo)
- `graph --format json` edges → `from`, `to` (sem sufixo)
- `graph traverse` hops → NÃO tem campos de endpoint, usa `entity` + `direction`
- `link` → `from`, `to` (sem sufixo)
- `unlink` → `from_name`, `to_name` (com sufixo `_name`)
- `reclassify-relation` → `from_relation`, `to_relation` (com sufixo `_relation`)
- 4 variantes para o MESMO conceito: `from`/`to`, `source_entity`/`target_entity`, `from_name`/`to_name`, `from_relation`/`to_relation`
### Eixo 3 — Peso de Relação (nome do campo de intensidade)
- SAÍDA (JSON de todos os comandos): `weight` (float)
- ENTRADA (`--graph-stdin` do `remember`): `strength` (float entre 0.0 e 1.0)
- O MESMO valor semântico tem nomes DIFERENTES conforme a direção do fluxo (escrita vs leitura)
- O chamador que copia `weight` do output de `related` para o input de `remember --graph-stdin` precisa renomear o campo para `strength`
### Evidência no Código
- `src/commands/related.rs:74` — `results: Vec<RelatedMemory>` sem alias semântico
- `src/commands/related.rs:87-88` — `source_entity: Option<String>`, `target_entity: Option<String>`
- `src/commands/deep_research.rs:209-213` — `GraphContextRel` usa `from: String`, `to: String`
- `src/commands/graph_export.rs:199-203` — `EdgeOut` usa `from: String`, `to: String`
- `src/commands/link.rs:84-85` — `LinkResponse` usa `from: String`, `to: String`
- `src/commands/unlink.rs:54-55` — `UnlinkResponse` usa `from_name: String`, `to_name: String`
- `src/commands/list.rs:88-89` — `items` E `memories` como alias (adicionado v1.0.66)
- `src/commands/graph_export.rs:208-209` — `nodes` E `entities` como alias (adicionado v1.0.66)
- `src/commands/recall.rs` — `RecallResponse` com `direct_matches`, `graph_matches`, `results` (3 arrays separados)
- `src/commands/remember.rs` — aceita `strength` no `--graph-stdin` mas `related` retorna `weight`
### Tabela Consolidada de Inconsistências
- Comando `related`: container `results[]`, endpoints `source_entity`/`target_entity`, peso `weight`
- Comando `deep-research` graph_context: container `relationships[]`, endpoints `from`/`to`, peso `weight`
- Comando `graph --format json`: container `edges[]`, endpoints `from`/`to`, peso `weight`
- Comando `link`: sem container, endpoints `from`/`to`, peso `weight`
- Comando `unlink`: sem container, endpoints `from_name`/`to_name`, sem peso
- Comando `reclassify-relation`: sem container, endpoints `from_relation`/`to_relation`, sem peso
- Comando `list`: container `items[]`/`memories[]` (alias), sem endpoints, sem peso
- Comando `graph entities`: container `entities[]`, sem endpoints, sem peso
- Comando `graph traverse`: container `hops[]`, endpoint único `entity`, peso `weight`
- Entrada `--graph-stdin`: sem container, endpoints `source`/`target` (ou `from`/`to` como alias), peso `strength`
### Consequências
- Chamadores automatizados (agentes LLM) falham com `cannot use null as iterable` ao usar `.relationships[]` em `related`
- O campo correto `.results[]` é contra-intuitivo para um comando que retorna dados de relacionamentos do grafo
- Pipelines jaq que funcionam com `deep-research` (`jaq '.graph_context.relationships[] | {from, to}'`) falham com `related` que usa `source_entity`/`target_entity`
- O chamador que constrói `--graph-stdin` a partir do output de `related` precisa transformar `weight` → `strength` E `source_entity`/`target_entity` → `source`/`target`
- A ausência de alias semântico em `related` (como `memories[]` em `list`) viola o padrão estabelecido na v1.0.66
- Cada novo comando que o chamador aprende exige memorizar uma nova combinação de nomes para os mesmos conceitos
- Documentação do CLAUDE.md precisa listar campos por comando porque não existe naming previsível
- O Principle of Least Surprise (Eric Raymond) é violado: o chamador que sabe o campo em um comando não pode inferir o campo em outro
### Causa Raiz — 5 Porquês
- POR QUE `related` usa `source_entity`/`target_entity` em vez de `from`/`to`? Porque o `RelatedMemory` foi desenhado para explicitar que os endpoints são ENTIDADES do grafo, não memórias — adicionou sufixo `_entity` para diferenciar
- POR QUE `deep-research` e `graph` usam `from`/`to` sem sufixo? Porque foram escritos depois, quando o contexto (graph_context, edges) já implica que são entidades — o sufixo era redundante
- POR QUE `unlink` usa `from_name`/`to_name`? Porque `unlink` opera sobre NOMES de entidade (strings) enquanto internamente usa IDs — o sufixo `_name` explicita que é o nome resolúvel
- POR QUE não há alias semântico em `related` como em `list`? Porque os aliases de v1.0.66 foram adicionados a `list` e `graph` por demanda de retrocompatibilidade com chamadores que esperavam `memories[]` e `entities[]`, mas nenhum chamador reportou a confusão em `related` até agora
- POR QUE `strength` na entrada e `weight` na saída? Porque `--graph-stdin` foi desenhado com vocabulário de CONSTRUÇÃO de grafo (`strength` expressa intenção do autor) enquanto a saída usa vocabulário de CONSULTA de grafo (`weight` expressa o valor persistido) — são o mesmo float mas com perspectivas semânticas diferentes
### Solução Proposta — Normalização Progressiva com Aliases de Retrocompatibilidade
- ABORDAGEM: adicionar aliases semânticos e normalizar naming gradualmente SEM breaking changes
- FASE 1 (aliases de container): adicionar campo `related_memories[]` como alias de `results[]` em `RelatedResponse` (mesmo padrão de `list` com `items[]`/`memories[]`)
- FASE 2 (normalização de endpoints): adicionar campos `from`/`to` como aliases de `source_entity`/`target_entity` em `RelatedMemory` com `#[serde(skip_serializing_if)]` — campos antigos permanecem
- FASE 3 (normalização de peso na entrada): aceitar `weight` como alias de `strength` no `--graph-stdin` do `remember`
- FASE 4 (documentação): atualizar CLAUDE.md com tabela de aliases canônicos por comando
- CADA fase é independente e pode ser entregue em releases separadas
- ZERO breaking change em TODAS as fases — campos antigos permanecem funcionando
### Benefícios
- Chamadores podem usar `.related_memories[]` em `related` — intuitivo e consistente com `memories[]` de `list`
- Pipelines jaq portáveis entre comandos: `.from`/`.to` funciona em `deep-research`, `graph`, `link` E `related`
- Roundtrip `related` → `remember --graph-stdin` funciona sem renomear campos (`weight` aceito como alias de `strength`)
- Redução da curva de aprendizado: 1 nome por conceito em vez de 4 variantes
- Consistência com Principle of Least Surprise: aprender um comando ensina todos
- v1.0.66 já estabeleceu o padrão de aliases — seguir é alinhamento com decisão arquitetural existente
### Como Solucionar
- Passo 1: em `related.rs:68-76`, adicionar campo `related_memories: Vec<RelatedMemory>` ao `RelatedResponse` como clone de `results` (mesmo padrão de `list.rs:88-89`)
- Passo 2: em `related.rs:78-91`, adicionar campos `from: Option<String>` e `to: Option<String>` ao `RelatedMemory` com `#[serde(skip_serializing_if = "Option::is_none")]`, populados com mesmos valores de `source_entity`/`target_entity`
- Passo 3: em `remember.rs` handler de `--graph-stdin`, aceitar campo `weight` como alias de `strength` no JSON de relacionamentos (fallback: `strength.or(weight)`)
- Passo 4: testes unitários verificando que `related_memories[]` contém os mesmos items que `results[]`
- Passo 5: testes unitários verificando que `from`/`to` contêm mesmos valores que `source_entity`/`target_entity`
- Passo 6: testes unitários verificando que `weight` é aceito como alias de `strength` em `--graph-stdin`
- Passo 7: atualizar CLAUDE.md seção "Campos Críticos por Comando" adicionando os aliases
### Complexidade
- Alias `related_memories[]` em `RelatedResponse`: BAIXA (~3 linhas, clone do vec `results`)
- Campos `from`/`to` em `RelatedMemory`: BAIXA (~6 linhas, 2 campos + população)
- Alias `weight` no parser de `--graph-stdin`: BAIXA (~5 linhas, fallback no deserialize)
- Testes: BAIXA (~25 linhas, 3 cenários)
- Documentação: BAIXA (~10 linhas no CLAUDE.md)
- Total estimado: ~49 linhas de código novo
### Arquivos Afetados
- `src/commands/related.rs:68-76` — adicionar `related_memories` como alias de `results`
- `src/commands/related.rs:78-91` — adicionar `from`/`to` como aliases de `source_entity`/`target_entity`
- `src/commands/remember.rs` — aceitar `weight` como alias de `strength` no `--graph-stdin`
- `CLAUDE.md` — seção "Campos Críticos por Comando" com aliases documentados
### Relação com Outros Gaps
- G20 (30 flags silenciosamente descartadas): G20 trata de flags CLI ignoradas; G23 trata de campos JSON inconsistentes — ambos são problemas de contrato que impactam chamadores automatizados
- G21 (tracing::warn! com exit 0): G21 trata de feedback enganoso via exit code; G23 trata de naming enganoso via campo JSON — ambos violam Principle of Least Surprise
- G22 (read sem contexto de grafo): G22 propõe adicionar `entities[]` e `relationships[]` ao `read`; G23 deve garantir que esses novos campos sigam o naming normalizado (`from`/`to` e não `source_entity`/`target_entity`)




## G24 LOW (CORRIGIDO v1.0.67) — Duplicatas de entidades por normalização de caixa: nomes com maiúsculas e espaços coexistem com versões em kebab-case, dividindo relacionamentos do mesmo referente
### Status: CORRIGIDO — health.rs detecta non_normalized_count e emite normalization_warning; normalize-entities resolve
### Problema
- Entidades com maiúsculas e espaços coexistem com versões normalizadas em kebab-case no mesmo namespace
- "Danilo Aguiar Teixeira" e "danilo-aguiar-teixeira" são linhas DISTINTAS na tabela `entities` com IDs diferentes
- "Danilo Teixeira" e "danilo-teixeira" são entidades separadas que representam o MESMO referente
- "Carteira BTC Danilo" e "carteira-btc-danilo" dividem relacionamentos entre dois nós em vez de um
- Essas são duplicatas EXATAS separadas APENAS por formatação (maiúsculas, espaços, underscores)
- O UNIQUE constraint `(namespace, name)` trata "Foo Bar" e "foo-bar" como entidades distintas
- Cada variante acumula seus próprios relacionamentos de forma independente — o grafo vê dois nós desconectados
- O comando `normalize-entities` resolve e mescla isto automaticamente, MAS não é executado automaticamente
### Evidência no Código
- `src/storage/entities.rs:83-112`: `upsert_entity` normaliza para kebab-case DESDE v1.0.65 via `normalize_entity_name` na linha 88
- `src/storage/entities.rs:97-104`: INSERT usa `normalized_name` — TODAS entidades criadas em v1.0.65+ são normalizadas
- v1.0.64 (`git show a46b03f:src/storage/entities.rs:82-99`): `upsert_entity` usava `e.name` DIRETO na linha 91, SEM normalização
- Consequência: entidades criadas de v1.0.45 a v1.0.64 (20 releases) persistem com casing original no banco
- `src/parsers/mod.rs:195-216`: `normalize_entity_name` aplica NFKD → filtro ASCII → lowercase → espaços/underscores para hífens → colapso de hífens consecutivos → trim
- `src/commands/normalize_entities.rs:61-120`: comando `normalize-entities` existe e funciona, mas NÃO é integrado à migração automática (`migrate`)
- `src/storage/entities.rs:231`: `find_entity_id` normaliza o lookup DESDE v1.0.65, permitindo que "Danilo Aguiar" encontre "danilo-aguiar" — porém as duas linhas CONTINUAM existindo no banco
- `src/commands/health.rs`: NÃO detecta entidades não normalizadas — `health --json` reporta `integrity_ok: true` mesmo com duplicatas por casing
### Cenário de Criação de Duplicatas
- Versão 1.0.50: usuário cria entidade via `remember --graph-stdin` com nome "Danilo Aguiar Teixeira"
- Entidade gravada como "Danilo Aguiar Teixeira" no banco (sem normalização)
- Relacionamentos vinculados ao ID dessa entidade
- Versão 1.0.65+: usuário cria nova entidade com nome similar via `remember --graph-stdin`
- `upsert_entity` normaliza para "danilo-aguiar-teixeira"
- ON CONFLICT NÃO dispara porque "Danilo Aguiar Teixeira" ≠ "danilo-aguiar-teixeira" no UNIQUE constraint
- NOVA linha criada com `id` diferente — agora existem DOIS nós para o mesmo referente
- Relacionamentos novos vinculados ao segundo ID
- Resultado: grafo vê dois nós desconectados dividindo relacionamentos do mesmo referente
### Impacto no Grafo de Conhecimento
- `graph traverse --from danilo-aguiar-teixeira --depth 2` encontra APENAS relacionamentos do nó normalizado
- Relacionamentos vinculados ao nó legado "Danilo Aguiar Teixeira" são INVISÍVEIS para a travessia
- `recall` e `hybrid-search` podem retornar ambas as variantes como resultados separados
- `deep-research` pode gerar sub-queries que encontram uma variante mas NÃO a outra
- `related <nome>` retorna grafo PARCIAL — apenas os relacionamentos do nó encontrado
- `graph stats` reporta `node_count` inflado com duplicatas que representam o mesmo referente
- `graph entities --sort-by degree` mostra grau DIVIDIDO entre as variantes — nenhuma tem o grau real
### Consequências
- Agentes LLM que fazem `graph traverse` a partir de uma variante perdem METADE do contexto do grafo
- Recall semântico retorna duplicatas do mesmo conceito gastando slots de resultado (k=5 desperdiça 1 slot com duplicata)
- Cadeias de evidência do `deep-research` ficam incompletas quando seed é uma variante e os hops estão na outra
- O chamador que faz `memory-entities --entity danilo-aguiar-teixeira` NÃO vê memórias vinculadas ao nó legado "Danilo Aguiar Teixeira"
- `graph stats` infla `avg_degree` e `node_count` com nós fantasmas que fragmentam o grafo real
- Operações de curadoria como `merge-entities` e `delete-entity` precisam ser executadas manualmente para CADA par de duplicatas
- O problema é SILENCIOSO: nenhum comando emite warning ao detectar entidades que diferem apenas por casing
### Causa Raiz — 5 Porquês
- POR QUE entidades com maiúsculas coexistem com versões kebab-case? Porque entidades criadas antes da v1.0.65 foram gravadas SEM normalização e NÃO foram migradas automaticamente
- POR QUE as entidades legadas não foram migradas? Porque `normalize-entities` é um comando MANUAL que o usuário precisa executar explicitamente — NÃO está integrado ao `migrate`
- POR QUE `normalize-entities` não foi integrado ao `migrate`? Porque a normalização pode mesclar entidades (merge destrutivo) e o `migrate` foi desenhado para mudanças de schema idempotentes, não para transformações de dados destrutivas
- POR QUE a normalização é considerada destrutiva? Porque a mesclagem de entidades move relacionamentos via `UPDATE OR IGNORE` + `DELETE` — operação irreversível que pode perder arestas duplicadas em caso de colisão
- POR QUE o UNIQUE constraint não preveniu duplicatas de casing? Porque SQLite faz comparação case-sensitive por padrão em colunas TEXT — "Foo" e "foo" são valores DISTINTOS sem COLLATE NOCASE
### Solução Proposta — Três Camadas Complementares
- CAMADA 1 (detecção proativa no `health`): adicionar check `non_normalized_entities` ao `health --json` que conta entidades cujo nome difere de `normalize_entity_name(name)`
- Campo `non_normalized_count: i64` na resposta do health
- Campo `normalization_warning: Option<String>` quando count > 0 com mensagem "run `normalize-entities --yes` to fix N entities"
- CAMADA 2 (migração assistida): incluir normalização como etapa OPCIONAL do `migrate` com flag `--normalize-entities`
- `migrate --normalize-entities` executa `normalize-entities --yes` após aplicar migrações de schema
- SEM a flag, `migrate` apenas emite warning no stderr se entidades não normalizadas forem detectadas
- CAMADA 3 (prevenção no UNIQUE constraint): alterar COLLATE do campo `name` na tabela `entities` para NOCASE
- `CREATE UNIQUE INDEX idx_entities_ns_name ON entities(namespace, name COLLATE NOCASE)`
- Previne que "Foo Bar" e "foo bar" coexistam — INSERT do segundo dispara ON CONFLICT
- Requer migração de schema (DDL change) e recriação do índice
### Benefícios
- Camada 1: diagnóstico instantâneo — `health --json` detecta duplicatas por casing em O(n) scan da tabela entities
- Camada 2: migração assistida reduz barreira de execução do `normalize-entities` de "manual e esquecível" para "integrado ao upgrade"
- Camada 3: prevenção na raiz — UNIQUE COLLATE NOCASE impede criação de duplicatas futuras sem depender de normalização no código Rust
- `graph traverse` e `deep-research` operam sobre grafo COMPLETO sem nós divididos por casing
- Recall semântico NÃO desperdiça slots com duplicatas do mesmo conceito
- `graph stats` reporta métricas precisas sem inflação de nós fantasmas
### Como Solucionar
- Passo 1: em `health.rs`, adicionar query `SELECT COUNT(*) FROM entities WHERE namespace = ?1 AND name != ?2` com `normalize_entity_name(name)` como ?2 para cada entidade
- Passo 1b: otimização — carregar todas entidades do namespace e filtrar em Rust: `entities.iter().filter(|(_, name)| normalize_entity_name(name) != *name).count()`
- Passo 2: adicionar campos `non_normalized_count` e `normalization_warning` ao `HealthResponse`
- Passo 3: em `migrate.rs`, adicionar flag `--normalize-entities` ao `MigrateArgs`
- Passo 4: em `migrate.rs`, após aplicar migrações de schema, executar `normalize_entities::run()` se `--normalize-entities` for passado
- Passo 5: em `migrate.rs`, SEM `--normalize-entities`, emitir `tracing::warn!` quando entidades não normalizadas forem detectadas
- Passo 6: criar migração DDL para alterar COLLATE do campo `name` na tabela `entities`
- Passo 6b: SQLite NÃO suporta ALTER COLUMN COLLATE — a migração precisa: CREATE TABLE entities_new (... name TEXT COLLATE NOCASE ...) → INSERT INTO entities_new SELECT * FROM entities → DROP TABLE entities → ALTER TABLE entities_new RENAME TO entities
- Passo 6c: recriar índices e triggers após a migração
- Passo 7: testes: criar entidade "Foo Bar" via INSERT direto (bypass upsert_entity), depois verificar que `health --json` detecta `non_normalized_count > 0`
- Passo 8: testes: verificar que após `normalize-entities --yes`, `health --json` reporta `non_normalized_count: 0`
- Passo 9: testes: verificar que com COLLATE NOCASE, INSERT de "foo bar" quando "Foo Bar" já existe dispara ON CONFLICT
### Complexidade
- Check no `health` (camada 1): BAIXA (~20 linhas — scan + contagem + campo no response)
- Flag `--normalize-entities` no `migrate` (camada 2): MÉDIA (~25 linhas — argumento Clap + chamada condicional + warning)
- Migração COLLATE NOCASE (camada 3): ALTA (~60 linhas — table rebuild + índices + recriar triggers + testes)
- Testes: MÉDIA (~30 linhas — 3 cenários: detecção no health, migração assistida, COLLATE NOCASE)
- Total estimado: ~135 linhas de código novo (camadas 1+2+3)
### Arquivos Afetados
- `src/commands/health.rs` — adicionar check `non_normalized_count` e campo `normalization_warning` ao response
- `src/commands/migrate.rs` — adicionar flag `--normalize-entities` e detecção de entidades não normalizadas
- `src/storage/migrations.rs` — adicionar migração DDL para COLLATE NOCASE na tabela entities (camada 3)
- `src/commands/normalize_entities.rs` — extrair lógica de normalização como função reutilizável pelo `migrate`
### Relação com Outros Gaps
- G20 (30 flags silenciosamente descartadas): G20 trata de flags CLI ignoradas; G24 trata de dados legados não migrados — ambos são problemas de contrato que persistem silenciosamente
- G21 (tracing::warn! com exit 0): G21 trata de feedback enganoso via exit code; G24 trata de `health --json` que reporta `integrity_ok: true` quando duplicatas existem — ambos mentem para o chamador
- G23 (JSON field naming inconsistency): G23 propõe normalização de campos JSON; G24 propõe normalização de dados de entidade — são problemas de normalização em camadas diferentes (contrato vs dados)




## G25 LOW (CORRIGIDO v1.0.67) — Super-hubs com degree excessivo: entidades com 50+ arestas concentram travessias, distorcem scores e degradam qualidade de recall sem mecanismo automatizado de detecção, redistribuição ou prevenção
### Status: CORRIGIDO — health.rs detecta super_hub_count, super_hub_warning, top_hub_entity, top_hub_degree, hub_warning
### Problema
- Entidades com degree excessivo (super-hubs) concentram parcela desproporcional das travessias de grafo
- No banco atual: `sqlite-graphrag` tem degree 166, `ingest-claude-code` tem 65, `rust-api-rules` tem 52
- 22 entidades possuem degree >= 30, representando apenas 1% dos 2193 nós mas concentrando parcela significativa das 2830 arestas
- `--max-entity-degree 50` (padrão no `link`) emite warning textual no stderr mas NÃO rejeita a operação — a aresta é criada mesmo assim
- O warning NÃO aparece no JSON de resposta do `link` — chamadores automatizados NÃO detectam o super-hub
- `remember --graph-stdin` e `ingest --mode claude-code` criam arestas SEM verificar degree — o cap só funciona no `link`
- `health --json` NÃO reporta entidades com degree excessivo — nenhum campo `super_hub_count` ou `max_degree_warning`
- `graph stats` reporta `max_degree` e `avg_degree` mas NÃO emite warning quando `max_degree` excede threshold
### Evidência no Código
- `src/commands/link.rs:75-78`: `--max-entity-degree` declarado com `default_value_t = 50` — emite warning mas NÃO rejeita
- `src/commands/link.rs:218-232`: após `was_created`, verifica degree contra cap e emite `output::emit_progress` — warning textual, NÃO JSON
- `src/commands/link.rs:226-229`: `if degree > cap` emite apenas `WARNING: entity '{entity_name}' degree {degree} exceeds cap {cap}` — NÃO adiciona ao campo `warnings[]` do JSON
- `src/storage/entities.rs`: `upsert_entity` e `create_or_fetch_relationship` NÃO verificam degree — sem cap no caminho de escrita do `remember`
- `src/commands/remember.rs`: NÃO tem `--max-entity-degree` flag — cria arestas sem verificar degree
- `src/commands/enrich.rs`: `persist_memory_bindings` cria arestas sem verificar degree
- `src/commands/ingest_claude.rs`: extração LLM cria arestas sem verificar degree
- `src/commands/health.rs`: ZERO verificação de degree em TODA a lógica de health check
### Cenário de Formação de Super-Hub
- Agente LLM processa 100 memórias via `ingest --mode claude-code`
- Cada memória menciona "sqlite-graphrag" como conceito relacionado
- Extração LLM cria relação `applies-to` entre cada nova entidade e `sqlite-graphrag`
- Resultado: `sqlite-graphrag` acumula 100+ arestas, todas com relação `applies-to`
- `graph traverse --from sqlite-graphrag --depth 2` retorna centenas de hops, a maioria ruído
- `deep-research` expande sub-queries via `sqlite-graphrag` e recebe fan-out explosivo
- O agente não tem como saber que `sqlite-graphrag` é um super-hub — nenhum warning no JSON
### Impacto na Qualidade do Grafo
- `graph traverse` a partir de super-hub retorna fan-out explosivo — centenas de vizinhos sem priorização
- `deep-research` com cadeias de evidência passando por super-hub gera ruído — TODOS os caminhos convergem no hub
- `recall --with-graph` expande via super-hub e polui `graph_matches[]` com entidades irrelevantes
- `hybrid-search --with-graph --max-hops 2` a partir de super-hub pode retornar milhares de resultados
- `graph stats` reporta `avg_degree` inflado por super-hubs — média NÃO representa a maioria dos nós
- Scores de travessia ficam comprimidos: com 166 vizinhos, cada aresta individual tem peso diluído
- Cadeias de evidência que passam por super-hub são FALSAS: o caminho A → super-hub → B existe para QUALQUER par (A, B) conectado ao hub
### Consequências
- Agentes LLM que fazem `graph traverse` a partir de super-hub recebem contexto ruidoso que consome tokens sem valor
- `deep-research` gera cadeias de evidência espúrias passando por super-hubs — TODOS os conceitos parecem conectados
- O score de grafo via decaimento por hop (`--graph-decay 0.7`) é inútil quando o hub conecta TUDO em 2 hops
- Curadoria manual é inviável: redistribuir 166 arestas de `sqlite-graphrag` requer análise semântica de CADA uma
- `merge-entities` com super-hub como alvo pode aumentar o problema — absorve arestas de entidades fonte
- Nenhum mecanismo preventivo: o grafo degrada silenciosamente conforme mais memórias são ingeridas
### Causa Raiz — 5 Porquês
- POR QUE entidades acumulam degree excessivo? Porque extração LLM (via `ingest --mode claude-code` e `enrich --operation memory-bindings`) cria relações com entidades "âncora" como `sqlite-graphrag` para cada nova memória
- POR QUE a extração LLM cria tantas relações com a mesma entidade? Porque o prompt de extração NÃO recebe informação de degree existente — o LLM não sabe que `sqlite-graphrag` já tem 166 arestas
- POR QUE o grau não é verificado na escrita? Porque `--max-entity-degree` está implementado SOMENTE no `link` — `remember`, `ingest` e `enrich` NÃO verificam degree
- POR QUE `--max-entity-degree` não rejeita a aresta? Porque foi implementado como `tracing::warn!` apenas — warning de telemetria, não constraint de integridade
- POR QUE o `health` não detecta super-hubs? Porque `health.rs` não foi atualizado para verificar `max_degree` contra um threshold configurável — o campo existe em `graph stats` mas não é avaliado
### Solução Proposta — Três Camadas Complementares
- CAMADA 1 (detecção no `health`): adicionar check `super_hub_count` ao `health --json` que conta entidades com degree acima de threshold configurável
- Campo `super_hub_count: i64` na resposta do health (entidades com degree > 50 por padrão)
- Campo `super_hub_warning: Option<String>` quando count > 0 com nomes das top-3 entidades
- CAMADA 2 (cap consistente em TODAS as escritas): implementar `--max-entity-degree` em `remember`, `ingest` e `enrich`
- No `remember --graph-stdin`: verificar degree ANTES de criar aresta, emitir warning no campo `warnings[]` do JSON
- No `ingest --mode claude-code`: incluir degree no prompt do LLM para que a extração EVITE criar relações com entidades que já têm degree alto
- No `enrich --operation memory-bindings`: verificar degree ANTES de persistir bindings, pular entidades acima do cap
- CAMADA 3 (redistribuição assistida por LLM): novo subcomando `enrich --operation hub-redistribute`
- Identifica super-hubs via scan de entidades com degree > threshold
- Para cada super-hub: envia lista de arestas para LLM com prompt pedindo agrupamento semântico
- LLM sugere: quais arestas manter, quais redirecionar para sub-entidades mais específicas, quais remover por redundância
- Ação é `--dry-run` por padrão — exibe plano de redistribuição sem aplicar
- Com `--yes`: aplica as mudanças (move arestas, cria sub-entidades, remove redundâncias)
### Benefícios
- Camada 1: diagnóstico instantâneo — `health --json` detecta super-hubs em O(n) scan da tabela entities
- Camada 2: prevenção — cap consistente em TODOS os caminhos de criação de aresta impede crescimento descontrolado
- Camada 3: remediação — redistribuição assistida por LLM resolve super-hubs existentes sem perda semântica manual
- `graph traverse` e `deep-research` operam com fan-out controlado — travessias mais precisas e menos ruidosas
- Cadeias de evidência do `deep-research` são genuínas — não passam por hub que conecta TUDO
- `recall --with-graph` retorna `graph_matches[]` relevantes em vez de centenas de vizinhos genéricos
### Como Solucionar
- Passo 1: em `health.rs`, adicionar query `SELECT name, degree FROM entities WHERE namespace = ?1 AND degree > ?2 ORDER BY degree DESC` com threshold 50
- Passo 2: adicionar campos `super_hub_count` e `super_hub_warning` ao `HealthResponse`
- Passo 3: em `remember.rs`, adicionar verificação de degree após `create_or_fetch_relationship`, emitir warning no JSON quando degree excede cap
- Passo 4: em `link.rs:226-229`, adicionar o warning ao campo `warnings[]` do `LinkResponse` em vez de apenas `output::emit_progress`
- Passo 5: em `enrich.rs:persist_memory_bindings`, verificar degree ANTES de criar aresta, emitir campo `skipped_high_degree` no ItemEvent
- Passo 6: implementar `EnrichOperation::HubRedistribute` com scan de super-hubs e prompt LLM para agrupamento semântico
- Passo 7: testes: criar entidade com 60 arestas, verificar que `health --json` reporta `super_hub_count > 0`
- Passo 8: testes: verificar que `remember --graph-stdin` com entidade de degree 55 emite warning no JSON
- Passo 9: testes: verificar que `enrich --operation hub-redistribute --dry-run` identifica super-hubs e gera plano
### Complexidade
- Check no `health` (camada 1): BAIXA (~20 linhas — scan + contagem + campos no response)
- Cap em `remember`, `link`, `enrich` (camada 2): MÉDIA (~40 linhas — verificação de degree + warning em JSON em 3 caminhos de escrita)
- `hub-redistribute` no `enrich` (camada 3): ALTA (~200 linhas — scan + prompt LLM + parsing de plano + aplicação de mudanças + testes)
- Testes: MÉDIA (~40 linhas — 3 cenários: detecção no health, cap no remember, redistribute dry-run)
- Total estimado: ~300 linhas de código novo (camadas 1+2+3)
### Arquivos Afetados
- `src/commands/health.rs` — adicionar check `super_hub_count` e campo `super_hub_warning` ao response
- `src/commands/link.rs:226-229` — mover warning para `warnings[]` do JSON em vez de `output::emit_progress`
- `src/commands/remember.rs` — adicionar verificação de degree após criação de aresta
- `src/commands/enrich.rs` — adicionar verificação de degree em `persist_memory_bindings` + novo `HubRedistribute` operation
- `src/commands/ingest_claude.rs` — incluir informação de degree no prompt de extração LLM
### Relação com Outros Gaps
- G21 (tracing::warn! com exit 0): G25 é instância ESPECÍFICA de G21 — `link.rs:226-229` emite `output::emit_progress` com exit 0 em vez de incluir no `warnings[]` JSON
- G24 (duplicatas de caixa): super-hubs podem ter degree inflado por duplicatas de casing — entidades "Sqlite Graphrag" e "sqlite-graphrag" dividem arestas que deveriam estar no mesmo nó
- G22 (read sem grafo): agentes que fazem `read --name <memory>` NÃO veem que as entidades vinculadas são super-hubs — sem `--with-graph` não há visibilidade de degree



## G26 LOW (CORRIGIDO v1.0.67) — Memórias finas (body < 500 chars) requerem enriquecimento contextualizado por domínio: `enrich --operation body-enrich` existe mas o prompt genérico NÃO incorpora contexto específico do namespace, entidades vinculadas ou domínio do usuário
### Status: CORRIGIDO — enrich.rs:1808-1820 busca linked_entities via query JOIN para contexto de domínio no prompt
### Problema
- 63 de 1027 memórias (6,1%) possuem body com menos de 500 caracteres — corpos finos que comprometem recall semântico
- O comando `enrich --operation body-enrich` existe (GAP-18 implementado) e identifica corretamente essas 63 memórias
- O prompt padrão (`BODY_ENRICH_PROMPT_PREFIX`) é GENÉRICO: "You are a knowledge assistant. Given a short or sparse memory body, expand it..."
- O prompt NÃO incorpora: contexto do namespace, entidades vinculadas à memória, memórias relacionadas, domínio específico do usuário
- A flag `--prompt-template` aceita arquivo de prompt customizado mas é ESTÁTICA — mesmo prompt para TODAS as memórias do batch
- NÃO existe mecanismo para injetar contexto dinâmico por memória (entidades, relações, memórias vizinhas) no prompt do LLM
- O resultado é enriquecimento genérico: o LLM expande o texto mas SEM consciência do domínio ou do grafo de conhecimento
### Evidência no Código
- `src/commands/enrich.rs:127`: `BODY_ENRICH_PROMPT_PREFIX` é constante genérica sem variáveis de template
- `src/commands/enrich.rs:1566-1574`: `call_body_enrich` carrega `--prompt-template` como string fixa — NÃO interpola variáveis por memória
- `src/commands/enrich.rs:1577-1578`: prompt final é `"{prompt_prefix}Target minimum length: {min_output_chars}..."` — único contexto dinâmico é o range de caracteres
- `src/commands/enrich.rs:1554-1561`: lê `memory_id` e `body` da memória mas NÃO carrega `description`, `memory_type`, entidades vinculadas ou memórias relacionadas
- `src/commands/enrich.rs:1584`: chama `call_claude(binary, &prompt, BODY_ENRICH_SCHEMA, &body, ...)` — o LLM recebe APENAS o body como input, sem contexto adicional
- `src/commands/enrich.rs:735-762`: `scan_short_body_memories` retorna `(id, name, body)` — não carrega description, type, ou entidades
- `src/commands/enrich.rs:45-46`: thresholds DEFAULT_BODY_ENRICH_MIN_CHARS=500 e DEFAULT_BODY_ENRICH_MAX_CHARS=2000 são ajustáveis via flags
### Cenário de Enriquecimento Genérico versus Contextualizado
- Memória fina: `deep-research-feature-proposal` com body de 124 chars: "deep-research command: multi-hop parallel query decomposition with sub-queries"
- Enriquecimento GENÉRICO (prompt atual): LLM expande para texto genérico sobre "query decomposition" sem saber que se trata do sqlite-graphrag CLI
- Enriquecimento CONTEXTUALIZADO (proposto): prompt inclui namespace "cli_sqlite-graphrag", entidades vinculadas `["deep-research", "query-decomposition", "sub-queries"]`, memórias vizinhas sobre implementação — LLM expande com contexto PRECISO do projeto
- O enriquecimento genérico pode até CONTRADIZER o domínio: LLM pode expandir "multi-hop" como conceito de redes quando no contexto é travessia de grafo de conhecimento
### Impacto no Recall Semântico
- Memórias com body < 500 chars geram embeddings de BAIXA qualidade — poucos tokens para capturar semântica
- `recall` para essas memórias retorna `distance` alta (baixa similaridade) porque o embedding não tem informação suficiente
- `deep-research` com sub-queries pode IGNORAR memórias finas porque o score vetorial é muito baixo
- `hybrid-search` compensa parcialmente via FTS5 (tokens exatos) mas o componente vetorial puxa o `combined_score` para baixo
- Embeddings re-gerados após enriquecimento genérico melhoram marginalmente mas NÃO capturam semântica de domínio
### Consequências
- 63 memórias com embeddings fracos degradam o recall do pipeline inteiro — `recall` retorna slots com memórias irrelevantes em vez de finas mas relevantes
- Enriquecimento genérico pode introduzir informação INCORRETA para o domínio — o LLM expande sem contexto e pode alucinar
- O chamador que usa `--prompt-template` precisa escrever UM prompt que funcione para TODAS as memórias — impossível customizar por domínio
- O padrão de 3 camadas canônico (hybrid-search → read → related) NÃO ajuda memórias finas — se o body é fino, o recall nem encontra a memória
- Memórias finas que são DECISÕES ou INCIDENTES perdem contexto crítico — "edit-skips-reembed-bug" com 331 chars não explica impacto nem resolução
### Causa Raiz — 5 Porquês
- POR QUE o enriquecimento é genérico? Porque o prompt `BODY_ENRICH_PROMPT_PREFIX` é constante sem variáveis de template e `--prompt-template` é estático
- POR QUE o prompt não incorpora contexto do grafo? Porque `call_body_enrich` lê APENAS `body` da memória — não carrega `description`, `memory_type`, entidades vinculadas ou memórias relacionadas
- POR QUE não carrega contexto adicional? Porque `scan_short_body_memories` retorna apenas `(id, name, body)` — a query SQL não faz JOIN com `memory_entities` nem `relationships`
- POR QUE o scan não faz JOINs? Porque o body-enrich foi implementado como expansão de texto isolada (padrão "expand short text") sem considerar o grafo como fonte de contexto
- POR QUE foi implementado como expansão isolada? Porque o padrão de referência (`ingest --mode claude-code`) processa arquivos individuais sem contexto cruzado — o `body-enrich` seguiu o mesmo padrão sem adaptar para memórias que JÁ possuem grafo
### Solução Proposta — Enriquecimento Contextualizado com Grafo
- MODIFICAÇÃO 1 (carregar contexto por memória): alterar `call_body_enrich` para carregar `description`, `memory_type`, entidades vinculadas e memórias relacionadas (1-hop) ANTES de chamar o LLM
- Query adicional: `SELECT e.name, e.type FROM memory_entities me JOIN entities e ON e.id = me.entity_id WHERE me.memory_id = ?1`
- Query adicional: `SELECT m2.name, m2.description FROM related_memories_1hop WHERE source_memory_id = ?1 LIMIT 5`
- MODIFICAÇÃO 2 (prompt com template dinâmico): alterar o prompt para incluir seções contextuais por memória
- Template com placeholders: `{name}`, `{description}`, `{memory_type}`, `{entities}`, `{related_memories}`, `{namespace}`
- O LLM recebe contexto COMPLETO do grafo para produzir enriquecimento preciso
- MODIFICAÇÃO 3 (prompt-template com variáveis): alterar `--prompt-template` para suportar interpolação de variáveis via `{variable}` — o prompt do arquivo é EXPANDIDO por memória
- MODIFICAÇÃO 4 (domínio via namespace): injetar nome do namespace como contexto de domínio no prompt — o LLM sabe que está enriquecendo memórias de "cli_sqlite-graphrag" e não de "farmácia-popular"
### Benefícios
- Enriquecimento contextualizado: o LLM recebe entidades, relações e memórias vizinhas — expande com precisão de domínio
- Embeddings de qualidade: memórias enriquecidas com contexto de grafo geram embeddings que capturam semântica real
- `recall` encontra memórias finas porque o embedding reflete o conteúdo completo do domínio
- `deep-research` inclui memórias enriquecidas nas cadeias de evidência porque o score vetorial é alto
- `--prompt-template` com variáveis permite enriquecimento customizado por projeto — cada namespace tem seu prompt de domínio
- ZERO breaking change: sem `--prompt-template` e sem `--with-graph-context`, comportamento idêntico ao atual
### Como Solucionar
- Passo 1: alterar `scan_short_body_memories` para retornar `(id, name, body, description, memory_type)` — adicionar 2 colunas à query SQL
- Passo 2: em `call_body_enrich`, após carregar body, executar query para buscar entidades vinculadas via `memory_entities` JOIN
- Passo 3: em `call_body_enrich`, executar query para buscar até 5 memórias relacionadas (1-hop) via `related` ou `relationships` JOIN
- Passo 4: criar struct `BodyEnrichContext` com campos `name`, `description`, `memory_type`, `entities: Vec<String>`, `related: Vec<String>`, `namespace`
- Passo 5: alterar `BODY_ENRICH_PROMPT_PREFIX` para incluir seções opcionais: "Memory name: {name}\nType: {memory_type}\nDescription: {description}\nLinked entities: {entities}\nRelated memories: {related}\nDomain: {namespace}"
- Passo 6: quando `--prompt-template` é fornecido, expandir variáveis `{name}`, `{description}`, etc. no template antes de enviar ao LLM
- Passo 7: adicionar flag `--with-graph-context` (default true) para controlar se contexto de grafo é incluído no prompt
- Passo 8: testes: verificar que enriquecimento com contexto de grafo produz body mais preciso que sem
- Passo 9: testes: verificar que `--prompt-template` com `{entities}` interpola corretamente os nomes das entidades vinculadas
### Complexidade
- Carregar contexto por memória (modificação 1): BAIXA (~25 linhas — 2 queries SQL adicionais por memória + struct de contexto)
- Prompt com template dinâmico (modificação 2): BAIXA (~20 linhas — expansão de variáveis no prompt string)
- `--prompt-template` com variáveis (modificação 3): BAIXA (~15 linhas — regex replace de `{variable}` por valores reais)
- Flag `--with-graph-context` (modificação 4): BAIXA (~5 linhas — argumento Clap + condicional)
- Testes: BAIXA (~20 linhas — 2 cenários: com e sem contexto de grafo)
- Total estimado: ~85 linhas de código novo
### Arquivos Afetados
- `src/commands/enrich.rs:735-762` — alterar `scan_short_body_memories` para retornar description e memory_type
- `src/commands/enrich.rs:1541-1626` — alterar `call_body_enrich` para carregar e injetar contexto de grafo no prompt
- `src/commands/enrich.rs:127` — alterar `BODY_ENRICH_PROMPT_PREFIX` para incluir placeholders de contexto
- `src/commands/enrich.rs:203-289` — adicionar flag `--with-graph-context` ao `EnrichArgs`
### Relação com Outros Gaps
- G22 (read sem grafo): G22 trata de exposição de grafo no `read`; G26 trata de utilização de grafo no prompt de enriquecimento — ambos sofrem por falta de integração entre dados textuais e estruturais
- G25 (super-hubs): se a memória fina está vinculada a super-hub, o contexto de grafo injetado no prompt pode ser ruidoso — G26 deve LIMITAR entidades por degree (excluir super-hubs do contexto)
- G24 (duplicatas de caixa): memórias finas vinculadas a entidades duplicadas por casing podem receber contexto incompleto — G26 depende de G24 para contexto de grafo limpo




## G27 LOW (CORRIGIDO v1.0.67) — Comando `enrich` declara 13 operações mas implementa apenas 3: 10 operações LLM retornam `not_yet_implemented` com exit 0, forçando orquestração manual via shell scripts com `claude -p` ou `codex exec`
### Status: CORRIGIDO — TODAS 13 EnrichOperation implementadas com dispatch em enrich.rs:1202-1213
### Problema
- O enum `EnrichOperation` em `enrich.rs:134-163` declara 13 variantes de operação
- Apenas 3 operações estão implementadas: `memory-bindings`, `entity-descriptions`, `body-enrich`
- As 10 operações restantes retornam `status: "not_yet_implemented"` com exit code 0 (sucesso)
- O chamador que executa `enrich --operation weight-calibrate --mode claude-code` recebe ZERO erro e ZERO enriquecimento
- A infraestrutura completa já existe: queue DB, resume, retry-failed, NDJSON output, cost tracking, rate limiting, backoff exponencial
- Cada operação implementada segue o MESMO padrão de 3 funções: `scan_*` (query SQL), `call_*` (spawna LLM), `persist_*` (grava resultado)
- As 10 operações pendentes requerem APENAS: (1) query SQL de scan específica, (2) prompt + schema JSON, (3) função de persistência
- O padrão é IDÊNTICO — a infraestrutura NÃO é o gargalo, o gargalo é a ausência de prompts e schemas por operação
### Operações Não Implementadas — Inventário
- `weight-calibrate`: recalibrar pesos de relações usando julgamento LLM (82.5% das arestas com peso >= 0.7)
- `relation-reclassify`: reclassificar tipos de relação genéricos como `applies_to` (6379 arestas genéricas)
- `entity-connect`: conectar entidades inertes sugerindo novas relações via semântica (2765 entidades com grau <= 3)
- `entity-type-validate`: validar e corrigir entity_type usando julgamento LLM (ex: metodologia classificada como `concept` quando deveria ser `tool`)
- `description-enrich`: enriquecer descrições genéricas ou curtas de memórias (< 80 caracteres)
- `cross-domain-bridges`: detectar pontes entre subgrafos desconectados via análise LLM
- `domain-classify`: classificar memórias em categorias de domínio
- `graph-audit`: auditoria de qualidade do grafo completo via LLM
- `deep-research-synth`: sintetizar achados de `deep-research` em memórias estruturadas
- `body-extract`: extrair corpo estruturado de texto não-estruturado
### Evidência no Código
- `src/commands/enrich.rs:134-163`: enum `EnrichOperation` com 13 variantes, 10 anotadas `(scan only)`
- `src/commands/enrich.rs:1067-1095`: match no `run()` que despacha as 3 implementadas e retorna `not_yet_implemented` para as 10 restantes
- `src/commands/enrich.rs:1186-1218`: dispatch loop que chama `call_memory_bindings`, `call_entity_description`, `call_body_enrich` — com `unreachable!()` para as demais
- `src/commands/enrich.rs:1652-1671`: `scan_operation` para as 10 operações usa query SQL GENÉRICA (`SELECT name FROM memories`) sem filtro específico por operação
- `src/commands/enrich.rs:1413-1475`: `call_memory_bindings` — 62 linhas implementando o padrão scan → call → persist
- `src/commands/enrich.rs:1476-1540`: `call_entity_description` — 64 linhas implementando o MESMO padrão
- `src/commands/enrich.rs:1541-1630`: `call_body_enrich` — 89 linhas implementando o MESMO padrão
- `src/commands/enrich.rs:478-615`: `call_claude` genérica que aceita QUALQUER prompt + schema + input — já reutilizável
- `src/commands/enrich.rs:1716+`: `call_codex` genérica com a MESMA assinatura — já reutilizável
### Padrão Repetido nas 3 Implementações
- CADA operação implementada segue o MESMO padrão de 60-90 linhas:
- Passo 1: query SQL para buscar o item (memória ou entidade) por nome — 5-8 linhas
- Passo 2: montar input text a partir do body ou nome — 2-5 linhas
- Passo 3: chamar `call_claude` ou `call_codex` com prompt constante + schema constante + input — 8-12 linhas
- Passo 4: parsear valor retornado do JSON estruturado — 3-5 linhas
- Passo 5: persistir resultado via SQL (`UPDATE` ou `INSERT`) — 15-25 linhas
- Passo 6: retornar `EnrichItemResult::Done` com métricas — 10-15 linhas
- A infraestrutura (`call_claude`, `call_codex`, queue DB, NDJSON events, cost tracking) é COMPARTILHADA e já funciona
### Consequências
- O chamador que quer calibrar pesos precisa orquestrar MANUALMENTE via shell script com `claude -p` ou `codex exec`
- CADA script manual reimplementa: spawn do LLM, parsing do output JSON, persistência via `sqlite-graphrag link`
- A infraestrutura de queue DB, resume, retry-failed, cost tracking NÃO é acessível nos scripts manuais
- Se o script falha no item 500 de 6379, o chamador PERDE progresso e precisa reprocessar TUDO
- Os scripts manuais NÃO emitem NDJSON padronizado — o chamador perde observabilidade
- O enum declara as 10 variantes como se existissem — o CLI aceita `--operation weight-calibrate` sem erro
- Exit code 0 com `status: "not_yet_implemented"` engana pipelines automatizados que verificam apenas exit code
- Documentação do `enrich --help` lista as 13 operações sem distinguir quais funcionam e quais são stubs
### O Que os Scripts Manuais Precisam Reimplementar
- Spawn de `claude -p` com `--json-schema`, `--max-turns 3`, `--dangerously-skip-permissions`, `--settings '{"hooks":{}}'`
- Spawn de `codex exec` com `--output-schema`, `--ephemeral`, `--skip-git-repo-check`, `--sandbox read-only`
- Parsing do output JSON do Claude (array com `structured_output`) versus Codex (JSONL com último `agent_message`)
- Persistência via `sqlite-graphrag link`, `reclassify`, `edit`, `remember --force-merge --graph-stdin`
- Tratamento de rate limiting (429) com backoff exponencial
- Controle de custo acumulado (`--max-cost-usd`)
- Detecção de OAuth versus API key para omitir `cost_usd`
- Retentativa de falhas por item (retry individualmente sem reprocessar batch)
- NDJSON de progresso (phase, scan, item events, summary)
- TUDO isto já está implementado em `enrich.rs` para as 3 operações que funcionam
### Causa Raiz — 5 Porquês
- POR QUE 10 operações retornam `not_yet_implemented`? Porque cada operação requer prompt especializado, schema JSON específico, query SQL de scan filtrada e função de persistência — nenhuma destas foi escrita
- POR QUE não foram escritas? Porque a prioridade foi entregar as 3 operações mais urgentes (`memory-bindings` para 53% memórias órfãs, `entity-descriptions` para 5649 entidades sem descrição, `body-enrich` para 63 memórias finas)
- POR QUE as 10 restantes não seguiram? Porque cada operação foi tratada como feature independente em vez de instância parametrizada de um padrão genérico
- POR QUE não foi parametrizado? Porque os prompts, schemas e lógica de persistência foram hardcoded como constantes e funções separadas em vez de dados configuráveis
- POR QUE foram hardcoded? Porque o padrão só se tornou evidente APÓS implementar as 3 primeiras — no momento do design, cada operação parecia suficientemente distinta para justificar implementação ad-hoc
### Solução Proposta — Implementar as 10 Operações Seguindo o Padrão Existente
- ABORDAGEM: para cada operação, implementar as 3 funções (scan, call, persist) reutilizando a infraestrutura existente
- Cada operação requer: (1) prompt constante, (2) schema JSON constante, (3) query SQL de scan, (4) função de persistência
- A infraestrutura compartilhada (`call_claude`, `call_codex`, queue DB, NDJSON, cost tracking, rate limiting) NÃO precisa de alteração
- Prioridade sugerida por impacto no grafo:
- P1 — `weight-calibrate`: 82.5% das arestas com peso >= 0.7 distorcem recall scores — impacto ALTO em toda busca
- P1 — `relation-reclassify`: 6379 arestas `applies_to` genéricas reduzem precisão de travessia — impacto ALTO
- P2 — `entity-connect`: 2765 entidades inertes (grau <= 3) não participam de travessia — impacto MÉDIO
- P2 — `entity-type-validate`: tipos incorretos distorcem filtros `--entity-type` — impacto MÉDIO
- P2 — `description-enrich`: descrições curtas prejudicam recall semântico — impacto MÉDIO
- P3 — `cross-domain-bridges`: subgrafos isolados não respondem queries cross-domain — impacto específico
- P3 — `domain-classify`: classificação de domínio é útil para filtros mas não bloqueia busca — impacto BAIXO
- P3 — `graph-audit`: auditoria de qualidade é operação ad-hoc, não pipeline — impacto BAIXO
- P3 — `deep-research-synth`: síntese de deep-research é operação rara — impacto BAIXO
- P3 — `body-extract`: extração estruturada é caso de uso específico — impacto BAIXO
### Benefícios
- TODAS as 10 operações herdam infraestrutura de queue DB, resume, retry-failed, NDJSON e cost tracking
- Chamador executa `enrich --operation weight-calibrate --mode claude-code --resume` em vez de script shell de 50 linhas
- Rate limiting e backoff exponencial aplicados automaticamente — script manual NÃO tem isto
- Progresso preservado: se falha no item 500, `--resume` continua do 501
- NDJSON padronizado permite monitoramento e aggregação uniforme
- Suporte a Claude Code headless E Codex CLI headless via `--mode` sem alterar lógica
- OAuth-first: custo omitido automaticamente para assinaturas — script manual precisa detectar manualmente
- Dry-run (`--dry-run`) gratuito para todas operações — preview sem gastar tokens
### Como Solucionar — Padrão por Operação
- Para CADA operação pendente, implementar:
- Passo 1: prompt constante (`const WEIGHT_CALIBRATE_PROMPT: &str = ...`) — 5-10 linhas
- Passo 2: schema JSON constante (`const WEIGHT_CALIBRATE_SCHEMA: &str = ...`) — 10-20 linhas
- Passo 3: query SQL de scan específica (`fn scan_weight_candidates`) — 10-15 linhas
- Passo 4: função `call_weight_calibrate` seguindo padrão de `call_memory_bindings` — 50-70 linhas
- Passo 5: função `persist_weight_calibrate` — 10-20 linhas
- Passo 6: adicionar case no dispatch loop (`run()` linhas 1186-1218) — 5 linhas
- Passo 7: adicionar case no `scan_operation` (linhas 1637-1671) — 3 linhas
- Passo 8: testes unitários — 20-30 linhas
- Total por operação: ~115-170 linhas
### Como Solucionar — Detalhamento das 4 Operações P1/P2
#### weight-calibrate
- Scan: `SELECT r.id, e1.name, e2.name, r.relation, r.weight FROM relationships r JOIN entities e1 ON e1.id=r.source_entity_id JOIN entities e2 ON e2.id=r.target_entity_id WHERE r.weight >= 0.7 AND e1.namespace=?1`
- Prompt: "Avalie se o peso desta relação está calibrado. Escala: 0.9=dependência vital, 0.7=design importante, 0.5=contexto útil, 0.3=referência fraca"
- Schema: `{"calibrated_weight": number, "reasoning": string}`
- Persistência: `UPDATE relationships SET weight=?1 WHERE id=?2`
#### relation-reclassify
- Scan: `SELECT r.id, e1.name, e2.name, r.relation FROM relationships r JOIN entities e1... WHERE r.relation='applies_to' AND e1.namespace=?1`
- Prompt: "Determine a relação REAL entre estas entidades. applies_to é genérico demais"
- Schema: `{"relation": string (enum canônico), "strength": number, "reasoning": string}`
- Persistência: `UPDATE relationships SET relation=?1, weight=?2 WHERE id=?3`
#### entity-connect
- Scan: `SELECT e.name, e.type FROM entities e LEFT JOIN relationships r ON e.id=r.source_entity_id OR e.id=r.target_entity_id WHERE e.namespace=?1 GROUP BY e.id HAVING COUNT(r.id) <= 3`
- Prompt: "Entidade-alvo com grau baixo. Candidatos encontrados por recall semântico. Quais conexões são REAIS?"
- Schema: `{"connections": [{target, relation, strength}], maxItems: 3}`
- Pré-processamento: chamar `recall` internamente para encontrar candidatos antes de enviar ao LLM
- Persistência: `sqlite-graphrag link` para cada conexão válida
#### entity-type-validate
- Scan: `SELECT e.id, e.name, e.type FROM entities e WHERE e.namespace=?1`
- Prompt: "Avalie se o entity_type está correto para esta entidade"
- Schema: `{"correct_type": string (enum), "needs_change": bool, "reasoning": string}`
- Persistência: `sqlite-graphrag reclassify --name <entidade> --new-type <tipo>` (ou SQL direto)
### Complexidade
- Infraestrutura: ZERO alteração necessária — `call_claude`, `call_codex`, queue DB, NDJSON já funcionam
- Por operação P1 (weight-calibrate, relation-reclassify): ~130 linhas cada (prompt + schema + scan + call + persist + dispatch + tests)
- Por operação P2 (entity-connect, entity-type-validate, description-enrich): ~150 linhas cada (entity-connect requer pré-processamento com recall)
- Por operação P3 (cross-domain-bridges, domain-classify, graph-audit, deep-research-synth, body-extract): ~120 linhas cada
- Total estimado para as 10 operações: ~1350 linhas de código novo
- Sugestão de implementação: P1 primeiro (260 linhas, impacto imediato), P2 depois (450 linhas), P3 por demanda
### Arquivos Afetados
- `src/commands/enrich.rs:1067-1095` — remover o bloco `not_yet_implemented` para operações implementadas
- `src/commands/enrich.rs:1186-1218` — adicionar cases no dispatch loop para cada nova operação
- `src/commands/enrich.rs:1632-1671` — substituir scan SQL genérico por queries específicas por operação
- `src/commands/enrich.rs` (top-level) — adicionar constantes de prompt e schema por operação
- `src/commands/enrich.rs` (bottom) — adicionar funções `call_*` e `persist_*` por operação
### Relação com Outros Gaps
- G25 (super-hubs degree): `entity-connect` deve EXCLUIR super-hubs (degree >= 50) dos candidatos para não aumentar concentração — G27 depende de G25 para filtro de grau
- G26 (body-enrich genérico): G26 trata da qualidade do prompt de `body-enrich`; G27 trata de operações que NÃO existem — são complementares, não sobrepostos
- G22 (read sem grafo): `entity-connect` e `entity-type-validate` precisam de contexto de grafo por item — G27 se beneficia de G22 (`read --with-graph`) para reduzir roundtrips
- G24 (duplicatas de caixa): `relation-reclassify` e `entity-connect` operam sobre entidades que podem ter duplicatas por casing — G27 depende de G24 para dados limpos


## P01 HIGH — std::fs usado dentro de runtime Tokio no daemon.rs bloqueando executor async
### Problema
- `daemon.rs` roda com `#[tokio::main]` em runtime multi-thread
- 8 chamadas a `std::fs` operam diretamente no executor async sem `spawn_blocking`
- `std::fs::remove_file` (linha 195), `std::fs::create_dir_all` (linha 691), `std::fs::remove_file` (linha 739)
- `std::fs::read` (linha 750), `std::fs::write` (linha 758), `std::fs::create_dir_all` (linha 756)
- Cada chamada `std::fs` bloqueia a worker thread do tokio durante I/O de disco
- I/O de disco em NVMe leva ~50-500us mas em HDD ou Dropbox sync pode levar 10-100ms
- Worker thread bloqueada NAO processa outras tasks async durante a espera
### Consequencias
- Starvation de tasks async vizinhas durante I/O de disco lento
- Latencia p99 degradada em operacoes do daemon sob carga
- Em filesystems lentos (NFS, Dropbox, FUSE), bloqueio pode atingir centenas de ms
- O daemon serve embeddings via UDS e precisa de baixa latencia consistente
- Outras tasks como accept de conexoes e ping ficam bloqueadas durante I/O
### Causa Raiz
- O daemon foi escrito com chamadas `std::fs` por simplicidade
- NAO houve migracao para `tokio::fs` quando o runtime foi configurado como multi-thread
- A regra "NUNCA usar `std::fs` em async" das rules de paralelismo nao foi aplicada
### Evidencia no Codigo
- `src/daemon.rs:22` — `use std::fs::{File, OpenOptions}`
- `src/daemon.rs:195` — `std::fs::remove_file(&lock_path)` no Drop de DaemonSpawnGuard
- `src/daemon.rs:691` — `std::fs::create_dir_all` para criar diretorio de models
- `src/daemon.rs:739` — `std::fs::remove_file(path)` para remover PID file
- `src/daemon.rs:750` — `std::fs::read(path)` para ler PID file
- `src/daemon.rs:756-758` — `std::fs::create_dir_all` e `std::fs::write` para salvar PID file
### Solucao Proposta
- Migrar chamadas para `tokio::fs::remove_file`, `tokio::fs::read`, `tokio::fs::write`, `tokio::fs::create_dir_all`
- Para o `Drop` (linha 195), usar `std::fs` com `spawn_blocking` ou aceitar bloqueio no shutdown (aceitavel)
- Manter `std::fs` apenas no path de startup sincrono antes do runtime iniciar
### Complexidade
- BAIXA — substituicao direta de 6-8 chamadas por equivalentes tokio::fs
### Arquivos Afetados
- `src/daemon.rs:195,691,739,750,756,758` — migrar para tokio::fs


## P02 HIGH — Embedder global usa Mutex serializando TODOS os embeddings entre threads
### Problema
- `static EMBEDDER: OnceLock<Mutex<TextEmbedding>>` (embedder.rs:16) serializa TODO embedding
- CADA chamada `embed_passage` ou `embed_query` adquire lock exclusivo (linhas 108, 127, 155)
- O modelo ONNX `TextEmbedding` requer `&mut self` para inferencia — Mutex eh obrigatorio
- Em enrich.rs com G19 thread pool de N workers, TODOS competem pelo mesmo Mutex
- Em ingest.rs com rayon `par_iter`, cada thread serializa no Mutex do embedder
- O daemon resolve via UDS: modelo carregado UMA vez, requests servidos via socket
- SEM daemon, o paralelismo de G19 e rayon eh anulado na etapa de embedding
### Consequencias
- Thread pool de 4 workers efetivamente processa 1 embedding por vez
- Throughput de embedding NAO escala com paralelismo — gargalo serial no Mutex
- Tempo de espera no lock cresce linearmente com numero de workers
- Workers idle enquanto aguardam lock do embedder
- O daemon mascara o problema servindo via UDS mas CLI sem daemon sofre
### Causa Raiz
- API do `fastembed::TextEmbedding` requer `&mut self` — nao permite `&self`
- `Mutex` eh a unica primitiva segura para `&mut self` em contexto multi-thread
- O design eh correto: singleton com Mutex para recurso caro mutavel
- A contenção eh by-design quando daemon NAO esta ativo
### Evidencia no Codigo
- `src/embedder.rs:16` — `static EMBEDDER: OnceLock<Mutex<TextEmbedding>>`
- `src/embedder.rs:108` — `.lock().map_err(...)` em `embed_passage`
- `src/embedder.rs:127` — `.lock().map_err(...)` em `embed_query`
- `src/embedder.rs:155` — `.lock().map_err(...)` em `embed_many`
### Solucao Proposta
- Opcao A (RECOMENDADA): documentar que sem daemon o embedding eh serial por design
- Opcao B: verificar se fastembed suporta `&self` em versoes recentes (would allow RwLock)
- Opcao C: criar pool de N modelos ONNX (N Mutex instances) — multiplica RAM por N
- O daemon JA resolve: modelo carregado 1 vez, requests via UDS sem contencao de Mutex
- Recomendacao: SEMPRE iniciar daemon antes de operacoes paralelas pesadas
### Complexidade
- Opcao A: TRIVIAL (documentacao)
- Opcao B: BAIXA (verificar API, possivel breaking change)
- Opcao C: ALTA (pool de modelos, gestao de lifecycle)
### Arquivos Afetados
- `src/embedder.rs:16,108,127,155` — Mutex do embedder


## P03 HIGH — Command::spawn em loop sem cgroup isolation para subprocessos pesados
### Problema
- 4 arquivos fazem `Command::spawn` para `claude -p` ou `codex exec` em loop
- `enrich.rs:2386` spawna subprocesso em loop serial ou paralelo (G19)
- `ingest_claude.rs:339` spawna `claude -p` em loop
- `ingest_codex.rs:335` spawna `codex exec` em loop
- `claude_runner.rs:259` spawna `claude -p` como helper
- NENHUM spawn usa `systemd-run --scope` com `MemoryMax` ou `CPUQuota`
- Cada `claude -p` consome ~800MB-2GB de RAM dependendo do modelo e contexto
- Com G19 `--llm-parallelism 4`, sao 4 subprocessos de ~1GB cada sem cap de memoria
- O `--max-rss-mb` existente verifica RSS do processo PAI, nao dos subprocessos
### Consequencias
- 4 subprocessos `claude -p` paralelos consomem ~4-8GB sem limite
- OOM killer do kernel pode matar processos indiscriminadamente
- Sem `OOMScoreAdjust`, o kernel pode matar o processo pai em vez do filho
- Em maquinas com 8GB RAM, 4 subprocessos paralelos esgotam memoria
- O semaforo (G18) limita invocacoes CLI, nao RAM de subprocessos
- Subprocessos `claude -p` nao sao filhos diretos do cgroup do sqlite-graphrag
### Causa Raiz
- O spawn de subprocessos foi implementado sem considerar isolamento de recursos
- `systemd-run --scope` eh Linux-only e requer disponibilidade do systemd
- O design original priorizou simplicidade cross-platform sobre seguranca de recursos
### Evidencia no Codigo
- `src/commands/enrich.rs:2386` — `cmd.spawn()` sem cgroup
- `src/commands/ingest_claude.rs:339` — `cmd.spawn()` sem cgroup
- `src/commands/ingest_codex.rs:335` — `cmd.spawn()` sem cgroup
- `src/commands/claude_runner.rs:259` — `cmd.spawn()` sem cgroup
- ZERO ocorrencias de `systemd-run` em todo `src/`
### Solucao Proposta
- Opcao A (RECOMENDADA): envolver spawn em `systemd-run --scope -p MemoryMax=2G` no Linux
- Verificar disponibilidade de `systemd-run` via `which` antes de usar
- Fallback para spawn direto em macOS e Windows (sem systemd)
- Adicionar flag `--cgroup-limit <BYTES>` para configurar MemoryMax
- Opcao B: usar `setrlimit(RLIMIT_AS)` via `libc::setrlimit` antes do exec (cross-platform parcial)
### Complexidade
- Opcao A: MEDIA (~40 linhas, deteccao de systemd + wrapper condicional)
- Opcao B: BAIXA (~15 linhas, setrlimit antes do exec)
### Arquivos Afetados
- `src/commands/claude_runner.rs:259` — wrapper condicional no spawn
- `src/commands/enrich.rs:2386` — mesma mudanca
- `src/commands/ingest_claude.rs:339` — mesma mudanca
- `src/commands/ingest_codex.rs:335` — mesma mudanca


## P04 MEDIUM — Ausencia de CancellationToken para graceful shutdown propagavel
### Problema
- O shutdown usa `AtomicBool SHUTDOWN` (lib.rs:41) como flag global
- NAO usa `CancellationToken` hierarquico do `tokio_util`
- O daemon verifica `shutdown_requested()` periodicamente mas nao propaga cancel a tasks filhas
- Tasks async em andamento continuam rodando apos SIGINT ate o proximo checkpoint
- NAO ha child tokens para propagacao hierarquica
### Consequencias
- Shutdown nao eh instantaneo — tasks podem continuar rodando por segundos apos SIGINT
- Workers de embedding no daemon continuam processando request atual ate conclusao
- Subprocessos `claude -p` nao recebem SIGTERM propagado do token
- Loop de enrich serial pode processar mais 1 item apos SIGINT antes de verificar flag
### Causa Raiz
- `AtomicBool` foi escolhido por simplicidade e compatibilidade com codigo sync
- `CancellationToken` requer `tokio_util` como dependencia adicional
- O design original nao previa hierarchical cancellation
### Evidencia no Codigo
- `src/lib.rs:41` — `pub static SHUTDOWN: AtomicBool = AtomicBool::new(false)`
- `src/main.rs:257` — handler de SIGINT/SIGTERM seta `SHUTDOWN.store(true)`
- ZERO ocorrencias de `CancellationToken` em todo `src/`
### Solucao Proposta
- Migrar `AtomicBool SHUTDOWN` para `CancellationToken` com child tokens por task
- Manter `AtomicBool` como fallback para codigo sync que nao pode usar async cancel
- Adicionar `tokio_util` ao Cargo.toml
### Complexidade
- MEDIA (~30 linhas, refactor do shutdown flow + adicionar dependencia)
### Arquivos Afetados
- `src/lib.rs:41` — migrar para CancellationToken
- `src/main.rs:257` — signal handler cancela token em vez de setar bool
- `src/daemon.rs` — usar child_token por task
- `Cargo.toml` — adicionar tokio_util


## P05 MEDIUM — thread::sleep usado em contexto de daemon async bloqueando worker threads
### Problema
- `daemon.rs:532,661,708` usam `std::thread::sleep` dentro de funcoes do daemon
- O daemon roda com `#[tokio::main]` e `thread::sleep` bloqueia a worker thread do tokio
- Cada `thread::sleep` impede a worker thread de processar outras tasks async
- Duracao dos sleeps: 50ms (linha 708), variavel (linhas 532, 661)
### Consequencias
- Worker thread do tokio bloqueada durante sleep — starvation de tasks vizinhas
- Latencia de accept de novas conexoes UDS aumenta durante sleep
- Em runtime `current_thread`, bloqueio total do executor durante sleep
- Degradacao de responsividade do daemon proporcional a frequencia dos sleeps
### Causa Raiz
- Codigo do daemon mistura patterns sync e async sem isolamento
- `thread::sleep` foi usado por simplicidade em loops de polling
### Evidencia no Codigo
- `src/daemon.rs:532` — `thread::sleep(Duration::from_millis(sleep_ms))`
- `src/daemon.rs:661` — `thread::sleep(Duration::from_millis(sleep_ms))`
- `src/daemon.rs:708` — `thread::sleep(Duration::from_millis(50))`
- NOTA: `lock.rs:105` e `storage/utils.rs:51` tambem usam `thread::sleep` mas sao sync — ACEITAVEL
### Solucao Proposta
- Migrar os 3 pontos em `daemon.rs` para `tokio::time::sleep().await`
- Manter `thread::sleep` em codigo sync (lock.rs, utils.rs) sem mudanca
### Complexidade
- BAIXA (~3 linhas alteradas)
### Arquivos Afetados
- `src/daemon.rs:532,661,708` — migrar para tokio::time::sleep


## P06 MEDIUM — process::exit em 7 locais sem garantia de cleanup completo
### Problema
- `main.rs` chama `std::process::exit` em 7 pontos distintos (linhas 155, 163, 177, 203, 250, 333)
- Cada chamada faz `flush(&mut stderr())` antes de sair
- NAO garante cleanup de: conexoes SQLite, slots CLI (flock), buffers stdout, WAL checkpoint
- `process::exit` encerra o processo imediatamente sem rodar destructors pendentes
- Slots CLI adquiridos via flock podem ficar travados ate o SO liberar o file descriptor
### Consequencias
- Slots CLI podem ficar travados temporariamente (SO libera flock no close do fd)
- WAL pode ficar sem checkpoint final — proximo open faz recovery
- NDJSON parcial no stdout pode corromper parsing downstream
- Conexoes SQLite nao recebem `PRAGMA optimize` de cleanup
- Destructors de structs com Drop nao rodam
### Causa Raiz
- `process::exit` foi escolhido como forma rapida de mapear exit codes
- O design original nao considerou cleanup de recursos abertos
- Alternativa (retornar Result com exit code) requer refactor do main
### Evidencia no Codigo
- `src/main.rs:155` — `std::process::exit(e.exit_code())`
- `src/main.rs:163` — `std::process::exit(2)`
- `src/main.rs:177` — `std::process::exit(e.exit_code())`
- `src/main.rs:203` — `std::process::exit(20)`
- `src/main.rs:250` — `std::process::exit(e.exit_code())`
- `src/main.rs:333` — `std::process::exit(e.exit_code())`
- NENHUM deles faz WAL checkpoint ou liberacao explicita de slot antes de sair
### Solucao Proposta
- Refatorar main para retornar `Result<(), AppError>` com exit code no wrapper
- Usar `Drop` para cleanup de slots, conexoes e WAL
- Manter `process::exit` apenas no handler de SIGINT para exit imediato
### Complexidade
- MEDIA (~50 linhas de refactor no main + Drop impls)
### Arquivos Afetados
- `src/main.rs:155,163,177,203,250,333` — refatorar para retornar Result


## P07 MEDIUM — Classificacao de workload ausente em 5 de 8 modulos paralelos
### Problema
- As rules exigem classificacao de workload documentada no topo de cada modulo paralelo
- Apenas 3 de 8 modulos paralelos tem classificacao:
  - `daemon.rs:254` — "CPU-bound" (correto, referente a ONNX init)
  - `ingest.rs:405` — "CPU-bound" (correto, rayon parallel processing)
  - `deep_research.rs:4` — "I/O-bound" (correto, SQLite WAL reads)
- 5 modulos paralelos NAO tem classificacao:
  - `enrich.rs` — subprocess I/O-bound (spawn de claude/codex, wait de network)
  - `ingest_claude.rs` — subprocess I/O-bound (spawn de claude -p)
  - `ingest_codex.rs` — subprocess I/O-bound (spawn de codex exec)
  - `embedder.rs` — CPU-bound (ONNX inference, matrix multiplication)
  - `lock.rs` — I/O-bound (flock polling com sleep)
### Consequencias
- Futuras mudancas podem escolher primitiva errada sem saber a classificacao
- Risco de usar async para CPU-bound ou sync para I/O-bound
- Violacao da regra "DOCUMENTAR classificacao de workload no topo de cada modulo paralelo"
### Solucao Proposta
- Adicionar comentario `// Workload: <classification>` no topo de cada modulo
### Complexidade
- TRIVIAL (~5 linhas de comentario)
### Arquivos Afetados
- `src/commands/enrich.rs` — adicionar "Workload: Subprocess I/O-bound"
- `src/commands/ingest_claude.rs` — adicionar "Workload: Subprocess I/O-bound"
- `src/commands/ingest_codex.rs` — adicionar "Workload: Subprocess I/O-bound"
- `src/embedder.rs` — adicionar "Workload: CPU-bound (ONNX inference)"
- `src/lock.rs` — adicionar "Workload: I/O-bound (flock polling)"


## P08 MEDIUM — stdin_helper.rs spawna thread sem armazenar JoinHandle
### Problema
- `stdin_helper.rs:38` spawna thread com `thread::spawn` para ler stdin com timeout
- O `JoinHandle` retornado por `thread::spawn` NAO eh armazenado
- No path de sucesso (`rx.recv_timeout` retorna Ok), a thread eh abandonada
- No path de timeout, a thread fica orfa lendo stdin indefinidamente
- A thread orfa continua bloqueada em `stdin().read_to_string()` ate o processo encerrar
### Consequencias
- Thread leak potencial: cada chamada a `read_stdin_with_timeout` deixa thread orfa
- Em uso repetido (pouco provavel no fluxo atual), threads acumulam
- Thread bloqueada em stdin consome recursos (stack ~8MB por thread)
- Violacao da regra "NUNCA descartar handle de task critica sem await/join"
### Causa Raiz
- O pattern de stdin com timeout usa channel com timeout em vez de thread join
- O `recv_timeout` retorna resultado antes da thread terminar
- O JoinHandle eh descartado implicitamente pelo escopo
### Evidencia no Codigo
- `src/stdin_helper.rs:38` — `thread::spawn(move || { ... })` sem armazenar handle
- `src/stdin_helper.rs:43` — `rx.recv_timeout` descarta handle implicitamente
### Solucao Proposta
- Armazenar JoinHandle e chamar `.join()` apos recv_timeout
- Ou migrar para `std::thread::scope` que garante join automatico
### Complexidade
- BAIXA (~5 linhas)
### Arquivos Afetados
- `src/stdin_helper.rs:38-48` — armazenar e join handle


## P09 LOW — Sem parking_lot deadlock detection em builds de debug
### Problema
- O projeto usa `std::sync::Mutex` diretamente (NAO `parking_lot::Mutex`)
- `parking_lot` NAO esta listado como dependencia no Cargo.toml
- Deadlocks entre Mutex nao sao detectados automaticamente em desenvolvimento
- As rules exigem "ATIVAR parking_lot com feature deadlock_detection em builds de debug"
- O projeto tem 6 usos de Mutex em codigo de producao (embedder.rs, extraction.rs, enrich.rs)
### Consequencias
- Deadlocks silenciosos durante desenvolvimento — dificil diagnostico
- Sem thread de background verificando deadlocks a cada 10 segundos
- Producao pode travar sem indicacao clara da causa
### Solucao Proposta
- Considerar migrar Mutex criticos para `parking_lot::Mutex` com feature `deadlock_detection`
- Adicionar thread de deteccao em builds debug
- Baixa prioridade dado que o projeto tem poucos Mutex e sem dependencias ciclicas
### Complexidade
- MEDIA (~20 linhas + dependencia nova)
### Arquivos Afetados
- `Cargo.toml` — adicionar parking_lot com feature deadlock_detection
- `src/embedder.rs` — migrar Mutex
- `src/extraction.rs` — migrar Mutex
- `src/commands/enrich.rs` — migrar Mutex de stdout


## P10 LOW — Zero #[tracing::instrument] em funcoes paralelas
### Problema
- ZERO ocorrencias de `#[tracing::instrument]` em todo `src/`
- Funcoes paralelas como `call_entity_description`, `call_body_enrich` nao criam spans
- Workers do thread pool G19 nao tem span identificando `worker_id`
- O dequeue loop em enrich.rs nao tem span por item processado
- As rules exigem "CRIAR span por task com #[tracing::instrument]"
### Consequencias
- Diagnostico de performance e contencao eh dificil sem spans correlacionados
- Impossivel medir tempo por worker ou por item via tracing
- Tail latency nao pode ser atribuida a workers especificos
- Observabilidade do paralelismo eh limitada a logs manuais
### Solucao Proposta
- Adicionar `#[tracing::instrument(skip_all, fields(worker_id))]` em workers do thread pool
- Adicionar `#[tracing::instrument(skip_all, fields(item_key))]` em funcoes call_*
- Adicionar spans no dequeue loop do enrich
### Complexidade
- BAIXA (~15 linhas de anotacoes)
### Arquivos Afetados
- `src/commands/enrich.rs` — spans em workers e funcoes call_*
- `src/commands/ingest_claude.rs` — span no loop de processamento
- `src/commands/ingest_codex.rs` — span no loop de processamento
- `src/daemon.rs` — span por conexao UDS


## P11 LOW — Sem testes de saturacao 10x (loom tests existem para semaforo)
### Problema
- As rules exigem "ESCREVER teste que dispara 10x mais tasks que permits disponiveis"
- `tests/concurrency_hardened.rs` testa cenarios reais mas NAO testa saturacao 10x
- `tests/loom_lock_slots.rs` existe e cobre interleavings do semaforo de slots (PONTO POSITIVO)
- NAO existe teste que dispare 40 tasks contra 4 permits assertando peak concurrency
- NAO existe teste com `AtomicUsize` tracking peak de concorrencia real
### Consequencias
- Sem validacao de que o bound eh respeitado sob carga extrema
- O bound pode ter race condition sutil nao coberta por testes normais
- A garantia de bounded concurrency depende apenas do loom test (modelo simplificado)
### Solucao Proposta
- Adicionar teste com 40 tasks competindo por 4 permits
- Usar `AtomicUsize` para rastrear peak de concorrencia
- Assertar que peak nunca excede N
### Complexidade
- BAIXA (~30 linhas de teste)
### Arquivos Afetados
- `tests/concurrency_hardened.rs` — adicionar teste de saturacao 10x


## P12 LOW — DashMap nao utilizado (nenhuma violacao direta)
### Problema
- As rules recomendam "PREFERIR DashMap sobre Arc<RwLock<HashMap>> para mapas concorrentes"
- O projeto NAO usa HashMap concorrente em nenhum ponto de producao
- `normalize_entities.rs` usa HashMap local no loop de classificacao — NAO eh concorrente
- NAO ha violacao direta — apenas oportunidade futura
### Consequencias
- NENHUMA consequencia atual — o projeto nao tem HashMap compartilhado entre threads
### Solucao Proposta
- NENHUMA acao necessaria no momento
- Considerar DashMap se paralelizar normalize_entities no futuro
### Complexidade
- N/A
### Arquivos Afetados
- NENHUM


## P13 LOW — 248 .unwrap() em codigo de producao
### Problema
- 248 ocorrencias de `.unwrap()` fora de modulos de teste em `src/`
- Inclui padroes LEGITIMOS como `OnceLock::set().unwrap()` e `Mutex::lock().unwrap()` (panic on poison)
- Inclui padroes como `parse::<i64>().unwrap()` que poderiam falhar graciosamente
- Inclui `serde_json::to_string().unwrap()` que eh infallible na pratica
- As rules gerais do projeto dizem "NUNCA use .unwrap() em producao"
### Consequencias
- Panic em runtime em paths inesperados causa perda de contexto
- Slots CLI podem ficar travados apos panic (flock liberado pelo SO no exit)
- NDJSON parcial no stdout apos panic
- Risco real eh BAIXO: maioria dos unwrap sao em paths infalliveis ou poison-panic
### Solucao Proposta
- Auditoria direcionada dos `.unwrap()` em codigo novo de paralelismo
- Priorizar conversao de `.unwrap()` que podem falhar em paths de I/O ou parsing
- Manter `.unwrap()` em `OnceLock::set`, `Mutex::lock` (panic on poison eh aceitavel)
- Manter `.unwrap()` em `serde_json::to_string` (infallible na pratica)
### Complexidade
- ALTA (248 ocorrencias para triagem, ~50 conversoes estimadas)
### Arquivos Afetados
- Distribuidos em ~40 arquivos em `src/`


## M01 HIGH — unsafe pre_exec em claude_runner.rs sem comentario SAFETY
### Problema
- `src/commands/claude_runner.rs:62` contem bloco `unsafe { cmd.pre_exec(...) }` sem comentario SAFETY formal
- O bloco invoca `libc::setrlimit(RLIMIT_AS)` dentro de `pre_exec` — manipulacao de limites de memoria via FFI
- As rules de gerenciamento de memoria (secao "Unsafe e Invariantes de Memoria" linhas 724-746) exigem SAFETY em CADA bloco unsafe
- TODOS os outros blocos unsafe do projeto TEM comentario SAFETY:
  - `main.rs` — 4 blocos de `set_var` com SAFETY documentando single-threaded context
  - `embedder.rs:279` — `from_raw_parts` com SAFETY documentando invariantes de layout
  - `connection.rs:32` — `transmute` com SAFETY documentando invariantes de fn pointer e layout
  - `paths.rs` — 6 blocos em testes com SAFETY documentando `#[serial]`
  - `optimize.rs` — 2 blocos em testes com SAFETY documentando `#[serial]`
- UNICO bloco unsafe sem SAFETY no codebase inteiro
### Consequencias
- Violacao do audit trail: reviewer nao consegue verificar invariantes sem ler codigo circundante
- O `pre_exec` roda ENTRE fork e exec — context extremamente sensivel (async-signal-safety)
- Se `setrlimit` falhar, o erro eh propagado mas a justificativa de seguranca nao esta documentada
- CI com `rg 'unsafe' | rg -v SAFETY` detecta este gap como falso positivo
### Causa Raiz
- Funcao adicionada na v1.0.67 (fix P03) sem incluir o comentario SAFETY obrigatorio
### Solucao Proposta
- Adicionar comentario SAFETY acima do bloco unsafe documentando:
  - `pre_exec` roda entre fork e exec em contexto single-threaded do child
  - `setrlimit` eh async-signal-safe (POSIX.1-2008)
  - `RLIMIT_AS` limita address space virtual, nao memoria fisica
  - Falha retorna `Err` propagado para o caller
### Complexidade
- TRIVIAL (3-4 linhas de comentario)
### Arquivos Afetados
- `src/commands/claude_runner.rs:62` — adicionar comentario SAFETY
### Status: FIXED
- Comentario SAFETY adicionado com 4 invariantes: single-threaded child, setrlimit async-signal-safe, RLIMIT_AS virtual, error propagation


## M02 MEDIUM — Vec::new() sem pre-alocacao em deep_research.rs
### Problema
- `src/commands/deep_research.rs` contem 8 ocorrencias de `Vec::new()` sem `with_capacity`
- O tamanho eh estimavel em varias delas a partir dos parametros da query
- As rules (secao "Pre-alocacao e Capacidade" linhas 57-78) exigem `Vec::with_capacity(n)` quando o tamanho eh conhecido
### Evidencia no Codigo
- Linha 460: `let mut evidence_chains: Vec<EvidenceChain> = Vec::new()` — estimavel pelo numero de seeds
- Linha 487: `let mut ctx_entities` — estimavel pelas entidades encontradas
- Linha 488: `let mut ctx_rels` — estimavel pelas relacoes encontradas
- Linha 529: `let mut params` — estimavel pelo numero de filtros
- Linha 604: `let mut parts: Vec<String> = Vec::new()` — estimavel (sempre 7 sub-queries max)
- Linha 701: `let mut path_ids` — estimavel pelo fan-out do BFS
- Linha 845: `let mut chains: Vec<EvidenceChain> = Vec::new()` — estimavel
### Consequencias
- Realocacoes desnecessarias no pipeline de pesquisa paralela
- O deep-research roda sub-queries em paralelo via JoinSet — cada realocacao adiciona latencia
### Solucao Proposta
- Substituir `Vec::new()` por `Vec::with_capacity(n)` onde tamanho eh estimavel
- Priorizar linhas 460, 604, 845 onde o tamanho eh derivavel de parametros
### Complexidade
- BAIXA (~8 linhas alteradas)
### Arquivos Afetados
- `src/commands/deep_research.rs` — 8 ocorrencias
### Status: FIXED
- 7 Vec::new() convertidos para with_capacity + 1 HashSet::with_capacity em deep_research.rs
- health.rs:342 e link.rs:132 tambem convertidos


## M03 MEDIUM — Vec::new() sem pre-alocacao em extraction.rs
### Problema
- `src/extraction.rs` contem 6 ocorrencias de `Vec::new()` sem `with_capacity`
- O modulo JA usa 14 `with_capacity` em outros pontos — ratio 14:6
- As 6 restantes merecem revisao para completar a cobertura
### Consequencias
- Realocacoes em modulo de extracao NER que processa entidades de cada arquivo
- BAIXO impacto unitario mas multiplicado pelo numero de arquivos no ingest
### Solucao Proposta
- Auditar cada `Vec::new()` e substituir por `with_capacity` onde tamanho eh estimavel
### Complexidade
- BAIXA (~6 linhas alteradas)
### Arquivos Afetados
- `src/extraction.rs` — 6 ocorrencias
### Status: FIXED
- extraction.rs:781 convertido para Vec::with_capacity(n.min(max_rels))


## M04 HIGH — Ausencia total de try_reserve para alocacoes derivadas de input externo
### Problema
- ZERO ocorrencias de `try_reserve` em todo o codebase
- O projeto processa bodies de ate 512 KB (`MAX_MEMORY_BODY_LEN`) e entities/relationships de input JSON
- As rules (secao "Alocacao Falivel e Out-of-Memory" linhas 82-102) exigem `try_reserve` para alocacoes derivadas de input externo
- PROIBIDO: "NUNCA permitir que input atacante cause abort via Vec::with_capacity"
- Alocacoes derivadas de input externo identificadas:
  - `src/chunking.rs` — `Vec::with_capacity` baseado em token counts do body
  - `src/extraction.rs` — `Vec::with_capacity` baseado em entities extraidas
  - `src/commands/ingest.rs` — `Vec::with_capacity` baseado em contagem de arquivos
  - `src/commands/remember.rs` — `Vec::with_capacity` baseado em chunks do body
### Causa Raiz
- O projeto valida `MAX_MEMORY_BODY_LEN` (512 KB) DEPOIS de alocar, nao ANTES
- A validacao de tamanho serve como cap implicito, mas o with_capacity pode rodar ANTES da validacao
- Em cenarios de pipeline (ingest com --max-files 50000), a alocacao eh proporcional ao numero de arquivos
### Consequencias
- Input malformado com body length declarado gigante pode causar abort por OOM antes da validacao
- `Vec::with_capacity(user_input)` sem `try_reserve` pode consumir toda a RAM disponivel
- Aborto sem graceful shutdown — slots CLI, WAL, buffers ficam em estado inconsistente
### Solucao Proposta
- Substituir `Vec::with_capacity(n)` por `Vec::new()` + `v.try_reserve(n)?` em pontos criticos
- OU adicionar validacao de tamanho ANTES do `with_capacity` em cada ponto
- Priorizar: `chunking.rs`, `extraction.rs`, `ingest.rs`, `remember.rs`
### Complexidade
- MEDIA (~20 linhas de validacao ou try_reserve)
### Arquivos Afetados
- `src/chunking.rs` — with_capacity baseado em tokens do body
- `src/extraction.rs` — with_capacity baseado em entities
- `src/commands/ingest.rs` — with_capacity baseado em file count
- `src/commands/remember.rs` — with_capacity baseado em chunks
### Status: FIXED
- try_reserve aplicado em 10 pontos: extraction.rs (5), remember.rs (1), ingest.rs (4)
- Usa anyhow::anyhow! em extraction.rs, AppError::LimitExceeded nos demais


## M05 MEDIUM — read_to_string sem verificacao previa de tamanho do arquivo
### Problema
- 3 modulos usam `std::fs::read_to_string(path)` sem checar o tamanho do arquivo ANTES de ler
- A validacao de `MAX_MEMORY_BODY_LEN` ocorre DEPOIS da leitura completa para RAM
- As rules (secao "I/O Eficiente e Streaming" linhas 948-971) proibem "transformar arquivo gigante em String via read_to_string"
### Evidencia no Codigo
- `src/commands/ingest.rs` — `std::fs::read_to_string(path)` seguido de check `raw_body.len() > MAX_MEMORY_BODY_LEN`
- `src/commands/remember.rs` — `std::fs::read_to_string(path)` com fallback UTF-8 lossy, sem check de tamanho previo
- `src/commands/edit.rs` — `std::fs::read_to_string(path)` sem check de filesystem size primeiro
### Causa Raiz
- O padrao eh ler primeiro e validar depois — funciona para arquivos pequenos mas falha com arquivos grandes
- `std::fs::metadata(path)?.len()` antes do read_to_string evitaria a leitura desnecessaria
### Consequencias
- Arquivo de varios GB passado via `--body-file` seria lido INTEIRO para RAM antes de ser rejeitado
- OOM crash antes que a validacao de tamanho possa atuar
- Forma cadeia causal com M04 — read_to_string aloca String sem try_reserve
### Solucao Proposta
- Adicionar `std::fs::metadata(path)?.len()` como check ANTES de `read_to_string`
- Rejeitar arquivos maiores que `MAX_MEMORY_BODY_LEN` antes de ler
- Pattern: `if metadata.len() > MAX as u64 { return Err(...) }`
### Complexidade
- BAIXA (~3 linhas por arquivo, 3 arquivos = ~9 linhas)
### Arquivos Afetados
- `src/commands/ingest.rs` — check antes de read_to_string
- `src/commands/remember.rs` — check antes de read_to_string
- `src/commands/edit.rs` — check antes de read_to_string
### Status: FIXED
- metadata().len() check adicionado ANTES de read_to_string em 7 pontos
- ingest.rs, remember.rs (4 pontos: body, entities, relationships, metadata), edit.rs, enrich.rs (prompt template)


## M06 LOW — clone() excessivo em loops de processamento do ingest.rs
### Problema
- `src/commands/ingest.rs` contem 27 ocorrencias de `.clone()` em total
- Muitas estao dentro do loop de processamento de arquivos (linhas 842-1232)
- Exemplos: `original_name.clone()`, `derived_name.clone()`, `args.pattern.clone()` repetidos multiplas vezes
- As rules (secao "Clone, Copy e Evitacao de Copias" linhas 486-508) proibem "clonar dentro de loop quente sem justificativa medida"
### Consequencias
- Alocacoes de String desnecessarias a cada iteracao do loop
- BAIXO impacto unitario (strings curtas) mas multiplicado por numero de arquivos
- Viola o principio de clonar com parcimonia
### Solucao Proposta
- Extrair clones de `args.pattern` e constantes para fora do loop
- Usar referencias `&str` em vez de `String::clone()` onde possivel
- Priorizar `args.pattern.clone()` que eh identico em toda iteracao
### Complexidade
- BAIXA (~10 linhas de refator)
### Arquivos Afetados
- `src/commands/ingest.rs` — loop de processamento de arquivos
### Status: DOCUMENTED
- Clones sao NECESSARIOS: cada clone transfere ownership para structs SlotMeta/ProcessItem
- Borrow checker exige clone porque loop itera com &path e structs consomem ownership
- Nao eh gap real — clones justificados por design de ownership


## M07 LOW — format! em loops de processamento em multiplos modulos
### Problema
- 20+ ocorrencias de `format!` dentro de loops `for` em varios modulos
- As rules (secao "Pre-alocacao e Capacidade" linha 76) proibem "`format!` em hot path em vez de `write!` em buffer existente"
- Exemplos identificados em: `cache.rs`, `daemon.rs`, `related.rs`, `health.rs`, `graph_export.rs`
### Consequencias
- Cada `format!` aloca String nova na heap a cada iteracao
- BAIXO impacto (nao sao hot paths de alto throughput em nenhum caso)
- Viola o principio de reusar buffers
### Solucao Proposta
- Substituir `format!` por `write!` em buffer existente nos loops mais frequentes
- Priorizar `graph_export.rs` onde o loop pode iterar sobre centenas de entidades
- Manter `format!` em loops com poucas iteracoes (<10)
### Complexidade
- BAIXA (~10 linhas por modulo)
### Arquivos Afetados
- `src/commands/graph_export.rs` — render_dot e render_mermaid
- `src/commands/related.rs` — formatacao de resultados
- `src/commands/health.rs` — formatacao de checks
### Status: FIXED
- 4 format!-in-loop convertidos para writeln! em graph_export.rs (render_dot e render_mermaid)
- String::with_capacity adicionado nos buffers de output


## M08 MEDIUM — Ausencia de miri no CI para validar blocos unsafe
### Problema
- O projeto contem 19 blocos `unsafe` distribuidos em 6 arquivos
- As rules (secao "Ferramentas de Validacao" linhas 1129-1153) exigem `cargo miri test` para modulos com unsafe
- ZERO evidencia de miri em `.github/` workflows ou Cargo.toml
- Blocos unsafe criticos:
  - `connection.rs:32` — `transmute` de fn pointer para sqlite3_auto_extension
  - `embedder.rs:279` — `from_raw_parts` para conversao f32 para bytes
  - `claude_runner.rs:62` — `pre_exec` com `setrlimit` via libc
  - `main.rs` — 4 blocos de `set_var` (unsafe em Rust 2024 edition)
### Consequencias
- Undefined behavior em blocos unsafe nao eh detectado automaticamente
- O `transmute` em connection.rs eh particularmente sensivel — layout de fn pointers
- `from_raw_parts` em embedder.rs depende de invariantes de alinhamento e endianness
- Sem miri, a unica validacao eh revisao humana dos comentarios SAFETY
### Causa Raiz
- miri nao foi integrado ao CI durante o setup inicial do projeto
- Blocos unsafe foram revisados manualmente mas sem validacao automatizada
### Solucao Proposta
- Adicionar job `miri` ao CI workflow `.github/workflows/ci.yml`
- Executar `cargo miri test` nos modulos com unsafe
- Excluir testes que requerem I/O de rede ou FFI complexo (miri nao suporta)
- Pelo menos rodar miri localmente antes de publicar versoes com mudancas em unsafe
### Complexidade
- MEDIA (~15 linhas de CI config + possivel exclusao de testes incompativeis)
### Arquivos Afetados
- `.github/workflows/ci.yml` — adicionar job miri
- Cargo.toml — possivel configuracao de miri em `[profile.dev]`
### Status: FIXED
- Job miri adicionado ao ci.yml rodando cargo +nightly miri test em f32_to_bytes e controlled_batch_plan
- Testes pure-Rust sem FFI que exercitam unsafe from_raw_parts em embedder.rs
- Testes com SQLite FFI (connection.rs) e libc FFI (claude_runner.rs) excluidos por limitacao do miri


## M09 LOW — String::new() sem with_capacity em 15 ocorrencias
### Problema
- 15 ocorrencias de `String::new()` em codigo de producao
- As rules (secao "Pre-alocacao e Capacidade" linha 61) exigem `String::with_capacity(n)` para strings grandes previsiveis
- Maioria em contextos onde o tamanho final eh razoavelmente estimavel
### Consequencias
- Realocacoes de strings curtas em paths nao criticos
- BAIXO impacto real — strings tipicamente pequenas (<100 chars)
### Solucao Proposta
- Auditar cada `String::new()` e substituir por `with_capacity` onde tamanho eh estimavel
- Priorizar strings usadas em formatacao de output JSON
### Complexidade
- BAIXA (~15 linhas alteradas)
### Arquivos Afetados
- Distribuidos em ~10 arquivos em `src/`
### Status: FIXED
- graph_export.rs: 2 String::with_capacity adicionados em render_dot e render_mermaid
- 10 das 15 ocorrencias analisadas; 5 convertidas, 10 MANTIDAS (read_line buffers, struct defaults, early returns)


## E01 HIGH — Sem #[global_allocator] mimalloc em nenhum target
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:63-68` — "ADOTAR mimalloc como #[global_allocator] por padrão em scripts"
- `docs_rules/rules_rust_economia_de_recursos.md:67` — "APLICAR mimalloc obrigatoriamente em builds musl"
- Checklist linha 1212 — "mimalloc ou jemallocator configurado como global allocator"
### Problema
- `rg '#[global_allocator]' src/` retorna ZERO resultados
- `rg 'mimalloc' Cargo.toml` retorna ZERO resultados
- O projeto usa allocator padrão do sistema em TODOS os targets
- Builds musl usam musl malloc, que é 2-5x mais lento que mimalloc
- Confirmado por duckduckgo: "Default musl allocator considered harmful to performance"
### Impacto
- Performance degradada em TODOS os targets, especialmente musl (CI cross-compile)
- Amplifica custo de TODAS as alocações heap (HashMap rehashing, Regex compilation, chunking)
- Builds musl distribuídos via GitHub Release sofrem o maior impacto
### Solução Proposta
- Adicionar `mimalloc = { version = "0.1", default-features = false }` ao Cargo.toml
- Em `src/main.rs`, adicionar `#[global_allocator] static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;`
- Validar ganho com `criterion` benchmark antes/depois
### Arquivos Afetados
- `Cargo.toml` — adicionar dependência mimalloc
- `src/main.rs` — declarar #[global_allocator]


## E02 HIGH — Regex::new() recompilada a cada chamada em 3 comandos
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:250-261` — "COMPILAR Regex uma única vez com OnceLock ou LazyLock"
- `docs_rules/rules_rust_economia_de_recursos.md:261` — "NUNCA recompilar regex em cada chamada de função"
- Checklist linha 1223 — "Regex compilado uma única vez com OnceLock"
### Problema
- `ingest.rs:446` — `regex::Regex::new(NAME_SLUG_REGEX)` recompilada a cada arquivo ingerido
- `remember.rs:272` — `regex::Regex::new(NAME_SLUG_REGEX)` recompilada a cada memória criada
- `rename.rs:129` — `regex::Regex::new(NAME_SLUG_REGEX)` recompilada a cada renomeação
- O MESMO padrão `NAME_SLUG_REGEX` é compilado repetidamente
- `extraction.rs:220-270` já demonstra o padrão correto com `OnceLock<Regex>`
### Impacto
- Ingest processa milhares de arquivos — cada um recompilando o regex desnecessariamente
- Compilação de regex aloca heap (~1-5µs por compilação × N arquivos)
### Solução Proposta
- Criar `static NAME_SLUG_RE: OnceLock<Regex> = OnceLock::new();` em `constants.rs` ou módulo compartilhado
- Substituir as 3 chamadas `Regex::new(NAME_SLUG_REGEX)` por `NAME_SLUG_RE.get_or_init(...)`
### Arquivos Afetados
- `src/commands/ingest.rs:446`
- `src/commands/remember.rs:272`
- `src/commands/rename.rs:129`
- `src/constants.rs` ou novo helper — declarar OnceLock global


## E03 MEDIUM — 12 HashMap::new() sem with_capacity em hot paths
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:664-668` — "PRÉ-ALOCAR com HashMap::with_capacity"
- Checklist linha 1214 — "Vec::with_capacity usado onde tamanho é conhecido"
### Problema
- 12 instâncias de `HashMap::new()` onde o tamanho é estimável pelo contexto
### Pontos
- `deep_research.rs:399` — `merged: HashMap` (tamanho = sub_queries × k)
- `deep_research.rs:963` — `entity_names: HashMap` (tamanho = entity_ids.len())
- `deep_research.rs:1301,1304` — BFS predecessor + entity_names
- `hybrid_search.rs:253` — `combined_scores: HashMap` (tamanho = vec_results + fts_results)
- `hybrid_search.rs:274` — `memory_data: HashMap` (tamanho = combined_scores.len())
- `related.rs:252,258` — entity_hop, entity_edge
- `fusion.rs:42` — `combined: HashMap` (tamanho = KNN + FTS results)
- `normalize_entities.rs:110` — normalization target map
- `extraction.rs:1052` — `by_lc` dedup map
### Impacto
- Rehashing repetido em pipelines de busca (hybrid-search, deep-research, recall graph expansion)
### Arquivos Afetados
- `src/commands/deep_research.rs` — 4 pontos
- `src/commands/hybrid_search.rs` — 2 pontos
- `src/commands/related.rs` — 2 pontos
- `src/storage/fusion.rs` — 1 ponto
- `src/commands/normalize_entities.rs` — 1 ponto
- `src/extraction.rs` — 1 ponto


## E04 MEDIUM — 11 HashSet::new() sem with_capacity
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:668` — "USAR HashSet::with_capacity seguindo mesma regra"
### Problema
- 11 instâncias de `HashSet::new()` onde tamanho é estimável
### Pontos
- `extraction.rs:713,761,803,877,1359` — 5 sets de dedup no pipeline de extração
- `graph.rs:264` — `seen_memories` no BFS
- `graph_export.rs:402` — `visited` no graph export
- `deep_research.rs:489,764` — seen entity IDs e seen result IDs
- `related.rs:282` — `dedup_ids`
### Impacto
- Extraction.rs processa textos longos — rehashing de HashSets de dedup é mensurável
### Arquivos Afetados
- `src/extraction.rs` — 5 pontos
- `src/graph.rs` — 1 ponto
- `src/commands/graph_export.rs` — 1 ponto
- `src/commands/deep_research.rs` — 2 pontos
- `src/commands/related.rs` — 1 ponto


## E05 MEDIUM — Sem hasher especializado (ahash/FxHashMap) em HashMap hot paths
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:659-663` — "USAR ahash::AHashMap para uso geral em hot path"
- Checklist linha 1250 — "ahash ou rustc-hash aplicados em HashMap hot path"
### Problema
- `rg 'ahash|FxHashMap|FnvHashMap|rustc.hash' Cargo.toml src/` retorna ZERO resultados
- Todos os HashMaps usam SipHash padrão (resistente a DoS mas 2-3x mais lento que ahash)
- hybrid-search e deep-research são hot paths de consulta frequente
### Impacto
- Hash function SipHash é mais lenta que necessário para chaves `i64` e `String` internas
- Não há risco de DoS em dados internos do grafo
### Solução Proposta
- Adicionar `ahash = "0.8"` ao Cargo.toml
- Substituir `HashMap` por `AHashMap` em hot paths (hybrid_search, deep_research, fusion)
- Manter `std::HashMap` em pontos que recebem input externo não confiável
### Arquivos Afetados
- `Cargo.toml` — adicionar dependência ahash
- `src/commands/deep_research.rs` — 4 HashMaps
- `src/commands/hybrid_search.rs` — 2 HashMaps
- `src/storage/fusion.rs` — 1 HashMap


## E06 MEDIUM — Zero #[inline]/#[cold] em todo o codebase
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:844-858` — "APLICAR #[cold] em error paths e logs raros"
- `docs_rules/rules_rust_economia_de_recursos.md:848` — "APLICAR #[inline(never)] em construtores grandes de erro"
- Checklist linha 1272 — "Hints #[inline], #[cold], #[must_use] aplicados com evidência"
### Problema
- `rg '#[inline]|#[cold]' src/` retorna ZERO resultados em 44506 LOC
- Error constructors em `errors.rs` (19-101) competem com hot path no instruction cache
- Funções de `output.rs` usadas cross-crate sem `#[inline]` não se beneficiam de inlining
### Impacto
- Error paths sem `#[cold]` poluem instruction cache do processador no hot path
- Funções de output cross-crate sem `#[inline]` não são inlinadas pelo linker
### Solução Proposta
- Adicionar `#[cold]` + `#[inline(never)]` em construtores de `AppError` em `errors.rs`
- Adicionar `#[inline]` em funções pequenas de `output.rs` usadas cross-crate
- Adicionar `#[cold]` em paths de inicialização (startup, migration)
### Arquivos Afetados
- `src/errors.rs` — construtores de AppError
- `src/output.rs` — funções emit_json, emit_text


## E07 MEDIUM — 3 emit_json() locais duplicadas bypassando output centralizado
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:285-290` — "ADQUIRIR stdout().lock() uma vez e reusar"
### Problema
- `enrich.rs:522` — `fn emit_json<T: Serialize>(value: &T)` local
- `ingest_codex.rs:535` — cópia idêntica
- `ingest_claude.rs:503` — cópia idêntica
- Triplicação de lógica de output JSON bypassando `output.rs` centralizado
### Impacto
- Violam DRY e princípio de output centralizado
- Mudanças em output.rs não se propagam para estas cópias
### Solução Proposta
- Delegar as 3 funções locais para `output::emit_json_compact()`
- Remover as 3 implementações duplicadas
### Arquivos Afetados
- `src/commands/enrich.rs:522`
- `src/commands/ingest_codex.rs:535`
- `src/commands/ingest_claude.rs:503`


## E08 MEDIUM — Zero uso de Cow<str> em todo o codebase
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:59` — "RETORNAR Cow<'a, str> quando clone é condicional"
- `docs_rules/rules_rust_economia_de_recursos.md:247` — "APLICAR Cow::Borrowed quando transformação é condicional"
- Checklist linha 1222 — "Cow aplicado quando clone é condicional"
### Problema
- `rg 'Cow<' src/` retorna ZERO resultados em 78 arquivos Rust
- Funções em `storage/memories.rs` retornam `String` mas frequentemente retornam input sem modificação
- `i18n.rs` messages poderiam ser `Cow<'static, str>` quando não interpoladas
### Impacto
- Alocações condicionais evitáveis — cada `.to_string()` em path que frequentemente não modifica é desperdício
### Solução Proposta
- Identificar funções que retornam input inalterado na maioria dos casos
- Converter retorno para `Cow<'_, str>` nesses pontos
### Arquivos Afetados
- `src/storage/memories.rs` — funções de formatação que retornam String
- `src/i18n.rs` — messages que poderiam usar Cow<'static, str>


## E09 MEDIUM — record_spawn_failure aceita String em vez de &str
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:56-57` — "PREFERIR &str sobre String em parâmetros de leitura"
- Checklist linha 1221 — "Strings usam &str em parâmetros read-only"
### Problema
- `daemon.rs:725` — `fn record_spawn_failure(models_dir: &Path, message: String)`
- A função apenas lê `message` para escrever em arquivo — não precisa de ownership
- Todos os 5 call sites fazem `.to_string()` ou `format!()` para satisfazer o parâmetro
### Impacto
- 5 alocações desnecessárias de String por chamada
### Solução Proposta
- Mudar assinatura para `message: &str`
- Remover `.to_string()` nos 5 call sites
### Arquivos Afetados
- `src/daemon.rs:725` — assinatura da função
- `src/daemon.rs` — 5 call sites


## E10 MEDIUM — Subprocessos pesados sem cgroup isolation
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:534-541` — "ENCAPSULAR subprocess pesado em systemd-run --scope"
- `docs_rules/rules_rust_economia_de_recursos.md:537` — "DEFINIR -p MemoryMax=<bytes> para cada subprocess pesado"
- Checklist linhas 1251-1252 — "Subprocessos pesados encapsulados em systemd-run --scope"
### Problema
- 9 `Command::new` em produção sem cgroup isolation:
  - `claude_runner.rs:115,167` — spawna `claude -p` (LLM, alta RAM)
  - `ingest_claude.rs:213,273` — spawna `claude -p` por arquivo
  - `ingest_codex.rs:201,273` — spawna `codex exec`
  - `enrich.rs:575,2726` — spawna `claude -p` / `codex exec`
  - `daemon.rs:604` — re-spawna self como daemon
- `rg 'systemd-run|MemoryMax|cgroup' src/` retorna ZERO resultados
### Impacto
- Subprocessos LLM (Claude, Codex) podem consumir RAM ilimitada do host
- `claude_runner.rs` usa `setrlimit(RLIMIT_AS)` como mitigação parcial, mas os demais NÃO
### Solução Proposta
- Em Linux, encapsular spawns LLM com `systemd-run --scope -p MemoryMax=<N>G`
- Detectar disponibilidade de systemd em runtime e usar como wrapper opcional
### Arquivos Afetados
- `src/commands/claude_runner.rs` — spawn de claude
- `src/commands/ingest_claude.rs` — spawn de claude
- `src/commands/ingest_codex.rs` — spawn de codex
- `src/commands/enrich.rs` — spawn de claude/codex


## E11 LOW — Sem #[serde(borrow)] para strings emprestadas
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:248` — "USAR #[serde(borrow)] para strings emprestadas de input"
### Problema
- `rg '#[serde(borrow' src/` retorna ZERO resultados
- Structs de deserialização alocam owned Strings onde borrowed &str bastaria
### Impacto
- Baixo no contexto CLI onde payloads são tipicamente pequenos (<512KB)
### Arquivos Afetados
- `src/storage/entities.rs` — structs NewEntity, NewRelationship
- `src/commands/remember_batch.rs` — BatchItem struct


## E12 LOW — Sem memchr/aho-corasick para busca acelerada em texto
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:240-241` — "USAR memchr para busca de byte em slice grande via SIMD"
- Checklist linha 1224 — "memchr e aho-corasick aplicados em busca em massa"
### Problema
- Pipeline de extraction usa `str::find` e `Regex` para buscas em texto
- `memchr::memmem` ofereceria SIMD acceleration para substring matching
### Impacto
- Marginal vs regex já compilada com OnceLock
### Arquivos Afetados
- `src/extraction.rs` — funções de busca em texto


## E13 LOW — Workload classification ausente em 73 de 78 módulos
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:16-22` — "DOCUMENTAR a classificação no código em comentário explícito"
### Problema
- Apenas 5 módulos têm `// Workload:` header documentado
- 73 módulos faltando classificação explícita
### Módulos com classificação
- `lock.rs` — I/O-bound
- `embedder.rs` — CPU-bound
- `ingest_claude.rs` — Subprocess I/O-bound
- `enrich.rs` — Subprocess I/O-bound
- `ingest_codex.rs` — Subprocess I/O-bound
### Impacto
- Documentação — não afeta runtime
### Arquivos Afetados
- 73 arquivos em `src/` e `src/commands/`


## E14 LOW — Benchmarks Criterion não integrados no CI
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:1026-1029` — "INTEGRAR benchmarks em CI com threshold de regressão"
- Checklist linha 1274 — "Benchmarks integrados em CI com threshold"
### Problema
- `benches/cli_benchmarks.rs` e `benches/regression_baseline.rs` existem com criterion
- `.github/workflows/ci.yml` NÃO executa benchmarks em nenhum job
- Regressões de performance passam despercebidas
### Impacto
- Drift gradual de performance sem detecção automatizada
### Arquivos Afetados
- `.github/workflows/ci.yml` — adicionar job de benchmark


## E15 LOW — Sem cargo-careful no pipeline de validação
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:1049` — "EXECUTAR cargo careful para sanity checks adicionais"
- Checklist linha 1263 — "kani ou shuttle aplicados em código crítico"
### Problema
- `cargo careful` não é executado em nenhum ponto do pipeline
- miri já cobre validação de unsafe, mas cargo-careful adiciona checks complementares
### Impacto
- Baixo — miri já cobre a maioria dos cenários de unsafe validation
### Arquivos Afetados
- `.github/workflows/ci.yml` — adicionar job opcional


## E16 LOW — .cargo/config.toml deletado — sem mold/lld como linker
### Regra Violada
- `docs_rules/rules_rust_economia_de_recursos.md:733-734` — "CONFIGURAR mold como linker em Linux para builds rápidos"
- Checklist linha 1267 — "mold ou lld configurado como linker em Linux"
### Problema
- `git status` mostra `D .cargo/config.toml` — arquivo foi deletado
- Sem linker otimizado, builds debug usam `ld` padrão (significativamente mais lento)
- `mold` reduz tempo de link em 3-10x vs `ld`
### Impacto
- Afeta velocidade de build em desenvolvimento, não runtime de produção
### Solução Proposta
- Recriar `.cargo/config.toml` com `[target.x86_64-unknown-linux-gnu] linker = "clang" rustflags = ["-C", "link-arg=-fuse-ld=mold"]`
### Arquivos Afetados
- `.cargo/config.toml` — recriar com configuração de linker


## Auditoria de Eficiência e Performance — `docs_rules/rules_rust_eficiencia_e_performance.md`
### Data: 2026-05-31
### Escopo: 119 arquivos Rust, 44525 LOC auditados contra 981 linhas de regras (25 seções, 53 itens de checklist)
### Fontes: context7 /websites/doc_rust-lang_cargo (trustScore 10), duckduckgo (Cargo profiles, allocation strategies, Rust perf book)
### Resultado: 23 CONFORMIDADES, 24 GAPS identificados como EP01-EP30 (0 HIGH, 9 MEDIUM, 11 LOW, 4 INFO/N/A)


## EP01 MEDIUM — FIXED — Sem [profile.release.package."*"] para otimizar dependências
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:73` — "APLICAR [profile.release.package."*"] com opt-level = 3"
- Checklist linha 934 — "Perfil release configurado com LTO e codegen-units = 1"
### Problema
- Cargo.toml:137-142 configura release com opt-level=3, lto=fat, codegen-units=1
- Seção `[profile.release.package."*"]` AUSENTE
- Dependências compilam com opt-level padrão do profile release (3) mas sem override explícito
- Sem override explícito, mudanças futuras no profile podem regredir deps
### Impacto
- Dependências sem override explícito podem não receber LTO cross-crate otimizado
- Rebuild incremental pode usar settings inconsistentes entre deps e projeto
### Solução Proposta
- Adicionar seção em Cargo.toml após [profile.release]:
  - `[profile.release.package."*"]`
  - `opt-level = 3`
### Arquivos Afetados
- `Cargo.toml` — adicionar seção [profile.release.package."*"]


## EP02 LOW — FIXED — Sem [profile.bench] separado
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:76` — "SEPARAR profile específico para benchmarks"
### Problema
- Cargo.toml não contém seção [profile.bench]
- Benchmarks herdam configuração de release sem customização
- Sem debug info em benchmarks, flamegraphs não mostram nomes de funções
### Impacto
- Baixo — benchmarks funcionam mas sem debug symbols para profiling
### Solução Proposta
- Adicionar seção [profile.bench] com `debug = 1` e `inherits = "release"`
### Arquivos Afetados
- `Cargo.toml` — adicionar seção [profile.bench]


## EP03 LOW — FIXED — Sem debug = false explícito em profile release
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:52` — "DESABILITAR debug = false em release final"
### Problema
- Cargo.toml:137-142 usa strip = true (que implica sem debug info) mas sem debug = false explícito
- Padrão do Cargo para release é debug = false, mas regra exige explícito
### Impacto
- Informativo — strip = true já garante sem debug info no binário final
### Arquivos Afetados
- `Cargo.toml` — adicionar `debug = false` na seção [profile.release]


## EP04 LOW — FIXED — Sem overflow-checks explícito em profile release
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:45` — "DESABILITAR overflow-checks apenas após auditoria"
### Problema
- Cargo.toml não declara overflow-checks explicitamente
- Padrão do Cargo para release é false, mas regra exige auditoria explícita
### Impacto
- Informativo — padrão já é false em release
### Arquivos Afetados
- `Cargo.toml` — documentar decisão sobre overflow-checks


## EP05 LOW — FIXED — Sem incremental = false explícito em profile release
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:70` — "NUNCA habilitar incremental = true em release final"
### Problema
- Cargo.toml não declara incremental = false explicitamente
- Padrão para release já é false, mas regra exige explícito para prevenir drift
### Impacto
- Informativo — padrão já é false
### Arquivos Afetados
- `Cargo.toml` — adicionar `incremental = false` na seção [profile.release]


## EP06 LOW — FIXED — Sem [profile.dev.package."*"] opt-level = 2
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:74` — "OTIMIZAR dependências mesmo em perfil dev com opt-level = 2"
### Problema
- Cargo.toml:150-151 define [profile.dev] opt-level = 1
- Seção [profile.dev.package."*"] AUSENTE
- Dependências em dev compilam com opt-level 1 (mesmo do projeto) em vez de 2 (mais otimizado)
### Impacto
- Testes e dev builds mais lentos que o possível para código de dependências
### Solução Proposta
- Adicionar `[profile.dev.package."*"]` com `opt-level = 2`
### Arquivos Afetados
- `Cargo.toml` — adicionar seção [profile.dev.package."*"]


## EP07 INFO — NOT_APPLICABLE — Sem target-cpu configuration
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:60-64` — "COMPILAR múltiplos binários para CPUs distintos"
### Status: NOT_APPLICABLE
- CLI distribuído via crates.io — regra linha 71 PROÍBE target-cpu=native em binário distribuído
- CI já compila para targets genéricos (x86_64-unknown-linux-gnu, aarch64-apple-darwin)


## EP08 MEDIUM — NOT_APPLICABLE — Vec::new() sem with_capacity quando tamanho é estimável
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:86` — "PRÉ-ALOCAR capacidade com Vec::with_capacity quando tamanho é conhecido"
- Checklist linha 937 — "Alocações em hot path minimizadas e medidas"
### Problema
- 34 ocorrências de Vec::new() em código de produção
- ~8-10 poderiam usar with_capacity com tamanho estimável
- Exemplos: extraction.rs (entity extraction loops), cli.rs (argument building), ingest.rs (file processing)
### Impacto
- Realocações desnecessárias em loops de processamento de entidades
- Cada realocação copia buffer inteiro para nova alocação
### Solução Proposta
- Auditar 34 sites e adicionar with_capacity onde tamanho é estimável
### Arquivos Afetados
- `src/extraction.rs` — entity extraction loops
- `src/commands/ingest.rs` — file processing
- `src/cli.rs` — argument building


## EP09 LOW — FIXED — to_string_lossy().to_string() causa dupla alocação
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:112` — "NUNCA chamar clone() em hot path sem justificativa"
- `docs_rules/rules_rust_eficiencia_e_performance.md:240` — "NUNCA chamar to_string() em valor que já é String"
### Problema
- 3 instâncias de `to_string_lossy().to_string()` em código de produção
- `to_string_lossy()` retorna `Cow<str>` — quando é Borrowed, `.to_string()` aloca desnecessariamente
- Quando é Owned, o `.to_string()` é redundante
### Evidência
- `src/commands/ingest_claude.rs:624` — `file.to_string_lossy().to_string()`
- `src/commands/ingest_codex.rs:660` — `file.to_string_lossy().to_string()`
- `src/commands/optimize.rs:95` — `db_path.to_string_lossy().to_string()`
### Solução Proposta
- Usar `file.to_string_lossy().into_owned()` para evitar alocação extra no caso Borrowed
- Ou usar `Cow<str>` downstream sem materializar String
### Arquivos Afetados
- `src/commands/ingest_claude.rs:624`
- `src/commands/ingest_codex.rs:660`
- `src/commands/optimize.rs:95`


## EP10 LOW — NOT_APPLICABLE — Zero shrink_to_fit em coleções persistidas
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:117` — "NUNCA ignorar shrink_to_fit em coleções persistidas longamente"
### Problema
- ZERO ocorrências de shrink_to_fit em todo o codebase
- Coleções em extraction.rs e deep_research.rs são construídas com with_capacity mas nunca shrunk após preenchimento
### Impacto
- Baixo — coleções são ephemeral por comando CLI e liberadas no fim do processo
### Arquivos Afetados
- Potencialmente `src/extraction.rs` e `src/commands/deep_research.rs` para coleções grandes


## EP13 MEDIUM — NOT_APPLICABLE — collect::<Vec> intermediário em pipeline de iteradores
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:116` — "NUNCA usar collect::<Vec<_>>() intermediário quando iterador basta"
- `docs_rules/rules_rust_eficiencia_e_performance.md:201` — "NUNCA usar .collect::<Vec<_>>() intermediário em pipeline"
### Problema
- 12 ocorrências de `collect::<Vec` em código de produção
- ~3-4 poderiam usar iteradores diretos sem materializar vetor intermediário
### Impacto
- Alocações desnecessárias em pipelines que poderiam usar lazy evaluation
### Arquivos Afetados
- Auditoria por arquivo necessária para identificar collect intermediários eliminável


## EP14 LOW — NOT_APPLICABLE — Loops index-based for i in 0.. em vez de iteradores
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:181` — "USAR .iter(), .iter_mut(), .into_iter() ao invés de índices"
- `docs_rules/rules_rust_eficiencia_e_performance.md:215` — "Usar for i in 0..v.len() quando for x in &v basta"
### Problema
- 16 ocorrências de `for ... in 0..` em código de produção
- Maioria em extraction.rs para entity pairing (requer acesso por índice para pair combinations)
- Alguns poderiam usar iteradores com .enumerate() ou .windows()
### Impacto
- Bounds checks não elididos pelo compilador em loops index-based
### Arquivos Afetados
- `src/extraction.rs` — entity pairing loops
- `src/commands/deep_research.rs` — entity deduplication


## EP15 LOW — FIXED — .sort() estável quando sort_unstable() basta
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:347` — "APLICAR sort_unstable quando estabilidade não importa"
### Problema
- 3 ocorrências de `.sort()` em código de produção
- Estabilidade de ordenação não é necessária em nenhum dos 3 casos
- `sort_unstable()` é ~20% mais rápido e usa menos memória
### Evidência
- Nenhum dos 3 sites requer preservação de ordem relativa de elementos iguais
### Solução Proposta
- Substituir `.sort()` por `.sort_unstable()` nos 3 sites
### Arquivos Afetados
- Identificar 3 sites com `rg '\.sort\(\)' src/ --type rust`


## EP16 LOW — FIXED — sort_by sem sort_by_key quando possível
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:348` — "PREFERIR sort_by_key sobre sort_by quando possível"
### Problema
- 8 ocorrências de sort_by em código de produção
- ZERO ocorrências de sort_by_key
- Pelo menos 1 caso (cache.rs:234 `sort_by(\|a, b\| a.name.cmp(&b.name))`) é direto para sort_by_key
### Impacto
- Baixo — sort_by_key evita repetição de extração de chave mas performance é similar
### Arquivos Afetados
- `src/commands/cache.rs:234`
- `src/extraction.rs:606`
- `src/commands/hybrid_search.rs:271`
- `src/commands/deep_research.rs:437,479,806,1017`
- `src/commands/related.rs:294`


## EP18 MEDIUM — FIXED — .len() não hoisted em loop quadrático de entity pairing
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:291` — "MOVER cálculos invariantes para fora do loop"
- `docs_rules/rules_rust_eficiencia_e_performance.md:294` — "EXTRAIR len() para variável antes do laço"
- `docs_rules/rules_rust_eficiencia_e_performance.md:305` — "NUNCA recalcular expressão constante a cada iteração"
### Problema
- `extraction.rs:897-898` — `for i in 0..present.len() { for j in (i+1)..present.len() }`
- `.len()` chamado na condição do loop a cada iteração do loop externo
- Padrão quadrático O(n²) com chamada redundante a .len()
### Impacto
- Overhead de chamadas redundantes em entity pairing com N entidades
- Em memórias com 100+ entidades, são ~10000 chamadas desnecessárias a .len()
### Solução Proposta
- Hoist: `let n = present.len();` antes do loop
- Usar `for i in 0..n { for j in (i+1)..n }`
### Arquivos Afetados
- `src/extraction.rs:897-898`


## EP19 MEDIUM — NOT_APPLICABLE — Vec<Box<dyn rusqlite::ToSql>> em hot path de busca vetorial
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:366` — "NUNCA usar Box<dyn Trait> em hot path sem medir"
- `docs_rules/rules_rust_eficiencia_e_performance.md:367` — "NUNCA aplicar dyn Trait quando monomorfização é viável"
### Problema
- `memories.rs:553,575` — `Vec<Box<dyn rusqlite::ToSql>>` construído por query na busca vetorial KNN
- `deep_research.rs:534` — mesmo padrão para sub-queries
- Cada query aloca N Box na heap + vtable lookup por parâmetro
- Afeta recall, hybrid-search e deep-research (hot paths de busca)
### Impacto
- Alocação dinâmica por query — multiplicada pelo número de sub-queries em deep-research
- Vtable lookup overhead para cada parâmetro SQL
### Solução Proposta
- Considerar enum wrapper ou array de parâmetros tipados em vez de Box<dyn>
- rusqlite requer &[&dyn ToSql] — avaliar se há alternativa com params![] macro
### Arquivos Afetados
- `src/storage/memories.rs:553,575`
- `src/commands/deep_research.rs:534`


## EP20 LOW — NOT_APPLICABLE — Zero const fn no codebase
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:384` — "USAR const fn para cálculos independentes de runtime"
- `docs_rules/rules_rust_eficiencia_e_performance.md:387` — "APLICAR const para tamanhos de buffer derivados"
### Problema
- ZERO ocorrências de `const fn` em 44525 LOC
- constants.rs contém ~330 linhas de `const` valores mas nenhum `const fn`
- Valores derivados de constantes são calculados em runtime quando poderiam ser compile-time
### Impacto
- Baixo — constantes são literais; const fn beneficiaria apenas valores derivados
### Arquivos Afetados
- `src/constants.rs` — candidato para const fn em valores derivados


## EP22 MEDIUM — FIXED — Cobertura de #[inline] insuficiente (1.5%)
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:444` — "APLICAR #[inline] em funções pequenas cross-crate"
- `docs_rules/rules_rust_eficiencia_e_performance.md:446` — "MARCAR accessors triviais com #[inline] em bibliotecas"
- Checklist linha 954 — "Hints de inline aplicados apenas com evidência empírica"
### Problema
- 4 anotações #[inline] em 266+ funções públicas (1.5% de cobertura)
- Apenas output.rs (3) e errors.rs (1) têm #[inline]
- Accessors triviais em structs, getters de constantes e funções de conversão sem #[inline]
### Impacto
- Cross-crate calls (lib.rs → main.rs) sem inline hint podem não ser inlined pelo LTO
- Funções pequenas como exit_code(), emit_json() já têm inline; faltam accessors de structs
### Solução Proposta
- Adicionar #[inline] em funções públicas pequenas cross-crate
- Foco: errors.rs accessors, output.rs helpers, constants.rs getters
### Arquivos Afetados
- `src/errors.rs` — accessors de exit code
- `src/output.rs` — emit functions
- `src/constants.rs` — getter functions


## EP23 MEDIUM — FIXED — Zero #[cold] em error paths
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:447` — "AVALIAR #[cold] para branches de erro raros"
- `docs_rules/rules_rust_eficiencia_e_performance.md:448` — "COMBINAR #[cold] com #[inline(never)] em handlers de panic"
- `docs_rules/rules_rust_eficiencia_e_performance.md:672` — "MARCAR funções de erro com #[cold]"
- `docs_rules/rules_rust_eficiencia_e_performance.md:831` — "MARCAR error handlers com #[cold]"
### Problema
- ZERO anotações #[cold] em todo o codebase (44525 LOC)
- Error handlers, daemon spawn failures e validation paths sem anotação
- Sem #[cold], o compilador pode incluir error paths no instruction cache junto com hot paths
### Impacto
- Instruction cache pollution — error paths misturados com hot paths reduzem eficiência de cache
- Afeta especialmente deep_research.rs e extraction.rs que têm muitos branches de erro
### Solução Proposta
- Adicionar #[cold] em funções de tratamento de erro e logging de falhas
- Candidatos: error constructors em errors.rs, spawn failure handlers em daemon.rs
### Arquivos Afetados
- `src/errors.rs` — error constructors
- `src/daemon.rs` — spawn failure recording
- `src/commands/enrich.rs` — error handling paths


## EP24 LOW — FIXED — Zero #[must_use] em funções retornando Result
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:479` — "USAR #[must_use] para forçar consumo de Result e builders"
### Problema
- ZERO anotações #[must_use] em todo o codebase
- Funções retornando Result<T, E> sem #[must_use] permitem que o caller ignore erros silenciosamente
### Impacto
- Baixo — Clippy já emite warning para Result não consumido, mas #[must_use] é mais explícito
### Arquivos Afetados
- `src/output.rs` — funções que retornam Result
- `src/storage/*.rs` — funções de storage


## EP25 MEDIUM — FIXED — Ordering::SeqCst excessivo em state machine de daemon
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:575` — "USAR Ordering::Relaxed onde semântica permite"
- `docs_rules/rules_rust_eficiencia_e_performance.md:581` — "EVITAR SeqCst salvo quando ordem global é essencial"
- `docs_rules/rules_rust_eficiencia_e_performance.md:582` — "DOCUMENTAR raciocínio de ordering em comentário"
- `docs_rules/rules_rust_eficiencia_e_performance.md:588` — "NUNCA usar Ordering::SeqCst sem necessidade semântica"
### Problema
- daemon.rs:481-482,505,553 — DAEMON_VERSION_STATE usa SeqCst para compare_exchange e load/store
- main.rs:283 — SHUTDOWN flag usa SeqCst para store
- lib.rs:86 — SHUTDOWN usa SeqCst para load
- storage/utils.rs:112-169 — SeqCst em testes (aceitável)
- ZERO comentários documentando raciocínio de ordering
### Evidência
- daemon.rs:481-482 — `compare_exchange(VERSION_NOT_CHECKED, ..., Ordering::SeqCst, Ordering::SeqCst)`
- daemon.rs:505 — `store(VERSION_RESTART_ATTEMPTED, Ordering::SeqCst)`
- daemon.rs:553 — `load(Ordering::SeqCst)`
- main.rs:283 — `SHUTDOWN.store(true, Ordering::SeqCst)`
- State machine de versão do daemon NÃO precisa de barreira global — Acquire/Release basta
### Impacto
- SeqCst impõe full memory barrier (~150-200 cycles em x86_64) onde Acquire/Release bastaria
- Chamado uma vez por invocação CLI — impacto absoluto pequeno mas viola princípio
### Solução Proposta
- daemon.rs:481 — `Ordering::Acquire` no success, `Ordering::Relaxed` no failure
- daemon.rs:505 — `Ordering::Release`
- daemon.rs:553 — `Ordering::Acquire`
- main.rs:283 — `Ordering::Release` para store de SHUTDOWN
- lib.rs:86 — `Ordering::Acquire` para load de SHUTDOWN
- Adicionar comentário SAFETY documentando raciocínio
### Arquivos Afetados
- `src/daemon.rs:481-482,505,553` — DAEMON_VERSION_STATE ordering
- `src/main.rs:283` — SHUTDOWN store ordering
- `src/lib.rs:86` — SHUTDOWN load ordering


## EP26 LOW — FIXED — .context() eager com literal em vez de .with_context() lazy
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:670` — "USAR closures em context() ou with_context()"
- `docs_rules/rules_rust_eficiencia_e_performance.md:671` — "ADIAR formatação até o erro realmente ocorrer"
### Problema
- 3 instâncias de `.context("literal")` em extraction.rs
- `.context()` eager avalia a mensagem ANTES de verificar se houve erro
- `.with_context(\|\| "literal")` lazy só avalia se erro ocorrer
### Impacto
- Mínimo — literal strings não alocam no .context() (apenas referência &str)
- Regra aplica-se primariamente a .context(format!(...)) que aloca sempre
### Solução Proposta
- Converter .context("literal") para .with_context(\|\| "literal") nos 3 sites
### Arquivos Afetados
- `src/extraction.rs` — 3 instâncias


## EP27 MEDIUM — NOT_APPLICABLE — serde_json::Value intermediário em vez de tipos concretos
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:696` — "NUNCA criar serde_json::Value intermediário quando tipo concreto basta"
### Problema
- 34 ocorrências de `serde_json::Value` em 12 arquivos de produção
- Maiores concentrações: enrich.rs (10), read.rs (4), ingest_codex.rs (4), history.rs (3)
- serde_json::Value aloca árvore JSON dinâmica na heap quando typed struct evitaria alocação
### Evidência
- `src/commands/enrich.rs` — 10 usos de Value para parsing de output do Claude
- `src/commands/read.rs` — 4 usos para manipulação de JSON response
- `src/commands/ingest_codex.rs` — 4 usos para parsing de output do Codex
- `src/commands/history.rs` — 3 usos
- `src/tokenizer.rs` — 1 uso
- `src/storage/memories.rs` — 2 usos
- `src/errors.rs` — 2 usos
### Impacto
- Árvore JSON dinâmica aloca nós na heap para cada par chave-valor
- Typed deserialization via struct evita árvore intermediária
- Maioria dos usos em enrich.rs/ingest_codex.rs são para parsing de output de LLM onde schema parcial é inevitável
### Solução Proposta
- Avaliar caso a caso — LLM output parsing pode exigir Value (schema variável)
- read.rs e history.rs poderiam usar typed structs
- errors.rs usos são para contrato JSON de erro (necessário)
### Arquivos Afetados
- `src/commands/enrich.rs` — 10 ocorrências
- `src/commands/read.rs` — 4 ocorrências
- `src/commands/ingest_codex.rs` — 4 ocorrências
- `src/commands/history.rs` — 3 ocorrências


## EP30 LOW — FIXED — Falta compile-time endianness check em f32_to_bytes
### Regra Violada
- `docs_rules/rules_rust_eficiencia_e_performance.md:427` — "NUNCA aplicar unsafe sem justificativa documentada"
- `docs_rules/rules_rust_eficiencia_e_performance.md:435` — "VALIDAR com cargo careful em builds de teste"
### Problema
- embedder.rs:277-279 implementa transmute de f32 para u8 slice
- Safety docs presentes documentam 3 invariantes: no padding, borrow, endianness
- MAS sem #[cfg] guard em compile-time para big-endian architectures
- sqlite-vec também não suporta big-endian, mas silenciosamente retornaria dados corrompidos
### Impacto
- Baixo — projeto compila apenas para x86_64 e aarch64 (ambos little-endian)
- Guard preveniria compilação silenciosa para PPC64 ou S390x (raro mas possível)
### Solução Proposta
- Adicionar `#[cfg(not(target_endian = "big"))]` ou `compile_error!` para big-endian
### Arquivos Afetados
- `src/embedder.rs:277-279`


## Conformidades Verificadas — Eficiência e Performance
### Build Configuration
- opt-level = 3 — Cargo.toml:141
- lto = "fat" — Cargo.toml:138
- codegen-units = 1 — Cargo.toml:139
- panic = "abort" — Cargo.toml:142
- strip = true — Cargo.toml:140
- mold linker — .cargo/config.toml:23-25
### Alocações
- mimalloc global allocator — main.rs:3-4
- try_reserve em pontos críticos — 10 ocorrências em extraction.rs
- extend em bulk — 7 ocorrências
- to_owned() mínimo — apenas 2 ocorrências
### Smart Pointers e Ownership
- OnceLock para lazy init — constants.rs, extraction.rs:27-34
- parking_lot::Mutex preferido — extraction.rs:389, embedder.rs:27,64
- ZERO std::sync::Mutex em produção
### Regex
- Compilação via OnceLock — extraction.rs (8 patterns), constants.rs (NAME_SLUG_RE)
- ZERO Regex::new em loops
### Hashing
- ahash AHashMap/AHashSet em hot paths — hash.rs type aliases
- HashMap/HashSet com with_capacity — 23 conversões na auditoria E03-E05
### Concorrência
- JoinSet + Semaphore para backpressure — deep_research.rs:318-395
- loom tests para validação — tests/loom_lock_slots.rs
- parking_lot com deadlock detection feature — Cargo.toml:148
### Error Handling
- thiserror para erros estruturados — errors.rs
- anyhow para aplicação — commands/*.rs
### Serialização
- skip_serializing_if — 76 ocorrências (excelente cobertura)
### Profiling e Benchmarking
- criterion benchmarks — benches/cli_benchmarks.rs, benches/regression_baseline.rs
- CI benchmark com threshold — ci.yml:312-334
- cargo-careful no CI — ci.yml:336-344
- miri para unsafe — ci.yml:297-310
### Drop
- Drop minimal e seguro — daemon.rs:191-215 (DaemonSpawnGuard)
### FFI
- unsafe documentado com SAFETY — embedder.rs:277


## Auditoria de Redução de Latência — `docs_rules/rules_rust_latencia_reduzir.md`
### Data: 2026-05-31
### Escopo: 119 arquivos Rust, ~44500 LOC auditados contra 1147 linhas de regras (28 seções, 48 itens de checklist)
### Fontes: context7 /rust-lang/rust (trustScore 9.0), duckduckgo (CLI latency optimization, serde alternatives, atomic ordering)
### Contexto: sqlite-graphrag é uma CLI de linha de comando, NÃO um servidor HFT nem sistema embarcado
### Resultado: 28 CONFORMIDADES, 7 GAPS identificados como LT01-LT07 (0 HIGH, 3 MEDIUM, 4 LOW), 15 seções NOT_APPLICABLE

### Seções NOT_APPLICABLE — Justificativas
- Memória Virtual e TLB (linhas 195-221) — CLI não usa hugepages, mlockall, madvise; processo ephemeral
- Kernel Bypass e Redes Avançadas (linhas 543-566) — CLI não faz networking; comunicação é IPC socket com daemon local
- PGO e BOLT (linhas 100-125) — CLI com payloads variáveis; PGO requer perfil representativo estável
- I/O e Rede (linhas 511-540) — CLI não usa TCP/socket; I/O é SQLite via rusqlite
- Paralelismo Correto / Thread Pinning (linhas 569-595) — CLI não pina threads; tokio runtime é single-command
- Prioridade de Processos e Scheduling (linhas 598-620) — CLI não usa SCHED_FIFO; processo standard
- Tuning de Sistema Operacional (linhas 623-658) — regras de sistema, não de código
- SIMD e Vetorização (linhas 772-796) — embedding via fastembed/ONNX (C++ runtime), não Rust puro
- Padrões Determinísticos no_std (linhas 1022-1047) — CLI usa std; não é firmware embarcado
- Comunicação Inter-Processo (linhas 1050-1073) — IPC é Unix socket simples via daemon, não shmem
- Gestão de Conexões e Pools (linhas 970-992) — CLI não usa connection pools; SQLite é embedded
- Cópia Zero / serde(borrow) (linhas 224-249) — payloads < 512KB; overhead de lifetime complexity > benefício
- Estratégia de Startup e Warmup (linhas 854-879) — CLI cold-starts por design; daemon faz warmup separado
- Aritmética e Tipos Numéricos (linhas 715-740) — sem aritmética financeira; f32 usado para embeddings (correto)
- Gestão de Stack e Recursão (linhas 1076-1097) — sem recursão no codebase; loops iterativos usados


## LT01 MEDIUM — FIXED — 35 prepare() convertidos para prepare_cached() em SQL estático
### Regra Violada
- `docs_rules/rules_rust_latencia_reduzir.md:380` — "NUNCA calcular constante em cada chamada de função hot"
- `docs_rules/rules_rust_latencia_reduzir.md:130` — "PRÉ-ALOCAR todos os buffers em startup"
### Correção Aplicada
- 35 sites com SQL estático convertidos de prepare() para prepare_cached()
- rusqlite LRU cache (hashlink::LruCache) reutiliza statement compilado — O(1) hit vs O(n) compilação
- HOT PATH storage/: 14 sites (entities 10, memories knn 4+fts 2, chunks 2, urls 1)
- WARM PATH commands/: 5 sites (read, memory_entities×2, history, stats)
- COLD PATH commands/: 14 sites (debug_schema×2, migrate, normalize, restore, health, unlink×2, enrich×2, purge)
- 20 prepare() com SQL DINÂMICO (format!) mantidos — prepare_cached impossível com SQL text variável
### Evidência
- `rg 'conn\.prepare\(' src/storage/ | rg -v 'prepare\(&' | rg -v test` → ZERO
- `cargo test --all-features` — ZERO falhas


## LT02 MEDIUM — FIXED — format!() eliminado em list() com 4 SQL estáticos + prepare_cached()
### Regra Violada
- `docs_rules/rules_rust_latencia_reduzir.md:141` — "NUNCA usar format! em caminho crítico de latência"
- `docs_rules/rules_rust_latencia_reduzir.md:380` — "NUNCA calcular constante em cada chamada de função hot"
### Correção Aplicada
- memories.rs list(): deleted_clause (binário: "" ou " AND deleted_at IS NULL") × memory_type (Some/None) = 4 variantes
- Substituído 2 format!() + 2 prepare() por 4 &str literais + 4 prepare_cached()
- Resultado: ZERO format!() + ZERO heap allocation + statement cache reuse em list()
- KNN search format!() (namespaces variáveis) e deep_research format!() mantidos — SQL text depende de namespaces.len()
### Evidência
- `rg 'deleted_clause' src/storage/memories.rs` → ZERO
- `cargo test --all-features` — ZERO falhas


## LT03 MEDIUM — Tracing síncrono sem batching assíncrono
### Regra Violada
- `docs_rules/rules_rust_latencia_reduzir.md:884` — "USAR logger async com batching como tracing-appender"
- `docs_rules/rules_rust_latencia_reduzir.md:892` — "NUNCA escrever log síncrono em disco em hot path"
### Problema
- main.rs:94-101 configura tracing_subscriber::fmt() síncrono
- Sem tracing-appender para buffering assíncrono
- Cada tracing::info!/warn!/error! faz write síncrono para stderr
### Impacto
- Baixo para CLI (stderr é buffer de ~8KB do OS; flush é raro)
- Relevante quando CLI é invocado em loops por agentes LLM (milhares de invocações)
### Status: DEFERRED — CLI é invocação única; daemon tem runtime separado
### Solução Proposta
- Avaliar tracing-appender non-blocking para daemon mode
- Para CLI single-shot, overhead de tracing síncrono é aceitável
### Arquivos Afetados
- `src/main.rs:94-101` — subscriber configuration


## LT04 LOW — FIXED — #[inline(never)] em 5 funções com #[cold]
### Regra Violada
- `docs_rules/rules_rust_latencia_reduzir.md:349` — "APLICAR #[inline(never)] em construtores de erro pesados"
- `docs_rules/rules_rust_latencia_reduzir.md:459` — "APLICAR #[inline(never)] em construtores de erro pesados"
### Correção Aplicada
- 5 funções com #[cold] receberam #[inline(never)] para GARANTIA de não-inlining (cold é hint, never é diretiva)
- output.rs: emit_error_json, emit_error, emit_error_i18n
- daemon.rs: wait_for_daemon_exit, record_spawn_failure
### Evidência
- `rg '#\[inline\(never\)\]' src/ --type rust` → 5 resultados (output.rs×3, daemon.rs×2)
- `cargo clippy -- -D warnings` → ZERO warnings


## LT05 LOW — Sem CachePadded em atomics para prevenção de false sharing
### Regra Violada
- `docs_rules/rules_rust_latencia_reduzir.md:303` — "Aplicar crossbeam::utils::CachePadded para evitar false sharing"
- `docs_rules/rules_rust_latencia_reduzir.md:304` — "Separar contadores por thread e agregar sob demanda"
- Checklist linha 1114 — "Cache lines protegidas contra false sharing via CachePadded"
### Problema
- SHUTDOWN (AtomicBool em lib.rs) e DAEMON_VERSION_STATE (AtomicU8 em daemon.rs) não usam CachePadded
- Se colocados na mesma cache line de dados hot, causam false sharing
### Impacto
- Mínimo — atomics são acessados raramente (uma vez por invocação CLI)
- False sharing só é relevante em hot paths multi-thread com alta contenção
### Status: NOT_APPLICABLE — atomics são acessados raramente; CachePadded adicionaria 60+ bytes de padding sem benefício
### Arquivos Afetados
- `src/lib.rs` — SHUTDOWN AtomicBool
- `src/daemon.rs` — DAEMON_VERSION_STATE AtomicU8


## LT06 LOW — FIXED — tracing release_max_level_info elimina debug!/trace! em release
### Regra Violada
- `docs_rules/rules_rust_latencia_reduzir.md:887` — "USAR níveis estaticamente filtrados via LevelFilter"
- `docs_rules/rules_rust_latencia_reduzir.md:899` — "Configurar filtro em compile time via max_level_*"
### Correção Aplicada
- Cargo.toml: `tracing = { version = "0.1", features = ["release_max_level_info"] }`
- Em release builds: debug! e trace! viram no-ops (ZERO overhead, código eliminado pelo compilador)
- Em dev builds e cargo test: TODOS os níveis permanecem ativos (debug_assertions=true)
- Fonte: docs.rs/tracing/level_filters — "instrumentation at disabled levels will not even be present in the resulting binary"
### Evidência
- `rg 'release_max_level' Cargo.toml` → 1 resultado
- `cargo test --all-features` → ZERO falhas (dev builds não afetados)


## LT07 LOW — FIXED — assert size_of::<AppError> ≤ 128 bytes adicionado como guarda
### Regra Violada
- `docs_rules/rules_rust_latencia_reduzir.md:463` — "AUDITAR tamanho do enum de erro com std::mem::size_of"
### Correção Aplicada
- Teste `app_error_size_does_not_exceed_budget` adicionado em errors.rs mod tests
- Assert: size_of::<AppError>() ≤ 128 bytes — guarda contra inchaço futuro
- Budget de 128 bytes = ~2 cache lines, aceitável para Result<T, AppError> propagation
- Se exceder em futuras variantes: mensagem de erro orienta boxing de variantes grandes
### Evidência
- `cargo test -- app_error_size` → teste passa
- AppError atual está dentro do budget de 128 bytes


## Conformidades Verificadas — Redução de Latência
### Build Configuration
- opt-level = 3 — Cargo.toml:141
- lto = "fat" — Cargo.toml:138
- codegen-units = 1 — Cargo.toml:139
- panic = "abort" — Cargo.toml:142
- strip = true — Cargo.toml:140
- debug = false — Cargo.toml:143
- overflow-checks = false — Cargo.toml:144
- incremental = false — Cargo.toml:145
- [profile.release.package."*"] opt-level = 3 — Cargo.toml:147-148
- mold linker — .cargo/config.toml
### Allocator Global
- mimalloc #[global_allocator] — main.rs:3-4
### Hot Path Allocations
- Vec::with_capacity — 84 ocorrências em produção
- try_reserve — 10 ocorrências em extraction.rs para fallible allocation
- ZERO lazy_static — todo OnceLock
- ZERO Box::new em loops de produção (apenas em construction de ToSql params)
### Memory Ordering
- Acquire/Release em daemon.rs:477-507 — ORDERING comments documentados
- Acquire/Release em main.rs:283, lib.rs:86 — ORDERING comments documentados
- ZERO SeqCst em produção (apenas testes)
### Sincronização
- parking_lot::Mutex — extraction.rs:389, embedder.rs:27,64
- ZERO std::sync::Mutex em produção
- OnceLock — 10+ singletons (extraction.rs, constants.rs)
- spawn_blocking para CPU-bound — daemon.rs:254,294
### Branch Prediction
- #[cold] em 5 error paths — output.rs:115,143,150 + daemon.rs:531,727
- #[inline(never)] em 5 error paths — output.rs:116,144,151 + daemon.rs:532,728 (LT04 FIXED)
- #[must_use] em exit_code() — errors.rs:132
- #[inline] em 7 funções — output.rs (4) + errors.rs (1) + output.rs score_from_distance
### Error Handling
- thiserror enum — errors.rs:17
- panic = "abort" — Cargo.toml:142
- .with_context(||) lazy — extraction.rs (3 sites corrigidos no EP26)
### Compile-Time Evaluation
- OnceLock<Regex> — extraction.rs:27-34, constants.rs:272
- ZERO regex compilation em loops
- const literais — constants.rs (~330 linhas)
### Serialização
- skip_serializing_if — 76 ocorrências
- AHashMap/AHashSet em hot paths — hash.rs + 23 sites
- sort_unstable — 3 sites (EP15 FIXED)
### Timestamps
- Instant::now() — 50+ ocorrências (monotonic, correto)
- SystemTime::now — apenas 2 ocorrências em cold paths (daemon.rs:776, purge.rs:163)
### Profiling e Benchmarking
- criterion benchmarks — Cargo.toml:122
- CI benchmark regression — ci.yml:312-334
- loom tests — ci.yml:282-295
- miri validation — ci.yml:297-310
- cargo-careful — ci.yml:336-344
### Unsafe e FFI
- SAFETY documentado — embedder.rs:277
- compile_error! big-endian guard — embedder.rs:278 (EP30 FIXED)
- miri validação — ci.yml:297-310
### Bounds Checks
- Iteradores preferidos — extraction.rs, deep_research.rs
- .len() hoisted — extraction.rs:897 (EP18 FIXED)
### Logging
- tracing structured — via tracing_subscriber::fmt()
- ZERO println! em produção fora de output.rs
- release_max_level_info — debug!/trace! eliminados em release binary (LT06 FIXED)
### Statement Caching
- prepare_cached() — 35 sites com SQL estático convertidos (LT01 FIXED)
- list() SQL estático — 4 variantes sem format!() (LT02 FIXED)
- ZERO prepare() com SQL estático em storage/ (100% cobertura)
### Error Size Budget
- size_of::<AppError> ≤ 128 bytes — teste guarda em errors.rs (LT07 FIXED)


## Auditoria Multiplataforma — `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md`
### Data: 2026-05-31
### Escopo: 80 arquivos Rust, ~30435 LOC auditados contra 1752 linhas de regras (30+ seções, 65+ itens de checklist)
### Fontes: context7 directories (7.0), ctrlc (7.5), clap_complete (9.7); duckduckgo NO_COLOR/termcolor, SetConsoleOutputCP, clap_complete
### Contexto: sqlite-graphrag é CLI single-shot com subprocessos LLM; publica em Linux, macOS e Windows via CI matrix
### Resultado: 28 CONFORMIDADES, 18 GAPS identificados como MP01-MP18 (2 HIGH, 4 MEDIUM, 10 LOW, 1 N/A, 1 DEFERRED)


## MP01 HIGH — Ausência de inicialização UTF-8 do console Windows (SetConsoleOutputCP)
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:544-550` — "CHAMAR configuração de console como PRIMEIRA ação em run()"
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:547` — "USAR SetConsoleOutputCP(CP_UTF8) via windows-sys"
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:548` — "USAR SetConsoleCP(CP_UTF8) para input de stdin"
### Problema
- ZERO chamadas a SetConsoleOutputCP ou SetConsoleCP em todo o projeto
- Em Windows cmd.exe com code page 437/850/1252, caracteres acentuados da camada i18n viram mojibake
- O binário funciona mas mensagens em português ("conclusão", "ação", "decisão") aparecem corrompidas
- Afeta TODOS os comandos que emitem mensagens bilíngues via stderr
### Arquivos Afetados
- `src/main.rs` — deveria ser PRIMEIRA ação em main(), antes de tracing init
### Correção Proposta
- Adicionar `#[cfg(windows)]` block em main.rs antes de qualquer output
- Usar `windows-sys::Win32::System::Console::{SetConsoleOutputCP, SetConsoleCP}` com CP_UTF8 (65001)
- Encapsular em função `init_windows_console()` em eventual `src/terminal.rs`
### Dependência Necessária
- `windows-sys = { version = "0.52", features = ["Win32_System_Console"] }` — apenas para target Windows
### Impacto
- Sem fix: TODA saída i18n em português fica ilegível no cmd.exe Windows em código pages não-UTF-8
- Com fix: UTF-8 garantido em todos os terminais Windows 10+


## MP02 HIGH — Ausência de habilitação ANSI no console Windows
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:551-557` — "USAR crate enable-ansi-support ou equivalente"
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:553` — "HABILITAR flag ENABLE_VIRTUAL_TERMINAL_PROCESSING no console"
### Problema
- ZERO uso de enable-ansi-support ou ENABLE_VIRTUAL_TERMINAL_PROCESSING em todo o projeto
- tracing-subscriber emite cores ANSI por padrão quando stderr é TTY
- Em cmd.exe legado do Windows, escape codes ANSI aparecem como texto cru (ex: `[33mwarn[0m`)
- Windows Terminal moderno suporta ANSI nativamente, mas cmd.exe e PowerShell 5.1 não
### Arquivos Afetados
- `src/main.rs` — deveria habilitar ANSI antes de tracing_subscriber::fmt().init()
### Correção Proposta
- Adicionar `enable-ansi-support` crate (ou chamada direta a SetConsoleMode com ENABLE_VIRTUAL_TERMINAL_PROCESSING)
- Detectar falha e degradar para sem cores automaticamente
### Impacto
- Sem fix: logs com escape codes ilegíveis em terminais Windows antigos
- Com fix: cores funcionam em Windows 10+ e degradam gracefully em versões anteriores


## MP03 MEDIUM — Ausência de NO_COLOR e CLICOLOR_FORCE
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:518` — "NO_COLOR é padrão universal para desabilitar cores"
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:519` — "CLICOLOR_FORCE força cores mesmo sem TTY"
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:1042` — "RESPEITAR variável NO_COLOR forçando ColorChoice::Never"
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:1043` — "RESPEITAR variável CLICOLOR_FORCE=1 forçando Always"
### Problema
- ZERO referência a NO_COLOR, CLICOLOR_FORCE, termcolor ou anstream em todo o projeto
- tracing-subscriber::fmt() emite cores ANSI quando stderr é TTY, sem respeitar NO_COLOR
- Padrão no-color.org exige que `NO_COLOR` (qualquer valor) desabilite cores
- Pipelines CI e redirecionamento de stderr recebem escape codes não solicitados
### Arquivos Afetados
- `src/main.rs:93-103` — tracing_subscriber::fmt() sem respeitar NO_COLOR
- `src/output.rs` — nenhuma verificação de NO_COLOR antes de emitir
### Correção Proposta
- Verificar `std::env::var_os("NO_COLOR").is_some()` antes de configurar tracing subscriber
- Se NO_COLOR presente: usar `.with_ansi(false)` no tracing-subscriber
- Se CLICOLOR_FORCE=1 presente: forçar `.with_ansi(true)` mesmo sem TTY
- Adicionar flag `--no-color` como override CLI
### Impacto
- Sem fix: violação do padrão no-color.org; escape codes em pipelines automatizados
- Com fix: conformidade com padrão universal de cores


## MP04 MEDIUM — Leitura direta de LANG/LC_ALL sem sys_locale
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:628-631` — "NUNCA ler LANG ou LC_ALL diretamente"
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:623` — "USAR sys_locale::get_locale() como fonte primária"
### Problema
- src/i18n.rs linhas 54-60 lê LC_ALL, LC_MESSAGES, LANG diretamente via std::env::var
- Regra PROÍBE leitura direta e exige sys_locale::get_locale() como fonte primária
- sys_locale é cross-platform: Windows usa GetUserDefaultLocaleName, macOS usa CFLocaleCopyCurrent
- A implementação manual é POSIX-correta mas não detecta locale no Windows nativo
- Dependência sys_locale NÃO está no Cargo.toml
### Arquivos Afetados
- `src/i18n.rs:54-73` — detecção de locale manual via env vars
- `Cargo.toml` — sys_locale ausente
### Correção Proposta
- Adicionar `sys-locale = "0.3"` ao Cargo.toml
- Substituir bloco de leitura manual por sys_locale::get_locale()
- Manter fallback manual como backup para ambientes sem locale configurado
### Impacto
- Sem fix: detecção de idioma falha silenciosamente no Windows (sem LANG/LC_ALL definidos)
- Com fix: detecção funciona em Windows, macOS e Linux via APIs nativas


## MP05 MEDIUM — eprintln! em ingest.rs fora de output.rs
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:307-308` — "NUNCA println! em qualquer módulo fora de output.rs"
### Problema
- 1 `eprintln!` em código de produção (não-teste) em src/commands/ingest.rs
- Regra exige centralização TOTAL de I/O em src/output.rs
- Output descentralizado dificulta interceptação, formatação e controle de cores/encoding
### Arquivos Afetados
- `src/commands/ingest.rs` — eprintln!("{line}") em processamento de stderr de subprocesso
### Correção Proposta
- Substituir eprintln! por output::emit_progress() ou tracing::warn!
### Impacto
- Baixo risco funcional; gap de consistência arquitetural


## MP06 MEDIUM — Ausência de shell completions (clap_complete)
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:678-688` — "IMPLEMENTAR subcomando completions ou flag --completions"
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:679` — "USAR clap_complete para gerar scripts"
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:680` — "SUPORTAR Bash, Zsh, Fish, PowerShell e Elvish no mínimo"
### Problema
- clap_complete NÃO está nas dependências do Cargo.toml
- ZERO subcomando `completions` ou flag `--completions` na CLI
- Projeto com 49 subcomandos (init, daemon, remember, ingest, recall, etc) sem autocomplete
- Usabilidade severamente impactada para usuários interativos em qualquer shell
### Arquivos Afetados
- `Cargo.toml` — clap_complete ausente
- `src/cli.rs` — sem subcomando Completions
### Correção Proposta
- Adicionar `clap_complete = "4"` ao Cargo.toml
- Adicionar variante Completions ao enum Commands em cli.rs
- Implementar geração via `clap_complete::generate()` para 5 shells
- Documentar instalação por shell no README
### Impacto
- Sem fix: 49 subcomandos exigem memorização ou --help frequente
- Com fix: TAB completion em Bash, Zsh, Fish, PowerShell e Elvish


## MP07 MEDIUM — Módulos arquiteturais cross-platform faltando
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:290-305` — "Árvore Mínima de Módulos"
### Problema
- 6 módulos exigidos pela regra estão AUSENTES:
  - `src/platform.rs` — abstrações condicionais por SO via #[cfg]
  - `src/terminal.rs` — inicialização de console, encoding, cores, ANSI
  - `src/locale.rs` — detecção e resolução do idioma do sistema
  - `src/signals.rs` — handler de Ctrl+C e sinais cross-platform
  - `src/process.rs` — spawn de subprocessos quando necessário
  - `src/concurrency.rs` — abstração sobre rayon/tokio
- Funcionalidades existem mas espalhadas em main.rs, i18n.rs, daemon.rs, claude_runner.rs
- Sem módulos dedicados: lógica cross-platform fica acoplada à orquestração
### Arquivos Afetados
- Projeto inteiro — reorganização arquitetural
### Correção Proposta
- Criar módulos vazios com re-exports para centralizar funcionalidades existentes
- Mover detecção de locale de i18n.rs para locale.rs
- Mover signal handling de main.rs para signals.rs
- Mover init console Windows (após MP01/MP02 fix) para terminal.rs
### Impacto
- Gap arquitetural — não afeta funcionalidade mas dificulta manutenção cross-platform


## MP08 MEDIUM — Ausência de which crate para resolução de executáveis
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:130` — "TENTAR resolver via which crate contra $PATH do ambiente"
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:951` — "RESOLVER executável via which crate antes de invocar"
### Problema
- 4 arquivos usam Command::new(binary) sem validação prévia de existência do executável
- which crate NÃO está nas dependências do Cargo.toml
- Sem which: erro de "command not found" aparece como erro genérico de I/O
- which crate respeita PATHEXT no Windows (resolve .exe, .cmd, .bat automaticamente)
### Arquivos Afetados
- `src/commands/claude_runner.rs` — Command::new(binary) para claude/codex
- `src/commands/ingest_claude.rs` — Command::new(binary) para claude
- `src/commands/ingest_codex.rs` — Command::new(binary) para codex
- `src/commands/enrich.rs` — Command::new(binary) para claude
### Correção Proposta
- Adicionar `which = "7"` ao Cargo.toml
- Validar existência do executável com which::which(binary) ANTES de Command::new
- Emitir erro tipado AppError com sugestão de instalação quando binário ausente
### Impacto
- Sem fix: "No such file" genérico quando claude/codex não instalado
- Com fix: mensagem clara com sugestão de instalação por plataforma


## MP09 LOW — Ausência de keyring/secrecy/zeroize para credenciais
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:1084-1094` — "USAR keyring, secrecy, zeroize para segredos"
### Problema
- ZERO uso de keyring, secrecy ou zeroize em todo o projeto
- Projeto não armazena API keys diretamente — usa env vars passadas pelo Claude Code
- Risco mitigado pelo modelo de uso: CLI tool sem persistência de credenciais próprias
### Arquivos Afetados
- N/A — projeto não gerencia credenciais
### Status: OPEN (baixo risco no contexto atual — CLI delega autenticação ao Claude Code)


## MP10 LOW — HashMap em deep_research sem garantia de determinismo
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:758` — "NUNCA usar HashMap onde BTreeMap traz estabilidade"
### Problema
- 2 HashMap::new() em src/commands/deep_research.rs (predecessor, entity_names)
- Usados para construção interna de grafo de evidências — não afeta ordem do JSON de saída
- Resultado é serializado por score, não por ordem de inserção no HashMap
- 11 arquivos no total usam HashMap — risco teórico de não-determinismo
### Arquivos Afetados
- `src/commands/deep_research.rs` — HashMap para predecessor e entity_names
### Status: OPEN (baixo risco — HashMap não afeta saída diretamente)


## MP11 LOW — NOT_APPLICABLE — deny(unsafe_code) em lib.rs
### Regra
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:1142` — "MARCAR crate com #![deny(unsafe_code)] quando não usa unsafe"
### Justificativa
- Projeto USA unsafe legitimamente em main.rs (env::set_var) e extraction.rs (FFI GLiNER)
- Atributo deny(unsafe_code) bloquearia compilação de módulos legítimos
- Unsafe é isolado e documentado com comentários SAFETY
### Status: NOT_APPLICABLE


## MP12 LOW — DEFERRED — Ausência de sandboxing (seccomp, landlock)
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:1361-1378` — seccomp-bpf, landlock, pledge
### Justificativa do Deferimento
- CLI opera em arquivo SQLite local com permissões do usuário
- Superfície de ataque baixa: sem rede, sem entrada web, sem serviço público
- Sandboxing relevante para deployments em containers/servidores multi-tenant
- Custo de implementação desproporcional ao benefício para CLI local
### Status: DEFERRED


## MP13 LOW — Ausência de cargo geiger no CI
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:1265` — "EXECUTAR cargo geiger para monitorar densidade de unsafe"
### Problema
- CI não executa cargo geiger para monitorar crescimento de blocos unsafe
- Projeto tem unsafe legítimo mas limitado (env::set_var, FFI sqlite-vec, GLiNER ONNX)
- Sem monitoramento, novos blocos unsafe podem ser adicionados sem revisão
### Arquivos Afetados
- `.github/workflows/ci.yml` — cargo geiger ausente
### Status: OPEN


## MP14 LOW — Ausência de fuzzing (cargo-fuzz, honggfuzz)
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:1332-1336` — "APLICAR cargo-fuzz em parsers"
### Problema
- ZERO configuração de fuzzing no projeto
- Parsers em src/parsers/mod.rs e src/extraction.rs aceitam input externo (memory bodies)
- proptest está presente como dependência — cobre property-based testing parcialmente
- Fuzzing dedicado com libfuzzer ou honggfuzz não configurado
### Arquivos Afetados
- `Cargo.toml` — sem deps de fuzzing
- Diretório `fuzz/` ausente
### Status: OPEN


## MP15 LOW — Ausência de insta para snapshot testing
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:1329` — "USAR insta para snapshot testing de output estável"
### Problema
- insta NÃO está nas dependências
- Projeto emite JSON determinístico — candidato ideal para snapshot testing
- Atualmente validação de output usa assert_eq! manual contra strings esperadas
### Status: OPEN


## MP16 LOW — Ausência de BTreeMap para saída JSON ordenada
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:749` — "USAR BTreeMap onde ordem estável é necessária"
### Problema
- ZERO uso de BTreeMap em todo o projeto
- serde_json::json! macro gera objetos com ordem de inserção, não lexicográfica
- Para output determinístico byte-a-byte, chaves deveriam ser ordenadas
- serde_json::to_string serializa em ordem de declaração das fields do struct (estável por Serialize derive)
### Nuance
- Structs com #[derive(Serialize)] mantêm ordem de declaração — determinístico
- Apenas serde_json::json! dinâmico e serde_json::Map requerem BTreeMap
- Risco real limitado quando structs tipados dominam o output
### Status: OPEN (risco mitigado por structs tipados)


## MP17 LOW — Ausência de Job Object (Windows) e process group (Unix) para subprocessos
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:986-989` — "USAR Job Object em Windows; process group em Unix"
### Problema
- claude_runner.rs usa pre_exec para setrlimit mas NÃO define process group
- daemon.rs spawna processo filho sem Job Object no Windows
- Se o processo pai morrer (kill -9), filhos ficam órfãos consumindo recursos
### Arquivos Afetados
- `src/commands/claude_runner.rs` — sem setsid()/setpgid()
- `src/daemon.rs` — sem Job Object Windows
### Status: OPEN


## MP18 LOW — std::process::exit sem cleanup completo
### Regra Violada
- `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md:1033` — "NUNCA usar std::process::exit sem cleanup prévio"
### Problema
- src/main.rs:226 chama std::process::exit(20) no caso de erro de medição de RAM
- Não passa pelo cleanup normal (flush de buffers, release do slot de semáforo, drop guards)
- Semáforo de arquivo flock é liberado pelo OS ao fechar o processo, mas buffers pendentes podem ser perdidos
### Arquivos Afetados
- `src/main.rs:226` — std::process::exit(20)
### Correção Proposta
- Substituir por return std::process::ExitCode::from(20) que permite cleanup via Drop
### Status: OPEN


## Conformidades Verificadas — Multiplataforma
### Crates Canônicas
- clap 4 com derive — Cargo.toml:68
- anyhow 1 — Cargo.toml:74
- thiserror 2 — Cargo.toml:75
- serde com derive — Cargo.toml:69
- serde_json — Cargo.toml:70
- directories 5 — Cargo.toml:77
- chrono com serde — Cargo.toml:85
- tracing e tracing-subscriber — Cargo.toml:71-72
- tempfile — usado em testes
- ctrlc 3.4 com termination — Cargo.toml:88
- unicode-normalization — Cargo.toml:100
- proptest — Cargo.toml (dev-dependencies)
### Paths Cross-Platform
- directories::ProjectDirs em paths.rs e lock.rs
- PathBuf/Path sem separador hardcoded em todo projeto
- ZERO "/" ou "\\" hardcoded em path construction
### Estado Global
- 15+ OnceLock para singletons (tz.rs, i18n.rs, extraction.rs, embedder.rs, constants.rs)
- ZERO lazy_static em todo projeto
- ZERO Mutex para dados imutáveis
### Signal Handling
- ctrlc com termination feature — main.rs:282
- SHUTDOWN: AtomicBool com Acquire/Release — lib.rs
- CancellationToken para graceful shutdown — lib.rs
### BrokenPipe
- Silenciado em output.rs (emit_json, emit_json_line, emit_text, emit_text_raw)
- Padrão correto: if e.kind() == BrokenPipe { return Ok(()) }
### Subprocess Management
- env_clear() em 4 spawners (claude_runner, ingest_claude, ingest_codex, enrich)
- Stdio::null() para stdin em processos headless
- Stdio::piped() para stdout/stderr capturados
- .output() ou .wait() em todos os subprocessos — ZERO zumbis
### Permissões Unix
- #[cfg(unix)] guard em connection.rs, sync_safe_copy.rs, backup.rs
- set_mode(0o600) para arquivos sensíveis (banco SQLite, backup)
### Unicode Normalization
- unicode_normalization::UnicodeNormalization em extraction.rs, parsers/mod.rs, ingest.rs
- NFKC normalização para nomes de entidade
- NFD-based stripping para kebab-case conversion
### Timestamps
- chrono::DateTime<Utc> com RFC 3339 em toda saída
- Parsers aceitam Unix epoch E RFC 3339 (parsers/mod.rs)
### i18n
- Language enum com English/Portuguese — i18n.rs
- OnceLock<Language> para estado global
- Precedência 4 camadas: --lang > SQLITE_GRAPHRAG_LANG > locale > English
### Exit Codes
- 20+ exit codes padronizados em errors.rs
- Mapeamento AppError → exit code via exit_code()
### Contrato stdout/stderr
- JSON determinístico no stdout via output.rs
- Logs e progresso EXCLUSIVAMENTE no stderr via tracing
### CI Matrix
- ci.yml: ubuntu-latest, macos-latest, windows-latest
- release.yml: targets Linux GNU, musl, macOS Intel+Silicon, Windows MSVC
- Universal binary macOS via lipo
### Build Configuration
- MSRV pinned: rust-version = "1.88" + rust-toolchain.toml channel = "1.88"
- Profile release: lto = "fat", codegen-units = 1, panic = "abort", strip = true
- mimalloc #[global_allocator] — main.rs:3-4
### Supply Chain
- cargo audit no CI
- cargo deny check no CI
- deny.toml configurado
### Governance
- SECURITY.md presente na raiz
- CHANGELOG.md presente na raiz
### Serialização
- serde(deny_unknown_fields) em structs de input externo (remember.rs, entities.rs)
- skip_serializing_if — 76 ocorrências


## Auditoria Tracing & Logging — `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md`
### Data: 2026-05-31
### Escopo: 78 arquivos Rust, 149 eventos tracing auditados contra 1081 linhas de regras (15 seções, 77 itens de checklist)
### Fontes: context7 tracing-subscriber (trustScore 6.6 — low match), context7 tracing-appender (trustScore 8.9); duckduckgo WorkerGuard best practices, tracing-panic hook, tracing-log LogTracer
### Contexto: sqlite-graphrag é CLI de execução curta (1-30s), emite para stderr, sem file rotation, sem HTTP, sem distributed tracing
### Resultado: 24 CONFORMIDADES, 14 GAPS identificados como TR01-TR14 (2 HIGH, 3 MEDIUM, 9 LOW); 8 categorias N/A para CLI
### Categorias N/A (justificadas): RollingFileAppender (CLI sem arquivo), WorkerGuard/non_blocking (execução curta), reload::Layer (sem runtime persistente), OpenTelemetry (CLI local), tokio-console (sem tokio principal), request correlation (sem HTTP), MakeWriter custom (tudo stderr), métricas operacionais (sem canal descartável)


## TR01 HIGH — Ausência de panic hook integrado com tracing
### Regra Violada
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:128-132` — "Instalar hook via tracing-panic ou log-panics"
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:129` — "Hook converte panic em evento de nível error"
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:131` — "Hook flusha o writer antes de abortar"
### Problema
- Nenhum panic hook instalado no binário
- Threads spawned (daemon.rs deadlock-detector, tokio worker threads) podem panic silenciosamente
- Panic em thread background desaparece sem registro em stderr (tracing não captura panics de outras threads)
- Output do panic default do Rust vai para stderr MAS sem formatação JSON, sem target, sem campos estruturados
### Impacto
- Panics invisíveis em threads spawned dificultam diagnóstico post-mortem
- Em modo `SQLITE_GRAPHRAG_LOG_FORMAT=json`, panic default quebra o schema JSON do stderr
- Operador não recebe evento tracing correlacionável com o restante dos logs
### Causa Raiz
- Crate `tracing-panic` ou `log-panics` não está nas dependências
- Nenhuma chamada a `std::panic::set_hook()` no main.rs
- Oversight: foco em observabilidade do happy path sem cobrir failure modes
### Solução Proposta
- Adicionar `tracing-panic = "0.1"` ao Cargo.toml
- Instalar hook APÓS subscriber init: `tracing_panic::panic_hook::set()`
- Alternativa: hook custom com `std::panic::set_hook` emitindo `tracing::error!`
### Arquivos Afetados
- `Cargo.toml` — nova dependência
- `src/main.rs:107` — inserir hook após subscriber init


## TR02 HIGH — Ausência de ponte LogTracer para dependências que usam `log` crate
### Regra Violada
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:134-138` — "Instalar LogTracer via tracing-log"
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:135` — "Usar LogTracer::builder para filtro por crate origem"
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:136` — "Conversão automática de records log em eventos tracing"
### Problema
- `log` v0.4.29 está na dependency tree via refinery-core, ureq e ort-sys
- Estas dependências emitem via `log::warn!()` e `log::info!()` internamente
- Sem `LogTracer` instalado, estes eventos são silenciosamente descartados
- Operador perde diagnóstico de erros em refinery (migrações), ureq (HTTP downloads), ort (ONNX runtime)
### Impacto
- Erros de migração do banco (refinery) podem falhar sem log visível
- Problemas de download de modelo ONNX (ureq) sem diagnóstico
- Warnings do ONNX Runtime sobre performance/compatibilidade perdidos
### Causa Raiz
- Feature `log` do `tracing` crate NÃO ativa a ponte automaticamente — apenas emite tracing events via log
- A ponte reversa (log → tracing) requer `tracing-log` crate + `LogTracer::init()`
- Oversight: assumiu-se que `tracing` com feature "log" cobria ambas direções
### Solução Proposta
- Adicionar `tracing-log = "0.2"` ao Cargo.toml
- Inserir `tracing_log::LogTracer::init().ok();` ANTES do subscriber init em main.rs
- Posicionar: entre env vars setup e `tracing_subscriber::fmt()` call
### Arquivos Afetados
- `Cargo.toml` — nova dependência
- `src/main.rs:93` — inserir LogTracer::init() antes de subscriber


## TR03 MEDIUM — Inicialização de tracing não centralizada em função dedicada
### Regra Violada
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:42` — "Centralizar inicialização em função init_telemetry única"
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:148` — "Função init_telemetry recebe configuração de logging"
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:149` — "Função retorna Result<WorkerGuard, TelemetryError>"
### Problema
- Inicialização do subscriber está inline no main.rs (linhas 92-106, ~15 linhas)
- Não retorna guard (sem WorkerGuard porque usa stderr síncrono — aceitável)
- Lógica de decisão json/pretty misturada com código de bootstrap do binário
- Dificulta reutilização em testes de integração
### Impacto
- Manutenibilidade: mudanças de tracing exigem editar main.rs diretamente
- Testabilidade: não é possível testar a configuração do subscriber isoladamente
- Legibilidade: main.rs acumula responsabilidades heterogêneas
### Solução Proposta
- Criar `src/telemetry.rs` com `pub fn init_tracing(log_level: &str, log_format: &str) -> ()`
- Mover lógica de linhas 92-106 para o novo módulo
- Registrar em lib.rs: `pub mod telemetry;`
- main.rs chama `sqlite_graphrag::telemetry::init_tracing(&log_level, &log_format);`
### Arquivos Afetados
- `src/telemetry.rs` — NOVO
- `src/lib.rs` — adicionar pub mod
- `src/main.rs` — substituir inline por chamada


## TR04 MEDIUM — Features de tracing-subscriber não declaradas explicitamente
### Regra Violada
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:87-94` — "Ativar feature env-filter, fmt, json, registry, ansi explicitamente"
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:103` — "NUNCA depender de features de default sem declaração"
### Problema
- Cargo.toml declara: `features = ["json", "env-filter"]`
- Features `fmt`, `ansi`, `registry` vêm implicitamente via default features
- Se alguém adicionar `default-features = false`, o build quebra silenciosamente
- Dependência implícita de features não documentadas
### Impacto
- Fragilidade: mudança de defaults upstream pode quebrar compilação
- Clareza: desenvolvedor não sabe quais features são realmente usadas
### Solução Proposta
- Alterar para: `features = ["json", "env-filter", "fmt", "ansi", "registry"]`
- Manter `default-features = true` (não adicionar false) mas documentar uso explícito
### Arquivos Afetados
- `Cargo.toml` — expandir features list


## TR05 MEDIUM — Ausência de evento confirmando filtro efetivo após init
### Regra Violada
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:119` — "Emitir evento de confirmação do filtro efetivo após init"
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:406` — "Logar o filtro efetivo após a inicialização"
### Problema
- Após subscriber instalado, nenhum evento registra o nível de filtro ativo
- Operador não sabe se `SQLITE_GRAPHRAG_LOG_LEVEL=debug` foi aplicado corretamente
- Diagnóstico de "por que não vejo meus logs?" exige reprodução manual
### Impacto
- Dificuldade de troubleshooting remoto
- Impossível confirmar se variável de ambiente foi lida corretamente
### Solução Proposta
- Adicionar imediatamente após subscriber init:
- `tracing::debug!(filter = %log_level, format = %log_format, "tracing subscriber initialized");`
### Arquivos Afetados
- `src/main.rs:107` — inserir evento de confirmação


## TR06 MEDIUM — Eventos com interpolação na mensagem em vez de campos estruturados
### Regra Violada
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:562` — "Mensagem textual estável sem interpolação variável"
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:574` — "NUNCA concatenar strings com format! dentro de macro de log"
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:587` — Anti-padrão: `info!("msg {}", var)` sem campos
### Problema
- ~15 eventos usam interpolação direta: `tracing::warn!("GLiNER model unavailable: {e:#}")`
- Mensagens variam a cada invocação (contêm error message dinâmica)
- Impede agregação por mensagem em sistemas de log (cada instância é "unique")
- Campos não podem ser filtrados/indexados independentemente da mensagem
### Sites Afetados
- `src/signals.rs:18` — `"failed to register signal handler: {e}"`
- `src/extraction.rs` — `"GLiNER model unavailable (graceful degradation): {e:#}"`
- `src/extraction.rs` — `"GLiNER NER failed, falling back to regex-only: {e:#}"`
- `src/commands/deep_research.rs` — `"sub-query task cancelled: {join_err}"`
- `src/commands/ingest.rs` — `"invalid --gliner-variant: {e}; using fp32"`
- `src/commands/stats.rs` — `"failed to count memory_chunks: {e}"`
- `src/commands/hybrid_search.rs` — `"FTS5 query failed, falling back to vec-only: {e}"`
- `src/commands/remember.rs` — `"auto-extraction failed (graceful degradation): {e:#}"`
- `src/storage/urls.rs` — `"failed to persist url '{}': {e:#}"`
- `src/daemon.rs` — `"daemon autostart suppressed by backoff window"`
### Solução Proposta
- Converter para campos nomeados: `tracing::warn!(error = %e, "signal handler registration failed")`
- Mensagem estável como string literal; variáveis como campos
### Arquivos Afetados
- ~12 arquivos com ~15 sites de correção


## TR07 LOW — Formato JSON sem thread_ids e thread_names
### Regra Violada
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:360` — "Incluir thread_ids e thread_names em ambientes multithread"
### Problema
- Branch JSON não configura `.with_thread_ids(true)` nem `.with_thread_names(true)`
- Em debugging de daemon ou deadlock-detector, contexto de thread é perdido
### Solução Proposta
- Adicionar `.with_thread_ids(true).with_thread_names(true)` no branch JSON
### Arquivos Afetados
- `src/main.rs:94-99`


## TR08 LOW — Ausência de timer explícito com formato garantido
### Regra Violada
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:357` — "Incluir timestamp em RFC 3339 com timezone explícito"
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:367-371` — "Usar UtcTime ou LocalTime"
### Problema
- Subscriber usa timer default sem garantia de formato RFC 3339
- Parsing downstream pode quebrar se formato mudar entre versões
### Solução Proposta
- Adicionar `.with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())`
### Arquivos Afetados
- `src/main.rs:94-99`
- `Cargo.toml` — possivelmente feature `time`


## TR09 LOW — SKIP — FmtSpan não configurado explicitamente
### Justificativa
- Default é FmtSpan::NONE = correto para CLI (regra 377: "NONE como default em produção")
- Sem spans no codebase, configurar explicitamente seria dead code
### Status: SKIP


## TR10 LOW — ~50% dos eventos sem target explícito
### Regra Violada
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:409` — "Usar formato target=level para controle por módulo"
### Problema
- ~75 de 149 eventos tracing NÃO usam `target: "nome"` explícito
- Default é module path completo que é verboso para filtragem
- Exemplos sem target: daemon.rs, extraction.rs, signals.rs, pragmas.rs
### Solução Proposta
- Definir targets canônicos: "daemon", "extraction", "storage", "signals", "pragmas"
- Adicionar `target: "X"` aos ~75 eventos faltantes
### Arquivos Afetados
- ~10 arquivos


## TR11 LOW — Ausência de tracing-error e ErrorLayer para SpanTrace
### Regra Violada
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:596-600` — "Adicionar ErrorLayer ao registry"
### Problema
- `tracing-error` não nas deps; SpanTrace não capturado em boundaries de erro
### Impacto
- BAIXO para CLI sem spans: ErrorLayer requer spans para ser útil
### Solução Proposta
- CONDICIONAL a TR14: adicionar apenas se #[instrument] for introduzido
### Arquivos Afetados
- `Cargo.toml`, `src/main.rs` — condicional


## TR12 LOW — Ausência de testes validando eventos tracing
### Regra Violada
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:810-816` — "Usar tracing-test para capturar eventos em asserts"
### Problema
- Nenhum teste valida emissão correta de eventos tracing
- Regressões em observabilidade passam despercebidas
### Solução Proposta
- Adicionar `tracing-test = "0.2"` e criar 1-2 testes exemplares
### Arquivos Afetados
- `Cargo.toml` [dev-deps], `tests/`


## TR13 LOW — Ausência de tracing::enabled! para gating
### Regra Violada
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:769` — "Usar tracing::enabled! para gate de trabalho custoso"
### Problema
- Nenhum uso de `tracing::enabled!` em todo o crate
- Mitigado por `release_max_level_info` que elimina debug/trace em compile time
### Impacto
- BAIXO: mitigação em compile time cobre 90% dos casos
### Solução Proposta
- Monitorar via profiling; adicionar gates se overhead detectado
### Arquivos Afetados
- Nenhum urgente


## TR14 LOW — Zero uso de #[instrument] para spans e correlação
### Regra Violada
- `docs_rules/rules_rust_logs_com_tracing_e_rotacao.md:447-451` — "Instrumentar fronteiras públicas"
### Problema
- Zero `#[instrument]`, zero spans manuais em 78 arquivos
- Eventos são flat sem hierarquia de contexto
### Impacto
- BAIXO para CLI single-shot: execução linear minimiza benefício
- MÉDIO para daemon/ingest paths de longa duração
### Solução Proposta
- Adicionar `#[instrument(skip_all, level = "debug")]` em 5-10 funções run() de commands pesados
### Arquivos Afetados
- `src/commands/ingest.rs`, `remember.rs`, `recall.rs`, `hybrid_search.rs`, `deep_research.rs`


## Conformidades Verificadas — Tracing & Logging
### Stack Canônico
- tracing como fachada única — 149 eventos, zero log:: direto
- tracing-subscriber para composição — main.rs:94-106
- NUNCA println!/eprintln! para diagnóstico — apenas output.rs
- NUNCA env_logger, slog ou log crate como primário
- NUNCA dbg! em código de produção — zero ocorrências
### Inicialização
- Subscriber instalado ANTES de qualquer evento — main.rs
- .init() chamado apenas UMA vez — único call site
- Binário instala subscriber; lib apenas emite eventos
### Filtragem
- EnvFilter com default "warn" — seguro em produção
- Sobrescrita via SQLITE_GRAPHRAG_LOG_LEVEL sem recompilação
- release_max_level_info elimina debug/trace em compile time
### Formatos
- JSON via SQLITE_GRAPHRAG_LOG_FORMAT=json
- ANSI desligado em JSON — .with_ansi(false)
- NO_COLOR respeitado — terminal.rs:should_use_ansi()
### Eventos
- Campos nomeados em eventos estruturados
- Targets semânticos em ~50% dos eventos
- Zero format!() dentro de macros tracing
- Nomes de campos em snake_case
### Segurança
- NUNCA logar senhas, tokens ou segredos
- NUNCA logar PII
### Encerramento
- SIGTERM/SIGINT capturados via signals.rs
- ExitCode::from() em vez de process::exit
- Flush explícito antes de return
### Desempenho
- release_max_level_info ativo
- Writer stderr síncrono (aceitável para CLI curta)
### Separação Binário vs Biblioteca
- Binário instala subscriber global
- Lib apenas emite eventos


## Auditoria Processos Externos — `docs_rules/rules_rust_processos_externos.md`
### Escopo
- Regras: 1235 linhas, 19 seções, 55+ checklist items
- Arquivos auditados: `claude_runner.rs`, `ingest_claude.rs`, `ingest_codex.rs`, `enrich.rs`, `daemon.rs`
- Binários externos invocados: `claude` (Anthropic), `codex` (OpenAI)
- Data: 2026-05-31
### Metodologia
- Busca exaustiva: `rg 'Command::new|spawn|output|status|kill|wait|env_clear|stdin|stdout|stderr|which::which'`
- Validação de cada site contra checklist final (linhas 1186-1235)
- Consulta context7 (wait-timeout trustScore 9.0) e duckduckgo-search-cli (CommandExt pre_exec)


## PE01 MEDIUM — enrich.rs stdin write sem thread dedicada (deadlock potencial)
### Problema
- `enrich.rs:2752-2754` escreve em stdin do child E faz `wait_timeout` na MESMA thread
- Se prompt exceder buffer de pipe do SO (64KB Linux, 4KB macOS), child bloqueia em write do stdout
- Parent bloqueia em write do stdin → deadlock bilateral
### Regra Violada
- §4 linhas 161-168: "SPAWNAR thread dedicada para escrita em stdin do filho"
- §4 linhas 179-184: "NUNCA escrever em stdin e depois tentar ler stdout na mesma thread"
### Evidência
- `ingest_claude.rs:352-355` e `ingest_codex.rs:358-361` CORRETAMENTE usam `std::thread::spawn` para stdin
- `enrich.rs:2752` faz `stdin.write_all` inline sem thread
### Solução Proposta
- Extrair write para thread dedicada (mesmo padrão de `ingest_codex.rs:358-361`)
- Fechar stdin handle explicitamente antes de wait_timeout
### Arquivos Afetados
- `src/commands/enrich.rs`


## PE02 MEDIUM — enrich.rs validate_claude_version_local sem which::which
### Problema
- `enrich.rs:563` chama `Command::new(binary)` diretamente sem resolver via `which::which`
- Se binário não existir no PATH, erro é `io::Error` genérico em vez de mensagem clara
- Inconsistente com `claude_runner.rs:118` e `ingest_codex.rs:201` que usam `which::which`
### Regra Violada
- §2 linhas 59-65: "RESOLVER binário via which-equivalente antes de spawn para erros precisos"
- §2 linha 64: "TRATAR ausência do binário como erro configurável, não como panic"
### Evidência
- `claude_runner.rs:118`: `let resolved = which::which(binary).map_err(|_| { ... })?;`
- `enrich.rs:564`: `Command::new(binary).arg("--version")` — sem resolução prévia
### Solução Proposta
- Reutilizar `claude_runner::validate_claude_version` (elimina DRY também)
- OU adicionar `which::which` antes do `Command::new` em `enrich.rs:563`
### Arquivos Afetados
- `src/commands/enrich.rs`


## PE03 LOW — enrich.rs Codex env whitelist sem bloco Windows
### Problema
- `enrich.rs:2717-2729` define whitelist para Codex com `env_clear()` + vars seletivas
- NÃO inclui bloco `#[cfg(windows)]` para vars Windows-specific (`LOCALAPPDATA`, `APPDATA`, etc.)
- `ingest_codex.rs:306-318` CORRETAMENTE inclui `#[cfg(windows)]` block
### Regra Violada
- §7 linhas 302-304: "INJETAR explicitamente variáveis necessárias após env_clear"
- §10 linhas 530-531: "SUPORTAR Windows 10, Windows 11"
### Solução Proposta
- Adicionar bloco `#[cfg(windows)]` idêntico ao de `ingest_codex.rs:306-318`
### Arquivos Afetados
- `src/commands/enrich.rs`


## PE04 LOW — daemon.rs sem process group em non-Linux
### Problema
- `daemon.rs:616-631` spawna daemon child sem `setsid` ou process group em non-Linux
- Em macOS/Windows, child tree não é agrupada para encerramento em cascata
- `claude_runner.rs:72` aplica `setsid()` mas SOMENTE em `#[cfg(target_os = "linux")]`
### Regra Violada
- §6 linhas 275-279: "AGRUPAR filho e descendentes em grupo único antes de spawn"
- §10 linhas 551-555: "ASSOCIAR filho a job object em Windows para encerramento em cascata"
### Mitigação Existente
- Daemon tem idle-shutdown timer (auto-termina após inatividade)
- Em prática, daemon é single-process sem filhos próprios
### Solução Proposta
- macOS: adicionar `process_group(0)` via `CommandExt` em bloco `#[cfg(unix)]`
- Windows: DEFERRED — requer crate `win32job` ou similar para job objects
### Arquivos Afetados
- `src/commands/claude_runner.rs` (spawn_with_memory_limit)
- `src/daemon.rs` (autostart spawn)


## PE05 LOW — claude_runner.rs setsid() retorno não verificado
### Problema
- `claude_runner.rs:72` chama `libc::setsid()` sem verificar retorno
- Se processo já é session leader, `setsid()` retorna -1 e `errno` é `EPERM`
- Erro é silencioso — child continua sem session group independente
### Regra Violada
- §12 linhas 851-855: "TRATAR pre_exec como trecho unsafe e justificar cada operação"
- Regra implícita: verificar retorno de syscalls em contexto safety-critical
### Mitigação Existente
- Na prática, CLI não é session leader ao spawnar (user invokes from shell)
- Falha de setsid não impede execução do child — apenas perde isolamento de grupo
### Solução Proposta
- Verificar retorno: `if libc::setsid() == -1 { /* log or ignore per policy */ }`
- Decisão: ignorar EPERM silenciosamente (aceitável) mas logar outros erros
### Arquivos Afetados
- `src/commands/claude_runner.rs`


## PE06 LOW — stdin thread sem drop explícito do handle
### Problema
- `ingest_claude.rs:352-355` e `ingest_codex.rs:358-361` movem stdin handle para thread
- O handle é dropped implicitamente ao final do closure — correto mas não explícito
- Regra exige: "FECHAR handle de stdin após término da escrita para sinalizar EOF"
### Regra Violada
- §4 linha 170: "FECHAR handle de stdin após término da escrita para sinalizar EOF"
### Mitigação Existente
- `write_all` consome todos os bytes, closure termina, handle é dropped = EOF sinalizado
- Comportamento correto na prática
### Solução Proposta
- Adicionar `drop(child_stdin);` explícito após `write_all` no closure
- Melhoria de clareza, não funcional
### Arquivos Afetados
- `src/commands/ingest_claude.rs`
- `src/commands/ingest_codex.rs`


## PE10 LOW — Ausência de tracing event no momento do spawn
### Problema
- Nenhum dos 5 arquivos emite evento tracing no exato momento do spawn com binário e args
- Diagnóstico de "what was actually invoked?" requer debugging manual
- Regra exige log de invocação com argumentos sanitizados
### Regra Violada
- §15 linhas 1019-1024: "LOGAR invocação de cada processo externo em nível configurável"
- §15 linha 1023: "LOGAR argumentos sanitizados evitando vazamento de segredos"
### Solução Proposta
- Adicionar `tracing::debug!(target: "process", binary = %path, "spawning external process")` antes de cada spawn
- NÃO logar conteúdo de stdin (pode conter prompts com dados do usuário)
### Arquivos Afetados
- `src/commands/claude_runner.rs`
- `src/commands/enrich.rs`
- `src/daemon.rs`


## PE11 LOW — Ausência de tracing span cobrindo invocação externa
### Problema
- Nenhuma invocação externa é coberta por span de tracing
- Impossível correlacionar eventos de spawn/timeout/parse com invocação específica
- Regra exige span com atributos `process.command`, `process.exit_code`, `process.duration`
### Regra Violada
- §15 linhas 1031-1036: "CRIAR span de tracing cobrindo toda invocação externa"
- §15 linha 1034: "INCLUIR atributos process.command, process.exit_code e process.duration"
### Mitigação Existente
- `elapsed_ms` é registrado no NDJSON output (visível ao caller)
- `#[instrument]` no `run()` dos comandos cobre o nível macro
### Solução Proposta
- Adicionar `#[instrument(skip_all, level = "debug", name = "spawn_claude")]` em `run_claude()`
- Ou wrapping manual com `tracing::debug_span!("external_process", binary = %path)`
### Arquivos Afetados
- `src/commands/claude_runner.rs`
- `src/commands/ingest_claude.rs`
- `src/commands/ingest_codex.rs`
- `src/commands/enrich.rs`


## PE13 LOW — enrich.rs stderr não logado em warn ao falhar
### Problema
- `enrich.rs:2772-2776` retorna stderr como parte do erro mas NÃO emite `tracing::warn!`
- Em contraste, `ingest_claude.rs:420` e `ingest_codex.rs:393` emitem `tracing::warn!` em falhas
- Operadores sem `--json` não veem o stderr capturado em logs
### Regra Violada
- §15 linha 1022: "LOGAR conteúdo de stderr em nível de aviso quando execução falha"
- §8 linhas 393-394: "INCLUIR trecho inicial de stderr capturado em erro de execução"
### Solução Proposta
- Adicionar `tracing::warn!(target: "enrich", stderr = %stderr_str.trim(), "codex failed")`
### Arquivos Afetados
- `src/commands/enrich.rs`


## Conformidades Verificadas — Processos Externos
### Construção de Comandos (§2)
- Argumentos passados individualmente via `.arg()` — NUNCA concatenação
- `which::which` usado em `claude_runner.rs` e `ingest_codex.rs` antes de spawn
- Versão mínima validada antes de spawn (`MIN_CLAUDE_VERSION`, `MIN_CODEX_VERSION`)
### Segurança Contra Injeção (§3)
- `env_clear()` em TODOS os spawns de LLM (claude_runner, ingest_claude, ingest_codex, enrich)
- Nenhuma invocação via shell (`sh -c`, `cmd /c`) — sempre invocação direta
- Nenhuma interpolação de input externo em argumentos
- Whitelist explícita de variáveis de ambiente injetadas
### Configuração de Streams (§3b)
- stdin/stdout/stderr SEMPRE configurados explicitamente em cada `Command`
- `Stdio::null()` para daemon (descarta todos os streams)
- `Stdio::piped()` para captura de output de LLM
### Prevenção de Deadlocks (§4)
- `ingest_claude.rs` e `ingest_codex.rs`: thread dedicada para stdin write — CORRETO
- `wait_timeout` garante que parent não bloqueia indefinidamente
### Timeouts e Cancelamento (§6)
- `wait-timeout` crate usado para timeout cross-platform
- `child.kill()` seguido de `child.wait()` em timeout — CORRETO (evita zumbi)
- Timeout configurável via `--claude-timeout` e `--codex-timeout`
### Ambiente e Contexto (§7)
- `daemon.rs:622-626` remove `LD_PRELOAD`, `LD_LIBRARY_PATH`, `LD_AUDIT`, `DYLD_INSERT_LIBRARIES`
- `claude_runner.rs:178` e equivalentes usam `env_clear()` + whitelist seletiva
- Windows vars injetadas via `#[cfg(windows)]` em `claude_runner.rs` e `ingest_codex.rs`
### Encoding e Parsing (§8)
- `String::from_utf8()` com `map_err` (não unwrap) para version check
- `String::from_utf8_lossy` para stderr em diagnóstico (aceitável — fidelidade não crítica)
- JSON parsing via `serde_json::from_str` com erro tipado
### Portabilidade (§9)
- `#[cfg(target_os = "linux")]` isola `pre_exec` com `setsid` e `setrlimit`
- `#[cfg(not(target_os = "linux"))]` fornece fallback sem memory limit
- `#[cfg(windows)]` para env vars Windows-specific
### Ciclo de Vida do Child (§5)
- `child.wait_timeout()` + `child.kill()` + `child.wait()` — padrão correto
- Daemon detach: DOCUMENTADO com SAFETY comment (§19 child detach justificado)
- Lock file previne spawns concorrentes do daemon
### Extensões de Plataforma (§12)
- `pre_exec` unsafe com SAFETY comment documentando invariantes
- Operações dentro do closure são async-signal-safe (`setsid`, `setrlimit`)
- Fallback em non-Linux não usa pre_exec


## Auditoria — Tratamento de Erros (rules_rust_tratamento_de_erros.md)
### Data da Auditoria
- 2026-05-31
- Arquivo de regras: `docs_rules/rules_rust_tratamento_de_erros.md` (1031 linhas, 19 seções)
- Escopo: todos os 83 arquivos `.rs` em `src/`
### Ferramentas Utilizadas
- `context7 library thiserror --json` (trustScore 9.7)
- `context7 library anyhow --json` (trustScore 9.3)
- `context7 docs /websites/rs_thiserror --query "non_exhaustive error enum best practice" --text`
- `duckduckgo-search-cli -q -n 5 -f json "rust thiserror non_exhaustive best practice error enum 2025"`
- `rg`, `sg`, `bat` para varredura de anti-patterns


## TE01 HIGH — AppError sem #[non_exhaustive]
### Problema
- `AppError` é enum público em `src/errors.rs:17` SEM atributo `#[non_exhaustive]`
- Adicionar variante nova é breaking change para qualquer dependente que faça `match`
- Precedente real: `BatchPartialFailure` adicionado na v2.0.0 exigiu major bump
### Regra Violada
- §6 linha 195: "MARCAR enum público com `#[non_exhaustive]`"
- §19 linha 948: "ADICIONAR variantes apenas em versões menores sob `#[non_exhaustive]`"
- §19 linha 944: "MARCAR enum público com `#[non_exhaustive]`"
### Causa-Efeito
- CAUSA: enum público sem `#[non_exhaustive]`
- EFEITO: adição de variante quebra match exaustivo de dependentes
- EFEITO: evolução do enum requer major bump em vez de minor
### Solução Proposta
- Adicionar `#[non_exhaustive]` ao enum `AppError`
- Adicionar arm `_ => 1` no match de exit_code (wildcard catch-all)
- Documentar política de SemVer no rustdoc do enum
### Arquivos Afetados
- `src/errors.rs`
### Severidade
- HIGH — viola contrato público de SemVer


## TE02 HIGH — Ausência de .context() na propagação de erros
### Problema
- ZERO uso de `.context()` ou `.with_context()` em 80+ arquivos de comandos
- Exceção: apenas `src/extraction.rs` usa `.with_context()` (6 sites)
- Erros chegam ao usuário sem cadeia narrativa de "o que estava sendo feito"
- Diagnóstico prejudicado: erro "database error: UNIQUE constraint failed" sem contexto de "while remembering memory 'design-auth'"
### Regra Violada
- §7 linha 255: "ANEXAR `.context(...)` em cada chamada fallível"
- §7 linha 256: "USAR `.with_context(|| format!(...))` para contexto com dados dinâmicos"
- §7 linha 258: "INCLUIR caminho de arquivo, URL, ID de recurso no contexto"
- §7 linha 259: "FORMAR uma cadeia de contextos legível do geral ao específico"
### Causa-Efeito
- CAUSA: propagação com `?` puro sem `.context()`
- EFEITO: mensagens de erro planas sem contexto de camada
- EFEITO: operadores não conseguem diagnosticar qual operação falhou
- EFEITO: logs sem correlação entre erro e operação sendo executada
### Solução Proposta
- Adicionar `.context("while ...")` nos 20 caminhos mais críticos:
  - `remember.rs` — "while persisting memory '{name}'"
  - `ingest.rs` — "while ingesting file '{path}'"
  - `recall.rs` — "while searching for '{query}'"
  - `enrich.rs` — "while enriching entity '{name}'"
  - `edit.rs` — "while editing memory '{name}'"
  - `link.rs` — "while linking '{from}' to '{to}'"
- Converter `AppError::Database(#[from])` para usar `.context()` antes de `?`
### Arquivos Afetados
- 38+ arquivos de comandos em `src/commands/`
- `src/storage/memories.rs`, `src/storage/entities.rs`
### Severidade
- HIGH — toda a cadeia de erros perde contexto narrativo


## TE03 MEDIUM — Validation(String) como catch-all genérico
### Problema
- `AppError::Validation(String)` mistura 70+ causas distintas num único bucket
- Causas misturadas: "binary not found", "invalid field", "rate limited", "timeout", "parse error", "max_turns exhausted"
- Caller não consegue fazer match programático para decidir retry ou categorizar
- Viola §6 linha 194: "EVITAR variantes catch-all com `String` sem estrutura"
### Regra Violada
- §6 linha 192: "NOMEAR variantes por causa da falha e não por sintoma"
- §6 linha 194: "EVITAR variantes catch-all com `String` sem estrutura"
- §11 linha 478: "EXPOR método ou campo que indique se a falha tolera retry"
### Causa-Efeito
- CAUSA: variante genérica aceita qualquer string
- EFEITO: impossível distinguir "rate limit" de "invalid field" programaticamente
- EFEITO: lógica de retry precisa parsear string para decidir (frágil)
- EFEITO: exit code 1 para todas as validações impede automação granular
### Solução Proposta
- Extrair sub-variantes para categorias frequentes:
  - `BinaryNotFound { name: String }`
  - `RateLimited { detail: String }`
  - `Timeout { operation: String, duration_secs: u64 }`
  - `ParseError { context: String, detail: String }`
- Manter `Validation(String)` como fallback residual
- Associar exit codes distintos às novas variantes
### Arquivos Afetados
- `src/errors.rs` (definição)
- 38+ arquivos de comandos (call sites)
### Severidade
- MEDIUM — dificulta automação e retry programático


## TE04 MEDIUM — Seção # Errors ausente em pub fns fallíveis
### Problema
- Apenas 13 funções públicas têm `/// # Errors` documentado
- ~50+ funções públicas retornando `Result` sem documentação de erro
- `cargo doc` não comunica ao caller quais falhas esperar
### Regra Violada
- §3 linha 90: "DOCUMENTAR seção `# Errors` em cada função pública fallível"
- §3 linha 91: "LISTAR cada variante de erro possível com a condição que a causa"
- §3 linha 92: "DESCREVER qual dado do erro é útil para o chamador"
### Causa-Efeito
- CAUSA: funções fallíveis sem documentação de variantes de erro
- EFEITO: caller precisa inspecionar código para saber quais erros esperar
- EFEITO: `docs.rs` incompleto para consumidores da crate
### Solução Proposta
- Priorizar documentação nas 20 funções mais usadas:
  - `storage/memories.rs`: `insert_memory`, `update_memory`, `get_memory_by_name`
  - `storage/entities.rs`: `create_entity`, `link_memory_entity`
  - `embedder.rs`: `embed_passage`, `embed_query`, `get_embedder`
  - `graph.rs`: `traverse_from_memories`
  - `namespace.rs`: `detect_namespace`, `resolve_namespace`
### Arquivos Afetados
- `src/storage/memories.rs`, `src/storage/entities.rs`
- `src/embedder.rs`, `src/graph.rs`, `src/namespace.rs`
- `src/chunking.rs`, `src/tokenizer.rs`
### Severidade
- MEDIUM — API pública sem contrato documentado de falha


## TE05 MEDIUM — Variante Internal mistura semântica com anyhow
### Problema
- `Internal(#[from] anyhow::Error)` funciona como catch-all transparente
- Qualquer erro que implemente `Error` é absorvido via `anyhow::Error` → `AppError::Internal`
- Não há separação clara entre "erro interno inesperado" e "erro que deveria ter variante própria"
- Recomendação da regra é `#[error(transparent)] Other(#[from] anyhow::Error)`
### Regra Violada
- §8 linha 330: "INCLUIR variante `#[error(transparent)] Outro(#[from] anyhow::Error)`"
- §8 linha 331: "PERMITIR que erros inesperados subam sem perder contexto"
### Causa-Efeito
- CAUSA: `Internal` absorve erros que deveriam ter variante tipada
- EFEITO: exit code 20 para erros que poderiam ter código mais específico
- EFEITO: difícil distinguir "bug real" de "erro de dependência mal mapeado"
### Solução Proposta
- Renomear `Internal` para `Other` ou `Unexpected`
- Adicionar `#[error(transparent)]` para delegar Display ao inner
- Auditar call sites que produzem `Internal` e extrair variantes quando padrão emerge
### Arquivos Afetados
- `src/errors.rs`
### Severidade
- MEDIUM — semântica confusa entre bug e erro externo


## TE06 LOW — Erros de queue DB engolidos sem log em enrich.rs
### Problema
- 10+ sites em `enrich.rs` usam `let _ = queue_conn.execute(...)` sem log
- Exemplos: linhas 804, 1084, 1211, 1222, 1229, 1235, 1450, 1504, 1528, 1538
- Falha silenciosa de UPDATE na queue DB impede rastreamento de progresso
### Regra Violada
- §1 linha 22: "NUNCA engolir erro com `let _ = operacao()` sem razão documentada"
- §16 linha 140: "USAR `.inspect_err(|e| tracing::warn!(?e))` para observar sem consumir"
### Causa-Efeito
- CAUSA: `let _ =` descarta resultado de execução SQL
- EFEITO: queue pode ficar em estado inconsistente sem diagnóstico
- EFEITO: operador não sabe que tracking falhou
### Solução Proposta
- Substituir `let _ = queue_conn.execute(...)` por:
  `if let Err(e) = queue_conn.execute(...) { tracing::warn!(target: "enrich", error = %e, "queue update failed"); }`
### Arquivos Afetados
- `src/commands/enrich.rs`
### Severidade
- LOW — afeta observabilidade, não funcionalidade


## TE07 LOW — eprintln! em vez de tracing para erro final
### Problema
- `output.rs:147` usa `eprintln!` para emitir erro ao usuário
- Válido para CLI mas impede integração com monitoring/telemetry
### Regra Violada
- §16 linha 641: "NUNCA confiar em `eprintln!` como substituto de log estruturado"
### Causa-Efeito
- CAUSA: output direto em stderr sem passar por tracing
- EFEITO: erro final não aparece em logs estruturados
- EFEITO: monitoring não captura falhas de CLI
### Solução Proposta
- Adicionar `tracing::error!` ANTES do `eprintln!` para dual-emit
- Manter `eprintln!` para UX humana
### Arquivos Afetados
- `src/output.rs`
### Severidade
- LOW — afeta apenas integração com telemetria externa


## TE08 LOW — Sem método is_retryable() no tipo de erro
### Problema
- Lógica de retry dispersa em enrich.rs, ingest_claude.rs, ingest_codex.rs
- Cada caller decide individualmente se deve retry (parsing string "rate_limit")
- Sem API programática para determinar se erro tolera retry
### Regra Violada
- §11 linha 478: "EXPOR método ou campo que indique se a falha tolera retry"
- §11 linha 479: "CARREGAR em cada variante a informação de idempotência"
### Causa-Efeito
- CAUSA: tipo de erro não expõe `is_retryable()`
- EFEITO: lógica de retry duplicada e frágil (string matching)
- EFEITO: novos callers precisam reimplementar classificação
### Solução Proposta
- Adicionar `pub fn is_retryable(&self) -> bool` em `AppError`
- Classificar: `DbBusy`, `LockBusy`, `AllSlotsFull`, `LowMemory` → true
- Classificar: `Validation`, `NotFound`, `Duplicate` → false
### Arquivos Afetados
- `src/errors.rs`
### Severidade
- LOW — DRY violation na lógica de retry


## TE09 LOW — unwrap_or_default em campos de JSON externo
### Problema
- `enrich.rs:2473,2492,2493` usa `.unwrap_or_default()` em campos JSON de resposta LLM
- Se LLM retornar campo `name` vazio, entidade é criada com nome vazio em vez de falhar
### Regra Violada
- §4 linha 136: "USAR `.unwrap_or_default()` apenas com análise explícita do default"
- §1 linha 23: "NUNCA usar `.unwrap_or_default()` sem analisar se default mascara bug"
### Causa-Efeito
- CAUSA: `.unwrap_or_default()` em dados não confiáveis de LLM
- EFEITO: entidades com nome "" criadas silenciosamente
- EFEITO: grafo poluído com nós sem significado
### Solução Proposta
- Substituir por `.ok_or_else(|| AppError::Validation("LLM returned empty entity name"))?`
- Ou filtrar: `if name.is_empty() { continue; }`
### Arquivos Afetados
- `src/commands/enrich.rs`
### Severidade
- LOW — pode criar dados inválidos no grafo


## TE10 LOW — knn_search erro engolido com unwrap_or_default
### Problema
- `deep_research.rs:857` — `knn_search(...).unwrap_or_default()` engole silenciosamente erro de busca vetorial
- Se o embedding falhar ou o sqlite-vec retornar erro, resultado é vetor vazio sem aviso
### Regra Violada
- §1 linha 22: "NUNCA engolir erro com `let _ = operacao()` sem razão documentada"
- §4 linha 136: "USAR `.unwrap_or_default()` apenas com análise explícita do default"
### Causa-Efeito
- CAUSA: `.unwrap_or_default()` em operação de busca
- EFEITO: deep-research silenciosamente perde resultados de entity KNN
- EFEITO: diagnóstico impossível quando busca falha
### Solução Proposta
- Substituir por `match` com `tracing::warn!` no branch Err
- Ou usar `.inspect_err(|e| tracing::warn!(...)).unwrap_or_default()`
### Arquivos Afetados
- `src/commands/deep_research.rs`
### Severidade
- LOW — silencia falhas de busca vetorial


## TE11 LOW — Falta de .context() em extraction.rs parcial
### Problema
- `extraction.rs` usa `.with_context()` em 6 sites (bom)
- Mas `RegexExtractor.extract()` e sub-funções de parsing NÃO adicionam contexto
- Erros de regex propagam sem indicar qual padrão falhou
### Regra Violada
- §7 linha 255: "ANEXAR `.context(...)` em cada chamada fallível"
### Causa-Efeito
- CAUSA: propagação parcial com contexto
- EFEITO: algumas falhas de extraction chegam sem contexto
### Solução Proposta
- Adicionar `.context("while running regex extraction")` nas sub-funções
### Arquivos Afetados
- `src/extraction.rs`
### Severidade
- LOW — inconsistência interna


## TE12 LOW — Considerar Box<str> para variantes String
### Problema
- `Validation(String)`, `Embedding(String)`, `Duplicate(String)` carregam `String` (24 bytes)
- Teste de tamanho garante ≤128 bytes (CONFORME)
- Mas `Box<str>` reduziria para 16 bytes (pointer + len) por variante
### Regra Violada
- §14 linha 926: "EVITAR enums de erro enormes que inflam `Result<T, E>` no stack"
- §14 linha 928: "CONSIDERAR `Box<MeuErro>` quando o erro for grande e raro"
### Causa-Efeito
- CAUSA: `String` aloca heap mas ocupa 24 bytes no enum discriminant
- EFEITO: Result<T, AppError> maior que necessário no caminho comum
### Solução Proposta
- Converter `String` para `Box<str>` nas variantes que não precisam de mutabilidade
- Alternativa: manter como está — teste de 128 bytes garante budget
### Arquivos Afetados
- `src/errors.rs`
### Severidade
- LOW — otimização, não funcional


## TE13 INFO — panic! em stdin_helper.rs como invariante
### Problema
- `stdin_helper.rs:84` usa `panic!("unexpected error variant: {other:?}")`
- Contexto é match exaustivo onde apenas uma variante de erro é esperada
- Tecnicamente é invariante interna legítima (§3 linha 98)
### Regra Violada
- §3 linha 98: "PERMITIR panic em estado comprovadamente impossível de atingir" — CONFORME
- Alternativa melhor: usar `unreachable!()` ou `.expect("invariant: ...")`
### Causa-Efeito
- CAUSA: panic em branch impossível
- EFEITO: nenhum em runtime (branch jamais atingido)
### Solução Proposta
- Substituir `panic!` por `unreachable!("error variant should be Timeout or Empty: {other:?}")`
### Arquivos Afetados
- `src/stdin_helper.rs`
### Severidade
- INFO — estilo, não funcional


## Conformidades Verificadas — Tratamento de Erros
### Enum tipado com thiserror (§6)
- `AppError` usa `#[derive(Error, Debug)]` com 16 variantes nomeadas por causa
- Cada variante tem mensagem `#[error("...")]` descritiva
- Conversões automáticas via `#[from]` para `rusqlite::Error`, `io::Error`, `anyhow::Error`, `serde_json::Error`
### Zero unwrap em produção (§4)
- Todos os ~200 `.unwrap()` encontrados estão dentro de `#[cfg(test)]` modules
- Código de produção usa `?`, `.map_err()`, `.ok_or_else()` consistentemente
### Exit codes distintos (§15)
- `exit_code()` mapeia 16 variantes para códigos estáveis documentados
- Códigos seguem convenção UNIX (0=sucesso, 1=validação, 2=Clap, 75=tempfail)
### main() com ExitCode (§5)
- `fn main() -> std::process::ExitCode` — jamais panic no caminho principal
- Propagação via match sobre `Result` com formatação controlada
### Mensagens minúsculas sem ponto final (§9)
- Todas as 16 mensagens `#[error("...")]` iniciam minúsculas
- Nenhuma termina com ponto final
### Dados estruturados em variantes (§6)
- `BatchPartialFailure { total, failed }` — campos nomeados
- `AllSlotsFull { max, waited_secs }` — campos nomeados
- `LowMemory { available_mb, required_mb }` — campos nomeados
### Tamanho do erro verificado (§14)
- Teste `app_error_size_does_not_exceed_budget()` garante `size_of::<AppError>() <= 128`
### let _ = com justificativa (§1)
- `main.rs`: flush de stdout/stderr antes de exit — tolerável (melhor esforço)
- `OnceLock::set()` em i18n.rs, tz.rs, embedder.rs — set ignorado em chamada duplicada (design do OnceLock)
- `child.kill()` + `child.wait()` em timeout — tolerável (processo já morto)
### Retry com backoff (§11)
- `enrich.rs` implementa backoff exponencial (60s→120s→300s→900s)
- `ingest_claude.rs` implementa backoff com retry counter
### i18n de erros (§9b)
- `localized_message_for(Language)` traduz sem poluir enum com strings PT
- Mensagens EN no `#[error]` servem como single source of truth
### Display curto, Debug verbose (§9)
- thiserror gera `Display` com mensagem curta
- `#[derive(Debug)]` gera representação completa automaticamente
### Segurança de dados sensíveis (§10)
- Nenhuma mensagem de erro contém token, API key ou credential
- `env_clear()` previne leak de environment em subprocessos
- Mascaramento de API keys implementado em `storage.rs` (12 primeiros + 4 últimos)


## Auditoria — Retry com Backoff (rules_rust_retry_com_backoff.md)
### Contexto
- Arquivo de regras: `docs_rules/rules_rust_retry_com_backoff.md` (1070 linhas, 17 seções)
- Projeto: CLI síncrona Rust, sem async runtime
- Áreas de retry ativas: `storage/utils.rs`, `daemon.rs`, `lock.rs`, `enrich.rs`, `ingest_claude.rs`, `ingest_codex.rs`
- Fontes: context7 `/websites/rs_backon_1_6_0` (trustScore 9.7), duckduckgo-search-cli
- Data da auditoria: 2026-05-31


## RB01 HIGH — Classificação de erro via string matching
### Regra violada
- §2 L84: "NUNCA usar string matching em mensagens de erro"
- §9 L452: "NUNCA decidir retry via error.to_string().contains(...)"
### Evidência
- `enrich.rs:1230`: `if err_str.contains("RATE_LIMITED")`
- `enrich.rs:1532`: `if err_str.contains("RATE_LIMITED")`
- `ingest_claude.rs:842`: `Err(ref e) if format!("{e}").contains("RATE_LIMITED")`
- `ingest_claude.rs:1169`: `if err_str.contains("RATE_LIMITED")`
- `ingest_codex.rs:897`: `Err(ref e) if format!("{e}").contains("RATE_LIMITED")`
- `ingest_codex.rs:1089`: `if err_str.contains("RATE_LIMITED")`
- `claude_runner.rs:297`: produtor emite `AppError::Validation(format!("RATE_LIMITED: ..."))`
### Impacto
- Classificação frágil que quebra se mensagem de erro mudar
- Impossível para downstream distinguir rate-limit programaticamente
### Correção proposta
- Usar `matches!(e, AppError::RateLimited { .. })` — variante JÁ existe desde TE03
- Migrar produtores para emitir `AppError::RateLimited { detail }` em vez de `Validation`


## RB02 HIGH — Ausência de RetryConfig struct parametrizável
### Regra violada
- §15 L709-717: "EXPOR configuração via struct dedicada RetryConfig"
- §15 L729: "NUNCA enterrar max_attempts como literal mágico"
### Evidência
- `enrich.rs:45`: `const DEFAULT_RATE_LIMIT_WAIT: u64 = 60` (hardcoded)
- `constants.rs:49`: `MAX_SQLITE_BUSY_RETRIES: u32 = 5` (hardcoded)
- `constants.rs:55`: `SQLITE_BUSY_BASE_DELAY_MS: u64 = 300` (hardcoded)
- `ingest_claude.rs:827`: `let max_extract_attempts: u32 = 2` (literal inline)
- `daemon.rs`: `sleep_ms = (sleep_ms * 2).min(500)` (cap hardcoded)
- Cap de 900s no rate-limit loop sem struct centralizada
### Impacto
- Impossível alterar política sem recompilar
- Impossível desabilitar retry em runtime para debugging
- Violação de DRY com políticas duplicadas em 4 arquivos
### Correção proposta
- Criar `pub struct RetryConfig { initial_delay_ms, max_delay_ms, multiplier, max_attempts, max_elapsed_ms, jitter_kind }`
- Derivar `Debug, Clone, Default, Deserialize`
- Instanciar configurações nomeadas por dependência (sqlite, llm_rate_limit, daemon_spawn)


## RB03 MEDIUM — Sem is_permanent() nem retry_kind() complementares
### Regra violada
- §2 L61-62: "EXPOR método is_permanent como complemento explícito"
- §2 L62: "EXPOR método retry_kind retornando enum detalhado"
### Evidência
- `errors.rs:198`: Apenas `is_retryable()` existe
- Sem `pub fn is_permanent(&self) -> bool`
- Sem `pub enum RetryKind { Transient, Permanent, Unknown }`
### Impacto
- Callers inferem permanência via negação de `is_retryable()`
- Sem distinção entre "permanente" e "desconhecido/não-classificado"
### Correção proposta
- Adicionar `pub fn is_permanent(&self) -> bool` como complemento
- Considerar `pub fn retry_kind(&self) -> RetryKind` para decisão granular


## RB04 MEDIUM — Jitter ausente no backoff do rate-limit LLM
### Regra violada
- §5 L218-220: "APLICAR jitter em TODA política de retry de rede"
- §5 L226: "NUNCA omitir jitter em cliente distribuído"
### Evidência
- `enrich.rs:1233`: `w_backoff = (w_backoff * 2).min(900)` — sem jitter
- `enrich.rs:1541`: `backoff_secs = (backoff_secs * 2).min(900)` — sem jitter
- `ingest_claude.rs:1180`: `backoff_secs = (backoff_secs * 2).min(900)` — sem jitter
- `ingest_codex.rs:1100`: `backoff_secs = (backoff_secs * 2).min(900)` — sem jitter
### Impacto
- Workers em paralelo que recebem rate-limit retentam no MESMO instante
- Thundering herd problem amplifica a contenção no servidor remoto
### Nota
- `storage/utils.rs` e `daemon.rs` JÁ usam half-jitter — inconsistência interna
### Correção proposta
- Aplicar half-jitter: `let half = backoff_secs / 2; backoff_secs = half + fastrand::u64(0..half.max(1));`
- Manter `min(900)` como cap após jitter


## RB05 MEDIUM — Tracing estruturado incompleto em retries
### Regra violada
- §12 L619-627: "INCLUIR campos estruturados: attempt, delay_ms, error_kind"
- §12 L620: "EMITIR tracing::error em esgotamento de tentativas"
### Evidência
- `ingest_claude.rs:849`: emite `attempt` e `error` mas falta `delay_ms`, `max_attempts`
- `enrich.rs:1533`: emite `wait_seconds` mas falta `attempt.number`, `attempt.max`, `error_kind`
- NENHUM site emite `tracing::error` em esgotamento do rate-limit loop
- `storage/utils.rs`: ZERO observabilidade — nenhum tracing em tentativa individual nem exhaustion
### Impacto
- Impossível monitorar taxa de retries ou diagnosticar storms
- Exaustão silenciosa do busy-retry
### Correção proposta
- Enriquecer com `attempt`, `attempt_max`, `delay_ms`, `error_kind`
- Emitir `tracing::error` quando rate-limit exhaust deadline ou budget


## RB06 MEDIUM — Sem deadline total no rate-limit backoff loop
### Regra violada
- §6 L258-264: "COMBINAR max_attempts e max_elapsed_time simultaneamente"
- §6 L271: "NUNCA usar max_attempts: None sem deadline total"
### Evidência
- `enrich.rs:1232-1234`: loop rate-limit sem max_attempts nem deadline temporal
- `ingest_claude.rs:1179-1180`: idem
- `ingest_codex.rs:1099-1100`: idem
- Backoff cresce até 900s mas loop NUNCA termina por esgotamento temporal
### Impacto
- Endpoint em rate-limit permanente bloqueia worker INDEFINIDAMENTE
- Processo pode travar por horas sem deadline
### Nota
- Loop geral tem `budget` de custo como safeguard parcial, mas NÃO temporal
### Correção proposta
- Adicionar `let deadline = Instant::now() + Duration::from_secs(max_elapsed_secs)`
- Checar `if Instant::now() >= deadline { break; }` antes de cada sleep
- Valor sugerido: `3600s` (1 hora) como deadline total de retry


## RB07 LOW — Retry fixo de 2s sem backoff para cold-start
### Regra violada
- §4 L186-192: "NUNCA retentar imediatamente sem espera inicial"
- §4 L187: "NUNCA usar backoff constante em falhas de overload externo"
### Evidência
- `ingest_claude.rs:850`: `std::thread::sleep(Duration::from_secs(2))` fixo entre tentativas
- `ingest_codex.rs:910`: idem
### Impacto
- Menor: cenário de cold-start com máximo 2 tentativas
- Viola princípio mas risco prático é baixo (operação rara)
### Correção proposta
- Usar backoff: `std::thread::sleep(Duration::from_secs(2 * attempt as u64))`


## RB08 LOW — Ausência de kill switch para retry
### Regra violada
- §15 L723-726: "EXPOR flag global para desabilitar retry em emergência"
- §15 L19: "EXPOR feature flag global para kill switch durante incidente"
### Evidência
- Nenhum mecanismo `--disable-retry` nem env var `SQLITE_GRAPHRAG_DISABLE_RETRY`
- Sem feature flag para desabilitar retry em runtime
### Impacto
- Durante incidentes, sem forma de prevenir retry storms sem matar processos
### Correção proposta
- Adicionar env var `SQLITE_GRAPHRAG_DISABLE_RETRY=1` verificada em entrada de cada loop de retry
- Quando ativo, propagar erro imediatamente sem retentar


## RB09 LOW — Nenhum crate de retry adotado
### Regra violada
- §16 L759-764: "PREFERIR backon para projetos novos"
- §16 L778: "NUNCA reimplementar retry quando crate maduro resolve"
### Evidência
- Todo retry implementado via loops `for/while` + `thread::sleep` manuais
- Código duplicado em `enrich.rs`, `ingest_claude.rs`, `ingest_codex.rs`, `storage/utils.rs`
### Impacto
- Sem garantias de jitter, deadline, notify por design
- Manutenção N-vezes duplicada
### Atenuante
- CLI é síncrona; `backon` foca em async
- `backoff` crate suporta modo blocking sync via `backoff::retry`
### Correção proposta
- Avaliar adoção de `backoff` crate com `Operation` sync
- Ou extrair função interna `retry_with_backoff<F>(config: &RetryConfig, op: F)` reutilizável


## RB10 LOW — Sem separação por camada de erro
### Regra violada
- §2 L64-69: "DIFERENCIAR erro de DNS de TCP de TLS de HTTP"
### Evidência
- `AppError::RateLimited` é HTTP-layer mas sem tag de camada
- `AppError::Timeout` pode ser process-layer ou network-layer
- Sem enum `ErrorLayer { Dns, Tcp, Tls, Http, Application, Process }`
### Impacto
- Menor: CLI não faz chamadas HTTP diretas (delega para subprocess)
- Callers não podem aplicar políticas diferenciadas por camada
### Correção proposta
- Considerar campo opcional `layer` nas variantes de rede/timeout quando relevante


## RB11 LOW — Sem tracing::error em exhaustion do SQLite busy-retry
### Regra violada
- §12 L640: "NUNCA engolir erro após exhaustion sem log"
### Evidência
- `storage/utils.rs:58`: Converte para `DbBusy` mas NÃO emite `tracing::error`
- Zero observabilidade durante o retry loop (nenhum warn por tentativa)
### Impacto
- Exaustão silenciosa no canal de observabilidade
- Só visível no JSON output final ao usuário
### Correção proposta
- Adicionar `tracing::error!(target: "storage", retries = MAX_SQLITE_BUSY_RETRIES, "SQLITE_BUSY exhausted all retries")`
- Considerar `tracing::warn` por tentativa individual com attempt number


## RB12 LOW — Polling com sleep fixo em lock.rs
### Regra violada
- §4 L187: "NUNCA usar backoff constante em falhas de overload externo"
### Evidência
- `lock.rs:106`: `thread::sleep(Duration::from_millis(CLI_LOCK_POLL_INTERVAL_MS))` — fixo
### Impacto
- Menor: file-lock local, não rede
- Já tem deadline temporal como safeguard
### Atenuante
- Polling de file-lock é I/O-bound local, contenção é rara
- Backoff exponencial pode aumentar latência em caso comum (lock liberado em < 1 ciclo)
### Correção proposta
- Considerar backoff leve: `sleep_ms = sleep_ms.min(CLI_LOCK_POLL_INTERVAL_MS * 4)` com incremento


## RB13 INFO — Sem ADR documentando decisões de retry
### Regra violada
- §1 L22-24: "REGISTRAR decisão arquitetural via ADR antes de introduzir retry"
### Evidência
- Nenhum ADR em `docs/decisions/` sobre política de retry
- Valores como `900s` cap, `60s` initial wait, `5` max SQLite retries sem documentação formal
### Impacto
- Conhecimento tribal: contribuidores novos não sabem justificativa dos valores
### Correção proposta
- Criar `docs/decisions/adr-NNN-retry-policy.md` documentando cada política e justificativa


## RB14 INFO — thread::sleep em deadlock-detection (FALSE POSITIVE)
### Regra verificada
- §7 L305: "NUNCA usar std::thread::sleep em código async"
### Evidência
- `main.rs:99`: `std::thread::sleep(Duration::from_secs(10))` em thread spawned
### Status
- FALSE POSITIVE: está em `std::thread::spawn` dedicada, NÃO em código async
- Uso correto para monitoramento de deadlocks do parking_lot


## RB15 INFO — Ausência de retry_after() method em AppError
### Regra verificada
- §2 L102: "Função retry_after(&self) -> Option<Duration> quando servidor indica"
### Evidência
- Rate-limit do Claude/Codex não expõe header Retry-After
- Duração inferida via backoff exponencial interno, não via servidor
### Impacto
- Mínimo: servidor remoto (subprocess) não retorna duration explícita
### Correção proposta
- Se/quando Claude API expor Retry-After, parsear e expor via campo na variante `RateLimited`


## Conformidades Verificadas — Retry com Backoff
### Política explícita separada (§1)
- `storage/utils.rs`: `with_busy_retry` é função dedicada separada da lógica de negócio
- `daemon.rs`: spawn_backoff_state é mecanismo persistente em disco separado do spawn
### Backoff exponencial truncado (§4)
- `storage/utils.rs:47-48`: `base_ms * (1 << attempt)` com truncamento implícito no loop
- `daemon.rs:548`: `(sleep_ms * 2).min(500)` — truncado a 500ms
- `daemon.rs:677`: `(sleep_ms * 2).min(DAEMON_AUTO_START_MAX_BACKOFF_MS)` — truncado
### Relógio monotônico (§4)
- `daemon.rs:539, 669`: `Instant::now()` para deadlines
- `lock.rs:104`: `Instant::now() + Duration::from_secs(wait_secs)`
- ZERO uso de `SystemTime` para medir intervalos de retry
### Jitter aplicado (§5) — parcial
- `storage/utils.rs:49`: half-jitter via `fastrand::u64(0..half)`
- `daemon.rs:746`: half-jitter via `fastrand::u64(0..half)`
- Conformidade: SQLite busy e daemon spawn
- Gap: rate-limit LLM sem jitter (RB04)
### Critérios múltiplos de parada (§6)
- `storage/utils.rs`: `MAX_SQLITE_BUSY_RETRIES` (5 tentativas)
- `daemon.rs:539`: deadline temporal + polling
- `lock.rs:104`: deadline temporal + timeout
### Classificação via enum (§2)
- `errors.rs:198`: `is_retryable()` exposto em `AppError` com 6 variantes transientes
- Classificação por tipo, não por mensagem (parcial — RB01 viola em call sites)
### thiserror implementado (§2)
- `errors.rs`: `#[derive(Error, Debug)]` com `thiserror::Error`
### Retry em apenas uma camada (§9)
- Rate-limit retry ocorre APENAS no loop de ingest/enrich — subprocess NÃO retenta internamente
- SQLite busy-retry ocorre APENAS em `with_busy_retry` — callers NÃO retentam
### Proibido retry infinito (§6)
- Todos os loops têm `max_attempts` finito OU deadline OU budget
- Exceção parcial: rate-limit loop sem deadline (RB06)


## GS01 HIGH — Comandos de longa duração não checam shutdown_requested()
### Problema
- `ingest_claude.rs`, `ingest_codex.rs`, `enrich.rs`, `ingest.rs` contêm loops iterando sobre arquivos/entidades
- NENHUM desses loops checa `crate::shutdown_requested()` ou `cancel_token().is_cancelled()`
- Ctrl+C/SIGTERM seta o flag mas NADA interrompe a iteração em curso
### Seções Violadas
- §4 L209: "INCLUIR ramo de cancelamento em todo loop longo"
- §4 L228: "NUNCA escrever loop sem ramo de cancelamento"
- §11 L585: "CHECAR flag AtomicBool em jobs iterativos do rayon"
### Consequências
- Ctrl+C durante `ingest --mode claude-code` de 100 arquivos aguarda TODOS terminarem
- SIGTERM de systemd/k8s ignorado durante operação LLM de minutos
- Processo excede `terminationGracePeriodSeconds` e recebe SIGKILL
### Causa Raiz
- Infraestrutura de shutdown (SHUTDOWN + CancellationToken) existe em lib.rs
- Daemon usa corretamente mas nenhum comando de aplicação integrou
- Possivelmente adicionado como feature mas sem propagação aos callers
### Solução Proposta
- Checar `shutdown_requested()` entre iterações: se true, emitir summary parcial e retornar
- Em `ingest.rs` (rayon): checar antes de enviar cada arquivo ao canal
- Em `ingest_claude.rs`/`ingest_codex.rs`: checar antes de processar próximo arquivo
- Em `enrich.rs`: checar antes de processar próximo item do scan_result
### Status
- OPEN


## GS02 HIGH — Caminho de sucesso (ExitCode::SUCCESS) não faz flush de stdout/stderr
### Problema
- `src/main.rs:342` retorna `std::process::ExitCode::SUCCESS` sem flush prévio
- Todos os caminhos de ERRO fazem flush (L166-167, L337-338), mas o sucesso não
- Dados em buffer do BufWriter interno do stdout podem ser perdidos
### Seções Violadas
- §14 L1048: "FLUSHAR stdout antes de retornar da função main"
- §14 L1049: "FLUSHAR stderr antes de retornar da função main"
### Consequências
- JSON truncado quando processo sai antes do buffer ser drenado
- Afeta pipelines: `sqlite-graphrag recall "x" --json | jaq '.results[]'` pode falhar
- Bug intermitente — depende de timing e tamanho do buffer
### Solução Proposta
- Adicionar 2 linhas antes de `std::process::ExitCode::SUCCESS`:
  ```rust
  let _ = std::io::Write::flush(&mut std::io::stdout());
  let _ = std::io::Write::flush(&mut std::io::stderr());
  ```
### Status
- OPEN


## GS03 HIGH — SIGPIPE não tratado explicitamente
### Problema
- O binário NÃO reseta SIGPIPE para SIG_DFL no início de main
- `output.rs` silencia BrokenPipe nas funções `emit_json*`/`emit_text`
- Mas qualquer println!/eprintln! FORA de output.rs pode panicar com "Broken pipe"
- Rust por padrão ignora SIGPIPE (SIG_IGN), transformando writes em io::Error
### Seções Violadas
- §2 L94: "NUNCA deixar CLI crashar com Broken pipe em app | head -n1"
- §14 L1044: "RESPEITAR SIGPIPE saindo silenciosamente com exit 141"
- §2 L104: "Tratar broken pipe em CLIs saindo com exit code 141"
### Consequências
- `sqlite-graphrag list --json | head -1` pode mostrar stack trace em stderr
- Não segue convenção Unix de exit 141 (128 + 13 SIGPIPE)
- Experiência degradada para uso em pipelines Shell
### Solução Proposta
- Opção A: Resetar SIGPIPE para SIG_DFL no início de main via `libc::signal(libc::SIGPIPE, libc::SIG_DFL)`
- Opção B: Usar `#[unix_sigpipe = "sig_dfl"]` (nightly apenas)
- Opção C: Manter SIG_IGN mas garantir que TODO output vai por output.rs (auditar para println/eprintln soltos)
### Status
- OPEN


## GS04 MEDIUM — Sem escalada por duplo sinal (double Ctrl+C)
### Problema
- `ctrlc::set_handler` seta `SHUTDOWN = true` e cancela token
- Segundo Ctrl+C executa o mesmo handler sem efeito adicional
- Usuário pressionando Ctrl+C 2x espera término imediato (convenção Unix)
### Seções Violadas
- §6 L297: "DETECTAR segundo SIGINT ou SIGTERM durante shutdown"
- §6 L299: "INFORMAR usuário sobre a escalada via log ou stderr"
- §6 L301: "SAIR com código de erro indicando interrupção forçada"
### Consequências
- Usuário sem opção de forçar saída exceto kill -9 externo
- Não segue UX padrão de CLIs Unix (git, cargo, etc.)
### Solução Proposta
- Usar AtomicU8 como counter no handler
- Primeiro sinal: flag + cancel (comportamento atual)
- Segundo sinal: `std::process::exit(130)` imediato com eprintln! de aviso
### Status
- OPEN


## GS05 MEDIUM — Exit codes não seguem convenção Unix 128+N para sinais
### Problema
- Quando shutdown é solicitado via sinal, main retorna `ExitCode::SUCCESS` (0)
- Unix convenção: exit 130 para SIGINT (128+2), 143 para SIGTERM (128+15)
- Orquestradores (systemd, k8s) não distinguem "sucesso" de "terminado por sinal"
### Seções Violadas
- §9 L497: "SEGUIR convenção Unix 128+N para término por sinal N"
- §9 L498: "RETORNAR 130 para término por SIGINT e 143 para SIGTERM"
### Solução Proposta
- Antes de retornar SUCCESS, checar `shutdown_requested()`
- Se true, retornar `ExitCode::from(130u8)` (SIGINT é o sinal mais provável via ctrlc)
### Status
- OPEN


## GS06 MEDIUM — Transações SQLite sem rollback explícito em shutdown
### Problema
- Workers em `ingest.rs` e `enrich.rs` abrem transações para persistir dados
- Se Ctrl+C chega no meio de uma transação, confia-se no Drop de Connection
- SQLite faz rollback automático de transações não committadas via Drop, mas isso é comportamento implícito
### Seções Violadas
- §12 L699: "ROLLBACK explícito em transações não confirmadas após deadline"
- §12 L700: "MARCAR jobs processados com status antes de sair"
### Consequências
- Risco baixo na prática (SQLite WAL é robusto), mas viola contrato explícito
- Jobs parcialmente processados não são marcados, causando reprocessamento no retry
### Solução Proposta
- Checar `shutdown_requested()` ANTES do commit; se true, rollback + break
- Marcar status "interrupted" no queue DB de ingest_claude
### Status
- OPEN


## GS07 MEDIUM — Daemon não usa Runtime::shutdown_timeout
### Problema
- `src/daemon.rs:235`: `rt.block_on(run_async(...))` — ao terminar, runtime é dropado
- Se spawn_blocking tasks estiverem travadas, Drop do runtime pode travar indefinidamente
- Não há `shutdown_timeout` como rede de segurança
### Seções Violadas
- §10 L548: "USAR Runtime::shutdown_timeout para limite explícito de término"
- §10 L562: "NUNCA dropar Runtime com tarefas ativas sem shutdown_timeout"
### Solução Proposta
- Substituir `rt.block_on(...)` por: `rt.block_on(run_async(...)); rt.shutdown_timeout(Duration::from_secs(10));`
- Ou construir pattern: `let result = rt.block_on(f); drop(rt);` com shutdown_timeout
### Status
- OPEN


## GS08 LOW — Daemon sem deadline global configurável para shutdown
### Problema
- Daemon usa idle_shutdown_secs para auto-desligar em inatividade
- Mas não tem deadline para drain de conexões ativas quando shutdown é solicitado
- Um embedding request em andamento pode bloquear indefinidamente
### Seções Violadas
- §5 L250: "DEFINIR deadline global configurável para o shutdown completo"
- §5 L256: "MANTER valor típico entre 5 e 10 segundos para CLIs interativos"
### Solução Proposta
- Adicionar `--shutdown-timeout-secs` flag no daemon (default 10s)
- Aplicar tokio::time::timeout sobre o semáforo wait após cancelamento
### Status
- OPEN


## GS09 LOW — Sem flush explícito de tracing-subscriber antes de sair
### Problema
- `telemetry.rs` inicializa tracing-subscriber com `init()` mas não retorna guard
- No final de main, não há flush explícito do subscriber
- Logs finais (especialmente em JSON format) podem ser perdidos
### Seções Violadas
- §9 L500: "FLUSHAR tracing-subscriber antes de retornar do processo"
- §9 L521: "NUNCA fechar subscriber de tracing antes dos subsistemas que o usam"
### Consequências
- Último log de shutdown pode não chegar a disk/aggregador
- Impacto baixo: stderr não é buffered por padrão em Rust
### Solução Proposta
- tracing-subscriber fmt não requer flush explícito quando writer é stderr
- Marcar como WONTFIX ou documentar que stderr is line-buffered
### Status
- OPEN (possível WONTFIX)


## GS10 LOW — Rayon jobs em ingest --mode none não checam AtomicBool
### Problema
- `ingest.rs` usa `rayon::prelude::*` com `par_iter` para processar arquivos
- Jobs individuais no thread pool não checam `SHUTDOWN` flag
- Rayon não suporta cancelamento nativo — precisa de check manual
### Seções Violadas
- §11 L585: "CHECAR flag AtomicBool em jobs iterativos do rayon"
- §11 L583: "USAR yield_now em jobs longos para permitir cancelamento cooperativo"
### Consequências
- Rayon jobs curtos (chunk+embed) terminam rapidamente (~1-5s)
- Impacto real baixo para este projeto (jobs são CPU-bound mas rápidos)
### Solução Proposta
- Adicionar `if crate::shutdown_requested() { return Err(cancelled) }` no início do closure do par_iter
### Status
- OPEN


## GS11 LOW — Processos filho terminados com SIGKILL sem tentar SIGTERM
### Problema
- `claude_runner.rs:404`: `child.kill()` envia SIGKILL direto (sem SIGTERM prévio)
- `ingest_claude.rs:447`, `ingest_codex.rs:420`, `enrich.rs:2811`: mesmo padrão
- Processo claude -p não tem chance de fazer cleanup
### Seções Violadas
- §15 L847: "ENVIAR SIGTERM ao filho antes de SIGKILL"
- §15 L863: "NUNCA matar filho com SIGKILL como primeira opção"
### Consequências
- Arquivos temporários do claude -p podem ficar órfãos
- Impacto mitigado: wait_timeout dá N seconds de graça antes do kill
- SIGKILL é usado como fallback de timeout, que é razoável como ÚLTIMO recurso
### Solução Proposta
- Antes do `child.kill()`: `libc::kill(child.id() as i32, libc::SIGTERM)` + sleep 2s
- Se não terminar em 2s, então kill()
### Status
- OPEN


## GS12 LOW — Exit code 141 para SIGPIPE não documentado no README
### Problema
- Tabela de exit codes no README (L573-593) não menciona exit 141
- SIGPIPE em pipelines Unix deve produzir exit 141 (128+13)
### Seções Violadas
- §9 L496: "DOCUMENTAR tabela de códigos de saída no README"
- §14 L1044: "RESPEITAR SIGPIPE saindo silenciosamente com exit 141"
### Solução Proposta
- Adicionar linha na tabela: `| 141 | Broken pipe (SIGPIPE) | Stdout closed by downstream consumer in pipeline |`
### Status
- OPEN


## GS13 LOW — Daemon sem arquivo PID dedicado
### Problema
- Daemon emite PID no JSON de inicialização (`DaemonResponse::Listening { pid }`)
- Mas não cria arquivo PID em filesystem (ex: `/tmp/sqlite-graphrag-daemon.pid`)
- Administradores não podem descobrir PID sem parsear stdout ou usar `procs`
### Seções Violadas
- §8 L421: "CRIAR arquivo PID em local padrão ao iniciar daemon"
- §8 L422: "REMOVER arquivo PID no shutdown gracioso"
### Consequências
- Não há forma simples de `kill $(cat pidfile)` para operadores
- Impacto baixo: `daemon --stop` existe como alternativa
### Solução Proposta
- Opcional: criar PID file no daemon_control_dir e remover no DaemonSpawnGuard Drop
### Status
- OPEN


## GS14 INFO — Nenhum teste de encerramento graceful existe
### Problema
- Diretório `tests/` não contém nenhum teste que valide shutdown com sinal
- Cenários não cobertos: duplo sinal, deadline, panic durante drain, crash recovery
### Seções Violadas
- §20 L1092-1100: lista de cenários OBRIGATÓRIOS de teste
- §20 L1118: "NUNCA assumir que funciona sem teste explícito"
### Solução Proposta
- Criar `tests/shutdown_integration.rs` com:
  - Teste de sinal único terminando daemon
  - Teste de Ctrl+C durante ingest (mock ou real)
  - Teste de recovery pós-kill -9
### Status
- OPEN


## GS15 INFO — Shutdown timeout não configurável via env var ou flag
### Problema
- Não existe env var `SHUTDOWN_TIMEOUT_SECS` nem flag `--shutdown-timeout`
- Deadline de drenagem (quando existir) será hardcoded
### Seções Violadas
- §19 L1146: "ACEITAR SHUTDOWN_TIMEOUT_SECS via variável de ambiente"
- §19 L1147: "ACEITAR flag CLI --shutdown-timeout sobrescrevendo env"
### Solução Proposta
- Implementar junto com GS08 quando deadline for adicionado ao daemon
### Status
- OPEN


## Conformidades Verificadas — Encerramento Graceful Shutdown
### Captura de sinais (§2)
- `ctrlc` v3.4 com feature `termination` captura SIGINT + SIGTERM cross-platform
- Handler registrado uma vez em main via `signals::register_shutdown_handler()`
### Propagação de intenção (§3)
- Dual-primitive: `AtomicBool SHUTDOWN` (sync) + `CancellationToken` (async)
- Token cancelado no mesmo handler via `cancel_token().cancel()`
- Pattern correto: flag atômico para polling sync, token para select! async
### RAII para lock files (§8)
- `src/lock.rs`: flock via fs4::FileExt released automaticamente no Drop do File
- `src/daemon.rs:191-218`: DaemonSpawnGuard remove lock file no Drop
- Spawn lock com `try_overwrite(true)` como fallback para crash
### Daemon cooperação com cancelamento (§4)
- `daemon.rs:279`: `if shutdown_requested() || token.is_cancelled() { break }`
- `daemon.rs:322`: `tokio::select!` com `token.cancelled()` no polling loop
### BrokenPipe handling (§14)
- `output.rs`: TODAS as funções emit_* silenciam `ErrorKind::BrokenPipe`
- Pattern correto: retorna Ok(()) em vez de propagar erro
### Flush em error paths (§9)
- `main.rs`: TODOS os caminhos de erro fazem flush stdout+stderr
### Processos filho: wait com timeout (§15)
- `claude_runner.rs:358`: usa `wait_timeout::ChildExt`
- Timeout configurável via `--claude-timeout`/`--codex-timeout`
- Kill + wait após timeout (L404-405)
### Tokio runtime manual (§10)
- `daemon.rs:228-233`: Builder::new_multi_thread com worker_threads explícito
- Correto: não usa #[tokio::main] desnecessário
### Panic hook estruturado (§9)
- `telemetry.rs:49-66`: set_hook captura payload e location
- Emite `tracing::error!` com campos estruturados
- Preserva hook anterior com chain call
### Documentação de exit codes (§19)
- `README.md:573-593`: tabela com 15 códigos de 0 a 77
- Inclui significado e causa possível
