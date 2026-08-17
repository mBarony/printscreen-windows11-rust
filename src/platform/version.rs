//! Versão do Windows em execução, para o app recusar sistemas anteriores ao
//! alvo (build 22000 = Windows 11 21H2).
//!
//! A leitura usa `GetVersionExW`, que só é confiável porque o manifesto embutido
//! declara `supportedOS` do Windows 10/11: sem esse GUID o Windows aplica a
//! camada de compatibilidade e reporta 6.2 para qualquer sistema moderno. Para
//! nunca barrar um Windows 11 por causa disso, a regra reprova apenas o que é
//! inequivocamente um Windows 10 — qualquer outra resposta, inclusive um shim de
//! compatibilidade ou uma versão futura, passa.

/// Primeira build do Windows 11 (21H2).
#[cfg(any(windows, test))]
const WINDOWS_11_BUILD: u32 = 22_000;

/// `true` se o sistema atende ao alvo (ou se não deu para saber).
#[cfg(windows)]
pub fn is_supported() -> bool {
    current().is_none_or(|(major, build)| supported(major, build))
}

#[cfg(not(windows))]
pub fn is_supported() -> bool {
    true
}

/// A regra em si: reprova só `major == 10` com build anterior à do Windows 11.
#[cfg(any(windows, test))]
fn supported(major: u32, build: u32) -> bool {
    major != 10 || build >= WINDOWS_11_BUILD
}

/// `(major, build)` do sistema, ou `None` se a consulta falhar.
#[cfg(windows)]
fn current() -> Option<(u32, u32)> {
    use windows_sys::Win32::System::SystemInformation::{GetVersionExW, OSVERSIONINFOW};

    // SAFETY: OSVERSIONINFOW é composto só de inteiros e de um array de u16 —
    // tudo zero é um valor válido; `dwOSVersionInfoSize` é preenchido como a API
    // exige e `GetVersionExW` recebe um ponteiro para a struct viva.
    unsafe {
        let mut info: OSVERSIONINFOW = std::mem::zeroed();
        info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
        let ok = GetVersionExW(&mut info);
        (ok != 0).then_some((info.dwMajorVersion, info.dwBuildNumber))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_real_windows_10_is_rejected() {
        assert!(!supported(10, 19045), "Windows 10 22H2");
        assert!(!supported(10, 21996), "pré-release anterior ao 21H2");
        assert!(supported(10, WINDOWS_11_BUILD), "Windows 11 21H2");
        assert!(supported(10, 26100), "Windows 11 24H2");
        // Sem o supportedOS no manifesto o Windows reporta 6.2 para tudo:
        // reprovar aqui barraria justamente o sistema que queremos suportar.
        assert!(supported(6, 9200), "shim de compatibilidade");
        // Versão futura hipotética: passa por padrão.
        assert!(supported(11, 30000));
    }
}
