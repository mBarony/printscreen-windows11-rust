//! Nomeação (template, colisões), codificação e escrita das capturas
//! (RF-07), com fallback de pasta e notificações (§14).
//!
//! A codificação/escrita roda em thread de trabalho (`save_in_background`)
//! para nunca bloquear a UI.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::{Context as _, Result};
use crate::imgbuf::RgbaImage;
use crate::notify;
use crate::platform::time;

/// Snapshot dos campos de configuração de que o salvamento precisa —
/// congelado no momento da captura para não depender de mutações posteriores.
#[derive(Clone)]
pub struct SaveTarget {
    pub output_dir: PathBuf,
    pub filename_template: String,
    pub image_format: crate::imgout::Format,
}

impl SaveTarget {
    pub fn from_config(config: &Config) -> Self {
        Self {
            output_dir: config.effective_output_dir(),
            filename_template: config.filename_template.clone(),
            image_format: config.image_format,
        }
    }
}

/// Garante a existência da pasta de destino; em falha usa `Imagens\RustShot`
/// como fallback e notifica (RF-05/§14). Retorna a pasta efetiva.
pub fn ensure_output_dir(target: &SaveTarget) -> Result<PathBuf> {
    let wanted = &target.output_dir;
    match std::fs::create_dir_all(wanted) {
        Ok(()) => Ok(wanted.clone()),
        Err(err) => {
            let fallback = crate::config::default_output_dir();
            log::warn!(
                "pasta configurada {} inacessível ({err}); usando {}",
                wanted.display(),
                fallback.display()
            );
            std::fs::create_dir_all(&fallback)
                .with_context(|| format!("criando pasta de fallback {}", fallback.display()))?;
            notify::toast(
                "Pasta de capturas indisponível",
                &format!(
                    "Não foi possível usar {}. Salvando em {}.",
                    wanted.display(),
                    fallback.display()
                ),
            );
            Ok(fallback)
        }
    }
}

/// Expande o template (`{date}`, `{time}`) em um stem saneado, sem extensão
/// (o formato é fixo, então `shot.jpg`/`nome.png` digitados no template não
/// devem virar `shot.jpg.jpg`).
fn expand_stem(template: &str) -> String {
    let now = time::now();
    let date = now.date();
    let time = now.time();
    let stem = sanitize_filename(
        &template
            .replace("{date}", &date)
            .replace("{time}", &time),
    );
    let mut stem = if stem.is_empty() { format!("screenshot_{date}_{time}") } else { stem };
    let lower = stem.to_ascii_lowercase();
    for ext in [".jpg", ".jpeg", ".png"] {
        if lower.ends_with(ext) && stem.len() > ext.len() {
            stem.truncate(stem.len() - ext.len());
            break;
        }
    }
    stem
}

/// Expande o template e resolve colisões com `_1`, `_2`… (produção usa a
/// variante atômica `claim_free_path`; esta permanece para os testes).
#[cfg(test)]
pub fn next_free_path(dir: &Path, template: &str, extension: &str) -> PathBuf {
    let stem = expand_stem(template);
    let candidate = dir.join(format!("{stem}.{extension}"));
    if !candidate.exists() {
        return candidate;
    }
    for n in 1u32.. {
        let candidate = dir.join(format!("{stem}_{n}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("sempre há um sufixo livre");
}

/// Reserva atomicamente o próximo caminho livre com `create_new`: duas
/// capturas no mesmo segundo (saves em threads paralelas) recebem arquivos
/// distintos em vez de a segunda truncar a primeira.
fn claim_free_path(
    dir: &Path,
    template: &str,
    extension: &str,
) -> Result<(PathBuf, std::fs::File)> {
    let stem = expand_stem(template);
    let mut n = 0u32;
    loop {
        let name = if n == 0 {
            format!("{stem}.{extension}")
        } else {
            format!("{stem}_{n}.{extension}")
        };
        let candidate = dir.join(name);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
            Err(err) => {
                return Err(err).with_context(|| format!("criando {}", candidate.display()))
            }
        }
    }
}

/// Remove caracteres inválidos em nomes de arquivo do Windows.
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            c if (c as u32) < 0x20 => '-',
            c => c,
        })
        .collect();
    cleaned.trim().trim_end_matches('.').to_string()
}

/// Codifica e grava a imagem; retorna o caminho final.
///
/// O formato sai do `config.json`, e com `auto` é decidido por imagem — daí
/// a extensão ser resolvida **antes** de reservar o caminho: reservar como
/// `.jpg` e gravar um PNG dentro deixaria o arquivo mentindo sobre si.
pub fn write_image(target: &SaveTarget, image: &RgbaImage) -> Result<PathBuf> {
    let dir = ensure_output_dir(target)?;
    let format = crate::imgout::resolve(target.image_format, image);
    let extension = match format {
        crate::imgout::Format::Png => "png",
        _ => "jpg",
    };
    let (path, file) = claim_free_path(&dir, &target.filename_template, extension)?;
    let writer = std::io::BufWriter::new(file);
    crate::imgout::encode(writer, image, format)
        .with_context(|| format!("gravando {}", path.display()))?;
    Ok(path)
}

/// Grava os quadros como um GIF que alterna entre eles; devolve o caminho.
///
/// A extensão não passa pela escolha automática de formato: um GIF é um GIF,
/// e o `image_format` do config diz respeito às capturas paradas.
pub fn write_gif(target: &SaveTarget, frames: &[&RgbaImage], delay_cs: u16) -> Result<PathBuf> {
    let dir = ensure_output_dir(target)?;
    let (path, file) = claim_free_path(&dir, &target.filename_template, "gif")?;
    let writer = std::io::BufWriter::new(file);
    crate::gif::encode(writer, frames, delay_cs)
        .with_context(|| format!("gravando {}", path.display()))?;
    Ok(path)
}

/// Salva em thread de trabalho e notifica o resultado (toast RF-07/§14). Via
/// `jobs`, para o processo não encerrar no meio da gravação.
pub fn save_in_background(target: SaveTarget, image: RgbaImage) {
    crate::jobs::spawn(move || match write_image(&target, &image) {
        Ok(path) => {
            log::info!("captura salva em {}", path.display());
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            notify::toast("Captura salva", &name);
        }
        Err(err) => {
            notify::toast_error("Falha ao salvar captura", &format!("{err}"));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_windows_reserved_chars() {
        assert_eq!(sanitize_filename("shot: 12/08*?"), "shot- 12-08--");
        assert_eq!(sanitize_filename("  name. "), "name");
    }

    #[test]
    fn template_with_extension_does_not_duplicate() {
        let dir = std::env::temp_dir().join(format!("rustshot-ext-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = next_free_path(&dir, "shot.jpg", "jpg");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "shot.jpg");
        let path = next_free_path(&dir, "nome.PNG", "jpg");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "nome.jpg");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn collision_suffixes() {
        let dir = std::env::temp_dir().join(format!("rustshot-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = next_free_path(&dir, "fixed", "jpg");
        std::fs::write(&first, b"x").unwrap();
        let second = next_free_path(&dir, "fixed", "jpg");
        assert_eq!(second.file_name().unwrap().to_str().unwrap(), "fixed_1.jpg");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn claim_is_atomic_and_sequential() {
        let dir = std::env::temp_dir().join(format!("rustshot-claim-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (first, f1) = claim_free_path(&dir, "fixed", "jpg").unwrap();
        let (second, f2) = claim_free_path(&dir, "fixed", "jpg").unwrap();
        drop((f1, f2));
        assert_eq!(first.file_name().unwrap().to_str().unwrap(), "fixed.jpg");
        assert_eq!(second.file_name().unwrap().to_str().unwrap(), "fixed_1.jpg");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn target_para(dir: &Path, format: crate::imgout::Format) -> SaveTarget {
        SaveTarget {
            output_dir: dir.to_path_buf(),
            filename_template: "sample".into(),
            image_format: format,
        }
    }

    #[test]
    fn write_image_produces_valid_jpeg() {
        let dir = std::env::temp_dir().join(format!("rustshot-jpg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = target_para(&dir, crate::imgout::Format::Jpg);
        let image = RgbaImage::filled(40, 24, [200, 40, 40, 255]);
        let path = write_image(&target, &image).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF], "assinatura JPEG");
        assert_eq!(&bytes[bytes.len() - 2..], &[0xFF, 0xD9], "EOI");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_image_em_png_usa_a_extensao_certa() {
        // O arquivo não pode mentir sobre si: PNG dentro, .png fora.
        let dir = std::env::temp_dir().join(format!("rustshot-png-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = target_para(&dir, crate::imgout::Format::Png);
        let image = RgbaImage::filled(40, 24, [200, 40, 40, 255]);
        let path = write_image(&target, &image).unwrap();
        assert_eq!(path.extension().unwrap(), "png");
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "assinatura PNG");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_image_no_automatico_escolhe_png_para_cor_chapada() {
        // Uma imagem de cor única é o extremo de "poucas cores": PNG.
        let dir = std::env::temp_dir().join(format!("rustshot-auto-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = target_para(&dir, crate::imgout::Format::Auto);
        let image = RgbaImage::filled(40, 24, [10, 120, 200, 255]);
        let path = write_image(&target, &image).unwrap();
        assert_eq!(path.extension().unwrap(), "png");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
