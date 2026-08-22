//! Buffer de imagem RGBA próprio (substitui `image::RgbaImage`).
//!
//! Pixels em RGBA8 não pré-multiplicado, linha a linha, origem no canto
//! superior esquerdo — o layout que egui (`ColorImage`), a exportação e o
//! clipboard consomem diretamente.

#[derive(Clone, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RgbaImage {
    /// Cria a partir de um buffer RGBA8 (`width × height × 4` bytes).
    /// (Usada pela captura GDI, que só compila no Windows.)
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn from_raw(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        assert_eq!(
            pixels.len(),
            width as usize * height as usize * 4,
            "buffer RGBA com tamanho inconsistente"
        );
        Self { width, height, pixels }
    }

    /// Imagem preenchida com uma única cor.
    pub fn filled(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..width as usize * height as usize {
            pixels.extend_from_slice(&rgba);
        }
        Self { width, height, pixels }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn as_raw(&self) -> &[u8] {
        &self.pixels
    }

    /// Cor de um pixel. Fora dos testes, é o que o conta-gotas do editor lê.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = self.index(x, y);
        self.pixels[i..i + 4].try_into().expect("4 bytes")
    }

    #[inline]
    pub fn pixel_mut(&mut self, x: u32, y: u32) -> &mut [u8] {
        let i = self.index(x, y);
        &mut self.pixels[i..i + 4]
    }

    #[inline]
    fn index(&self, x: u32, y: u32) -> usize {
        debug_assert!(x < self.width && y < self.height);
        (y as usize * self.width as usize + x as usize) * 4
    }

    /// Recorte `(x, y, w, h)` — limitado às bordas da imagem.
    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
        let x = x.min(self.width);
        let y = y.min(self.height);
        let w = w.min(self.width - x);
        let h = h.min(self.height - y);
        let mut pixels = Vec::with_capacity(w as usize * h as usize * 4);
        for row in y..y + h {
            let start = (row as usize * self.width as usize + x as usize) * 4;
            pixels.extend_from_slice(&self.pixels[start..start + w as usize * 4]);
        }
        RgbaImage { width: w, height: h, pixels }
    }

    /// Cola `src` com o canto superior esquerdo em `(dst_x, dst_y)`,
    /// recortando o que ficar fora (aceita offsets negativos).
    pub fn paste(&mut self, src: &RgbaImage, dst_x: i64, dst_y: i64) {
        for row in 0..src.height as i64 {
            let ty = dst_y + row;
            if ty < 0 || ty >= self.height as i64 {
                continue;
            }
            let src_x0 = (-dst_x).clamp(0, src.width as i64);
            let src_x1 = (self.width as i64 - dst_x).clamp(0, src.width as i64);
            if src_x0 >= src_x1 {
                continue;
            }
            let count = (src_x1 - src_x0) as usize * 4;
            let src_start = (row as usize * src.width as usize + src_x0 as usize) * 4;
            let dst_start =
                (ty as usize * self.width as usize + (dst_x + src_x0) as usize) * 4;
            self.pixels[dst_start..dst_start + count]
                .copy_from_slice(&src.pixels[src_start..src_start + count]);
        }
    }

    /// Converte para RGB8 (descarta alfa; a captura é opaca).
    pub fn to_rgb(&self) -> Vec<u8> {
        let mut rgb = Vec::with_capacity(self.width as usize * self.height as usize * 3);
        for px in self.pixels.chunks_exact(4) {
            rgb.extend_from_slice(&px[..3]);
        }
        rgb
    }
}

impl std::fmt::Debug for RgbaImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RgbaImage({}×{})", self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> RgbaImage {
        let mut img = RgbaImage::filled(w, h, [0, 0, 0, 255]);
        for y in 0..h {
            for x in 0..w {
                img.pixel_mut(x, y).copy_from_slice(&[x as u8, y as u8, 7, 255]);
            }
        }
        img
    }

    #[test]
    fn crop_exact_dimensions_and_content() {
        let img = gradient(64, 48);
        let c = img.crop(10, 5, 20, 12);
        assert_eq!((c.width(), c.height()), (20, 12));
        assert_eq!(c.pixel(0, 0), [10, 5, 7, 255]);
        assert_eq!(c.pixel(19, 11), [29, 16, 7, 255]);
    }

    #[test]
    fn crop_clamps_to_borders() {
        let img = gradient(16, 16);
        let c = img.crop(12, 12, 100, 100);
        assert_eq!((c.width(), c.height()), (4, 4));
    }

    #[test]
    fn paste_with_negative_offset() {
        let mut canvas = RgbaImage::filled(8, 8, [0, 0, 0, 255]);
        let src = RgbaImage::filled(4, 4, [255, 0, 0, 255]);
        canvas.paste(&src, -2, -2);
        assert_eq!(canvas.pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(canvas.pixel(2, 2), [0, 0, 0, 255]);
    }

    #[test]
    fn to_rgb_drops_alpha() {
        let img = RgbaImage::filled(2, 1, [1, 2, 3, 255]);
        assert_eq!(img.to_rgb(), vec![1, 2, 3, 1, 2, 3]);
    }
}
