//! Transporte das capturas do processo residente para o processo de GUI.
//!
//! O residente é quem captura (BitBlt é Win32 puro, sem GUI): assim a tela
//! continua congelada no instante do atalho, e não ~300 ms depois, quando o
//! processo de GUI terminaria de subir. Os pixels vão para um mapeamento de
//! memória nomeado, cujo nome o residente passa ao filho na linha de comando.
//!
//! O nome fica no namespace `Local\` (a sessão do usuário, nunca `Global\`) e
//! carrega pid e um contador. Não é segredo criptográfico: outro processo do
//! mesmo usuário que soubesse o nome leria a captura — mas esse processo já
//! poderia capturar a tela ele mesmo, então o modelo de ameaça não muda.
//!
//! O residente mantém o handle aberto até o filho encerrar; fechá-lo antes
//! destruiria o objeto (ninguém mais o referencia) e o filho abriria o vazio.

use crate::capture::MonitorShot;
use crate::error::{err, Result};
use crate::imgbuf::RgbaImage;

/// `RSS1` — identifica o layout abaixo e barra lixo de versão trocada.
const MAGIC: u32 = 0x3153_5352;
/// magic + contagem.
const HEADER_BYTES: usize = 8;
/// x, y, largura, altura, escala, offset dos pixels.
const ENTRY_BYTES: usize = 4 + 4 + 4 + 4 + 4 + 8;

/// Geometria de um monitor no bloco compartilhado (pixels à parte).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotEntry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    /// Deslocamento dos pixels RGBA a partir do início do bloco.
    pub offset: u64,
}

impl ShotEntry {
    fn pixel_bytes(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height) * 4
    }
}

/// Monta o bloco inteiro (cabeçalho + pixels) a ser copiado para o mapeamento.
pub fn encode(shots: &[MonitorShot]) -> Vec<u8> {
    let pixels_start = HEADER_BYTES + shots.len() * ENTRY_BYTES;
    let total: usize =
        pixels_start + shots.iter().map(|s| s.image.as_raw().len()).sum::<usize>();

    let mut buf = Vec::with_capacity(total);
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.extend_from_slice(&(shots.len() as u32).to_le_bytes());

    let mut offset = pixels_start as u64;
    for shot in shots {
        buf.extend_from_slice(&shot.x.to_le_bytes());
        buf.extend_from_slice(&shot.y.to_le_bytes());
        buf.extend_from_slice(&shot.width.to_le_bytes());
        buf.extend_from_slice(&shot.height.to_le_bytes());
        buf.extend_from_slice(&shot.scale.to_le_bytes());
        buf.extend_from_slice(&offset.to_le_bytes());
        offset += shot.image.as_raw().len() as u64;
    }
    for shot in shots {
        buf.extend_from_slice(shot.image.as_raw());
    }
    buf
}

/// Lê o cabeçalho e valida que cada faixa de pixels cabe em `len` bytes.
pub fn decode_header(buf: &[u8], len: usize) -> Result<Vec<ShotEntry>> {
    if buf.len() < HEADER_BYTES {
        return Err(err!("bloco compartilhado truncado"));
    }
    if u32::from_le_bytes(buf[0..4].try_into().unwrap()) != MAGIC {
        return Err(err!("bloco compartilhado com assinatura inválida"));
    }
    let count = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
    if count == 0 {
        return Err(err!("bloco compartilhado sem monitores"));
    }
    if buf.len() < HEADER_BYTES + count * ENTRY_BYTES {
        return Err(err!("cabeçalho truncado para {count} monitores"));
    }

    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let at = HEADER_BYTES + i * ENTRY_BYTES;
        let field = |off: usize| -> [u8; 4] { buf[at + off..at + off + 4].try_into().unwrap() };
        let entry = ShotEntry {
            x: i32::from_le_bytes(field(0)),
            y: i32::from_le_bytes(field(4)),
            width: u32::from_le_bytes(field(8)),
            height: u32::from_le_bytes(field(12)),
            scale: f32::from_le_bytes(field(16)),
            offset: u64::from_le_bytes(buf[at + 20..at + 28].try_into().unwrap()),
        };
        if entry.width == 0 || entry.height == 0 {
            return Err(err!("monitor {i} com dimensão zero"));
        }
        // O offset vem de outro processo: sem esta checagem uma leitura
        // adiante indexaria fora do mapeamento.
        let end = entry
            .offset
            .checked_add(entry.pixel_bytes())
            .ok_or_else(|| err!("monitor {i} com offset inválido"))?;
        if end > len as u64 {
            return Err(err!("monitor {i} aponta para fora do bloco"));
        }
        entries.push(entry);
    }
    Ok(entries)
}

/// Reconstrói as capturas a partir do bloco já validado por `decode_header`.
pub fn decode(buf: &[u8], entries: &[ShotEntry]) -> Vec<MonitorShot> {
    entries
        .iter()
        .map(|e| {
            let start = e.offset as usize;
            let end = start + e.pixel_bytes() as usize;
            MonitorShot {
                x: e.x,
                y: e.y,
                width: e.width,
                height: e.height,
                scale: e.scale,
                image: RgbaImage::from_raw(e.width, e.height, buf[start..end].to_vec()),
            }
        })
        .collect()
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ,
        FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
    };

    /// Mapeamento vivo no residente: fecha ao ser descartado, o que só pode
    /// acontecer depois de o processo de GUI encerrar.
    pub struct SharedShots {
        name: String,
        handle: HANDLE,
        len: usize,
    }

    // SAFETY: um HANDLE de mapeamento é válido em qualquer thread do processo;
    // esta struct só o fecha (no Drop) e nunca o duplica.
    unsafe impl Send for SharedShots {}

    impl SharedShots {
        /// Nome a passar para o filho, junto do tamanho.
        pub fn name(&self) -> &str {
            &self.name
        }

        pub fn len(&self) -> usize {
            self.len
        }
    }

    impl Drop for SharedShots {
        fn drop(&mut self) {
            // SAFETY: handle obtido de CreateFileMappingW e fechado uma vez.
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }

    /// Cria o mapeamento e copia as capturas para dentro dele.
    pub fn publish(shots: &[MonitorShot]) -> Result<SharedShots> {
        let bytes = encode(shots);
        let len = bytes.len();
        let name = next_name();
        let wide_name = crate::platform::wide(&name);

        // SAFETY: cria um mapeamento anônimo em memória (INVALID_HANDLE_VALUE)
        // do tamanho exato, mapeia a view, copia `bytes` e desfaz a view. O
        // handle continua vivo em SharedShots.
        unsafe {
            let handle = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                std::ptr::null(),
                PAGE_READWRITE,
                (len >> 32) as u32,
                len as u32,
                wide_name.as_ptr(),
            );
            if handle.is_null() {
                return Err(err!("CreateFileMappingW falhou"));
            }
            let view = MapViewOfFile(handle, FILE_MAP_WRITE, 0, 0, len);
            if view.Value.is_null() {
                CloseHandle(handle);
                return Err(err!("MapViewOfFile falhou"));
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), view.Value.cast::<u8>(), len);
            UnmapViewOfFile(view);
            Ok(SharedShots { name, handle, len })
        }
    }

    /// Abre o mapeamento publicado pelo residente e devolve as capturas.
    pub fn consume(name: &str, len: usize) -> Result<Vec<MonitorShot>> {
        let wide_name = crate::platform::wide(name);
        // SAFETY: abre o mapeamento por nome, mapeia `len` bytes para leitura e
        // desfaz a view antes de retornar. Todo acesso ao slice acontece com a
        // view viva, e o cabeçalho é validado antes de qualquer leitura de
        // pixels.
        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_READ, 0, wide_name.as_ptr());
            if handle.is_null() {
                return Err(err!("OpenFileMappingW falhou para {name}"));
            }
            let view = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, len);
            if view.Value.is_null() {
                CloseHandle(handle);
                return Err(err!("MapViewOfFile falhou para {name}"));
            }
            let result = read_view(view, len);
            UnmapViewOfFile(view);
            CloseHandle(handle);
            result
        }
    }

    unsafe fn read_view(view: MEMORY_MAPPED_VIEW_ADDRESS, len: usize) -> Result<Vec<MonitorShot>> {
        let buf = std::slice::from_raw_parts(view.Value.cast::<u8>(), len);
        let entries = decode_header(buf, len)?;
        Ok(decode(buf, &entries))
    }

    /// `Local\rustshot-shots-<pid>-<n>`: por sessão, e único dentro do processo.
    fn next_name() -> String {
        static COUNTER: AtomicU32 = AtomicU32::new(1);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("Local\\rustshot-shots-{}-{n}", std::process::id())
    }
}

#[cfg(windows)]
pub use imp::{consume, publish, SharedShots};

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(x: i32, y: i32, w: u32, h: u32, fill: u8) -> MonitorShot {
        MonitorShot {
            x,
            y,
            width: w,
            height: h,
            scale: 1.5,
            image: RgbaImage::filled(w, h, [fill, fill, fill, 255]),
        }
    }

    #[test]
    fn roundtrip_preserves_geometry_and_pixels() {
        let shots = vec![shot(-1920, 0, 4, 3, 10), shot(0, 0, 2, 2, 20)];
        let buf = encode(&shots);
        let entries = decode_header(&buf, buf.len()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].x, -1920, "origem negativa sobrevive");
        assert_eq!(entries[1].scale, 1.5);

        let decoded = decode(&buf, &entries);
        assert_eq!((decoded[0].width, decoded[0].height), (4, 3));
        assert_eq!(decoded[0].image.pixel(0, 0), [10, 10, 10, 255]);
        assert_eq!(decoded[1].image.pixel(1, 1), [20, 20, 20, 255]);
    }

    #[test]
    fn rejects_foreign_or_corrupt_blocks() {
        let buf = encode(&[shot(0, 0, 2, 2, 1)]);

        assert!(decode_header(&buf[..4], 4).is_err(), "truncado");
        assert!(decode_header(b"XXXX\0\0\0\0", 8).is_err(), "magic errado");

        // Comprimento menor do que o cabeçalho promete: os pixels ficariam
        // fora do mapeamento.
        assert!(decode_header(&buf, buf.len() - 1).is_err(), "offset fora do bloco");

        let mut zero_count = buf.clone();
        zero_count[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert!(decode_header(&zero_count, zero_count.len()).is_err(), "sem monitores");
    }
}
