//! Aviso do reconhecimento de texto: o começo do que foi copiado, e a opção
//! de tirar as quebras de linha.
//!
//! Não é um passo do fluxo. Quando esta janela aparece o texto **já está** na
//! área de transferência, com as quebras como o motor as devolveu — que é o
//! padrão. Ela é uma segunda chance: quem for colar num campo de uma linha só
//! aperta o botão e o mesmo texto é recopiado emendado, sem repetir a captura.
//!
//! Some sozinha ao colar, e depois de alguns segundos se ninguém colar. O
//! relógio para enquanto o cursor estiver sobre ela: ninguém deve perder a
//! janela no meio de uma decisão.

use crate::clipboard;

/// Quantos caracteres do começo aparecem na prévia.
const PREVIEW_CHARS: usize = 60;
/// Quantas linhas da prévia aparecem. Mais que isto não caberia na janela:
/// o texto cresceria para baixo e por cima do botão.
const PREVIEW_LINES: usize = 2;
/// Segundos na tela sem ninguém encostar.
const LIFETIME_SECS: f64 = 8.0;
/// Tamanho da janela, em pontos.
pub const SIZE: (f32, f32) = (440.0, 76.0);
/// Tamanho mínimo do botão, em pontos — alvo de clique confortável, que o
/// `small_button` de antes não dava.
const BUTTON_SIZE: (f32, f32) = (108.0, 28.0);
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
    ///
    /// O corte é em dois eixos, e os dois importam: por linhas, senão um
    /// reconhecimento de parágrafo inteiro cresceria para baixo e passaria
    /// por cima do botão; por caracteres, para a linha não empurrá-lo para
    /// fora da janela.
    fn preview(&self) -> String {
        let text = self.current();
        let mut preview = text
            .lines()
            .take(PREVIEW_LINES)
            .collect::<Vec<_>>()
            .join("\n");
        let mut cortado = text.lines().count() > PREVIEW_LINES;

        if preview.chars().count() > PREVIEW_CHARS {
            preview = preview.chars().take(PREVIEW_CHARS).collect();
            cortado = true;
        }
        if cortado {
            preview.push('…');
        }
        preview
    }
}

/// `true` quando o usuário acabou de apertar Ctrl+V — em qualquer aplicativo.
///
/// O aviso não tem foco no momento em que isso acontece: quem recebe a tecla é
/// a janela onde o texto está sendo colado. Por isso a consulta ao estado
/// global do teclado, em vez de um evento do egui, que nunca chegaria aqui.
///
/// Não é um hook de teclado: nada é interceptado nem registrado, a colagem
/// segue para o destino como sempre, e a consulta é a duas teclas específicas.
#[cfg(windows)]
fn paste_pressed() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_V};

    // SAFETY: `GetAsyncKeyState` só lê estado de teclado e aceita qualquer
    // código; não há ponteiro nem recurso envolvido.
    unsafe {
        // Ctrl tem de estar segurado agora (0x8000). No V vale o bit de "foi
        // pressionada desde a última consulta" (0x0001): um toque curto cabe
        // inteiro entre dois pulsos de 100 ms e seria perdido se olhássemos
        // só o estado instantâneo.
        GetAsyncKeyState(VK_CONTROL as i32) as u16 & 0x8000 != 0
            && GetAsyncKeyState(VK_V as i32) as u16 & 0x0001 != 0
    }
}

#[cfg(not(windows))]
fn paste_pressed() -> bool {
    false
}

pub fn show(ctx: &egui::Context, popup: &mut OcrPopup) {
    if ctx.input(|i| i.viewport().close_requested()) {
        popup.closed = true;
        return;
    }

    let now = ctx.input(|i| i.time);
    if popup.deadline.is_infinite() {
        popup.deadline = now + LIFETIME_SECS;
        // Descarta um Ctrl+V anterior a esta janela: o bit de "foi
        // pressionada" acumula desde a última consulta, e sem esta leitura o
        // aviso poderia nascer e sumir no mesmo quadro.
        let _ = paste_pressed();
    }

    // Colar é o fim natural da tarefa: o texto já estava na área de
    // transferência antes desta janela existir, e insistir no aviso depois
    // disso é ruído sobre o que o usuário foi fazer.
    if paste_pressed() {
        popup.closed = true;
        return;
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
        // O botão entra primeiro, encostado à direita, e só depois a prévia
        // recebe o que sobrou. É o que impede a sobreposição: com a prévia
        // primeiro, um texto largo tomava a linha inteira e o botão era
        // desenhado por cima dele.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            toggled = ui
                .add(egui::Button::new(label).min_size(BUTTON_SIZE.into()))
                .on_hover_text(hint)
                .clicked();
            ui.add_space(8.0);
            // `truncate` corta no fim do espaço disponível em vez de
            // transbordar — a prévia nunca decide a largura do botão.
            ui.add(
                egui::Label::new(
                    egui::RichText::new(preview)
                        .size(12.0)
                        .color(ui.visuals().weak_text_color()),
                )
                .truncate(),
            );
        });

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
    fn a_previa_corta_no_limite_de_linhas() {
        // Um parágrafo inteiro cresceria para baixo e passaria por cima do
        // botão; a prévia para na segunda linha e avisa que cortou.
        let popup = OcrPopup::new("uma\nduas\ntres\nquatro".to_owned(), (0.0, 0.0));
        assert_eq!(popup.preview(), "uma\nduas…");
    }

    #[test]
    fn a_previa_no_limite_exato_de_linhas_nao_marca_corte() {
        let popup = OcrPopup::new("uma\nduas".to_owned(), (0.0, 0.0));
        assert_eq!(popup.preview(), "uma\nduas");
    }

    #[test]
    fn emendar_cabe_numa_linha_e_o_corte_por_linhas_deixa_de_valer() {
        // Emendado, o mesmo texto que estourava o limite de linhas passa a
        // caber — é o efeito que o botão anuncia.
        let mut popup = OcrPopup::new("uma\nduas\ntres\nquatro".to_owned(), (0.0, 0.0));
        popup.joined = true;
        assert_eq!(popup.preview(), "uma duas tres quatro");
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
