//! O que o processo de GUI diz ao residente.
//!
//! O filho não tem ícone na bandeja (criar um segundo apareceria duplicado para
//! o usuário) nem é dono dos atalhos globais, então delega as duas coisas: os
//! balões de notificação e o aviso de que o `config.json` foi regravado. O
//! `HWND` do residente chega na linha de comando (`--parent`).
//!
//! Quando não há residente registrado — o residente é ele mesmo, ou uma GUI
//! aberta à mão em depuração — as funções devolvem `false` e o chamador cai no
//! caminho local (balão próprio ou log).

use std::sync::OnceLock;

use crate::platform::shell::{self, IPC_BALLOON, IPC_CONFIG_CHANGED, IPC_EDITOR_OPEN};

static RESIDENT_HWND: OnceLock<isize> = OnceLock::new();

/// Registra o `HWND` do residente. Chamar uma única vez, no boot do modo GUI.
pub fn set_resident(hwnd_value: isize) {
    let _ = RESIDENT_HWND.set(hwnd_value);
}

fn resident() -> Option<isize> {
    RESIDENT_HWND.get().copied().filter(|&hwnd| hwnd != 0)
}

/// Pede ao residente que exiba um balão. `false` = sem residente.
pub fn balloon(title: &str, text: &str) -> bool {
    let Some(hwnd) = resident() else { return false };
    shell::send_to_resident(hwnd, IPC_BALLOON, &format!("{title}\n{text}"))
}

/// Avisa que o `config.json` mudou: o residente relê e re-registra os atalhos.
pub fn config_changed() {
    let Some(hwnd) = resident() else {
        log::warn!("config gravado sem residente para avisar; atalhos não recarregados");
        return;
    };
    if !shell::send_to_resident(hwnd, IPC_CONFIG_CHANGED, "") {
        log::warn!("residente não respondeu ao aviso de config alterado");
    }
}

/// Avisa o residente de que o editor abriu.
///
/// Enquanto o processo de GUI está só mostrando o overlay, acionar o atalho
/// de novo o encerra — é o "aperta e fecha". Depois que o editor abre há
/// anotações lá dentro, e fechá-lo por um atalho seria destruir trabalho.
pub fn editor_opened() {
    if let Some(hwnd) = resident() {
        shell::send_to_resident(hwnd, IPC_EDITOR_OPEN, "");
    }
}
