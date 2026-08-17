//! Trabalhos de fundo que não podem morrer com o processo: renderizar
//! anotações, codificar o JPG, gravar o arquivo, copiar para a área de
//! transferência.
//!
//! No desenho de dois processos o de GUI é efêmero — ele encerra assim que a
//! última janela fecha, e um `thread::spawn` solto seria abortado no meio da
//! gravação. Todo trabalho passa por aqui e o `main` espera por eles antes de
//! deixar o processo terminar.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread::JoinHandle;

static HANDLES: Mutex<Vec<JoinHandle<()>>> = Mutex::new(Vec::new());
static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// Decrementa o contador mesmo se o trabalho entrar em pânico.
struct RunningGuard;

impl Drop for RunningGuard {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::Release);
    }
}

/// Dispara `work` em thread de trabalho, registrando-a para o `join_all`.
pub fn spawn(work: impl FnOnce() + Send + 'static) {
    RUNNING.fetch_add(1, Ordering::Release);
    let handle = std::thread::spawn(move || {
        let _guard = RunningGuard;
        work();
    });
    match HANDLES.lock() {
        Ok(mut handles) => handles.push(handle),
        // Mutex envenenado: sem registro, o trabalho ainda roda — só perde a
        // garantia de ser esperado. Melhor que abortar a captura.
        Err(_) => log::warn!("registro de trabalhos indisponível"),
    }
}

/// Quantos trabalhos ainda estão rodando. O residente usa para só enxugar o
/// working set depois que a codificação terminou de ler a imagem.
pub fn pending() -> usize {
    RUNNING.load(Ordering::Acquire)
}

/// Espera todos os trabalhos pendentes. Chamar uma única vez, ao encerrar.
pub fn join_all() {
    let handles = match HANDLES.lock() {
        Ok(mut handles) => std::mem::take(&mut *handles),
        Err(_) => return,
    };
    for handle in handles {
        if handle.join().is_err() {
            log::error!("trabalho de fundo entrou em pânico");
        }
    }
}
