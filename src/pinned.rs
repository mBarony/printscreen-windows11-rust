//! Captura fixada na tela: uma janelinha sem bordas, sempre no topo, que
//! fica até ser fechada.
//!
//! Serve para consultar algo enquanto se trabalha noutra janela — um valor,
//! um trecho de código, uma referência. É o oposto do aviso do OCR: aquele
//! some sozinho, este só sai quando o usuário manda.
//!
//! Não tem barra de título, então o corpo inteiro é a área de arrasto. Fechar
//! é `Esc`, e a roda redimensiona.

use std::sync::Arc;

use crate::imgbuf::RgbaImage;

/// Lado máximo que a janela assume ao nascer, em pontos. Uma captura de tela
/// cheia fixada em tamanho natural cobriria o monitor e seria inútil.
const MAX_INICIAL: f32 = 520.0;
/// Limites do redimensionamento pela roda, como fração do tamanho natural.
const ESCALA_MIN: f32 = 0.15;
const ESCALA_MAX: f32 = 4.0;
/// Quanto cada passo da roda muda a escala.
const PASSO_RODA: f32 = 0.1;

pub const WINDOW_TITLE: &str = "RustShot — fixado";

pub struct PinnedShot {
    pub image: Arc<RgbaImage>,
    /// Fator sobre o tamanho natural da imagem, em pontos.
    pub scale: f32,
    /// Onde a janela nasce, em pontos — o canto da região capturada.
    pub anchor: (f32, f32),
    texture: Option<egui::TextureHandle>,
    pub closed: bool,
}

impl PinnedShot {
    pub fn new(image: Arc<RgbaImage>, anchor: (f32, f32)) -> Self {
        Self {
            scale: escala_inicial(image.width(), image.height()),
            image,
            anchor,
            texture: None,
            closed: false,
        }
    }

    /// Tamanho da janela em pontos, para o `ViewportBuilder`.
    pub fn size(&self) -> (f32, f32) {
        (
            (self.image.width() as f32 * self.scale).max(1.0),
            (self.image.height() as f32 * self.scale).max(1.0),
        )
    }
}

/// Encolhe o suficiente para a janela nascer utilizável, nunca amplia.
///
/// Uma captura pequena aparece em tamanho natural; uma de tela cheia entra
/// reduzida, e a roda ajusta a partir daí.
fn escala_inicial(largura: u32, altura: u32) -> f32 {
    let maior = largura.max(altura) as f32;
    if maior <= MAX_INICIAL {
        1.0
    } else {
        MAX_INICIAL / maior
    }
}

/// Nova escala depois de girar a roda `delta` pontos.
fn escala_apos_roda(atual: f32, delta: f32) -> f32 {
    // Multiplicativo, não aditivo: um passo perto do mínimo tem de mexer
    // menos que um passo perto do máximo, senão a janela salta de tamanho
    // quando está pequena.
    let fator = 1.0 + PASSO_RODA * delta.signum();
    (atual * fator).clamp(ESCALA_MIN, ESCALA_MAX)
}

pub fn show(ctx: &egui::Context, pinned: &mut PinnedShot) {
    if ctx.input(|i| i.viewport().close_requested()) {
        pinned.closed = true;
        return;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        pinned.closed = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        return;
    }

    let textura = pinned.texture.get_or_insert_with(|| {
        let img = egui::ColorImage::from_rgba_unmultiplied(
            [pinned.image.width() as usize, pinned.image.height() as usize],
            pinned.image.as_raw(),
        );
        ctx.load_texture("pinned", img, egui::TextureOptions::LINEAR)
    });
    let id = textura.id();

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ctx, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            ui.painter().image(
                id,
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            // Contorno fino: sem moldura, a captura se confunde com o que
            // está atrás dela quando as cores são parecidas.
            ui.painter().rect_stroke(
                rect,
                egui::CornerRadius::ZERO,
                egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(60)),
                egui::StrokeKind::Inside,
            );

            // Sem barra de título, o corpo é a área de arrasto.
            if response.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        });

    let roda = ctx.input(|i| i.raw_scroll_delta.y);
    if roda != 0.0 {
        pinned.scale = escala_apos_roda(pinned.scale, roda);
        let (w, h) = pinned.size();
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::Vec2::new(w, h)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captura_pequena_nasce_em_tamanho_natural() {
        assert_eq!(escala_inicial(400, 300), 1.0);
    }

    #[test]
    fn captura_grande_nasce_encolhida_e_cabendo_no_limite() {
        let escala = escala_inicial(3840, 2160);
        assert!(escala < 1.0);
        assert!((3840.0 * escala - MAX_INICIAL).abs() < 0.01);
    }

    #[test]
    fn a_roda_amplia_e_reduz() {
        let maior = escala_apos_roda(1.0, 5.0);
        let menor = escala_apos_roda(1.0, -5.0);
        assert!(maior > 1.0, "girar para cima tem de ampliar");
        assert!(menor < 1.0, "girar para baixo tem de reduzir");
    }

    #[test]
    fn a_roda_respeita_os_limites() {
        assert_eq!(escala_apos_roda(ESCALA_MAX, 1.0), ESCALA_MAX);
        assert_eq!(escala_apos_roda(ESCALA_MIN, -1.0), ESCALA_MIN);
    }

    #[test]
    fn o_passo_da_roda_e_proporcional() {
        // Perto do mínimo o passo é pequeno em valor absoluto; perto do
        // máximo é grande. Um passo aditivo faria a janela saltar quando
        // pequena.
        let perto_do_min = escala_apos_roda(0.2, 1.0) - 0.2;
        let perto_do_max = escala_apos_roda(2.0, 1.0) - 2.0;
        assert!(perto_do_max > perto_do_min);
    }

    #[test]
    fn o_tamanho_acompanha_a_escala() {
        let img = Arc::new(RgbaImage::from_raw(200, 100, vec![0; 200 * 100 * 4]));
        let mut pin = PinnedShot::new(img, (0.0, 0.0));
        assert_eq!(pin.size(), (200.0, 100.0));
        pin.scale = 0.5;
        assert_eq!(pin.size(), (100.0, 50.0));
    }
}
