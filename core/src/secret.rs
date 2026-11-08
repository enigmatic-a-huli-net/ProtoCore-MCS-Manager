//! Шифрование секретов в config.json (токены Telegram/Discord, ключ CurseForge).
//!
//! На Windows — DPAPI: расшифровать может только та же учётная запись Windows на том же ПК.
//! Если папку менеджера переслать или скопировать на другой компьютер, секреты
//! там не расшифруются и их нужно ввести заново — так и задумано.
//! На других системах значения хранятся как есть.

use base64::Engine;

const PREFIX: &str = "dpapi:";

/// Зашифровать для записи на диск. Пустую строку не трогаем.
pub fn protect(plain: &str) -> String {
    if plain.is_empty() || plain.starts_with(PREFIX) {
        return plain.to_string();
    }
    match imp::protect(plain.as_bytes()) {
        Some(b) => format!("{PREFIX}{}", base64::engine::general_purpose::STANDARD.encode(b)),
        None => plain.to_string(),
    }
}

/// Расшифровать прочитанное с диска. Старые незашифрованные значения возвращаются как есть;
/// то, что не расшифровалось (другой ПК или пользователь), — пустая строка.
pub fn unprotect(stored: &str) -> String {
    let Some(b64) = stored.strip_prefix(PREFIX) else { return stored.to_string() };
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .ok()
        .and_then(|b| imp::unprotect(&b))
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default()
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};

    const ENTROPY: &[u8] = b"ProtoCore-MCS-Manager";

    fn run(data: &[u8], encrypt: bool) -> Option<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let entropy = CRYPT_INTEGER_BLOB { cbData: ENTROPY.len() as u32, pbData: ENTROPY.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        let ok = unsafe {
            if encrypt {
                CryptProtectData(&input, std::ptr::null(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
            } else {
                CryptUnprotectData(&input, std::ptr::null_mut(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
            }
        };
        if ok == 0 || out.pbData.is_null() {
            return None;
        }
        let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
        unsafe { LocalFree(out.pbData as _) };
        Some(v)
    }

    pub fn protect(d: &[u8]) -> Option<Vec<u8>> {
        run(d, true)
    }
    pub fn unprotect(d: &[u8]) -> Option<Vec<u8>> {
        run(d, false)
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn protect(_: &[u8]) -> Option<Vec<u8>> {
        None
    }
    pub fn unprotect(_: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_legacy() {
        assert_eq!(unprotect("plain-old-token"), "plain-old-token");
        assert_eq!(protect(""), "");
        let p = protect("123:ABC");
        assert_eq!(unprotect(&p), "123:ABC");
        // чужой/битый шифротекст не роняет программу
        assert_eq!(unprotect("dpapi:@@@"), "");
    }
}
