//! Логи сервера: поиск по всем файлам (включая сжатые .log.gz),
//! сжатие старых логов и краш-репортов, удаление совсем старых.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::Serialize;

use crate::Result;

const DIRS: [&str; 2] = ["logs", "crash-reports"];

#[derive(Debug, Clone, Serialize)]
pub struct LogFile {
    /// путь относительно папки сервера
    pub rel: String,
    pub size: u64,
    /// unix-секунды
    pub mtime: i64,
    pub gz: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LogsInfo {
    pub files: Vec<LogFile>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub rel: String,
    pub line: usize,
    pub text: String,
}

fn mtime(p: &Path) -> i64 {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn list(server_dir: &Path) -> LogsInfo {
    let mut files = Vec::new();
    for d in DIRS {
        let Ok(rd) = fs::read_dir(server_dir.join(d)) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let n = e.file_name().to_string_lossy().to_string();
            if !(n.ends_with(".log") || n.ends_with(".gz") || n.ends_with(".txt")) {
                continue;
            }
            files.push(LogFile { rel: format!("{d}/{n}"), size: e.metadata().map(|m| m.len()).unwrap_or(0), mtime: mtime(&p), gz: n.ends_with(".gz") });
        }
    }
    files.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    let total = files.iter().map(|f| f.size).sum();
    LogsInfo { files, total }
}

fn reader(p: &Path) -> Result<Box<dyn BufRead>> {
    let f = fs::File::open(p)?;
    Ok(if p.extension().is_some_and(|x| x == "gz") {
        Box::new(BufReader::new(GzDecoder::new(f)))
    } else {
        Box::new(BufReader::new(f))
    })
}

/// Поиск подстроки (без учёта регистра) по всем логам, от новых к старым.
pub fn search(server_dir: &Path, query: &str, max: usize) -> Vec<Hit> {
    let q = query.trim().to_lowercase();
    let mut out = Vec::new();
    if q.is_empty() {
        return out;
    }
    for f in list(server_dir).files {
        let Ok(r) = reader(&server_dir.join(&f.rel)) else { continue };
        for (i, line) in r.split(b'\n').enumerate() {
            let Ok(bytes) = line else { break };
            let text = String::from_utf8_lossy(&bytes);
            if text.to_lowercase().contains(&q) {
                out.push(Hit { rel: f.rel.clone(), line: i + 1, text: text.trim_end().chars().take(400).collect() });
                if out.len() >= max {
                    return out;
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CleanResult {
    pub compressed: usize,
    pub deleted: usize,
    pub freed: u64,
}

fn gzip(p: &Path) -> Result<u64> {
    let before = fs::metadata(p)?.len();
    let dst = PathBuf::from(format!("{}.gz", p.display()));
    let mut enc = GzEncoder::new(fs::File::create(&dst)?, Compression::default());
    let mut src = fs::File::open(p)?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = src.read(&mut buf)?;
        if n == 0 {
            break;
        }
        enc.write_all(&buf[..n])?;
    }
    enc.finish()?;
    let after = fs::metadata(&dst)?.len();
    fs::remove_file(p)?;
    Ok(before.saturating_sub(after))
}

/// Сжать несжатые старые логи (кроме текущих latest.log / debug.log)
/// и удалить сжатые старше `delete_days` дней (0 — не удалять).
pub fn clean(server_dir: &Path, delete_days: u32) -> Result<CleanResult> {
    let mut r = CleanResult::default();
    let now = SystemTime::now();
    let hour_ago = now.checked_sub(Duration::from_secs(3600)).unwrap_or(now);
    let cutoff = now.checked_sub(Duration::from_secs(delete_days as u64 * 86_400));
    for f in list(server_dir).files {
        let p = server_dir.join(&f.rel);
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let modified = fs::metadata(&p).and_then(|m| m.modified()).unwrap_or(now);
        if f.gz {
            if delete_days > 0 && cutoff.is_some_and(|c| modified < c) && fs::remove_file(&p).is_ok() {
                r.deleted += 1;
                r.freed += f.size;
            }
            continue;
        }
        // текущие файлы сервера и свежие (могут ещё писаться) не трогаем
        if name == "latest.log" || name == "debug.log" || modified > hour_ago {
            continue;
        }
        if let Ok(saved) = gzip(&p) {
            r.compressed += 1;
            r.freed += saved;
        }
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_and_clean() {
        let t = tempfile::tempdir().unwrap();
        let logs = t.path().join("logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(logs.join("latest.log"), "a\nSteve joined the game\n").unwrap();
        let old = logs.join("2026-01-01-1.log");
        fs::write(&old, "x\n".repeat(5000) + "Griefer joined the game\n").unwrap();
        let past = SystemTime::now() - Duration::from_secs(7200);
        fs::File::options().write(true).open(&old).unwrap().set_modified(past).unwrap();
        assert_eq!(search(t.path(), "JOINED", 10).len(), 2);
        let r = clean(t.path(), 30).unwrap();
        assert_eq!(r.compressed, 1);
        assert!(r.freed > 0);
        assert!(logs.join("2026-01-01-1.log.gz").is_file() && logs.join("latest.log").is_file());
        let h = search(t.path(), "griefer", 10);
        assert_eq!((h.len(), h[0].line), (1, 5001));
    }
}
