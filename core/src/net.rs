//! Сеть: JSON-запросы и скачивание файлов с проверкой контрольной суммы.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use serde_json::Value;

use crate::{Error, Result};

/// User-Agent с контактом — его требуют PaperMC и Modrinth.
pub const UA: &str = concat!(
    "ProtoCore-MCS-Manager/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/enigmatic-a-huli-net/ProtoCore-MCS-Manager)"
);

fn map_err(url: &str, e: ureq::Error) -> Error {
    match e {
        ureq::Error::Status(code, r) => {
            let body = r.into_string().unwrap_or_default();
            let short: String = body.chars().take(200).collect();
            Error::Network(format!("{url}: HTTP {code} {short}"))
        }
        other => Error::Network(format!("{url}: {other}")),
    }
}

pub fn get_json(url: &str) -> Result<Value> {
    get_json_h(url, &[])
}

pub fn get_json_h(url: &str, headers: &[(&str, &str)]) -> Result<Value> {
    let mut req = crate::util::http().get(url).set("User-Agent", UA).set("Accept", "application/json");
    for (k, v) in headers {
        req = req.set(k, v);
    }
    req.call()
        .map_err(|e| map_err(url, e))?
        .into_json()
        .map_err(|e| Error::Network(format!("{url}: {e}")))
}

pub fn post_json(url: &str, body: &Value) -> Result<Value> {
    crate::util::http()
        .post(url)
        .set("User-Agent", UA)
        .send_json(body.clone())
        .map_err(|e| map_err(url, e))?
        .into_json()
        .map_err(|e| Error::Network(format!("{url}: {e}")))
}

pub fn get_text(url: &str) -> Result<String> {
    crate::util::http()
        .get(url)
        .set("User-Agent", UA)
        .call()
        .map_err(|e| map_err(url, e))?
        .into_string()
        .map_err(|e| Error::Network(e.to_string()))
}

/// SHA-1 файла (Modrinth ищет моды по нему).
pub fn sha1_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = sha1_smol::Sha1::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.digest().to_string())
}

/// Скачивает файл в `dst` через временный `.part`. Если указан sha1 — проверяет.
/// `on_progress(скачано, всего)`; всего = 0, если сервер не сообщил размер.
pub fn download(url: &str, dst: &Path, sha1: Option<&str>, mut on_progress: impl FnMut(u64, u64)) -> Result<()> {
    if let Some(p) = dst.parent() {
        fs::create_dir_all(p)?;
    }
    let resp = crate::util::http()
        .get(url)
        .set("User-Agent", UA)
        .call()
        .map_err(|e| map_err(url, e))?;
    let total: u64 = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut reader = resp.into_reader().take(2 * 1024 * 1024 * 1024);
    let tmp = dst.with_extension("part");
    let mut out = fs::File::create(&tmp)?;
    let mut h = sha1_smol::Sha1::new();
    let mut buf = vec![0u8; 128 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        h.update(&buf[..n]);
        done += n as u64;
        on_progress(done, total);
    }
    out.flush()?;
    drop(out);
    if let Some(expect) = sha1 {
        let got = h.digest().to_string();
        if !got.eq_ignore_ascii_case(expect) {
            let _ = fs::remove_file(&tmp);
            return Err(Error::Network(format!("файл скачался повреждённым (sha1 {got} ≠ {expect})")));
        }
    }
    if total > 0 && done < total {
        let _ = fs::remove_file(&tmp);
        return Err(Error::Network("загрузка оборвалась".into()));
    }
    fs::rename(&tmp, dst)?;
    Ok(())
}

/// Безопасное имя файла из URL / API.
pub fn safe_file_name(name: &str) -> String {
    let n: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    n.trim().trim_start_matches('.').to_string()
}
