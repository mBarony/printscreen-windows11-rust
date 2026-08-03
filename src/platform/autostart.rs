//! "Iniciar com o Windows" via `HKCU\...\Run` (substitui `auto-launch`).

use crate::error::Result;

#[cfg(windows)]
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

/// Grava (`enabled`) ou remove a entrada `name` apontando para `command`
/// (caminho já entre aspas — evita o clássico unquoted-path).
#[cfg(windows)]
pub fn set(name: &str, command: &str, enabled: bool) -> Result<()> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        REG_SZ,
    };

    let key_path = super::wide(RUN_KEY);
    let value_name = super::wide(name);
    let mut key: HKEY = std::ptr::null_mut();

    // SAFETY: strings NUL-terminadas; a chave aberta é fechada ao final.
    unsafe {
        let status = RegCreateKeyW(HKEY_CURRENT_USER, key_path.as_ptr(), &mut key);
        if status != 0 {
            return Err(crate::error::err!("abrindo chave Run (código {status})"));
        }

        let status = if enabled {
            let data = super::wide(command);
            RegSetValueExW(
                key,
                value_name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                (data.len() * 2) as u32,
            )
        } else {
            let status = RegDeleteValueW(key, value_name.as_ptr());
            // Remover algo que não existe não é erro.
            if status == windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND {
                0
            } else {
                status
            }
        };
        RegCloseKey(key);
        if status != 0 {
            return Err(crate::error::err!(
                "gravando valor na chave Run (código {status})"
            ));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set(_name: &str, _command: &str, _enabled: bool) -> Result<()> {
    Ok(())
}
