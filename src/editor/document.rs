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

use super::shapes::{Handle, Layer, Point, Shape, Style};

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
}

/// Ponto de partida do replay: imagem e anotações já consolidadas.
struct Baseline {
    image: Arc<RgbaImage>,
    layers: Vec<Layer>,
    /// Recortes que já foram assados na imagem acima. Continuam contando na
    /// assinatura: consolidar um recorte não muda o que se vê, então não
    /// pode parecer uma mudança de enquadramento.
    crops: Vec<(u32, u32, u32, u32)>,
}

pub struct Document {
    baseline: Baseline,
    ops: Vec<Op>,
    /// Quantas operações do log estão aplicadas — tudo à frente é o "refazer".
    index: usize,
    next_id: u64,

    // Estado derivado, reconstruído por `replay`.
    image: Arc<RgbaImage>,
    layers: Vec<Layer>,

    /// Recortes aplicados no último replay, e um selo que só avança quando
    /// eles mudam. O replay reconstrói a imagem toda vez, então o `Arc` é
    /// sempre novo — comparar ponteiros faria o editor achar que a imagem
    /// mudou a cada anotação criada e jogar fora o zoom do usuário.
    crops: Vec<(u32, u32, u32, u32)>,
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
            image,
            layers: Vec::new(),
            crops: Vec::new(),
            image_version: 0,
            pending: None,
        }
    }

    pub fn image(&self) -> &Arc<RgbaImage> {
        &self.image
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
            if let Op::Crop { x, y, w, h } = op {
                crops.push((*x, *y, *w, *h));
            }
            apply(op, &mut image, &mut layers);
        }
        if crops != self.crops {
            self.crops = crops;
            self.image_version += 1;
        }
        self.image = image;
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
            if let Op::Crop { x, y, w, h } = oldest {
                self.baseline.crops.push((x, y, w, h));
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

    /// Remove a anotação de índice `index`.
    pub fn delete(&mut self, index: usize) {
        let Some(layer) = self.layers.get(index) else {
            return;
        };
        self.commit(Op::Delete(vec![layer.id]));
    }

    /// Duplica a anotação de índice `index`, deslocada por `(dx, dy)`.
    /// A cópia nasce no topo da pilha e recebe um `id` próprio.
    pub fn duplicate(&mut self, index: usize, dx: f32, dy: f32) -> Option<u64> {
        let source = self.layers.get(index)?;
        let mut shape = source.shape.clone();
        let style = source.style;
        shape.translate(dx, dy);
        Some(self.push(shape, style))
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

    /// Deslocamento incremental da anotação `index` durante o arrasto.
    pub fn translate(&mut self, index: usize, dx: f32, dy: f32) {
        if let Some(layer) = self.layers.get_mut(index) {
            layer.shape.translate(dx, dy);
        }
    }

    /// Arrasta uma alça da anotação `index`. Como o movimento, só entra no
    /// histórico quando o arrasto termina (`end_move`).
    pub fn resize(&mut self, index: usize, handle: Handle, to: Point, constrain: bool) {
        if let Some(layer) = self.layers.get_mut(index) {
            layer.resize(handle, to, constrain);
        }
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
    use crate::editor::shapes::{shape_from_drag, Point, Tool};

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
        doc.translate(0, 3.0, 0.0);
        doc.translate(0, 2.0, 4.0);
        doc.end_move();
        assert_eq!(shapes(&doc), vec![rect(5.0, 4.0)]);

        doc.undo();
        assert_eq!(shapes(&doc), vec![rect(0.0, 0.0)]);
        doc.redo();
        assert_eq!(shapes(&doc), vec![rect(5.0, 4.0)]);

        // Movimento abortado no meio: posição restaurada, histórico intacto.
        doc.begin_move();
        doc.translate(0, 100.0, 100.0);
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

    #[test]
    fn delete_removes_the_layer_and_is_undoable() {
        let mut doc = doc();
        doc.push(rect(0.0, 0.0), style());
        let second = doc.push(rect(20.0, 20.0), style());
        doc.delete(0);
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

        assert_eq!((doc.image().width(), doc.image().height()), (32, 24));
        // A anotação acompanha o conteúdo: (30,20) − (10,5) = (20,15).
        assert_eq!(shapes(&doc), vec![rect(20.0, 15.0)]);
    }

    #[test]
    fn crop_undo_restores_image_and_shapes() {
        let mut doc = doc();
        doc.push(rect(30.0, 20.0), style());
        doc.crop(10, 5, 32, 24);

        doc.undo();
        assert_eq!((doc.image().width(), doc.image().height()), (64, 48));
        assert_eq!(shapes(&doc), vec![rect(30.0, 20.0)], "anotação volta ao lugar");

        doc.redo();
        assert_eq!((doc.image().width(), doc.image().height()), (32, 24));
        assert_eq!(shapes(&doc), vec![rect(20.0, 15.0)]);
    }

    #[test]
    fn successive_crops_compose() {
        let mut doc = doc();
        doc.push(rect(30.0, 20.0), style());
        doc.crop(10, 5, 40, 40);
        doc.crop(5, 5, 20, 20);

        assert_eq!((doc.image().width(), doc.image().height()), (20, 20));
        assert_eq!(shapes(&doc), vec![rect(15.0, 10.0)]);

        // Cada recorte é um passo próprio no histórico.
        doc.undo();
        assert_eq!((doc.image().width(), doc.image().height()), (40, 40));
        doc.undo();
        assert_eq!((doc.image().width(), doc.image().height()), (64, 48));
        // Resta a criação da anotação — só então o histórico se esgota.
        assert!(doc.can_undo());
        doc.undo();
        assert!(doc.layers().is_empty() && !doc.can_undo());
    }

    #[test]
    fn crop_clamps_to_image_bounds() {
        let mut doc = doc();
        // Pedido maior que a imagem: o recorte para na borda, sem panicar.
        doc.crop(60, 40, 999, 999);
        assert_eq!((doc.image().width(), doc.image().height()), (4, 8));
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
            (doc.image().width(), doc.image().height()),
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
