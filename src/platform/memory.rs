//! Devolve ao sistema as páginas que a captura tocou.
//!
//! Um fluxo de captura passa dezenas de MB pelo working set (RGBA de cada
//! monitor, cópia para upload, canvas composto, buffer RGB da codificação) e o
//! Windows não encolhe o working set de um processo que não sofre pressão de
//! memória — o app volta para a bandeja com o número do Gerenciador de Tarefas
//! inflado por horas.
//!
//! `SetProcessWorkingSetSize(-1, -1)` remove o máximo possível de páginas: elas
//! vão para a lista de standby (nada é descartado, o conteúdo continua válido)
//! e retornam por page fault quando a próxima captura precisar delas. O custo é
//! alguns ms na captura seguinte; a troca vale para um app que fica idle.

/// Enxuga o working set do processo. Chamar ao voltar para `Idle`, nunca em
/// caminho de frame.
#[cfg(windows)]
pub fn trim_working_set() {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};

    // SAFETY: pseudo-handle do próprio processo; `usize::MAX` em mínimo e
    // máximo é o valor documentado para "esvazie o quanto puder".
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

#[cfg(not(windows))]
pub fn trim_working_set() {}
