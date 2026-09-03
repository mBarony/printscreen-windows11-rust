//! Registro do esquema `rustshot://` em `HKCU\Software\Classes`.
//!
//! Fica no ramo do usuário, e não no da máquina, pelo mesmo motivo que o
//! resto do estado mora ao lado do executável: a aplicação é portátil e não
//! pede elevação para nada. O custo é o esquema valer só para quem registrou,
//! que é exatamente o alcance que ele precisa ter.
//!
//! Registrar aponta para o caminho **atual** do executável. Mover a pasta
//! quebra o esquema até alguém registrar de novo — é o preço de ser portátil,
//! e o mesmo que já vale para "Iniciar com o Windows".

// Fora do Windows o módulo existe como stub, para o resto do código
// compilar; ninguém o chama.
#![cfg_attr(not(windows), allow(dead_code))]

use crate::error::Result;

/// Nome do esquema, sem `://`.
pub const SCHEME: &str = "rustshot";

#[cfg(windows)]
const CLASSES: &str = "Software\\Classes";

/// Registra (`enabled`) ou remove o esquema apontando para `exe`.
#[cfg(windows)]
pub fn set(exe: &std::path::Path, enabled: bool) -> Result<()> {
    use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        REG_SZ,
    };

    let raiz = format!("{CLASSES}\\{SCHEME}");
    if !enabled {
        let caminho = super::wide(&raiz);
        // SAFETY: string NUL-terminada; remover o que não existe não é erro.
        let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, caminho.as_ptr()) };
        if status != 0 && status != ERROR_FILE_NOT_FOUND {
            return Err(crate::error::err!(
                "removendo o esquema {SCHEME} (código {status})"
            ));
        }
        return Ok(());
    }

    // O valor padrão da chave é a descrição; `URL Protocol` (vazio) é o que
    // diz ao shell que isto é um esquema, e não um tipo de arquivo.
    let comando = format!("\"{}\" \"%1\"", exe.display());
    let valores: [(&str, &str, &str); 3] = [
        (&raiz, "", "URL:Captura de tela RustShot"),
        (&raiz, "URL Protocol", ""),
        (&format!("{raiz}\\shell\\open\\command"), "", &comando),
    ];

    for (chave, nome, dado) in valores {
        let caminho = super::wide(chave);
        let nome_w = super::wide(nome);
        let dado_w = super::wide(dado);
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: strings NUL-terminadas; a chave é fechada em todo caminho.
        unsafe {
            let status = RegCreateKeyW(HKEY_CURRENT_USER, caminho.as_ptr(), &mut key);
            if status != 0 {
                return Err(crate::error::err!("abrindo {chave} (código {status})"));
            }
            let status = RegSetValueExW(
                key,
                nome_w.as_ptr(),
                0,
                REG_SZ,
                dado_w.as_ptr() as *const u8,
                (dado_w.len() * 2) as u32,
            );
            RegCloseKey(key);
            if status != 0 {
                return Err(crate::error::err!("gravando {chave} (código {status})"));
            }
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set(_exe: &std::path::Path, _enabled: bool) -> Result<()> {
    Ok(())
}

/// O esquema está registrado e apontando para este executável?
///
/// Apontar para **outro** executável conta como não registrado: é o caso de
/// uma cópia antiga da pasta, e o usuário precisa ver a caixa desmarcada para
/// poder consertar clicando nela.
#[cfg(windows)]
pub fn is_registered(exe: &std::path::Path) -> bool {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    };

    let caminho = super::wide(&format!("{CLASSES}\\{SCHEME}\\shell\\open\\command"));
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: string NUL-terminada; a chave é fechada antes de retornar.
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, caminho.as_ptr(), 0, KEY_READ, &mut key) != 0 {
            return false;
        }
        let mut tamanho: u32 = 0;
        let ok = RegQueryValueExW(
            key,
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut tamanho,
        ) == 0;
        let mut buffer = vec![0u16; (tamanho as usize / 2) + 1];
        let lido = ok
            && RegQueryValueExW(
                key,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                buffer.as_mut_ptr() as *mut u8,
                &mut tamanho,
            ) == 0;
        RegCloseKey(key);
        if !lido {
            return false;
        }
        let fim = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        let valor = String::from_utf16_lossy(&buffer[..fim]);
        valor.contains(&exe.display().to_string())
    }
}

#[cfg(not(windows))]
pub fn is_registered(_exe: &std::path::Path) -> bool {
    false
}
