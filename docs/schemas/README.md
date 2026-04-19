# Machine-Readable JSON Schemas


## English
### Purpose
- Each file in this directory is a JSON Schema Draft 2020-12 document
- Schemas describe the exact stdout contract of every `neurographrag` subcommand
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
| `cleanup-orphans` | `cleanup-orphans.schema.json` |
| `__debug_schema` | `debug-schema.schema.json` |
### Usage
- Validate a `recall` response: `neurographrag recall "query" | jaq --from-file docs/schemas/recall.schema.json`
- Validate with Python: `jsonschema --instance <(neurographrag stats) docs/schemas/stats.schema.json`
- The `__debug_schema` subcommand is hidden and intended for diagnostic tooling only — the binary exposes it with a double-underscore prefix (`__debug_schema`) while the schema file uses the kebab-case name `debug-schema.schema.json` following the directory convention
### Stability Guarantee
- Schemas track the `main` branch and are updated with every breaking change
- Minor additions (new optional fields) do NOT bump the schema version
- Removals or renames of required fields constitute a breaking change and increment the CLI major version


## Português Brasileiro
### Objetivo
- Cada arquivo neste diretório é um documento JSON Schema Draft 2020-12
- Os schemas descrevem o contrato exato de stdout de cada subcomando `neurographrag`
- Agentes e parsers DEVEM validar respostas contra estes schemas antes de processar
- Todos os schemas usam `"additionalProperties": false` — chaves inesperadas são violações de contrato
### Arquivos de Schema
- Veja a tabela na seção English acima — os nomes de arquivo são idênticos entre idiomas
### Uso
- Validar resposta do `recall`: `neurographrag recall "consulta" | jaq --from-file docs/schemas/recall.schema.json`
- Validar com Python: `jsonschema --instance <(neurographrag stats) docs/schemas/stats.schema.json`
- O subcomando `__debug_schema` é oculto e destinado apenas a ferramentas de diagnóstico — o binário o expõe com prefixo duplo sublinhado (`__debug_schema`) enquanto o arquivo de schema usa o nome kebab-case `debug-schema.schema.json` seguindo a convenção do diretório
### Garantia de Estabilidade
- Os schemas acompanham a branch `main` e são atualizados a cada breaking change
- Adições menores (novos campos opcionais) NÃO incrementam a versão do schema
- Remoções ou renomeações de campos obrigatórios constituem breaking change e incrementam a versão major da CLI
