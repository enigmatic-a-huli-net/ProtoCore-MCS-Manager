//! Ядра серверов: списки версий, сборки и установка.
//!
//! | Ядро            | Источник                                 | Установка                 |
//! |-----------------|------------------------------------------|---------------------------|
//! | Vanilla         | piston-meta.mojang.com                   | server.jar                |
//! | Paper, Folia    | fill.papermc.io (v3)                     | jar                       |
//! | Purpur          | api.purpurmc.org                         | jar                       |
//! | Fabric          | meta.fabricmc.net                        | готовый server-launcher   |
//! | Forge           | maven.minecraftforge.net                 | установщик --installServer|
//! | NeoForge        | maven.neoforged.net                      | установщик --installServer|
//! | Youer, Mohist   | api.mohistmc.com (+ запасной mohistmc.com)| jar                      |
//! | Arclight        | GitHub Releases IzzelAliz/Arclight       | jar                       |

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::detect::{neoforge_to_mc, version_cmp, CoreKind};
use crate::net;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct McVersion {
    pub id: String,
    pub stable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Build {
    pub id: String,
    pub label: String,
    pub stable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    /// Имя jar сервера, если ядро запускается через -jar
    pub jar: Option<String>,
    pub build: String,
}

/// Какие ядра менеджер умеет скачивать.
pub const DOWNLOADABLE: &[CoreKind] = &[
    CoreKind::Vanilla,
    CoreKind::Paper,
    CoreKind::Purpur,
    CoreKind::Folia,
    CoreKind::Fabric,
    CoreKind::Forge,
    CoreKind::NeoForge,
    CoreKind::Youer,
    CoreKind::Mohist,
    CoreKind::Arclight,
];

// --------------------------------------------------------------- мелочи

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

fn is_release(v: &str) -> bool {
    Regex::new(r"^\d+(\.\d+)+$").unwrap().is_match(v)
}

fn sort_desc(v: &mut [String]) {
    v.sort_by(|a, b| version_cmp(b, a));
}

fn mohist_name(core: CoreKind) -> &'static str {
    match core {
        CoreKind::Youer => "youer",
        _ => "mohist",
    }
}

fn paper_project(core: CoreKind) -> &'static str {
    match core {
        CoreKind::Folia => "folia",
        _ => "paper",
    }
}

// --------------------------------------------------------------- версии Minecraft

pub fn mc_versions(core: CoreKind) -> Result<Vec<McVersion>> {
    let list: Vec<McVersion> = match core {
        CoreKind::Vanilla => {
            let j = net::get_json("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")?;
            arr(&j["versions"])
                .iter()
                .filter(|v| matches!(v["type"].as_str(), Some("release") | Some("snapshot")))
                .map(|v| McVersion { id: s(&v["id"]), stable: v["type"] == "release" })
                .collect()
        }
        CoreKind::Paper | CoreKind::Folia => {
            let j = net::get_json(&format!("https://fill.papermc.io/v3/projects/{}", paper_project(core)))?;
            // versions: { "1.21": ["1.21.4", "1.21.3"...], ... } — группы, новые первыми
            let mut v: Vec<String> = Vec::new();
            match &j["versions"] {
                Value::Object(groups) => {
                    for (_, list) in groups {
                        v.extend(arr(list).iter().map(s));
                    }
                }
                Value::Array(a) => v.extend(a.iter().map(s)),
                _ => {}
            }
            sort_desc(&mut v);
            v.into_iter().map(|id| McVersion { stable: is_release(&id), id }).collect()
        }
        CoreKind::Purpur => {
            let j = net::get_json("https://api.purpurmc.org/v2/purpur")?;
            let mut v: Vec<String> = arr(&j["versions"]).iter().map(s).collect();
            sort_desc(&mut v);
            v.into_iter().map(|id| McVersion { stable: true, id }).collect()
        }
        CoreKind::Fabric => {
            let j = net::get_json("https://meta.fabricmc.net/v2/versions/game")?;
            arr(&j)
                .iter()
                .map(|v| McVersion { id: s(&v["version"]), stable: v["stable"].as_bool().unwrap_or(false) })
                .collect()
        }
        CoreKind::Forge => {
            let j = net::get_json("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json")?;
            let mut v: Vec<String> = j["promos"]
                .as_object()
                .map(|o| o.keys().filter_map(|k| k.split_once('-').map(|x| x.0.to_string())).collect())
                .unwrap_or_default();
            v.sort();
            v.dedup();
            sort_desc(&mut v);
            v.into_iter().map(|id| McVersion { stable: true, id }).collect()
        }
        CoreKind::NeoForge => {
            let mut v: Vec<String> = neoforge_versions()?.iter().map(|x| neoforge_to_mc(x)).filter(|x| !x.is_empty()).collect();
            v.sort();
            v.dedup();
            sort_desc(&mut v);
            v.into_iter().map(|id| McVersion { stable: true, id }).collect()
        }
        CoreKind::Youer | CoreKind::Mohist => mohist_versions(mohist_name(core))?
            .into_iter()
            .map(|id| McVersion { stable: true, id })
            .collect(),
        CoreKind::Arclight => {
            let mut v: Vec<String> = arclight_assets()?.into_iter().map(|a| a.mc).collect();
            v.sort();
            v.dedup();
            sort_desc(&mut v);
            v.into_iter().map(|id| McVersion { stable: true, id }).collect()
        }
        _ => return Err(Error::other("это ядро пока нельзя скачать из менеджера")),
    };
    if list.is_empty() {
        return Err(Error::Network("список версий пуст".into()));
    }
    Ok(list)
}

fn neoforge_versions() -> Result<Vec<String>> {
    let j = net::get_json("https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge")?;
    Ok(arr(&j["versions"]).iter().map(s).collect())
}

/// Префикс версий NeoForge для Minecraft: 1.21.1 → "21.1.", 1.21 → "21.0.", 26.1 → "26.1.".
pub fn neoforge_prefix(mc: &str) -> String {
    let p: Vec<&str> = mc.split('.').collect();
    if p.first() == Some(&"1") {
        let major = p.get(1).copied().unwrap_or("0");
        let minor = p.get(2).copied().unwrap_or("0");
        format!("{major}.{minor}.")
    } else {
        let major = p.first().copied().unwrap_or("0");
        let minor = p.get(1).copied().unwrap_or("0");
        format!("{major}.{minor}.")
    }
}

fn mohist_versions(p: &str) -> Result<Vec<String>> {
    // новый API
    let parse = |j: &Value| -> Vec<String> {
        let list = if j.is_array() { arr(j) } else { arr(&j["versions"]) };
        list.iter()
            .map(|x| if x.is_object() { s(x.get("name").or(x.get("version")).or(x.get("id")).unwrap_or(&Value::Null)) } else { s(x) })
            .filter(|x| !x.is_empty())
            .collect()
    };
    let mut v = net::get_json(&format!("https://api.mohistmc.com/project/{p}/versions"))
        .map(|j| parse(&j))
        .unwrap_or_default();
    if v.is_empty() {
        v = net::get_json(&format!("https://mohistmc.com/api/v2/projects/{p}"))
            .map(|j| parse(&j))
            .unwrap_or_default();
    }
    if v.is_empty() {
        // API не ответил — известные версии, чтобы мастер не был пустым
        v = if p == "youer" { vec!["1.21.1".into()] } else { vec!["1.20.1".into(), "1.16.5".into(), "1.12.2".into()] };
    }
    sort_desc(&mut v);
    Ok(v)
}

struct ArclightAsset {
    mc: String,
    loader: String,
    version: String,
    url: String,
    name: String,
}

fn arclight_assets() -> Result<Vec<ArclightAsset>> {
    let j = net::get_json("https://api.github.com/repos/IzzelAliz/Arclight/releases?per_page=30")?;
    let re = Regex::new(r"(?i)^arclight-(forge|neoforge|fabric)-([\d.]+)-(.+)\.jar$").unwrap();
    let mut out = Vec::new();
    for rel in arr(&j) {
        for a in arr(&rel["assets"]) {
            let name = s(&a["name"]);
            if let Some(c) = re.captures(&name) {
                out.push(ArclightAsset {
                    loader: c[1].to_lowercase(),
                    mc: c[2].to_string(),
                    version: c[3].to_string(),
                    url: s(&a["browser_download_url"]),
                    name,
                });
            }
        }
    }
    Ok(out)
}

// --------------------------------------------------------------- сборки

/// Сборки ядра для версии Minecraft, новые первыми.
pub fn builds(core: CoreKind, mc: &str) -> Result<Vec<Build>> {
    let out: Vec<Build> = match core {
        CoreKind::Vanilla => vec![Build { id: mc.into(), label: format!("Minecraft {mc}"), stable: true }],
        CoreKind::Paper | CoreKind::Folia => {
            let j = net::get_json(&format!(
                "https://fill.papermc.io/v3/projects/{}/versions/{mc}/builds",
                paper_project(core)
            ))?;
            let mut v: Vec<Build> = arr(&j)
                .iter()
                .map(|b| Build {
                    id: s(&b["id"]),
                    label: format!("#{} {}", s(&b["id"]), s(&b["channel"]).to_lowercase()),
                    stable: b["channel"].as_str().map(|c| c.eq_ignore_ascii_case("STABLE")).unwrap_or(false),
                })
                .collect();
            v.sort_by(|a, b| version_cmp(&b.id, &a.id));
            v
        }
        CoreKind::Purpur => {
            let j = net::get_json(&format!("https://api.purpurmc.org/v2/purpur/{mc}"))?;
            let mut v: Vec<String> = arr(&j["builds"]["all"]).iter().map(s).collect();
            sort_desc(&mut v);
            v.into_iter().map(|id| Build { label: format!("#{id}"), id, stable: true }).collect()
        }
        CoreKind::Fabric => {
            let j = net::get_json("https://meta.fabricmc.net/v2/versions/loader")?;
            arr(&j)
                .iter()
                .map(|b| Build {
                    id: s(&b["version"]),
                    label: format!("loader {}", s(&b["version"])),
                    stable: b["stable"].as_bool().unwrap_or(false),
                })
                .collect()
        }
        CoreKind::Forge => {
            let xml = net::get_text("https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml")?;
            let re = Regex::new(r"<version>([^<]+)</version>").unwrap();
            let pre = format!("{mc}-");
            let mut v: Vec<String> = re
                .captures_iter(&xml)
                .map(|c| c[1].to_string())
                .filter(|x| x.starts_with(&pre))
                .map(|x| x[pre.len()..].to_string())
                .collect();
            sort_desc(&mut v);
            v.into_iter().map(|id| Build { label: format!("Forge {id}"), id, stable: true }).collect()
        }
        CoreKind::NeoForge => {
            let pre = neoforge_prefix(mc);
            let mut v: Vec<String> = neoforge_versions()?.into_iter().filter(|x| x.starts_with(&pre)).collect();
            sort_desc(&mut v);
            v.into_iter()
                .map(|id| Build { stable: !id.contains("beta") && !id.contains("alpha"), label: format!("NeoForge {id}"), id })
                .collect()
        }
        CoreKind::Youer | CoreKind::Mohist => {
            let p = mohist_name(core);
            let j = net::get_json(&format!("https://api.mohistmc.com/project/{p}/{mc}/builds"))
                .or_else(|_| net::get_json(&format!("https://mohistmc.com/api/v2/projects/{p}/{mc}/builds")))?;
            let list = if j.is_array() { arr(&j) } else { arr(&j["builds"]) };
            let mut v: Vec<Build> = list
                .iter()
                .map(|b| {
                    let id = s(b.get("id").or(b.get("number")).or(b.get("name")).unwrap_or(&Value::Null));
                    let date = s(b.get("createdAt").or(b.get("created_at")).or(b.get("date")).or(b.get("buildDate")).unwrap_or(&Value::Null));
                    let date: String = date.chars().take(10).collect();
                    Build { label: if date.is_empty() { id.clone() } else { format!("{id} · {date}") }, id, stable: true }
                })
                .filter(|b| !b.id.is_empty())
                .collect();
            // старый API отдаёт по возрастанию номеров — разворачиваем
            if !j.is_array() {
                v.reverse();
            }
            v
        }
        CoreKind::Arclight => arclight_assets()?
            .into_iter()
            .filter(|a| a.mc == mc)
            .map(|a| Build { id: format!("{}:{}", a.loader, a.version), label: format!("{} {}", a.loader, a.version), stable: true })
            .collect(),
        _ => return Err(Error::other("это ядро пока нельзя скачать из менеджера")),
    };
    if out.is_empty() {
        return Err(Error::Network(format!("нет сборок {} для Minecraft {mc}", core.label())));
    }
    Ok(out)
}

// --------------------------------------------------------------- установка

/// Проверка, что Java запускается. Возвращает строку версии.
pub fn java_version(java: &str) -> Result<String> {
    let mut c = Command::new(java);
    c.arg("-version").stdout(Stdio::null()).stderr(Stdio::piped());
    hide(&mut c);
    let out = c.output().map_err(|e| Error::other(format!("Java не найдена ({java}): {e}. Установи Java 21 или укажи путь к java.exe")))?;
    let s = String::from_utf8_lossy(&out.stderr);
    // первой строкой бывает «Picked up JAVA_TOOL_OPTIONS…» — берём строку с версией
    Ok(s.lines()
        .find(|l| l.contains(" version "))
        .or_else(|| s.lines().find(|l| !l.starts_with("Picked up")))
        .unwrap_or("")
        .trim()
        .to_string())
}

fn hide(c: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let _ = c;
}

fn run_installer(java: &str, installer: &Path, dir: &Path, log: &mut dyn FnMut(String)) -> Result<()> {
    log(format!("▶ {java} -jar {} --installServer", crate::util::file_name(installer)));
    let mut c = Command::new(java);
    c.arg("-jar").arg(installer).arg("--installServer").current_dir(dir).stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    hide(&mut c);
    let mut child = c.spawn().map_err(|e| Error::other(format!("не удалось запустить установщик: {e}")))?;
    let err = child.stderr.take();
    let err_t = std::thread::spawn(move || {
        let mut v = Vec::new();
        if let Some(e) = err {
            for l in BufReader::new(e).lines().map_while(std::result::Result::ok) {
                v.push(l);
            }
        }
        v
    });
    if let Some(out) = child.stdout.take() {
        let mut n = 0usize;
        for l in BufReader::new(out).lines().map_while(std::result::Result::ok) {
            n += 1;
            // установщик очень болтлив — показываем каждую 20-ю строку и важные
            if n % 20 == 0 || l.contains("Downloading") && n % 5 == 0 || l.contains("rror") || l.contains("Success") || l.contains("successfully") {
                log(l);
            }
        }
    }
    let st = child.wait()?;
    for l in err_t.join().unwrap_or_default().into_iter().rev().take(5).rev() {
        log(l);
    }
    if !st.success() {
        return Err(Error::other(format!("установщик завершился с ошибкой (код {:?})", st.code())));
    }
    let _ = fs::remove_file(installer);
    let _ = fs::remove_file(dir.join(format!("{}.log", crate::util::file_name(installer))));
    let _ = fs::remove_file(dir.join("installer.log"));
    Ok(())
}

/// Скачивает и устанавливает ядро в `dir`. `build` пустой — последняя стабильная сборка.
pub fn install(
    core: CoreKind,
    mc: &str,
    build: &str,
    dir: &Path,
    java: &str,
    mut log: impl FnMut(String),
    mut progress: impl FnMut(u64, u64),
) -> Result<InstallResult> {
    fs::create_dir_all(dir)?;
    let pick = |b: &str| -> Result<String> {
        if !b.is_empty() {
            return Ok(b.to_string());
        }
        let list = builds(core, mc)?;
        Ok(list.iter().find(|x| x.stable).or(list.first()).map(|x| x.id.clone()).unwrap_or_default())
    };

    match core {
        CoreKind::Vanilla => {
            let j = net::get_json("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")?;
            let url = arr(&j["versions"])
                .iter()
                .find(|v| v["id"] == mc)
                .map(|v| s(&v["url"]))
                .ok_or_else(|| Error::other(format!("нет версии {mc}")))?;
            let vj = net::get_json(&url)?;
            let dl = &vj["downloads"]["server"];
            if dl.is_null() {
                return Err(Error::other("для этой версии нет серверного jar"));
            }
            let name = format!("minecraft_server.{mc}.jar");
            log(format!("⬇ {name}"));
            net::download(&s(&dl["url"]), &dir.join(&name), Some(&s(&dl["sha1"])), &mut progress)?;
            Ok(InstallResult { jar: Some(name), build: mc.into() })
        }
        CoreKind::Paper | CoreKind::Folia => {
            let b = pick(build)?;
            let j = net::get_json(&format!(
                "https://fill.papermc.io/v3/projects/{}/versions/{mc}/builds/{b}",
                paper_project(core)
            ))?;
            let d = &j["downloads"]["server:default"];
            let url = s(&d["url"]);
            if url.is_empty() {
                return Err(Error::Network("PaperMC не вернул ссылку на скачивание".into()));
            }
            let name = net::safe_file_name(&s(&d["name"]));
            let name = if name.is_empty() { format!("{}-{mc}-{b}.jar", paper_project(core)) } else { name };
            log(format!("⬇ {name}"));
            net::download(&url, &dir.join(&name), None, &mut progress)?;
            Ok(InstallResult { jar: Some(name), build: b })
        }
        CoreKind::Purpur => {
            let b = pick(build)?;
            let name = format!("purpur-{mc}-{b}.jar");
            log(format!("⬇ {name}"));
            net::download(&format!("https://api.purpurmc.org/v2/purpur/{mc}/{b}/download"), &dir.join(&name), None, &mut progress)?;
            Ok(InstallResult { jar: Some(name), build: b })
        }
        CoreKind::Fabric => {
            let b = pick(build)?;
            let inst = net::get_json("https://meta.fabricmc.net/v2/versions/installer")?;
            let iv = arr(&inst)
                .iter()
                .find(|x| x["stable"].as_bool().unwrap_or(false))
                .or(arr(&inst).first())
                .map(|x| s(&x["version"]))
                .unwrap_or_else(|| "1.0.1".into());
            let name = format!("fabric-server-mc.{mc}-loader.{b}-launcher.{iv}.jar");
            log(format!("⬇ {name}"));
            net::download(
                &format!("https://meta.fabricmc.net/v2/versions/loader/{mc}/{b}/{iv}/server/jar"),
                &dir.join(&name),
                None,
                &mut progress,
            )?;
            Ok(InstallResult { jar: Some(name), build: b })
        }
        CoreKind::Forge | CoreKind::NeoForge => {
            let b = pick(build)?;
            java_version(java)?;
            let (url, name) = if core == CoreKind::Forge {
                (
                    format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{b}/forge-{mc}-{b}-installer.jar"),
                    format!("forge-{mc}-{b}-installer.jar"),
                )
            } else {
                (
                    format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{b}/neoforge-{b}-installer.jar"),
                    format!("neoforge-{b}-installer.jar"),
                )
            };
            log(format!("⬇ {name}"));
            let inst = dir.join(&name);
            net::download(&url, &inst, None, &mut progress)?;
            log("⚙ Установка (скачиваются библиотеки, это может занять пару минут)…".into());
            run_installer(java, &inst, dir, &mut log)?;
            // старые Forge (< 1.17) кладут forge-*.jar в корень
            let jar = crate::util::list_jars(dir)
                .into_iter()
                .map(|p| crate::util::file_name(&p))
                .find(|n| n.starts_with("forge-") && n.contains(&b) && !n.contains("installer"));
            Ok(InstallResult { jar, build: b })
        }
        CoreKind::Youer | CoreKind::Mohist => {
            let p = mohist_name(core);
            let b = pick(build)?;
            let name = format!("{p}-{mc}-{b}.jar");
            log(format!("⬇ {name}"));
            let url = format!("https://api.mohistmc.com/project/{p}/{mc}/builds/{b}/download");
            if net::download(&url, &dir.join(&name), None, &mut progress).is_err() {
                net::download(
                    &format!("https://mohistmc.com/api/v2/projects/{p}/{mc}/builds/{b}/download"),
                    &dir.join(&name),
                    None,
                    &mut progress,
                )?;
            }
            Ok(InstallResult { jar: Some(name), build: b })
        }
        CoreKind::Arclight => {
            let b = pick(build)?;
            let (loader, ver) = b.split_once(':').unwrap_or(("", b.as_str()));
            let a = arclight_assets()?
                .into_iter()
                .find(|a| a.mc == mc && a.version == ver && (loader.is_empty() || a.loader == loader))
                .ok_or_else(|| Error::other("сборка Arclight не найдена"))?;
            log(format!("⬇ {}", a.name));
            net::download(&a.url, &dir.join(&a.name), None, &mut progress)?;
            Ok(InstallResult { jar: Some(a.name.clone()), build: b.clone() })
        }
        _ => Err(Error::other("это ядро пока нельзя скачать из менеджера")),
    }
}

/// Перед обновлением ядра убирает старые jar ядра в cores_old/ (ничего не удаляется).
pub fn archive_old_core_jars(dir: &Path, core: CoreKind) -> Result<Vec<PathBuf>> {
    let prefixes: &[&str] = match core {
        CoreKind::Vanilla => &["minecraft_server", "server.jar"],
        CoreKind::Paper => &["paper-"],
        CoreKind::Folia => &["folia-"],
        CoreKind::Purpur => &["purpur-"],
        CoreKind::Fabric => &["fabric-server-"],
        CoreKind::Youer => &["youer-"],
        CoreKind::Mohist => &["mohist-"],
        CoreKind::Arclight => &["arclight-"],
        CoreKind::Forge => &["forge-"],
        _ => &[],
    };
    let mut moved = Vec::new();
    for j in crate::util::list_jars(dir) {
        let n = crate::util::file_name(&j).to_lowercase();
        if prefixes.iter().any(|p| n.starts_with(p)) {
            moved.push(crate::util::move_file(&j, &dir.join("cores_old"))?);
        }
    }
    Ok(moved)
}

/// Файлы нового сервера: eula, server.properties с рекомендациями.
pub fn write_defaults(dir: &Path, core: CoreKind, accept_eula: bool, motd: &str) -> Result<()> {
    if accept_eula {
        fs::write(dir.join("eula.txt"), "# https://aka.ms/MinecraftEULA\n# принято через ProtoCore-MCS-Manager\neula=true\n")?;
    }
    if !dir.join("server.properties").exists() {
        crate::props::write(
            dir,
            &[
                ("motd".into(), motd.into()),
                ("view-distance".into(), "10".into()),
                ("simulation-distance".into(), "6".into()),
                ("network-compression-threshold".into(), "256".into()),
                ("sync-chunk-writes".into(), "true".into()),
            ],
        )?;
    }
    if core.supports_mods() {
        fs::create_dir_all(dir.join("mods")).ok();
    }
    if core.supports_plugins() {
        fs::create_dir_all(dir.join("plugins")).ok();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neo_prefix() {
        assert_eq!(neoforge_prefix("1.21.1"), "21.1.");
        assert_eq!(neoforge_prefix("1.21"), "21.0.");
        assert_eq!(neoforge_prefix("26.1"), "26.1.");
    }

    /// Живая проверка (сеть): NeoForge и Vanilla.
    #[test]
    #[ignore]
    fn live_lists() {
        let v = mc_versions(CoreKind::NeoForge).unwrap();
        assert!(v.iter().any(|x| x.id == "1.21.1"));
        let b = builds(CoreKind::NeoForge, "1.21.1").unwrap();
        assert!(b[0].id.starts_with("21.1."));
        let van = mc_versions(CoreKind::Vanilla).unwrap();
        assert!(van.iter().any(|x| x.id == "1.21.1" && x.stable));
    }
}
