//! Автоопределение ядра сервера и версии Minecraft.
//!
//! Порядок проверок (от самого надёжного к наименее):
//! 1. Имя jar-файла в корне (youer-1.21.1-xxxx.jar, paper-1.21.1-130.jar, …)
//! 2. Папка libraries/ (установщики Forge/NeoForge кладут туда версию)
//! 3. Содержимое jar (version.json у ванильного, install.properties у Fabric, paperclip у Paper)
//! 4. logs/latest.log («Starting minecraft server version …»)

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::util::{file_name, list_jars};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum CoreKind {
    Vanilla,
    Paper,
    Purpur,
    Folia,
    Spigot,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
    /// Гибрид NeoForge + Bukkit (MohistMC)
    Youer,
    /// Гибрид Forge + Bukkit
    Mohist,
    /// Гибрид Forge/NeoForge/Fabric + Bukkit
    Arclight,
    #[default]
    Unknown,
}

impl CoreKind {
    pub fn supports_mods(self) -> bool {
        matches!(
            self,
            Self::Fabric | Self::Quilt | Self::Forge | Self::NeoForge | Self::Youer | Self::Mohist | Self::Arclight
        )
    }

    pub fn supports_plugins(self) -> bool {
        matches!(
            self,
            Self::Paper | Self::Purpur | Self::Folia | Self::Spigot | Self::Youer | Self::Mohist | Self::Arclight
        )
    }

    /// Каким загрузчиком читаются моды (для гибридов — их основа).
    pub fn mod_loader(self) -> Option<ModLoader> {
        match self {
            Self::Fabric => Some(ModLoader::Fabric),
            Self::Quilt => Some(ModLoader::Quilt),
            Self::Forge | Self::Mohist => Some(ModLoader::Forge),
            Self::NeoForge | Self::Youer => Some(ModLoader::NeoForge),
            Self::Arclight => Some(ModLoader::NeoForge),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Paper => "Paper",
            Self::Purpur => "Purpur",
            Self::Folia => "Folia",
            Self::Spigot => "Spigot",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::Youer => "Youer",
            Self::Mohist => "Mohist",
            Self::Arclight => "Arclight",
            Self::Unknown => "?",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModLoader {
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

/// Что запускать.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "path", rename_all = "snake_case")]
pub enum LaunchTarget {
    /// `java … -jar <файл>`
    Jar(String),
    /// `java … @<файл>` — современные Forge/NeoForge (win_args.txt / unix_args.txt)
    ArgsFile(String),
    /// Не нашли, что запускать
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    pub core: CoreKind,
    pub mc_version: String,
    pub loader_version: String,
    pub target: LaunchTarget,
    /// Найденные в папке скрипты запуска (run.bat, start.bat …)
    pub scripts: Vec<String>,
    pub has_mods_dir: bool,
    pub has_plugins_dir: bool,
    pub eula_accepted: bool,
    /// Откуда взята информация — показывается пользователю
    pub source: String,
}

pub fn detect(dir: &Path) -> Detection {
    let jars = list_jars(dir);
    let mut d = Detection {
        core: CoreKind::Unknown,
        mc_version: String::new(),
        loader_version: String::new(),
        target: LaunchTarget::None,
        scripts: find_scripts(dir),
        has_mods_dir: dir.join("mods").is_dir(),
        has_plugins_dir: dir.join("plugins").is_dir(),
        eula_accepted: fs::read_to_string(dir.join("eula.txt"))
            .map(|s| s.lines().any(|l| l.trim().eq_ignore_ascii_case("eula=true")))
            .unwrap_or(false),
        source: String::new(),
    };

    // 1. По имени jar
    for jar in &jars {
        if let Some((core, mc, loader)) = from_jar_name(&file_name(jar)) {
            d.core = core;
            d.mc_version = mc;
            d.loader_version = loader;
            d.target = LaunchTarget::Jar(file_name(jar));
            d.source = format!("имя файла {}", file_name(jar));
            break;
        }
    }

    // 2. libraries/ (NeoForge/Forge). Гибрид, найденный по имени, не перебиваем.
    if !matches!(d.core, CoreKind::Youer | CoreKind::Mohist | CoreKind::Arclight) {
        if let Some((core, mc, loader, args)) = from_libraries(dir) {
            d.core = core;
            if d.mc_version.is_empty() || !mc.is_empty() {
                d.mc_version = mc;
            }
            d.loader_version = loader;
            if let Some(a) = args {
                d.target = LaunchTarget::ArgsFile(a);
            }
            d.source = "папка libraries".into();
        }
    }

    // 3. Содержимое jar
    if d.core == CoreKind::Unknown || d.mc_version.is_empty() {
        for jar in &jars {
            if let Some((core, mc, loader)) = from_jar_contents(jar) {
                if d.core == CoreKind::Unknown {
                    d.core = core;
                    d.target = LaunchTarget::Jar(file_name(jar));
                    d.loader_version = loader;
                    d.source = format!("содержимое {}", file_name(jar));
                }
                if d.mc_version.is_empty() {
                    d.mc_version = mc;
                }
                break;
            }
        }
    }

    // 4. Лог
    if d.mc_version.is_empty() {
        if let Some(v) = from_latest_log(dir) {
            d.mc_version = v;
        }
    }

    // Нашли ядро без jar (бывает у Forge/NeoForge без args-файла) — берём любой подходящий jar
    if d.target == LaunchTarget::None {
        if let Some(j) = jars.iter().find(|j| {
            let n = file_name(j).to_lowercase();
            !n.contains("installer") && !n.starts_with("restic")
        }) {
            d.target = LaunchTarget::Jar(file_name(j));
        }
    }

    // Если ядро не понятно, но есть mods/ или plugins/ — подсказываем хотя бы это
    if d.core == CoreKind::Unknown && d.has_plugins_dir && !d.has_mods_dir {
        d.core = CoreKind::Paper;
        d.source = "есть папка plugins".into();
    }
    d
}

fn find_scripts(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .map(|e| {
                        let e = e.to_string_lossy().to_lowercase();
                        e == "bat" || e == "cmd" || e == "sh"
                    })
                    .unwrap_or(false)
        })
        .map(|p| file_name(&p))
        .collect();
    v.sort();
    v
}

/// Разбор имени jar. Возвращает (ядро, версия MC, версия ядра/загрузчика).
pub fn from_jar_name(name: &str) -> Option<(CoreKind, String, String)> {
    let n = name.to_lowercase();
    if !n.ends_with(".jar") || n.contains("installer") {
        return None;
    }
    let stem = &n[..n.len() - 4];
    let mc_re = Regex::new(r"(\d+\.\d+(?:\.\d+)?)").unwrap();

    let simple = |core: CoreKind, prefix: &str| -> Option<(CoreKind, String, String)> {
        let rest = stem.strip_prefix(prefix)?;
        let rest = rest.trim_start_matches(['-', '_']);
        let mc = mc_re.captures(rest).map(|c| c[1].to_string()).unwrap_or_default();
        let build = rest
            .strip_prefix(mc.as_str())
            .unwrap_or("")
            .trim_start_matches(['-', '_'])
            .to_string();
        Some((core, mc, build))
    };

    // гибриды — первыми
    for (p, k) in [
        ("youer", CoreKind::Youer),
        ("mohist", CoreKind::Mohist),
        ("arclight", CoreKind::Arclight),
        ("purpur", CoreKind::Purpur),
        ("folia", CoreKind::Folia),
        ("paper", CoreKind::Paper),
        ("spigot", CoreKind::Spigot),
    ] {
        if n.starts_with(p) {
            return simple(k, p);
        }
    }

    // fabric-server-mc.1.21.1-loader.0.16.5-launcher.1.0.1.jar
    if let Some(c) = Regex::new(r"fabric-server-mc\.([\d.]+)-loader\.([\d.]+)")
        .unwrap()
        .captures(&n)
    {
        return Some((CoreKind::Fabric, c[1].to_string(), c[2].to_string()));
    }
    if let Some(c) = Regex::new(r"quilt-server-mc\.([\d.]+)-loader\.([\d.]+)")
        .unwrap()
        .captures(&n)
    {
        return Some((CoreKind::Quilt, c[1].to_string(), c[2].to_string()));
    }
    // старый Forge: forge-1.12.2-14.23.5.2860.jar
    if let Some(c) = Regex::new(r"^forge-(\d+\.\d+(?:\.\d+)?)-([\d.]+)(?:-universal)?$")
        .unwrap()
        .captures(stem)
    {
        return Some((CoreKind::Forge, c[1].to_string(), c[2].to_string()));
    }
    // minecraft_server.1.21.1.jar
    if let Some(c) = Regex::new(r"^minecraft_server\.([\d.]+)$").unwrap().captures(stem) {
        return Some((CoreKind::Vanilla, c[1].to_string(), String::new()));
    }
    None
}

/// NeoForge 21.1.77 → Minecraft 1.21.1; 21.0.x → 1.21; 20.4.x → 1.20.4.
pub fn neoforge_to_mc(v: &str) -> String {
    let mut it = v.split('.');
    let (Some(major), Some(minor)) = (it.next(), it.next()) else {
        return String::new();
    };
    let (Ok(major), Ok(minor)) = (major.parse::<u32>(), minor.parse::<u32>()) else {
        return String::new();
    };
    if major >= 25 {
        // новая схема версий Minecraft (26.1 …) — NeoForge повторяет её
        return if minor == 0 { format!("{major}") } else { format!("{major}.{minor}") };
    }
    if minor == 0 {
        format!("1.{major}")
    } else {
        format!("1.{major}.{minor}")
    }
}

fn args_file_name() -> &'static str {
    if cfg!(windows) {
        "win_args.txt"
    } else {
        "unix_args.txt"
    }
}

/// Возвращает (ядро, MC, загрузчик, относительный путь к args-файлу).
fn from_libraries(dir: &Path) -> Option<(CoreKind, String, String, Option<String>)> {
    let newest = |p: PathBuf| -> Option<String> {
        let mut vs: Vec<String> = fs::read_dir(&p)
            .ok()?
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        vs.sort_by(|a, b| version_cmp(a, b));
        vs.pop()
    };

    let neo = dir.join("libraries/net/neoforged/neoforge");
    if let Some(v) = newest(neo.clone()) {
        let rel = format!("libraries/net/neoforged/neoforge/{v}/{}", args_file_name());
        let args = dir.join(&rel).is_file().then_some(rel);
        return Some((CoreKind::NeoForge, neoforge_to_mc(&v), v, args));
    }
    // NeoForge 1.20.1 жил под старым именем пакета forge
    let neo_old = dir.join("libraries/net/neoforged/forge");
    if let Some(v) = newest(neo_old) {
        let (mc, loader) = v.split_once('-').unwrap_or((&v, ""));
        let rel = format!("libraries/net/neoforged/forge/{v}/{}", args_file_name());
        let args = dir.join(&rel).is_file().then_some(rel);
        return Some((CoreKind::NeoForge, mc.into(), loader.into(), args));
    }
    let forge = dir.join("libraries/net/minecraftforge/forge");
    if let Some(v) = newest(forge) {
        let (mc, loader) = v.split_once('-').unwrap_or((&v, ""));
        let rel = format!("libraries/net/minecraftforge/forge/{v}/{}", args_file_name());
        let args = dir.join(&rel).is_file().then_some(rel);
        return Some((CoreKind::Forge, mc.into(), loader.into(), args));
    }
    None
}

/// Сравнение версий «по числам»: 21.1.100 > 21.1.77.
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u64> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter(|x| !x.is_empty())
            .map(|x| x.parse().unwrap_or(0))
            .collect()
    };
    parse(a).cmp(&parse(b))
}

fn read_zip_entry(zip: &mut zip::ZipArchive<fs::File>, name: &str) -> Option<String> {
    let mut f = zip.by_name(name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

/// Смотрит внутрь jar: (ядро, MC, загрузчик).
fn from_jar_contents(jar: &Path) -> Option<(CoreKind, String, String)> {
    let file = fs::File::open(jar).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;

    // Fabric server launcher
    if let Some(p) = read_zip_entry(&mut zip, "install.properties") {
        let get = |k: &str| {
            p.lines()
                .find_map(|l| l.strip_prefix(&format!("{k}=")))
                .unwrap_or("")
                .trim()
                .to_string()
        };
        return Some((CoreKind::Fabric, get("game-version"), get("fabric-loader-version")));
    }
    // Paperclip (Paper/Purpur/Folia)
    if let Some(list) = read_zip_entry(&mut zip, "META-INF/versions.list") {
        // строки «hash\tid\tpath», id вида «1.21.1» или «paper-1.21.1»
        let id = list
            .lines()
            .next()
            .and_then(|l| l.split('\t').nth(1))
            .unwrap_or("")
            .to_string();
        let mc = Regex::new(r"(\d+\.\d+(?:\.\d+)?)")
            .unwrap()
            .captures(&id)
            .map(|c| c[1].to_string())
            .unwrap_or_default();
        let core = if id.contains("purpur") {
            CoreKind::Purpur
        } else if id.contains("folia") {
            CoreKind::Folia
        } else {
            CoreKind::Paper
        };
        return Some((core, mc, String::new()));
    }
    // Ванильный сервер
    if let Some(v) = read_zip_entry(&mut zip, "version.json") {
        if let Ok(j) = serde_json::from_str::<serde_json::Value>(&v) {
            let id = j.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let is_spigot = zip.by_name("org/bukkit/Bukkit.class").is_ok();
            return Some((
                if is_spigot { CoreKind::Spigot } else { CoreKind::Vanilla },
                id,
                String::new(),
            ));
        }
    }
    None
}

fn from_latest_log(dir: &Path) -> Option<String> {
    let s = fs::read_to_string(dir.join("logs/latest.log")).ok()?;
    let re = Regex::new(r"Starting minecraft server version ([\w.\-]+)").unwrap();
    s.lines().take(400).find_map(|l| re.captures(l).map(|c| c[1].to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jar_names() {
        assert_eq!(
            from_jar_name("youer-1.21.1-3068bd4.jar"),
            Some((CoreKind::Youer, "1.21.1".into(), "3068bd4".into()))
        );
        assert_eq!(
            from_jar_name("paper-1.21.1-130.jar"),
            Some((CoreKind::Paper, "1.21.1".into(), "130".into()))
        );
        assert_eq!(
            from_jar_name("fabric-server-mc.1.21.1-loader.0.16.5-launcher.1.0.1.jar"),
            Some((CoreKind::Fabric, "1.21.1".into(), "0.16.5".into()))
        );
        assert_eq!(from_jar_name("neoforge-21.1.77-installer.jar"), None);
        assert_eq!(from_jar_name("random.jar"), None);
    }

    #[test]
    fn neo_versions() {
        assert_eq!(neoforge_to_mc("21.1.77"), "1.21.1");
        assert_eq!(neoforge_to_mc("21.0.167"), "1.21");
        assert_eq!(neoforge_to_mc("20.4.237"), "1.20.4");
        assert_eq!(neoforge_to_mc("26.1.0.5"), "26.1");
    }

    #[test]
    fn version_sort() {
        assert_eq!(version_cmp("21.1.100", "21.1.77"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn detect_neoforge_folder() {
        let t = tempfile::tempdir().unwrap();
        let v = t.path().join("libraries/net/neoforged/neoforge/21.1.77");
        fs::create_dir_all(&v).unwrap();
        fs::write(v.join(args_file_name()), "").unwrap();
        fs::create_dir_all(t.path().join("mods")).unwrap();
        fs::write(t.path().join("eula.txt"), "eula=true\n").unwrap();
        fs::write(t.path().join("run.bat"), "").unwrap();
        let d = detect(t.path());
        assert_eq!(d.core, CoreKind::NeoForge);
        assert_eq!(d.mc_version, "1.21.1");
        assert!(matches!(d.target, LaunchTarget::ArgsFile(_)));
        assert!(d.eula_accepted && d.has_mods_dir);
        assert_eq!(d.scripts, vec!["run.bat".to_string()]);
    }

    #[test]
    fn detect_youer_beats_libraries() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("libraries/net/neoforged/neoforge/21.1.77")).unwrap();
        fs::write(t.path().join("youer-1.21.1-3068bd4.jar"), "").unwrap();
        let d = detect(t.path());
        assert_eq!(d.core, CoreKind::Youer);
        assert_eq!(d.target, LaunchTarget::Jar("youer-1.21.1-3068bd4.jar".into()));
        assert!(d.core.supports_mods() && d.core.supports_plugins());
    }
}
