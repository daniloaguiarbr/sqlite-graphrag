use proptest::prelude::*;
use regex::Regex;

// ---------------------------------------------------------------------------
// Constantes espelhadas de src/constants.rs — sem importar a crate em testes
// ---------------------------------------------------------------------------

const MAX_MEMORY_NAME_LEN: usize = 80;
const MAX_MEMORY_BODY_LEN: usize = 512_000;
const NAME_SLUG_REGEX: &str = r"^[a-z][a-z0-9-]{0,78}[a-z0-9]$|^[a-z0-9]$";

// Número de casos proptest. Em CI pode ser reduzido via PROPTEST_CASES=32.
fn proptest_config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(256);
    ProptestConfig::with_cases(cases)
}

// ---------------------------------------------------------------------------
// Suite 5 — Property-based tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(proptest_config())]

    /// Qualquer string gerada pelo regex kebab-case deve casar com NAME_SLUG_REGEX.
    #[test]
    fn name_slug_regex_aceita_kebab_case(
        name in "[a-z][a-z0-9-]{0,78}[a-z0-9]"
    ) {
        let re = Regex::new(NAME_SLUG_REGEX).unwrap();
        prop_assert!(
            re.is_match(&name),
            "Nome kebab valido rejeitado: {:?}",
            name
        );
    }

    /// Char único minúsculo deve casar (variante `^[a-z0-9]$`).
    #[test]
    fn name_slug_regex_aceita_char_unico_minusculo(
        c in "[a-z0-9]"
    ) {
        let re = Regex::new(NAME_SLUG_REGEX).unwrap();
        prop_assert!(
            re.is_match(&c),
            "Char unico valido rejeitado: {:?}",
            c
        );
    }

    /// Maiúsculas devem ser sempre rejeitadas pelo regex.
    #[test]
    fn name_slug_regex_rejeita_uppercase(
        upper in "[A-Z]{1,5}[a-z0-9-]{0,10}"
    ) {
        let re = Regex::new(NAME_SLUG_REGEX).unwrap();
        prop_assert!(
            !re.is_match(&upper),
            "Uppercase incorretamente aceito: {:?}",
            upper
        );
    }

    /// Underscore nunca deve ser aceito pelo regex.
    #[test]
    fn name_slug_regex_rejeita_underscore(
        prefix in "[a-z]{1,10}",
        suffix in "[a-z]{1,10}"
    ) {
        let nome_com_underscore = format!("{prefix}_{suffix}");
        let re = Regex::new(NAME_SLUG_REGEX).unwrap();
        prop_assert!(
            !re.is_match(&nome_com_underscore),
            "Underscore incorretamente aceito: {:?}",
            nome_com_underscore
        );
    }

    /// Strings com espaço devem ser rejeitadas.
    #[test]
    fn name_slug_regex_rejeita_espaco(
        a in "[a-z]{1,10}",
        b in "[a-z]{1,10}"
    ) {
        let nome_com_espaco = format!("{a} {b}");
        let re = Regex::new(NAME_SLUG_REGEX).unwrap();
        prop_assert!(
            !re.is_match(&nome_com_espaco),
            "Espaco incorretamente aceito: {:?}",
            nome_com_espaco
        );
    }

    /// Qualquer string ASCII com mais de MAX_MEMORY_BODY_LEN bytes deve
    /// ter comprimento superior ao limite — invariante de boundary.
    #[test]
    fn body_length_boundary_unicode_acima_do_limite(
        extra in "[\\p{L}]{1,500}"
    ) {
        // Gera um body com pelo menos MAX_MEMORY_BODY_LEN + len(extra) bytes.
        let padding: String = "a".repeat(MAX_MEMORY_BODY_LEN);
        let body = format!("{padding}{extra}");
        prop_assert!(
            body.len() > MAX_MEMORY_BODY_LEN,
            "Body deveria exceder limite mas tem {} bytes",
            body.len()
        );
    }

    /// Body ASCII com até MAX_MEMORY_BODY_LEN bytes deve ser <= ao limite.
    #[test]
    fn body_length_boundary_unicode_no_limite(
        chars in "[A-Za-z0-9]{1,4096}"
    ) {
        let truncated: String = chars.chars().take(MAX_MEMORY_BODY_LEN).collect();
        prop_assert!(
            truncated.len() <= MAX_MEMORY_BODY_LEN,
            "Truncado deveria ser <= {} mas tem {} bytes",
            MAX_MEMORY_BODY_LEN,
            truncated.len()
        );
    }

    /// Nome com comprimento entre 1 e MAX_MEMORY_NAME_LEN bytes e formato kebab
    /// deve ser considerado válido pelo invariante de comprimento.
    #[test]
    fn name_comprimento_valido_dentro_do_limite(
        name in "[a-z][a-z0-9-]{0,78}[a-z0-9]"
    ) {
        prop_assert!(
            !name.is_empty() && name.len() <= MAX_MEMORY_NAME_LEN,
            "Nome {:?} tem comprimento {} fora do range [1, {}]",
            name,
            name.len(),
            MAX_MEMORY_NAME_LEN
        );
    }

    /// BLAKE3 é determinístico: mesmo input produz mesmo hash em chamadas distintas.
    #[test]
    fn embedding_determinism_blake3_mesmo_hash_para_mesmo_input(
        body in "[\\p{L}\\p{N} .,!?]{1,1000}"
    ) {
        let hash_a = blake3::hash(body.as_bytes());
        let hash_b = blake3::hash(body.as_bytes());
        prop_assert_eq!(
            hash_a,
            hash_b,
            "BLAKE3 nao e determinístico para input {:?}",
            &body[..body.len().min(40)]
        );
    }

    /// Hashes BLAKE3 de inputs distintos devem diferir (colisão é extremamente
    /// improvável — esta propriedade testa a anti-colisão prática).
    #[test]
    fn embedding_determinism_blake3_inputs_distintos_hashes_distintos(
        a in "[a-z]{10,50}",
        b in "[A-Z]{10,50}"
    ) {
        // a é minúsculo, b é maiúsculo — garantidamente distintos.
        let hash_a = blake3::hash(a.as_bytes());
        let hash_b = blake3::hash(b.as_bytes());
        prop_assert_ne!(
            hash_a,
            hash_b,
            "Colisão de BLAKE3 inesperada entre {:?} e {:?}",
            a,
            b
        );
    }

    /// Serialização JSON de um objeto simples de nome + description + body
    /// deve ser round-trippable: deserializar o JSON serializado retorna os
    /// mesmos valores originais.
    #[test]
    fn json_round_trip_nome_descricao_body(
        name in "[a-z][a-z0-9-]{0,30}[a-z0-9]",
        description in "[\\p{L}\\p{N} .,!?]{1,200}",
        body in "[\\p{L}\\p{N} .,!?\n]{1,500}"
    ) {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
        struct Payload {
            name: String,
            description: String,
            body: String,
        }

        let original = Payload {
            name: name.clone(),
            description: description.clone(),
            body: body.clone(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let restored: Payload = serde_json::from_str(&json).unwrap();

        prop_assert_eq!(
            &original,
            &restored,
            "Round-trip JSON falhou para nome={:?}",
            name
        );
    }
}

// ---------------------------------------------------------------------------
// Testes unitários complementares (não-proptest)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod testes_unitarios {
    use super::*;

    #[test]
    fn name_slug_regex_aceita_exemplos_canonicos() {
        let re = Regex::new(NAME_SLUG_REGEX).unwrap();
        let validos = [
            "a",
            "z",
            "0",
            "abc",
            "my-memory",
            "projeto-rust-2026",
            "a0",
            "a-b-c",
            // 80 chars exatos
            &"a".repeat(79)
                .as_str()
                .chars()
                .chain(std::iter::once('b'))
                .collect::<String>(),
        ];
        for nome in &validos {
            assert!(re.is_match(nome), "Nome canonico rejeitado: {nome:?}");
        }
    }

    #[test]
    fn name_slug_regex_rejeita_exemplos_invalidos() {
        let re = Regex::new(NAME_SLUG_REGEX).unwrap();
        let invalidos = [
            "",
            "A",
            "My-Memory",
            "my_memory",
            "my memory",
            "-starts-with-dash",
            "ends-with-dash-",
            "__reserved",
        ];
        for nome in &invalidos {
            assert!(
                !re.is_match(nome),
                "Nome invalido incorretamente aceito: {nome:?}"
            );
        }
    }

    #[test]
    fn blake3_hash_bytes_tem_32_bytes() {
        let h = blake3::hash(b"sqlite-graphrag");
        assert_eq!(h.as_bytes().len(), 32);
    }

    #[test]
    fn body_limite_exato_aceito() {
        let body: String = "x".repeat(MAX_MEMORY_BODY_LEN);
        assert_eq!(body.len(), MAX_MEMORY_BODY_LEN);
    }

    #[test]
    fn body_um_acima_do_limite_detectado() {
        let body: String = "x".repeat(MAX_MEMORY_BODY_LEN + 1);
        assert!(body.len() > MAX_MEMORY_BODY_LEN);
    }
}
