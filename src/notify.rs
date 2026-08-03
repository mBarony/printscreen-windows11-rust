//! Notificações do app: balões do ícone da bandeja (via `platform::shell`),
//! com fallback silencioso em log.
//!
//! Antes dos balões, os toasts WinRT (`notify-rust`) apareciam com origem
//! "Windows PowerShell" (exe sem AUMID registrado); o balão exibe o nome e
//! o ícone do RustShot. Chamável de qualquer thread (a entrega é feita por
//! PostMessage à janela da bandeja).

use crate::platform;

/// Exibe uma notificação "summary / body" sem bloquear o chamador.
pub fn toast(summary: &str, body: &str) {
    if platform::shell::show_balloon(summary, body) {
        log::debug!("toast: {summary} — {body}");
    } else {
        log::warn!("toast indisponível (sem bandeja): {summary} — {body}");
    }
}

/// Toast de erro (mesmo canal; mantido separado para leitura do código).
pub fn toast_error(summary: &str, body: &str) {
    log::error!("{summary}: {body}");
    toast(summary, body);
}

/// Variante síncrona para caminhos que encerram o processo em seguida
/// (ex.: segunda instância, RF-08) — sem bandeja própria, usa uma caixa de
/// mensagem nativa.
pub fn toast_blocking(summary: &str, body: &str) {
    platform::msgbox::info(summary, body);
}
