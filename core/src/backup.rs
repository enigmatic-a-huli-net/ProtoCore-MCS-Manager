//! Снапшоты (образы) сервера через restic и откат.
//!
//! Почему restic, а не zip-копии:
//! - **дедупликация** — неизменённые файлы и куски регионов не хранятся повторно,
//!   поэтому снапшот при каждом запуске почти не занимает места;
//! - **целостность** — у каждого куска есть контрольная сумма, `restic check` ловит порчу;
//! - **откат** до любого снапшота, не только последнего.
//!
//! У каждого сервера свой репозиторий (см. `ServerEntry::restic_repo`).
//! restic.exe скачивается автоматически в `tools/` при первом использовании.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{AppPaths, ServerEntry};
use crate::process::ProcessManager;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub short_id: String,
    pub time: String,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub summary: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub name: String,
    /// путь относительно папки сервера, через «/»
    pub rel: String,
    pub dir: bool,
    pub size: u64,
    pub mtime: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Diff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct BackupResult {
    pub snapshot_id: String,
    pub files_new: u64,
    pub files_changed: u64,
    pub data_added: u64,
    pub total_bytes: u64,
    pub seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    /// 0.0 … 1.0
    pub percent: f64,
    pub files_done: u64,
    pub bytes_done: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResticStatus {
    pub found: bool,
    pub path: String,
    pub version: String,
}

// ===================================================================== поиск и загрузка restic

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "restic.exe"
    } else {
        "restic"
    }
}

/// tools/restic(.exe) рядом с менеджером, иначе restic из PATH.
pub fn find_restic(paths: &AppPaths) -> Option<PathBuf> {
    let local = paths.tools.join(exe_name());
    if local.is_file() {
        return Some(local);
    }
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|d| d.join(exe_name()))
        .find(|p| p.is_file())
}

pub fn status(paths: &AppPaths) -> ResticStatus {
    match find_restic(paths) {
        Some(p) => {
            let version = hidden(Command::new(&p).arg("version"))
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            ResticStatus { found: true, path: p.to_string_lossy().into(), version }
        }
        None => ResticStatus { found: false, path: String::new(), version: String::new() },
    }
}

/// Скачивает последнюю версию restic с GitHub в tools/.
pub fn download_restic(paths: &AppPaths) -> Result<PathBuf> {
    paths.ensure()?;
    let rel: Value = crate::util::http().get("https://api.github.com/repos/restic/restic/releases/latest")
        .set("User-Agent", "ProtoCore-MCS-Manager")
        .call()
        .map_err(|e| Error::Network(e.to_string()))?
        .into_json()
        .map_err(|e| Error::Network(e.to_string()))?;
    let tag = rel.get("tag_name").and_then(Value::as_str).unwrap_or("");
    let ver = tag.trim_start_matches('v');
    let (os, ext) = if cfg!(windows) {
        ("windows", "zip")
    } else if cfg!(target_os = "macos") {
        ("darwin", "bz2")
    } else {
        ("linux", "bz2")
    };
    if ext != "zip" {
        return Err(Error::Restic(
            "автозагрузка restic есть только для Windows — установи restic через пакетный менеджер".into(),
        ));
    }
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
    let asset = format!("restic_{ver}_{os}_{arch}.{ext}");
    let url = rel
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .find(|x| x.get("name").and_then(Value::as_str) == Some(asset.as_str()))
                .and_then(|x| x.get("browser_download_url").and_then(Value::as_str))
        })
        .ok_or_else(|| Error::Restic(format!("в релизе {tag} нет файла {asset}")))?
        .to_string();

    let mut bytes = Vec::new();
    crate::util::http().get(&url)
        .set("User-Agent", "ProtoCore-MCS-Manager")
        .call()
        .map_err(|e| Error::Network(e.to_string()))?
        .into_reader()
        .take(200 * 1024 * 1024)
        .read_to_end(&mut bytes)?;

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let inner = (0..zip.len())
        .find(|&i| zip.by_index(i).map(|f| f.name().ends_with(".exe")).unwrap_or(false))
        .ok_or_else(|| Error::Restic("в архиве нет restic.exe".into()))?;
    let mut f = zip.by_index(inner)?;
    let dst = paths.tools.join(exe_name());
    let tmp = dst.with_extension("download");
    let mut out = fs::File::create(&tmp)?;
    std::io::copy(&mut f, &mut out)?;
    drop(out);
    fs::rename(&tmp, &dst)?;
    Ok(dst)
}

// ===================================================================== запуск restic

fn hidden(c: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}

pub struct Repo {
    restic: PathBuf,
    repo: PathBuf,
    password: String,
}

impl Repo {
    /// Готовит репозиторий сервера. Если пароля ещё нет — его нужно сгенерировать
    /// и сохранить в конфиг до вызова (это делает вызывающая сторона).
    pub fn for_server(paths: &AppPaths, server: &ServerEntry) -> Result<Self> {
        let restic = find_restic(paths).ok_or_else(|| {
            Error::Restic("restic не найден — нажми «Скачать restic» на вкладке бэкапов".into())
        })?;
        if server.backup.password.is_empty() {
            return Err(Error::Restic("у сервера нет пароля репозитория".into()));
        }
        Ok(Self { restic, repo: server.restic_repo(paths), password: server.backup.password.clone() })
    }

    pub fn path(&self) -> &Path {
        &self.repo
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(&self.restic);
        c.env("RESTIC_REPOSITORY", &self.repo)
            .env("RESTIC_PASSWORD", &self.password)
            // кэш restic — внутри репозитория, чтобы менеджер оставался портативным
            .env("RESTIC_CACHE_DIR", self.repo.join(".cache"))
            .stdin(Stdio::null());
        hidden(&mut c);
        c
    }

    fn run(&self, args: &[String]) -> Result<String> {
        let out = self.cmd().args(args).output()?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(Error::Restic(if err.is_empty() { format!("код {:?}", out.status.code()) } else { err }));
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    pub fn is_initialized(&self) -> bool {
        self.repo.join("config").is_file()
    }

    pub fn init_if_needed(&self) -> Result<()> {
        if self.is_initialized() {
            return Ok(());
        }
        fs::create_dir_all(&self.repo)?;
        self.run(&["init".into()])?;
        Ok(())
    }

    /// Снапшот папки сервера.
    pub fn backup(
        &self,
        server_dir: &Path,
        excludes: &[String],
        tags: &[&str],
        mut on_progress: impl FnMut(Progress),
    ) -> Result<BackupResult> {
        self.init_if_needed()?;
        let mut args: Vec<String> = vec!["backup".into(), "--json".into()];
        for t in tags {
            args.push("--tag".into());
            args.push((*t).to_string());
        }
        // не отменяем снапшот из-за файла, занятого другой программой
        args.push("--no-scan".into());
        for e in excludes {
            let e = e.trim();
            if e.is_empty() {
                continue;
            }
            args.push("--exclude".into());
            if e.starts_with('*') || e.contains('/') && !e.starts_with("./") {
                args.push(e.to_string());
            } else {
                // «logs» → только <сервер>/logs, а не любые папки logs глубже
                args.push(server_dir.join(e.trim_start_matches("./")).to_string_lossy().to_string());
            }
        }
        args.push(server_dir.to_string_lossy().to_string());

        let mut child = self.cmd().args(&args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let stderr = child.stderr.take();
        let err_handle = std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(mut e) = stderr {
                let _ = e.read_to_string(&mut s);
            }
            s
        });
        let mut result = BackupResult::default();
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines().map_while(std::result::Result::ok) {
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                match v.get("message_type").and_then(Value::as_str) {
                    Some("status") => on_progress(Progress {
                        percent: v.get("percent_done").and_then(Value::as_f64).unwrap_or(0.0),
                        files_done: v.get("files_done").and_then(Value::as_u64).unwrap_or(0),
                        bytes_done: v.get("bytes_done").and_then(Value::as_u64).unwrap_or(0),
                        message: String::new(),
                    }),
                    Some("summary") => {
                        let u = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
                        result = BackupResult {
                            snapshot_id: v.get("snapshot_id").and_then(Value::as_str).unwrap_or("").to_string(),
                            files_new: u("files_new"),
                            files_changed: u("files_changed"),
                            data_added: u("data_added"),
                            total_bytes: u("total_bytes_processed"),
                            seconds: v.get("total_duration").and_then(Value::as_f64).unwrap_or(0.0),
                        };
                    }
                    _ => {}
                }
            }
        }
        let status = child.wait()?;
        let err = err_handle.join().unwrap_or_default();
        // код 3 = часть файлов не прочитана (заняты), снапшот всё равно создан
        if !status.success() && status.code() != Some(3) {
            return Err(Error::Restic(err.trim().to_string()));
        }
        if result.snapshot_id.is_empty() {
            return Err(Error::Restic(format!("снапшот не создан: {}", err.trim())));
        }
        Ok(result)
    }

    pub fn snapshots(&self) -> Result<Vec<Snapshot>> {
        if !self.is_initialized() {
            return Ok(vec![]);
        }
        let out = self.run(&["snapshots".into(), "--json".into()])?;
        let mut v: Vec<Snapshot> = serde_json::from_str(&out).unwrap_or_default();
        v.sort_by(|a, b| b.time.cmp(&a.time)); // новые сверху
        Ok(v)
    }

    /// Удаляет старые снапшоты, оставляя `keep_last` последних, и чистит место.
    pub fn forget(&self, keep_last: u32) -> Result<()> {
        let b = crate::config::BackupSettings { keep_last, keep_daily: 0, keep_weekly: 0, keep_monthly: 0, ..Default::default() };
        self.apply_retention(&b)
    }

    /// Политика хранения: последние N + дневные/недельные/месячные. Всё по нулям — ничего не удаляется.
    pub fn apply_retention(&self, b: &crate::config::BackupSettings) -> Result<()> {
        let keep = retention_args(b);
        if keep.is_empty() || !self.is_initialized() {
            return Ok(());
        }
        let mut args: Vec<String> = vec!["forget".into()];
        args.extend(keep.into_iter().map(String::from));
        args.push("--prune".into());
        self.run(&args)?;
        Ok(())
    }

    pub fn delete_snapshot(&self, id: &str) -> Result<()> {
        self.run(&["forget".into(), id.into(), "--prune".into()])?;
        Ok(())
    }

    pub fn check(&self) -> Result<String> {
        self.run(&["check".into()])
    }

    /// Проверка с чтением части данных: ловит испорченные файлы репозитория на диске.
    pub fn check_data(&self, percent: u32) -> Result<String> {
        self.run(&["check".into(), format!("--read-data-subset={}%", percent.clamp(1, 100))])
    }

    /// Корень снапшота внутри restic (`/C/Servers/mc` для `C:\Servers\mc`).
    pub fn snapshot_root(snap: &Snapshot) -> Result<String> {
        snap.paths
            .first()
            .map(|p| restic_inner_path(p))
            .ok_or_else(|| Error::Restic("в снапшоте нет пути".into()))
    }

    /// Содержимое папки в снапшоте. `rel` — путь относительно папки сервера ("" — корень).
    pub fn ls(&self, snap: &Snapshot, rel: &str) -> Result<Vec<Entry>> {
        let root = Self::snapshot_root(snap)?;
        let rel = rel.trim_matches('/').replace('\\', "/");
        let dir = if rel.is_empty() { root.clone() } else { format!("{root}/{rel}") };
        let out = self.run(&["ls".into(), "--json".into(), snap.id.clone(), dir.clone()])?;
        let mut v = Vec::new();
        for line in out.lines() {
            let Ok(n) = serde_json::from_str::<Value>(line) else { continue };
            if n.get("struct_type").and_then(Value::as_str) != Some("node") {
                continue;
            }
            let path = n.get("path").and_then(Value::as_str).unwrap_or("");
            if path == dir {
                continue; // сама папка
            }
            let Some(r) = path.strip_prefix(&format!("{root}/")) else { continue };
            v.push(Entry {
                name: n.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                rel: r.to_string(),
                dir: n.get("type").and_then(Value::as_str) == Some("dir"),
                size: n.get("size").and_then(Value::as_u64).unwrap_or(0),
                mtime: n.get("mtime").and_then(Value::as_str).unwrap_or("").to_string(),
            });
        }
        v.sort_by(|a, b| b.dir.cmp(&a.dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        Ok(v)
    }

    /// Восстановить один файл или папку из снапшота в `dest` (полный путь назначения).
    /// Существующий `dest` заменяется.
    pub fn restore_item(&self, snap: &Snapshot, rel: &str, dest: &Path) -> Result<()> {
        let root = Self::snapshot_root(snap)?;
        let rel = rel.trim_matches('/').replace('\\', "/");
        if rel.is_empty() || rel.split('/').any(|p| p == "..") {
            return Err(Error::other("недопустимый путь"));
        }
        let (parent, name) = match rel.rsplit_once('/') {
            Some((p, n)) => (format!("{root}/{p}"), n.to_string()),
            None => (root.clone(), rel.clone()),
        };
        let tmp = dest
            .parent()
            .ok_or_else(|| Error::other("нет папки назначения"))?
            .join(format!(".pcm-restore-{}", crate::util::short_id()));
        fs::create_dir_all(&tmp)?;
        let res = self.run(&[
            "restore".into(),
            format!("{}:{parent}", snap.id),
            "--target".into(),
            tmp.to_string_lossy().to_string(),
            "--include".into(),
            format!("/{name}"),
        ]);
        let restored = tmp.join(&name);
        let out = res.and_then(|_| {
            if !restored.exists() {
                return Err(Error::Restic(format!("в снапшоте нет «{rel}»")));
            }
            if dest.is_dir() {
                fs::remove_dir_all(dest)?;
            } else if dest.exists() {
                fs::remove_file(dest)?;
            }
            if let Some(p) = dest.parent() {
                fs::create_dir_all(p)?;
            }
            fs::rename(&restored, dest)?;
            Ok(())
        });
        let _ = fs::remove_dir_all(&tmp);
        out
    }

    /// Что изменилось между двумя снапшотами. Пути — относительно папки сервера.
    pub fn diff(&self, a: &Snapshot, b: &Snapshot) -> Result<Diff> {
        let out = self.run(&["diff".into(), "--json".into(), a.id.clone(), b.id.clone()])?;
        let roots: Vec<String> = [a, b].iter().filter_map(|s| Self::snapshot_root(s).ok()).collect();
        let mut d = Diff::default();
        for line in out.lines() {
            let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
            if v.get("message_type").and_then(Value::as_str) != Some("change") {
                continue;
            }
            let p = v.get("path").and_then(Value::as_str).unwrap_or("");
            // путь в diff бывает от корня диска или от последней папки — отрезаем любой из корней
            let mut rel = p.trim_start_matches('/').to_string();
            for r in &roots {
                let base = r.rsplit('/').next().unwrap_or("");
                if let Some(x) = p.strip_prefix(&format!("{r}/")) {
                    rel = x.to_string();
                    break;
                }
                if let Some(x) = p.strip_prefix(&format!("/{base}/")) {
                    rel = x.to_string();
                    break;
                }
            }
            if rel.ends_with('/') {
                continue;
            }
            match v.get("modifier").and_then(Value::as_str).unwrap_or("") {
                "+" => d.added.push(rel),
                "-" => d.removed.push(rel),
                _ => d.modified.push(rel),
            }
        }
        Ok(d)
    }

    /// Скопировать снапшоты во второе хранилище (создаёт его при первом запуске).
    pub fn copy_to(&self, dest: &str, env: &[String], keep_last: u32) -> Result<String> {
        let mk = |args: &[&str]| -> Command {
            let mut c = Command::new(&self.restic);
            c.env("RESTIC_REPOSITORY", dest)
                .env("RESTIC_PASSWORD", &self.password)
                .env("RESTIC_FROM_REPOSITORY", &self.repo)
                .env("RESTIC_FROM_PASSWORD", &self.password)
                .env("RESTIC_CACHE_DIR", self.repo.join(".cache"))
                .stdin(Stdio::null())
                .args(args);
            for kv in env {
                if let Some((k, v)) = kv.split_once('=') {
                    if !k.trim().is_empty() {
                        c.env(k.trim(), v.trim());
                    }
                }
            }
            hidden(&mut c);
            c
        };
        let run = |args: &[&str]| -> Result<String> {
            let out = mk(args).output()?;
            if !out.status.success() {
                return Err(Error::Restic(String::from_utf8_lossy(&out.stderr).trim().to_string()));
            }
            Ok(String::from_utf8_lossy(&out.stdout).to_string())
        };
        // есть ли хранилище? нет — создаём с теми же параметрами нарезки (так работает дедупликация)
        if run(&["cat", "config"]).is_err() {
            if !dest.contains(':') || dest.len() > 2 && dest.as_bytes()[1] == b':' {
                fs::create_dir_all(dest)?;
            }
            run(&["init", "--copy-chunker-params"])?;
        }
        let out = run(&["copy"])?;
        if keep_last > 0 {
            let _ = run(&["forget", "--keep-last", &keep_last.to_string(), "--prune"]);
        }
        Ok(out)
    }

    /// Восстанавливает снапшот в папку сервера.
    /// Файлы, которых не было в снапшоте, удаляются (`--delete`), кроме исключённых.
    pub fn restore(&self, snap: &Snapshot, server_dir: &Path, excludes: &[String]) -> Result<()> {
        let src = snap
            .paths
            .first()
            .ok_or_else(|| Error::Restic("в снапшоте нет пути".into()))?;
        let inner = restic_inner_path(src);
        let mut args: Vec<String> = vec![
            "restore".into(),
            format!("{}:{}", snap.id, inner),
            "--target".into(),
            server_dir.to_string_lossy().to_string(),
            "--delete".into(),
        ];
        for e in excludes.iter().filter(|e| !e.trim().is_empty()) {
            args.push("--exclude".into());
            args.push(e.trim().trim_start_matches("./").to_string());
        }
        self.run(&args)?;
        Ok(())
    }

    /// Размер репозитория на диске, байт.
    pub fn size_on_disk(&self) -> u64 {
        dir_size(&self.repo)
    }
}

pub fn retention_args(b: &crate::config::BackupSettings) -> Vec<String> {
    let mut v = Vec::new();
    for (flag, n) in [("--keep-last", b.keep_last), ("--keep-daily", b.keep_daily), ("--keep-weekly", b.keep_weekly), ("--keep-monthly", b.keep_monthly)] {
        if n > 0 {
            v.push(flag.to_string());
            v.push(n.to_string());
        }
    }
    v
}

/// Внутри restic пути Windows хранятся как `/C/Users/...`.
pub fn restic_inner_path(p: &str) -> String {
    let p = p.replace('\\', "/");
    let b = p.as_bytes();
    if b.len() >= 2 && b[1] == b':' {
        format!("/{}{}", &p[..1], &p[2..])
    } else {
        p
    }
}

fn dir_size(p: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(_) => total += e.metadata().map(|m| m.len()).unwrap_or(0),
                Err(_) => {}
            }
        }
    }
    total
}

// ===================================================================== бэкап «на ходу»

/// Снапшот запущенного сервера без риска побить мир:
/// `save-off` → `save-all flush` → ждём «Saved the game» → снапшот → `save-on`.
/// Если сервер не запущен — обычный снапшот.
pub fn backup_safely(
    pm: &Arc<ProcessManager>,
    repo: &Repo,
    server: &ServerEntry,
    tags: &[&str],
    on_progress: impl FnMut(Progress),
) -> Result<BackupResult> {
    let live = pm.is_running(&server.id);
    if live {
        pm.system(&server.id, "💾 Бэкап: save-off, save-all flush…");
        pm.send(&server.id, "save-off")?;
        pm.send(&server.id, "save-all flush")?;
        let saved = pm.wait_for(
            &server.id,
            r"(?i)(Saved the game|Saved the world|All dimensions are saved|Сохранение)",
            Duration::from_secs(120),
        );
        if saved.is_none() {
            pm.system(&server.id, "⚠ Не дождался подтверждения сохранения — делаю снапшот всё равно");
        }
    }
    let res = repo.backup(&server.dir(), &server.backup.excludes, tags, on_progress);
    if live {
        let _ = pm.send(&server.id, "save-on");
        match &res {
            Ok(r) => pm.system(
                &server.id,
                format!("💾 Снапшот {} готов, добавлено {}", &r.snapshot_id[..8.min(r.snapshot_id.len())], human_bytes(r.data_added)),
            ),
            Err(e) => pm.system(&server.id, format!("✖ Бэкап не удался: {e}")),
        }
    }
    res
}

pub fn human_bytes(b: u64) -> String {
    const U: [&str; 5] = ["Б", "КБ", "МБ", "ГБ", "ТБ"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} Б")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_path() {
        assert_eq!(restic_inner_path(r"C:\Servers\mc"), "/C/Servers/mc");
        assert_eq!(restic_inner_path("/home/x/mc"), "/home/x/mc");
    }

    #[test]
    fn bytes() {
        assert_eq!(human_bytes(512), "512 Б");
        assert_eq!(human_bytes(1536), "1.5 КБ");
    }

    /// Откат: изменённое возвращается, лишнее удаляется, но исключённые папки
    /// (логи, restored, бэкапы других программ) откат НЕ трогает.
    #[test]
    fn rollback_keeps_excluded_if_available() {
        let t = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_base(t.path().join("mgr"));
        if find_restic(&paths).is_none() {
            return;
        }
        let srv = t.path().join("srv");
        fs::create_dir_all(srv.join("world")).unwrap();
        fs::create_dir_all(srv.join("logs")).unwrap();
        fs::write(srv.join("world/level.dat"), "good").unwrap();
        fs::write(srv.join("logs/latest.log"), "log-1").unwrap();
        let mut s = ServerEntry { id: "r1".into(), name: "R".into(), path: srv.to_string_lossy().into(), ..Default::default() };
        s.backup.password = "pw".into();
        let excludes = s.backup.excludes.clone();
        assert!(excludes.iter().any(|e| e == "logs") && excludes.iter().any(|e| e == "restored"));
        let repo = Repo::for_server(&paths, &s).unwrap();
        repo.backup(&srv, &excludes, &["t"], |_| {}).unwrap();
        let snap = repo.snapshots().unwrap().remove(0);

        // после снапшота: мир испорчен, появился мусорный файл, лог дописан, есть restored/
        fs::write(srv.join("world/level.dat"), "griefed").unwrap();
        fs::write(srv.join("world/junk.dat"), "x").unwrap();
        fs::write(srv.join("logs/latest.log"), "log-2").unwrap();
        fs::create_dir_all(srv.join("restored/abc")).unwrap();
        fs::write(srv.join("restored/abc/keep.txt"), "mine").unwrap();

        repo.restore(&snap, &srv, &excludes).unwrap();
        assert_eq!(fs::read_to_string(srv.join("world/level.dat")).unwrap(), "good");
        assert!(!srv.join("world/junk.dat").exists(), "лишний файл мира должен удалиться");
        assert_eq!(fs::read_to_string(srv.join("logs/latest.log")).unwrap(), "log-2", "логи откат не трогает");
        assert_eq!(fs::read_to_string(srv.join("restored/abc/keep.txt")).unwrap(), "mine", "restored/ откат не трогает");
    }

    #[test]
    fn retention_policy() {
        let mut b = crate::config::BackupSettings::default();
        b.keep_last = 5;
        b.keep_daily = 7;
        b.keep_weekly = 0;
        b.keep_monthly = 6;
        assert_eq!(super::retention_args(&b), vec!["--keep-last", "5", "--keep-daily", "7", "--keep-monthly", "6"]);
        b.keep_last = 0;
        b.keep_daily = 0;
        b.keep_monthly = 0;
        assert!(super::retention_args(&b).is_empty(), "всё по нулям — ничего не удаляем");
    }

    /// Просмотр снапшота, точечное восстановление, сравнение и копия во второе хранилище.
    #[test]
    fn browse_restore_diff_copy_if_available() {
        let t = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_base(t.path().join("mgr"));
        if find_restic(&paths).is_none() {
            return;
        }
        let srv = t.path().join("srv");
        fs::create_dir_all(srv.join("world/playerdata")).unwrap();
        fs::create_dir_all(srv.join("mods")).unwrap();
        fs::write(srv.join("world/playerdata/u1.dat"), "inv-v1").unwrap();
        fs::write(srv.join("mods/a.jar"), "a").unwrap();
        let mut s = ServerEntry { id: "b1".into(), name: "B".into(), path: srv.to_string_lossy().into(), ..Default::default() };
        s.backup.password = "pw".into();
        let repo = Repo::for_server(&paths, &s).unwrap();
        repo.backup(&srv, &[], &["t"], |_| {}).unwrap();
        fs::write(srv.join("world/playerdata/u1.dat"), "inv-griefed").unwrap();
        fs::remove_file(srv.join("mods/a.jar")).unwrap();
        fs::write(srv.join("mods/b.jar"), "b").unwrap();
        repo.backup(&srv, &[], &["t"], |_| {}).unwrap();
        let snaps = repo.snapshots().unwrap(); // новые первыми
        let (new, old) = (&snaps[0], &snaps[1]);

        let root = repo.ls(old, "").unwrap();
        assert!(root.iter().any(|e| e.name == "world" && e.dir));
        let pd = repo.ls(old, "world/playerdata").unwrap();
        assert_eq!(pd.len(), 1);
        assert_eq!(pd[0].rel, "world/playerdata/u1.dat");

        // точечно вернуть инвентарь
        repo.restore_item(old, "world/playerdata/u1.dat", &srv.join("world/playerdata/u1.dat")).unwrap();
        assert_eq!(fs::read_to_string(srv.join("world/playerdata/u1.dat")).unwrap(), "inv-v1");
        // папку — в отдельное место
        repo.restore_item(old, "mods", &srv.join("restored/x/mods")).unwrap();
        assert!(srv.join("restored/x/mods/a.jar").exists());
        assert!(srv.join("mods/b.jar").exists());

        let d = repo.diff(old, new).unwrap();
        assert!(d.added.contains(&"mods/b.jar".to_string()), "{d:?}");
        assert!(d.removed.contains(&"mods/a.jar".to_string()));
        assert!(d.modified.contains(&"world/playerdata/u1.dat".to_string()));

        let dest = t.path().join("second");
        repo.copy_to(&dest.to_string_lossy(), &[], 0).unwrap();
        let mut s2 = s.clone();
        s2.backup.repo_path = String::new();
        let r2 = Repo { restic: repo.restic.clone(), repo: dest.clone(), password: "pw".into() };
        assert_eq!(r2.snapshots().unwrap().len(), 2);
        assert!(repo.check_data(5).is_ok());
    }

    /// Полный цикл, если restic установлен в системе: снапшот → изменение → откат.
    #[test]
    fn restic_roundtrip_if_available() {
        let t = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_base(t.path().join("mgr"));
        if find_restic(&paths).is_none() {
            eprintln!("restic не установлен — тест пропущен");
            return;
        }
        let srv = t.path().join("srv");
        fs::create_dir_all(srv.join("world")).unwrap();
        fs::create_dir_all(srv.join("logs")).unwrap();
        fs::write(srv.join("world/level.dat"), "v1").unwrap();
        fs::write(srv.join("logs/latest.log"), "log").unwrap();
        let mut s = ServerEntry { id: "t1".into(), name: "T".into(), path: srv.to_string_lossy().into(), ..Default::default() };
        s.backup.password = "pw".into();
        let repo = Repo::for_server(&paths, &s).unwrap();
        let r = repo.backup(&srv, &s.backup.excludes, &["test"], |_| {}).unwrap();
        assert!(!r.snapshot_id.is_empty());
        fs::write(srv.join("world/level.dat"), "BROKEN").unwrap();
        fs::write(srv.join("world/new.dat"), "x").unwrap();
        let snaps = repo.snapshots().unwrap();
        assert_eq!(snaps.len(), 1);
        repo.restore(&snaps[0], &srv, &s.backup.excludes).unwrap();
        assert_eq!(fs::read_to_string(srv.join("world/level.dat")).unwrap(), "v1");
        assert!(!srv.join("world/new.dat").exists());
        // логи были исключены — откат их не трогает
        assert!(srv.join("logs/latest.log").exists());
    }
}
