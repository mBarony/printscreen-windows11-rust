//! Documento editável do editor: uma imagem de partida mais um log de
//! operações, do qual o estado visível é derivado por replay.
//!
//! O histórico não guarda estados, guarda **o que foi feito**. Desfazer é
//! recuar o índice e reconstruir; refazer é avançar. A vantagem prática
//! aparece no recorte, que muda a imagem e desloca as anotações na mesma
//! edição: com snapshots seria preciso guardar uma cópia da imagem por passo
//! do histórico, e com o log basta guardar o retângulo.
//!
//! O log é limitado a [`MAX_OPS`]. Ao estourar, a operação mais antiga não é
//! simplesmente jogada fora — ela é aplicada à imagem de partida, virando
//! permanente. Descartá-la sem mais deixaria as anotações no espaço errado,
//! porque um recorte antigo deixaria de ser aplicado no replay.

use std::sync::Arc;

use crate::imgbuf::RgbaImage;

use super::backdrop::{self, BackdropStyle};
use super::cut::{self, Band};
use super::redact;
use super::spotlight::{self, Spotlight};
use super::shapes::{Handle, Layer, Point, RedactionStyle, Shape, Style};

/// Teto do histórico, em operações.
const MAX_OPS: usize = 100;

/// Uma edição registrada no log.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Cria uma anotação.
    Annotate(Layer),
    /// Substitui anotações existentes (mover, redimensionar, trocar estilo),
    /// casadas por `id`.
    Patch(Vec<Layer>),
    /// Remove anotações por `id`.
    Delete(Vec<u64>),
    /// Recorta a imagem e desloca as anotações junto.
    Crop { x: u32, y: u32, w: u32, h: u32 },
    /// Remove uma faixa e junta o que sobrou, arrastando as anotações.
    Cut(Band),
    /// Troca a moldura decorativa.
    Backdrop(BackdropStyle),
}

/// Uma redação aplicada, na forma que o replay compara para decidir se os
/// pixels visíveis mudaram.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RedactionMark {
    min: Point,
    max: Point,
    style: RedactionStyle,
    seed: u32,
}

fn redaction_marks(layers: &[Layer]) -> Vec<RedactionMark> {
    layers
        .iter()
        .filter_map(|layer| match &layer.shape {
            Shape::Redaction { min, max, seed } => Some(RedactionMark {
                min: *min,
                max: *max,
                style: layer.style.redaction,
                seed: *seed,
            }),
            _ => None,
        })
        .collect()
}

fn spotlights(layers: &[Layer]) -> Vec<Spotlight> {
    layers
        .iter()
        .filter_map(|layer| match &layer.shape {
            Shape::Spotlight { center, rx, ry } => Some(Spotlight {
                center: *center,
                rx: *rx,
                ry: *ry,
                form: layer.style.spotlight,
                magnification: layer.style.magnification,
                border: layer.style.stroke_width,
                border_color: layer.style.color,
            }),
            _ => None,
        })
        .collect()
}

/// Ponto de partida do replay: imagem e anotações já consolidadas.
struct Baseline {
    image: Arc<RgbaImage>,
    layers: Vec<Layer>,
    /// Recortes e cortes já assados na imagem acima. Continuam contando na
    /// assinatura: consolidar um deles não muda o que se vê, então não pode
    /// parecer uma mudança de enquadramento.
    crops: Vec<Reframe>,
}

/// Uma operação que muda o tamanho da imagem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reframe {
    Crop(u32, u32, u32, u32),
    Cut(Band),
}

pub struct Document {
    baseline: Baseline,
    ops: Vec<Op>,
    /// Quantas operações do log estão aplicadas — tudo à frente é o "refazer".
    index: usize,
    next_id: u64,

    // Estado derivado, reconstruído por `replay`.
    image: Arc<RgbaImage>,
    /// A mesma imagem com as redações e os holofotes já queimados. É dela
    /// que a exportação parte: uma região redigida nunca chega à tela nem ao
    /// arquivo com o conteúdo original.
    redacted: Arc<RgbaImage>,
    /// A anterior dentro da moldura decorativa, quando há uma. É o que a
    /// textura do editor carrega, para o preview coincidir com o JPG.
    framed: Arc<RgbaImage>,
    backdrop: BackdropStyle,
    layers: Vec<Layer>,
    /// Avança quando os pixels visíveis mudam (recorte ou redação) — é o que
    /// diz ao editor para refazer a textura.
    pixels_version: u64,
    /// Redações e holofotes aplicados no último replay.
    redactions: Vec<RedactionMark>,
    spotlights: Vec<Spotlight>,

    /// Recortes aplicados no último replay, e um selo que só avança quando
    /// eles mudam. O replay reconstrói a imagem toda vez, então o `Arc` é
    /// sempre novo — comparar ponteiros faria o editor achar que a imagem
    /// mudou a cada anotação criada e jogar fora o zoom do usuário.
    crops: Vec<Reframe>,
    image_version: u64,

    /// Estado anterior a um arrasto em andamento. Fica fora do log: só vira
    /// operação se o arrasto de fato mudar alguma coisa (`end_move`); um
    /// clique parado (`abort_move`) não toca no histórico.
    pending: Option<Vec<Layer>>,
}

/// Aplica uma operação sobre um par (imagem, anotações).
///
/// É a mesma função usada pelo replay e pela consolidação do log antigo —
/// por isso "assar" uma operação na base tem exatamente o efeito de tê-la
/// executado.
fn apply(op: &Op, image: &mut Arc<RgbaImage>, layers: &mut Vec<Layer>) {
    match op {
        Op::Annotate(layer) => layers.push(layer.clone()),
        Op::Patch(updated) => {
            for patch in updated {
                if let Some(slot) = layers.iter_mut().find(|l| l.id == patch.id) {
                    *slot = patch.clone();
                }
            }
        }
        Op::Delete(ids) => layers.retain(|l| !ids.contains(&l.id)),
        Op::Crop { x, y, w, h } => {
            *image = Arc::new(image.crop(*x, *y, *w, *h));
            for layer in layers.iter_mut() {
                layer.shape.translate(-(*x as f32), -(*y as f32));
            }
        }
        Op::Cut(band) => {
            *image = Arc::new(cut::remove_band(image, *band));
            for layer in layers.iter_mut() {
                layer.shape.shift_for_cut(*band);
            }
        }
        // A moldura não mexe na imagem: é montada em volta, no fim.
        Op::Backdrop(_) => {}
    }
}

impl Document {
    pub fn new(image: RgbaImage) -> Self {
        let image = Arc::new(image);
        Self {
            baseline: Baseline {
                image: image.clone(),
                layers: Vec::new(),
                crops: Vec::new(),
            },
            ops: Vec::new(),
            index: 0,
            next_id: 1,
            image: image.clone(),
            redacted: image.clone(),
            framed: image,
            backdrop: BackdropStyle::None,
            layers: Vec::new(),
            pixels_version: 0,
            redactions: Vec::new(),
            spotlights: Vec::new(),
            crops: Vec::new(),
            image_version: 0,
            pending: None,
        }
    }

    /// Imagem exibida: conteúdo redigido dentro da moldura, se houver uma.
    pub fn visible_image(&self) -> &Arc<RgbaImage> {
        &self.framed
    }

    /// Só o conteúdo, sem moldura — é nesse espaço que as anotações vivem.
    pub fn content_image(&self) -> &Arc<RgbaImage> {
        &self.redacted
    }

    /// Deslocamento do conteúdo dentro da imagem exibida, em px. As
    /// anotações continuam em coordenadas do conteúdo; quem desenha soma
    /// isto.
    pub fn content_offset(&self) -> f32 {
        if self.backdrop == BackdropStyle::None {
            0.0
        } else {
            backdrop::MARGIN.round()
        }
    }

    pub fn backdrop(&self) -> BackdropStyle {
        self.backdrop
    }

    /// Troca a moldura decorativa — uma operação do histórico como as outras.
    pub fn set_backdrop(&mut self, style: BackdropStyle) {
        if style != self.backdrop {
            self.commit(Op::Backdrop(style));
        }
    }

    /// Imagem de origem, antes de qualquer operação — é o que a gravação de
    /// sessão guarda, já que o resto é reconstruído por replay.
    pub fn source_image(&self) -> &Arc<RgbaImage> {
        &self.baseline.image
    }

    /// Operações registradas e quantas estão aplicadas — o que a gravação
    /// de sessão precisa para reconstruir o documento depois de um fechamento
    /// inesperado.
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    pub fn applied(&self) -> usize {
        self.index
    }

    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    /// Recria um documento a partir da imagem de origem e de um log gravado.
    pub fn restore(image: RgbaImage, ops: Vec<Op>, index: usize, next_id: u64) -> Self {
        let mut doc = Self::new(image);
        doc.index = index.min(ops.len());
        doc.ops = ops;
        doc.next_id = next_id.max(1);
        doc.replay();
        doc
    }

    /// Selo dos pixels visíveis — muda com recorte e com redação.
    pub fn pixels_version(&self) -> u64 {
        self.pixels_version
    }

    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// Selo da imagem visível: só avança quando o enquadramento muda de
    /// verdade. É o que o editor usa para decidir se precisa refazer a
    /// textura e reajustar zoom e pan.
    pub fn image_version(&self) -> u64 {
        self.image_version
    }

    /// Reconstrói o estado visível a partir da base e das operações aplicadas.
    fn replay(&mut self) {
        let mut image = self.baseline.image.clone();
        let mut layers = self.baseline.layers.clone();
        let mut crops = self.baseline.crops.clone();
        for op in &self.ops[..self.index] {
            // Recortes e cortes mudam o tamanho da imagem: os dois entram na
            // assinatura que decide se o enquadramento mudou.
            match op {
                Op::Crop { x, y, w, h } => crops.push(Reframe::Crop(*x, *y, *w, *h)),
                Op::Cut(band) => crops.push(Reframe::Cut(*band)),
                _ => {}
            }
            apply(op, &mut image, &mut layers);
        }

        let reframed = crops != self.crops;
        if reframed {
            self.crops = crops;
            self.image_version += 1;
        }

        // As redações queimam a imagem antes de qualquer coisa ser desenhada
        // por cima: uma seta sobre a área redigida continua visível, e o que
        // estava embaixo não volta nem na tela nem no arquivo.
        let marks = redaction_marks(&layers);
        // Os holofotes vêm depois das redações, de propósito: a lupa nunca
        // pode ampliar o que foi censurado.
        let lights = spotlights(&layers);
        let redacted = if marks.is_empty() && lights.is_empty() {
            image.clone()
        } else {
            let mut burnt = (*image).clone();
            for mark in &marks {
                redact::apply(&mut burnt, mark.min, mark.max, mark.style, mark.seed);
            }
            spotlight::apply(&mut burnt, &lights);
            Arc::new(burnt)
        };
        // A moldura é a última coisa: ela emoldura o resultado de tudo.
        let backdrop = self.ops[..self.index]
            .iter()
            .rev()
            .find_map(|op| match op {
                Op::Backdrop(style) => Some(*style),
                _ => None,
            })
            .unwrap_or(BackdropStyle::None);
        let framed = if backdrop == BackdropStyle::None {
            redacted.clone()
        } else {
            Arc::new(backdrop::compose(&redacted, backdrop))
        };

        if reframed
            || marks != self.redactions
            || lights != self.spotlights
            || backdrop != self.backdrop
        {
            self.redactions = marks;
            self.spotlights = lights;
            self.pixels_version += 1;
        }
        // Trocar a moldura muda o tamanho do que se vê: o editor precisa
        // reajustar o enquadramento, como faz depois de um recorte.
        if backdrop != self.backdrop {
            self.backdrop = backdrop;
            self.image_version += 1;
        }

        self.image = image;
        self.redacted = redacted;
        self.framed = framed;
        self.layers = layers;
    }

    /// Registra uma operação: descarta o refazer pendente, consolida o log
    /// que passou do teto e reconstrói o estado.
    fn commit(&mut self, op: Op) {
        self.ops.truncate(self.index);
        self.ops.push(op);
        self.index = self.ops.len();
        while self.ops.len() > MAX_OPS {
            let oldest = self.ops.remove(0);
            match oldest {
                Op::Crop { x, y, w, h } => self.baseline.crops.push(Reframe::Crop(x, y, w, h)),
                Op::Cut(band) => self.baseline.crops.push(Reframe::Cut(band)),
                _ => {}
            }
            apply(&oldest, &mut self.baseline.image, &mut self.baseline.layers);
            self.index -= 1;
        }
        self.replay();
    }

    /// Cria uma anotação e devolve o `id` atribuído a ela.
    pub fn push(&mut self, shape: Shape, style: Style) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.commit(Op::Annotate(Layer { id, shape, style }));
        id
    }

    /// Número do próximo contador: um a mais que o maior entre os que estão
    /// na tela.
    ///
    /// A sequência acompanha o que se vê, não o histórico: apagar o contador
    /// de maior número faz o próximo reusá-lo, em vez de deixar um buraco na
    /// numeração.
    pub fn next_marker(&self) -> u32 {
        self.layers
            .iter()
            .filter_map(|layer| match layer.shape {
                Shape::Marker { number, .. } => Some(number),
                _ => None,
            })
            .max()
            .map_or(1, |highest| highest + 1)
    }

    /// Remove as anotações dos índices dados — uma única operação, para o
    /// desfazer trazer todas de volta de uma vez.
    pub fn delete_all(&mut self, indices: &[usize]) {
        let ids: Vec<u64> = indices
            .iter()
            .filter_map(|i| self.layers.get(*i).map(|l| l.id))
            .collect();
        if !ids.is_empty() {
            self.commit(Op::Delete(ids));
        }
    }

    /// Duplica a anotação de índice `index`, deslocada por `(dx, dy)`.
    /// A cópia nasce no topo da pilha e recebe um `id` próprio.
    pub fn duplicate(&mut self, index: usize, dx: f32, dy: f32) -> Option<u64> {
        let source = self.layers.get(index)?;
        let mut shape = source.shape.clone();
        let style = source.style;
        shape.translate(dx, dy);
        // A cópia de uma redação ganha semente própria: dois mosaicos
        // idênticos denunciariam que escondem a mesma coisa.
        if let Shape::Redaction { seed, .. } = &mut shape {
            *seed = redact::fresh_seed();
        }
        Some(self.push(shape, style))
    }

    /// Remove uma faixa da imagem e junta o que sobrou.
    pub fn cut(&mut self, band: Band) {
        self.commit(Op::Cut(band));
    }

    /// Recorta a imagem para `(x, y, w, h)` em px da imagem e desloca as
    /// anotações junto, mantendo-as sobre o mesmo conteúdo — uma única
    /// edição no histórico (issue #5).
    pub fn crop(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.commit(Op::Crop { x, y, w, h });
    }

    /// Início de um arrasto de reposicionamento: guarda o estado atual fora
    /// do histórico.
    pub fn begin_move(&mut self) {
        self.pending = Some(self.layers.clone());
    }

    /// Deslocamento incremental das anotações durante o arrasto.
    pub fn translate_all(&mut self, indices: &[usize], dx: f32, dy: f32) {
        for index in indices {
            if let Some(layer) = self.layers.get_mut(*index) {
                layer.shape.translate(dx, dy);
            }
        }
    }

    /// Arrasta uma alça da anotação `index`. Como o movimento, só entra no
    /// histórico quando o arrasto termina (`end_move`).
    pub fn resize(&mut self, index: usize, handle: Handle, to: Point, constrain: bool) {
        if let Some(layer) = self.layers.get_mut(index) {
            layer.resize(handle, to, constrain);
        }
    }

    /// Anotações inteiramente dentro do retângulo — o critério do marquee.
    ///
    /// Contenção e não interseção: passar o laço por cima de meia dúzia de
    /// anotações para pegar uma só seria o oposto do esperado.
    pub fn layers_within(&self, min: Point, max: Point) -> Vec<usize> {
        self.layers
            .iter()
            .enumerate()
            .filter(|(_, layer)| {
                layer.bbox().is_some_and(|(lo, hi)| {
                    lo.x >= min.x && lo.y >= min.y && hi.x <= max.x && hi.y <= max.y
                })
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Troca o estilo da anotação `index`. Também só entra no histórico ao
    /// fim da corrida — arrastar um controle de espessura geraria um passo
    /// de desfazer por quadro.
    pub fn set_style(&mut self, index: usize, style: Style) {
        if let Some(layer) = self.layers.get_mut(index) {
            layer.style = style;
        }
    }

    /// Confirma o arrasto: o que mudou vira **uma** operação de patch.
    /// Um arrasto que não moveu nada não entra no histórico.
    pub fn end_move(&mut self) {
        let Some(before) = self.pending.take() else {
            return;
        };
        let changed: Vec<Layer> = self
            .layers
            .iter()
            .filter(|now| !before.iter().any(|old| old == *now))
            .cloned()
            .collect();
        if changed.is_empty() {
            return;
        }
        self.commit(Op::Patch(changed));
    }

    /// Descarta o arrasto em andamento (clique parado, troca de ferramenta,
    /// Esc): restaura o estado anterior sem tocar no histórico.
    pub fn abort_move(&mut self) {
        if let Some(before) = self.pending.take() {
            self.layers = before;
        }
    }

    pub fn can_undo(&self) -> bool {
        self.index > 0
    }

    pub fn can_redo(&self) -> bool {
        self.index < self.ops.len()
    }

    pub fn undo(&mut self) {
        if self.index > 0 {
            self.index -= 1;
            self.replay();
        }
    }

    pub fn redo(&mut self) {
        if self.index < self.ops.len() {
            self.index += 1;
            self.replay();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::shapes::{
        shape_from_drag, Point, SpotlightForm, Tool, MAGNIFICATION_DEFAULT,
    };

    fn doc() -> Document {
        Document::new(RgbaImage::filled(64, 48, [10, 20, 30, 255]))
    }

    fn style() -> Style {
        Style {
            color: [255, 0, 0, 255],
            stroke_width: 3.0,
            font_size: 24.0,
            filled: false,
            corner_radius: 0.0,
            text_pill: false,
            redaction: RedactionStyle::default(),
            spotlight: SpotlightForm::default(),
            magnification: MAGNIFICATION_DEFAULT,
        }
    }

    fn line() -> Shape {
        shape_from_drag(Tool::Line, Point::new(0.0, 0.0), Point::new(1.0, 1.0), false, false).unwrap()
    }

    fn rect(x: f32, y: f32) -> Shape {
        Shape::Rect { min: Point::new(x, y), max: Point::new(x + 10.0, y + 10.0) }
    }

    fn shapes(doc: &Document) -> Vec<Shape> {
        doc.layers().iter().map(|l| l.shape.clone()).collect()
    }

    #[test]
    fn undo_redo_cycle() {
        let mut doc = doc();
        doc.push(line(), style());
        assert!(doc.can_undo());
        doc.undo();
        assert!(doc.layers().is_empty() && doc.can_redo());
        doc.redo();
        assert_eq!(shapes(&doc), vec![line()]);
        // Nova forma limpa o redo.
        doc.undo();
        doc.push(line(), style());
        assert!(!doc.can_redo());
    }

    #[test]
    fn every_layer_gets_a_distinct_id_even_after_undo() {
        let mut doc = doc();
        let first = doc.push(line(), style());
        doc.undo();
        let second = doc.push(line(), style());
        assert_ne!(first, second, "o id não pode ser reaproveitado");
    }

    #[test]
    fn move_undo_redo_and_abort() {
        let mut doc = doc();
        doc.push(rect(0.0, 0.0), style());

        // Movimento real: um único ponto de undo para o arrasto inteiro.
        doc.begin_move();
        doc.translate_all(&[0], 3.0, 0.0);
        doc.translate_all(&[0], 2.0, 4.0);
        doc.end_move();
        assert_eq!(shapes(&doc), vec![rect(5.0, 4.0)]);

        doc.undo();
        assert_eq!(shapes(&doc), vec![rect(0.0, 0.0)]);
        doc.redo();
        assert_eq!(shapes(&doc), vec![rect(5.0, 4.0)]);

        // Movimento abortado no meio: posição restaurada, histórico intacto.
        doc.begin_move();
        doc.translate_all(&[0], 100.0, 100.0);
        doc.abort_move();
        assert_eq!(shapes(&doc), vec![rect(5.0, 4.0)]);

        doc.undo(); // desfaz o movimento
        doc.undo(); // desfaz a criação
        assert!(doc.layers().is_empty() && !doc.can_undo());
    }

    #[test]
    fn a_drag_that_moved_nothing_is_not_history() {
        let mut doc = doc();
        doc.push(rect(0.0, 0.0), style());
        doc.begin_move();
        doc.end_move();
        doc.undo();
        assert!(doc.layers().is_empty(), "só a criação estava no histórico");
    }

    #[test]
    fn select_click_preserves_redo() {
        let mut doc = doc();
        doc.push(line(), style());
        doc.push(line(), style());
        doc.undo(); // segunda forma vai para o redo
        assert!(doc.can_redo());

        // Clique parado com a ferramenta Mover (selecionar sem arrastar):
        // não é edição — o refazer precisa sobreviver.
        doc.begin_move();
        doc.abort_move();
        assert!(doc.can_redo(), "clique de seleção não pode destruir o redo");
        doc.redo();
        assert_eq!(doc.layers().len(), 2);
    }

    #[test]
    fn a_run_of_style_changes_is_a_single_undo_step() {
        // Arrastar o controle de espessura mexe na anotação a cada quadro;
        // o histórico só pode receber um passo pela corrida inteira.
        let mut doc = doc();
        doc.push(rect(0.0, 0.0), style());
        doc.begin_move();
        for width in [4.0, 5.0, 6.0, 7.0] {
            doc.set_style(0, Style { stroke_width: width, ..style() });
        }
        doc.end_move();
        assert_eq!(doc.layers()[0].style.stroke_width, 7.0);

        doc.undo();
        assert_eq!(doc.layers()[0].style.stroke_width, 3.0, "volta de uma vez só");
        assert!(doc.can_undo(), "resta a criação da anotação");
    }

    #[test]
    fn restyling_keeps_the_layer_identity() {
        let mut doc = doc();
        let id = doc.push(rect(0.0, 0.0), style());
        doc.begin_move();
        doc.set_style(0, Style { color: [0, 0, 255, 255], ..style() });
        doc.end_move();
        assert_eq!(doc.layers()[0].id, id, "trocar o estilo não cria outra anotação");
        assert_eq!(doc.layers().len(), 1);
    }

    fn marker(n: u32) -> Shape {
        Shape::Marker { center: Point::new(n as f32 * 10.0, 10.0), number: n }
    }

    #[test]
    fn marker_numbering_follows_what_is_on_screen() {
        let mut doc = doc();
        assert_eq!(doc.next_marker(), 1, "o primeiro contador é o 1");
        doc.push(marker(1), style());
        doc.push(marker(2), style());
        doc.push(marker(3), style());
        assert_eq!(doc.next_marker(), 4);

        // Apagar o maior devolve o número: a sequência acompanha a tela, não
        // o histórico.
        doc.delete_all(&[2]);
        assert_eq!(doc.next_marker(), 3);

        // Apagar um do meio não renumera os outros nem reusa o buraco.
        doc.push(marker(3), style());
        doc.delete_all(&[1]);
        assert_eq!(doc.next_marker(), 4);
    }

    #[test]
    fn undoing_a_marker_frees_its_number_again() {
        let mut doc = doc();
        doc.push(marker(1), style());
        doc.push(marker(2), style());
        doc.undo();
        assert_eq!(doc.next_marker(), 2, "o 2 volta a estar livre");
    }

    #[test]
    fn a_redaction_burns_the_visible_image_and_undo_brings_it_back() {
        let mut doc = doc();
        let before = doc.visible_image().pixel(20, 20);
        let shape = Shape::Redaction {
            min: Point::new(10.0, 10.0),
            max: Point::new(40.0, 30.0),
            seed: 5,
        };
        // Sólida: numa imagem de cor única o mosaico devolveria a própria cor
        // (a paleta sai da região), e o teste não provaria nada.
        doc.push(shape, Style { redaction: RedactionStyle::Solid, ..style() });

        assert_ne!(doc.visible_image().pixel(20, 20), before, "a região foi apagada");
        assert_eq!(
            doc.visible_image().pixel(60, 40),
            before,
            "fora da região, nada muda"
        );

        // A redação é uma anotação como as outras: desfazer devolve os pixels,
        // porque o replay parte sempre da imagem de origem.
        doc.undo();
        assert_eq!(doc.visible_image().pixel(20, 20), before);
    }

    #[test]
    fn the_pixels_version_only_moves_when_the_pixels_do() {
        let mut doc = doc();
        let start = doc.pixels_version();
        doc.push(rect(0.0, 0.0), style());
        assert_eq!(doc.pixels_version(), start, "uma seta não mexe nos pixels");

        doc.push(
            Shape::Redaction {
                min: Point::new(4.0, 4.0),
                max: Point::new(20.0, 20.0),
                seed: 3,
            },
            style(),
        );
        assert_ne!(doc.pixels_version(), start, "a redação mexe");
    }

    #[test]
    fn duplicating_a_redaction_gives_it_a_new_mosaic() {
        let mut doc = doc();
        doc.push(
            Shape::Redaction {
                min: Point::new(4.0, 4.0),
                max: Point::new(40.0, 30.0),
                seed: 11,
            },
            style(),
        );
        doc.duplicate(0, 0.0, 0.0).unwrap();
        let seeds: Vec<u32> = doc
            .layers()
            .iter()
            .filter_map(|l| match l.shape {
                Shape::Redaction { seed, .. } => Some(seed),
                _ => None,
            })
            .collect();
        assert_eq!(seeds.len(), 2);
        assert_ne!(seeds[0], seeds[1], "duas redações iguais se denunciariam");
    }

    #[test]
    fn the_marquee_takes_what_is_wholly_inside_it() {
        // Contenção e não interseção: encostar o laço numa anotação não a
        // seleciona.
        let mut doc = doc();
        doc.push(rect(5.0, 5.0), style());   // 5..15 — dentro
        doc.push(rect(30.0, 5.0), style());  // 30..40 — só encosta
        let inside = doc.layers_within(Point::new(0.0, 0.0), Point::new(20.0, 20.0));
        assert_eq!(inside, vec![0]);
    }

    #[test]
    fn deleting_a_selection_is_a_single_undo_step() {
        let mut doc = doc();
        doc.push(rect(0.0, 0.0), style());
        doc.push(rect(20.0, 0.0), style());
        doc.push(rect(40.0, 0.0), style());
        doc.delete_all(&[0, 2]);
        assert_eq!(doc.layers().len(), 1);
        doc.undo();
        assert_eq!(doc.layers().len(), 3, "as duas voltam juntas");
    }

    #[test]
    fn moving_a_selection_moves_every_member() {
        let mut doc = doc();
        doc.push(rect(0.0, 0.0), style());
        doc.push(rect(20.0, 0.0), style());
        doc.begin_move();
        doc.translate_all(&[0, 1], 5.0, 5.0);
        doc.end_move();
        assert_eq!(shapes(&doc), vec![rect(5.0, 5.0), rect(25.0, 5.0)]);
        doc.undo();
        assert_eq!(shapes(&doc), vec![rect(0.0, 0.0), rect(20.0, 0.0)], "e voltam juntas");
    }

    #[test]
    fn delete_removes_the_layer_and_is_undoable() {
        let mut doc = doc();
        doc.push(rect(0.0, 0.0), style());
        let second = doc.push(rect(20.0, 20.0), style());
        doc.delete_all(&[0]);
        assert_eq!(doc.layers().len(), 1);
        assert_eq!(doc.layers()[0].id, second, "sobrou a anotação certa");
        doc.undo();
        assert_eq!(doc.layers().len(), 2);
    }

    #[test]
    fn duplicate_offsets_the_copy_and_gives_it_a_new_id() {
        let mut doc = doc();
        let original = doc.push(rect(10.0, 10.0), style());
        let copy = doc.duplicate(0, -5.0, 5.0).unwrap();
        assert_ne!(original, copy);
        assert_eq!(doc.layers().len(), 2);
        assert_eq!(shapes(&doc)[1], rect(5.0, 15.0), "cópia deslocada");
        doc.undo();
        assert_eq!(doc.layers().len(), 1, "duplicar é um passo do histórico");
    }

    #[test]
    fn crop_resizes_image_and_moves_shapes() {
        let mut doc = doc();
        doc.push(rect(30.0, 20.0), style());
        doc.crop(10, 5, 32, 24);

        assert_eq!((doc.visible_image().width(), doc.visible_image().height()), (32, 24));
        // A anotação acompanha o conteúdo: (30,20) − (10,5) = (20,15).
        assert_eq!(shapes(&doc), vec![rect(20.0, 15.0)]);
    }

    #[test]
    fn crop_undo_restores_image_and_shapes() {
        let mut doc = doc();
        doc.push(rect(30.0, 20.0), style());
        doc.crop(10, 5, 32, 24);

        doc.undo();
        assert_eq!((doc.visible_image().width(), doc.visible_image().height()), (64, 48));
        assert_eq!(shapes(&doc), vec![rect(30.0, 20.0)], "anotação volta ao lugar");

        doc.redo();
        assert_eq!((doc.visible_image().width(), doc.visible_image().height()), (32, 24));
        assert_eq!(shapes(&doc), vec![rect(20.0, 15.0)]);
    }

    #[test]
    fn successive_crops_compose() {
        let mut doc = doc();
        doc.push(rect(30.0, 20.0), style());
        doc.crop(10, 5, 40, 40);
        doc.crop(5, 5, 20, 20);

        assert_eq!((doc.visible_image().width(), doc.visible_image().height()), (20, 20));
        assert_eq!(shapes(&doc), vec![rect(15.0, 10.0)]);

        // Cada recorte é um passo próprio no histórico.
        doc.undo();
        assert_eq!((doc.visible_image().width(), doc.visible_image().height()), (40, 40));
        doc.undo();
        assert_eq!((doc.visible_image().width(), doc.visible_image().height()), (64, 48));
        // Resta a criação da anotação — só então o histórico se esgota.
        assert!(doc.can_undo());
        doc.undo();
        assert!(doc.layers().is_empty() && !doc.can_undo());
    }

    #[test]
    fn a_cut_shortens_the_image_and_drags_the_annotations_along() {
        use crate::editor::cut::{Axis, Band};
        let mut doc = doc(); // 64×48
        doc.push(rect(0.0, 2.0), style()); // acima da faixa
        doc.push(rect(0.0, 30.0), style()); // abaixo da faixa
        doc.cut(Band { axis: Axis::Horizontal, start: 10, end: 20 });

        assert_eq!(doc.visible_image().height(), 38, "10 linhas a menos");
        let ys: Vec<f32> = doc
            .layers()
            .iter()
            .map(|l| l.bbox().unwrap().0.y)
            .collect();
        assert_eq!(ys[0], 2.0, "o que estava antes não se move");
        assert_eq!(ys[1], 20.0, "o que estava depois sobe pela faixa removida");
    }

    #[test]
    fn a_cut_is_undoable_like_any_other_edit() {
        use crate::editor::cut::{Axis, Band};
        let mut doc = doc();
        doc.cut(Band { axis: Axis::Vertical, start: 5, end: 25 });
        assert_eq!(doc.visible_image().width(), 44);
        doc.undo();
        assert_eq!(doc.visible_image().width(), 64);
    }

    #[test]
    fn crop_clamps_to_image_bounds() {
        let mut doc = doc();
        // Pedido maior que a imagem: o recorte para na borda, sem panicar.
        doc.crop(60, 40, 999, 999);
        assert_eq!((doc.visible_image().width(), doc.visible_image().height()), (4, 8));
    }

    #[test]
    fn history_is_capped_and_the_oldest_edits_become_permanent() {
        let mut doc = doc();
        // Um recorte primeiro, depois operações suficientes para expulsá-lo.
        doc.crop(4, 4, 40, 40);
        for i in 0..MAX_OPS + 10 {
            doc.push(rect(i as f32, 0.0), style());
        }
        assert!(doc.ops.len() <= MAX_OPS, "o log não pode crescer sem limite");

        // Desfazer tudo o que ainda está no log não pode ressuscitar a
        // imagem original: o recorte saiu do histórico e virou permanente.
        while doc.can_undo() {
            doc.undo();
        }
        assert_eq!(
            (doc.visible_image().width(), doc.visible_image().height()),
            (40, 40),
            "o recorte consolidado continua valendo"
        );
    }

    #[test]
    fn consolidated_annotations_survive_a_full_undo() {
        let mut doc = doc();
        let first = doc.push(rect(1.0, 1.0), style());
        for _ in 0..MAX_OPS + 5 {
            doc.push(line(), style());
        }
        while doc.can_undo() {
            doc.undo();
        }
        assert!(
            doc.layers().iter().any(|l| l.id == first),
            "a anotação mais antiga foi assada na base e não some"
        );
    }
}
