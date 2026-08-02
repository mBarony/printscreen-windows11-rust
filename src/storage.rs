//! Nomeação (template, colisões), codificação e escrita das capturas
//! (RF-07), com fallback de pasta e notificações (§14).
//!
//! A codificação/escrita roda em thread de trabalho (`save_in_background`)
//! para nunca bloquear a UI.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use image::RgbaImage;

use crate::config::{Config, ImageFormat};
use crate::notify;

/// Snapshot dos campos de configuração de que o salvamento precisa —
/// congelado no momento da captura para não depender de mutações posteriores.
#[derive(Clone)]
pub struct SaveTarget {
    pub output_dir: PathBuf,
    pub filename_template: String,
    pub format: ImageFormat,
}

impl SaveTarget {
    pub fn from_config(config: &Config) -> Self {
        Self {
            output_dir: config.effective_output_dir(),
            filename_template: config.filename_template.clone(),
            format: config.image_format,
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

/// Expande o template (`{date}`, `{time}`) e resolve colisões com `_1`, `_2`…
pub fn next_free_path(dir: &Path, template: &str, extension: &str) -> PathBuf {
    let now = chrono::Local::now();
    let date = now.format("%Y-%m-%d").to_string();
    let time = now.format("%H-%M-%S").to_string();
    let stem = sanitize_filename(
        &template
            .replace("{date}", &date)
            .replace("{time}", &time),
    );
    let stem = if stem.is_empty() { format!("screenshot_{date}_{time}") } else { stem };

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
pub fn write_image(target: &SaveTarget, image: &RgbaImage) -> Result<PathBuf> {
    let dir = ensure_output_dir(target)?;
    let path = next_free_path(&dir, &target.filename_template, target.format.extension());

    match target.format {
        ImageFormat::Png => {
            image
                .save_with_format(&path, image::ImageFormat::Png)
                .with_context(|| format!("gravando {}", path.display()))?;
        }
        ImageFormat::Jpg => {
            // JPG não tem alfa: converte para RGB antes de codificar.
            let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
            let file = std::fs::File::create(&path)
                .with_context(|| format!("criando {}", path.display()))?;
            let mut writer = std::io::BufWriter::new(file);
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 90);
            rgb.write_with_encoder(encoder)
                .with_context(|| format!("gravando {}", path.display()))?;
        }
    }
    Ok(path)
}

/// Salva em thread de trabalho e notifica o resultado (toast RF-07/§14).
pub fn save_in_background(target: SaveTarget, image: RgbaImage) {
    std::thread::spawn(move || match write_image(&target, &image) {
        Ok(path) => {
            log::info!("captura salva em {}", path.display());
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            notify::toast("Captura salva", &name);
        }
        Err(err) => {
            notify::toast_error("Falha ao salvar captura", &format!("{err:#}"));
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
    fn collision_suffixes() {
        let dir = std::env::temp_dir().join(format!("rustshot-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = next_free_path(&dir, "fixed", "png");
        std::fs::write(&first, b"x").unwrap();
        let second = next_free_path(&dir, "fixed", "png");
        assert_eq!(second.file_name().unwrap().to_str().unwrap(), "fixed_1.png");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
