//! Concurrency tests via loom for the CLI slot semaphore.
//!
//! Models the central invariant of `src/lock.rs`: at most `MAX_SLOTS` threads
//! podem deter um slot simultaneamente. Os testes usam `AtomicUsize` como
//! counter of active slots — loom-visible equivalent of the `flock` semaphore.
//!
//! O loom limita o total de threads (incluindo main) a `loom::MAX_THREADS = 5`.
//! Therefore each model uses at most 4 spawned threads.
//!
//! Execute com:
//! ```text
//! RUSTFLAGS="--cfg sqlite_graphrag_loom" cargo nextest run --profile heavy -E 'test(/^loom_/)'
//! ```
//! Ou via script: `bash scripts/test-loom.sh`
//!
//! DO NOT run with `cargo test` without `--test-threads=1` — loom in parallel
//! may saturate the CPU and cause thermal livelock (incident 2026-04-19).

#![cfg(sqlite_graphrag_loom)]

use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::sync::Arc;
use serial_test::serial;

/// Non-blocking counting semaphore that mirrors the logic of `try_any_slot`.
///
/// `try_acquire` uses optimistic CAS: reads the current counter and, if below
/// `max`, attempts to increment it atomically. Returns `true` on success and
/// `false` if all slots are occupied — identical to the behavior
/// de `try_lock_exclusive` retornando `WouldBlock`.
struct SlotSemaforo {
    contador: Arc<AtomicUsize>,
    max: usize,
}

impl SlotSemaforo {
    fn novo(max: usize) -> Self {
        Self {
            contador: Arc::new(AtomicUsize::new(0)),
            max,
        }
    }

    fn clonar(&self) -> Self {
        Self {
            contador: Arc::clone(&self.contador),
            max: self.max,
        }
    }

    /// Tenta adquirir um slot sem bloquear. Retorna `true` se adquiriu.
    fn try_acquire(&self) -> bool {
        let mut atual = self.contador.load(Ordering::Acquire);
        loop {
            if atual >= self.max {
                return false;
            }
            match self.contador.compare_exchange_weak(
                atual,
                atual + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(novo) => atual = novo,
            }
        }
    }

    /// Libera um slot previamente adquirido.
    fn release(&self) {
        let anterior = self.contador.fetch_sub(1, Ordering::AcqRel);
        assert!(
            anterior > 0,
            "release sem acquire correspondente — double-free detectado"
        );
    }

    /// Reads the current number of occupied slots.
    fn ocupados(&self) -> usize {
        self.contador.load(Ordering::Acquire)
    }
}

/// Test 1 — Maximum capacity invariant: 4 threads compete for 3 slots.
///
/// With more threads than slots, at least 1 thread always fails acquisition.
/// Verifica que o contador de slots ocupados NUNCA ultrapassa `max_slots`,
/// independentemente do escalonamento concorrente explorado pelo loom.
///
/// loom::MAX_THREADS = 5 (main + 4 spawned), therefore maximum of 4 spawns.
#[serial(loom_model)]
#[test]
fn quatro_threads_invariante_maximo_tres_slots() {
    const NUM_THREADS: usize = 4;
    const MAX_SLOTS: usize = 3;

    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(1);
    builder.max_branches = 100;
    builder.check(|| {
        let sem = Arc::new(SlotSemaforo::novo(MAX_SLOTS));
        let contador_holds = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();

        for _ in 0..NUM_THREADS {
            let sem_t = Arc::new(sem.clonar());
            let holds_t = Arc::clone(&contador_holds);

            let h = loom::thread::spawn(move || {
                if sem_t.try_acquire() {
                    // Registra que esta thread detém um slot.
                    let agora = holds_t.fetch_add(1, Ordering::AcqRel) + 1;
                    // Invariante central: nunca mais que MAX_SLOTS holds simultâneos.
                    assert!(
                        agora <= MAX_SLOTS,
                        "violação: {agora} holds simultâneos ultrapassam o limite {MAX_SLOTS}"
                    );
                    loom::thread::yield_now();
                    holds_t.fetch_sub(1, Ordering::AcqRel);
                    sem_t.release();
                }
                // Thread que não obteve slot simplesmente retorna — sem pânico.
            });
            handles.push(h);
        }

        for h in handles {
            h.join().expect("thread terminou com pânico");
        }

        // Ao final, todos os slots devem ter sido liberados.
        assert_eq!(
            sem.ocupados(),
            0,
            "slots ainda ocupados após todas as threads terminarem"
        );
    });
}

/// Teste 2 — Release libera slot e permite outra thread adquirir.
///
/// Thread A acquires the single available slot. Thread B tries to acquire and fails.
/// After A releases, B successfully acquires on the next attempt.
/// Modela o comportamento de polling de `acquire_cli_slot` com `wait_seconds > 0`.
#[serial(loom_model)]
#[test]
fn release_libera_slot_para_proxima_thread() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(1);
    builder.max_branches = 100;
    builder.check(|| {
        // Semáforo com 1 slot para forçar contenção determinística.
        let sem = Arc::new(SlotSemaforo::novo(1));
        assert!(
            sem.try_acquire(),
            "main deve ocupar o único slot antes de iniciar as threads"
        );

        let sem_a = Arc::new(sem.clonar());
        let sem_b = Arc::new(sem.clonar());

        // Sinalização de que A liberou o slot.
        let liberado = Arc::new(AtomicUsize::new(0));
        let liberado_b = Arc::clone(&liberado);
        let b_adquiriu = Arc::new(AtomicUsize::new(0));
        let b_adquiriu_t = Arc::clone(&b_adquiriu);

        let ha = loom::thread::spawn(move || {
            loom::thread::yield_now();
            sem_a.release();
            // Sinaliza que o slot foi liberado.
            liberado.store(1, Ordering::Release);
        });

        let hb = loom::thread::spawn(move || {
            // B tenta em loop até o slot estar livre — modela polling de wait_seconds.
            loop {
                if sem_b.try_acquire() {
                    b_adquiriu_t.store(1, Ordering::Release);
                    sem_b.release();
                    break;
                }
                // Sem o slot: verifica se A já liberou antes de tentar de novo.
                if liberado_b.load(Ordering::Acquire) == 1 {
                    // Tenta uma última vez após a liberação confirmada.
                    if sem_b.try_acquire() {
                        b_adquiriu_t.store(1, Ordering::Release);
                        sem_b.release();
                    }
                    break;
                }
                loom::thread::yield_now();
            }
        });

        ha.join().expect("thread A terminou com pânico");
        hb.join().expect("thread B terminou com pânico");

        assert_eq!(
            sem.ocupados(),
            0,
            "slot deve estar livre após ambas as threads terminarem"
        );
        assert_eq!(
            b_adquiriu.load(Ordering::Acquire),
            1,
            "thread B deve adquirir o slot após a liberação"
        );
    });
}

/// Test 3 — Clean shutdown: all slots released after termination.
///
/// 4 threads acquire and release slots in parallel. After all finish,
/// the counter must be zero — shutdown invariant of the `flock` semaphore.
///
/// loom::MAX_THREADS = 5 (main + 4 spawned). Aqui usamos exatamente 4.
#[serial(loom_model)]
#[test]
fn shutdown_limpo_todos_slots_liberados() {
    const NUM_THREADS: usize = 4;
    const MAX_SLOTS: usize = 4;

    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(1);
    builder.max_branches = 100;
    builder.check(|| {
        let sem = Arc::new(SlotSemaforo::novo(MAX_SLOTS));
        let adquiridos_total = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();

        for _ in 0..NUM_THREADS {
            let sem_t = Arc::new(sem.clonar());
            let total_t = Arc::clone(&adquiridos_total);

            let h = loom::thread::spawn(move || {
                if sem_t.try_acquire() {
                    total_t.fetch_add(1, Ordering::AcqRel);
                    loom::thread::yield_now();
                    sem_t.release();
                }
            });
            handles.push(h);
        }

        for h in handles {
            h.join()
                .expect("thread terminou com pânico durante shutdown");
        }

        // Invariante de shutdown: contador retorna a zero.
        assert_eq!(
            sem.ocupados(),
            0,
            "shutdown sujo: {n} slots ainda ocupados",
            n = sem.ocupados()
        );

        // Ao menos uma thread adquiriu slot — sistema não estava travado.
        let total = adquiridos_total.load(Ordering::Acquire);
        assert!(
            total > 0,
            "nenhuma thread adquiriu slot — possível deadlock no modelo"
        );
    });
}
