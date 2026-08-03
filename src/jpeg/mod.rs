//! Codificador JPEG baseline (4:4:4) — o único formato de saída do RustShot.
//!
//! Código **incorporado e reduzido** do crate [`image`] v0.25
//! (<https://github.com/image-rs/image>, licença dupla MIT/Apache-2.0):
//! mantidos apenas o caminho RGB 8 bits, as tabelas padrão (Anexo K do
//! JPEG), o escritor de bits e os cabeçalhos JFIF — removidos EXIF/ICC,
//! tons de cinza, densidade configurável e a integração genérica com o
//! restante do crate. `transform.rs` (FDCT) e `entropy.rs` (códigos de
//! Huffman) são cópias verbatim, com seus avisos de licença preservados —
//! este software usa, em parte, trabalho do Independent JPEG Group.

mod entropy;
mod transform;

use std::io::{self, Write};

use entropy::build_huff_lut_const;

// Marcadores JPEG.
const SOF0: u8 = 0xC0; // baseline DCT
const DHT: u8 = 0xC4; // tabelas de Huffman
const SOI: u8 = 0xD8; // início da imagem
const EOI: u8 = 0xD9; // fim da imagem
const SOS: u8 = 0xDA; // início do scan
const DQT: u8 = 0xDB; // tabelas de quantização
const APP0: u8 = 0xE0; // segmento JFIF

// Seção K.1, tabelas K.1/K.2 do padrão JPEG.
#[rustfmt::skip]
static STD_LUMA_QTABLE: [u8; 64] = [
    16, 11, 10, 16,  24,  40,  51,  61,
    12, 12, 14, 19,  26,  58,  60,  55,
    14, 13, 16, 24,  40,  57,  69,  56,
    14, 17, 22, 29,  51,  87,  80,  62,
    18, 22, 37, 56,  68, 109, 103,  77,
    24, 35, 55, 64,  81, 104, 113,  92,
    49, 64, 78, 87, 103, 121, 120, 101,
    72, 92, 95, 98, 112, 100, 103,  99,
];

#[rustfmt::skip]
static STD_CHROMA_QTABLE: [u8; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99,
    18, 21, 26, 66, 99, 99, 99, 99,
    24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
];

// Seção K.3 — comprimentos e valores das tabelas de Huffman padrão.
static STD_LUMA_DC_CODE_LENGTHS: [u8; 16] = [
    0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
static STD_LUMA_DC_VALUES: [u8; 12] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
];
static STD_LUMA_DC_HUFF_LUT: [(u8, u16); 256] =
    build_huff_lut_const(&STD_LUMA_DC_CODE_LENGTHS, &STD_LUMA_DC_VALUES);

static STD_CHROMA_DC_CODE_LENGTHS: [u8; 16] = [
    0x00, 0x03, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
];
static STD_CHROMA_DC_VALUES: [u8; 12] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
];
static STD_CHROMA_DC_HUFF_LUT: [(u8, u16); 256] =
    build_huff_lut_const(&STD_CHROMA_DC_CODE_LENGTHS, &STD_CHROMA_DC_VALUES);

static STD_LUMA_AC_CODE_LENGTHS: [u8; 16] = [
    0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7D,
];
static STD_LUMA_AC_VALUES: [u8; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
    0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0,
    0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28,
    0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
    0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
    0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
    0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7,
    0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5,
    0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2,
    0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
    0xF9, 0xFA,
];
static STD_LUMA_AC_HUFF_LUT: [(u8, u16); 256] =
    build_huff_lut_const(&STD_LUMA_AC_CODE_LENGTHS, &STD_LUMA_AC_VALUES);

static STD_CHROMA_AC_CODE_LENGTHS: [u8; 16] = [
    0x00, 0x02, 0x01, 0x02, 0x04, 0x04, 0x03, 0x04, 0x07, 0x05, 0x04, 0x04, 0x00, 0x01, 0x02, 0x77,
];
static STD_CHROMA_AC_VALUES: [u8; 162] = [
    0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71,
    0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xA1, 0xB1, 0xC1, 0x09, 0x23, 0x33, 0x52, 0xF0,
    0x15, 0x62, 0x72, 0xD1, 0x0A, 0x16, 0x24, 0x34, 0xE1, 0x25, 0xF1, 0x17, 0x18, 0x19, 0x1A, 0x26,
    0x27, 0x28, 0x29, 0x2A, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
    0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
    0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
    0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5,
    0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3,
    0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA,
    0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
    0xF9, 0xFA,
];
static STD_CHROMA_AC_HUFF_LUT: [(u8, u16); 256] =
    build_huff_lut_const(&STD_CHROMA_AC_CODE_LENGTHS, &STD_CHROMA_AC_VALUES);

const DCCLASS: u8 = 0;
const ACCLASS: u8 = 1;
const LUMADESTINATION: u8 = 0;
const CHROMADESTINATION: u8 = 1;
const LUMAID: u8 = 1;
const CHROMABLUEID: u8 = 2;
const CHROMAREDID: u8 = 3;

#[rustfmt::skip]
static UNZIGZAG: [u8; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

#[derive(Copy, Clone)]
struct Component {
    id: u8,
    h: u8,
    v: u8,
    tq: u8,
    dc_table: u8,
    ac_table: u8,
}

struct BitWriter<W> {
    w: W,
    accumulator: u32,
    nbits: u8,
}

impl<W: Write> BitWriter<W> {
    fn new(w: W) -> Self {
        BitWriter { w, accumulator: 0, nbits: 0 }
    }

    fn write_bits(&mut self, bits: u16, size: u8) -> io::Result<()> {
        if size == 0 {
            return Ok(());
        }
        self.nbits += size;
        self.accumulator |= u32::from(bits) << (32 - self.nbits) as usize;
        while self.nbits >= 8 {
            let byte = self.accumulator >> 24;
            self.w.write_all(&[byte as u8])?;
            if byte == 0xFF {
                // Byte stuffing: 0xFF nos dados vira 0xFF 0x00.
                self.w.write_all(&[0x00])?;
            }
            self.nbits -= 8;
            self.accumulator <<= 8;
        }
        Ok(())
    }

    fn pad_byte(&mut self) -> io::Result<()> {
        self.write_bits(0x7F, 7)
    }

    fn huffman_encode(&mut self, val: u8, table: &[(u8, u16); 256]) -> io::Result<()> {
        let (size, code) = table[val as usize];
        assert!(size <= 16, "bad huffman value");
        self.write_bits(code, size)
    }

    fn write_block(
        &mut self,
        block: &[i32; 64],
        prevdc: i32,
        dctable: &[(u8, u16); 256],
        actable: &[(u8, u16); 256],
    ) -> io::Result<i32> {
        // Codificação diferencial do DC.
        let dcval = block[0];
        let diff = dcval - prevdc;
        let (size, value) = encode_coefficient(diff);
        self.huffman_encode(size, dctable)?;
        self.write_bits(value, size)?;

        // Figura F.2 do padrão: run-length de zeros + coeficiente.
        let mut zero_run = 0;
        for &k in &UNZIGZAG[1..] {
            if block[k as usize] == 0 {
                zero_run += 1;
            } else {
                while zero_run > 15 {
                    self.huffman_encode(0xF0, actable)?;
                    zero_run -= 16;
                }
                let (size, value) = encode_coefficient(block[k as usize]);
                let symbol = (zero_run << 4) | size;
                self.huffman_encode(symbol, actable)?;
                self.write_bits(value, size)?;
                zero_run = 0;
            }
        }
        if block[UNZIGZAG[63] as usize] == 0 {
            self.huffman_encode(0x00, actable)?;
        }
        Ok(dcval)
    }

    fn write_marker(&mut self, marker: u8) -> io::Result<()> {
        self.w.write_all(&[0xFF, marker])
    }

    fn write_segment(&mut self, marker: u8, data: &[u8]) -> io::Result<()> {
        self.w.write_all(&[0xFF, marker])?;
        self.w.write_all(&(data.len() as u16 + 2).to_be_bytes())?;
        self.w.write_all(data)
    }
}

fn encode_coefficient(coefficient: i32) -> (u8, u16) {
    let mut magnitude = coefficient.unsigned_abs() as u16;
    let mut num_bits = 0u8;
    while magnitude > 0 {
        magnitude >>= 1;
        num_bits += 1;
    }
    let mask = (1 << num_bits as usize) - 1;
    let val = if coefficient < 0 {
        (coefficient - 1) as u16 & mask
    } else {
        coefficient as u16 & mask
    };
    (num_bits, val)
}

/// RGB → YCbCr (BT.601 full range) em ponto fixo, arredondamento ao vizinho.
#[inline]
fn rgb_to_ycbcr(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));

    const C_YR: i32 = 19595; // 0.29900 * 2^16
    const C_YG: i32 = 38469; // 0.58700 * 2^16
    const C_YB: i32 = 7471; // 0.11400 * 2^16
    const Y_ROUNDING: i32 = (1 << 15) - 1;
    const C_UR: i32 = 11059; // 0.16874 * 2^16
    const C_UG: i32 = 21709; // 0.33126 * 2^16
    const C_UB: i32 = 32768; // 0.50000 * 2^16
    const UV_BIAS_ROUNDING: i32 = (128 * (1 << 16)) + ((1 << 15) - 1);
    const C_VR: i32 = C_UB;
    const C_VG: i32 = 27439; // 0.41869 * 2^16
    const C_VB: i32 = 5329; // 0.08131 * 2^16

    let y = (C_YR * r + C_YG * g + C_YB * b + Y_ROUNDING) >> 16;
    let cb = (-C_UR * r - C_UG * g + C_UB * b + UV_BIAS_ROUNDING) >> 16;
    let cr = (C_VR * r - C_VG * g - C_VB * b + UV_BIAS_ROUNDING) >> 16;
    (y as u8, cb as u8, cr as u8)
}

/// Copia um bloco 8×8 começando em `(x0, y0)` convertendo para YCbCr;
/// pixels fora da imagem repetem o pixel de borda mais próximo.
#[allow(clippy::too_many_arguments)]
fn copy_blocks_ycbcr(
    rgb: &[u8],
    width: u32,
    height: u32,
    x0: u32,
    y0: u32,
    yb: &mut [u8; 64],
    cbb: &mut [u8; 64],
    crb: &mut [u8; 64],
) {
    for y in 0..8u32 {
        let sy = (y0 + y).min(height - 1) as usize;
        for x in 0..8u32 {
            let sx = (x0 + x).min(width - 1) as usize;
            let i = (sy * width as usize + sx) * 3;
            let (yc, cb, cr) = rgb_to_ycbcr(rgb[i], rgb[i + 1], rgb[i + 2]);
            yb[(y * 8 + x) as usize] = yc;
            cbb[(y * 8 + x) as usize] = cb;
            crb[(y * 8 + x) as usize] = cr;
        }
    }
}

/// Codifica `rgb` (RGB8, `width × height`) como JPEG baseline no `writer`.
/// `quality` segue a escala 1–100 do libjpeg.
pub fn encode_rgb<W: Write>(
    writer: W,
    rgb: &[u8],
    width: u32,
    height: u32,
    quality: u8,
) -> io::Result<()> {
    assert_eq!(
        (width as usize) * (height as usize) * 3,
        rgb.len(),
        "buffer RGB com tamanho inconsistente"
    );
    let (Ok(w16 @ 1..), Ok(h16 @ 1..)) = (u16::try_from(width), u16::try_from(height)) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "dimensões fora do suportado pelo JPEG baseline (1–65535)",
        ));
    };

    let components = [
        Component {
            id: LUMAID,
            h: 1,
            v: 1,
            tq: LUMADESTINATION,
            dc_table: LUMADESTINATION,
            ac_table: LUMADESTINATION,
        },
        Component {
            id: CHROMABLUEID,
            h: 1,
            v: 1,
            tq: CHROMADESTINATION,
            dc_table: CHROMADESTINATION,
            ac_table: CHROMADESTINATION,
        },
        Component {
            id: CHROMAREDID,
            h: 1,
            v: 1,
            tq: CHROMADESTINATION,
            dc_table: CHROMADESTINATION,
            ac_table: CHROMADESTINATION,
        },
    ];

    // Escala das tabelas de quantização, algoritmo do libjpeg.
    let scale = u32::from(quality.clamp(1, 100));
    let scale = if scale < 50 { 5000 / scale } else { 200 - scale * 2 };
    let mut tables = [STD_LUMA_QTABLE, STD_CHROMA_QTABLE];
    for t in tables.iter_mut() {
        for v in t.iter_mut() {
            *v = ((u32::from(*v) * scale + 50) / 100).clamp(1, 255) as u8;
        }
    }

    let mut w = BitWriter::new(writer);
    w.write_marker(SOI)?;

    let mut buf = Vec::new();

    // JFIF 1.2, proporção de pixel 1:1, sem DPI.
    buf.extend_from_slice(b"JFIF");
    buf.extend_from_slice(&[0, 0x01, 0x02, 0x00]);
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&[0, 0]);
    w.write_segment(APP0, &buf)?;

    build_frame_header(&mut buf, 8, w16, h16, &components);
    w.write_segment(SOF0, &buf)?;

    for (i, table) in tables.iter().enumerate() {
        build_quantization_segment(&mut buf, 8, i as u8, table);
        w.write_segment(DQT, &buf)?;
    }

    for (class, destination, lengths, values) in [
        (DCCLASS, LUMADESTINATION, &STD_LUMA_DC_CODE_LENGTHS, &STD_LUMA_DC_VALUES[..]),
        (ACCLASS, LUMADESTINATION, &STD_LUMA_AC_CODE_LENGTHS, &STD_LUMA_AC_VALUES[..]),
        (DCCLASS, CHROMADESTINATION, &STD_CHROMA_DC_CODE_LENGTHS, &STD_CHROMA_DC_VALUES[..]),
        (ACCLASS, CHROMADESTINATION, &STD_CHROMA_AC_CODE_LENGTHS, &STD_CHROMA_AC_VALUES[..]),
    ] {
        build_huffman_segment(&mut buf, class, destination, lengths, values);
        w.write_segment(DHT, &buf)?;
    }

    build_scan_header(&mut buf, &components);
    w.write_segment(SOS, &buf)?;

    // Varredura dos blocos 8×8 (4:4:4 — sem subamostragem).
    let mut y_dcprev = 0;
    let mut cb_dcprev = 0;
    let mut cr_dcprev = 0;
    let mut yblock = [0u8; 64];
    let mut cb_block = [0u8; 64];
    let mut cr_block = [0u8; 64];
    let mut dct_yblock = [0i32; 64];
    let mut dct_cb_block = [0i32; 64];
    let mut dct_cr_block = [0i32; 64];

    for y0 in (0..height).step_by(8) {
        for x0 in (0..width).step_by(8) {
            copy_blocks_ycbcr(
                rgb, width, height, x0, y0, &mut yblock, &mut cb_block, &mut cr_block,
            );

            // Level shift + FDCT (coeficientes escalados por 8).
            transform::fdct(&yblock, &mut dct_yblock);
            transform::fdct(&cb_block, &mut dct_cb_block);
            transform::fdct(&cr_block, &mut dct_cr_block);

            for i in 0usize..64 {
                dct_yblock[i] =
                    ((dct_yblock[i] / 8) as f32 / f32::from(tables[0][i])).round() as i32;
                dct_cb_block[i] =
                    ((dct_cb_block[i] / 8) as f32 / f32::from(tables[1][i])).round() as i32;
                dct_cr_block[i] =
                    ((dct_cr_block[i] / 8) as f32 / f32::from(tables[1][i])).round() as i32;
            }

            y_dcprev = w.write_block(&dct_yblock, y_dcprev, &STD_LUMA_DC_HUFF_LUT, &STD_LUMA_AC_HUFF_LUT)?;
            cb_dcprev = w.write_block(&dct_cb_block, cb_dcprev, &STD_CHROMA_DC_HUFF_LUT, &STD_CHROMA_AC_HUFF_LUT)?;
            cr_dcprev = w.write_block(&dct_cr_block, cr_dcprev, &STD_CHROMA_DC_HUFF_LUT, &STD_CHROMA_AC_HUFF_LUT)?;
        }
    }

    w.pad_byte()?;
    w.write_marker(EOI)?;
    Ok(())
}

fn build_frame_header(m: &mut Vec<u8>, precision: u8, width: u16, height: u16, components: &[Component]) {
    m.clear();
    m.push(precision);
    m.extend_from_slice(&height.to_be_bytes());
    m.extend_from_slice(&width.to_be_bytes());
    m.push(components.len() as u8);
    for &comp in components {
        let hv = (comp.h << 4) | comp.v;
        m.extend_from_slice(&[comp.id, hv, comp.tq]);
    }
}

fn build_scan_header(m: &mut Vec<u8>, components: &[Component]) {
    m.clear();
    m.push(components.len() as u8);
    for &comp in components {
        let tables = (comp.dc_table << 4) | comp.ac_table;
        m.extend_from_slice(&[comp.id, tables]);
    }
    // Início/fim espectral e aproximação alta/baixa.
    m.extend_from_slice(&[0, 63, 0]);
}

fn build_huffman_segment(m: &mut Vec<u8>, class: u8, destination: u8, numcodes: &[u8; 16], values: &[u8]) {
    m.clear();
    m.push((class << 4) | destination);
    m.extend_from_slice(numcodes);
    debug_assert_eq!(numcodes.iter().map(|&x| x as usize).sum::<usize>(), values.len());
    m.extend_from_slice(values);
}

fn build_quantization_segment(m: &mut Vec<u8>, precision: u8, identifier: u8, qtable: &[u8; 64]) {
    m.clear();
    let p = if precision == 8 { 0 } else { 1 };
    m.push((p << 4) | identifier);
    for &i in &UNZIGZAG[..] {
        m.push(qtable[i as usize]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodificação mínima da estrutura de segmentos para validar o stream.
    fn segments(data: &[u8]) -> Vec<u8> {
        let mut markers = Vec::new();
        let mut i = 0;
        while i + 1 < data.len() {
            assert_eq!(data[i], 0xFF, "esperava marcador em {i}");
            let marker = data[i + 1];
            markers.push(marker);
            i += 2;
            match marker {
                0xD8 => {}
                0xD9 => break,
                0xDA => {
                    // Scan: segue até o EOI (0xFF 0xD9), com byte stuffing.
                    let len = u16::from_be_bytes([data[i], data[i + 1]]) as usize;
                    i += len;
                    while i + 1 < data.len() && !(data[i] == 0xFF && data[i + 1] == 0xD9) {
                        i += 1;
                    }
                }
                _ => {
                    let len = u16::from_be_bytes([data[i], data[i + 1]]) as usize;
                    i += len;
                }
            }
        }
        markers
    }

    #[test]
    fn stream_structure_is_valid() {
        let (w, h) = (17u32, 9u32); // não múltiplo de 8: exercita o clamp de borda
        let rgb: Vec<u8> = (0..w * h)
            .flat_map(|i| [(i % 251) as u8, (i * 7 % 251) as u8, (i * 13 % 251) as u8])
            .collect();
        let mut out = Vec::new();
        encode_rgb(&mut out, &rgb, w, h, 90).unwrap();

        assert_eq!(&out[..2], &[0xFF, 0xD8], "SOI");
        assert_eq!(&out[out.len() - 2..], &[0xFF, 0xD9], "EOI");
        let markers = segments(&out);
        // SOI, APP0, SOF0, 2×DQT, 4×DHT, SOS, EOI.
        assert_eq!(markers[0], 0xD8);
        assert_eq!(markers[1], 0xE0);
        assert!(markers.contains(&0xC0));
        assert_eq!(markers.iter().filter(|&&m| m == 0xDB).count(), 2);
        assert_eq!(markers.iter().filter(|&&m| m == 0xC4).count(), 4);
        assert_eq!(*markers.last().unwrap(), 0xD9);
    }

    #[test]
    fn sof_dimensions_match() {
        let (w, h) = (32u32, 16u32);
        let rgb = vec![128u8; (w * h * 3) as usize];
        let mut out = Vec::new();
        encode_rgb(&mut out, &rgb, w, h, 90).unwrap();

        // Localiza o SOF0 e confere altura/largura big-endian.
        let pos = out.windows(2).position(|p| p == [0xFF, 0xC0]).unwrap();
        let seg = &out[pos + 4..];
        assert_eq!(seg[0], 8, "precisão");
        assert_eq!(u16::from_be_bytes([seg[1], seg[2]]), h as u16);
        assert_eq!(u16::from_be_bytes([seg[3], seg[4]]), w as u16);
    }

    #[test]
    fn rejects_wrong_buffer_size() {
        let mut out = Vec::new();
        let result = std::panic::catch_unwind(move || {
            encode_rgb(&mut out, &[0u8; 10], 4, 4, 90).unwrap();
        });
        assert!(result.is_err());
    }
}
