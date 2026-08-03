//! Logger em arquivo (substitui `simplelog`): timestamp local, filtro de
//! módulos gráficos ruidosos e escrita serializada.

use std::fs::File;
use std::io::Write as _;
use std::sync::Mutex;

use crate::platform::time;

/// Alvos filtrados do log — o log é do app, não do renderizador (o naga
/// chega a despejar código de shader em nível Info).
const IGNORED_TARGET_PREFIXES: &[&str] = &["naga", "wgpu", "egui_wgpu"];

struct FileLogger {
    file: Mutex<File>,
    level: log::LevelFilter,
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= self.level
            && !IGNORED_TARGET_PREFIXES
                .iter()
                .any(|prefix| metadata.target().starts_with(prefix))
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} [{}] ({}) {}\n",
            time::now().timestamp(),
            record.level(),
            record.target(),
            record.args()
        );
        if let Ok(mut file) = self.file.lock() {
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

/// Instala o logger global gravando em `file`. Chamar uma única vez.
pub fn init(file: File, level: log::LevelFilter) {
    let logger = Box::new(FileLogger { file: Mutex::new(file), level });
    if log::set_boxed_logger(logger).is_ok() {
        log::set_max_level(level);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log as _;

    #[test]
    fn filters_noisy_graphics_targets() {
        let path = std::env::temp_dir().join(format!("rustshot-log-{}", std::process::id()));
        let logger = FileLogger {
            file: Mutex::new(File::create(&path).unwrap()),
            level: log::LevelFilter::Info,
        };
        let noisy = log::Metadata::builder()
            .level(log::Level::Info)
            .target("naga::front")
            .build();
        let ours = log::Metadata::builder()
            .level(log::Level::Info)
            .target("rustshot::app")
            .build();
        assert!(!logger.enabled(&noisy));
        assert!(logger.enabled(&ours));
        let _ = std::fs::remove_file(&path);
    }
}
