//! Java: поиск установленных версий, подбор нужной под версию Minecraft
//! и загрузка Eclipse Temurin (Adoptium) в папку `java/` рядом с менеджером.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct JavaInstall {
    /// путь к java(.exe)
    pub path: String,
    pub major: u32,
    pub version: String,
    pub vendor: String,
    /// path / java_home / managed / launcher / system
    pub source: String,
}

static MANAGED: OnceLock<PathBuf> = OnceLock::new();

/// Папка, куда менеджер ставит свои Java (`<рядом с exe>/java`).
pub fn set_managed_root(p: PathBuf) {
    let _ = MANAGED.set(p);
}
pub fn managed_root() -> Option<&'static Path> {
    MANAGED.get().map(PathBuf::as_path)
}

fn exe() -> &'static str {
    if cfg!(windows) {
        "java.exe"
    } else {
        "java"
    }
}

/// Нужная версия Java для версии Minecraft (по официальным требованиям Mojang).
pub fn required_for(mc: &str) -> Option<u32> {
    let mut it = mc.trim().split(['.', '-', ' ']).filter_map(|x| x.parse::<u32>().ok());
    let a = it.next()?;
    if a >= 26 {
        // новая нумерация (26.1 и дальше) — Java 25
        return Some(25);
    }
    if a != 1 {
        return None;
    }
    let minor = it.next()?;
    let patch = it.next().unwrap_or(0);
    Some(match minor {
        0..=16 => 8,
        17 => 16,
        18 | 19 => 17,
        20 if patch < 5 => 17,
        _ => 21,
    })
}

/// «21.0.4» → 21, «1.8.0_402» → 8, «17» → 17
pub fn parse_major(v: &str) -> Option<u32> {
    let v = v.trim().trim_matches('"');
    let mut it = v.split(['.', '_', '+', '-']);
    let a: u32 = it.next()?.parse().ok()?;
    if a == 1 {
        it.next()?.parse().ok()
    } else {
        Some(a)
    }
}

/// Читает `release` в папке JDK/JRE — быстро, без запуска java.
fn from_release(home: &Path) -> Option<(String, String)> {
    let t = fs::read_to_string(home.join("release")).ok()?;
    let get = |k: &str| {
        t.lines()
            .find_map(|l| l.strip_prefix(k).and_then(|r| r.strip_prefix('=')))
            .map(|v| v.trim().trim_matches('"').to_string())
    };
    let ver = get("JAVA_VERSION")?;
    let vendor = get("IMPLEMENTOR").unwrap_or_default();
    Some((ver, vendor))
}

/// `java -version` (печатает в stderr): openjdk version "21.0.4" 2024-07-16 LTS
fn from_run(java: &Path) -> Option<(String, String)> {
    let mut c = Command::new(java);
    c.arg("-version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = c.output().ok()?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout));
    let first = text.lines().next()?;
    let ver = first.split('"').nth(1)?.to_string();
    let vendor = text
        .lines()
        .nth(1)
        .map(|l| l.split(" Runtime").next().unwrap_or(l).trim().to_string())
        .unwrap_or_default();
    Some((ver, vendor))
}

fn inspect(java: &Path, source: &str) -> Option<JavaInstall> {
    if !java.is_file() {
        return None;
    }
    let home = java.parent()?.parent()?;
    let (version, vendor) = from_release(home).or_else(|| from_run(java))?;
    let major = parse_major(&version)?;
    let path = fs::canonicalize(java).unwrap_or_else(|_| java.to_path_buf());
    let mut p = path.to_string_lossy().to_string();
    if let Some(s) = p.strip_prefix(r"\\?\") {
        p = s.to_string();
    }
    Some(JavaInstall { path: p, major, version, vendor, source: source.into() })
}

/// Все `<root>/*/bin/java(.exe)` (и `<root>/*/Contents/Home/bin/java` на macOS).
fn homes_in(root: &Path, depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(root) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let j = p.join("bin").join(exe());
        if j.is_file() {
            out.push(j);
        } else if p.join("Contents/Home/bin").join(exe()).is_file() {
            out.push(p.join("Contents/Home/bin").join(exe()));
        } else if depth > 0 {
            out.extend(homes_in(&p, depth - 1));
        }
    }
    out
}

fn which_java() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(exe())).find(|p| p.is_file())
}

/// Найти все установленные Java.
pub fn scan() -> Vec<JavaInstall> {
    let mut cands: Vec<(PathBuf, &str)> = Vec::new();
    if let Some(m) = managed_root() {
        cands.extend(homes_in(m, 1).into_iter().map(|p| (p, "managed")));
    }
    if let Some(p) = which_java() {
        cands.push((p, "path"));
    }
    if let Some(h) = std::env::var_os("JAVA_HOME") {
        cands.push((PathBuf::from(h).join("bin").join(exe()), "java_home"));
    }
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let mut roots: Vec<(PathBuf, usize, &str)> = Vec::new();
    if cfg!(windows) {
        for pf in [env("ProgramFiles"), env("ProgramFiles(x86)"), env("ProgramW6432")].into_iter().flatten() {
            for v in ["Java", "Eclipse Adoptium", "Eclipse Foundation", "AdoptOpenJDK", "Microsoft", "Zulu", "BellSoft", "Amazon Corretto", "Semeru", "OpenJDK", "GraalVM"] {
                roots.push((pf.join(v), 1, "system"));
            }
        }
        if let Some(la) = env("LOCALAPPDATA") {
            roots.push((la.join("Programs").join("Eclipse Adoptium"), 1, "system"));
            // Minecraft Launcher из Microsoft Store
            roots.push((la.join(r"Packages\Microsoft.4297127D64EC6_8wekyb3d8bbwe\LocalCache\Local\runtime"), 3, "launcher"));
        }
        if let Some(ad) = env("APPDATA") {
            roots.push((ad.join(".minecraft").join("runtime"), 3, "launcher"));
        }
        if let Some(pf86) = env("ProgramFiles(x86)") {
            roots.push((pf86.join("Minecraft Launcher").join("runtime"), 3, "launcher"));
        }
    } else {
        roots.push((PathBuf::from("/usr/lib/jvm"), 0, "system"));
        roots.push((PathBuf::from("/usr/local/lib/jvm"), 0, "system"));
        roots.push((PathBuf::from("/Library/Java/JavaVirtualMachines"), 0, "system"));
        if let Some(h) = env("HOME") {
            roots.push((h.join(".minecraft/runtime"), 3, "launcher"));
        }
    }
    if let Some(h) = env("USERPROFILE").or_else(|| env("HOME")) {
        roots.push((h.join(".jdks"), 0, "system"));
        roots.push((h.join(".sdkman/candidates/java"), 0, "system"));
    }
    for (r, depth, src) in roots {
        cands.extend(homes_in(&r, depth).into_iter().map(|p| (p, src)));
    }

    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (p, src) in cands {
        if let Some(j) = inspect(&p, src) {
            if seen.insert(j.path.to_lowercase()) {
                out.push(j);
            }
        }
    }
    out.sort_by(|a, b| b.major.cmp(&a.major).then(a.source.cmp(&b.source)));
    out
}

fn rank(src: &str) -> u8 {
    match src {
        "managed" => 0,
        "path" => 1,
        "java_home" => 2,
        "system" => 3,
        _ => 4,
    }
}

/// Подобрать Java под версию Minecraft: точное совпадение, иначе ближайшая новее
/// (кроме Java 8 — старые Forge на новых Java не запускаются).
pub fn pick(installs: &[JavaInstall], required: Option<u32>) -> Option<&JavaInstall> {
    let req = required?;
    let best = |f: &dyn Fn(&JavaInstall) -> bool| {
        installs.iter().filter(|j| f(j)).min_by_key(|j| (j.major, rank(&j.source)))
    };
    best(&|j| j.major == req).or_else(|| if req == 8 { None } else { best(&|j| j.major > req) })
}

/// Java для автоматического запуска, если путь не указан вручную.
pub fn auto_java(mc_version: &str) -> Option<String> {
    let all = scan();
    pick(&all, required_for(mc_version)).map(|j| j.path.clone())
}

/// Проверка выбранной Java: (major, понятная проблема, если есть).
pub fn check(java: &str, mc_version: &str) -> (Option<u32>, Option<String>) {
    let path = if java.trim().is_empty() || java.trim() == "java" {
        match which_java() {
            Some(p) => p,
            None => return (None, Some("Java не найдена: её нет в PATH и путь не указан".into())),
        }
    } else {
        PathBuf::from(java.trim())
    };
    let Some(j) = inspect(&path, "check") else {
        return (None, Some(format!("не удалось запустить Java: {}", path.display())));
    };
    let Some(req) = required_for(mc_version) else { return (Some(j.major), None) };
    let problem = if j.major < req {
        Some(format!("Minecraft {mc_version} нужна Java {req}+, а выбрана Java {}", j.major))
    } else if req == 8 && j.major > 8 {
        Some(format!("Minecraft {mc_version} рассчитан на Java 8, а выбрана Java {} — старые моды и Forge на ней часто не запускаются", j.major))
    } else {
        None
    };
    (Some(j.major), problem)
}

// ===================================================================== загрузка Temurin

/// Скачать Eclipse Temurin нужной версии в `<managed_root>/temurin-<major>`.
pub fn download_temurin(major: u32, mut on_progress: impl FnMut(u64, u64)) -> Result<JavaInstall> {
    if !cfg!(windows) {
        return Err(Error::other("автозагрузка Java есть только для Windows — установи OpenJDK через пакетный менеджер"));
    }
    let root = managed_root().ok_or_else(|| Error::other("папка для Java не задана"))?.to_path_buf();
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };
    // JRE меньше; если для версии нет JRE (бывает у не-LTS) — берём JDK
    let mut pkg = None;
    for image in ["jre", "jdk"] {
        let url = format!("https://api.adoptium.net/v3/assets/latest/{major}/hotspot?os=windows&architecture={arch}&image_type={image}&vendor=eclipse");
        if let Ok(Value::Array(a)) = crate::net::get_json(&url) {
            if let Some(p) = a.iter().find_map(|x| {
                let p = &x["binary"]["package"];
                Some((p["link"].as_str()?.to_string(), p["checksum"].as_str().unwrap_or("").to_string(), p["size"].as_u64().unwrap_or(0)))
            }) {
                pkg = Some(p);
                break;
            }
        }
    }
    let (link, sha, size) = pkg.ok_or_else(|| Error::Network(format!("Adoptium не отдал сборку Java {major} для Windows {arch}")))?;
    if !link.starts_with("https://") || !link.ends_with(".zip") {
        return Err(Error::Network(format!("неожиданный файл Java: {link}")));
    }

    let mut resp = crate::util::http()
        .get(&link)
        .set("User-Agent", crate::net::UA)
        .call()
        .map_err(|e| Error::Network(e.to_string()))?
        .into_reader()
        .take(600 * 1024 * 1024);
    let mut bytes = Vec::with_capacity(size as usize);
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        on_progress(bytes.len() as u64, size);
    }
    if !sha.is_empty() {
        let got = format!("{:x}", Sha256::digest(&bytes));
        if !got.eq_ignore_ascii_case(&sha) {
            return Err(Error::Network("контрольная сумма Java не совпала — файл повреждён, попробуй ещё раз".into()));
        }
    }

    fs::create_dir_all(&root)?;
    let dest = root.join(format!("temurin-{major}"));
    let tmp = root.join(format!(".temurin-{major}.tmp"));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        let Some(name) = f.enclosed_name() else { continue };
        // в архиве всё лежит в папке вида jdk-21.0.4+7-jre/ — её срезаем
        let rel: PathBuf = name.components().skip(1).collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = tmp.join(rel);
        if f.is_dir() {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(p) = out.parent() {
                fs::create_dir_all(p)?;
            }
            std::io::copy(&mut f, &mut fs::File::create(&out)?)?;
        }
    }
    let _ = fs::remove_dir_all(&dest);
    fs::rename(&tmp, &dest)?;
    inspect(&dest.join("bin").join(exe()), "managed").ok_or_else(|| Error::other("Java скачана, но не запускается"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements() {
        assert_eq!(required_for("1.12.2"), Some(8));
        assert_eq!(required_for("1.16.5"), Some(8));
        assert_eq!(required_for("1.17.1"), Some(16));
        assert_eq!(required_for("1.18.2"), Some(17));
        assert_eq!(required_for("1.20.4"), Some(17));
        assert_eq!(required_for("1.20.6"), Some(21));
        assert_eq!(required_for("1.21.1"), Some(21));
        assert_eq!(required_for("26.1"), Some(25));
        assert_eq!(required_for(""), None);
        assert_eq!(parse_major("1.8.0_402"), Some(8));
        assert_eq!(parse_major("21.0.4"), Some(21));
        assert_eq!(parse_major("\"17\""), Some(17));
    }

    #[test]
    fn picking() {
        let j = |m: u32, s: &str| JavaInstall { path: format!("/j{m}{s}"), major: m, version: m.to_string(), vendor: String::new(), source: s.into() };
        let all = vec![j(8, "system"), j(17, "path"), j(21, "system"), j(21, "managed"), j(25, "system")];
        assert_eq!(pick(&all, Some(21)).unwrap().path, "/j21managed");
        assert_eq!(pick(&all, Some(16)).unwrap().major, 17);
        assert_eq!(pick(&all, Some(8)).unwrap().major, 8);
        assert!(pick(&[j(17, "path")], Some(8)).is_none());
        assert!(pick(&[j(17, "path")], Some(21)).is_none());
    }

    #[test]
    fn scan_finds_release_file() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().join("temurin-21");
        fs::create_dir_all(home.join("bin")).unwrap();
        fs::write(home.join("bin").join(exe()), b"").unwrap();
        fs::write(home.join("release"), "IMPLEMENTOR=\"Eclipse Adoptium\"\nJAVA_VERSION=\"21.0.4\"\n").unwrap();
        let found = homes_in(t.path(), 1);
        assert_eq!(found.len(), 1);
        let j = inspect(&found[0], "managed").unwrap();
        assert_eq!((j.major, j.vendor.as_str()), (21, "Eclipse Adoptium"));
    }
}
