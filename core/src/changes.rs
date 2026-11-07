//! Журнал изменений сервера: что и когда меняли через менеджер (моды, ядро, настройки,
//! откаты, карантин). Помогает ответить на вопрос «что изменилось с тех пор, как работало».
//! Хранится в `data/changes-<id сервера>.json`.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::Result;

const CAP: usize = 2000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub ts: i64,
    /// mods / plugins / core / settings / props / backup / quarantine / start
    pub kind: String,
    pub text: String,
}

fn file(data: &Path, id: &str) -> PathBuf {
    data.join(format!("changes-{id}.json"))
}

static LOCK: Mutex<()> = Mutex::new(());

pub fn load(data: &Path, id: &str) -> VecDeque<Change> {
    fs::read_to_string(file(data, id)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn record(data: &Path, id: &str, kind: &str, text: impl Into<String>) -> Result<()> {
    let _g = LOCK.lock().unwrap();
    let mut v = load(data, id);
    v.push_back(Change { ts: chrono::Utc::now().timestamp(), kind: kind.into(), text: text.into() });
    while v.len() > CAP {
        v.pop_front();
    }
    fs::create_dir_all(data)?;
    let p = file(data, id);
    let tmp = p.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string(&v)?)?;
    fs::rename(&tmp, &p)?;
    Ok(())
}

/// Новые первыми.
pub fn recent(data: &Path, id: &str, n: usize) -> Vec<Change> {
    load(data, id).into_iter().rev().take(n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal() {
        let t = tempfile::tempdir().unwrap();
        record(t.path(), "s", "mods", "Выключен: a.jar").unwrap();
        record(t.path(), "s", "core", "Ядро обновлено").unwrap();
        let r = recent(t.path(), "s", 10);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].kind, "core");
    }
}
