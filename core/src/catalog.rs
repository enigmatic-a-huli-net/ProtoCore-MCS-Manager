//! Каталог модов и плагинов: поиск, установка с зависимостями, обновления.
//!
//! Источники:
//! - **Modrinth** — моды и плагины, без ключа. Обновления ищутся по SHA-1 установленных jar.
//! - **Hangar** (PaperMC) — плагины Paper.
//! - **CurseForge** — моды и плагины Bukkit, только с API-ключом (их правило).
//!   Если автор запретил стороннее скачивание, файл не качается — открывается страница.
//!
//! Старые версии при обновлении не удаляются, а переносятся в `mods_old/` / `plugins_old/`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::ServerEntry;
use crate::detect::CoreKind;
use crate::net;
use crate::util::{file_name, list_jars};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Modrinth,
    Hangar,
    Curseforge,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mod,
    Plugin,
}

/// Куда ставим: папка, загрузчики, версия игры.
#[derive(Debug, Clone)]
pub struct Target {
    pub kind: Kind,
    pub core: CoreKind,
    pub loaders: Vec<String>,
    pub game_version: String,
    pub dir: PathBuf,
    pub old_dir: PathBuf,
    pub cf_key: String,
}

impl Target {
    pub fn for_server(s: &ServerEntry, kind: Kind, cf_key: &str) -> Self {
        let loaders: Vec<&str> = match kind {
            Kind::Mod => match s.core {
                CoreKind::Fabric => vec!["fabric"],
                CoreKind::Quilt => vec!["quilt", "fabric"],
                CoreKind::Forge | CoreKind::Mohist => vec!["forge"],
                CoreKind::NeoForge | CoreKind::Youer | CoreKind::Arclight => vec!["neoforge"],
                _ => vec!["neoforge", "forge", "fabric"],
            },
            Kind::Plugin => match s.core {
                CoreKind::Folia => vec!["folia"],
                CoreKind::Purpur => vec!["purpur", "paper", "spigot", "bukkit"],
                CoreKind::Spigot => vec!["spigot", "bukkit"],
                _ => vec!["paper", "spigot", "bukkit"],
            },
        };
        let (d, o) = match kind {
            Kind::Mod => ("mods", "mods_old"),
            Kind::Plugin => ("plugins", "plugins_old"),
        };
        Self {
            kind,
            core: s.core,
            loaders: loaders.into_iter().map(String::from).collect(),
            game_version: s.mc_version.clone(),
            dir: s.dir().join(d),
            old_dir: s.dir().join(o),
            cf_key: clean_cf_key(cf_key),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub source: Source,
    pub id: String,
    pub slug: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub icon_url: String,
    pub downloads: u64,
    pub url: String,
    /// required / optional / unsupported (только Modrinth)
    pub server_side: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub items: Vec<Project>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepRef {
    pub project_id: String,
    pub required: bool,
    /// для Hangar зависимость указана по имени
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRef {
    pub source: Source,
    pub project_id: String,
    pub version_id: String,
    pub version_name: String,
    pub version_number: String,
    pub file_name: String,
    /// пусто — скачать можно только со страницы
    pub url: String,
    pub sha1: Option<String>,
    pub size: u64,
    pub date: String,
    pub channel: String,
    pub deps: Vec<DepRef>,
    pub incompatible: Vec<String>,
    pub page_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    pub title: String,
    pub icon_url: String,
    pub file: FileRef,
    /// selected / dependency / update / fix
    pub reason: String,
    /// имя старого файла, который будет перенесён в *_old
    pub replaces: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Plan {
    pub items: Vec<PlanItem>,
    /// зависимости, которые уже стоят
    pub already: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateItem {
    pub file: String,
    pub title: String,
    pub icon_url: String,
    pub current: String,
    pub latest: FileRef,
    /// Новая версия — бета/альфа, а стоит релиз (по умолчанию не отмечается)
    pub prerelease: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdatesReport {
    pub updates: Vec<UpdateItem>,
    /// jar, которых нет на Modrinth (обновлять вручную)
    pub unknown: Vec<String>,
    pub checked: usize,
}

// ===================================================================== общие мелочи

fn s(v: &Value) -> String {
    match v {
        Value::String(x) => x.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}
fn arr(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}
fn enc(x: &str) -> String {
    let mut out = String::new();
    for b in x.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// SHA-1 с кэшем по (имя, размер, время) — 200+ модов не пересчитываются каждый раз.
fn cached_sha1(p: &Path) -> Option<String> {
    static C: OnceLock<Mutex<HashMap<(String, u64, u64), String>>> = OnceLock::new();
    let md = std::fs::metadata(p).ok()?;
    let mt = md.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let key = (p.to_string_lossy().to_string(), md.len(), mt);
    let c = C.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(h) = c.lock().unwrap().get(&key) {
        return Some(h.clone());
    }
    let h = net::sha1_file(p).ok()?;
    c.lock().unwrap().insert(key, h.clone());
    Some(h)
}

/// Что из Modrinth уже установлено: project_id → файл. Смотрим и в выключенных.
fn installed_modrinth(t: &Target) -> HashMap<String, String> {
    let disabled = t.dir.with_file_name(format!("{}_disabled", file_name(&t.dir)));
    let mut by_hash: HashMap<String, String> = HashMap::new();
    for j in list_jars(&t.dir).into_iter().chain(list_jars(&disabled)) {
        if let Some(h) = cached_sha1(&j) {
            by_hash.insert(h, file_name(&j));
        }
    }
    if by_hash.is_empty() {
        return HashMap::new();
    }
    let hashes: Vec<&String> = by_hash.keys().collect();
    let Ok(v) = net::post_json("https://api.modrinth.com/v2/version_files", &json!({"hashes": hashes, "algorithm": "sha1"})) else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    if let Value::Object(map) = v {
        for (h, ver) in map {
            if let Some(f) = by_hash.get(&h) {
                out.insert(s(&ver["project_id"]), f.clone());
            }
        }
    }
    out
}

/// id установленных модов/плагинов из метаданных (для сравнения со slug / названием).
fn installed_ids(t: &Target) -> HashSet<String> {
    let mut set = HashSet::new();
    match t.kind {
        Kind::Mod => {
            for j in list_jars(&t.dir) {
                let m = crate::mods::read_meta(&j);
                for id in m.ids.iter().chain(m.provides.iter()) {
                    set.insert(id.to_lowercase());
                    set.insert(id.to_lowercase().replace('_', "-"));
                }
            }
        }
        Kind::Plugin => {
            let srv = t.dir.parent().unwrap_or(Path::new("."));
            for p in crate::plugins::list(srv).plugins.iter().filter(|p| p.enabled) {
                set.insert(p.name.to_lowercase());
            }
        }
    }
    set
}

// ===================================================================== поиск

pub fn search(t: &Target, source: Source, query: &str, sort: &str, offset: u32) -> Result<SearchResult> {
    let mut r = match source {
        Source::Modrinth => mr_search(t, query, sort, offset)?,
        Source::Hangar => hangar_search(t, query, sort, offset)?,
        Source::Curseforge => cf_search(t, query, sort, offset)?,
    };
    // отметить установленные
    let ids = installed_ids(t);
    let mr = if source == Source::Modrinth { installed_modrinth(t) } else { HashMap::new() };
    for p in &mut r.items {
        p.installed = mr.contains_key(&p.id)
            || ids.contains(&p.slug.to_lowercase())
            || ids.contains(&p.slug.to_lowercase().replace('-', "_"))
            || (t.kind == Kind::Plugin && ids.contains(&p.title.to_lowercase()));
    }
    Ok(r)
}

fn mr_search(t: &Target, query: &str, sort: &str, offset: u32) -> Result<SearchResult> {
    let ptype = if t.kind == Kind::Mod { "mod" } else { "plugin" };
    let mut facets: Vec<Vec<String>> = vec![vec![format!("project_type:{ptype}")]];
    facets.push(t.loaders.iter().map(|l| format!("categories:{l}")).collect());
    if !t.game_version.is_empty() {
        facets.push(vec![format!("versions:{}", t.game_version)]);
    }
    if t.kind == Kind::Mod {
        facets.push(vec!["server_side:required".into(), "server_side:optional".into()]);
    }
    let index = match sort {
        "downloads" => "downloads",
        "updated" => "updated",
        "newest" => "newest",
        _ => "relevance",
    };
    let url = format!(
        "https://api.modrinth.com/v2/search?query={}&facets={}&index={index}&offset={offset}&limit=20",
        enc(query),
        enc(&serde_json::to_string(&facets)?)
    );
    let j = net::get_json(&url)?;
    let items = arr(&j["hits"])
        .iter()
        .map(|h| Project {
            source: Source::Modrinth,
            id: s(&h["project_id"]),
            slug: s(&h["slug"]),
            title: s(&h["title"]),
            author: s(&h["author"]),
            description: s(&h["description"]),
            icon_url: s(&h["icon_url"]),
            downloads: h["downloads"].as_u64().unwrap_or(0),
            url: format!("https://modrinth.com/{}/{}", s(&h["project_type"]), s(&h["slug"])),
            server_side: s(&h["server_side"]),
            installed: false,
        })
        .collect();
    Ok(SearchResult { items, total: j["total_hits"].as_u64().unwrap_or(0) })
}

fn hangar_search(t: &Target, query: &str, sort: &str, offset: u32) -> Result<SearchResult> {
    let sort = match sort {
        "downloads" => "-downloads",
        "updated" => "-updated",
        "newest" => "-newest",
        _ => "-stars",
    };
    let mut url = format!(
        "https://hangar.papermc.io/api/v1/projects?limit=20&offset={offset}&sort={sort}&platform=PAPER&q={}",
        enc(query)
    );
    if !t.game_version.is_empty() {
        url.push_str(&format!("&version={}", enc(&t.game_version)));
    }
    let j = net::get_json(&url)?;
    let items = arr(&j["result"])
        .iter()
        .map(|p| {
            let owner = s(&p["namespace"]["owner"]);
            let slug = s(&p["namespace"]["slug"]);
            Project {
                source: Source::Hangar,
                id: slug.clone(),
                slug: slug.clone(),
                title: s(&p["name"]),
                author: owner.clone(),
                description: s(&p["description"]),
                icon_url: s(&p["avatarUrl"]),
                downloads: p["stats"]["downloads"].as_u64().unwrap_or(0),
                url: format!("https://hangar.papermc.io/{owner}/{slug}"),
                server_side: "required".into(),
                installed: false,
            }
        })
        .collect();
    Ok(SearchResult { items, total: j["pagination"]["count"].as_u64().unwrap_or(0) })
}

/// Чистит ключ: кавычки, пробелы, переносы, невидимые символы, приставку «x-api-key:».
pub fn clean_cf_key(k: &str) -> String {
    let k = k.trim();
    let k = k
        .strip_prefix("x-api-key:")
        .or_else(|| k.strip_prefix("X-Api-Key:"))
        .unwrap_or(k);
    k.chars()
        .filter(|c| c.is_ascii_graphic() && !matches!(c, '"' | '\'' | '`'))
        .collect()
}

/// Проверка ключа: простой запрос к API. Ok — ключ принят.
pub fn check_cf_key(key: &str) -> Result<String> {
    let key = clean_cf_key(key);
    if key.is_empty() {
        return Err(Error::other("ключ пустой"));
    }
    match net::get_json_h("https://api.curseforge.com/v1/games/432", &[("x-api-key", &key)]) {
        Ok(j) => Ok(s(&j["data"]["name"])),
        Err(e) if e.to_string().contains("403") => Err(Error::other(format!(
            "CurseForge не принял ключ (403). Нужен ключ из console.curseforge.com → API Keys — он начинается с «$2a$10$» и длиной около 60 символов. Сейчас: {} символов, начало «{}…», конец «…{}». Ответ сервера: {}",
            key.len(),
            key.chars().take(7).collect::<String>(),
            key.chars().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<String>(),
            e.to_string().rsplit("HTTP 403").next().unwrap_or("").trim()
        ))),
        Err(e) => Err(e),
    }
}

fn cf_need_key(t: &Target) -> Result<()> {
    if t.cf_key.is_empty() {
        return Err(Error::other("для CurseForge нужен API-ключ — укажи его в настройках менеджера"));
    }
    Ok(())
}

fn cf_loader(t: &Target) -> Option<u32> {
    if t.kind == Kind::Plugin {
        return None;
    }
    match t.loaders.first().map(String::as_str) {
        Some("forge") => Some(1),
        Some("fabric") => Some(4),
        Some("quilt") => Some(5),
        Some("neoforge") => Some(6),
        _ => None,
    }
}

fn cf_get(t: &Target, path: &str) -> Result<Value> {
    cf_need_key(t)?;
    net::get_json_h(&format!("https://api.curseforge.com{path}"), &[("x-api-key", &t.cf_key)])
}

fn cf_search(t: &Target, query: &str, sort: &str, offset: u32) -> Result<SearchResult> {
    let class = if t.kind == Kind::Mod { 6 } else { 5 };
    let sort_field = match sort {
        "downloads" => 6,
        "updated" => 3,
        "newest" => 11,
        _ => 2,
    };
    let mut path = format!(
        "/v1/mods/search?gameId=432&classId={class}&searchFilter={}&sortField={sort_field}&sortOrder=desc&index={offset}&pageSize=20",
        enc(query)
    );
    if !t.game_version.is_empty() {
        path.push_str(&format!("&gameVersion={}", enc(&t.game_version)));
    }
    if let Some(l) = cf_loader(t) {
        path.push_str(&format!("&modLoaderType={l}"));
    }
    let j = cf_get(t, &path)?;
    let items = arr(&j["data"])
        .iter()
        .map(|m| Project {
            source: Source::Curseforge,
            id: s(&m["id"]),
            slug: s(&m["slug"]),
            title: s(&m["name"]),
            author: arr(&m["authors"]).first().map(|a| s(&a["name"])).unwrap_or_default(),
            description: s(&m["summary"]),
            icon_url: s(&m["logo"]["thumbnailUrl"]),
            downloads: m["downloadCount"].as_f64().unwrap_or(0.0) as u64,
            url: s(&m["links"]["websiteUrl"]),
            server_side: String::new(),
            installed: false,
        })
        .collect();
    Ok(SearchResult { items, total: j["pagination"]["totalCount"].as_u64().unwrap_or(0) })
}

// ===================================================================== версии

pub fn versions(t: &Target, source: Source, project: &str) -> Result<Vec<FileRef>> {
    match source {
        Source::Modrinth => mr_versions(t, project),
        Source::Hangar => hangar_versions(t, project),
        Source::Curseforge => cf_versions(t, project),
    }
}

fn mr_file_ref(v: &Value) -> Option<FileRef> {
    let files = arr(&v["files"]);
    let f = files.iter().find(|f| f["primary"].as_bool() == Some(true)).or(files.first())?;
    let mut deps = Vec::new();
    let mut incompatible = Vec::new();
    for d in arr(&v["dependencies"]) {
        let pid = s(&d["project_id"]);
        match d["dependency_type"].as_str() {
            Some("required") if !pid.is_empty() => deps.push(DepRef { project_id: pid, required: true, name: String::new() }),
            Some("incompatible") if !pid.is_empty() => incompatible.push(pid),
            _ => {}
        }
    }
    Some(FileRef {
        source: Source::Modrinth,
        project_id: s(&v["project_id"]),
        version_id: s(&v["id"]),
        version_name: s(&v["name"]),
        version_number: s(&v["version_number"]),
        file_name: net::safe_file_name(&s(&f["filename"])),
        url: s(&f["url"]),
        sha1: Some(s(&f["hashes"]["sha1"])).filter(|x| !x.is_empty()),
        size: f["size"].as_u64().unwrap_or(0),
        date: s(&v["date_published"]),
        channel: s(&v["version_type"]),
        deps,
        incompatible,
        page_url: format!("https://modrinth.com/project/{}/version/{}", s(&v["project_id"]), s(&v["id"])),
    })
}

fn mr_versions(t: &Target, project: &str) -> Result<Vec<FileRef>> {
    let mut url = format!(
        "https://api.modrinth.com/v2/project/{}/version?loaders={}",
        enc(project),
        enc(&serde_json::to_string(&t.loaders)?)
    );
    if !t.game_version.is_empty() {
        url.push_str(&format!("&game_versions={}", enc(&serde_json::to_string(&[&t.game_version])?)));
    }
    let j = net::get_json(&url)?;
    Ok(arr(&j).iter().filter_map(mr_file_ref).collect())
}

fn hangar_versions(t: &Target, slug: &str) -> Result<Vec<FileRef>> {
    let mut url = format!("https://hangar.papermc.io/api/v1/projects/{}/versions?limit=25&offset=0&platform=PAPER", enc(slug));
    if !t.game_version.is_empty() {
        url.push_str(&format!("&platformVersion={}", enc(&t.game_version)));
    }
    let j = net::get_json(&url)?;
    let mut out = Vec::new();
    for v in arr(&j["result"]) {
        let name = s(&v["name"]);
        let d = &v["downloads"]["PAPER"];
        let fi = &d["fileInfo"];
        let direct = s(&d["downloadUrl"]);
        let url = if !direct.is_empty() {
            direct
        } else if s(&d["externalUrl"]).is_empty() {
            format!("https://hangar.papermc.io/api/v1/projects/{}/versions/{}/PAPER/download", enc(slug), enc(&name))
        } else {
            String::new() // внешняя ссылка — только вручную
        };
        let deps = arr(&v["pluginDependencies"]["PAPER"])
            .iter()
            .filter(|d| d["required"].as_bool().unwrap_or(false))
            .map(|d| DepRef { project_id: String::new(), required: true, name: s(&d["name"]) })
            .collect();
        let file = s(&fi["name"]);
        out.push(FileRef {
            source: Source::Hangar,
            project_id: slug.to_string(),
            version_id: name.clone(),
            version_name: name.clone(),
            version_number: name.clone(),
            file_name: net::safe_file_name(if file.is_empty() { &name } else { &file }),
            url,
            sha1: None,
            size: fi["sizeBytes"].as_u64().unwrap_or(0),
            date: s(&v["createdAt"]),
            channel: s(&v["channel"]["name"]).to_lowercase(),
            deps,
            incompatible: vec![],
            page_url: format!("https://hangar.papermc.io/{}/versions/{}", s(&v["author"]), enc(&name)),
        });
    }
    // некоторые версии API не умеют фильтр platformVersion — тогда фильтруем сами
    let _ = t;
    Ok(out)
}

fn cf_versions(t: &Target, project: &str) -> Result<Vec<FileRef>> {
    let mut path = format!("/v1/mods/{}/files?pageSize=30", enc(project));
    if !t.game_version.is_empty() {
        path.push_str(&format!("&gameVersion={}", enc(&t.game_version)));
    }
    if let Some(l) = cf_loader(t) {
        path.push_str(&format!("&modLoaderType={l}"));
    }
    let j = cf_get(t, &path)?;
    let mut out: Vec<FileRef> = arr(&j["data"])
        .iter()
        .map(|f| {
            let sha1 = arr(&f["hashes"]).iter().find(|h| h["algo"].as_u64() == Some(1)).map(|h| s(&h["value"]));
            let mut deps = Vec::new();
            let mut inc = Vec::new();
            for d in arr(&f["dependencies"]) {
                match d["relationType"].as_u64() {
                    Some(3) => deps.push(DepRef { project_id: s(&d["modId"]), required: true, name: String::new() }),
                    Some(5) => inc.push(s(&d["modId"])),
                    _ => {}
                }
            }
            FileRef {
                source: Source::Curseforge,
                project_id: project.to_string(),
                version_id: s(&f["id"]),
                version_name: s(&f["displayName"]),
                version_number: s(&f["displayName"]),
                file_name: net::safe_file_name(&s(&f["fileName"])),
                url: s(&f["downloadUrl"]), // null — автор запретил стороннее скачивание
                sha1,
                size: f["fileLength"].as_u64().unwrap_or(0),
                date: s(&f["fileDate"]),
                channel: match f["releaseType"].as_u64() {
                    Some(1) => "release".into(),
                    Some(2) => "beta".into(),
                    _ => "alpha".into(),
                },
                deps,
                incompatible: inc,
                page_url: format!("https://www.curseforge.com/minecraft/mc-mods/{project}/files/{}", s(&f["id"])),
            }
        })
        .collect();
    out.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(out)
}

/// Лучшая версия: последний релиз, иначе последняя бета, иначе что есть.
fn best(list: &[FileRef]) -> Option<FileRef> {
    list.iter()
        .find(|v| v.channel == "release")
        .or_else(|| list.iter().find(|v| v.channel == "beta"))
        .or(list.first())
        .cloned()
}

// ===================================================================== план установки

struct Meta {
    title: String,
    slug: String,
    icon: String,
}

fn project_meta(t: &Target, source: Source, ids: &[String]) -> HashMap<String, Meta> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return out;
    }
    match source {
        Source::Modrinth => {
            let url = format!("https://api.modrinth.com/v2/projects?ids={}", enc(&serde_json::to_string(ids).unwrap_or_default()));
            if let Ok(j) = net::get_json(&url) {
                for p in arr(&j) {
                    out.insert(s(&p["id"]), Meta { title: s(&p["title"]), slug: s(&p["slug"]), icon: s(&p["icon_url"]) });
                }
            }
        }
        Source::Curseforge => {
            for id in ids {
                if let Ok(j) = cf_get(t, &format!("/v1/mods/{id}")) {
                    let m = &j["data"];
                    out.insert(id.clone(), Meta { title: s(&m["name"]), slug: s(&m["slug"]), icon: s(&m["logo"]["thumbnailUrl"]) });
                }
            }
        }
        Source::Hangar => {
            for id in ids {
                out.insert(id.clone(), Meta { title: id.clone(), slug: id.clone(), icon: String::new() });
            }
        }
    }
    out
}

/// Установка проекта: выбранная версия (или лучшая) + все обязательные зависимости, которых нет.
pub fn plan_install(t: &Target, source: Source, project: &str, version_id: Option<&str>) -> Result<Plan> {
    let mut plan = Plan::default();
    let installed_mr = if source == Source::Modrinth { installed_modrinth(t) } else { HashMap::new() };
    let ids = installed_ids(t);
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, Option<String>, String)> = VecDeque::new(); // (project, version, reason)
    queue.push_back((project.to_string(), version_id.map(String::from), "selected".into()));

    while let Some((pid, ver, reason)) = queue.pop_front() {
        if !seen.insert(pid.clone()) || plan.items.len() > 40 {
            continue;
        }
        let meta = project_meta(t, source, &[pid.clone()]).remove(&pid);
        let title = meta.as_ref().map(|m| m.title.clone()).unwrap_or_else(|| pid.clone());
        let slug = meta.as_ref().map(|m| m.slug.to_lowercase()).unwrap_or_default();

        let already = installed_mr.get(&pid).cloned().or_else(|| {
            (ids.contains(&slug) || ids.contains(&slug.replace('-', "_")) || ids.contains(&title.to_lowercase()))
                .then(|| title.clone())
        });
        if reason == "dependency" {
            if let Some(f) = already {
                plan.already.push(format!("{title} ({f})"));
                continue;
            }
        }

        let list = versions(t, source, &pid)?;
        let pick = match &ver {
            Some(v) => list.iter().find(|x| &x.version_id == v).cloned(),
            None => best(&list),
        };
        let Some(file) = pick else {
            let msg = format!("«{title}»: нет версии для {} {}", t.loaders.join("/"), t.game_version);
            if reason == "selected" {
                return Err(Error::other(msg));
            }
            plan.warnings.push(msg);
            continue;
        };
        if file.url.is_empty() {
            plan.warnings.push(format!("«{title}»: автор разрешает скачивание только с сайта — открой страницу и скачай вручную"));
        }
        for inc in &file.incompatible {
            if installed_mr.contains_key(inc) {
                plan.warnings.push(format!("«{title}» несовместим с уже установленным модом ({})", installed_mr[inc]));
            }
        }
        for d in file.deps.iter().filter(|d| d.required) {
            if source == Source::Hangar {
                // у Hangar зависимости по имени плагина
                if !ids.contains(&d.name.to_lowercase()) {
                    plan.warnings.push(format!("«{title}» требует плагин «{}» — найди его в каталоге", d.name));
                }
                continue;
            }
            queue.push_back((d.project_id.clone(), None, "dependency".into()));
        }
        // переустановка того же проекта = обновление
        let replaces = installed_mr.get(&pid).cloned().filter(|f| f != &file.file_name);
        plan.items.push(PlanItem {
            title,
            icon_url: meta.map(|m| m.icon).unwrap_or_default(),
            replaces,
            reason,
            file,
        });
    }
    Ok(plan)
}

/// Выполнить план. Возвращает имена установленных файлов.
pub fn apply(t: &Target, plan: &Plan, mut progress: impl FnMut(&str, u64, u64)) -> Result<Vec<String>> {
    std::fs::create_dir_all(&t.dir)?;
    let mut done = Vec::new();
    for it in &plan.items {
        if it.file.url.is_empty() {
            continue;
        }
        let dst = t.dir.join(&it.file.file_name);
        let title = it.title.clone();
        net::download(&it.file.url, &dst, it.file.sha1.as_deref(), |a, b| progress(&title, a, b))?;
        if let Some(old) = &it.replaces {
            let p = t.dir.join(old);
            if p.is_file() && p != dst {
                crate::util::move_file(&p, &t.old_dir)?;
            }
        }
        done.push(it.file.file_name.clone());
    }
    Ok(done)
}

// ===================================================================== обновления

/// Проверка обновлений через Modrinth: по SHA-1 каждого jar.
pub fn check_updates(t: &Target) -> Result<UpdatesReport> {
    let jars = list_jars(&t.dir);
    let mut by_hash: HashMap<String, String> = HashMap::new();
    for j in &jars {
        if let Some(h) = cached_sha1(j) {
            by_hash.insert(h, file_name(j));
        }
    }
    let hashes: Vec<&String> = by_hash.keys().collect();
    if hashes.is_empty() {
        return Ok(UpdatesReport { updates: vec![], unknown: vec![], checked: 0 });
    }
    let current = net::post_json("https://api.modrinth.com/v2/version_files", &json!({"hashes": hashes, "algorithm": "sha1"}))?;
    let mut body = json!({"hashes": hashes, "algorithm": "sha1", "loaders": t.loaders});
    if !t.game_version.is_empty() {
        body["game_versions"] = json!([t.game_version]);
    }
    let latest = net::post_json("https://api.modrinth.com/v2/version_files/update", &body)?;

    let mut unknown = Vec::new();
    let mut updates = Vec::new();
    let mut pids = Vec::new();
    for (h, f) in &by_hash {
        let Some(cur) = current.get(h) else {
            unknown.push(f.clone());
            continue;
        };
        let Some(new) = latest.get(h) else { continue };
        if s(&cur["id"]) == s(&new["id"]) {
            continue;
        }
        // не предлагаем «обновиться» на более старую дату (бывает при смене каналов)
        if s(&new["date_published"]) < s(&cur["date_published"]) {
            continue;
        }
        let Some(mut fr) = mr_file_ref(new) else { continue };
        let cur_release = cur["version_type"].as_str() == Some("release");
        let mut prerelease = false;
        if cur_release && fr.channel != "release" {
            // со стабильной версии не переводим на бету: ищем более новый релиз
            let cur_date = s(&cur["date_published"]);
            let newer_release = mr_versions(t, &fr.project_id)
                .unwrap_or_default()
                .into_iter()
                .find(|v| v.channel == "release" && v.date > cur_date && v.version_id != s(&cur["id"]));
            match newer_release {
                Some(r) => fr = r,
                None => prerelease = true,
            }
        }
        pids.push(fr.project_id.clone());
        updates.push(UpdateItem {
            file: f.clone(),
            title: String::new(),
            icon_url: String::new(),
            current: s(&cur["version_number"]),
            latest: fr,
            prerelease,
        });
    }
    let meta = project_meta(t, Source::Modrinth, &pids);
    for u in &mut updates {
        if let Some(m) = meta.get(&u.latest.project_id) {
            u.title = m.title.clone();
            u.icon_url = m.icon.clone();
        }
        if u.title.is_empty() {
            u.title = u.file.clone();
        }
    }
    updates.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    unknown.sort();
    Ok(UpdatesReport { checked: by_hash.len(), updates, unknown })
}

/// План обновления выбранных файлов.
pub fn plan_updates(items: &[UpdateItem]) -> Plan {
    Plan {
        items: items
            .iter()
            .map(|u| PlanItem {
                title: u.title.clone(),
                icon_url: u.icon_url.clone(),
                file: u.latest.clone(),
                reason: "update".into(),
                replaces: Some(u.file.clone()).filter(|f| f != &u.latest.file_name),
            })
            .collect(),
        ..Default::default()
    }
}

// ===================================================================== «скачать подходящую версию»

/// Достаёт номер версии мода из строки Modrinth: «6.0.10+mc1.21.1» → «6.0.10»,
/// «mc1.21.1-1.3.1» → «1.3.1», «tfmg-1.3.1» → «1.3.1».
pub fn mod_version_of(s: &str, mc: &str) -> String {
    let mut x = s.to_lowercase();
    for pat in [format!("mc{mc}"), format!("{mc}-"), format!("-{mc}"), format!("+{mc}")] {
        x = x.replace(&pat, " ");
    }
    for w in ["neoforge", "forge", "fabric", "quilt", "mc"] {
        x = x.replace(w, " ");
    }
    let re = regex::Regex::new(r"\d+(?:\.\d+)+").unwrap();
    re.find(&x).map(|m| m.as_str().to_string()).unwrap_or_default()
}

/// Найти на Modrinth версию мода `mod_id`, подходящую под диапазон, и составить план замены.
pub fn plan_fix_version(t: &Target, mod_id: &str, range: &str, old_file: Option<&str>, old_name: Option<&str>) -> Result<Plan> {
    // slug на Modrinth чаще всего совпадает с id мода; иначе ищем
    let pid = match net::get_json(&format!("https://api.modrinth.com/v2/project/{}", enc(mod_id))) {
        Ok(p) => s(&p["id"]),
        Err(_) => {
            let alt = mod_id.replace('_', "-");
            match net::get_json(&format!("https://api.modrinth.com/v2/project/{}", enc(&alt))) {
                Ok(p) => s(&p["id"]),
                Err(_) => mr_search(t, mod_id, "relevance", 0)?
                    .items
                    .first()
                    .map(|p| p.id.clone())
                    .ok_or_else(|| Error::other(format!("«{mod_id}» не найден на Modrinth")))?,
            }
        }
    };
    let list = mr_versions(t, &pid)?;
    let ok: Vec<&FileRef> = list
        .iter()
        .filter(|v| {
            let n = mod_version_of(&v.version_number, &t.game_version);
            let n = if n.is_empty() { mod_version_of(&v.file_name, &t.game_version) } else { n };
            crate::mods::version_matches(range, &n) == Some(true)
        })
        .collect();
    let pick = ok
        .iter()
        .find(|v| v.channel == "release")
        .or(ok.first())
        .copied()
        .cloned()
        .ok_or_else(|| {
            Error::other(format!(
                "на Modrinth нет версии «{mod_id}» {} для {} {}",
                crate::analyze::human_range(range),
                t.loaders.join("/"),
                t.game_version
            ))
        })?;
    let meta = project_meta(t, Source::Modrinth, &[pid.clone()]).remove(&pid);
    let title = meta.as_ref().map(|m| m.title.clone()).unwrap_or_else(|| mod_id.to_string());
    let mut warnings = Vec::new();
    if let Some(old) = old_name.filter(|n| !n.is_empty()) {
        if !names_similar(old, &title) {
            warnings.push(format!(
                "На Modrinth под id «{mod_id}» найден проект «{title}», а у тебя установлен «{old}». Это может быть другой мод или форк — открой страницу и проверь перед установкой."
            ));
        }
    }
    Ok(Plan {
        warnings,
        items: vec![PlanItem {
            title,
            icon_url: meta.map(|m| m.icon).unwrap_or_default(),
            replaces: old_file.map(String::from).filter(|f| f != &pick.file_name),
            reason: "fix".into(),
            file: pick,
        }],
        already: vec![],
    })
}

/// Похожи ли названия: «Create: The Factory Must Grow» и «TFMG Community Edition» — нет.
pub fn names_similar(a: &str, b: &str) -> bool {
    let norm = |s: &str| -> String { s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect() };
    let (x, y) = (norm(a), norm(b));
    if x.is_empty() || y.is_empty() {
        return true;
    }
    if x.contains(&y) || y.contains(&x) {
        return true;
    }
    // доля общих слов
    let words = |s: &str| -> std::collections::HashSet<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2 && !["the", "mod", "create", "for", "and"].contains(w))
            .map(String::from)
            .collect()
    };
    let (wa, wb) = (words(a), words(b));
    let common = wa.intersection(&wb).count();
    common * 2 >= wa.len().min(wb.len()).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cf_key_cleanup() {
        assert_eq!(clean_cf_key(" \"$2a$10$abc\"\n"), "$2a$10$abc");
        assert_eq!(clean_cf_key("x-api-key: $2a$10$abc\u{200b}"), "$2a$10$abc");
    }

    #[test]
    fn similar_names() {
        assert!(!names_similar("Create: The Factory Must Grow", "Create: TFMG Community Edition"));
        assert!(names_similar("Create", "Create"));
        assert!(names_similar("Just Enough Items", "Just Enough Items (JEI)"));
        assert!(names_similar("Sodium", "Sodium Extra") );
    }

    #[test]
    fn versions_from_names() {
        assert_eq!(mod_version_of("6.0.10+mc1.21.1", "1.21.1"), "6.0.10");
        assert_eq!(mod_version_of("mc1.21.1-1.3.1", "1.21.1"), "1.3.1");
        assert_eq!(mod_version_of("1.3.1-neoforge", "1.21.1"), "1.3.1");
        assert_eq!(mod_version_of("tfmg-1.21.1-1.3.1.jar", "1.21.1"), "1.3.1");
    }

    fn target(dir: &Path) -> Target {
        let s = ServerEntry {
            path: dir.to_string_lossy().into(),
            core: CoreKind::NeoForge,
            mc_version: "1.21.1".into(),
            ..Default::default()
        };
        Target::for_server(&s, Kind::Mod, "")
    }

    /// Живая проверка Modrinth: поиск, план с зависимостями, установка, обновления.
    #[test]
    #[ignore]
    fn live_modrinth() {
        let t = tempfile::tempdir().unwrap();
        let tg = target(t.path());
        let r = search(&tg, Source::Modrinth, "create", "relevance", 0).unwrap();
        assert!(r.items.iter().any(|p| p.slug == "create"));
        // Create: Power Grid требует tfmg? проверим на моде с зависимостями
        let plan = plan_install(&tg, Source::Modrinth, "create-enchantment-industry", None).unwrap();
        assert!(plan.items.len() >= 2, "{:?}", plan.items.iter().map(|i| &i.title).collect::<Vec<_>>());
        let got = apply(&tg, &plan, |_, _, _| {}).unwrap();
        assert!(!got.is_empty());
        let u = check_updates(&tg).unwrap();
        assert!(u.unknown.is_empty());
        // повторный план — зависимость уже стоит
        let p2 = plan_install(&tg, Source::Modrinth, "create-enchantment-industry", None).unwrap();
        assert!(!p2.already.is_empty());
    }

    #[test]
    #[ignore]
    fn live_fix_version() {
        let t = tempfile::tempdir().unwrap();
        let tg = target(t.path());
        let p = plan_fix_version(&tg, "create", "[6.0.0,7.0.0)", Some("create-old.jar"), Some("Create")).unwrap();
        assert!(p.warnings.is_empty());
        assert!(p.items[0].file.version_number.starts_with("6."));
        assert_eq!(p.items[0].replaces.as_deref(), Some("create-old.jar"));
    }
}
