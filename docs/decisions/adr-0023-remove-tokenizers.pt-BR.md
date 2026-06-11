# ADR-0023: Remoção do Crate `tokenizers` (v1.0.76)

- Status: Aceito (2026-06-07)
- Atualização (v1.0.79): a válvula de escape `embedding-legacy` mencionada abaixo foi removida antecipando o cronograma da v1.1.0; a janela de transição está fechada
- Decisores: Danilo Aguiar
- Escopo: src/tokenizer.rs, src/chunking.rs, src/commands/ingest.rs, src/commands/remember.rs, src/commands/enrich.rs, src/commands/ingest_claude.rs, Cargo.toml

## Contexto

`tokenizers` 0.22 (Hugging Face) era usado para três coisas na v1.0.74:

1. Contar tokens em um corpo de memória para decidir se chunking era necessário.
2. Produzir pares de byte-offset `(start, end)` para cada token no corpo, usados pelo chunker para alinhar fronteiras de chunk com fronteiras de token.
3. Carregar a config do tokenizer multilingual-e5 de `tokenizer_config.json` para descobrir `model_max_length`.

Na v1.0.76, o pipeline fastembed se foi, então o tokenizer `multilingual-e5-small` não é mais usado para embedar nada. O crate `tokenizers` ainda precisava estar presente para o chunker funcionar corretamente com a API v1.0.74 do `text-splitter` (que recebe um `Tokenizer` para seu `with_sizer`).

## Decisão

O crate `tokenizers` é REMOVIDO do build padrão. O chunker e o tokenizer são simplificados:

- `token_count_approx` agora é uma heurística char/word: `(words * 3) / 2` arredondado para cima. Isso é conservador para a família SentencePiece do multilingual-e5 e bate com a calibração que o resto do crate usa (`CHARS_PER_TOKEN = 2`).
- `passage_token_offsets` agora retorna fronteiras de palavras delimitadas por whitespace em vez de offsets reais de sub-palavra. A extração do lado LLM não precisa de granularidade de sub-palavra; o prompt vai para o LLM, que lida com tokenização do lado dele.
- `get_model_max_length` agora retorna a constante `crate::constants::EMBEDDING_MAX_TOKENS` (512). O operador pode sobrescrever via a env var `SQLITE_GRAPHRAG_EMBEDDING_MAX_TOKENS`.

O crate `text-splitter` é mantido mas a chamada `with_sizer` é substituída por uma heurística de contagem de caracteres (o sizer padrão `ChunkConfig::new` em `text-splitter` 0.30.1).

## Consequências

### Positivas

- ~50 MB de código compilado removido do binário (tokenizers + onig + os arquivos de vocabulário BPE embarcados).
- A contagem de tokens agora é determinística e reproduzível sem precisar carregar um arquivo de vocabulário de 1 MB do disco.
- O chunker e o tokenizer podem ser testados em unitários sem dependência de rede ou filesystem.

### Negativas

- Contagens de tokens são aproximadas. Para corpos muito longos, a aproximação pode subcontar ou supercontar em 10-20%. Isso é aceitável para a decisão de chunking (um teto de 512 tokens é verificado antes de cada invocação LLM; o próprio LLM impõe o cap rígido).
- O sizer de contagem de caracteres do `text-splitter` não respeita fronteiras semânticas de Markdown tão limpamente quanto o sizer anterior baseado em `tokenizer`. Operadores que precisam de chunking exato ciente de Markdown devem habilitar a feature `embedding-legacy` e usar o caminho v1.0.74.

## Verificação

- `cargo test --lib tokenizers` (os novos testes do tokenizer baseado em whitespace): 6 testes unitários cobrem string vazia, palavra única, múltiplas palavras, whitespace no início/fim, e os casos de borda de `passage_offsets`.
- `cargo test --lib chunking`: todos os testes verdes.
- `cargo test --lib`: 711 testes verdes.
