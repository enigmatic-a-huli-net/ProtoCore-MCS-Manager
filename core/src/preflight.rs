//! Проверка перед запуском: то, из-за чего сервер упадёт или не откроется,
//! ловим заранее и объясняем по-человечески.

use std::fs;
use std::net::TcpListener;
use std::path::Path;

use serde::Serialize;

use crate::config::{LaunchMode, ServerEntry};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warn,
    Ok,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub id: String,
    pub level: Level,
    pub title: String,
    pub detail: String,
    /// подсказка-действие для интерфейса: open_tab:<tab> / download_java:<major>
    pub fix: String,
}

fn c(id: &str, level: Level, title: impl Into<String>, detail: impl Into<String>, fix: &str) -> Check {
    Check { id: id.into(), level, title: title.into(), detail: detail.into(), fix: fix.into() }
}

pub fn port_of(dir: &Path) -> u16 {
    crate::props::read(dir)
        .into_iter()
        .find(|(k, _)| k == "server-port")
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or(25565)
}

fn port_free(port: u16) -> bool {
    TcpListener::bind(("0.0.0.0", port)).is_ok()
}

/// Мир занят другим процессом (Java держит блокировку на session.lock).
fn world_locked(dir: &Path) -> bool {
    let world = crate::props::read(dir)
        .into_iter()
        .find(|(k, _)| k == "level-name")
        .map(|(_, v)| v)
        .unwrap_or_else(|| "world".into());
    let lock = dir.join(world).join("session.lock");
    if !lock.is_file() {
        return false;
    }
    // на Windows чтение заблокированного участка даёт ошибку (lock violation)
    if cfg!(windows) {
        return fs::read(&lock).is_err();
    }
    false
}

/// Полная проверка. `running` — сервер уже запущен (тогда порт и мир не проверяем).
pub fn run(s: &ServerEntry, running: bool) -> Vec<Check> {
    let dir = s.dir();
    let mut out = Vec::new();
    if !dir.is_dir() {
        out.push(c("dir", Level::Error, "Папка сервера не найдена", dir.display().to_string(), ""));
        return out;
    }
    let d = crate::detect::detect(&dir);

    // EULA
    if !d.eula_accepted && s.core != crate::detect::CoreKind::Unknown {
        out.push(c("eula", Level::Warn, "EULA не принята", "При запуске менеджер попросит согласиться с лицензией Minecraft.", ""));
    }

    // порт
    let port = port_of(&dir);
    if !running {
        if port_free(port) {
            out.push(c("port", Level::Ok, format!("Порт {port} свободен"), "", ""));
        } else {
            out.push(c(
                "port",
                Level::Error,
                format!("Порт {port} занят"),
                "Его уже слушает другая программа — часто это второй запущенный сервер или «зависшая» java в диспетчере задач. Закрой её или поменяй server-port в настройках сервера.",
                "open_tab:props",
            ));
        }
        if world_locked(&dir) {
            out.push(c(
                "lock",
                Level::Error,
                "Мир открыт другим процессом",
                "session.lock занят: этот мир уже запущен (другой сервер из той же папки или незакрытая java). Два процесса в одном мире портят его.",
                "",
            ));
        }
    }

    // Java
    if matches!(s.launch.mode, LaunchMode::Auto) {
        let mc = if s.mc_version.is_empty() { d.mc_version.clone() } else { s.mc_version.clone() };
        let req = crate::java::required_for(&mc);
        let java = if s.launch.java_path.trim().is_empty() {
            crate::java::auto_java(&mc).unwrap_or_default()
        } else {
            s.launch.java_path.trim().to_string()
        };
        let (major, problem) = crate::java::check(&java, &mc);
        let fix = req.map(|r| format!("download_java:{r}")).unwrap_or_default();
        match (major, problem) {
            (_, Some(p)) => {
                let lvl = if major.is_some_and(|m| req.is_some_and(|r| m >= r)) { Level::Warn } else { Level::Error };
                out.push(c("java", lvl, "Неподходящая Java", p, &fix));
            }
            (Some(m), None) => out.push(c("java", Level::Ok, format!("Java {m}"), java, "")),
            (None, None) => {}
        }
    }

    // диск
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let canon = fs::canonicalize(&dir).unwrap_or(dir.clone());
    if let Some(disk) = disks
        .list()
        .iter()
        .filter(|x| canon.starts_with(x.mount_point()))
        .max_by_key(|x| x.mount_point().as_os_str().len())
    {
        let free = disk.available_space();
        let gb = free as f64 / 1_073_741_824.0;
        if free < 512 * 1024 * 1024 {
            out.push(c("disk", Level::Error, format!("На диске почти нет места: {gb:.1} ГБ"), "Сервер не сможет сохранять мир — это портит чанки. Освободи место.", ""));
        } else if free < 3 * 1024 * 1024 * 1024 {
            out.push(c("disk", Level::Warn, format!("Мало места на диске: {gb:.1} ГБ"), "Мир, логи и снапшоты растут. Лучше освободить место заранее.", ""));
        } else {
            out.push(c("disk", Level::Ok, format!("Свободно {gb:.0} ГБ"), "", ""));
        }
    }

    // память
    if matches!(s.launch.mode, LaunchMode::Auto) {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let total_mb = sys.total_memory() / 1024 / 1024;
        let want = s.launch.memory_mb as u64;
        if total_mb > 0 && want + 1536 > total_mb {
            out.push(c(
                "memory",
                Level::Warn,
                format!("Выделено {} ГБ из {} ГБ памяти ПК", want / 1024, total_mb / 1024),
                "Системе и самой Java нужно место сверх -Xmx, иначе Windows начнёт сбрасывать память на диск и сервер будет лагать.",
                "open_tab:launch",
            ));
        }
    }
    out
}

pub fn blocking(checks: &[Check]) -> Vec<&Check> {
    checks.iter().filter(|c| c.level == Level::Error).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_busy_is_error() {
        let t = tempfile::tempdir().unwrap();
        let l = TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        fs::write(t.path().join("server.properties"), format!("server-port={port}\n")).unwrap();
        let mut s = ServerEntry { path: t.path().to_string_lossy().to_string(), ..Default::default() };
        s.launch.mode = LaunchMode::Script;
        let r = run(&s, false);
        let p = r.iter().find(|c| c.id == "port").unwrap();
        assert_eq!(p.level, Level::Error);
        assert!(blocking(&r).iter().any(|c| c.id == "port"));
        assert!(run(&s, true).iter().all(|c| c.id != "port"));
    }
}
