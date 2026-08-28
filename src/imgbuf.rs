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

    /// Reamostra por interpolação bilinear pelo fator dado.
    ///
    /// O alinhamento é por centro de pixel (`+0.5`), e não por canto: sem
    /// isso a imagem escorrega meio pixel para o canto superior esquerdo a
    /// cada redimensionamento, o que aparece depois de dois ou três.
    pub fn resized(&self, scale: f32) -> RgbaImage {
        let (sw, sh) = (self.width, self.height);
        if sw == 0 || sh == 0 || !scale.is_finite() || scale <= 0.0 {
            return RgbaImage::from_raw(0, 0, Vec::new());
        }
        let dw = ((sw as f32 * scale).round() as u32).max(1);
        let dh = ((sh as f32 * scale).round() as u32).max(1);
        let mut out = Vec::with_capacity(dw as usize * dh as usize * 4);

        for y in 0..dh {
            let fy = ((y as f32 + 0.5) / scale - 0.5).clamp(0.0, (sh - 1) as f32);
            let (y0, ty) = (fy.floor() as u32, fy - fy.floor());
            let y1 = (y0 + 1).min(sh - 1);
            for x in 0..dw {
                let fx = ((x as f32 + 0.5) / scale - 0.5).clamp(0.0, (sw - 1) as f32);
                let (x0, tx) = (fx.floor() as u32, fx - fx.floor());
                let x1 = (x0 + 1).min(sw - 1);
                let (p00, p10) = (self.pixel(x0, y0), self.pixel(x1, y0));
                let (p01, p11) = (self.pixel(x0, y1), self.pixel(x1, y1));
                let mut px = [0u8; 4];
                for (c, slot) in px.iter_mut().enumerate() {
                    let top = p00[c] as f32 * (1.0 - tx) + p10[c] as f32 * tx;
                    let bottom = p01[c] as f32 * (1.0 - tx) + p11[c] as f32 * tx;
                    *slot = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8;
                }
                out.extend_from_slice(&px);
            }
        }
        RgbaImage { width: dw, height: dh, pixels: out }
    }

    /// Multiplica o canal alfa pelo fator, deixando a imagem translúcida.
    ///
    /// Só faz sentido em formatos com alfa: gravada em JPG, uma imagem
    /// translúcida sairia opaca do mesmo jeito.
    pub fn with_opacity(&self, opacity: f32) -> RgbaImage {
        let k = opacity.clamp(0.0, 1.0);
        let mut pixels = self.pixels.clone();
        for px in pixels.as_chunks_mut::<4>().0.iter_mut() {
            px[3] = (px[3] as f32 * k).round().clamp(0.0, 255.0) as u8;
        }
        RgbaImage { width: self.width, height: self.height, pixels }
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
        for px in self.pixels.as_chunks::<4>().0 {
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
    fn opacidade_multiplica_o_alfa() {
        let img = RgbaImage::filled(2, 2, [10, 20, 30, 200]);
        let meio = img.with_opacity(0.5);
        assert_eq!(meio.pixel(0, 0), [10, 20, 30, 100], "só o alfa muda");
    }

    #[test]
    fn opacidade_cheia_nao_muda_nada() {
        let img = RgbaImage::filled(2, 2, [10, 20, 30, 200]);
        assert_eq!(img.with_opacity(1.0).pixel(0, 0), [10, 20, 30, 200]);
    }

    #[test]
    fn opacidade_fora_da_faixa_e_limitada() {
        let img = RgbaImage::filled(1, 1, [0, 0, 0, 255]);
        assert_eq!(img.with_opacity(5.0).pixel(0, 0)[3], 255);
        assert_eq!(img.with_opacity(-1.0).pixel(0, 0)[3], 0);
    }

    #[test]
    fn redimensionar_mantem_a_proporcao_e_as_bordas() {
        let img = gradient(64, 48);
        let menor = img.resized(0.5);
        assert_eq!((menor.width(), menor.height()), (32, 24));
        // O alinhamento por centro de pixel preserva os cantos.
        assert_eq!(menor.pixel(0, 0)[2], 7);
    }

    #[test]
    fn redimensionar_por_fator_invalido_da_imagem_vazia() {
        let img = gradient(8, 8);
        for fator in [0.0, -2.0, f32::NAN] {
            assert_eq!(img.resized(fator).width(), 0, "fator {fator}");
        }
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
