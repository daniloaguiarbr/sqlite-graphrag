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

use sysinfo::{
    get_current_pid, MemoryRefreshKind, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System,
    UpdateKind,
};

use crate::errors::AppError;

/// Retorna a memória disponível atual em MiB.
pub fn available_memory_mb() -> u64 {
    let sys =
        System::new_with_specifics(RefreshKind::new().with_memory(MemoryRefreshKind::everything()));
    let available_bytes = sys.available_memory();
    available_bytes / (1024 * 1024)
}

/// Retorna o RSS atual do processo em MiB quando disponível.
pub fn current_process_memory_mb() -> Option<u64> {
    let pid = get_current_pid().ok()?;
    let mut sys =
        System::new_with_specifics(RefreshKind::new().with_memory(MemoryRefreshKind::everything()));
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::new()
            .with_memory()
            .with_exe(UpdateKind::OnlyIfNotSet),
    );
    sys.process(pid).map(|p| p.memory() / (1024 * 1024))
}

/// Calcula o teto seguro de concorrência para cargas pesadas de embedding.
///
/// Fórmula canônica:
/// `permits = min(cpus, available_memory_mb / ram_por_task_mb) * 0.5`
///
/// O resultado é clampado entre `1` e `max_concurrency`.
pub fn calculate_safe_concurrency(
    available_mb: u64,
    cpu_count: usize,
    ram_per_task_mb: u64,
    max_concurrency: usize,
) -> usize {
    let cpu_count = cpu_count.max(1);
    let max_concurrency = max_concurrency.max(1);
    let ram_per_task_mb = ram_per_task_mb.max(1);

    let memory_bound = (available_mb / ram_per_task_mb) as usize;
    let resource_bound = cpu_count.min(memory_bound).max(1);
    let safe_with_margin = (resource_bound / 2).max(1);

    safe_with_margin.min(max_concurrency)
}

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
    let available_mb = available_memory_mb();

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

    #[test]
    fn calculate_safe_concurrency_respeita_metade_da_margem() {
        let permits = calculate_safe_concurrency(8_000, 8, 1_000, 4);
        assert_eq!(permits, 4);
    }

    #[test]
    fn calculate_safe_concurrency_nunca_retorna_zero() {
        let permits = calculate_safe_concurrency(100, 1, 10_000, 4);
        assert_eq!(permits, 1);
    }

    #[test]
    fn calculate_safe_concurrency_respeita_teto_maximo() {
        let permits = calculate_safe_concurrency(128_000, 64, 500, 4);
        assert_eq!(permits, 4);
    }

    #[test]
    fn current_process_memory_mb_retorna_algum_valor() {
        let rss = current_process_memory_mb();
        assert!(rss.is_some());
    }
}
