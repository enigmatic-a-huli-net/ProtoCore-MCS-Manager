//! Плагины Bukkit / Spigot / Paper (и гибридов вроде Youer).
//!
//! Читаем `plugin.yml` или `paper-plugin.yml` из jar.
//! Отключённые плагины лежат в `plugins_disabled/` — папки с данными плагинов
//! (plugins/ИмяПлагина/) не трогаются, настройки не теряются.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_yaml::Value;

use crate::mods::{read_reasons, DisableReason};
use crate::util::{self, file_name, list_jars};
use crate::Result;

pub const ENABLED_DIR: &str = "plugins";
pub const DISABLED_DIR: &str = "plugins_disabled";

#[derive(Debug, Clone, Serialize, Default)]
pub struct PluginInfo {
    pub file: String,
    pub enabled: bool,
    pub size: u64,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub main: String,
    pub api_version: String,
    pub website: String,
    pub depend: Vec<String>,
    pub softdepend: Vec<String>,
    pub paper_plugin: bool,
    /// Плагин сам по себе лишь «оболочка» для мода (бывает у гибридов) — для информации
    pub folia_supported: bool,
    pub icon: Option<String>,
    pub required_by: Vec<String>,
    pub missing_deps: Vec<String>,
    pub problems: Vec<String>,
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginReport {
    pub plugins: Vec<PluginInfo>,
    pub missing: Vec<crate::mods::MissingDep>,
    pub enabled_count: usize,
    pub disabled_count: usize,
}

fn str_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Sequence(s)) => s.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => vec![],
    }
}

fn s(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

pub fn parse_plugin_yml(text: &str, info: &mut PluginInfo) {
    let Ok(v) = serde_yaml::from_str::<Value>(text) else {
        info.problems.push("plugin.yml не читается".into());
        return;
    };
    info.name = s(v.get("name"));
    info.version = s(v.get("version"));
    info.description = s(v.get("description"));
    info.main = s(v.get("main"));
    info.api_version = s(v.get("api-version"));
    info.website = s(v.get("website"));
    info.folia_supported = v.get("folia-supported").and_then(Value::as_bool).unwrap_or(false);
    info.authors = str_list(v.get("authors"));
    let author = s(v.get("author"));
    if !author.is_empty() && !info.authors.contains(&author) {
        info.authors.insert(0, author);
    }
    info.depend = str_list(v.get("depend"));
    info.softdepend = str_list(v.get("softdepend"));

    // paper-plugin.yml: dependencies: server: { Name: { required: true } }
    if let Some(Value::Mapping(server)) = v.get("dependencies").and_then(|d| d.get("server")) {
        for (k, val) in server {
            let Some(name) = k.as_str() else { continue };
            let required = val.get("required").and_then(Value::as_bool).unwrap_or(true);
            if required {
                info.depend.push(name.to_string());
            } else {
                info.softdepend.push(name.to_string());
            }
        }
    }
}

fn read_plugin(path: &Path, enabled: bool) -> PluginInfo {
    let mut info = PluginInfo {
        file: file_name(path),
        enabled,
        size: fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        ..Default::default()
    };
    let Ok(f) = fs::File::open(path) else { return info };
    let Ok(mut zip) = zip::ZipArchive::new(f) else {
        info.problems.push("jar повреждён".into());
        return info;
    };
    let mut read = |n: &str| -> Option<String> {
        let mut e = zip.by_name(n).ok()?;
        let mut s = String::new();
        e.read_to_string(&mut s).ok()?;
        Some(s)
    };
    if let Some(t) = read("paper-plugin.yml") {
        info.paper_plugin = true;
        parse_plugin_yml(&t, &mut info);
    } else if let Some(t) = read("plugin.yml") {
        parse_plugin_yml(&t, &mut info);
    } else if read("fabric.mod.json").is_some()
        || read("META-INF/neoforge.mods.toml").is_some()
        || read("META-INF/mods.toml").is_some()
    {
        info.problems.push("это мод, а не плагин — его место в папке mods".into());
    } else {
        info.problems.push("нет plugin.yml — это не плагин".into());
    }
    // иконки у плагинов не стандартизованы; иногда кладут icon.png или logo.png
    for n in ["icon.png", "logo.png", "assets/icon.png"] {
        if let Ok(mut e) = zip.by_name(n) {
            let mut b = Vec::new();
            if e.read_to_end(&mut b).is_ok() {
                info.icon = util::image_to_data_uri(&b, 64);
                break;
            }
        }
    }
    if info.name.is_empty() {
        info.name = info.file.trim_end_matches(".jar").to_string();
    }
    info
}

pub fn list(server_dir: &Path) -> PluginReport {
    let en = server_dir.join(ENABLED_DIR);
    let dis = server_dir.join(DISABLED_DIR);
    let reasons = read_reasons(&dis);
    let mut plugins: Vec<PluginInfo> = list_jars(&en)
        .iter()
        .map(|p| read_plugin(p, true))
        .chain(list_jars(&dis).iter().map(|p| {
            let mut i = read_plugin(p, false);
            i.disabled_reason = Some(reasons.get(&i.file).map(|r| r.reason.clone()).unwrap_or_else(|| "user".into()));
            i
        }))
        .collect();

    let available: HashMap<String, String> = plugins
        .iter()
        .filter(|p| p.enabled)
        .map(|p| (p.name.to_lowercase(), p.file.clone()))
        .collect();
    let disabled: HashMap<String, String> = plugins
        .iter()
        .filter(|p| !p.enabled)
        .map(|p| (p.name.to_lowercase(), p.file.clone()))
        .collect();

    let mut missing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut required_by: HashMap<String, Vec<String>> = HashMap::new();
    for p in plugins.iter_mut().filter(|p| p.enabled) {
        for d in &p.depend {
            match available.get(&d.to_lowercase()) {
                Some(f) => required_by.entry(f.clone()).or_default().push(p.name.clone()),
                None => {
                    p.missing_deps.push(d.clone());
                    missing.entry(d.clone()).or_default().push(p.name.clone());
                }
            }
        }
    }
    // дубликаты по имени
    let mut seen = HashSet::new();
    let dups: HashSet<String> = plugins
        .iter()
        .filter(|p| p.enabled)
        .filter(|p| !seen.insert(p.name.to_lowercase()))
        .map(|p| p.name.to_lowercase())
        .collect();
    for p in plugins.iter_mut() {
        if let Some(r) = required_by.get(&p.file) {
            p.required_by = r.clone();
        }
        if p.enabled && dups.contains(&p.name.to_lowercase()) {
            p.problems.push("плагин с таким именем установлен дважды".into());
        }
    }
    plugins.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    PluginReport {
        enabled_count: plugins.iter().filter(|p| p.enabled).count(),
        disabled_count: plugins.iter().filter(|p| !p.enabled).count(),
        missing: missing
            .into_iter()
            .map(|(id, needed_by)| crate::mods::MissingDep {
                disabled_file: disabled.get(&id.to_lowercase()).cloned(),
                id,
                needed_by,
            })
            .collect(),
        plugins,
    }
}

pub fn set_enabled(server_dir: &Path, file: &str, enable: bool, reason: &str) -> Result<PathBuf> {
    let en = server_dir.join(ENABLED_DIR);
    let dis = server_dir.join(DISABLED_DIR);
    let (from, to) = if enable { (&dis, &en) } else { (&en, &dis) };
    let src = from.join(file);
    if !src.is_file() {
        return Err(crate::Error::other(format!("файл не найден: {}", src.display())));
    }
    let dst = util::move_file(&src, to)?;
    let mut reasons = read_reasons(&dis);
    if enable {
        reasons.remove(file);
    } else {
        reasons.insert(
            file_name(&dst),
            DisableReason { reason: reason.into(), detail: String::new(), ts: chrono::Utc::now().timestamp() },
        );
    }
    fs::create_dir_all(&dis)?;
    fs::write(dis.join(".pcm-reasons.json"), serde_json::to_string_pretty(&reasons)?)?;
    Ok(dst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yml() {
        let mut i = PluginInfo::default();
        parse_plugin_yml(
            "name: Essentials\nversion: 2.20\nmain: a.B\nauthor: x\nauthors: [y]\ndepend: [Vault]\nsoftdepend: LuckPerms\n",
            &mut i,
        );
        assert_eq!(i.name, "Essentials");
        assert_eq!(i.version, "2.2");
        assert_eq!(i.authors, vec!["x", "y"]);
        assert_eq!(i.depend, vec!["Vault"]);
        assert_eq!(i.softdepend, vec!["LuckPerms"]);
    }

    #[test]
    fn paper_yml() {
        let mut i = PluginInfo::default();
        parse_plugin_yml(
            "name: P\nversion: '1'\nmain: a\ndependencies:\n  server:\n    Vault:\n      required: true\n    PAPI:\n      required: false\n",
            &mut i,
        );
        assert_eq!(i.depend, vec!["Vault"]);
        assert_eq!(i.softdepend, vec!["PAPI"]);
    }
}
