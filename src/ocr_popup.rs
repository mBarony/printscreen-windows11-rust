//! Aviso do reconhecimento de texto: o começo do que foi copiado, e a opção
//! de tirar as quebras de linha.
//!
//! Não é um passo do fluxo. Quando esta janela aparece o texto **já está** na
//! área de transferência, com as quebras como o motor as devolveu — que é o
//! padrão. Ela é uma segunda chance: quem for colar num campo de uma linha só
//! aperta o botão e o mesmo texto é recopiado emendado, sem repetir a captura.
//!
//! Some sozinha depois de alguns segundos. O relógio para enquanto o cursor
//! estiver sobre ela: ninguém deve perder a janela no meio de uma decisão.

use crate::clipboard;

/// Quantos caracteres do começo aparecem na prévia.
const PREVIEW_CHARS: usize = 60;
/// Segundos na tela sem ninguém encostar.
const LIFETIME_SECS: f64 = 8.0;
/// Tamanho da janela, em pontos.
pub const SIZE: (f32, f32) = (400.0, 68.0);
/// Distância do topo do monitor, em pontos.
pub const TOP_MARGIN: f32 = 24.0;

pub const WINDOW_TITLE: &str = "RustShot — texto reconhecido";

/// Junta as linhas num parágrafo só.
///
/// Linhas vazias somem em vez de virarem espaços duplos, e cada linha é
/// aparada antes de entrar: o motor costuma devolver sobras nas pontas, e
/// emendar sem limpar produziria "palavra  outra".
pub fn joined(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub struct OcrPopup {
    /// Como o motor devolveu, com as quebras — a versão copiada de saída.
    original: String,
    /// `true` = o que está na área de transferência é a versão emendada.
    joined: bool,
    /// Instante do relógio do egui em que some sozinha.
    deadline: f64,
    /// Alto e centro do monitor onde a seleção aconteceu, em pontos.
    pub anchor: (f32, f32),
    pub closed: bool,
}

impl OcrPopup {
    // Sem a feature `ocr` nada cria este aviso; o resto do módulo continua
    // compilando porque o campo em `AppShared` e o viewport são incondicionais.
    #[cfg_attr(not(feature = "ocr"), allow(dead_code))]
    pub fn new(original: String, anchor: (f32, f32)) -> Self {
        Self {
            original,
            joined: false,
            // Só vale a partir do primeiro quadro, que é quando há relógio.
            deadline: f64::INFINITY,
            anchor,
            closed: false,
        }
    }

    /// Texto na forma atualmente escolhida.
    fn current(&self) -> String {
        if self.joined {
            joined(&self.original)
        } else {
            self.original.clone()
        }
    }

    /// Prévia truncada, preservando as quebras quando elas existem — é vendo
    /// a prévia mudar que se percebe o efeito do botão.
    fn preview(&self) -> String {
        let text = self.current();
        let mut preview: String = text.chars().take(PREVIEW_CHARS).collect();
        if text.chars().count() > PREVIEW_CHARS {
            preview.push('…');
        }
        preview
    }
}

pub fn show(ctx: &egui::Context, popup: &mut OcrPopup) {
    if ctx.input(|i| i.viewport().close_requested()) {
        popup.closed = true;
        return;
    }

    let now = ctx.input(|i| i.time);
    if popup.deadline.is_infinite() {
        popup.deadline = now + LIFETIME_SECS;
    }

    // As duas metades do `Sides` não podem tocar `popup` ao mesmo tempo: o
    // que elas precisam sai daqui pronto, e o clique só é relatado.
    let preview = popup.preview();
    let (label, hint) = if popup.joined {
        ("com quebras", "Volta a copiar o texto com as quebras de linha")
    } else {
        ("sem quebras", "Recopia o mesmo texto emendado numa linha só")
    };
    let mut toggled = false;

    egui::CentralPanel::default().show(ctx, |ui| {
        // Prévia à esquerda, botão encostado na direita. `Sides` precisa da
        // altura explícita: o padrão dele é `interact_size.y`, menor que o
        // conteúdo, e o botão sairia cortado.
        egui::Sides::new()
            .height(SIZE.1 - 16.0)
            .shrink_left()
            .show(
                ui,
                |ui| {
                    ui.label(
                        egui::RichText::new(preview)
                            .size(12.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                },
                |ui| {
                    toggled = ui.small_button(label).on_hover_text(hint).clicked();
                },
            );

        // O relógio para enquanto o cursor estiver sobre a janela.
        if ctx.input(|i| i.pointer.has_pointer()) {
            popup.deadline = now + LIFETIME_SECS;
        }
    });

    if toggled {
        popup.joined = !popup.joined;
        // O texto trocado substitui o que já estava na área de transferência:
        // é o mesmo reconhecimento, noutra forma, e não uma segunda cópia.
        if let Err(err) = clipboard::copy_text(&popup.current()) {
            log::warn!("falha ao recopiar o texto: {err:#}");
        }
        // Quem acabou de decidir merece tempo para conferir o resultado.
        popup.deadline = now + LIFETIME_SECS;
    }

    if now >= popup.deadline {
        popup.closed = true;
        return;
    }
    // A janela vive de tempo, não de eventos: sem isto ela ficaria parada na
    // tela até alguém mexer o mouse.
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emendar_junta_linhas_com_um_espaco() {
        assert_eq!(joined("uma\nduas\ntres"), "uma duas tres");
    }

    #[test]
    fn emendar_descarta_linhas_vazias_e_sobras_nas_pontas() {
        // O motor costuma devolver espaços nas pontas e linhas em branco
        // entre parágrafos; emendar sem limpar daria espaços duplos.
        assert_eq!(joined("  uma  \n\n   duas\n \n tres "), "uma duas tres");
    }

    #[test]
    fn emendar_texto_de_uma_linha_nao_muda_nada() {
        assert_eq!(joined("linha unica"), "linha unica");
    }

    #[test]
    fn a_previa_trunca_e_marca_que_truncou() {
        let longo = "a".repeat(PREVIEW_CHARS + 10);
        let popup = OcrPopup::new(longo, (0.0, 0.0));
        let preview = popup.preview();
        assert_eq!(preview.chars().count(), PREVIEW_CHARS + 1);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn a_previa_curta_sai_inteira_e_sem_reticencias() {
        let popup = OcrPopup::new("curto".to_owned(), (0.0, 0.0));
        assert_eq!(popup.preview(), "curto");
    }

    #[test]
    fn a_previa_acompanha_o_botao() {
        let mut popup = OcrPopup::new("uma\nduas".to_owned(), (0.0, 0.0));
        assert_eq!(popup.preview(), "uma\nduas");
        popup.joined = true;
        assert_eq!(popup.preview(), "uma duas");
    }
}
