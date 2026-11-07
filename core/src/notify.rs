//! Уведомления в Telegram и Discord и простой Telegram-бот для управления с телефона.
//!
//! - Telegram: бот от @BotFather (токен) + id чата. Бот отвечает только этому чату.
//! - Discord: webhook канала (Настройки канала → Интеграции → Вебхуки).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifySettings {
    pub telegram_token: String,
    pub telegram_chat: String,
    /// Принимать команды от этого чата (/status, /restart …)
    pub telegram_bot: bool,
    pub discord_webhook: String,
    pub on_crash: bool,
    pub on_start: bool,
    pub on_stop: bool,
    pub on_low_tps: bool,
    pub low_tps: f32,
    pub on_backup_fail: bool,
    pub on_disk_low: bool,
    /// ГБ свободного места
    pub disk_low_gb: u32,
    pub on_join: bool,
    pub on_net_bad: bool,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self {
            telegram_token: String::new(),
            telegram_chat: String::new(),
            telegram_bot: false,
            discord_webhook: String::new(),
            on_crash: true,
            on_start: false,
            on_stop: false,
            on_low_tps: true,
            low_tps: 15.0,
            on_backup_fail: true,
            on_disk_low: true,
            disk_low_gb: 5,
            on_join: false,
            on_net_bad: false,
        }
    }
}

impl NotifySettings {
    pub fn telegram_ready(&self) -> bool {
        !self.telegram_token.trim().is_empty() && !self.telegram_chat.trim().is_empty()
    }
    pub fn discord_ready(&self) -> bool {
        self.discord_webhook.trim().starts_with("https://")
    }
    pub fn any(&self) -> bool {
        self.telegram_ready() || self.discord_ready()
    }
}

fn tg_url(token: &str, method: &str) -> String {
    format!("https://api.telegram.org/bot{}/{method}", token.trim())
}

/// Ответ Telegram без токена в тексте ошибки (чтобы он не попал в логи и интерфейс).
fn tg_call(token: &str, method: &str, body: &Value) -> Result<Value> {
    let resp = crate::util::http()
        .post(&tg_url(token, method))
        .set("User-Agent", crate::net::UA)
        .send_json(body.clone());
    match resp {
        Ok(r) => r.into_json().map_err(|e| Error::Network(e.to_string())),
        Err(ureq::Error::Status(code, r)) => {
            let desc = r
                .into_json::<Value>()
                .ok()
                .and_then(|v| v.get("description").and_then(Value::as_str).map(String::from))
                .unwrap_or_default();
            Err(Error::Network(format!("Telegram: HTTP {code} {desc}")))
        }
        Err(e) => Err(Error::Network(format!("Telegram: {}", e.kind()))),
    }
}

/// id чата: цифры (у групп с минусом) или @имя_канала. Убираем пробелы и случайный «@» перед цифрами.
pub fn normalize_chat(chat: &str) -> String {
    let c: String = chat.chars().filter(|ch| !ch.is_whitespace()).collect();
    match c.strip_prefix('@') {
        Some(rest) if rest.trim_start_matches('-').chars().all(|x| x.is_ascii_digit()) && !rest.is_empty() => rest.to_string(),
        _ => c,
    }
}

pub fn telegram_send(token: &str, chat: &str, text: &str) -> Result<()> {
    match tg_call(token, "sendMessage", &json!({ "chat_id": normalize_chat(chat), "text": text, "disable_web_page_preview": true })) {
        Err(Error::Network(m)) if m.contains("chat not found") => Err(Error::Network(
            "Telegram не нашёл чат. Так бывает, если ты ещё не написал боту /start (бот не может писать первым), \
             или указан не твой id (а, например, id бота из токена). Нажми «Найти мой чат» в настройках."
                .into(),
        )),
        Err(e) => Err(e),
        Ok(_) => Ok(()),
    }
}

/// Найти чат по последним сообщениям боту: (id, название). Сначала напиши боту что угодно.
pub fn telegram_find_chat(token: &str) -> Result<Option<(String, String)>> {
    let v = tg_call(token, "getUpdates", &json!({ "timeout": 0, "allowed_updates": ["message", "channel_post", "my_chat_member"] }))?;
    let mut found = None;
    for u in v["result"].as_array().into_iter().flatten() {
        let chat = ["message", "channel_post", "my_chat_member"].iter().find_map(|k| u.get(*k).map(|m| &m["chat"]));
        if let Some(c) = chat {
            if let Value::Number(n) = &c["id"] {
                let name = c["title"].as_str().or(c["username"].as_str().map(|_| "")).unwrap_or("");
                let name = if name.is_empty() {
                    [c["first_name"].as_str(), c["last_name"].as_str()].into_iter().flatten().collect::<Vec<_>>().join(" ")
                } else {
                    name.to_string()
                };
                found = Some((n.to_string(), name));
            }
        }
    }
    Ok(found)
}

pub fn discord_send(webhook: &str, text: &str) -> Result<()> {
    crate::util::http()
        .post(webhook.trim())
        .set("User-Agent", crate::net::UA)
        .send_json(json!({ "content": text, "username": "ProtoCore", "allowed_mentions": { "parse": [] } }))
        .map_err(|e| Error::Network(format!("Discord: {}", match e {
            ureq::Error::Status(c, _) => format!("HTTP {c}"),
            other => other.kind().to_string(),
        })))?;
    Ok(())
}

/// Отправить во все настроенные каналы. Ошибки возвращаются списком.
pub fn send(s: &NotifySettings, text: &str) -> Vec<String> {
    let mut errs = Vec::new();
    if s.telegram_ready() {
        if let Err(e) = telegram_send(&s.telegram_token, &s.telegram_chat, text) {
            errs.push(e.to_string());
        }
    }
    if s.discord_ready() {
        if let Err(e) = discord_send(&s.discord_webhook, text) {
            errs.push(e.to_string());
        }
    }
    errs
}

// ===================================================================== бот

#[derive(Debug, Clone, PartialEq)]
pub enum BotCmd {
    Help,
    Status,
    Players(Option<String>),
    Start(Option<String>),
    Stop(Option<String>),
    Restart(Option<String>),
    Backup(Option<String>),
    Say(Option<String>, String),
    Unknown(String),
}

/// Разбор команды бота. Сервер можно указать после команды: `/restart smp`.
pub fn parse_cmd(text: &str) -> Option<BotCmd> {
    let t = text.trim();
    if !t.starts_with('/') {
        return None;
    }
    let mut parts = t.splitn(2, char::is_whitespace);
    // «/status@MyBot» → «status»
    let cmd = parts.next()?.trim_start_matches('/').split('@').next()?.to_lowercase();
    let rest = parts.next().map(str::trim).filter(|x| !x.is_empty()).map(String::from);
    Some(match cmd.as_str() {
        "start" if rest.is_none() => BotCmd::Help, // «/start» — первое сообщение боту
        "help" => BotCmd::Help,
        "status" | "s" => BotCmd::Status,
        "players" | "online" => BotCmd::Players(rest),
        "run" | "start" | "up" => BotCmd::Start(rest),
        "stop" => BotCmd::Stop(rest),
        "restart" | "rs" => BotCmd::Restart(rest),
        "backup" => BotCmd::Backup(rest),
        "say" => {
            let r = rest.unwrap_or_default();
            // «/say smp текст» или «/say текст»
            BotCmd::Say(None, r)
        }
        other => BotCmd::Unknown(other.to_string()),
    })
}

pub const BOT_HELP: &str = "ProtoCore-MCS-Manager\n\
/status — список серверов с номерами\n\
/players <сервер> — кто онлайн\n\
/run <сервер> — запустить\n\
/stop <сервер> — остановить\n\
/restart <сервер> — перезапустить\n\
/backup <сервер> — снапшот\n\
/say <сервер> <текст> — сообщение в чат сервера\n\
Сервер — номер из /status или название: /restart 1, /stop lobby.";

/// Получить новые сообщения (long polling). Возвращает (новый offset, [(chat_id, текст)]).
pub fn telegram_updates(token: &str, offset: i64, timeout_s: u32) -> Result<(i64, Vec<(String, String)>)> {
    let v = tg_call(token, "getUpdates", &json!({ "offset": offset, "timeout": timeout_s, "allowed_updates": ["message"] }))?;
    let mut next = offset;
    let mut out = Vec::new();
    for u in v["result"].as_array().into_iter().flatten() {
        if let Some(id) = u["update_id"].as_i64() {
            next = next.max(id + 1);
        }
        let chat = match &u["message"]["chat"]["id"] {
            Value::Number(n) => n.to_string(),
            _ => continue,
        };
        if let Some(t) = u["message"]["text"].as_str() {
            out.push((chat, t.to_string()));
        }
    }
    Ok((next, out))
}

/// Проверка токена: имя бота.
pub fn telegram_me(token: &str) -> Result<String> {
    let v = tg_call(token, "getMe", &json!({}))?;
    Ok(v["result"]["username"].as_str().unwrap_or("").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_commands() {
        assert_eq!(parse_cmd("/status"), Some(BotCmd::Status));
        assert_eq!(parse_cmd("/status@ProtoBot"), Some(BotCmd::Status));
        assert_eq!(parse_cmd("/restart smp"), Some(BotCmd::Restart(Some("smp".into()))));
        assert_eq!(parse_cmd("/start"), Some(BotCmd::Help));
        assert_eq!(parse_cmd("/start lobby"), Some(BotCmd::Start(Some("lobby".into()))));
        assert_eq!(parse_cmd("/say привет всем"), Some(BotCmd::Say(None, "привет всем".into())));
        assert_eq!(parse_cmd("привет"), None);
        assert_eq!(normalize_chat(" 123 456 "), "123456");
        assert_eq!(normalize_chat("@-100123"), "-100123");
        assert_eq!(normalize_chat("@mychannel"), "@mychannel");
    }

    #[test]
    fn readiness() {
        let mut s = NotifySettings::default();
        assert!(!s.any());
        s.discord_webhook = "https://discord.com/api/webhooks/1/x".into();
        assert!(s.discord_ready() && s.any());
    }
}
