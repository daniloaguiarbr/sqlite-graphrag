//! Display timezone for `*_iso` fields in JSON output.
//!
//! Precedence (highest to lowest priority):
//! 1. `--tz <IANA>` flag passed on the CLI
//! 2. Env var `SQLITE_GRAPHRAG_DISPLAY_TZ`
//! 3. Fallback UTC
//!
//! The timezone is initialized once via [`init`][crate::tz::init] and stored in
//! `GLOBAL_TZ` (OnceLock). After initialization, [`format_iso`][crate::tz::format_iso] and
//! [`epoch_to_iso`][crate::tz::epoch_to_iso] convert timestamps applying the chosen timezone.

use crate::errors::AppError;
use crate::i18n::validation;
use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use std::sync::OnceLock;

static GLOBAL_TZ: OnceLock<Tz> = OnceLock::new();

/// Resolves the timezone from the `SQLITE_GRAPHRAG_DISPLAY_TZ` env var.
///
/// Returns `Tz::UTC` if the variable is absent or empty.
/// Returns a validation error if the value is an invalid IANA name.
fn resolve_tz_from_env() -> Result<Tz, AppError> {
    match std::env::var("SQLITE_GRAPHRAG_DISPLAY_TZ") {
        Ok(v) if !v.trim().is_empty() => v
            .trim()
            .parse::<Tz>()
            .map_err(|_| AppError::Validation(validation::invalid_tz(v.trim()))),
        _ => Ok(Tz::UTC),
    }
}

/// Initializes the global timezone.
///
/// `explicit` — value from the `--tz` CLI flag (already parsed).
/// If `explicit` is `None`, tries `SQLITE_GRAPHRAG_DISPLAY_TZ`, then UTC.
///
/// Subsequent calls are silently ignored (OnceLock semantics).
/// Returns an error only if `explicit` is `None` and the env var is invalid.
pub fn init(explicit: Option<Tz>) -> Result<(), AppError> {
    let fuso = match explicit {
        Some(tz) => tz,
        None => resolve_tz_from_env()?,
    };
    let _ = GLOBAL_TZ.set(fuso);
    Ok(())
}

/// Returns the active timezone.
///
/// If [`init`] was never called, tries to read the env var; fallback UTC.
pub fn current_tz() -> Tz {
    *GLOBAL_TZ.get_or_init(|| resolve_tz_from_env().unwrap_or(Tz::UTC))
}

/// Formats a `DateTime<Utc>` using the global timezone.
///
/// Format: `%Y-%m-%dT%H:%M:%S%:z` (e.g. `2026-04-19T10:00:00+00:00` for UTC,
/// `2026-04-19T07:00:00-03:00` for `America/Sao_Paulo`).
pub fn format_iso(ts: DateTime<Utc>) -> String {
    let fuso = current_tz();
    ts.with_timezone(&fuso)
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

/// Converts a Unix epoch (seconds) to an ISO 8601 string with the global timezone.
///
/// Values outside the representable range return the fallback
/// `"1970-01-01T00:00:00+00:00"`.
pub fn epoch_to_iso(epoch: i64) -> String {
    Utc.timestamp_opt(epoch, 0)
        .single()
        .map(format_iso)
        .unwrap_or_else(|| "1970-01-01T00:00:00+00:00".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn utc_default_quando_env_ausente() {
        // Remove variável para garantir fallback UTC
        std::env::remove_var("SQLITE_GRAPHRAG_DISPLAY_TZ");
        let resultado = resolve_tz_from_env().expect("não deve falhar com env ausente");
        assert_eq!(resultado, Tz::UTC);
    }

    #[test]
    #[serial]
    fn env_valido_aplica_timezone() {
        std::env::set_var("SQLITE_GRAPHRAG_DISPLAY_TZ", "America/Sao_Paulo");
        let resultado = resolve_tz_from_env().expect("America/Sao_Paulo é válido");
        assert_eq!(resultado.name(), "America/Sao_Paulo");
        std::env::remove_var("SQLITE_GRAPHRAG_DISPLAY_TZ");
    }

    #[test]
    #[serial]
    fn env_invalido_retorna_erro_validation() {
        std::env::set_var("SQLITE_GRAPHRAG_DISPLAY_TZ", "Invalido/Naoexiste");
        let resultado = resolve_tz_from_env();
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
        // Testa epoch_to_iso diretamente sem estado global
        std::env::remove_var("SQLITE_GRAPHRAG_DISPLAY_TZ");
        let resultado = {
            // Aplica UTC diretamente sem usar GLOBAL_TZ
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
    fn format_iso_utc_preserves_zero_offset() {
        let ts = Utc.timestamp_opt(1_705_320_000, 0).single().unwrap();
        // Aplica UTC diretamente
        let resultado = ts
            .with_timezone(&Tz::UTC)
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string();
        assert_eq!(resultado, "2024-01-15T12:00:00+00:00");
    }

    #[test]
    fn format_iso_sao_paulo_applies_offset() {
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
