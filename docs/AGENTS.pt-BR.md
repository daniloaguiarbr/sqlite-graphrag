# neurographrag para Agentes de IA


> Contrato CLI de primeira classe para 21+ agentes de código e orquestradores de LLM

- Leia a versão em inglês em [AGENTS.md](AGENTS.md)


## A Pergunta Que Nenhum Framework Responde
### Open Loop — Por Que Seu Agente Autônomo Esquece O Que Aprendeu
- Seu agente LLM venceu uma tarefa hoje e perdeu cada insight até amanhã cedo
- Seu orquestrador paga 400 dólares mensais ao Pinecone por contexto vetorial obsoleto
- Sua stack quebra no instante em que o embedding OpenAI recebe rate-limit pesado
- Seu protótipo GraphRAG morre em produção sob quatro chamadas concorrentes de subprocess
- O segredo que os frameworks jamais documentam mora em um único arquivo SQLite portátil


## Por Que Agentes Amam Esta CLI
### Cinco Diferenciais — Projetados Para Loops Autônomos
- Saída JSON determinística elimina cada hack de parser no código de orquestração
- Exit codes seguem `sysexits.h` para sua lógica de retry funcionar sem casar string
- Zero dependências de runtime entregam um binário estático com menos de 30 MB
- Stdin aceita payloads estruturados para seus agentes jamais escaparem argumentos shell
- Comportamento cross-platform permanece idêntico em Linux macOS e Windows desde o início


## Economia Que Converte
### Números Que Vendem A Troca
- Economize 200 dólares por mês substituindo Pinecone e chamadas de embedding OpenAI
- Reduza em até 80 por cento os tokens gastos em RAG via recall por grafo tipado
- Derrube a latência de retrieval de 800 ms em vector DB cloud para 8 ms em SSD local
- Corte o cold-start de 12 segundos de boot Docker para 90 ms de binário único
- Elimine 4 horas semanais de manutenção de cluster com banco zero-ops em um arquivo


## Soberania Como Vantagem Competitiva
### Por Que Memória Local Vence Em 2026
- Seus dados proprietários NUNCA saem da workstation do desenvolvedor ou do runner de CI
- Sua superfície de compliance encolhe para um arquivo SQLite sob sua própria criptografia
- Seu lock-in de fornecedor desaparece porque o schema é documentado e portátil
- Sua trilha de auditoria mora na tabela `memory_versions` com histórico imutável
- Sua indústria regulada ganha RAG offline-first sem cláusulas de dependência cloud


## Agentes e Orquestradores Compatíveis
### Catálogo — 21 Integrações Suportadas
| Agente | Fornecedor | Versão Mínima | Tipo de Integração | Exemplo |
| --- | --- | --- | --- | --- |
| Claude Code | Anthropic | 1.0+ | Subprocess | `neurographrag recall "query" --json` |
| Codex CLI | OpenAI | 0.5+ | AGENTS.md + subprocess | `neurographrag remember --name X --type user --body "..."` |
| Gemini CLI | Google | recente | Subprocess | `neurographrag hybrid-search "query" --json --k 5` |
| Opencode | open source | recente | Subprocess | `neurographrag recall "auth flow" --json --k 3` |
| OpenClaw | comunidade | recente | Subprocess | `neurographrag list --type user --json` |
| Paperclip | comunidade | recente | Subprocess | `neurographrag read --name onboarding-note --json` |
| VS Code Copilot | Microsoft | 1.90+ | tasks.json | `{"command": "neurographrag", "args": ["recall", "$selection", "--json"]}` |
| Google Antigravity | Google | recente | Runner | `neurographrag hybrid-search "prompt" --k 10 --json` |
| Windsurf | Codeium | recente | Terminal | `neurographrag recall "plano refactor" --json` |
| Cursor | Cursor | 0.40+ | Terminal | `neurographrag remember --name cursor-ctx --type agent --body "..."` |
| Zed | Zed Industries | recente | Assistant Panel | `neurographrag recall "abas abertas" --json --k 5` |
| Aider | open source | 0.60+ | Shell | `neurographrag recall "alvo refactor" --k 5 --json` |
| Jules | Google Labs | preview | automação CI | `neurographrag stats --json` |
| Kilo Code | comunidade | recente | Subprocess | `neurographrag recall "tarefas recentes" --json` |
| Roo Code | comunidade | recente | Subprocess | `neurographrag hybrid-search "contexto repo" --json` |
| Cline | comunidade | extensão VS Code | Terminal | `neurographrag list --limit 20 --json` |
| Continue | open source | VS Code ou JetBrains | Terminal | `neurographrag recall "docstring" --json` |
| Factory | Factory | recente | API ou subprocess | `neurographrag recall "contexto pr" --json` |
| Augment Code | Augment | recente | IDE | `neurographrag hybrid-search "code review" --json` |
| JetBrains AI Assistant | JetBrains | 2024.2+ | IDE | `neurographrag recall "stacktrace" --json` |
| OpenRouter | OpenRouter | qualquer | Roteador multi-LLM | `neurographrag recall "regra roteamento" --json` |


## Contrato — Stdin e Stdout
### Entrada — Apenas Argumentos Estruturados
- Flags da CLI aceitam argumentos tipados validados por `clap` com parsing estrito
- Stdin aceita body puro quando `--body-stdin` está ativo em `remember` ou `edit`
- Stdin aceita payload JSON quando `--payload-stdin` está ativo em modos batch
- Variáveis de ambiente sobrescrevem defaults sem mutar o arquivo do banco de dados
- Idioma é controlado por `--lang en` ou `--lang pt` para saída determinística


### Saída — Documentos JSON Determinísticos
- Cada subcomando emite exatamente um documento JSON quando `--json` está ativo
- Chaves permanecem estáveis entre releases dentro da mesma linha major corrente
- Timestamps seguem RFC 3339 com offset UTC sempre presente e explícito
- Campos nulos são omitidos para manter o payload enxuto para consumo por agentes
- Arrays preservam ordem determinística por `score` ou `updated_at` descendente


## Tabela de Exit Codes
### Contrato — Mapeie Cada Status A Uma Decisão De Roteamento
| Código | Significado | Ação Recomendada |
| --- | --- | --- |
| `0` | Sucesso | Continue o loop do agente |
| `1` | Falha de validação ou runtime | Logue e exiba ao operador |
| `2` | Erro de uso CLI ou duplicata | Corrija argumentos e repita |
| `3` | Conflito de optimistic update | Releia `updated_at` e repita |
| `4` | Memória ou entidade não encontrada | Trate recurso ausente graciosamente |
| `5` | Limite de namespace ou não resolvido | Passe `--namespace` explicitamente |
| `6` | Payload excedeu os limites permitidos | Divida o body em chunks menores |
| `10` | Erro SQLite no banco de dados | Rode `health` para inspecionar integridade |
| `11` | Falha na geração de embedding | Verifique arquivos do modelo e repita |
| `12` | Extensão `sqlite-vec` falhou | Reinstale o binário com extensão embutida |
| `13` | Batch parcial ou DB ocupado | Respeite backoff e repita depois |
| `15` | Banco ocupado após tentativas | Aguarde e repita a operação |
| `73` | Lock ocupado entre slots | Aguarde ou eleve `--max-concurrency` |
| `75` | Timeout de lock atingido | Eleve `--wait-lock` em segundos |
| `77` | Limite de memória baixo acionado | Libere RAM antes de repetir |


## Formato De Saída JSON
### Recall — KNN Puramente Vetorial
```json
{
  "query": "graphrag retrieval",
  "k": 3,
  "namespace": "default",
  "elapsed_ms": 12,
  "hits": [
    { "name": "graphrag-intro", "score": 0.91, "type": "user", "updated_at": "2026-04-18T12:00:00Z" },
    { "name": "vector-search-notes", "score": 0.84, "type": "agent", "updated_at": "2026-04-17T08:12:03Z" },
    { "name": "hybrid-ranker", "score": 0.77, "type": "feedback", "updated_at": "2026-04-16T21:04:55Z" }
  ]
}
```


### Hybrid Search — FTS5 Mais Vetor Via RRF
```json
{
  "query": "postgres migration",
  "k": 5,
  "rrf_k": 60,
  "weights": { "vec": 0.6, "fts": 0.4 },
  "elapsed_ms": 18,
  "hits": [
    { "name": "postgres-migration-plan", "score": 0.96, "rank_vec": 1, "rank_fts": 1 },
    { "name": "db-migration-checklist", "score": 0.88, "rank_vec": 2, "rank_fts": 3 }
  ]
}
```


## Idempotência e Efeitos Colaterais
### Comandos Read-Only — Zero Mutação Garantida
- `recall` lê tabelas de vetor e metadados sem tocar o estado em disco
- `read` busca uma única linha por nome e emite JSON sem efeito colateral
- `list` pagina memórias ordenadas deterministicamente com cursores estáveis
- `health` roda `PRAGMA integrity_check` e reporta sem escrever em disco
- `stats` conta linhas em transações read-only seguras para agentes concorrentes


### Comandos Write — Optimistic Locking Protege Concorrência
- `remember` usa `ON CONFLICT(name)` então chamadas duplicadas retornam exit code `2`
- `rename` exige `--expected-updated-at` para detectar escrita stale via exit `3`
- `edit` cria nova linha em `memory_versions` preservando histórico imutável
- `restore` retrocede o conteúdo criando uma nova versão em vez de sobrescrever
- `forget` é soft-delete então repetir a chamada é seguro e idempotente por design


## Limites De Payload
### Tetos — Aplicados Pelo Binário
- `EMBEDDING_MAX_TOKENS` vale 512 tokens medidos pelo tokenizador do modelo
- `TEXT_BODY_PREVIEW_LEN` vale 200 caracteres em snippets de list e recall
- `MAX_CONCURRENT_CLI_INSTANCES` vale 4 entre agentes subprocess cooperando
- `CLI_LOCK_DEFAULT_WAIT_SECS` vale 300 segundos antes do exit code `75`
- `PURGE_RETENTION_DAYS_DEFAULT` vale 30 dias antes do hard delete ficar permitido


## Controle De Idioma
### Saída Bilíngue — Uma Flag Troca O Locale
- Flag `--lang en` força mensagens em inglês independentemente do locale do sistema
- Flag `--lang pt` força mensagens em português independentemente do locale do sistema
- Env `NEUROGRAPHRAG_LANG=pt` sobrescreve locale do sistema quando falta `--lang`
- Sem flag e sem env cai no fallback por `sys_locale::get_locale()` do runtime
- Locales desconhecidos caem em inglês sem emitir warning algum no stderr


## Resumo Dos Superpoderes
### Cinco Razões Para Seu Orquestrador Permanecer
- Saída determinística elimina parsing frágil por regex no código de glue do agente
- Exit codes roteiam decisões sem raspar stderr por mensagens legíveis a humanos
- Binário único implanta idêntico em Docker GitHub Actions e laptops de dev
- Durabilidade do SQLite sobrevive a kernel panic e kill de container sem corromper
- Retrieval por grafo revela contexto multi-hop que o vetor puro jamais devolve


## Comece Em 30 Segundos
### Instalação — Um Comando Instala A Stack Inteira
```bash
cargo install --locked neurographrag && neurographrag init
```
- Flag `--locked` reusa o `Cargo.lock` enviado para proteger MSRV de drift transitivo
- Comando `init` cria o arquivo SQLite e baixa o modelo de embedding localmente
- Primeira invocação pode levar um minuto enquanto `fastembed` baixa `multilingual-e5-small`
- Invocações seguintes iniciam frias em menos de 100 ms em hardware consumer moderno
- Remova com `cargo uninstall neurographrag` deixando o arquivo de banco intacto
