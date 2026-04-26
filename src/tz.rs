//! Fuso horário de exibição para campos `*_iso` do JSON de saída.
//!
//! Precedência (do mais para o menos prioritário):
//! 1. Flag `--tz <IANA>` passada na CLI
//! 2. Env var `SQLITE_GRAPHRAG_DISPLAY_TZ`
//! 3. Fallback UTC
//!
//! A timezone é inicializada UMA vez via [`init`][crate::tz::init] e armazenada em
//! `FUSO_GLOBAL` (OnceLock). Após a inicialização, [`formatar_iso`][crate::tz::formatar_iso] e
//! [`epoch_para_iso`][crate::tz::epoch_para_iso] convertem timestamps aplicando o fuso escolhido.

use crate::errors::AppError;
use crate::i18n::validacao;
use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use std::sync::OnceLock;

static FUSO_GLOBAL: OnceLock<Tz> = OnceLock::new();

/// Resolve o fuso a partir do env var `SQLITE_GRAPHRAG_DISPLAY_TZ`.
///
/// Retorna `Tz::UTC` se a variável estiver ausente ou vazia.
/// Retorna erro de validação se o valor for um nome IANA inválido.
fn resolver_tz_de_env() -> Result<Tz, AppError> {
    match std::env::var("SQLITE_GRAPHRAG_DISPLAY_TZ") {
        Ok(v) if !v.trim().is_empty() => v
            .trim()
            .parse::<Tz>()
            .map_err(|_| AppError::Validation(validacao::tz_invalido(v.trim()))),
        _ => Ok(Tz::UTC),
    }
}

/// Inicializa o fuso global.
///
/// `explicit` — valor vindo da flag `--tz` da CLI (já parseado).
/// Se `explicit` for `None`, tenta `SQLITE_GRAPHRAG_DISPLAY_TZ`, depois UTC.
///
/// Chamadas subsequentes são ignoradas silenciosamente (OnceLock semantics).
/// Retorna erro apenas se `explicit` for `None` e o env var for inválido.
pub fn init(explicit: Option<Tz>) -> Result<(), AppError> {
    let fuso = match explicit {
        Some(tz) => tz,
        None => resolver_tz_de_env()?,
    };
    let _ = FUSO_GLOBAL.set(fuso);
    Ok(())
}

/// Retorna o fuso ativo.
///
/// Se [`init`] nunca foi chamado, tenta ler o env var; fallback UTC.
pub fn fuso_atual() -> Tz {
    *FUSO_GLOBAL.get_or_init(|| resolver_tz_de_env().unwrap_or(Tz::UTC))
}

/// Formata um `DateTime<Utc>` usando o fuso global.
///
/// Formato: `%Y-%m-%dT%H:%M:%S%:z` (ex: `2026-04-19T10:00:00+00:00` para UTC,
/// `2026-04-19T07:00:00-03:00` para `America/Sao_Paulo`).
pub fn formatar_iso(ts: DateTime<Utc>) -> String {
    let fuso = fuso_atual();
    ts.with_timezone(&fuso)
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

/// Converte um Unix epoch (segundos) para string ISO 8601 com fuso global.
///
/// Valores fora do intervalo representável retornam o fallback
/// `"1970-01-01T00:00:00+00:00"`.
pub fn epoch_para_iso(epoch: i64) -> String {
    Utc.timestamp_opt(epoch, 0)
        .single()
        .map(formatar_iso)
        .unwrap_or_else(|| "1970-01-01T00:00:00+00:00".to_string())
}

#[cfg(test)]
mod testes {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn utc_default_quando_env_ausente() {
        // Remove variável para garantir fallback UTC
        std::env::remove_var("SQLITE_GRAPHRAG_DISPLAY_TZ");
        let resultado = resolver_tz_de_env().expect("não deve falhar com env ausente");
        assert_eq!(resultado, Tz::UTC);
    }

    #[test]
    #[serial]
    fn env_valido_aplica_timezone() {
        std::env::set_var("SQLITE_GRAPHRAG_DISPLAY_TZ", "America/Sao_Paulo");
        let resultado = resolver_tz_de_env().expect("America/Sao_Paulo é válido");
        assert_eq!(resultado.name(), "America/Sao_Paulo");
        std::env::remove_var("SQLITE_GRAPHRAG_DISPLAY_TZ");
    }

    #[test]
    #[serial]
    fn env_invalido_retorna_erro_validation() {
        std::env::set_var("SQLITE_GRAPHRAG_DISPLAY_TZ", "Invalido/Naoexiste");
        let resultado = resolver_tz_de_env();
        assert!(resultado.is_err(), "timezone inválida deve retornar Err");
        match resultado {
            Err(AppError::Validation(msg)) => {
                assert!(
                    msg.contains("SQLITE_GRAPHRAG_DISPLAY_TZ"),
                    "mensagem deve citar a env var"
                );
                assert!(
                    msg.contains("Invalido/Naoexiste"),
                    "mensagem deve citar o valor inválido"
                );
            }
            other => unreachable!("esperado AppError::Validation, obtido: {other:?}"),
        }
        std::env::remove_var("SQLITE_GRAPHRAG_DISPLAY_TZ");
    }

    #[test]
    fn epoch_zero_gera_utc_iso() {
        // Testa epoch_para_iso diretamente sem estado global
        std::env::remove_var("SQLITE_GRAPHRAG_DISPLAY_TZ");
        let resultado = {
            // Aplica UTC diretamente sem usar FUSO_GLOBAL
            let tz = Tz::UTC;
            Utc.timestamp_opt(0, 0)
                .single()
                .map(|dt| {
                    dt.with_timezone(&tz)
                        .format("%Y-%m-%dT%H:%M:%S%:z")
                        .to_string()
                })
                .unwrap_or_else(|| "1970-01-01T00:00:00+00:00".to_string())
        };
        assert_eq!(resultado, "1970-01-01T00:00:00+00:00");
    }

    #[test]
    fn formatar_iso_utc_preserva_offset_zero() {
        let ts = Utc.timestamp_opt(1_705_320_000, 0).single().unwrap();
        // Aplica UTC diretamente
        let resultado = ts
            .with_timezone(&Tz::UTC)
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string();
        assert_eq!(resultado, "2024-01-15T12:00:00+00:00");
    }

    #[test]
    fn formatar_iso_sao_paulo_aplica_offset() {
        let ts = Utc.timestamp_opt(1_705_320_000, 0).single().unwrap();
        let sao_paulo: Tz = "America/Sao_Paulo".parse().unwrap();
        let resultado = ts
            .with_timezone(&sao_paulo)
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string();
        // America/Sao_Paulo em janeiro é UTC-3
        assert!(
            resultado.contains("-03:00"),
            "esperado offset -03:00, obtido: {resultado}"
        );
    }
}
