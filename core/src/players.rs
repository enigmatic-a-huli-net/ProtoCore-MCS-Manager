//! Игроки: история заходов, IP (поиск твинков), чат, журнал команд,
//! а также списки сервера (операторы, белый список, баны) из его JSON-файлов.
//!
//! История собирается менеджером из консоли и хранится в
//! `data/players-<id сервера>.json` рядом с менеджером — она переживает перезапуски.

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Result;

const CHAT_CAP: usize = 1000;
const AUDIT_CAP: usize = 500;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Join { name: String },
    Login { name: String, ip: String },
    Leave { name: String },
    Chat { name: String, msg: String },
}

fn re(p: &'static str) -> Regex {
    Regex::new(p).unwrap()
}

macro_rules! rx {
    ($n:ident, $p:expr) => {
        fn $n() -> &'static Regex {
            static R: OnceLock<Regex> = OnceLock::new();
            R.get_or_init(|| re($p))
        }
    };
}

rx!(r_login, r"\]: (\.?[A-Za-z0-9_]{2,16})\[/([0-9a-fA-F.:]+?):\d+\] logged in with entity id");
rx!(r_join, r"\]: (\.?[A-Za-z0-9_]{2,16}) joined the game");
rx!(r_leave, r"\]: (\.?[A-Za-z0-9_]{2,16}) (?:left the game|lost connection)");
rx!(r_chat, r"\]: (?:\[Not Secure\] )?<(\.?[A-Za-z0-9_]{2,16})> (.+)$");

pub fn parse(line: &str) -> Option<Event> {
    if let Some(c) = r_login().captures(line) {
        return Some(Event::Login { name: c[1].into(), ip: c[2].into() });
    }
    if let Some(c) = r_join().captures(line) {
        return Some(Event::Join { name: c[1].into() });
    }
    if let Some(c) = r_leave().captures(line) {
        return Some(Event::Leave { name: c[1].into() });
    }
    if let Some(c) = r_chat().captures(line) {
        return Some(Event::Chat { name: c[1].into(), msg: c[2].into() });
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlayerRec {
    pub first_seen: i64,
    pub last_seen: i64,
    /// секунд на сервере всего
    pub total_secs: i64,
    pub sessions: u32,
    pub ips: Vec<String>,
    /// когда зашёл в текущую сессию (0 — не в игре)
    #[serde(default)]
    pub online_since: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatLine {
    pub ts: i64,
    pub name: String,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLine {
    pub ts: i64,
    pub cmd: String,
    /// откуда: console / bot / schedule
    pub by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlayerDb {
    pub players: BTreeMap<String, PlayerRec>,
    pub chat: VecDeque<ChatLine>,
    pub audit: VecDeque<AuditLine>,
    /// Если задано — IP хранятся только хэшем (для поиска твинков этого достаточно)
    #[serde(skip)]
    pub ip_salt: Option<String>,
}

fn is_local_ip(ip: &str) -> bool {
    ip.starts_with("127.") || ip.starts_with("192.168.") || ip.starts_with("10.") || ip == "::1"
}

/// Хэш IP с солью этой установки: одинаковые IP дают одинаковый хэш, но сам адрес не восстановить.
pub fn hash_ip(salt: &str, ip: &str) -> String {
    use sha2::{Digest, Sha256};
    if ip.starts_with('#') || is_local_ip(ip) {
        return ip.to_string();
    }
    let h = Sha256::digest(format!("{salt}|{ip}").as_bytes());
    format!("#{}", h.iter().take(5).map(|b| format!("{b:02x}")).collect::<String>())
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl PlayerDb {
    pub fn file(data_dir: &Path, server_id: &str) -> PathBuf {
        data_dir.join(format!("players-{server_id}.json"))
    }

    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string(self)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Применить событие. Возвращает true, если что-то изменилось.
    pub fn apply(&mut self, ev: &Event) -> bool {
        self.apply_at(ev, now())
    }

    pub fn apply_at(&mut self, ev: &Event, t: i64) -> bool {
        match ev {
            Event::Login { name, ip } => {
                let ip = &match &self.ip_salt {
                    Some(s) => hash_ip(s, ip),
                    None => ip.clone(),
                };
                let r = self.players.entry(name.clone()).or_default();
                if !r.ips.contains(ip) {
                    r.ips.push(ip.clone());
                    if r.ips.len() > 20 {
                        r.ips.remove(0);
                    }
                }
                true
            }
            Event::Join { name } => {
                let r = self.players.entry(name.clone()).or_default();
                if r.first_seen == 0 {
                    r.first_seen = t;
                }
                r.last_seen = t;
                r.sessions += 1;
                r.online_since = t;
                true
            }
            Event::Leave { name } => {
                if let Some(r) = self.players.get_mut(name) {
                    if r.online_since > 0 {
                        r.total_secs += (t - r.online_since).max(0);
                        r.online_since = 0;
                    }
                    r.last_seen = t;
                    true
                } else {
                    false
                }
            }
            Event::Chat { name, msg } => {
                self.chat.push_back(ChatLine { ts: t, name: name.clone(), msg: msg.clone() });
                while self.chat.len() > CHAT_CAP {
                    self.chat.pop_front();
                }
                true
            }
        }
    }

    /// Включили хранение хэшей — переводим уже сохранённые адреса.
    pub fn hash_existing(&mut self) -> bool {
        let Some(salt) = self.ip_salt.clone() else { return false };
        let mut changed = false;
        for r in self.players.values_mut() {
            for ip in r.ips.iter_mut() {
                let h = hash_ip(&salt, ip);
                if h != *ip {
                    *ip = h;
                    changed = true;
                }
            }
            r.ips.dedup();
        }
        changed
    }

    /// Сервер остановился — все сессии закрываются.
    pub fn close_all(&mut self) {
        let t = now();
        for r in self.players.values_mut() {
            if r.online_since > 0 {
                r.total_secs += (t - r.online_since).max(0);
                r.online_since = 0;
                r.last_seen = t;
            }
        }
    }

    pub fn audit(&mut self, cmd: &str, by: &str) {
        self.audit.push_back(AuditLine { ts: now(), cmd: cmd.to_string(), by: by.to_string() });
        while self.audit.len() > AUDIT_CAP {
            self.audit.pop_front();
        }
    }

    /// Возможные твинки: игроки, заходившие с одного IP.
    pub fn shared_ips(&self) -> Vec<(String, Vec<String>)> {
        let mut by_ip: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for (n, r) in &self.players {
            for ip in &r.ips {
                // локальные адреса (сам хост, домашняя сеть) не показательны
                if is_local_ip(ip) {
                    continue;
                }
                by_ip.entry(ip).or_default().push(n.clone());
            }
        }
        by_ip.into_iter().filter(|(_, v)| v.len() > 1).map(|(k, v)| (k.to_string(), v)).collect()
    }
}

// ===================================================================== списки сервера

#[derive(Debug, Clone, Serialize, Default)]
pub struct ServerLists {
    pub ops: Vec<String>,
    pub whitelist: Vec<String>,
    pub banned: Vec<(String, String)>,
    pub banned_ips: Vec<(String, String)>,
    pub whitelist_on: bool,
    /// имя → uuid из usercache.json
    pub known: BTreeMap<String, String>,
}

fn names(path: &Path, field: &str) -> Vec<String> {
    let Ok(t) = fs::read_to_string(path) else { return vec![] };
    let Ok(v) = serde_json::from_str::<Value>(&t) else { return vec![] };
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.get(field).and_then(Value::as_str).map(String::from))
        .collect()
}

fn with_reason(path: &Path, field: &str) -> Vec<(String, String)> {
    let Ok(t) = fs::read_to_string(path) else { return vec![] };
    let Ok(v) = serde_json::from_str::<Value>(&t) else { return vec![] };
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| {
            Some((
                x.get(field)?.as_str()?.to_string(),
                x.get("reason").and_then(Value::as_str).unwrap_or("").to_string(),
            ))
        })
        .collect()
}

pub fn lists(server_dir: &Path) -> ServerLists {
    let props = crate::props::read(server_dir);
    let wl = props.iter().any(|(k, v)| (k == "white-list" || k == "enforce-whitelist") && v == "true");
    let mut known = BTreeMap::new();
    if let Ok(t) = fs::read_to_string(server_dir.join("usercache.json")) {
        if let Ok(Value::Array(a)) = serde_json::from_str::<Value>(&t) {
            for x in a {
                if let (Some(n), Some(u)) = (x.get("name").and_then(Value::as_str), x.get("uuid").and_then(Value::as_str)) {
                    known.insert(n.to_string(), u.to_string());
                }
            }
        }
    }
    ServerLists {
        ops: names(&server_dir.join("ops.json"), "name"),
        whitelist: names(&server_dir.join("whitelist.json"), "name"),
        banned: with_reason(&server_dir.join("banned-players.json"), "name"),
        banned_ips: with_reason(&server_dir.join("banned-ips.json"), "ip"),
        whitelist_on: wl,
        known,
    }
}

/// Безопасное имя игрока для подстановки в команду.
pub fn valid_name(n: &str) -> bool {
    let n = n.strip_prefix('.').unwrap_or(n);
    (2..=16).contains(&n.len()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lines() {
        assert_eq!(
            parse("[12:00:00] [Server thread/INFO] [minecraft/PlayerList]: Steve[/93.1.2.3:51234] logged in with entity id 123 at (0.5, 64.0, 0.5)"),
            Some(Event::Login { name: "Steve".into(), ip: "93.1.2.3".into() })
        );
        assert_eq!(parse("[x] [Server thread/INFO]: Steve joined the game"), Some(Event::Join { name: "Steve".into() }));
        assert_eq!(parse("[x] [Server thread/INFO]: Steve left the game"), Some(Event::Leave { name: "Steve".into() }));
        assert_eq!(
            parse("[x] [Server thread/INFO]: [Not Secure] <Alex> привет всем"),
            Some(Event::Chat { name: "Alex".into(), msg: "привет всем".into() })
        );
        assert_eq!(parse("[x] [Server thread/INFO]: Done (3.2s)!"), None);
    }

    #[test]
    fn sessions_and_alts() {
        let mut db = PlayerDb::default();
        db.apply_at(&Event::Login { name: "A".into(), ip: "93.1.1.1".into() }, 100);
        db.apply_at(&Event::Join { name: "A".into() }, 100);
        db.apply_at(&Event::Leave { name: "A".into() }, 400);
        db.apply_at(&Event::Login { name: "B".into(), ip: "93.1.1.1".into() }, 500);
        db.apply_at(&Event::Join { name: "B".into() }, 500);
        let a = &db.players["A"];
        assert_eq!(a.total_secs, 300);
        assert_eq!(a.sessions, 1);
        assert_eq!(db.shared_ips(), vec![("93.1.1.1".to_string(), vec!["A".to_string(), "B".to_string()])]);
        db.close_all();
        assert_eq!(db.players["B"].online_since, 0);
        let t = tempfile::tempdir().unwrap();
        let p = PlayerDb::file(t.path(), "s1");
        db.save(&p).unwrap();
        assert_eq!(PlayerDb::load(&p).players.len(), 2);
    }

    #[test]
    fn names_validation() {
        assert!(valid_name("Steve_01"));
        assert!(valid_name(".BedrockGuy"));
        assert!(!valid_name("a; stop"));
        assert!(!valid_name("x"));
    }

    #[test]
    fn read_lists() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("ops.json"), r#"[{"uuid":"u1","name":"Admin","level":4}]"#).unwrap();
        fs::write(t.path().join("banned-players.json"), r#"[{"uuid":"u2","name":"Griefer","reason":"гриф"}]"#).unwrap();
        fs::write(t.path().join("server.properties"), "white-list=true\n").unwrap();
        let l = lists(t.path());
        assert_eq!(l.ops, vec!["Admin"]);
        assert_eq!(l.banned, vec![("Griefer".to_string(), "гриф".to_string())]);
        assert!(l.whitelist_on);
    }

    #[test]
    fn hashed_ips_still_find_alts() {
        let mut db = PlayerDb { ip_salt: Some("salt".into()), ..Default::default() };
        db.apply_at(&Event::Login { name: "A".into(), ip: "93.1.1.1".into() }, 1);
        db.apply_at(&Event::Login { name: "B".into(), ip: "93.1.1.1".into() }, 2);
        db.apply_at(&Event::Login { name: "C".into(), ip: "192.168.0.5".into() }, 3);
        let a = &db.players["A"].ips[0];
        assert!(a.starts_with('#') && !a.contains("93.1"));
        assert_eq!(db.shared_ips().len(), 1);
        assert_eq!(db.players["C"].ips[0], "192.168.0.5");
        // старые открытые адреса переводятся
        let mut old = PlayerDb::default();
        old.apply_at(&Event::Login { name: "X".into(), ip: "8.8.8.8".into() }, 1);
        old.ip_salt = Some("salt".into());
        assert!(old.hash_existing());
        assert!(old.players["X"].ips[0].starts_with('#'));
        // хэш не сохраняется в файл как настройка
        assert!(!serde_json::to_string(&db).unwrap().contains("ip_salt"));
    }
}
