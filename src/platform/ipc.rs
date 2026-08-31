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
use crate::platform::window_list::WindowTarget;

/// `RSS2` — identifica o layout abaixo e barra lixo de versão trocada. O
/// `RSS1` não levava a lista de janelas.
const MAGIC: u32 = 0x3253_5352;
/// magic + contagem de monitores + contagem de janelas.
const HEADER_BYTES: usize = 12;
/// x, y, largura, altura, escala, offset dos pixels.
const ENTRY_BYTES: usize = 4 + 4 + 4 + 4 + 4 + 8;
/// x, y, largura, altura, offset do título, bytes do título.
const WINDOW_BYTES: usize = 4 + 4 + 4 + 4 + 8 + 4;

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

/// Monta o bloco inteiro (cabeçalho + pixels + janelas) a ser copiado para o
/// mapeamento.
///
/// As janelas viajam junto porque precisam ser enumeradas no **mesmo
/// instante** da captura: se o processo de GUI as enumerasse ao subir, uns
/// 300 ms depois, uma janela movida nesse intervalo apareceria no lugar
/// errado sobre os pixels congelados.
/// O bloco como `Vec`. Só os testes de ida-e-volta usam: em produção o
/// `publish` escreve direto na memória mapeada, para não copiar tudo duas vezes.
#[cfg(test)]
pub fn encode(shots: &[MonitorShot], windows: &[WindowTarget]) -> Vec<u8> {
    let mut buf = vec![0u8; tamanho(shots, windows).2];
    escreve(shots, windows, &mut buf);
    buf
}

/// Onde começam os pixels, onde começam os títulos, e o total — as três somas
/// que o bloco precisa antes de existir.
///
/// Separado de `escreve` para o `publish` poder criar o mapeamento do tamanho
/// certo e escrever direto dentro dele: montar um `Vec` antes custaria uma
/// segunda cópia de tudo, e em dois monitores 4K "tudo" são ~66 MB — bem no
/// meio da latência entre apertar o atalho e o overlay aparecer.
fn tamanho(shots: &[MonitorShot], windows: &[WindowTarget]) -> (usize, usize, usize) {
    let pixels_start = HEADER_BYTES + shots.len() * ENTRY_BYTES + windows.len() * WINDOW_BYTES;
    let titles_start = pixels_start + shots.iter().map(|s| s.image.as_raw().len()).sum::<usize>();
    let total = titles_start + windows.iter().map(|w| w.title.len()).sum::<usize>();
    (pixels_start, titles_start, total)
}

/// Escreve o bloco em `dst`, que tem de ter exatamente o tamanho que `tamanho`
/// devolve. Menos que isso entra em pânico no primeiro `copy_from_slice` — o
/// que é a resposta certa: um bloco curto seria lido do outro lado como pixels.
fn escreve(shots: &[MonitorShot], windows: &[WindowTarget], dst: &mut [u8]) {
    let (pixels_start, titles_start, total) = tamanho(shots, windows);
    debug_assert_eq!(dst.len(), total);

    let mut escritor = Escritor { dst, at: 0 };
    escritor.put(&MAGIC.to_le_bytes());
    escritor.put(&(shots.len() as u32).to_le_bytes());
    escritor.put(&(windows.len() as u32).to_le_bytes());

    let mut offset = pixels_start as u64;
    for shot in shots {
        escritor.put(&shot.x.to_le_bytes());
        escritor.put(&shot.y.to_le_bytes());
        escritor.put(&shot.width.to_le_bytes());
        escritor.put(&shot.height.to_le_bytes());
        escritor.put(&shot.scale.to_le_bytes());
        escritor.put(&offset.to_le_bytes());
        offset += shot.image.as_raw().len() as u64;
    }
    let mut title_at = titles_start as u64;
    for window in windows {
        escritor.put(&window.x.to_le_bytes());
        escritor.put(&window.y.to_le_bytes());
        escritor.put(&window.width.to_le_bytes());
        escritor.put(&window.height.to_le_bytes());
        escritor.put(&title_at.to_le_bytes());
        escritor.put(&(window.title.len() as u32).to_le_bytes());
        title_at += window.title.len() as u64;
    }
    for shot in shots {
        escritor.put(shot.image.as_raw());
    }
    for window in windows {
        escritor.put(window.title.as_bytes());
    }
}

/// Cursor de escrita sequencial sobre um buffer de tamanho já conhecido.
struct Escritor<'a> {
    dst: &'a mut [u8],
    at: usize,
}

impl Escritor<'_> {
    fn put(&mut self, bytes: &[u8]) {
        self.dst[self.at..self.at + bytes.len()].copy_from_slice(bytes);
        self.at += bytes.len();
    }
}

/// Lê a lista de janelas. Um bloco com lista inválida devolve lista vazia —
/// a captura por janela deixa de funcionar, mas a captura de região, que é o
/// principal, continua de pé.
pub fn decode_windows(buf: &[u8], shot_count: usize) -> Vec<WindowTarget> {
    if buf.len() < HEADER_BYTES {
        return Vec::new();
    }
    let count = u32::from_le_bytes(buf[8..12].try_into().unwrap()) as usize;
    let base = HEADER_BYTES + shot_count * ENTRY_BYTES;
    if count == 0 || buf.len() < base + count * WINDOW_BYTES {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let at = base + i * WINDOW_BYTES;
        let field = |off: usize| -> [u8; 4] { buf[at + off..at + off + 4].try_into().unwrap() };
        let title_at = u64::from_le_bytes(buf[at + 16..at + 24].try_into().unwrap()) as usize;
        let title_len = u32::from_le_bytes(field(24)) as usize;
        // O offset vem de outro processo: sem esta checagem a fatia
        // indexaria fora do mapeamento.
        let Some(end) = title_at.checked_add(title_len).filter(|e| *e <= buf.len()) else {
            return Vec::new();
        };
        out.push(WindowTarget {
            x: i32::from_le_bytes(field(0)),
            y: i32::from_le_bytes(field(4)),
            width: u32::from_le_bytes(field(8)),
            height: u32::from_le_bytes(field(12)),
            title: String::from_utf8_lossy(&buf[title_at..end]).into_owned(),
        });
    }
    out
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
    let windows = u32::from_le_bytes(buf[8..12].try_into().unwrap()) as usize;
    if buf.len() < HEADER_BYTES + count * ENTRY_BYTES + windows * WINDOW_BYTES {
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

    /// Cria o mapeamento e escreve as capturas dentro dele.
    ///
    /// A escrita vai **direto na view**, sem `Vec` intermediário: montar o
    /// bloco antes custaria uma cópia a mais da captura inteira, e é neste
    /// ponto — entre o BitBlt e o processo de GUI nascer — que o usuário está
    /// esperando o overlay aparecer.
    pub fn publish(shots: &[MonitorShot], windows: &[WindowTarget]) -> Result<SharedShots> {
        let len = super::tamanho(shots, windows).2;
        let name = next_name();
        let wide_name = crate::platform::wide(&name);

        // SAFETY: cria um mapeamento anônimo em memória (INVALID_HANDLE_VALUE)
        // do tamanho exato, mapeia a view, escreve e desfaz a view. O slice
        // cobre exatamente os `len` bytes que o `CreateFileMappingW` reservou e
        // o `MapViewOfFile` mapeou, e `escreve` grava exatamente esse tanto —
        // é o mesmo `tamanho` que dimensionou os três. O handle continua vivo
        // em SharedShots.
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
            let destino = std::slice::from_raw_parts_mut(view.Value.cast::<u8>(), len);
            super::escreve(shots, windows, destino);
            UnmapViewOfFile(view);
            Ok(SharedShots { name, handle, len })
        }
    }

    /// Abre o mapeamento publicado pelo residente e devolve as capturas.
    pub fn consume(name: &str, len: usize) -> Result<(Vec<MonitorShot>, Vec<WindowTarget>)> {
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

    unsafe fn read_view(
        view: MEMORY_MAPPED_VIEW_ADDRESS,
        len: usize,
    ) -> Result<(Vec<MonitorShot>, Vec<WindowTarget>)> {
        let buf = std::slice::from_raw_parts(view.Value.cast::<u8>(), len);
        let entries = decode_header(buf, len)?;
        let windows = decode_windows(buf, entries.len());
        Ok((decode(buf, &entries), windows))
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
        let buf = encode(&shots, &[]);
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
        let buf = encode(&[shot(0, 0, 2, 2, 1)], &[]);

        assert!(decode_header(&buf[..4], 4).is_err(), "truncado");
        assert!(decode_header(b"XXXX\0\0\0\0", 8).is_err(), "magic errado");

        // Comprimento menor do que o cabeçalho promete: os pixels ficariam
        // fora do mapeamento.
        assert!(decode_header(&buf, buf.len() - 1).is_err(), "offset fora do bloco");

        let mut zero_count = buf.clone();
        zero_count[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert!(decode_header(&zero_count, zero_count.len()).is_err(), "sem monitores");
    }

    #[test]
    fn windows_survive_the_round_trip_with_their_titles() {
        let shots = vec![shot(0, 0, 2, 2, 1)];
        let windows = vec![
            WindowTarget { x: 10, y: 20, width: 300, height: 200, title: "Bloco de notas".into() },
            WindowTarget { x: -5, y: 0, width: 800, height: 600, title: "Café ☕".into() },
        ];
        let buf = encode(&shots, &windows);
        let entries = decode_header(&buf, buf.len()).unwrap();
        assert_eq!(decode_windows(&buf, entries.len()), windows);
    }

    #[test]
    fn a_block_without_windows_decodes_to_an_empty_list() {
        let buf = encode(&[shot(0, 0, 2, 2, 1)], &[]);
        let entries = decode_header(&buf, buf.len()).unwrap();
        assert!(decode_windows(&buf, entries.len()).is_empty());
    }

    #[test]
    fn a_title_pointing_outside_the_block_is_refused() {
        // O offset vem de outro processo: seguir um ponteiro inválido leria
        // fora do mapeamento.
        let windows =
            vec![WindowTarget { x: 0, y: 0, width: 10, height: 10, title: "x".into() }];
        let mut buf = encode(&[shot(0, 0, 2, 2, 1)], &windows);
        let entries = decode_header(&buf, buf.len()).unwrap();
        let at = HEADER_BYTES + entries.len() * ENTRY_BYTES + 16;
        buf[at..at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode_windows(&buf, entries.len()).is_empty());
    }
}
