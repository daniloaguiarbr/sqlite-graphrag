//! Guarda de memória: verifica disponibilidade de RAM antes de carregar o modelo ONNX.
//!
//! O carregamento do modelo via `fastembed` consome aproximadamente
//! [`crate::constants::EMBEDDING_LOAD_EXPECTED_RSS_MB`] MiB de memória residente.
//! Se o sistema não tiver memória suficiente disponível, múltiplas invocações
//! paralelas podem esgotar a RAM e causar OOM (Out-Of-Memory), travando o sistema.
//!
//! Esta guard interroga o SO via `sysinfo` antes de qualquer inicialização pesada,
//! abortando com [`crate::errors::AppError::LowMemory`] (exit 77) quando o piso
//! configurado não é atingido.

use sysinfo::{MemoryRefreshKind, RefreshKind, System};

use crate::errors::AppError;

/// Verifica se há memória disponível suficiente para iniciar o carregamento do modelo.
///
/// # Parâmetros
/// - `min_mb`: piso mínimo em MiB de memória disponível (tipicamente
///   [`crate::constants::MIN_AVAILABLE_MEMORY_MB`]).
///
/// # Erros
/// Retorna [`AppError::LowMemory`] quando `available_mb < min_mb`.
///
/// # Retorno
/// Retorna `Ok(available_mb)` com o valor real de memória disponível em MiB.
pub fn check_available_memory(min_mb: u64) -> Result<u64, AppError> {
    let sys =
        System::new_with_specifics(RefreshKind::new().with_memory(MemoryRefreshKind::everything()));
    let available_bytes = sys.available_memory();
    let available_mb = available_bytes / (1024 * 1024);
    drop(sys);

    if available_mb < min_mb {
        return Err(AppError::LowMemory {
            available_mb,
            required_mb: min_mb,
        });
    }

    Ok(available_mb)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn check_available_memory_com_zero_sempre_passa() {
        let resultado = check_available_memory(0);
        assert!(
            resultado.is_ok(),
            "min_mb=0 deve sempre passar, got: {resultado:?}"
        );
        let mb = resultado.unwrap();
        assert!(mb > 0, "sistema deve reportar memória positiva");
    }

    #[test]
    fn check_available_memory_com_valor_gigante_falha() {
        let resultado = check_available_memory(u64::MAX);
        assert!(
            matches!(resultado, Err(AppError::LowMemory { .. })),
            "u64::MAX MiB deve falhar com LowMemory, got: {resultado:?}"
        );
    }

    #[test]
    fn low_memory_error_contem_valores_corretos() {
        match check_available_memory(u64::MAX) {
            Err(AppError::LowMemory {
                available_mb,
                required_mb,
            }) => {
                assert_eq!(required_mb, u64::MAX);
                assert!(available_mb < u64::MAX);
            }
            outro => panic!("esperado LowMemory, got: {outro:?}"),
        }
    }
}
