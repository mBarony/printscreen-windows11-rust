//! Toasts do Windows via `notify-rust`, com fallback silencioso em log.
//!
//! As notificações nunca podem travar a UI nem derrubar a aplicação: `show()`
//! roda em uma thread descartável e qualquer erro vira uma linha de log.

use notify_rust::Notification;

use crate::config::APP_NAME;

/// Exibe um toast "summary / body" sem bloquear o chamador.
pub fn toast(summary: &str, body: &str) {
    let summary = summary.to_owned();
    let body = body.to_owned();
    std::thread::spawn(move || {
        let result = Notification::new()
            .appname(APP_NAME)
            .summary(&summary)
            .body(&body)
            .finalize()
            .show();
        match result {
            Ok(_) => log::debug!("toast: {summary} — {body}"),
            Err(err) => log::warn!("toast indisponível ({err}): {summary} — {body}"),
        }
    });
}

/// Toast de erro (mesmo canal; mantido separado para leitura do código).
pub fn toast_error(summary: &str, body: &str) {
    log::error!("{summary}: {body}");
    toast(summary, body);
}

/// Variante síncrona, para caminhos que encerram o processo em seguida
/// (ex.: segunda instância, RF-08) — o `show()` precisa concluir antes do
/// `main` retornar, senão o toast nunca aparece.
pub fn toast_blocking(summary: &str, body: &str) {
    let result = Notification::new()
        .appname(APP_NAME)
        .summary(summary)
        .body(body)
        .finalize()
        .show();
    if let Err(err) = result {
        log::warn!("toast indisponível ({err}): {summary} — {body}");
    }
}
