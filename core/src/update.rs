//! Обновление самого менеджера через GitHub Releases.
//!
//! 1. `check` — смотрит последний релиз репозитория и сравнивает версию.
//! 2. `download` — качает `ProtoCore-MCS-Manager-portable.zip` и достаёт из него новый .exe
//!    рядом со старым (`ProtoCore-MCS-Manager.new.exe`).
//! 3. `apply` — запускает новый .exe в режиме замены: он ждёт закрытия старого,
//!    копирует себя на его место и запускает. Настройки и снапшоты не трогаются.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::config::AppPaths;
use crate::detect::version_cmp;
use crate::{Error, Result};

pub const DEFAULT_REPO: &str = "enigmatic-a-huli-net/ProtoCore-MCS-Manager";
const ASSET_NAME: &str = "ProtoCore-MCS-Manager-portable.zip";
const SIG_NAME: &str = "ProtoCore-MCS-Manager-portable.zip.sig";

/// Публичный ключ подписи релизов (Ed25519). Закрытый ключ есть только в секретах GitHub Actions.
/// Обновление без верной подписи не устанавливается.
pub const UPDATE_PUBKEY: &str = "5cca0b6b2c3bbfe30028f37854642fc2f663b2701992969d3156539bb3d0a9c0";

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

/// Проверка подписи файла обновления.
pub fn verify(data: &[u8], sig_hex: &str, pubkey_hex: &str) -> Result<()> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let bad = || Error::Network("подпись обновления не совпала — файл подменён или повреждён, установка отменена".into());
    let pk: [u8; 32] = unhex(pubkey_hex).and_then(|v| v.try_into().ok()).ok_or_else(bad)?;
    let sig: [u8; 64] = unhex(sig_hex).and_then(|v| v.try_into().ok()).ok_or_else(bad)?;
    let key = VerifyingKey::from_bytes(&pk).map_err(|_| bad())?;
    key.verify(data, &Signature::from_bytes(&sig)).map_err(|_| bad())
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub available: bool,
    pub current: String,
    pub latest: String,
    pub notes: String,
    pub published: String,
    pub html_url: String,
    pub asset_url: String,
    pub asset_size: u64,
    pub sig_url: String,
}

fn get(url: &str) -> Result<ureq::Response> {
    crate::util::http().get(url)
        .set("User-Agent", "ProtoCore-MCS-Manager")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(404, _) => Error::Network(
                "релизов не найдено (репозиторий приватный или ещё нет ни одного релиза)".into(),
            ),
            ureq::Error::Status(403, _) => Error::Network("GitHub временно ограничил запросы, попробуй позже".into()),
            other => Error::Network(other.to_string()),
        })
}

pub fn check(repo: &str, current: &str) -> Result<UpdateInfo> {
    let rel: Value = get(&format!("https://api.github.com/repos/{repo}/releases/latest"))?
        .into_json()
        .map_err(|e| Error::Network(e.to_string()))?;
    let tag = rel.get("tag_name").and_then(Value::as_str).unwrap_or("").to_string();
    let latest = tag.trim_start_matches('v').to_string();
    let asset = rel
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|a| a.iter().find(|x| x.get("name").and_then(Value::as_str) == Some(ASSET_NAME)));
    Ok(UpdateInfo {
        available: !latest.is_empty() && version_cmp(&latest, current) == std::cmp::Ordering::Greater,
        current: current.to_string(),
        latest,
        notes: rel.get("body").and_then(Value::as_str).unwrap_or("").to_string(),
        published: rel.get("published_at").and_then(Value::as_str).unwrap_or("").to_string(),
        html_url: rel.get("html_url").and_then(Value::as_str).unwrap_or("").to_string(),
        asset_url: asset
            .and_then(|a| a.get("browser_download_url").and_then(Value::as_str))
            .unwrap_or("")
            .to_string(),
        asset_size: asset.and_then(|a| a.get("size").and_then(Value::as_u64)).unwrap_or(0),
        sig_url: rel
            .get("assets")
            .and_then(Value::as_array)
            .and_then(|a| a.iter().find(|x| x.get("name").and_then(Value::as_str) == Some(SIG_NAME)))
            .and_then(|a| a.get("browser_download_url").and_then(Value::as_str))
            .unwrap_or("")
            .to_string(),
    })
}

fn exe_path() -> Result<PathBuf> {
    std::env::current_exe().map_err(Error::from)
}

fn staged_path(exe: &Path) -> PathBuf {
    exe.with_extension("new.exe")
}

/// Скачивает обновление. `on_progress(скачано, всего)`.
pub fn download(info: &UpdateInfo, mut on_progress: impl FnMut(u64, u64)) -> Result<PathBuf> {
    if info.asset_url.is_empty() {
        return Err(Error::Network(format!("в релизе нет файла {ASSET_NAME}")));
    }
    let mut reader = get(&info.asset_url)?.into_reader().take(300 * 1024 * 1024);
    let mut bytes = Vec::with_capacity(info.asset_size as usize);
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        on_progress(bytes.len() as u64, info.asset_size);
    }
    // подпись: без неё или с неверной — не ставим
    if info.sig_url.is_empty() {
        return Err(Error::Network("у релиза нет подписи — установка отменена. Скачай версию вручную со страницы релиза".into()));
    }
    let mut sig = String::new();
    get(&info.sig_url)?.into_reader().take(4096).read_to_string(&mut sig)?;
    verify(&bytes, &sig, UPDATE_PUBKEY)?;

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let idx = (0..zip.len())
        .find(|&i| {
            zip.by_index(i)
                .map(|f| f.name().to_lowercase().ends_with("protocore-mcs-manager.exe"))
                .unwrap_or(false)
        })
        .ok_or_else(|| Error::Network("в архиве нет ProtoCore-MCS-Manager.exe".into()))?;
    let exe = exe_path()?;
    let dst = staged_path(&exe);
    let mut f = zip.by_index(idx)?;
    let mut out = fs::File::create(&dst)?;
    std::io::copy(&mut f, &mut out)?;
    Ok(dst)
}

/// Аргумент, с которым запускается новая версия, чтобы встать на место старой.
pub const FINISH_ARG: &str = "--pcm-finish-update";

/// Запускает скачанную новую версию в режиме «замены». После вызова менеджер должен закрыться.
///
/// Никаких .cmd и окон консоли: новый exe сам ждёт, пока старый закроется
/// (файл перестанет быть занятым), копирует себя на его место и запускает его.
pub fn apply(_paths: &AppPaths) -> Result<()> {
    let exe = exe_path()?;
    let staged = staged_path(&exe);
    if !staged.is_file() {
        return Err(Error::other("обновление не скачано"));
    }
    let mut c = std::process::Command::new(&staged);
    c.arg(FINISH_ARG).arg(&exe);
    c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    c.spawn().map_err(|e| Error::other(format!("не удалось запустить новую версию: {e}")))?;
    Ok(())
}

/// Режим замены (вызывается из main, если передан FINISH_ARG).
/// Ждёт до 5 минут, пока старый менеджер остановит серверы и закроется,
/// затем копирует себя поверх старого exe и запускает его.
pub fn finish(target: &Path) -> Result<()> {
    let me = exe_path()?;
    let t0 = std::time::Instant::now();
    loop {
        // пока старый exe запущен, Windows не даст его перезаписать
        match fs::copy(&me, target) {
            Ok(_) => break,
            Err(e) => {
                if t0.elapsed() > std::time::Duration::from_secs(300) {
                    return Err(Error::other(format!("не удалось заменить {}: {e}", target.display())));
                }
                std::thread::sleep(std::time::Duration::from_millis(700));
            }
        }
    }
    std::process::Command::new(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

/// Удаляет недокачанное/оставшееся обновление (при старте).
/// Удаляет файл новой версии после успешной замены. Он может ещё пару секунд
/// работать (он только что нас запустил), поэтому пробуем несколько раз в фоне.
pub fn cleanup() {
    let Ok(exe) = exe_path() else { return };
    let staged = staged_path(&exe);
    if !staged.exists() {
        return;
    }
    std::thread::spawn(move || {
        for _ in 0..30 {
            if fs::remove_file(&staged).is_ok() || !staged.exists() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
    // старый скрипт обновления из версий до 0.3.2
    if let Some(dir) = exe.parent() {
        let _ = fs::remove_file(dir.join("data").join("pcm-update.cmd"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn signature_check() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let pk: String = key.verifying_key().as_bytes().iter().map(|b| format!("{b:02x}")).collect();
        let data = b"update zip bytes";
        let sig: String = key.sign(data).to_bytes().iter().map(|b| format!("{b:02x}")).collect();
        assert!(verify(data, &sig, &pk).is_ok());
        assert!(verify(b"tampered", &sig, &pk).is_err());
        assert!(verify(data, "00", &pk).is_err());
        assert!(verify(data, &sig, UPDATE_PUBKEY).is_err());
        assert_eq!(UPDATE_PUBKEY.len(), 64);
    }
}
