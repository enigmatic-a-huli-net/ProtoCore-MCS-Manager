//! Моды: чтение метаданных из jar, иконки, зависимости, конфликты и клиентские моды.
//!
//! Поддерживаются форматы:
//! - Fabric   — `fabric.mod.json`
//! - Quilt    — `quilt.mod.json`
//! - NeoForge — `META-INF/neoforge.mods.toml`
//! - Forge    — `META-INF/mods.toml`
//!
//! Отключённые моды лежат в `mods_disabled/`. Ничего не удаляется.
//! Причина отключения (вручную / клиентский / краш) хранится в
//! `mods_disabled/.pcm-reasons.json`, чтобы в интерфейсе было видно, почему мод выключен.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::ModSettings;
use crate::detect::ModLoader;
use crate::util::{self, file_name, list_jars};
use crate::Result;

pub const ENABLED_DIR: &str = "mods";
pub const DISABLED_DIR: &str = "mods_disabled";
const REASONS_FILE: &str = ".pcm-reasons.json";

/// Размер иконок в списке, px.
const ICON_SIZE: u32 = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Both,
    Client,
    Server,
    Unknown,
}

/// Насколько уверены, что мод только клиентский.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum ClientConfidence {
    No,
    /// Есть признаки (чёрный список, side=CLIENT у зависимостей)
    Likely,
    /// Мод сам заявляет, что он клиентский
    Definite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub id: String,
    pub required: bool,
    pub version: String,
    pub side: Side,
}

/// Всё, что удалось прочитать из jar. Кэшируется по (путь, размер, время).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModMeta {
    pub loader: Option<ModLoader>,
    /// Все id внутри jar (у Forge в одном jar может быть несколько модов)
    pub ids: Vec<String>,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub icon: Option<String>,
    pub side: Side,
    pub client: ClientConfidence,
    pub client_reason: String,
    pub deps: Vec<Dependency>,
    /// id, которые мод «предоставляет» (provides + вложенные jar)
    pub provides: Vec<String>,
    /// Точно несовместим (breaks / incompatible)
    pub breaks: Vec<String>,
    /// Возможно несовместим (conflicts / discouraged)
    pub conflicts: Vec<String>,
    pub homepage: String,
    /// Основные Java-пакеты мода (com.simibubi.create …) — чтобы по стектрейсу понять, чей это код
    #[serde(default)]
    pub packages: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModInfo {
    pub file: String,
    pub enabled: bool,
    pub size: u64,
    pub id: String,
    #[serde(flatten)]
    pub meta: ModMeta,
    /// Почему отключён: user / client / crash / bisect
    pub disabled_reason: Option<String>,
    pub disabled_detail: Option<String>,
    /// Какие включённые моды требуют этот мод
    pub required_by: Vec<String>,
    /// Каких обязательных зависимостей не хватает
    pub missing_deps: Vec<String>,
    /// Найденные проблемы (короткие строки)
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConflictKind {
    /// Точно конфликтуют (так написано в метаданных, дубликат)
    Definite,
    /// Возможно конфликтуют (известные пары, conflicts/discouraged)
    Possible,
}

#[derive(Debug, Clone, Serialize)]
pub struct Conflict {
    pub kind: ConflictKind,
    pub a: String,
    pub b: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissingDep {
    pub id: String,
    pub needed_by: Vec<String>,
    /// Зависимость есть, но мод с ней выключен (лежит в mods_disabled)
    pub disabled_file: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModReport {
    pub mods: Vec<ModInfo>,
    pub missing: Vec<MissingDep>,
    pub conflicts: Vec<Conflict>,
    pub enabled_count: usize,
    pub disabled_count: usize,
    pub client_count: usize,
}

// ===================================================================== списки

/// id, которые всегда «есть» — их не надо искать среди модов.
const BUILTIN_IDS: &[&str] = &[
    "minecraft", "java", "fabricloader", "fabric-loader", "quilt_loader", "forge", "neoforge",
    "javafml", "lowcodefml", "mixinextras", "mixin", "fml",
];

/// Известные клиентские моды (по id). Считаются «вероятно клиентскими».
/// Свои id можно добавить в настройках сервера.
pub const CLIENT_BLACKLIST: &[&str] = &[
    // рендер и шейдеры
    "sodium", "embeddium", "rubidium", "magnesium", "iris", "oculus", "optifine", "optifabric",
    "indium", "sodium_extra", "sodiumextra", "reeses_sodium_options", "rubidium_extra",
    "embeddiumplus", "embeddium_extra", "flerovium", "oculusflywheelcompat", "sodiumoptionsapi",
    "sodiumdynamiclights", "immediatelyfast", "moreculling", "entityculling", "enhancedblockentities",
    "cull_less_leaves", "cullleaves", "dashloader", "betterfpsdist", "fpsreducer", "dynamic_fps",
    "dynamicfps", "nvidium", "c2me_client", "exordium",
    // освещение и эффекты
    "lambdynlights", "dynamiclights", "ryoamiclights", "dynamiclightsreforged", "visuality",
    "particular", "effective", "falling_leaves", "fallingleaves", "wakes", "illuminations",
    "betterclouds", "enhancedvisuals", "citresewn", "cit_resewn", "continuity", "lambdabettergrass",
    "animatica", "entity_texture_features", "entity_model_features", "betterbiomeblend",
    // модели и анимации игрока
    "skinlayers3d", "3dskinlayers", "notenoughanimations", "firstperson", "firstpersonmod",
    "realcamera", "waveycapes", "betteranimationscollection", "cameraoverhaul", "shouldersurfing",
    "betterthirdperson", "customskinloader", "freecam",
    // интерфейс
    "zoomify", "okzoomer", "logical_zoom", "controlling", "mousetweaks", "modmenu", "catalogue",
    "betterf3", "legendarytooltips", "equipmentcompare", "itemborders", "durabilitytooltip",
    "toastcontrol", "blur", "fancymenu", "drippyloadingscreen", "yungsmenutweaks", "bettermodsbutton",
    "paginatedadvancements", "betteradvancements", "chat_heads", "chatheads", "betterpingdisplay",
    "tooltipfix", "inventoryprofilesnext", "mousewheelie", "screenshot_viewer", "fastquit",
    "puzzle", "controllable", "midnightcontrols", "torohealth", "neat", "clienttweaks",
    "overflowingbars", "raised", "smoothscroll", "jeed",
    // карты
    "xaerominimap", "xaerominimapfair", "xaeroworldmap", "voxelmap",
    // звук
    "ambientsounds", "soundphysics", "sound_physics_remastered", "presencefootsteps", "auudio",
];

/// Известные пары, которые обычно не уживаются вместе (возможный конфликт).
const KNOWN_CONFLICTS: &[(&str, &str, &str)] = &[
    ("optifine", "sodium", "OptiFine и Sodium — оба заменяют рендер"),
    ("optifine", "embeddium", "OptiFine и Embeddium — оба заменяют рендер"),
    ("optifine", "rubidium", "OptiFine и Rubidium — оба заменяют рендер"),
    ("embeddium", "rubidium", "Embeddium — форк Rubidium, вместе не нужны"),
    ("embeddium", "sodium", "Embeddium и Sodium — оба заменяют рендер"),
    ("iris", "oculus", "Iris и Oculus — одно и то же для разных загрузчиков"),
    ("lithium", "canary", "Canary — порт Lithium, вместе не нужны"),
    ("lithium", "radium", "Radium — порт Lithium, вместе не нужны"),
    ("phosphor", "starlight", "оба переписывают движок света"),
    ("starlight", "scalablelux", "оба переписывают движок света"),
    ("c2me", "noisium", "оба ускоряют генерацию — бывают конфликты"),
    ("betterfoliage", "sodium", "Better Foliage несовместим с Sodium-подобными модами"),
    ("performant", "lithium", "Performant известен конфликтами с Lithium"),
    ("xaerominimap", "xaerominimapfair", "две версии одной миникарты"),
];

// ===================================================================== кэш

type CacheKey = (String, u64, i64);
fn cache() -> &'static Mutex<HashMap<CacheKey, Arc<ModMeta>>> {
    static C: OnceLock<Mutex<HashMap<CacheKey, Arc<ModMeta>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn file_key(path: &Path) -> Option<(CacheKey, u64)> {
    let md = fs::metadata(path).ok()?;
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // ключ — имя файла, а не путь: при переносе в mods_disabled кэш сохраняется
    Some(((file_name(path), md.len(), mtime), md.len()))
}

// ===================================================================== чтение jar

fn read_entry<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name).ok()?;
    let mut v = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut v).ok()?;
    Some(v)
}

fn read_text<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, name: &str) -> Option<String> {
    read_entry(zip, name).map(|b| String::from_utf8_lossy(&b).trim_start_matches('\u{feff}').to_string())
}

fn manifest_version<R: Read + Seek>(zip: &mut zip::ZipArchive<R>) -> Option<String> {
    let m = read_text(zip, "META-INF/MANIFEST.MF")?;
    m.lines()
        .find_map(|l| l.strip_prefix("Implementation-Version:"))
        .map(|v| v.trim().to_string())
}

fn empty_meta() -> ModMeta {
    ModMeta {
        loader: None,
        ids: vec![],
        name: String::new(),
        version: String::new(),
        description: String::new(),
        authors: vec![],
        icon: None,
        side: Side::Unknown,
        client: ClientConfidence::No,
        client_reason: String::new(),
        deps: vec![],
        provides: vec![],
        breaks: vec![],
        conflicts: vec![],
        homepage: String::new(),
        packages: vec![],
    }
}

/// Читает метаданные мода (с кэшем).
pub fn read_meta(path: &Path) -> ModMeta {
    let key = file_key(path);
    if let Some((k, _)) = &key {
        if let Some(m) = cache().lock().unwrap().get(k) {
            return (**m).clone();
        }
    }
    let meta = fs::File::open(path)
        .ok()
        .and_then(|f| zip::ZipArchive::new(f).ok())
        .map(|mut z| parse_archive(&mut z, true))
        .unwrap_or_else(|| {
            let mut m = empty_meta();
            m.client_reason = "не удалось открыть jar".into();
            m
        });
    if let Some((k, _)) = key {
        cache().lock().unwrap().insert(k, Arc::new(meta.clone()));
    }
    meta
}

fn parse_archive<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, nested: bool) -> ModMeta {
    let mut m = empty_meta();
    if let Some(s) = read_text(zip, "fabric.mod.json") {
        parse_fabric(&s, &mut m);
        if let Some(icon) = m.icon.take() {
            m.icon = read_entry(zip, icon.trim_start_matches('/')).and_then(|b| util::image_to_data_uri(&b, ICON_SIZE));
        }
        if nested {
            // Jar-in-Jar: вложенные моды тоже «предоставляются»
            if let Ok(v) = serde_json::from_str::<Value>(&s) {
                if let Some(arr) = v.get("jars").and_then(Value::as_array) {
                    for j in arr {
                        if let Some(f) = j.get("file").and_then(Value::as_str) {
                            m.provides.extend(nested_ids(zip, f));
                        }
                    }
                }
            }
        }
    } else if let Some(s) = read_text(zip, "quilt.mod.json") {
        parse_quilt(&s, &mut m);
        if let Some(icon) = m.icon.take() {
            m.icon = read_entry(zip, icon.trim_start_matches('/')).and_then(|b| util::image_to_data_uri(&b, ICON_SIZE));
        }
    } else {
        let (toml_src, loader) = if let Some(s) = read_text(zip, "META-INF/neoforge.mods.toml") {
            (Some(s), ModLoader::NeoForge)
        } else {
            (read_text(zip, "META-INF/mods.toml"), ModLoader::Forge)
        };
        if let Some(s) = toml_src {
            let jar_version = manifest_version(zip);
            parse_forge_toml(&s, loader, jar_version.as_deref(), &mut m);
            if let Some(icon) = m.icon.take() {
                m.icon = read_entry(zip, icon.trim_start_matches('/')).and_then(|b| util::image_to_data_uri(&b, ICON_SIZE));
            }
            if nested {
                if let Some(meta) = read_text(zip, "META-INF/jarjar/metadata.json") {
                    if let Ok(v) = serde_json::from_str::<Value>(&meta) {
                        for j in v.get("jars").and_then(Value::as_array).into_iter().flatten() {
                            if let Some(p) = j.get("path").and_then(Value::as_str) {
                                m.provides.extend(nested_ids(zip, p));
                            }
                        }
                    }
                }
            }
        }
    }
    if m.name.is_empty() {
        m.name = m.ids.first().cloned().unwrap_or_default();
    }
    if nested {
        m.packages = main_packages(zip);
    }
    m
}

/// Самые «населённые» пакеты jar (до 3 уровней), без вложенных библиотек.
fn main_packages<R: Read + Seek>(zip: &mut zip::ZipArchive<R>) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for name in zip.file_names() {
        if !name.ends_with(".class") || name.starts_with("META-INF/") {
            continue;
        }
        let parts: Vec<&str> = name.split('/').collect();
        if parts.len() < 3 {
            continue;
        }
        let depth = (parts.len() - 1).min(3);
        *counts.entry(parts[..depth].join(".")).or_default() += 1;
    }
    let mut v: Vec<(String, usize)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    let total: usize = v.iter().map(|x| x.1).sum();
    v.into_iter()
        .take(4)
        .filter(|(_, c)| *c * 10 >= total.max(1)) // пакет — хотя бы 10% классов
        .map(|(p, _)| p)
        .collect()
}

fn nested_ids<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, inner_path: &str) -> Vec<String> {
    let Some(bytes) = read_entry(zip, inner_path.trim_start_matches('/')) else { return vec![] };
    let Ok(mut inner) = zip::ZipArchive::new(Cursor::new(bytes)) else { return vec![] };
    let m = parse_archive(&mut inner, false);
    let mut ids = m.ids;
    ids.extend(m.provides);
    ids
}

fn authors_from(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    x.as_str()
                        .map(str::to_string)
                        .or_else(|| x.get("name").and_then(Value::as_str).map(str::to_string))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Иконка у Fabric — строка или словарь {"16": "...", "128": "..."} (берём самую большую).
fn icon_from(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o
            .iter()
            .max_by_key(|(k, _)| k.parse::<u32>().unwrap_or(0))
            .and_then(|(_, v)| v.as_str().map(str::to_string)),
        _ => None,
    }
}

fn version_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" || "),
        _ => "*".into(),
    }
}

pub fn parse_fabric(s: &str, m: &mut ModMeta) {
    m.loader = Some(ModLoader::Fabric);
    // некоторые моды кладут в JSON переводы строк и табы внутри строк — serde это переживает
    let Ok(v) = serde_json::from_str::<Value>(s) else {
        m.client_reason = "fabric.mod.json не читается".into();
        return;
    };
    if let Some(id) = v.get("id").and_then(Value::as_str) {
        m.ids.push(id.to_string());
    }
    m.name = v.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    m.version = v.get("version").and_then(Value::as_str).unwrap_or("").to_string();
    m.description = v.get("description").and_then(Value::as_str).unwrap_or("").to_string();
    m.authors = authors_from(v.get("authors"));
    m.icon = icon_from(v.get("icon"));
    m.homepage = v
        .pointer("/contact/homepage")
        .or_else(|| v.pointer("/contact/sources"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    m.side = match v.get("environment").and_then(Value::as_str) {
        Some("client") => Side::Client,
        Some("server") => Side::Server,
        _ => Side::Both,
    };
    if m.side == Side::Client {
        m.client = ClientConfidence::Definite;
        m.client_reason = "fabric.mod.json: \"environment\": \"client\"".into();
    }
    let map_ids = |key: &str| -> Vec<(String, String)> {
        v.get(key)
            .and_then(Value::as_object)
            .map(|o| o.iter().map(|(k, v)| (k.clone(), version_str(v))).collect())
            .unwrap_or_default()
    };
    for (id, ver) in map_ids("depends") {
        m.deps.push(Dependency { id, required: true, version: ver, side: Side::Both });
    }
    for (id, ver) in map_ids("recommends") {
        m.deps.push(Dependency { id, required: false, version: ver, side: Side::Both });
    }
    m.breaks = map_ids("breaks").into_iter().map(|x| x.0).collect();
    m.conflicts = map_ids("conflicts").into_iter().map(|x| x.0).collect();
    if let Some(p) = v.get("provides").and_then(Value::as_array) {
        m.provides.extend(p.iter().filter_map(Value::as_str).map(str::to_string));
    }
}

pub fn parse_quilt(s: &str, m: &mut ModMeta) {
    m.loader = Some(ModLoader::Quilt);
    let Ok(v) = serde_json::from_str::<Value>(s) else { return };
    let ql = v.get("quilt_loader").cloned().unwrap_or(Value::Null);
    if let Some(id) = ql.get("id").and_then(Value::as_str) {
        m.ids.push(id.to_string());
    }
    m.version = ql.get("version").and_then(Value::as_str).unwrap_or("").to_string();
    m.name = ql.pointer("/metadata/name").and_then(Value::as_str).unwrap_or("").to_string();
    m.description = ql.pointer("/metadata/description").and_then(Value::as_str).unwrap_or("").to_string();
    m.icon = icon_from(ql.pointer("/metadata/icon"));
    if let Some(c) = ql.pointer("/metadata/contributors").and_then(Value::as_object) {
        m.authors = c.keys().cloned().collect();
    }
    m.side = match v.pointer("/minecraft/environment").and_then(Value::as_str) {
        Some("client") => Side::Client,
        Some("dedicated_server") => Side::Server,
        _ => Side::Both,
    };
    if m.side == Side::Client {
        m.client = ClientConfidence::Definite;
        m.client_reason = "quilt.mod.json: environment = client".into();
    }
    let dep_list = |key: &str| -> Vec<(String, bool)> {
        ql.get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|d| match d {
                        Value::String(s) => Some((s.clone(), true)),
                        Value::Object(o) => o.get("id").and_then(Value::as_str).map(|id| {
                            (id.to_string(), !o.get("optional").and_then(Value::as_bool).unwrap_or(false))
                        }),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    for (id, req) in dep_list("depends") {
        // id в quilt могут быть вида "org.quiltmc:qsl" — берём часть после двоеточия
        let id = id.rsplit(':').next().unwrap_or(&id).to_string();
        m.deps.push(Dependency { id, required: req, version: "*".into(), side: Side::Both });
    }
    m.breaks = dep_list("breaks").into_iter().map(|x| x.0).collect();
    if let Some(p) = ql.get("provides").and_then(Value::as_array) {
        m.provides.extend(p.iter().filter_map(|x| {
            x.as_str().map(str::to_string).or_else(|| x.get("id").and_then(Value::as_str).map(str::to_string))
        }));
    }
}

fn toml_side(v: Option<&toml::Value>) -> Side {
    match v.and_then(|x| x.as_str()).map(|s| s.to_uppercase()).as_deref() {
        Some("CLIENT") => Side::Client,
        Some("SERVER") => Side::Server,
        _ => Side::Both,
    }
}

pub fn parse_forge_toml(s: &str, loader: ModLoader, jar_version: Option<&str>, m: &mut ModMeta) {
    m.loader = Some(loader);
    let doc: toml::Table = match s.parse() {
        Ok(t) => t,
        Err(_) => {
            // запасной вариант для «кривых» toml — достаём хотя бы modId
            let re = regex::Regex::new(r#"modId\s*=\s*["']([^"']+)["']"#).unwrap();
            if let Some(c) = re.captures(s) {
                m.ids.push(c[1].to_string());
            }
            m.client_reason = "mods.toml с ошибками, прочитан частично".into();
            return;
        }
    };
    let fill = |s: &str| -> String {
        if s.contains("${file.jarVersion}") {
            s.replace("${file.jarVersion}", jar_version.unwrap_or("?"))
        } else {
            s.to_string()
        }
    };

    if let Some(mods) = doc.get("mods").and_then(|v| v.as_array()) {
        for (i, md) in mods.iter().enumerate() {
            let Some(id) = md.get("modId").and_then(|x| x.as_str()) else { continue };
            m.ids.push(id.to_string());
            if i == 0 {
                m.name = md.get("displayName").and_then(|x| x.as_str()).unwrap_or("").to_string();
                m.version = fill(md.get("version").and_then(|x| x.as_str()).unwrap_or(""));
                m.description = md.get("description").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
                m.icon = md.get("logoFile").and_then(|x| x.as_str()).map(str::to_string);
                m.homepage = md.get("displayURL").and_then(|x| x.as_str()).unwrap_or("").to_string();
                if let Some(a) = md.get("authors").and_then(|x| x.as_str()) {
                    m.authors = a.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
                }
            }
        }
    }
    if m.icon.is_none() {
        m.icon = doc.get("logoFile").and_then(|x| x.as_str()).map(str::to_string);
    }

    // зависимости: [[dependencies.<modid>]]
    let mut client_side_core = false;
    if let Some(deps) = doc.get("dependencies").and_then(|v| v.as_table()) {
        for (_owner, list) in deps {
            for d in list.as_array().into_iter().flatten() {
                let Some(id) = d.get("modId").and_then(|x| x.as_str()) else { continue };
                let side = toml_side(d.get("side"));
                // NeoForge: type = "required" | "optional" | "incompatible" | "discouraged"
                // Forge:    mandatory = true/false
                let kind = d
                    .get("type")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_lowercase())
                    .unwrap_or_else(|| {
                        if d.get("mandatory").and_then(|x| x.as_bool()).unwrap_or(true) {
                            "required".into()
                        } else {
                            "optional".into()
                        }
                    });
                match kind.as_str() {
                    "incompatible" => m.breaks.push(id.to_string()),
                    "discouraged" => m.conflicts.push(id.to_string()),
                    k => {
                        let required = k == "required";
                        if (id == "minecraft" || id == "neoforge" || id == "forge") && side == Side::Client {
                            client_side_core = true;
                        }
                        m.deps.push(Dependency {
                            id: id.to_string(),
                            required,
                            version: d.get("versionRange").and_then(|x| x.as_str()).unwrap_or("*").to_string(),
                            side,
                        });
                    }
                }
            }
        }
    }

    // признаки клиентского мода
    let client_only = doc.get("clientSideOnly").and_then(|x| x.as_bool()).unwrap_or(false);
    if client_only {
        m.side = Side::Client;
        m.client = ClientConfidence::Definite;
        m.client_reason = "mods.toml: clientSideOnly = true".into();
    } else if client_side_core {
        m.side = Side::Client;
        m.client = ClientConfidence::Likely;
        m.client_reason = "зависимость от minecraft/neoforge помечена side=\"CLIENT\"".into();
    } else {
        m.side = Side::Both;
    }
}

// ===================================================================== причины отключения

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisableReason {
    pub reason: String,
    pub detail: String,
    pub ts: i64,
}

pub fn read_reasons(dir: &Path) -> BTreeMap<String, DisableReason> {
    fs::read_to_string(dir.join(REASONS_FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_reasons(dir: &Path, r: &BTreeMap<String, DisableReason>) -> Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(REASONS_FILE), serde_json::to_string_pretty(r)?)?;
    Ok(())
}

// ===================================================================== анализ

fn effective_client(meta: &ModMeta, settings: &ModSettings) -> (ClientConfidence, String) {
    if meta.client == ClientConfidence::Definite {
        return (meta.client, meta.client_reason.clone());
    }
    let extra: HashSet<String> = settings.extra_client_ids.iter().map(|s| s.to_lowercase()).collect();
    for id in &meta.ids {
        let lid = id.to_lowercase();
        if extra.contains(&lid) {
            return (ClientConfidence::Definite, "в твоём списке клиентских модов".into());
        }
        if CLIENT_BLACKLIST.contains(&lid.as_str()) {
            return (ClientConfidence::Likely, "в списке известных клиентских модов".into());
        }
    }
    (meta.client, meta.client_reason.clone())
}

/// Полный список модов сервера с анализом зависимостей и конфликтов.
pub fn list(server_dir: &Path, server_loader: Option<ModLoader>, settings: &ModSettings) -> ModReport {
    let enabled_dir = server_dir.join(ENABLED_DIR);
    let disabled_dir = server_dir.join(DISABLED_DIR);
    let reasons = read_reasons(&disabled_dir);

    let mut mods: Vec<ModInfo> = Vec::new();
    for (dir, enabled) in [(&enabled_dir, true), (&disabled_dir, false)] {
        for jar in list_jars(dir) {
            let file = file_name(&jar);
            let size = fs::metadata(&jar).map(|m| m.len()).unwrap_or(0);
            let mut meta = read_meta(&jar);
            let (c, r) = effective_client(&meta, settings);
            meta.client = c;
            meta.client_reason = r;
            let reason = (!enabled).then(|| reasons.get(&file).cloned()).flatten();
            mods.push(ModInfo {
                id: meta.ids.first().cloned().unwrap_or_else(|| file.trim_end_matches(".jar").to_string()),
                file,
                enabled,
                size,
                disabled_reason: reason.as_ref().map(|r| r.reason.clone()).or((!enabled).then(|| "user".into())),
                disabled_detail: reason.map(|r| r.detail),
                meta,
                required_by: vec![],
                missing_deps: vec![],
                problems: vec![],
            });
        }
    }
    analyze(&mut mods, server_loader)
}

/// Отдельно от list(), чтобы можно было проверить на тестовых данных.
pub fn analyze(mods: &mut Vec<ModInfo>, server_loader: Option<ModLoader>) -> ModReport {
    // что доступно среди включённых: id + provides
    let mut available: HashMap<String, String> = HashMap::new(); // id → файл
    for m in mods.iter().filter(|m| m.enabled) {
        for id in m.meta.ids.iter().chain(m.meta.provides.iter()) {
            available.entry(id.to_lowercase()).or_insert_with(|| m.file.clone());
        }
    }
    // где лежит выключенный мод с этим id
    let mut disabled_by_id: HashMap<String, String> = HashMap::new();
    for m in mods.iter().filter(|m| !m.enabled) {
        for id in m.meta.ids.iter().chain(m.meta.provides.iter()) {
            disabled_by_id.entry(id.to_lowercase()).or_insert_with(|| m.file.clone());
        }
    }
    let has_connector = available.contains_key("connector");
    let builtin: HashSet<&str> = BUILTIN_IDS.iter().copied().collect();

    // отсутствующие зависимости
    let mut missing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut required_by: HashMap<String, Vec<String>> = HashMap::new();
    for m in mods.iter_mut().filter(|m| m.enabled) {
        let name = m.meta.name.clone();
        let deps = m.meta.deps.clone();
        for d in deps.iter().filter(|d| d.required) {
            let did = d.id.to_lowercase();
            // fabric-api часто указывается как "fabric"
            let alias = if did == "fabric" { "fabric-api".to_string() } else { did.clone() };
            if builtin.contains(did.as_str()) {
                continue;
            }
            // клиентская зависимость серверу не нужна
            if d.side == Side::Client {
                continue;
            }
            if let Some(file) = available.get(&did).or_else(|| available.get(&alias)) {
                required_by.entry(file.clone()).or_default().push(name.clone());
            } else {
                m.missing_deps.push(d.id.clone());
                missing.entry(d.id.clone()).or_default().push(name.clone());
            }
        }
    }

    // конфликты
    let mut conflicts: Vec<Conflict> = Vec::new();
    let enabled: Vec<&ModInfo> = mods.iter().filter(|m| m.enabled).collect();
    // дубликаты id
    let mut seen: HashMap<String, &ModInfo> = HashMap::new();
    for m in &enabled {
        for id in &m.meta.ids {
            let lid = id.to_lowercase();
            if let Some(other) = seen.get(&lid) {
                conflicts.push(Conflict {
                    kind: ConflictKind::Definite,
                    a: other.file.clone(),
                    b: m.file.clone(),
                    reason: format!("один и тот же мод «{id}» в двух файлах — оставь один"),
                });
            } else {
                seen.insert(lid, m);
            }
        }
    }
    let file_of = |id: &str| available.get(&id.to_lowercase()).cloned();
    for m in &enabled {
        for b in &m.meta.breaks {
            if let Some(f) = file_of(b) {
                if f != m.file {
                    conflicts.push(Conflict {
                        kind: ConflictKind::Definite,
                        a: m.file.clone(),
                        b: f,
                        reason: format!("{} несовместим с «{b}» (указано в метаданных мода)", m.meta.name),
                    });
                }
            }
        }
        for c in &m.meta.conflicts {
            if let Some(f) = file_of(c) {
                if f != m.file {
                    conflicts.push(Conflict {
                        kind: ConflictKind::Possible,
                        a: m.file.clone(),
                        b: f,
                        reason: format!("{} не рекомендует использовать вместе с «{c}»", m.meta.name),
                    });
                }
            }
        }
    }
    for (x, y, why) in KNOWN_CONFLICTS {
        if let (Some(a), Some(b)) = (file_of(x), file_of(y)) {
            if a != b {
                conflicts.push(Conflict { kind: ConflictKind::Possible, a, b, reason: why.to_string() });
            }
        }
    }

    // моды не для этого загрузчика
    for m in mods.iter_mut() {
        if let (Some(server), Some(ml)) = (server_loader, m.meta.loader) {
            let compatible = match (server, ml) {
                (a, b) if a == b => true,
                (ModLoader::Quilt, ModLoader::Fabric) => true,
                (ModLoader::NeoForge, ModLoader::Fabric) => has_connector,
                (ModLoader::NeoForge, ModLoader::Forge) => false,
                _ => false,
            };
            if !compatible {
                m.problems.push(format!("мод для {:?}, а сервер {:?}", ml, server));
            }
        }
        if m.meta.ids.is_empty() {
            m.problems.push("в jar нет метаданных мода — возможно, это библиотека или мусор".into());
        }
        if m.enabled && m.meta.client == ClientConfidence::Definite {
            m.problems.push("клиентский мод — сервер с ним может не запуститься".into());
        }
        if let Some(rb) = required_by.get(&m.file) {
            m.required_by = rb.clone();
            m.required_by.sort();
            m.required_by.dedup();
        }
    }
    // проблемы-конфликты на самих модах
    for c in &conflicts {
        for m in mods.iter_mut().filter(|m| m.file == c.a || m.file == c.b) {
            let other = if m.file == c.a { &c.b } else { &c.a };
            let tag = if c.kind == ConflictKind::Definite { "конфликт" } else { "возможный конфликт" };
            m.problems.push(format!("{tag} с {other}"));
        }
    }

    let missing = missing
        .into_iter()
        .map(|(id, mut needed_by)| {
            needed_by.sort();
            needed_by.dedup();
            MissingDep { disabled_file: disabled_by_id.get(&id.to_lowercase()).cloned(), id, needed_by }
        })
        .collect();

    mods.sort_by(|a, b| a.meta.name.to_lowercase().cmp(&b.meta.name.to_lowercase()));
    ModReport {
        enabled_count: mods.iter().filter(|m| m.enabled).count(),
        disabled_count: mods.iter().filter(|m| !m.enabled).count(),
        client_count: mods.iter().filter(|m| m.meta.client != ClientConfidence::No).count(),
        missing,
        conflicts,
        mods: std::mem::take(mods),
    }
}

// ===================================================================== операции

/// Включить или выключить мод (перенос между mods/ и mods_disabled/).
pub fn set_enabled(server_dir: &Path, file: &str, enable: bool, reason: &str, detail: &str) -> Result<PathBuf> {
    let enabled_dir = server_dir.join(ENABLED_DIR);
    let disabled_dir = server_dir.join(DISABLED_DIR);
    let (from, to) = if enable { (&disabled_dir, &enabled_dir) } else { (&enabled_dir, &disabled_dir) };
    let src = from.join(file);
    if !src.is_file() {
        return Err(crate::Error::other(format!("файл не найден: {}", src.display())));
    }
    let dst = util::move_file(&src, to)?;
    let mut reasons = read_reasons(&disabled_dir);
    if enable {
        reasons.remove(file);
    } else {
        reasons.insert(
            file_name(&dst),
            DisableReason { reason: reason.into(), detail: detail.into(), ts: chrono::Utc::now().timestamp() },
        );
    }
    write_reasons(&disabled_dir, &reasons)?;
    Ok(dst)
}

#[derive(Debug, Clone, Serialize)]
pub struct MovedMod {
    pub file: String,
    pub name: String,
    pub reason: String,
}

/// Переносит клиентские моды в mods_disabled/. Возвращает список перенесённых.
pub fn disable_client_mods(
    server_dir: &Path,
    server_loader: Option<ModLoader>,
    settings: &ModSettings,
    include_likely: bool,
) -> Result<Vec<MovedMod>> {
    let report = list(server_dir, server_loader, settings);
    let keep: HashSet<String> = settings.never_disable.iter().map(|s| s.to_lowercase()).collect();
    let mut moved = Vec::new();
    for m in report.mods.iter().filter(|m| m.enabled) {
        let hit = match m.meta.client {
            ClientConfidence::Definite => true,
            ClientConfidence::Likely => include_likely,
            ClientConfidence::No => false,
        };
        if !hit || m.meta.ids.iter().any(|id| keep.contains(&id.to_lowercase())) {
            continue;
        }
        // если от мода зависят другие включённые — не трогаем, чтобы не сломать их
        if !m.required_by.is_empty() {
            continue;
        }
        set_enabled(server_dir, &m.file, false, "client", &m.meta.client_reason)?;
        moved.push(MovedMod { file: m.file.clone(), name: m.meta.name.clone(), reason: m.meta.client_reason.clone() });
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn jar(dir: &Path, name: &str, entries: &[(&str, &[u8])]) {
        fs::create_dir_all(dir).unwrap();
        let f = fs::File::create(dir.join(name)).unwrap();
        let mut z = zip::ZipWriter::new(f);
        for (n, data) in entries {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn fabric_client_and_deps() {
        let mut m = empty_meta();
        parse_fabric(
            r#"{"id":"zoomer","name":"Zoomer","version":"1.0","environment":"client",
                "depends":{"fabricloader":">=0.15","fabric-api":"*","cloth-config":"*"},
                "breaks":{"optifabric":"*"}, "icon":{"16":"a.png","128":"b.png"}}"#,
            &mut m,
        );
        assert_eq!(m.client, ClientConfidence::Definite);
        assert_eq!(m.deps.len(), 3);
        assert_eq!(m.breaks, vec!["optifabric"]);
        assert_eq!(m.icon.as_deref(), Some("b.png"));
    }

    #[test]
    fn neoforge_toml() {
        let mut m = empty_meta();
        parse_forge_toml(
            r#"
modLoader="javafml"
loaderVersion="[4,)"
license="MIT"
[[mods]]
modId="mymod"
version="${file.jarVersion}"
displayName="My Mod"
logoFile="logo.png"
[[dependencies.mymod]]
modId="neoforge"
type="required"
versionRange="[21.1,)"
side="BOTH"
[[dependencies.mymod]]
modId="curios"
type="required"
versionRange="*"
side="BOTH"
[[dependencies.mymod]]
modId="badmod"
type="incompatible"
"#,
            ModLoader::NeoForge,
            Some("2.3.4"),
            &mut m,
        );
        assert_eq!(m.ids, vec!["mymod"]);
        assert_eq!(m.version, "2.3.4");
        assert_eq!(m.breaks, vec!["badmod"]);
        assert_eq!(m.deps.iter().filter(|d| d.required).count(), 2);
        assert_eq!(m.client, ClientConfidence::No);
    }

    #[test]
    fn forge_client_side_dep() {
        let mut m = empty_meta();
        parse_forge_toml(
            "modLoader=\"javafml\"\n[[mods]]\nmodId=\"hud\"\n[[dependencies.hud]]\nmodId=\"minecraft\"\nmandatory=true\nside=\"CLIENT\"\n",
            ModLoader::Forge,
            None,
            &mut m,
        );
        assert_eq!(m.client, ClientConfidence::Likely);
    }

    #[test]
    fn full_scan_missing_conflict_and_disable() {
        let t = tempfile::tempdir().unwrap();
        let mods = t.path().join("mods");
        jar(&mods, "a.jar", &[("fabric.mod.json", br#"{"id":"a","name":"A","depends":{"b":"*","minecraft":"*"}}"#)]);
        jar(&mods, "c.jar", &[("fabric.mod.json", br#"{"id":"c","name":"C","environment":"client"}"#)]);
        jar(&mods, "d.jar", &[("fabric.mod.json", br#"{"id":"d","name":"D","breaks":{"a":"*"}}"#)]);
        jar(&mods, "d2.jar", &[("fabric.mod.json", br#"{"id":"d","name":"D copy"}"#)]);
        let s = ModSettings::default();
        let r = list(t.path(), Some(ModLoader::Fabric), &s);
        assert_eq!(r.enabled_count, 4);
        assert_eq!(r.missing.len(), 1);
        assert_eq!(r.missing[0].id, "b");
        assert!(r.conflicts.iter().any(|c| c.kind == ConflictKind::Definite && c.reason.contains("двух файлах")));
        assert!(r.conflicts.iter().any(|c| c.reason.contains("несовместим")));

        let moved = disable_client_mods(t.path(), Some(ModLoader::Fabric), &s, false).unwrap();
        assert_eq!(moved.len(), 1);
        assert!(t.path().join("mods_disabled/c.jar").exists());
        let r2 = list(t.path(), Some(ModLoader::Fabric), &s);
        let c = r2.mods.iter().find(|m| m.id == "c").unwrap();
        assert!(!c.enabled);
        assert_eq!(c.disabled_reason.as_deref(), Some("client"));

        set_enabled(t.path(), "c.jar", true, "", "").unwrap();
        assert!(mods.join("c.jar").exists());
    }

    #[test]
    fn jar_in_jar_provides() {
        let t = tempfile::tempdir().unwrap();
        // вложенный jar
        let mut inner = Vec::new();
        {
            let mut z = zip::ZipWriter::new(Cursor::new(&mut inner));
            z.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(br#"{"id":"cloth-config"}"#).unwrap();
            z.finish().unwrap();
        }
        let mods = t.path().join("mods");
        jar(
            &mods,
            "big.jar",
            &[
                ("fabric.mod.json", br#"{"id":"big","jars":[{"file":"META-INF/jars/cloth.jar"}]}"#),
                ("META-INF/jars/cloth.jar", &inner),
            ],
        );
        jar(&mods, "user.jar", &[("fabric.mod.json", br#"{"id":"user","depends":{"cloth-config":"*"}}"#)]);
        let r = list(t.path(), Some(ModLoader::Fabric), &ModSettings::default());
        assert!(r.missing.is_empty(), "{:?}", r.missing);
    }
}
