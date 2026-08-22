//! Corte: remove uma faixa da imagem e junta o que sobrou.
//!
//! Serve para encurtar uma captura longa sem perder as pontas — tirar o meio
//! de um log, aproximar o cabeçalho do rodapé. As anotações acompanham: o
//! que estava depois da faixa sobe (ou vem para a esquerda), e o que estava
//! *dentro* dela encosta na costura, já que o lugar onde morava deixou de
//! existir.

use crate::imgbuf::RgbaImage;

use super::shapes::Point;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Remove linhas: a imagem fica mais baixa.
    Horizontal,
    /// Remove colunas: a imagem fica mais estreita.
    Vertical,
}

/// A faixa removida, em px da imagem. `start` é inclusivo, `end` exclusivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    pub axis: Axis,
    pub start: u32,
    pub end: u32,
}

impl Band {
    /// Constrói a faixa a partir de um arrasto, escolhendo o eixo pelo lado
    /// que o gesto percorreu mais. Um arrasto sobretudo vertical quer dizer
    /// "tire estas linhas".
    pub fn from_drag(a: Point, b: Point) -> Self {
        let (dx, dy) = ((b.x - a.x).abs(), (b.y - a.y).abs());
        if dy >= dx {
            Band::new(Axis::Horizontal, a.y, b.y)
        } else {
            Band::new(Axis::Vertical, a.x, b.x)
        }
    }

    fn new(axis: Axis, from: f32, to: f32) -> Self {
        let start = from.min(to).max(0.0).round() as u32;
        let end = from.max(to).max(0.0).round() as u32;
        Band { axis, start, end }
    }

    pub fn width(self) -> u32 {
        self.end.saturating_sub(self.start)
    }
}

/// Remove a faixa e devolve a imagem já colada.
///
/// Uma faixa vazia, ou que tomaria a imagem inteira, é ignorada: o corte não
/// pode deixar nada para trás.
pub fn remove_band(img: &RgbaImage, band: Band) -> RgbaImage {
    let extent = match band.axis {
        Axis::Horizontal => img.height(),
        Axis::Vertical => img.width(),
    };
    let start = band.start.min(extent);
    let end = band.end.min(extent);
    let removed = end.saturating_sub(start);
    if removed == 0 || removed >= extent {
        return img.clone();
    }

    match band.axis {
        Axis::Horizontal => {
            let mut out = RgbaImage::filled(img.width(), extent - removed, [0, 0, 0, 255]);
            let top = img.crop(0, 0, img.width(), start);
            let bottom = img.crop(0, end, img.width(), extent - end);
            out.paste(&top, 0, 0);
            out.paste(&bottom, 0, start as i64);
            out
        }
        Axis::Vertical => {
            let mut out = RgbaImage::filled(extent - removed, img.height(), [0, 0, 0, 255]);
            let left = img.crop(0, 0, start, img.height());
            let right = img.crop(end, 0, extent - end, img.height());
            out.paste(&left, 0, 0);
            out.paste(&right, start as i64, 0);
            out
        }
    }
}

/// Para onde uma coordenada vai depois do corte.
///
/// O que estava antes da faixa não se move; o que estava depois sobe pela
/// largura removida; o que estava dentro colapsa na costura — o lugar onde
/// ele morava simplesmente não existe mais.
pub fn shift(value: f32, start: f32, end: f32) -> f32 {
    if value <= start {
        value
    } else if value >= end {
        value - (end - start)
    } else {
        start
    }
}

/// Desloca uma anotação para acompanhar o corte.
pub fn shift_point(p: &mut Point, band: Band) {
    let (start, end) = (band.start as f32, band.end as f32);
    match band.axis {
        Axis::Horizontal => p.y = shift(p.y, start, end),
        Axis::Vertical => p.x = shift(p.x, start, end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Imagem com faixas horizontais numeradas pelo canal vermelho.
    fn striped(w: u32, h: u32) -> RgbaImage {
        let mut img = RgbaImage::filled(w, h, [0, 0, 0, 255]);
        for y in 0..h {
            for x in 0..w {
                img.pixel_mut(x, y).copy_from_slice(&[y as u8, 0, 0, 255]);
            }
        }
        img
    }

    #[test]
    fn a_horizontal_cut_shortens_the_image_and_joins_the_ends() {
        let img = striped(8, 20);
        let out = remove_band(&img, Band { axis: Axis::Horizontal, start: 5, end: 12 });
        assert_eq!((out.width(), out.height()), (8, 13), "7 linhas a menos");
        assert_eq!(out.pixel(0, 4)[0], 4, "o que estava antes ficou onde estava");
        assert_eq!(out.pixel(0, 5)[0], 12, "a linha 12 encostou na 4");
    }

    #[test]
    fn a_vertical_cut_narrows_the_image() {
        let mut img = RgbaImage::filled(20, 4, [0, 0, 0, 255]);
        for y in 0..4 {
            for x in 0..20 {
                img.pixel_mut(x, y).copy_from_slice(&[x as u8, 0, 0, 255]);
            }
        }
        let out = remove_band(&img, Band { axis: Axis::Vertical, start: 5, end: 12 });
        assert_eq!((out.width(), out.height()), (13, 4));
        assert_eq!(out.pixel(4, 0)[0], 4);
        assert_eq!(out.pixel(5, 0)[0], 12);
    }

    #[test]
    fn an_empty_band_changes_nothing() {
        let img = striped(8, 20);
        let out = remove_band(&img, Band { axis: Axis::Horizontal, start: 7, end: 7 });
        assert_eq!(out.as_raw(), img.as_raw());
    }

    #[test]
    fn a_band_that_would_take_everything_is_refused() {
        // Cortar a imagem inteira não deixaria nada — melhor não cortar.
        let img = striped(8, 20);
        let out = remove_band(&img, Band { axis: Axis::Horizontal, start: 0, end: 20 });
        assert_eq!((out.width(), out.height()), (8, 20));
    }

    #[test]
    fn a_band_past_the_edge_is_clipped() {
        let img = striped(8, 20);
        let out = remove_band(&img, Band { axis: Axis::Horizontal, start: 15, end: 999 });
        assert_eq!(out.height(), 15);
    }

    #[test]
    fn the_drag_axis_follows_the_longer_side() {
        let across = Band::from_drag(Point::new(0.0, 10.0), Point::new(4.0, 40.0));
        assert_eq!(across.axis, Axis::Horizontal, "arrasto vertical tira linhas");
        let along = Band::from_drag(Point::new(10.0, 0.0), Point::new(40.0, 4.0));
        assert_eq!(along.axis, Axis::Vertical, "arrasto horizontal tira colunas");
    }

    #[test]
    fn a_backwards_drag_still_makes_a_band() {
        let band = Band::from_drag(Point::new(0.0, 40.0), Point::new(0.0, 10.0));
        assert_eq!((band.start, band.end), (10, 40));
    }

    #[test]
    fn annotations_before_the_band_stay_put() {
        assert_eq!(shift(3.0, 10.0, 20.0), 3.0);
    }

    #[test]
    fn annotations_after_the_band_move_up_by_its_width() {
        assert_eq!(shift(30.0, 10.0, 20.0), 20.0);
    }

    #[test]
    fn annotations_inside_the_band_collapse_onto_the_seam() {
        // O lugar onde a anotação estava deixou de existir; ela encosta na
        // emenda em vez de sumir ou de ficar fora da imagem.
        assert_eq!(shift(15.0, 10.0, 20.0), 10.0);
    }

    #[test]
    fn shift_point_only_touches_the_cut_axis() {
        let mut p = Point::new(30.0, 30.0);
        shift_point(&mut p, Band { axis: Axis::Horizontal, start: 10, end: 20 });
        assert_eq!((p.x, p.y), (30.0, 20.0), "só o eixo do corte");

        let mut p = Point::new(30.0, 30.0);
        shift_point(&mut p, Band { axis: Axis::Vertical, start: 10, end: 20 });
        assert_eq!((p.x, p.y), (20.0, 30.0));
    }
}
