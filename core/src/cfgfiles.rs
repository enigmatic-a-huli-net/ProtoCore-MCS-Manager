//! Файлы настроек сервера: server.properties, bukkit.yml, spigot.yml, paper-*.yml,
//! purpur.yml, конфиги модов (*.toml) и плагинов (plugins/*/config.yml).
//!
//! Правка «хирургическая»: файл разбирается построчно, и при записи меняется только
//! текст значения в нужной строке — комментарии, порядок и отступы остаются как были.
//! Сложные значения (списки, многострочные блоки) показываются, но правятся только
//! в режиме «Текст».

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const MAX_SIZE: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Properties,
    Yaml,
    Toml,
}

#[derive(Debug, Clone, Serialize)]
pub struct CfgFile {
    /// путь относительно папки сервера, через «/»
    pub rel: String,
    /// core / mods / plugins
    pub group: String,
    /// понятное название («Paper — мир по умолчанию»)
    pub title: String,
    pub format: Format,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bool,
    Int,
    Float,
    Str,
    /// список, вложенная структура, многострочный текст — только просмотр
    Raw,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    /// путь ключа: «world-settings.default.view-distance»
    pub path: String,
    pub key: String,
    pub value: String,
    pub kind: Kind,
    pub editable: bool,
    /// номер строки (0-based)
    pub line: usize,
    /// комментарий из файла над ключом
    pub comment: String,
    /// подсказка из встроенного справочника (если есть)
    pub hint: String,
}

fn rx(cell: &'static OnceLock<Regex>, p: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(p).unwrap())
}

// ===================================================================== поиск файлов

/// Известные файлы ядер: (путь, название)
const KNOWN: &[(&str, &str)] = &[
    ("server.properties", "Основные настройки"),
    ("bukkit.yml", "Bukkit: мобы, автосохранение"),
    ("spigot.yml", "Spigot: мир, сообщения"),
    ("config/paper-global.yml", "Paper: общие"),
    ("config/paper-world-defaults.yml", "Paper: мир по умолчанию"),
    ("paper.yml", "Paper (старый формат)"),
    ("purpur.yml", "Purpur"),
    ("pufferfish.yml", "Pufferfish"),
    ("leaves.yml", "Leaves"),
    ("config/gale-global.yml", "Gale: общие"),
    ("config/gale-world-defaults.yml", "Gale: мир по умолчанию"),
    ("arclight.conf", "Arclight"),
    ("youer.yml", "Youer"),
    ("mohist-config/mohist.yml", "Mohist"),
    ("commands.yml", "Псевдонимы команд"),
];

fn format_of(p: &Path) -> Option<Format> {
    match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "yml" | "yaml" => Some(Format::Yaml),
        "toml" => Some(Format::Toml),
        "properties" => Some(Format::Properties),
        _ => None,
    }
}

fn rel_of(dir: &Path, p: &Path) -> String {
    p.strip_prefix(dir).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

fn walk(dir: &Path, base: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(base) else { return };
    let mut items: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    items.sort();
    for p in items {
        if p.is_dir() {
            if depth > 0 {
                walk(dir, &p, depth - 1, out);
            }
        } else if format_of(&p).is_some() && fs::metadata(&p).map(|m| m.len() <= MAX_SIZE).unwrap_or(false) {
            out.push(p);
        }
    }
}

/// Все файлы настроек сервера, сгруппированные: ядро → моды → плагины.
pub fn discover(dir: &Path) -> Vec<CfgFile> {
    let mut out: Vec<CfgFile> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |p: &Path, group: &str, title: String, out: &mut Vec<CfgFile>| {
        let rel = rel_of(dir, p);
        if !seen.insert(rel.clone()) {
            return;
        }
        let Some(format) = format_of(p) else { return };
        let size = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        out.push(CfgFile { rel, group: group.into(), title, format, size });
    };
    for (rel, title) in KNOWN {
        let p = dir.join(rel);
        if p.is_file() {
            push(&p, "core", title.to_string(), &mut out);
        }
    }
    // прочие файлы в корне (кроме списков игроков и служебных)
    let mut root = Vec::new();
    walk(dir, dir, 0, &mut root);
    for p in root {
        let n = rel_of(dir, &p);
        if ["help.yml", "permissions.yml", "wepif.yml"].contains(&n.as_str()) {
            continue;
        }
        push(&p, "core", n.clone(), &mut out);
    }
    // моды: config/, defaultconfigs/, world/serverconfig/
    let level = crate::props::read(dir)
        .into_iter()
        .find(|(k, _)| k == "level-name")
        .map(|(_, v)| v)
        .unwrap_or_else(|| "world".into());
    for (sub, depth) in [("config".to_string(), 3), ("defaultconfigs".to_string(), 2), (format!("{level}/serverconfig"), 1)] {
        let mut v = Vec::new();
        walk(dir, &dir.join(&sub), depth, &mut v);
        for p in v {
            let rel = rel_of(dir, &p);
            push(&p, "mods", rel, &mut out);
        }
    }
    // плагины: plugins/<имя>/*.yml (без глубоких папок с данными)
    if let Ok(rd) = fs::read_dir(dir.join("plugins")) {
        let mut plugs: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        plugs.sort();
        for pd in plugs {
            let mut v = Vec::new();
            walk(dir, &pd, 0, &mut v);
            for p in v {
                let rel = rel_of(dir, &p);
                push(&p, "plugins", rel, &mut out);
            }
        }
    }
    out
}

/// Путь из интерфейса — только относительный, без «..», и только из списка найденных.
pub fn resolve(dir: &Path, rel: &str) -> Result<(PathBuf, Format)> {
    let p = Path::new(rel);
    let bad = rel.is_empty() || p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_)));
    if bad || !discover(dir).iter().any(|f| f.rel == rel) {
        return Err(Error::other(format!("файл настроек не найден: {rel}")));
    }
    let full = dir.join(p);
    let f = format_of(&full).ok_or_else(|| Error::other("неизвестный формат"))?;
    Ok((full, f))
}

// ===================================================================== разбор

fn kind_of(v: &str) -> Kind {
    let t = v.trim();
    if t == "true" || t == "false" {
        Kind::Bool
    } else if t.parse::<i64>().is_ok() {
        Kind::Int
    } else if t.parse::<f64>().is_ok() && t.chars().any(|c| c == '.') {
        Kind::Float
    } else {
        Kind::Str
    }
}

fn unquote(v: &str) -> (String, Option<char>) {
    let t = v.trim();
    if t.len() >= 2 {
        let (a, b) = (t.chars().next().unwrap(), t.chars().last().unwrap());
        if (a == '"' || a == '\'') && a == b {
            let inner = &t[1..t.len() - 1];
            let s = if a == '"' { inner.replace("\\\"", "\"").replace("\\\\", "\\") } else { inner.replace("''", "'") };
            return (s, Some(a));
        }
    }
    (t.to_string(), None)
}

/// Убрать комментарий « # …» в конце строки (вне кавычек).
fn split_trailing_comment(v: &str) -> (&str, &str) {
    let mut q: Option<char> = None;
    let b = v.as_bytes();
    for (i, ch) in v.char_indices() {
        match q {
            Some(c) if ch == c => q = None,
            Some(_) => {}
            None if ch == '"' || ch == '\'' => q = Some(ch),
            None if ch == '#' && (i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t') => return (v[..i].trim_end(), &v[i..]),
            None => {}
        }
    }
    (v.trim_end(), "")
}

fn take_comment(buf: &mut Vec<String>) -> String {
    let s = buf.join("\n");
    buf.clear();
    s
}

fn parse_properties(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut com = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if t.starts_with('#') || t.starts_with('!') {
            com.push(t.trim_start_matches(['#', '!']).trim().to_string());
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            com.clear();
            continue;
        };
        let k = k.trim();
        out.push(Entry {
            path: k.into(),
            key: k.into(),
            value: v.trim().into(),
            kind: kind_of(v),
            editable: true,
            line: i,
            comment: take_comment(&mut com),
            hint: String::new(),
        });
    }
    out
}

fn parse_yaml(text: &str) -> Vec<Entry> {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = rx(&R, r#"^(\s*)("[^"]*"|'[^']*'|[^\s#'"\-][^:#]*?|-?\d[^:#]*?)\s*:(?:\s+(.*))?\s*$"#);
    let mut out = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut com: Vec<String> = Vec::new();
    let mut block_indent: Option<usize> = None;
    // раздел без значения, у которого ещё не было детей: (путь, ключ, строка, комментарий)
    let mut open_key: Option<(String, String, usize, String)> = None;
    for (i, line) in text.lines().enumerate() {
        let indent = line.len() - line.trim_start().len();
        let t = line.trim();
        if let Some(bi) = block_indent {
            if t.is_empty() || indent > bi {
                continue; // продолжение многострочного блока
            }
            block_indent = None;
        }
        if t.is_empty() {
            continue;
        }
        if t.starts_with('#') {
            com.push(t.trim_start_matches('#').trim().to_string());
            continue;
        }
        if t.starts_with("- ") || t == "-" || t == "---" {
            // элементы списка — показываем ключ списка, править его можно в режиме «Текст»
            if let Some((path, key, line_no, comment)) = open_key.take() {
                out.push(Entry { path, key, value: "[список]".into(), kind: Kind::Raw, editable: false, line: line_no, comment, hint: String::new() });
            }
            com.clear();
            continue;
        }
        let Some(c) = re.captures(line) else {
            com.clear();
            continue;
        };
        while stack.last().is_some_and(|(ind, _)| *ind >= indent) {
            stack.pop();
        }
        let key = unquote(&c[2]).0;
        let raw = c.get(3).map(|m| m.as_str()).unwrap_or("");
        let (val, _) = split_trailing_comment(raw);
        let path = stack.iter().map(|(_, k)| k.as_str()).chain([key.as_str()]).collect::<Vec<_>>().join(".");
        let comment = take_comment(&mut com);
        if val.is_empty() {
            // начало вложенного раздела (или списка)
            open_key = Some((path, key.clone(), i, comment));
            stack.push((indent, key));
            continue;
        }
        open_key = None;
        let (kind, editable, value) = if val.starts_with('|') || val.starts_with('>') {
            block_indent = Some(indent);
            (Kind::Raw, false, "[многострочный текст]".to_string())
        } else if val.starts_with('[') || val.starts_with('{') || val.starts_with('&') || val.starts_with('*') || val.starts_with('!') {
            (Kind::Raw, false, val.to_string())
        } else {
            let (s, q) = unquote(val);
            let k = if q.is_some() { Kind::Str } else { kind_of(&s) };
            (k, true, s)
        };
        out.push(Entry { path, key, value, kind, editable, line: i, comment, hint: String::new() });
    }
    out
}

fn parse_toml(text: &str) -> Vec<Entry> {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = rx(&R, r#"^\s*("[^"]+"|'[^']+'|[A-Za-z0-9_\-.]+)\s*=\s*(.*)$"#);
    static H: OnceLock<Regex> = OnceLock::new();
    let head = rx(&H, r#"^\s*\[\[?\s*([^\]]+?)\s*\]\]?\s*(#.*)?$"#);
    let mut out = Vec::new();
    let mut section = String::new();
    let mut com: Vec<String> = Vec::new();
    let mut in_array = false;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if in_array {
            if split_trailing_comment(t).0.ends_with(']') {
                in_array = false;
            }
            continue;
        }
        if t.is_empty() {
            continue;
        }
        if t.starts_with('#') {
            com.push(t.trim_start_matches('#').trim().to_string());
            continue;
        }
        if let Some(c) = head.captures(line) {
            section = c[1].split('.').map(|s| unquote(s).0).collect::<Vec<_>>().join(".");
            // комментарий над разделом относится к разделу — не к первому ключу
            com.clear();
            continue;
        }
        let Some(c) = re.captures(line) else {
            com.clear();
            continue;
        };
        let key = unquote(&c[1]).0;
        let (val, _) = split_trailing_comment(&c[2]);
        let path = if section.is_empty() { key.clone() } else { format!("{section}.{key}") };
        let comment = take_comment(&mut com);
        let (kind, editable, value) = if val.starts_with('[') {
            if !val.ends_with(']') {
                in_array = true;
            }
            (Kind::Raw, false, if in_array { "[список]".to_string() } else { val.to_string() })
        } else if val.starts_with('{') || val.starts_with("\"\"\"") || val.starts_with("'''") {
            (Kind::Raw, false, val.to_string())
        } else {
            let (s, q) = unquote(val);
            (if q.is_some() { Kind::Str } else { kind_of(&s) }, true, s)
        };
        out.push(Entry { path, key, value, kind, editable, line: i, comment, hint: String::new() });
    }
    out
}

pub fn parse(format: Format, text: &str) -> Vec<Entry> {
    match format {
        Format::Properties => parse_properties(text),
        Format::Yaml => parse_yaml(text),
        Format::Toml => parse_toml(text),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Loaded {
    pub rel: String,
    pub format: Format,
    pub text: String,
    pub entries: Vec<Entry>,
}

pub fn load(dir: &Path, rel: &str, lang_en: bool) -> Result<Loaded> {
    let (path, format) = resolve(dir, rel)?;
    let text = fs::read_to_string(&path)?;
    let mut entries = parse(format, &text);
    for e in entries.iter_mut() {
        e.hint = crate::cfghints::lookup(rel, &e.path, lang_en).unwrap_or_default();
    }
    Ok(Loaded { rel: rel.into(), format, text, entries })
}

// ===================================================================== запись

fn needs_quotes_yaml(s: &str) -> bool {
    s.is_empty()
        || s.trim() != s
        || s.contains(": ")
        || s.contains(" #")
        || s.ends_with(':')
        || s.starts_with(['#', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`', '[', ']', '{', '}', ',', '-', '?'])
        || matches!(s.to_ascii_lowercase().as_str(), "true" | "false" | "yes" | "no" | "on" | "off" | "null" | "~")
        || s.parse::<f64>().is_ok()
}

fn fmt_value(format: Format, kind: Kind, orig_raw: &str, new: &str) -> Result<String> {
    match kind {
        Kind::Bool => {
            if new != "true" && new != "false" {
                return Err(Error::other(format!("нужно true или false, а не «{new}»")));
            }
            Ok(new.into())
        }
        Kind::Int => new.trim().parse::<i64>().map(|n| n.to_string()).map_err(|_| Error::other(format!("нужно целое число, а не «{new}»"))),
        Kind::Float => new.trim().parse::<f64>().map(|_| new.trim().to_string()).map_err(|_| Error::other(format!("нужно число, а не «{new}»"))),
        Kind::Raw => Err(Error::other("это значение правится только в режиме «Текст»")),
        Kind::Str => {
            if new.contains(['\n', '\r']) {
                return Err(Error::other("перенос строки здесь нельзя — используй режим «Текст»"));
            }
            let (_, q) = unquote(orig_raw);
            Ok(match format {
                Format::Properties => new.to_string(),
                Format::Toml => match q {
                    Some('\'') if !new.contains('\'') => format!("'{new}'"),
                    _ => format!("\"{}\"", new.replace('\\', "\\\\").replace('"', "\\\"")),
                },
                Format::Yaml => match q {
                    Some('\'') => format!("'{}'", new.replace('\'', "''")),
                    Some(_) => format!("\"{}\"", new.replace('\\', "\\\\").replace('"', "\\\"")),
                    None if needs_quotes_yaml(new) => format!("\"{}\"", new.replace('\\', "\\\\").replace('"', "\\\"")),
                    None => new.to_string(),
                },
            })
        }
    }
}

/// Заменить значение в строке, сохранив ключ, отступ и комментарий в конце.
fn replace_in_line(format: Format, line: &str, kind: Kind, new: &str) -> Result<String> {
    let (prefix, rest) = match format {
        Format::Properties => {
            let i = line.find('=').ok_or_else(|| Error::other("строка изменилась"))?;
            (&line[..=i], &line[i + 1..])
        }
        Format::Toml => {
            let i = line.find('=').ok_or_else(|| Error::other("строка изменилась"))?;
            let ws = line[i + 1..].len() - line[i + 1..].trim_start().len();
            (&line[..i + 1 + ws], &line[i + 1 + ws..])
        }
        Format::Yaml => {
            static R: OnceLock<Regex> = OnceLock::new();
            let re = rx(&R, r#"^(\s*(?:"[^"]*"|'[^']*'|[^\s#'"][^:#]*?)\s*:\s+)(.*)$"#);
            let c = re.captures(line).ok_or_else(|| Error::other("строка изменилась"))?;
            let p = c.get(1).unwrap().end();
            (&line[..p], &line[p..])
        }
    };
    let (val, tail) = if format == Format::Properties { (rest, "") } else { split_trailing_comment(rest) };
    let v = fmt_value(format, kind, val, new)?;
    let sep = if tail.is_empty() { "" } else { " " };
    Ok(format!("{prefix}{v}{sep}{tail}"))
}

#[derive(Debug, Clone, Deserialize)]
pub struct Change {
    pub path: String,
    pub value: String,
}

/// Применить изменения к тексту. Ключи ищутся заново — если файл поменялся, правка не потеряется молча.
pub fn apply(format: Format, text: &str, changes: &[Change]) -> Result<String> {
    let entries = parse(format, text);
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    for ch in changes {
        let e = entries
            .iter()
            .find(|e| e.path == ch.path)
            .ok_or_else(|| Error::other(format!("ключ {} не найден — файл изменился, обнови список", ch.path)))?;
        if !e.editable {
            return Err(Error::other(format!("{}: правится только в режиме «Текст»", ch.path)));
        }
        lines[e.line] = replace_in_line(format, &lines[e.line], e.kind, &ch.value)?;
    }
    let mut out = lines.join(if text.contains("\r\n") { "\r\n" } else { "\n" });
    if text.ends_with('\n') {
        out.push_str(if text.contains("\r\n") { "\r\n" } else { "\n" });
    }
    Ok(out)
}

/// Проверка текста перед сохранением в режиме «Текст».
pub fn validate(format: Format, text: &str) -> Result<()> {
    match format {
        Format::Yaml => serde_yaml::from_str::<serde_yaml::Value>(text).map(|_| ()).map_err(|e| Error::other(format!("ошибка YAML: {e}"))),
        Format::Toml => toml::from_str::<toml::Value>(text).map(|_| ()).map_err(|e| Error::other(format!("ошибка TOML: {e}"))),
        Format::Properties => Ok(()),
    }
}

/// Записать: копия старой версии в `<файл>.pcm-bak`, потом атомарная замена.
pub fn save_text(dir: &Path, rel: &str, text: &str) -> Result<()> {
    let (path, format) = resolve(dir, rel)?;
    validate(format, text)?;
    let _ = fs::copy(&path, PathBuf::from(format!("{}.pcm-bak", path.display())));
    let tmp = PathBuf::from(format!("{}.pcm-tmp", path.display()));
    fs::write(&tmp, text)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn save_changes(dir: &Path, rel: &str, changes: &[Change]) -> Result<String> {
    let (path, format) = resolve(dir, rel)?;
    let text = fs::read_to_string(&path)?;
    let new = apply(format, &text, changes)?;
    save_text(dir, rel, &new)?;
    Ok(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPIGOT: &str = "# This is the main configuration file for Spigot.\n\
settings:\n  debug: false\n  timeout-time: 60 # секунд\n  restart-script: ./start.sh\nmessages:\n  whitelist: You are not whitelisted on this server!\n  unknown-command: \"Unknown command. Type \\\"/help\\\" for help.\"\nworld-settings:\n  default:\n    # how far entities stay active\n    entity-activation-range:\n      animals: 32\n      monsters: 32\n    item-despawn-rate: 6000\n    hunger:\n      jump-walk-exhaustion: 0.05\n    view-distance: default\ncommands:\n  spam-exclusions:\n  - /skill\n  replace-commands:\n  - setblock\n  log: true\nmotd: |\n  hello\n  world\nafter: 1\n";

    #[test]
    fn yaml_parse_paths_types() {
        let e = parse(Format::Yaml, SPIGOT);
        let get = |p: &str| e.iter().find(|x| x.path == p).unwrap_or_else(|| panic!("{p}"));
        assert_eq!(get("settings.debug").kind, Kind::Bool);
        assert_eq!(get("settings.timeout-time").value, "60");
        assert_eq!(get("world-settings.default.entity-activation-range.animals").value, "32");
        assert_eq!(get("world-settings.default.entity-activation-range.animals").comment, "");
        assert_eq!(get("world-settings.default.hunger.jump-walk-exhaustion").kind, Kind::Float);
        assert_eq!(get("messages.unknown-command").value, "Unknown command. Type \"/help\" for help.");
        assert!(!get("motd").editable);
        assert_eq!(get("after").value, "1");
        assert_eq!(get("commands.log").value, "true");
        assert!(e.iter().all(|x| !x.path.contains("/skill")));
        assert_eq!(get("commands.spam-exclusions").value, "[список]");
        assert!(!get("commands.spam-exclusions").editable);
        assert_eq!(get("commands.log").path, "commands.log", "ключ после списка того же уровня — в своём разделе");
    }

    #[test]
    fn yaml_apply_keeps_everything_else() {
        let ch = |p: &str, v: &str| Change { path: p.into(), value: v.into() };
        let out = apply(
            Format::Yaml,
            SPIGOT,
            &[ch("settings.timeout-time", "90"), ch("world-settings.default.view-distance", "8"), ch("messages.whitelist", "Ты не в белом списке: напиши админу")],
        )
        .unwrap();
        assert!(out.contains("  timeout-time: 90 # секунд\n"));
        assert!(out.contains("    view-distance: \"8\"\n"), "строка «default» → число в кавычках, тип не меняем");
        assert!(out.contains("  whitelist: \"Ты не в белом списке: напиши админу\"\n"));
        // всё остальное байт в байт
        let a: Vec<&str> = SPIGOT.lines().collect();
        let b: Vec<&str> = out.lines().collect();
        assert_eq!(a.len(), b.len());
        assert_eq!(a.iter().zip(&b).filter(|(x, y)| x != y).count(), 3);
        assert!(validate(Format::Yaml, &out).is_ok());
        assert!(apply(Format::Yaml, SPIGOT, &[ch("settings.debug", "maybe")]).is_err());
        assert!(apply(Format::Yaml, SPIGOT, &[ch("motd", "x")]).is_err());
        assert!(apply(Format::Yaml, SPIGOT, &[ch("nope.key", "1")]).is_err());
    }

    const NEO: &str = "#General settings\n[general]\n\t#Max players per chunk\n\t#Range: 1 ~ 64\n\tmaxPerChunk = 16\n\t#Allowed Values: LOW, HIGH\n\tmode = \"LOW\"\n\tlist = [\"a\",\n\t \"b\"]\n\tafter = true\n\n[client.\"x y\"]\n\tscale = 1.5\n";

    #[test]
    fn toml_parse_and_apply() {
        let e = parse(Format::Toml, NEO);
        let get = |p: &str| e.iter().find(|x| x.path == p).unwrap_or_else(|| panic!("{p}"));
        assert_eq!(get("general.maxPerChunk").comment, "Max players per chunk\nRange: 1 ~ 64");
        assert_eq!(get("general.mode").value, "LOW");
        assert!(!get("general.list").editable);
        assert_eq!(get("general.after").kind, Kind::Bool);
        assert_eq!(get("client.x y.scale").kind, Kind::Float);
        let out = apply(Format::Toml, NEO, &[Change { path: "general.mode".into(), value: "HIGH".into() }, Change { path: "general.maxPerChunk".into(), value: "32".into() }]).unwrap();
        assert!(out.contains("\tmode = \"HIGH\"\n") && out.contains("\tmaxPerChunk = 32\n"));
        assert!(validate(Format::Toml, &out).is_ok());
    }

    #[test]
    fn properties_and_files() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path();
        fs::write(d.join("server.properties"), "#Minecraft server properties\nmotd=A Minecraft Server\npvp=true\n").unwrap();
        fs::write(d.join("bukkit.yml"), "settings:\n  allow-end: true\n").unwrap();
        fs::create_dir_all(d.join("config")).unwrap();
        fs::write(d.join("config/paper-global.yml"), "misc:\n  max-joins-per-tick: 5\n").unwrap();
        fs::write(d.join("config/somemod-common.toml"), "a = 1\n").unwrap();
        fs::create_dir_all(d.join("plugins/Essentials/userdata")).unwrap();
        fs::write(d.join("plugins/Essentials/config.yml"), "x: 1\n").unwrap();
        fs::write(d.join("plugins/Essentials/userdata/u.yml"), "y: 1\n").unwrap();
        fs::write(d.join("whitelist.json"), "[]").unwrap();
        let files = discover(d);
        let rels: Vec<&str> = files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(rels[0], "server.properties");
        assert!(rels.contains(&"bukkit.yml") && rels.contains(&"config/paper-global.yml") && rels.contains(&"config/somemod-common.toml"));
        assert!(rels.contains(&"plugins/Essentials/config.yml"));
        assert!(!rels.contains(&"plugins/Essentials/userdata/u.yml"), "данные плагинов не показываем");
        assert!(!rels.iter().any(|r| r.ends_with(".json")));
        // безопасность путей
        assert!(resolve(d, "../x.yml").is_err());
        assert!(resolve(d, "/etc/x.yml").is_err());
        assert!(resolve(d, "eula.txt").is_err());
        // правка properties
        let out = save_changes(d, "server.properties", &[Change { path: "motd".into(), value: "Привет: мир".into() }]).unwrap();
        assert_eq!(out, "#Minecraft server properties\nmotd=Привет: мир\npvp=true\n");
        assert!(d.join("server.properties.pcm-bak").is_file());
        // неверный YAML в режиме «Текст» не сохраняется
        assert!(save_text(d, "bukkit.yml", "settings:\n  allow-end: [oops\n").is_err());
        assert_eq!(fs::read_to_string(d.join("bukkit.yml")).unwrap(), "settings:\n  allow-end: true\n");
    }
}
