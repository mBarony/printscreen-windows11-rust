//! Documento editável do editor: a imagem base mais as anotações, com o
//! histórico de desfazer/refazer que versiona as duas juntas.
//!
//! O recorte (issue #5) troca a imagem base e desloca as anotações na mesma
//! edição, então o histórico não pode versionar apenas as formas: cada
//! snapshot guarda o par (imagem, formas). Snapshots são baratos — a imagem
//! é compartilhada por `Arc` (um snapshot que não a mudou custa um refcount)
//! e uma sessão tem no máximo dezenas de anotações.

use std::sync::Arc;

use crate::imgbuf::RgbaImage;

use super::shapes::Shape;

struct Snapshot {
    image: Arc<RgbaImage>,
    shapes: Vec<Shape>,
}

pub struct Document {
    image: Arc<RgbaImage>,
    shapes: Vec<Shape>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Snapshot do movimento em andamento (ferramenta Mover) — só entra no
    /// histórico quando o arrasto se confirma (`end_move`); um clique parado
    /// (`abort_move`) não toca nem no undo nem no redo.
    moving: Option<Vec<Shape>>,
}

impl Document {
    pub fn new(image: RgbaImage) -> Self {
        Self {
            image: Arc::new(image),
            shapes: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            moving: None,
        }
    }

    pub fn image(&self) -> &Arc<RgbaImage> {
        &self.image
    }

    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { image: self.image.clone(), shapes: self.shapes.clone() }
    }

    /// Registra o estado atual como ponto de desfazer (e, como toda edição
    /// nova, limpa a pilha de refazer, §8).
    fn checkpoint(&mut self) {
        let snapshot = self.snapshot();
        self.undo.push(snapshot);
        self.redo.clear();
    }

    pub fn push(&mut self, shape: Shape) {
        self.checkpoint();
        self.shapes.push(shape);
    }

    /// Recorta a imagem para `(x, y, w, h)` em px da imagem e desloca as
    /// anotações junto, mantendo-as sobre o mesmo conteúdo — uma única
    /// edição no histórico (issue #5).
    pub fn crop(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.checkpoint();
        self.image = Arc::new(self.image.crop(x, y, w, h));
        for shape in &mut self.shapes {
            shape.translate(-(x as f32), -(y as f32));
        }
    }

    /// Início de um arrasto de reposicionamento: guarda o estado atual fora
    /// do histórico — ele só vira ponto de desfazer se o movimento se
    /// confirmar (`end_move`).
    pub fn begin_move(&mut self) {
        self.moving = Some(self.shapes.clone());
    }

    /// Deslocamento incremental da forma `index` durante o arrasto.
    pub fn translate(&mut self, index: usize, dx: f32, dy: f32) {
        if let Some(shape) = self.shapes.get_mut(index) {
            shape.translate(dx, dy);
        }
    }

    /// Confirma o movimento: o estado pré-arrasto entra no undo e, como toda
    /// edição nova, a pilha de refazer é limpa (§8).
    pub fn end_move(&mut self) {
        if let Some(shapes) = self.moving.take() {
            self.undo.push(Snapshot { image: self.image.clone(), shapes });
            self.redo.clear();
        }
    }

    /// Descarta o movimento em andamento (clique parado, troca de ferramenta,
    /// Esc): restaura o estado pré-arrasto sem tocar no undo nem no redo.
    pub fn abort_move(&mut self) {
        if let Some(shapes) = self.moving.take() {
            self.shapes = shapes;
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(previous) = self.undo.pop() {
            let current = self.snapshot();
            self.redo.push(current);
            self.image = previous.image;
            self.shapes = previous.shapes;
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            let current = self.snapshot();
            self.undo.push(current);
            self.image = next.image;
            self.shapes = next.shapes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::shapes::{shape_from_drag, Point, Style, Tool};

    fn doc() -> Document {
        Document::new(RgbaImage::filled(64, 48, [10, 20, 30, 255]))
    }

    fn style() -> Style {
        Style { color: [255, 0, 0, 255], stroke_width: 3.0, font_size: 24.0 }
    }

    fn line() -> Shape {
        shape_from_drag(
            Tool::Line,
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            false,
            style(),
        )
        .unwrap()
    }

    fn rect_at(x: f32, y: f32) -> Shape {
        Shape::Rect {
            min: Point::new(x, y),
            max: Point::new(x + 10.0, y + 10.0),
            style: style(),
        }
    }

    #[test]
    fn undo_redo_cycle() {
        let mut doc = doc();
        doc.push(line());
        assert!(doc.can_undo());
        doc.undo();
        assert!(doc.shapes().is_empty() && doc.can_redo());
        doc.redo();
        assert_eq!(doc.shapes(), std::slice::from_ref(&line()));
        // Nova forma limpa o redo.
        doc.undo();
        doc.push(line());
        assert!(!doc.can_redo());
    }

    #[test]
    fn move_undo_redo_and_abort() {
        let mut doc = doc();
        doc.push(rect_at(0.0, 0.0));

        // Movimento real: um único ponto de undo para o arrasto inteiro.
        doc.begin_move();
        doc.translate(0, 3.0, 0.0);
        doc.translate(0, 2.0, 4.0);
        doc.end_move();
        let moved = doc.shapes()[0].clone();
        assert_eq!(moved, rect_at(5.0, 4.0));

        doc.undo();
        assert_eq!(doc.shapes()[0], rect_at(0.0, 0.0));
        doc.redo();
        assert_eq!(doc.shapes()[0], moved);

        // Movimento abortado no meio: posição restaurada, histórico intacto.
        doc.begin_move();
        doc.translate(0, 100.0, 100.0);
        doc.abort_move();
        assert_eq!(doc.shapes()[0], moved);

        doc.undo(); // desfaz o movimento
        doc.undo(); // desfaz a criação
        assert!(doc.shapes().is_empty() && !doc.can_undo());
    }

    #[test]
    fn select_click_preserves_redo() {
        let mut doc = doc();
        doc.push(line());
        doc.push(line());
        doc.undo(); // segunda forma vai para o redo
        assert!(doc.can_redo());

        // Clique parado com a ferramenta Mover (selecionar sem arrastar):
        // não é edição — o refazer precisa sobreviver.
        doc.begin_move();
        doc.abort_move();
        assert!(doc.can_redo(), "clique de seleção não pode destruir o redo");
        doc.redo();
        assert_eq!(doc.shapes().len(), 2);
    }

    #[test]
    fn crop_resizes_image_and_moves_shapes() {
        let mut doc = doc();
        doc.push(rect_at(30.0, 20.0));
        doc.crop(10, 5, 32, 24);

        assert_eq!((doc.image().width(), doc.image().height()), (32, 24));
        // A anotação acompanha o conteúdo: (30,20) − (10,5) = (20,15).
        assert_eq!(doc.shapes()[0], rect_at(20.0, 15.0));
    }

    #[test]
    fn crop_undo_restores_image_and_shapes() {
        let mut doc = doc();
        doc.push(rect_at(30.0, 20.0));
        doc.crop(10, 5, 32, 24);

        doc.undo();
        assert_eq!((doc.image().width(), doc.image().height()), (64, 48));
        assert_eq!(doc.shapes()[0], rect_at(30.0, 20.0), "anotação volta ao lugar");

        doc.redo();
        assert_eq!((doc.image().width(), doc.image().height()), (32, 24));
        assert_eq!(doc.shapes()[0], rect_at(20.0, 15.0));
    }

    #[test]
    fn successive_crops_compose() {
        let mut doc = doc();
        doc.push(rect_at(30.0, 20.0));
        doc.crop(10, 5, 40, 40);
        doc.crop(5, 5, 20, 20);

        assert_eq!((doc.image().width(), doc.image().height()), (20, 20));
        assert_eq!(doc.shapes()[0], rect_at(15.0, 10.0));

        // Cada recorte é um passo próprio no histórico.
        doc.undo();
        assert_eq!((doc.image().width(), doc.image().height()), (40, 40));
        doc.undo();
        assert_eq!((doc.image().width(), doc.image().height()), (64, 48));
        // Resta a criação da anotação — só então o histórico se esgota.
        assert!(doc.can_undo());
        doc.undo();
        assert!(doc.shapes().is_empty() && !doc.can_undo());
    }

    #[test]
    fn crop_clamps_to_image_bounds() {
        let mut doc = doc();
        // Pedido maior que a imagem: o recorte para na borda, sem panicar.
        doc.crop(60, 40, 999, 999);
        assert_eq!((doc.image().width(), doc.image().height()), (4, 8));
    }
}
