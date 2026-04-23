# Machine-Readable JSON Schemas


## English
### Purpose
- Each file in this directory is a JSON Schema Draft 2020-12 document
- Output schemas describe the exact stdout contract of every `sqlite-graphrag` subcommand
- Input schemas describe the accepted JSON payloads for file-driven graph ingestion
- Agents and parsers MUST validate responses against these schemas before processing
- All schemas use `"additionalProperties": false` — unexpected keys are contract violations
### Schema Files
| Subcommand | Schema file |
|---|---|
| `init` | `init.schema.json` |
| `remember` | `remember.schema.json` |
| `recall` | `recall.schema.json` |
| `read` | `read.schema.json` |
| `list` | `list.schema.json` |
| `forget` | `forget.schema.json` |
| `purge` | `purge.schema.json` |
| `rename` | `rename.schema.json` |
| `edit` | `edit.schema.json` |
| `history` | `history.schema.json` |
| `restore` | `restore.schema.json` |
| `hybrid-search` | `hybrid-search.schema.json` |
| `health` | `health.schema.json` |
| `migrate` | `migrate.schema.json` |
| `namespace-detect` | `namespace-detect.schema.json` |
| `optimize` | `optimize.schema.json` |
| `stats` | `stats.schema.json` |
| `sync-safe-copy` | `sync-safe-copy.schema.json` |
| `vacuum` | `vacuum.schema.json` |
| `link` | `link.schema.json` |
| `unlink` | `unlink.schema.json` |
| `related` | `related.schema.json` |
| `graph` | `graph.schema.json` |
| `graph traverse` | `graph-traverse.schema.json` |
| `graph stats` | `graph-stats.schema.json` |
| `graph entities` | `graph-entities.schema.json` |
| `cleanup-orphans` | `cleanup-orphans.schema.json` |
| `__debug_schema` | `debug-schema.schema.json` |
### Input Payload Schemas
- `entities-input.schema.json` validates the JSON array accepted by `remember --entities-file`
- `relationships-input.schema.json` validates the JSON array accepted by `remember --relationships-file`
### Usage
- Validate a `recall` response: `sqlite-graphrag recall "query" | jaq --from-file docs/schemas/recall.schema.json`
- Validate with Python: `jsonschema --instance <(sqlite-graphrag stats) docs/schemas/stats.schema.json`
- The `__debug_schema` subcommand is hidden and intended for diagnostic tooling only — the binary exposes it with a double-underscore prefix (`__debug_schema`) while the schema file uses the kebab-case name `debug-schema.schema.json` following the directory convention
### Flag Behavior
- Schemas describe the OUTPUT JSON contract, not the CLI input shape
- Several subcommands accept multiple flag aliases that produce the same output
- `sync-safe-copy` accepts `--dest` (primary), `--to`, and `--output` — all write to the same `dest_path` field in the response
- `graph stats` accepts both `--json` (no-op legacy) and `--format json` — neither changes the response shape
- `__debug_schema` is exposed by the binary with a double-underscore prefix; the schema file uses kebab-case `debug-schema.schema.json` following the directory convention
- The `--json` flag on any subcommand is a no-op kept for backward compatibility — JSON is always emitted on stdout
### Stability Guarantee
- Schemas track the `main` branch and are updated with every breaking change
- Minor additions (new optional fields) do NOT bump the schema version
- Removals or renames of required fields constitute a breaking change and increment the CLI major version


## Português Brasileiro
### Objetivo
- Cada arquivo neste diretório é um documento JSON Schema Draft 2020-12
- Os schemas de saída descrevem o contrato exato de stdout de cada subcomando `sqlite-graphrag`
- Os schemas de entrada descrevem os payloads JSON aceitos pela ingestão de grafo orientada a arquivo
- Agentes e parsers DEVEM validar respostas contra estes schemas antes de processar
- Todos os schemas usam `"additionalProperties": false` — chaves inesperadas são violações de contrato
### Arquivos de Schema
- Veja a tabela na seção English acima — os nomes de arquivo são idênticos entre idiomas
### Schemas de Payload de Entrada
- `entities-input.schema.json` valida o array JSON aceito por `remember --entities-file`
- `relationships-input.schema.json` valida o array JSON aceito por `remember --relationships-file`
### Comportamento de Flags
- Os schemas descrevem o contrato de OUTPUT JSON, não o formato de entrada CLI
- Vários subcomandos aceitam múltiplos aliases de flag que produzem a mesma saída
- `sync-safe-copy` aceita `--dest` (primária), `--to` e `--output` — todos gravam no mesmo campo `dest_path` da resposta
- `graph stats` aceita `--json` (no-op legado) e `--format json` — nenhum altera o formato da resposta
- `__debug_schema` é exposto pelo binário com prefixo duplo sublinhado; o arquivo de schema usa kebab-case `debug-schema.schema.json` seguindo a convenção do diretório
- A flag `--json` em qualquer subcomando é no-op mantida por compatibilidade — JSON é sempre emitido no stdout
### Uso
- Validar resposta do `recall`: `sqlite-graphrag recall "consulta" | jaq --from-file docs/schemas/recall.schema.json`
- Validar com Python: `jsonschema --instance <(sqlite-graphrag stats) docs/schemas/stats.schema.json`
- O subcomando `__debug_schema` é oculto e destinado apenas a ferramentas de diagnóstico — o binário o expõe com prefixo duplo sublinhado (`__debug_schema`) enquanto o arquivo de schema usa o nome kebab-case `debug-schema.schema.json` seguindo a convenção do diretório
### Garantia de Estabilidade
- Os schemas acompanham a branch `main` e são atualizados a cada breaking change
- Adições menores (novos campos opcionais) NÃO incrementam a versão do schema
- Remoções ou renomeações de campos obrigatórios constituem breaking change e incrementam a versão major da CLI
