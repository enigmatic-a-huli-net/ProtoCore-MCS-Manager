//! Сборки модов.
//!
//! **Импорт** — сервер из сборки:
//! - Modrinth `.mrpack`: `modrinth.index.json` (файлы со ссылками и хэшами) + папки
//!   `overrides/` и `server-overrides/`. Файлы с `env.server = "unsupported"` не скачиваются.
//! - CurseForge `.zip`: `manifest.json` (projectID/fileID) + `overrides/`. Ссылки на файлы
//!   выдаёт только API CurseForge — нужен ключ. Файлы, которые автор запретил скачивать
//!   сторонним программам, перечисляются отдельно — их нужно положить вручную.
//!
//! **Экспорт** — клиентская сборка `.mrpack` для игроков: те же моды, что на сервере,
//! плюс клиентские моды, которые менеджер убрал с сервера (их ставят игроки).
//! Моды, найденные на Modrinth, записываются ссылками, остальные кладутся в архив целиком.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::detect::CoreKind;
use crate::config::ModSettings;
use crate::mods;
use crate::net;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct PackInfo {
    /// "modrinth" или "curseforge"
    pub format: String,
    pub name: String,
    pub version: String,
    pub mc: String,
    pub core: CoreKind,
    pub loader_version: String,
    /// всего файлов в сборке
    pub files: usize,
    /// из них нужны серверу
    pub server_files: usize,
    /// только для клиента (не скачиваются)
    pub client_only: usize,
    pub needs_cf_key: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ImportResult {
    pub downloaded: usize,
    pub skipped_client: usize,
    pub overrides: usize,
    /// файлы, которые нужно скачать вручную (имя, страница)
    pub manual: Vec<(String, String)>,
    pub failed: Vec<String>,
}

fn s(v: &Value) -> String {
    match v {
        Value::String(x) => x.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn open(path: &Path) -> Result<zip::ZipArchive<fs::File>> {
    Ok(zip::ZipArchive::new(fs::File::open(path)?)?)
}

fn read_json(z: &mut zip::ZipArchive<fs::File>, name: &str) -> Option<Value> {
    let mut f = z.by_name(name).ok()?;
    let mut t = String::new();
    f.read_to_string(&mut t).ok()?;
    serde_json::from_str(&t).ok()
}

/// Путь из сборки не должен выходить за папку сервера (`../`, абсолютные пути).
fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    let p = Path::new(rel);
    if p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_) | Component::RootDir)) {
        return None;
    }
    Some(base.join(p))
}

fn core_from_deps(deps: &serde_json::Map<String, Value>) -> (CoreKind, String) {
    for (k, core) in [("neoforge", CoreKind::NeoForge), ("forge", CoreKind::Forge), ("fabric-loader", CoreKind::Fabric), ("quilt-loader", CoreKind::Quilt)] {
        if let Some(v) = deps.get(k) {
            return (core, s(v));
        }
    }
    (CoreKind::Vanilla, String::new())
}

fn server_needed(env: Option<&Value>) -> bool {
    env.and_then(|e| e.get("server")).and_then(Value::as_str) != Some("unsupported")
}

/// Что внутри сборки (без скачивания).
pub fn inspect(path: &Path) -> Result<PackInfo> {
    let mut z = open(path)?;
    if let Some(idx) = read_json(&mut z, "modrinth.index.json") {
        let deps = idx["dependencies"].as_object().cloned().unwrap_or_default();
        let (core, lv) = core_from_deps(&deps);
        let files = idx["files"].as_array().cloned().unwrap_or_default();
        let server = files.iter().filter(|f| server_needed(f.get("env"))).count();
        return Ok(PackInfo {
            format: "modrinth".into(),
            name: s(&idx["name"]),
            version: s(&idx["versionId"]),
            mc: s(&deps.get("minecraft").cloned().unwrap_or_default()),
            core,
            loader_version: lv,
            files: files.len(),
            server_files: server,
            client_only: files.len() - server,
            needs_cf_key: false,
        });
    }
    if let Some(m) = read_json(&mut z, "manifest.json") {
        let loader = m["minecraft"]["modLoaders"]
            .as_array()
            .and_then(|a| a.iter().find(|l| l["primary"].as_bool() == Some(true)).or(a.first()))
            .map(|l| s(&l["id"]))
            .unwrap_or_default();
        let (core, lv) = match loader.split_once('-') {
            Some(("neoforge", v)) => (CoreKind::NeoForge, v.to_string()),
            Some(("forge", v)) => (CoreKind::Forge, v.to_string()),
            Some(("fabric", v)) => (CoreKind::Fabric, v.to_string()),
            Some(("quilt", v)) => (CoreKind::Quilt, v.to_string()),
            _ => (CoreKind::Vanilla, String::new()),
        };
        let n = m["files"].as_array().map(|a| a.len()).unwrap_or(0);
        return Ok(PackInfo {
            format: "curseforge".into(),
            name: s(&m["name"]),
            version: s(&m["version"]),
            mc: s(&m["minecraft"]["version"]),
            core,
            loader_version: lv,
            files: n,
            server_files: n,
            client_only: 0,
            needs_cf_key: true,
        });
    }
    Err(Error::other("это не сборка Modrinth (.mrpack) и не CurseForge (manifest.json)"))
}

/// Распаковать папки overrides/ и server-overrides/ (вторая важнее) в папку сервера.
fn extract_overrides(z: &mut zip::ZipArchive<fs::File>, dir: &Path, prefixes: &[&str]) -> Result<usize> {
    let mut n = 0;
    for prefix in prefixes {
        let pre = format!("{prefix}/");
        for i in 0..z.len() {
            let mut f = z.by_index(i)?;
            let name = f.name().to_string();
            let Some(rel) = name.strip_prefix(&pre) else { continue };
            if rel.is_empty() || f.is_dir() {
                continue;
            }
            let Some(dst) = safe_join(dir, rel) else { continue };
            if let Some(p) = dst.parent() {
                fs::create_dir_all(p)?;
            }
            let mut out = fs::File::create(&dst)?;
            std::io::copy(&mut f, &mut out)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Скачать файлы сборки в папку сервера (ядро ставится отдельно).
pub fn import_files(path: &Path, dir: &Path, cf_key: &str, mut progress: impl FnMut(usize, usize, &str)) -> Result<ImportResult> {
    let mut z = open(path)?;
    let mut r = ImportResult::default();
    fs::create_dir_all(dir)?;

    if let Some(idx) = read_json(&mut z, "modrinth.index.json") {
        let files = idx["files"].as_array().cloned().unwrap_or_default();
        let total = files.len();
        for (i, f) in files.iter().enumerate() {
            let rel = s(&f["path"]);
            if !server_needed(f.get("env")) {
                r.skipped_client += 1;
                continue;
            }
            let Some(dst) = safe_join(dir, &rel) else {
                r.failed.push(format!("{rel}: недопустимый путь"));
                continue;
            };
            let name = Path::new(&rel).file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or(rel.clone());
            progress(i + 1, total, &name);
            let sha1 = f["hashes"]["sha1"].as_str();
            let urls: Vec<String> = f["downloads"].as_array().into_iter().flatten().map(s).collect();
            let ok = urls.iter().any(|u| net::download(u, &dst, sha1, |_, _| {}).is_ok());
            if ok {
                r.downloaded += 1;
            } else {
                r.failed.push(name);
            }
        }
        r.overrides = extract_overrides(&mut z, dir, &["overrides", "server-overrides"])?;
        return Ok(r);
    }

    if let Some(m) = read_json(&mut z, "manifest.json") {
        let key = crate::catalog::clean_cf_key(cf_key);
        if key.is_empty() {
            return Err(Error::other("для сборки CurseForge нужен API-ключ — укажи его в настройках менеджера"));
        }
        let files: Vec<(u64, u64)> = m["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|f| f["required"].as_bool().unwrap_or(true))
            .filter_map(|f| Some((f["projectID"].as_u64()?, f["fileID"].as_u64()?)))
            .collect();
        // ссылки пачками по 50
        let mut meta: HashMap<u64, Value> = HashMap::new();
        for chunk in files.chunks(50) {
            let ids: Vec<u64> = chunk.iter().map(|x| x.1).collect();
            let resp = cf_post("/v1/mods/files", &json!({ "fileIds": ids }), &key)?;
            for f in resp["data"].as_array().into_iter().flatten() {
                if let Some(id) = f["id"].as_u64() {
                    meta.insert(id, f.clone());
                }
            }
        }
        // проекты: нужен classId (6 — мод, 12 — ресурспак, 6552 — шейдер) и признак «только клиент»
        let pids: Vec<u64> = files.iter().map(|x| x.0).collect();
        let mut projects: HashMap<u64, Value> = HashMap::new();
        for chunk in pids.chunks(50) {
            if let Ok(resp) = cf_post("/v1/mods", &json!({ "modIds": chunk }), &key) {
                for p in resp["data"].as_array().into_iter().flatten() {
                    if let Some(id) = p["id"].as_u64() {
                        projects.insert(id, p.clone());
                    }
                }
            }
        }
        let total = files.len();
        for (i, (pid, fid)) in files.iter().enumerate() {
            let Some(f) = meta.get(fid) else {
                r.failed.push(format!("файл {fid}"));
                continue;
            };
            let name = net::safe_file_name(&s(&f["fileName"]));
            let class = projects.get(pid).and_then(|p| p["classId"].as_u64()).unwrap_or(6);
            if class != 6 {
                // ресурспаки и шейдеры серверу не нужны
                r.skipped_client += 1;
                continue;
            }
            progress(i + 1, total, &name);
            let url = s(&f["downloadUrl"]);
            let page = projects
                .get(pid)
                .map(|p| format!("{}/files/{fid}", s(&p["links"]["websiteUrl"])))
                .unwrap_or_default();
            if url.is_empty() {
                r.manual.push((name, page));
                continue;
            }
            let sha1 = f["hashes"].as_array().and_then(|a| a.iter().find(|h| h["algo"].as_u64() == Some(1))).map(|h| s(&h["value"]));
            match net::download(&url, &dir.join("mods").join(&name), sha1.as_deref(), |_, _| {}) {
                Ok(()) => r.downloaded += 1,
                Err(_) => r.failed.push(name),
            }
        }
        let ov = s(&m["overrides"]);
        r.overrides = extract_overrides(&mut z, dir, &[if ov.is_empty() { "overrides" } else { ov.as_str() }])?;
        return Ok(r);
    }
    Err(Error::other("это не сборка"))
}

fn cf_post(path: &str, body: &Value, key: &str) -> Result<Value> {
    crate::util::http()
        .post(&format!("https://api.curseforge.com{path}"))
        .set("User-Agent", net::UA)
        .set("x-api-key", key)
        .send_json(body.clone())
        .map_err(|e| Error::Network(e.to_string()))?
        .into_json()
        .map_err(|e| Error::Network(e.to_string()))
}

// ===================================================================== экспорт

#[derive(Debug, Clone, Serialize, Default)]
pub struct ExportResult {
    pub path: String,
    /// моды ссылками на Modrinth
    pub linked: usize,
    /// моды целиком внутри архива (не нашлись на Modrinth)
    pub embedded: usize,
    /// клиентские моды, убранные с сервера и добавленные в сборку
    pub client_added: usize,
    /// серверные моды, которые игрокам не нужны
    pub server_only: usize,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub name: String,
    pub version: String,
    pub mc: String,
    pub core: CoreKind,
    pub loader_version: String,
    pub include_config: bool,
}

/// Собрать клиентскую сборку .mrpack.
pub fn export_client(server_dir: &Path, settings: &ModSettings, opt: &ExportOptions, dst: &Path) -> Result<ExportResult> {
    let report = mods::list(server_dir, opt.core.mod_loader(), settings);
    // какие файлы берём: включённые + убранные как клиентские
    let mut chosen: Vec<(PathBuf, String, bool)> = Vec::new(); // (путь, имя, клиентский-с-сервера)
    for m in &report.mods {
        let p = if m.enabled { server_dir.join(mods::ENABLED_DIR).join(&m.file) } else { server_dir.join(mods::DISABLED_DIR).join(&m.file) };
        if m.enabled {
            chosen.push((p, m.file.clone(), false));
        } else if m.disabled_reason.as_deref() == Some("client") || m.meta.client != mods::ClientConfidence::No {
            // клиентские моды, которые сняты с сервера (автоотсевом, карантином или вручную), — игрокам
            chosen.push((p, m.file.clone(), true));
        }
    }

    // хэши → версии на Modrinth
    let mut hashes: HashMap<String, usize> = HashMap::new();
    for (i, (p, _, _)) in chosen.iter().enumerate() {
        if let Ok(h) = net::sha1_file(p) {
            hashes.insert(h, i);
        }
    }
    let keys: Vec<&String> = hashes.keys().collect();
    let found: Value = if keys.is_empty() {
        json!({})
    } else {
        net::post_json("https://api.modrinth.com/v2/version_files", &json!({ "hashes": keys, "algorithm": "sha1" })).unwrap_or(json!({}))
    };
    // client_side / server_side проектов
    let pids: Vec<String> = found.as_object().map(|o| o.values().map(|v| s(&v["project_id"])).collect()).unwrap_or_default();
    let mut sides: HashMap<String, (String, String)> = HashMap::new();
    for chunk in pids.chunks(100) {
        let url = format!("https://api.modrinth.com/v2/projects?ids={}", urlenc(&serde_json::to_string(chunk)?));
        if let Ok(v) = net::get_json(&url) {
            for p in v.as_array().into_iter().flatten() {
                sides.insert(s(&p["id"]), (s(&p["client_side"]), s(&p["server_side"])));
            }
        }
    }

    let mut res = ExportResult::default();
    let mut files_json = Vec::new();
    let mut embed: Vec<(PathBuf, String)> = Vec::new();
    for (h, i) in &hashes {
        let (path, name, client_from_server) = &chosen[*i];
        match found.get(h) {
            Some(ver) => {
                let pid = s(&ver["project_id"]);
                let (cs, ss) = sides.get(&pid).cloned().unwrap_or_default();
                if cs == "unsupported" {
                    res.server_only += 1;
                    continue;
                }
                let file = ver["files"]
                    .as_array()
                    .and_then(|a| a.iter().find(|f| f["hashes"]["sha1"].as_str() == Some(h.as_str())))
                    .cloned()
                    .unwrap_or(Value::Null);
                // Флаги Modrinth бывают неточными (у Create «клиент: необязательно»), поэтому:
                // мод, обязательный на сервере, обязателен и игроку; необязательны только
                // моды, необязательные с обеих сторон (оптимизации), и клиентские, снятые с сервера.
                let client_env = if *client_from_server || (cs == "optional" && ss != "required") { "optional" } else { "required" };
                let server_env = if *client_from_server { "unsupported" } else { "required" };
                files_json.push(json!({
                    "path": format!("mods/{name}"),
                    "hashes": { "sha1": h, "sha512": s(&file["hashes"]["sha512"]) },
                    "env": { "client": client_env, "server": server_env },
                    "downloads": [ s(&file["url"]) ],
                    "fileSize": file["size"].as_u64().unwrap_or(0),
                }));
                res.linked += 1;
            }
            None => {
                embed.push((path.clone(), name.clone()));
                res.embedded += 1;
            }
        }
        if *client_from_server {
            res.client_added += 1;
        }
    }
    files_json.sort_by(|a, b| s(&a["path"]).cmp(&s(&b["path"])));

    let loader_key = match opt.core.mod_loader() {
        Some(crate::detect::ModLoader::Fabric) => "fabric-loader",
        Some(crate::detect::ModLoader::Quilt) => "quilt-loader",
        Some(crate::detect::ModLoader::Forge) => "forge",
        _ => "neoforge",
    };
    let mut deps = json!({ "minecraft": opt.mc });
    if !opt.loader_version.is_empty() {
        deps[loader_key] = json!(opt.loader_version);
    }
    let index = json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": opt.version,
        "name": opt.name,
        "summary": "Клиентская сборка сервера (ProtoCore-MCS-Manager)",
        "files": files_json,
        "dependencies": deps,
    });

    if let Some(p) = dst.parent() {
        fs::create_dir_all(p)?;
    }
    let tmp = dst.with_extension("part");
    {
        let mut zw = zip::ZipWriter::new(fs::File::create(&tmp)?);
        let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zw.start_file("modrinth.index.json", o)?;
        zw.write_all(serde_json::to_string_pretty(&index)?.as_bytes())?;
        for (p, name) in &embed {
            zw.start_file(format!("overrides/mods/{name}"), o)?;
            zw.write_all(&fs::read(p)?)?;
        }
        if opt.include_config {
            let cfg = server_dir.join("config");
            let mut stack = vec![cfg.clone()];
            while let Some(d) = stack.pop() {
                for e in fs::read_dir(&d).into_iter().flatten().flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if let Ok(rel) = p.strip_prefix(server_dir) {
                        zw.start_file(format!("overrides/{}", rel.to_string_lossy().replace('\\', "/")), o)?;
                        zw.write_all(&fs::read(&p)?)?;
                    }
                }
            }
        }
        zw.finish()?;
    }
    fs::rename(&tmp, dst)?;
    res.size = fs::metadata(dst).map(|m| m.len()).unwrap_or(0);
    res.path = dst.to_string_lossy().to_string();
    Ok(res)
}

fn urlenc(x: &str) -> String {
    let mut out = String::new();
    for b in x.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_paths() {
        let b = Path::new("/srv/x");
        assert!(safe_join(b, "mods/a.jar").is_some());
        assert!(safe_join(b, "../evil.jar").is_none());
        assert!(safe_join(b, "/etc/passwd").is_none());
        assert!(safe_join(b, "config/../../x").is_none());
    }

    fn make_mrpack(dir: &Path) -> PathBuf {
        let p = dir.join("t.mrpack");
        let mut z = zip::ZipWriter::new(fs::File::create(&p).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("modrinth.index.json", o).unwrap();
        z.write_all(
            br#"{"formatVersion":1,"game":"minecraft","versionId":"1.0","name":"Test Pack",
            "dependencies":{"minecraft":"1.21.1","neoforge":"21.1.77"},
            "files":[{"path":"mods/client.jar","env":{"client":"required","server":"unsupported"},"downloads":[],"hashes":{}}]}"#,
        )
        .unwrap();
        z.start_file("overrides/config/a.toml", o).unwrap();
        z.write_all(b"a=1").unwrap();
        z.start_file("server-overrides/config/a.toml", o).unwrap();
        z.write_all(b"a=server").unwrap();
        z.start_file("overrides/../escape.txt", o).unwrap();
        z.write_all(b"no").unwrap();
        z.finish().unwrap();
        p
    }

    #[test]
    fn inspect_and_import_mrpack_offline() {
        let t = tempfile::tempdir().unwrap();
        let p = make_mrpack(t.path());
        let i = inspect(&p).unwrap();
        assert_eq!(i.core, CoreKind::NeoForge);
        assert_eq!(i.loader_version, "21.1.77");
        assert_eq!(i.client_only, 1);
        let dir = t.path().join("srv");
        let r = import_files(&p, &dir, "", |_, _, _| {}).unwrap();
        assert_eq!(r.skipped_client, 1);
        assert_eq!(fs::read_to_string(dir.join("config/a.toml")).unwrap(), "a=server");
        assert!(!t.path().join("escape.txt").exists());
    }

    /// Живая проверка: настоящая сборка Create+ (если скачана в /tmp/claude-0/mrp/pack.mrpack).
    #[test]
    #[ignore]
    fn live_real_pack() {
        let p = Path::new("/tmp/claude-0/mrp/pack.mrpack");
        let i = inspect(p).unwrap();
        assert_eq!(i.core, CoreKind::NeoForge);
        let t = tempfile::tempdir().unwrap();
        let r = import_files(p, t.path(), "", |_, _, _| {}).unwrap();
        eprintln!("{r:?}");
        assert!(r.downloaded > 50);
        assert!(r.failed.is_empty(), "{:?}", r.failed);
        assert_eq!(r.skipped_client, i.client_only);
    }
}
