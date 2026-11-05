//! Обновление самого менеджера через GitHub Releases.
//!
//! 1. `check` — смотрит последний релиз репозитория и сравнивает версию.
//! 2. `download` — качает `ProtoCore-MCS-Manager-portable.zip` и достаёт из него новый .exe
//!    рядом со старым (`ProtoCore-MCS-Manager.new.exe`).
//! 3. `apply` — запускает маленький .cmd, который ждёт закрытия менеджера,
//!    подменяет .exe и запускает новую версию. Настройки и снапшоты не трогаются.

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

/// Готовит и запускает скрипт замены. После вызова менеджер должен закрыться.
pub fn apply(paths: &AppPaths) -> Result<()> {
    let exe = exe_path()?;
    let staged = staged_path(&exe);
    if !staged.is_file() {
        return Err(Error::other("обновление не скачано"));
    }
    if !cfg!(windows) {
        return Err(Error::other("автообновление работает только в Windows"));
    }
    paths.ensure()?;
    let script = paths.data.join("pcm-update.cmd");
    let pid = std::process::id();
    let body = format!(
        "@echo off\r\n\
         chcp 65001 >nul\r\n\
         :wait\r\n\
         tasklist /FI \"PID eq {pid}\" 2>nul | find \"{pid}\" >nul && (timeout /t 1 /nobreak >nul & goto wait)\r\n\
         set tries=0\r\n\
         :swap\r\n\
         move /Y \"{new}\" \"{cur}\" >nul 2>&1\r\n\
         if errorlevel 1 (set /a tries+=1 & if %tries% lss 20 (timeout /t 1 /nobreak >nul & goto swap))\r\n\
         start \"\" \"{cur}\"\r\n\
         del \"%~f0\"\r\n",
        new = staged.display(),
        cur = exe.display(),
    );
    fs::write(&script, body)?;
    let mut c = std::process::Command::new("cmd");
    c.args(["/C", &script.to_string_lossy()]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // без окна и отдельно от нашего процесса
        c.creation_flags(0x0800_0000 | 0x0000_0008);
    }
    c.spawn()?;
    Ok(())
}

/// Удаляет недокачанное/оставшееся обновление (при старте).
pub fn cleanup() {
    if let Ok(exe) = exe_path() {
        let _ = fs::remove_file(staged_path(&exe));
    }
}
